//! Bidirectional RPC over HTTP with Server-Sent Events.
//!
//! Demonstrates bidirectional communication that works behind NAT:
//! - Client -> Server: HTTP POST requests
//! - Server -> Client: Server-Sent Events (SSE)
//!
//! This pattern allows both parties to communicate even when the client
//! is behind NAT or a firewall, since the client initiates both connections.
//!
//! Run with: cargo run --example http_bidirectional

use rpc_codec_json::JsonCodec;
use rpc_server::rpc;
use rpc_transport_http::{HttpListener, HttpTransport};
use std::time::Duration;

// Define server-side RPC interface (client calls these)
rpc! {
    extern "Rust" {
        fn echo(message: String) -> String;
        fn add(a: i32, b: i32) -> i32;
    }
}

// Server implementation
struct EchoServer;

impl server::AsyncService for EchoServer {
    async fn echo(&self, message: String) -> String {
        println!("[Server] Received echo request: {}", message);
        format!("Server echo: {}", message)
    }

    async fn add(&self, a: i32, b: i32) -> i32 {
        println!("[Server] Received add request: {} + {}", a, b);
        let result = a + b;
        println!("[Server] Returning result: {}", result);
        result
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("HTTP + SSE Bidirectional RPC Example\n");
    println!("This demonstrates bidirectional communication over HTTP that works behind NAT:");
    println!("- Client sends requests via HTTP POST");
    println!("- Server sends responses via Server-Sent Events (SSE)");
    println!("- Both connections are initiated by the client\n");

    // Start HTTP server
    let listener = HttpListener::bind("127.0.0.1:8080").await?;
    let server_transport = listener.serve().await?;
    let addr = server_transport.local_addr();

    println!("[Server] HTTP server listening on http://{}\n", addr);

    // Spawn server task
    let server_handle = tokio::spawn(async move {
        let codec = JsonCodec;
        let service = EchoServer;

        if let Err(e) = server::serve(service, server_transport, codec).await {
            eprintln!("[Server] Error: {}", e);
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let url = format!("http://{}", addr);
    println!("[Client] Connecting to server at {}", url);
    let client_transport = HttpTransport::connect(&url).await?;

    // Give SSE connection time to establish
    tokio::time::sleep(Duration::from_millis(100)).await;
    println!("[Client] Connected! SSE stream established for receiving responses\n");

    let codec = JsonCodec;
    let client = client::Client::new(client_transport, codec);

    println!("=== Making RPC calls over HTTP + SSE ===\n");

    // Call echo method
    println!("[Client] Calling echo(\"Hello over HTTP!\")...");
    let echo_result = client.echo("Hello over HTTP!".to_string()).await?;
    println!("[Client] Result: {}\n", echo_result);

    // Call add method
    println!("[Client] Calling add(42, 58)...");
    let add_result = client.add(42, 58).await?;
    println!("[Client] Result: {}\n", add_result);

    // More calls to show bidirectional streaming
    println!("[Client] Making multiple sequential calls...");
    for i in 0..3 {
        let msg = format!("Message #{}", i + 1);
        println!("[Client] Sending: {}", msg);
        let result = client.echo(msg).await?;
        println!("[Client] Received: {}", result);
    }

    println!("\n=== Summary ===");
    println!("Successfully demonstrated:");
    println!("1. Client sending HTTP POST requests to server");
    println!("2. Server sending responses via SSE to client");
    println!("3. Bidirectional communication working behind NAT");
    println!("4. Multiple sequential RPC calls over the same connection");
    println!("\nKey advantage: This works even when client is behind NAT/firewall");
    println!("because the client initiates both HTTP and SSE connections!\n");

    println!("Done!");

    server_handle.abort();

    Ok(())
}
