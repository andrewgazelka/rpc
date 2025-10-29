//! Test manual streaming with StreamChunk responses using InProcessTransport
//! This demonstrates the low-level protocol without macro-generated streaming

use rpc_codec_json::JsonCodec;
use rpc_core::{Codec, Message, RpcRequest, RpcResponse, ResponseResult, Transport};
use rpc_transport_inprocess::InProcessTransport;

#[tokio::test]
async fn test_manual_stream_chunks_with_inprocess() {
    let (mut client_transport, mut server_transport) = InProcessTransport::pair();

    let codec = JsonCodec;

    // Spawn server that sends multiple StreamChunk responses
    let server_handle = tokio::spawn(async move {
        // Receive request
        let msg = server_transport.recv().await.unwrap();
        let request: RpcRequest = codec.decode(&msg.data).unwrap();
        assert_eq!(request.method, "stream_data");

        let request_id = request.id;

        // Send 5 stream chunks
        for i in 0..5u8 {
            let response = RpcResponse {
                id: request_id,
                result: ResponseResult::StreamChunk(vec![i; 10]),
            };
            let data = codec.encode(&response).unwrap();
            server_transport.send(Message::new(data)).await.unwrap();
        }

        // Send StreamEnd
        let end_response = RpcResponse {
            id: request_id,
            result: ResponseResult::StreamEnd,
        };
        let data = codec.encode(&end_response).unwrap();
        server_transport.send(Message::new(data)).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Client sends request
    let request = RpcRequest {
        id: 1,
        method: "stream_data".to_string(),
        params: vec![],
    };
    let data = codec.encode(&request).unwrap();
    client_transport.send(Message::new(data)).await.unwrap();

    // Client receives stream chunks
    let mut chunks_received = 0;
    loop {
        let msg = client_transport.recv().await.unwrap();
        let response: RpcResponse = codec.decode(&msg.data).unwrap();
        assert_eq!(response.id, 1);

        match response.result {
            ResponseResult::StreamChunk(data) => {
                assert_eq!(data.len(), 10);
                assert_eq!(data[0], chunks_received);
                chunks_received += 1;
            }
            ResponseResult::StreamEnd => {
                break;
            }
            _ => panic!("Unexpected response type: {:?}", response.result),
        }
    }

    assert_eq!(chunks_received, 5);
    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_stream_with_error() {
    let (mut client_transport, mut server_transport) = InProcessTransport::pair();

    let codec = JsonCodec;

    let server_handle = tokio::spawn(async move {
        let msg = server_transport.recv().await.unwrap();
        let request: RpcRequest = codec.decode(&msg.data).unwrap();
        let request_id = request.id;

        // Send 3 chunks then error
        for i in 0..3u8 {
            let response = RpcResponse {
                id: request_id,
                result: ResponseResult::StreamChunk(vec![i]),
            };
            let data = codec.encode(&response).unwrap();
            server_transport.send(Message::new(data)).await.unwrap();
        }

        // Send error
        let error_response = RpcResponse {
            id: request_id,
            result: ResponseResult::Err("stream error".to_string()),
        };
        let data = codec.encode(&error_response).unwrap();
        server_transport.send(Message::new(data)).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let request = RpcRequest {
        id: 1,
        method: "stream_data".to_string(),
        params: vec![],
    };
    let data = codec.encode(&request).unwrap();
    client_transport.send(Message::new(data)).await.unwrap();

    let mut chunks_received = 0;
    let mut error_received = false;

    for _ in 0..4 {
        let msg = client_transport.recv().await.unwrap();
        let response: RpcResponse = codec.decode(&msg.data).unwrap();

        match response.result {
            ResponseResult::StreamChunk(_) => {
                chunks_received += 1;
            }
            ResponseResult::Err(e) => {
                assert_eq!(e, "stream error");
                error_received = true;
                break;
            }
            _ => panic!("Unexpected response type"),
        }
    }

    assert_eq!(chunks_received, 3);
    assert!(error_received);
    server_handle.await.unwrap();
}

#[tokio::test]
async fn test_multiple_concurrent_streams() {
    let (mut client_transport, mut server_transport) = InProcessTransport::pair();

    let codec = JsonCodec;

    let server_handle = tokio::spawn(async move {
        use std::collections::HashMap;

        let mut active_streams = HashMap::new();

        // Receive 2 requests
        for _ in 0..2 {
            let msg = server_transport.recv().await.unwrap();
            let request: RpcRequest = codec.decode(&msg.data).unwrap();
            active_streams.insert(request.id, 0u8);
        }

        // Interleave responses for both streams
        while !active_streams.is_empty() {
            let mut completed = Vec::new();

            for (&request_id, count) in &mut active_streams {
                if *count < 3 {
                    let response = RpcResponse {
                        id: request_id,
                        result: ResponseResult::StreamChunk(vec![*count]),
                    };
                    let data = codec.encode(&response).unwrap();
                    server_transport.send(Message::new(data)).await.unwrap();
                    *count += 1;
                } else {
                    let response = RpcResponse {
                        id: request_id,
                        result: ResponseResult::StreamEnd,
                    };
                    let data = codec.encode(&response).unwrap();
                    server_transport.send(Message::new(data)).await.unwrap();
                    completed.push(request_id);
                }
            }

            for id in completed {
                active_streams.remove(&id);
            }
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Send 2 requests
    for request_id in 1..=2 {
        let request = RpcRequest {
            id: request_id,
            method: "stream_data".to_string(),
            params: vec![],
        };
        let data = codec.encode(&request).unwrap();
        client_transport.send(Message::new(data)).await.unwrap();
    }

    use std::collections::HashMap;
    let mut stream_states = HashMap::new();
    stream_states.insert(1, 0);
    stream_states.insert(2, 0);

    // Receive interleaved responses
    while !stream_states.is_empty() {
        let msg = client_transport.recv().await.unwrap();
        let response: RpcResponse = codec.decode(&msg.data).unwrap();

        match response.result {
            ResponseResult::StreamChunk(_) => {
                *stream_states.get_mut(&response.id).unwrap() += 1;
            }
            ResponseResult::StreamEnd => {
                assert_eq!(*stream_states.get(&response.id).unwrap(), 3);
                stream_states.remove(&response.id);
            }
            _ => panic!("Unexpected response type"),
        }
    }

    server_handle.await.unwrap();
}
