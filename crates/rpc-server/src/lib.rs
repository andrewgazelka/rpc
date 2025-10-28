//! RPC server runtime.
//!
//! This crate re-exports the procedural macro and provides examples.

pub use rpc_core;
pub use rpc_macro::rpc;

#[cfg(test)]
mod tests {
    use super::*;

    // Example RPC interface
    rpc! {
        extern "Rust" {
            fn add(a: i32, b: i32) -> i32;
            fn greet(name: String) -> String;
            fn multiply(x: f64, y: f64) -> f64;
        }
    }

    // Server implementation
    struct MyService;

    impl server::AsyncService for MyService {
        async fn add(&mut self, a: i32, b: i32) -> i32 {
            a + b
        }

        async fn greet(&mut self, name: String) -> String {
            format!("Hello, {}!", name)
        }

        async fn multiply(&mut self, x: f64, y: f64) -> f64 {
            x * y
        }
    }

    #[tokio::test]
    async fn test_rpc_end_to_end() {
        use rpc_codec_json::JsonCodec;
        use rpc_transport_ws::{WebSocketListener, WebSocketTransport};

        // Start server
        let listener = WebSocketListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let transport = listener.accept().await.unwrap();
            let codec = JsonCodec;
            let service = MyService;

            // Handle just one request for testing
            server::serve(service, transport, codec).await
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Connect client
        let url = format!("ws://{}", addr);
        let transport = WebSocketTransport::connect(&url).await.unwrap();
        let codec = JsonCodec;
        let client = client::Client::new(transport, codec);

        // Test add
        let result = client.add(2, 3).await.unwrap();
        assert_eq!(result, 5);

        // Test greet
        let result = client.greet("World".to_string()).await.unwrap();
        assert_eq!(result, "Hello, World!");

        // Test multiply
        let result = client.multiply(2.5, 4.0).await.unwrap();
        assert_eq!(result, 10.0);

        // Cleanup
        drop(client);
        server_task.abort();
    }

    #[tokio::test]
    async fn test_rpc_with_msgpack() {
        use rpc_codec_msgpack::MessagePackCodec;
        use rpc_transport_ws::{WebSocketListener, WebSocketTransport};

        let listener = WebSocketListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let transport = listener.accept().await.unwrap();
            let codec = MessagePackCodec;
            let service = MyService;

            server::serve(service, transport, codec).await
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let url = format!("ws://{}", addr);
        let transport = WebSocketTransport::connect(&url).await.unwrap();
        let codec = MessagePackCodec;
        let client = client::Client::new(transport, codec);

        // Test with MessagePack codec
        let result = client.add(10, 20).await.unwrap();
        assert_eq!(result, 30);

        drop(client);
        server_task.abort();
    }

    #[tokio::test]
    async fn test_multiple_sequential_calls() {
        use rpc_codec_json::JsonCodec;
        use rpc_transport_ws::{WebSocketListener, WebSocketTransport};

        let listener = WebSocketListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let transport = listener.accept().await.unwrap();
            let codec = JsonCodec;
            let service = MyService;

            server::serve(service, transport, codec).await
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let url = format!("ws://{}", addr);
        let transport = WebSocketTransport::connect(&url).await.unwrap();
        let codec = JsonCodec;
        let client = client::Client::new(transport, codec);

        // Multiple sequential calls
        for i in 0..10 {
            let result = client.add(i, i + 1).await.unwrap();
            assert_eq!(result, 2 * i + 1);
        }

        drop(client);
        server_task.abort();
    }
}
