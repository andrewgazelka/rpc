//! Streaming RPC example (conceptual).
//!
//! NOTE: True streaming support is a future enhancement.
//! This example demonstrates a pattern for simulating streaming by making
//! multiple RPC calls and using callbacks.
//!
//! Run with: cargo run --example streaming

use rpc_codec_json::JsonCodec;
use rpc_server::rpc;
use rpc_transport_inprocess::InProcessTransport;

// Define the RPC interface
rpc! {
    extern "Rust" {
        fn get_chunk(stream_id: u64, chunk_index: u64) -> Option<Vec<u8>>;
        fn start_stream(size: u64) -> u64;
        fn get_total_chunks(stream_id: u64) -> u64;
    }
}

// Server implementation that simulates streaming
struct StreamingService {
    chunk_size: usize,
}

impl server::AsyncService for StreamingService {
    async fn start_stream(&self, size: u64) -> u64 {
        println!("[Server] Starting stream of {} bytes", size);
        // In a real implementation, this would create a stream handle
        // For demo, we just return a stream ID
        1
    }

    async fn get_total_chunks(&self, stream_id: u64) -> u64 {
        println!("[Server] Getting total chunks for stream {}", stream_id);
        // Simulate 10 chunks for this demo
        10
    }

    async fn get_chunk(&self, stream_id: u64, chunk_index: u64) -> Option<Vec<u8>> {
        println!(
            "[Server] Sending chunk {} of stream {}",
            chunk_index, stream_id
        );

        if chunk_index >= 10 {
            return None;
        }

        // Generate dummy data for this chunk
        let data: Vec<u8> = (0..self.chunk_size)
            .map(|i| ((chunk_index + i as u64) % 256) as u8)
            .collect();

        Some(data)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Streaming RPC Example (Simulated)\n");
    println!("This demonstrates patterns for streaming data over RPC.");
    println!("Note: Native streaming support is a future enhancement.\n");

    // Create transport pair
    let (client_transport, server_transport) = InProcessTransport::pair();

    // Start server
    let server_handle = tokio::spawn(async move {
        let codec = JsonCodec;
        let service = StreamingService { chunk_size: 1024 };

        if let Err(e) = server::serve(service, server_transport, codec).await {
            eprintln!("[Server] Error: {}", e);
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Create client
    let codec = JsonCodec;
    let client = client::Client::new(client_transport, codec);

    println!("=== Streaming large data ===\n");

    // Start a stream
    let stream_size = 10_240; // 10KB
    println!("[Client] Starting stream of {} bytes...", stream_size);
    let stream_id = client.start_stream(stream_size).await?;
    println!("[Client] Stream ID: {}\n", stream_id);

    // Get total chunks
    let total_chunks = client.get_total_chunks(stream_id).await?;
    println!("[Client] Total chunks: {}\n", total_chunks);

    // Stream chunks
    let mut total_bytes = 0;
    let mut chunk_index = 0;

    println!("[Client] Streaming chunks...");
    loop {
        let chunk_opt = client.get_chunk(stream_id, chunk_index).await?;

        match chunk_opt {
            Some(chunk) => {
                total_bytes += chunk.len();
                println!(
                    "[Client] Received chunk {} ({} bytes, total: {} bytes)",
                    chunk_index,
                    chunk.len(),
                    total_bytes
                );
                chunk_index += 1;
            }
            None => {
                println!("[Client] Stream complete");
                break;
            }
        }
    }

    println!("\n[Client] Total data received: {} bytes", total_bytes);

    println!("\n=== Future streaming improvements ===");
    println!("Future versions could support:");
    println!("- fn download() -> impl Stream<Item = Vec<u8>>");
    println!("- Automatic chunk management");
    println!("- Backpressure handling");
    println!("- Bidirectional streaming");
    println!("- Server-sent events");

    println!("\nDone!");

    server_handle.abort();

    Ok(())
}
