//! Stdio transport example demonstrating embedded binary usage.
//!
//! This example shows how to use stdio transport for process-to-process communication.
//! Run as server: cargo run --example stdio_embedded -- server
//! Run as client: cargo run --example stdio_embedded -- client | cargo run --example stdio_embedded -- server

use rpc_codec_msgpack::MessagePackCodec;
use rpc_server::rpc;
use rpc_transport_stdio::StdioTransport;

// Define the RPC interface
rpc! {
    extern "Rust" {
        fn process_data(data: Vec<u8>) -> Vec<u8>;
        fn transform(text: String) -> String;
        fn calculate(x: f64, y: f64, op: String) -> f64;
    }
}

// Server implementation
struct DataProcessor;

impl server::Server for DataProcessor {
    async fn process_data(&self, data: Vec<u8>) -> Vec<u8> {
        // Simple transformation: reverse the bytes
        data.into_iter().rev().collect()
    }

    async fn transform(&self, text: String) -> String {
        // Transform: uppercase and reverse
        text.to_uppercase().chars().rev().collect()
    }

    async fn calculate(&self, x: f64, y: f64, op: String) -> f64 {
        match op.as_str() {
            "add" => x + y,
            "sub" => x - y,
            "mul" => x * y,
            "div" if y != 0.0 => x / y,
            _ => 0.0,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("demo");

    match mode {
        "server" => run_server().await?,
        "client" => run_client().await?,
        _ => run_demo().await?,
    }

    Ok(())
}

async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[Server] Starting stdio RPC server...");

    let transport = StdioTransport::new();
    let codec = MessagePackCodec;
    let service = DataProcessor;

    eprintln!("[Server] Ready to accept requests");
    server::serve(service, transport, codec).await?;

    Ok(())
}

async fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[Client] Connecting via stdio...");

    let transport = StdioTransport::new();
    let codec = MessagePackCodec;
    let client = client::Client::new(transport, codec);

    // Make some test calls
    eprintln!("[Client] Calling process_data([1,2,3,4,5])...");
    let result = client.process_data(vec![1, 2, 3, 4, 5]).await?;
    eprintln!("[Client] Result: {:?}", result);

    eprintln!("[Client] Calling transform(\"Hello\")...");
    let result = client.transform("Hello".to_string()).await?;
    eprintln!("[Client] Result: {}", result);

    eprintln!("[Client] Calling calculate(10, 5, \"add\")...");
    let result = client.calculate(10.0, 5.0, "add".to_string()).await?;
    eprintln!("[Client] Result: {}", result);

    Ok(())
}

async fn run_demo() -> Result<(), Box<dyn std::error::Error>> {
    println!("Stdio RPC Embedded Binary Example\n");
    println!("This demonstrates how to use RPC over stdio for process communication.\n");
    println!("Usage:");
    println!("  As server: cargo run --example stdio_embedded -- server");
    println!("  As client: cargo run --example stdio_embedded -- client\n");
    println!("For piped communication:");
    println!("  cargo run --example stdio_embedded -- client | cargo run --example stdio_embedded -- server\n");
    println!("Demo mode: simulating in-process communication...\n");

    // For demo, we'll use in-process channels to simulate
    use rpc_transport_inprocess::InProcessTransport;

    let (client_transport, server_transport) = InProcessTransport::pair();

    // Start server
    let server_handle = tokio::spawn(async move {
        let codec = MessagePackCodec;
        let service = DataProcessor;
        server::serve(service, server_transport, codec).await
    });

    // Run client
    let codec = MessagePackCodec;
    let client = client::Client::new(client_transport, codec);

    println!("[Client] Calling process_data([1,2,3,4,5])...");
    let result = client.process_data(vec![1, 2, 3, 4, 5]).await?;
    println!("[Client] Result: {:?} (reversed bytes)\n", result);

    println!("[Client] Calling transform(\"Hello\")...");
    let result = client.transform("Hello".to_string()).await?;
    println!("[Client] Result: {} (uppercase + reversed)\n", result);

    println!("[Client] Calling calculate(10, 5, \"mul\")...");
    let result = client.calculate(10.0, 5.0, "mul".to_string()).await?;
    println!("[Client] Result: {}\n", result);

    println!("Demo complete!");

    server_handle.abort();

    Ok(())
}
