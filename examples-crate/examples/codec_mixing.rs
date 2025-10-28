//! Codec mixing example.
//!
//! Demonstrates that any codec can work with any transport.
//! Run with: cargo run --example codec_mixing

use rpc_codec_json::JsonCodec;
use rpc_codec_msgpack::MessagePackCodec;
use rpc_core::Codec;
use rpc_examples::Analysis;
use rpc_server::rpc;
use rpc_transport_inprocess::InProcessTransport;

// Define the RPC interface
rpc! {
    extern "Rust" {
        fn compress(data: Vec<u8>) -> Vec<u8>;
        fn analyze(text: String) -> rpc_examples::Analysis;
    }
}

// Server implementation
struct DataService;

impl server::Server for DataService {
    async fn compress(&self, data: Vec<u8>) -> Vec<u8> {
        println!("[Server] Compressing {} bytes", data.len());
        // Simple run-length encoding simulation
        data.into_iter().filter(|&b| b != 0).collect()
    }

    async fn analyze(&self, text: String) -> Analysis {
        println!("[Server] Analyzing text: '{}'", text);
        Analysis {
            length: text.len(),
            word_count: text.split_whitespace().count(),
            char_count: text.chars().count(),
            uppercase_count: text.chars().filter(|c| c.is_uppercase()).count(),
        }
    }
}

async fn test_with_json() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Testing with JSON codec ===\n");

    let (client_transport, server_transport) = InProcessTransport::pair();

    let server_handle = tokio::spawn(async move {
        let codec = JsonCodec;
        let service = DataService;
        server::serve(service, server_transport, codec).await
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let codec = JsonCodec;
    let client = client::Client::new(client_transport, codec);

    println!("[Client] Using JSON codec");
    let result = client.compress(vec![1, 2, 0, 3, 0, 4]).await?;
    println!("[Client] Compressed result: {:?}\n", result);

    let result = client.analyze("Hello World from JSON!".to_string()).await?;
    println!("[Client] Analysis result: {:?}\n", result);

    server_handle.abort();
    Ok(())
}

async fn test_with_msgpack() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing with MessagePack codec ===\n");

    let (client_transport, server_transport) = InProcessTransport::pair();

    let server_handle = tokio::spawn(async move {
        let codec = MessagePackCodec;
        let service = DataService;
        server::serve(service, server_transport, codec).await
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let codec = MessagePackCodec;
    let client = client::Client::new(client_transport, codec);

    println!("[Client] Using MessagePack codec");
    let result = client.compress(vec![5, 6, 0, 7, 0, 8]).await?;
    println!("[Client] Compressed result: {:?}\n", result);

    let result = client
        .analyze("Hello World from MessagePack!".to_string())
        .await?;
    println!("[Client] Analysis result: {:?}\n", result);

    server_handle.abort();
    Ok(())
}

async fn compare_codecs() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Comparing codec efficiency ===\n");

    // Test JSON
    let test_data = Analysis {
        length: 1000,
        word_count: 150,
        char_count: 1000,
        uppercase_count: 50,
    };

    let json_codec = JsonCodec;
    let json_bytes = json_codec.encode(&test_data)?;
    println!("JSON encoded size: {} bytes", json_bytes.len());
    println!("JSON content: {}", String::from_utf8_lossy(&json_bytes));

    // Test MessagePack
    let msgpack_codec = MessagePackCodec;
    let msgpack_bytes = msgpack_codec.encode(&test_data)?;
    println!("\nMessagePack encoded size: {} bytes", msgpack_bytes.len());
    println!("MessagePack content: {:?}", msgpack_bytes);

    let savings = json_bytes.len() as f64 / msgpack_bytes.len() as f64;
    println!(
        "\nMessagePack is {:.1}x more compact than JSON for this data",
        savings
    );

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Codec Mixing Example\n");
    println!("This demonstrates that any codec works with any transport.\n");

    test_with_json().await?;
    test_with_msgpack().await?;
    compare_codecs().await?;

    println!("\n=== Summary ===");
    println!("The same RPC interface and transport worked with:");
    println!("- JSON codec (human-readable, larger)");
    println!("- MessagePack codec (binary, more compact)");
    println!("\nYou can easily swap codecs without changing your RPC definitions!");
    println!("\nDone!");

    Ok(())
}
