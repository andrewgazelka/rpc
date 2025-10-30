//! WebSocket transport for the RPC framework.
//!
//! Provides WebSocket-based transport implementation using tokio-tungstenite.

use futures::{SinkExt, StreamExt};
use rpc_core::{Message, Transport};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, accept_async, connect_async,
    tungstenite::protocol::Message as WsMessage,
};

/// WebSocket transport error type
#[derive(Debug, thiserror::Error)]
pub enum WebSocketError {
    /// WebSocket protocol error
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    /// Connection closed
    #[error("connection closed")]
    ConnectionClosed,

    /// Unexpected message type received
    #[error("unexpected message type")]
    UnexpectedMessageType,

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// WebSocket transport implementation.
///
/// Can be used for both client and server connections.
pub struct WebSocketTransport {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

/// WebSocket transport for server-side connections (no TLS wrapping needed)
pub struct WebSocketServerTransport {
    ws: WebSocketStream<TcpStream>,
}

impl WebSocketTransport {
    /// Connect to a WebSocket server.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use rpc_transport_ws::WebSocketTransport;
    ///
    /// let transport = WebSocketTransport::connect("ws://127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(url: &str) -> Result<Self, WebSocketError> {
        let (ws, _) = connect_async(url).await?;
        Ok(Self { ws })
    }
}

impl Transport for WebSocketTransport {
    type Error = WebSocketError;

    async fn send(&mut self, msg: Message) -> Result<(), Self::Error> {
        self.ws.send(WsMessage::Binary(msg.data)).await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<Message, Self::Error> {
        match self.ws.next().await {
            Some(Ok(WsMessage::Binary(data))) => Ok(Message::new(data)),
            Some(Ok(WsMessage::Close(_))) => Err(WebSocketError::ConnectionClosed),
            Some(Ok(_)) => Err(WebSocketError::UnexpectedMessageType),
            Some(Err(e)) => Err(WebSocketError::WebSocket(e)),
            None => Err(WebSocketError::ConnectionClosed),
        }
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.ws.close(None).await?;
        Ok(())
    }
}

/// WebSocket server listener.
///
/// Accepts incoming WebSocket connections and creates transports.
pub struct WebSocketListener {
    pub listener: TcpListener,
}

impl WebSocketListener {
    /// Bind to an address and create a WebSocket listener.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use rpc_transport_ws::WebSocketListener;
    ///
    /// let listener = WebSocketListener::bind("127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn bind(addr: &str) -> Result<Self, WebSocketError> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self { listener })
    }

    /// Accept an incoming connection and create a WebSocket transport.
    pub async fn accept(&self) -> Result<WebSocketServerTransport, WebSocketError> {
        let (stream, _) = self.listener.accept().await?;
        WebSocketServerTransport::from_stream(stream).await
    }
}

impl WebSocketServerTransport {
    /// Create a WebSocket transport from an accepted connection.
    ///
    /// Used on the server side after accepting a TCP connection.
    pub async fn from_stream(stream: TcpStream) -> Result<Self, WebSocketError> {
        let ws = accept_async(stream).await?;
        Ok(Self { ws })
    }
}

impl Transport for WebSocketServerTransport {
    type Error = WebSocketError;

    async fn send(&mut self, msg: Message) -> Result<(), Self::Error> {
        self.ws.send(WsMessage::Binary(msg.data)).await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<Message, Self::Error> {
        match self.ws.next().await {
            Some(Ok(WsMessage::Binary(data))) => Ok(Message::new(data)),
            Some(Ok(WsMessage::Close(_))) => Err(WebSocketError::ConnectionClosed),
            Some(Ok(_)) => Err(WebSocketError::UnexpectedMessageType),
            Some(Err(e)) => Err(WebSocketError::WebSocket(e)),
            None => Err(WebSocketError::ConnectionClosed),
        }
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.ws.close(None).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rpc_core::Transport;

    #[tokio::test]
    async fn test_websocket_connection() {
        // Start a server
        let listener = WebSocketListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.listener.local_addr().unwrap();

        // Spawn server task
        let server_task = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();
            let msg = transport.recv().await.unwrap();
            transport.send(msg).await.unwrap();
        });

        // Connect client
        let url = format!("ws://{}", addr);
        let mut client = WebSocketTransport::connect(&url).await.unwrap();

        // Send message
        let test_msg = Message::new(vec![1, 2, 3, 4]);
        client.send(test_msg.clone()).await.unwrap();

        // Receive echo
        let received = client.recv().await.unwrap();
        assert_eq!(test_msg.data, received.data);

        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let listener = WebSocketListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();
            for _ in 0..5 {
                let msg = transport.recv().await.unwrap();
                transport.send(msg).await.unwrap();
            }
        });

