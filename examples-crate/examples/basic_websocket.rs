//! Basic WebSocket client-server example.
//!
//! Run with: cargo run --example basic_websocket

use rpc_codec_json::JsonCodec;
use rpc_server::rpc;
use rpc_transport_ws::{WebSocketListener, WebSocketTransport};

// Define the RPC interface
rpc! {
    extern "Rust" {
        fn add(a: i32, b: i32) -> i32;
        fn greet(name: String) -> String;
        fn echo(msg: String) -> String;
    }
}

// Implement the server
struct Calculator;

impl server::AsyncService for Calculator {
    async fn add(&mut self, a: i32, b: i32) -> i32 {
        println!("[Server] Computing {} + {}", a, b);
        a + b
    }

    async fn greet(&mut self, name: String) -> String {
        println!("[Server] Greeting {}", name);
        format!("Hello, {}!", name)
    }

    async fn echo(&mut self, msg: String) -> String {
        println!("[Server] Echoing: {}", msg);
        msg
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting WebSocket RPC example...\n");

    // Start server in background
    let listener = WebSocketListener::bind("127.0.0.1:8080").await?;
    println!("[Server] Listening on 127.0.0.1:8080");

    let server_handle = tokio::spawn(async move {
        loop {
            let transport = listener.accept().await.unwrap();
            println!("[Server] Client connected");

            let codec = JsonCodec;
            let service = Calculator;

            if let Err(e) = server::serve(service, transport, codec).await {
                eprintln!("[Server] Error: {}", e);
            }
        }
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Connect client
    println!("\n[Client] Connecting to server...");
    let transport = WebSocketTransport::connect("ws://127.0.0.1:8080").await?;
    let codec = JsonCodec;
    let client = client::Client::new(transport, codec);

    // Make RPC calls
    println!("\n[Client] Calling add(5, 3)...");
    let result = client.add(5, 3).await?;
    println!("[Client] Result: {}\n", result);

    println!("[Client] Calling greet(\"Alice\")...");
    let result = client.greet("Alice".to_string()).await?;
    println!("[Client] Result: {}\n", result);

    println!("[Client] Calling echo(\"Hello, RPC!\")...");
    let result = client.echo("Hello, RPC!".to_string()).await?;
    println!("[Client] Result: {}\n", result);

    println!("Done! Press Ctrl+C to exit.");

    // Keep server running
    server_handle.await?;

    Ok(())
}
