//! Test DynamicClient with call method

use rpc_server::rpc;
use schema::Schema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Schema)]
pub struct DataChunk {
    pub index: u32,
    pub data: Vec<u8>,
}

rpc! {
    extern "Rust" {
        fn get_data(index: u32) -> DataChunk;
        fn echo(msg: String) -> String;
    }
}

struct TestService;

impl server::AsyncService for TestService {
    async fn get_data(&self, index: u32) -> DataChunk {
        DataChunk {
            index,
            data: vec![index as u8; 10],
        }
    }

    async fn echo(&self, msg: String) -> String {
        msg
    }
}

#[tokio::test]
async fn test_dynamic_client_get_data() {
    use rpc_codec_json::JsonCodec;
    use rpc_transport_inprocess::InProcessTransport;

    let (client_transport, server_transport) = InProcessTransport::pair();

    let service = TestService;

    let server_handle = tokio::spawn(async move {
        let codec = JsonCodec;
        if let Err(e) = server::serve(service, server_transport, codec).await {
            eprintln!("Server error: {}", e);
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let codec = JsonCodec;
    let dyn_client = client::DynamicClient::new(client_transport, codec);

    // Test get_data via DynamicClient
    let index = 5u32;
    let params = serde_json::to_vec(&(index,)).unwrap();

    let result_bytes = dyn_client.call("get_data", params).await.unwrap();
    let chunk: DataChunk = serde_json::from_slice(&result_bytes).unwrap();

    assert_eq!(chunk.index, 5);
    assert_eq!(chunk.data.len(), 10);
    assert_eq!(chunk.data[0], 5);

    server_handle.abort();
}

#[tokio::test]
async fn test_dynamic_client_echo() {
    use rpc_codec_json::JsonCodec;
    use rpc_transport_inprocess::InProcessTransport;

    let (client_transport, server_transport) = InProcessTransport::pair();

    let service = TestService;

    let server_handle = tokio::spawn(async move {
        let codec = JsonCodec;
        if let Err(e) = server::serve(service, server_transport, codec).await {
            eprintln!("Server error: {}", e);
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let codec = JsonCodec;
    let dyn_client = client::DynamicClient::new(client_transport, codec);

    // Test echo via DynamicClient
    let msg = "hello world".to_string();
    let params = serde_json::to_vec(&(msg.clone(),)).unwrap();

    let response_bytes = dyn_client.call("echo", params).await.unwrap();
    let response: String = serde_json::from_slice(&response_bytes).unwrap();

    assert_eq!(response, msg);

    server_handle.abort();
}