        let url = format!("ws://{}", addr);
        let mut client = WebSocketTransport::connect(&url).await.unwrap();

        for i in 0..5 {
            let msg = Message::new(vec![i; 10]);
            client.send(msg.clone()).await.unwrap();
            let received = client.recv().await.unwrap();
            assert_eq!(msg.data, received.data);
        }

        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_streaming_multiple_responses() {
        use rpc_codec_json::JsonCodec;
        use rpc_core::{Codec, ResponseResult, RpcRequest, RpcResponse};

        let listener = WebSocketListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.listener.local_addr().unwrap();

        let codec = JsonCodec;

        let server_task = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();

            let msg = transport.recv().await.unwrap();
            let request: RpcRequest = codec.decode(&msg.data).unwrap();
            assert_eq!(request.method, "stream_data");

            let request_id = request.id;

            for i in 0..5 {
                let response = RpcResponse {
                    id: request_id,
                    result: ResponseResult::StreamChunk(vec![i; 100]),
                };
                let data = codec.encode(&response).unwrap();
                transport.send(Message::new(data)).await.unwrap();
            }

            let end_response = RpcResponse {
                id: request_id,
                result: ResponseResult::StreamEnd,
            };
            let data = codec.encode(&end_response).unwrap();
            transport.send(Message::new(data)).await.unwrap();
        });

        let url = format!("ws://{}", addr);
        let mut client = WebSocketTransport::connect(&url).await.unwrap();

        let request = RpcRequest {
            id: 1,
            method: "stream_data".to_string(),
            params: vec![],
        };
        let data = codec.encode(&request).unwrap();
        client.send(Message::new(data)).await.unwrap();

        let mut chunks_received = 0;
        loop {
            let msg = client.recv().await.unwrap();
            let response: RpcResponse = codec.decode(&msg.data).unwrap();
            assert_eq!(response.id, 1);

            match response.result {
                ResponseResult::StreamChunk(data) => {
                    assert_eq!(data.len(), 100);
                    assert_eq!(data[0], chunks_received);
                    chunks_received += 1;
                }
                ResponseResult::StreamEnd => {
                    break;
                }
                _ => panic!("Unexpected response type"),
            }
        }

        assert_eq!(chunks_received, 5);
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_streaming_with_early_termination() {
        use rpc_codec_json::JsonCodec;
        use rpc_core::{Codec, ResponseResult, RpcRequest, RpcResponse};

        let listener = WebSocketListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.listener.local_addr().unwrap();

        let codec = JsonCodec;

        let server_task = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();

            let msg = transport.recv().await.unwrap();
            let request: RpcRequest = codec.decode(&msg.data).unwrap();

            let request_id = request.id;

            for i in 0..10 {
                let response = RpcResponse {
                    id: request_id,
                    result: ResponseResult::StreamChunk(vec![i]),
                };
                let data = codec.encode(&response).unwrap();
                if transport.send(Message::new(data)).await.is_err() {
                    return;
                }
            }
        });

        let url = format!("ws://{}", addr);
        let mut client = WebSocketTransport::connect(&url).await.unwrap();

        let request = RpcRequest {
            id: 1,
            method: "stream_data".to_string(),
            params: vec![],
        };
        let data = codec.encode(&request).unwrap();
        client.send(Message::new(data)).await.unwrap();

        let mut chunks_received = 0;
        for _ in 0..3 {
            let msg = client.recv().await.unwrap();
            let response: RpcResponse = codec.decode(&msg.data).unwrap();
            assert!(matches!(response.result, ResponseResult::StreamChunk(_)));
            chunks_received += 1;
        }

        assert_eq!(chunks_received, 3);
        drop(client);

        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_streaming_error_handling() {
        use rpc_codec_json::JsonCodec;
        use rpc_core::{Codec, ResponseResult, RpcRequest, RpcResponse};

        let listener = WebSocketListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.listener.local_addr().unwrap();

        let codec = JsonCodec;

        let server_task = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();

            let msg = transport.recv().await.unwrap();
            let request: RpcRequest = codec.decode(&msg.data).unwrap();

            let request_id = request.id;

            for i in 0..3 {
                let response = RpcResponse {
                    id: request_id,
                    result: ResponseResult::StreamChunk(vec![i]),
                };
                let data = codec.encode(&response).unwrap();
                transport.send(Message::new(data)).await.unwrap();
            }

            let error_response = RpcResponse {
                id: request_id,
                result: ResponseResult::Err("stream error occurred".to_string()),
            };
            let data = codec.encode(&error_response).unwrap();
            transport.send(Message::new(data)).await.unwrap();
        });

        let url = format!("ws://{}", addr);
        let mut client = WebSocketTransport::connect(&url).await.unwrap();

        let request = RpcRequest {
            id: 1,
            method: "stream_data".to_string(),
            params: vec![],
        };
        let data = codec.encode(&request).unwrap();
        client.send(Message::new(data)).await.unwrap();

        let mut chunks_received = 0;
        let mut error_received = false;

        for _ in 0..4 {
            let msg = client.recv().await.unwrap();
            let response: RpcResponse = codec.decode(&msg.data).unwrap();

            match response.result {
                ResponseResult::StreamChunk(_) => {
                    chunks_received += 1;
                }
                ResponseResult::Err(e) => {
                    assert_eq!(e, "stream error occurred");
                    error_received = true;
                    break;
                }
                _ => panic!("Unexpected response type"),
            }
        }

        assert_eq!(chunks_received, 3);
        assert!(error_received);
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_concurrent_streaming_requests() {
        use rpc_codec_json::JsonCodec;
        use rpc_core::{Codec, ResponseResult, RpcRequest, RpcResponse};
        use std::collections::HashMap;

        let listener = WebSocketListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.listener.local_addr().unwrap();

        let codec = JsonCodec;

        let server_task = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();
            let mut active_streams = HashMap::new();

            for _ in 0..2 {
                let msg = transport.recv().await.unwrap();
                let request: RpcRequest = codec.decode(&msg.data).unwrap();
                active_streams.insert(request.id, 0);
            }

            while !active_streams.is_empty() {
                let mut completed = Vec::new();

                for (&request_id, count) in &mut active_streams {
                    if *count < 3 {
                        let response = RpcResponse {
                            id: request_id,
                            result: ResponseResult::StreamChunk(vec![*count]),
                        };
                        let data = codec.encode(&response).unwrap();
                        transport.send(Message::new(data)).await.unwrap();
                        *count += 1;
                    } else {
                        let response = RpcResponse {
                            id: request_id,
                            result: ResponseResult::StreamEnd,
                        };
                        let data = codec.encode(&response).unwrap();
                        transport.send(Message::new(data)).await.unwrap();
                        completed.push(request_id);
                    }
                }

                for id in completed {
                    active_streams.remove(&id);
                }
            }
        });

        let url = format!("ws://{}", addr);
        let mut client = WebSocketTransport::connect(&url).await.unwrap();

        for request_id in 1..=2 {
            let request = RpcRequest {
                id: request_id,
                method: "stream_data".to_string(),
                params: vec![],
            };
            let data = codec.encode(&request).unwrap();
            client.send(Message::new(data)).await.unwrap();
        }

        let mut stream_states = HashMap::new();
        stream_states.insert(1, 0);
        stream_states.insert(2, 0);

        while !stream_states.is_empty() {
            let msg = client.recv().await.unwrap();
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

        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_large_stream_chunks() {
        use rpc_codec_json::JsonCodec;
        use rpc_core::{Codec, ResponseResult, RpcRequest, RpcResponse};

        let listener = WebSocketListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.listener.local_addr().unwrap();

        let codec = JsonCodec;
        let chunk_size = 1024 * 100;

        let server_task = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();

            let msg = transport.recv().await.unwrap();
            let request: RpcRequest = codec.decode(&msg.data).unwrap();

            let request_id = request.id;

            for i in 0..10 {
                let chunk_data: Vec<u8> = (0..chunk_size).map(|j| ((i + j) % 256) as u8).collect();
                let response = RpcResponse {
                    id: request_id,
                    result: ResponseResult::StreamChunk(chunk_data),
                };
                let data = codec.encode(&response).unwrap();
                transport.send(Message::new(data)).await.unwrap();
            }

            let end_response = RpcResponse {
                id: request_id,
                result: ResponseResult::StreamEnd,
            };
            let data = codec.encode(&end_response).unwrap();
            transport.send(Message::new(data)).await.unwrap();
        });

        let url = format!("ws://{}", addr);
        let mut client = WebSocketTransport::connect(&url).await.unwrap();

        let request = RpcRequest {
            id: 1,
            method: "stream_large_data".to_string(),
            params: vec![],
        };
        let data = codec.encode(&request).unwrap();
        client.send(Message::new(data)).await.unwrap();

        let mut total_bytes = 0;
        let mut chunks_received = 0;

        loop {
            let msg = client.recv().await.unwrap();
            let response: RpcResponse = codec.decode(&msg.data).unwrap();

            match response.result {
                ResponseResult::StreamChunk(data) => {
                    assert_eq!(data.len(), chunk_size);
                    total_bytes += data.len();
                    chunks_received += 1;
                }
                ResponseResult::StreamEnd => {
                    break;
                }
                _ => panic!("Unexpected response type"),
            }
        }

        assert_eq!(chunks_received, 10);
        assert_eq!(total_bytes, chunk_size * 10);
        server_task.await.unwrap();
    }
}
