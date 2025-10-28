//! In-process transport example.
//!
//! Demonstrates using RPC within the same process via channels.
//! Run with: cargo run --example inprocess

use rpc_codec_json::JsonCodec;
use rpc_server::rpc;
use rpc_transport_inprocess::InProcessTransport;

// Define the RPC interface
rpc! {
    extern "Rust" {
        fn factorial(n: u64) -> u64;
        fn fibonacci(n: u64) -> u64;
        fn is_prime(n: u64) -> bool;
    }
}

// Server implementation
struct MathService;

impl server::AsyncService for MathService {
    async fn factorial(&mut self, n: u64) -> u64 {
        println!("[Server] Computing factorial({})", n);
        (1..=n).product()
    }

    async fn fibonacci(&mut self, n: u64) -> u64 {
        println!("[Server] Computing fibonacci({})", n);
        if n <= 1 {
            n
        } else {
            let mut a = 0;
            let mut b = 1;
            for _ in 2..=n {
                let tmp = a + b;
                a = b;
                b = tmp;
            }
            b
        }
    }

    async fn is_prime(&mut self, n: u64) -> bool {
        println!("[Server] Checking if {} is prime", n);
        if n < 2 {
            return false;
        }
        for i in 2..=(n as f64).sqrt() as u64 {
            if n % i == 0 {
                return false;
            }
        }
        true
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("In-Process RPC Example\n");
    println!("This demonstrates RPC communication within the same process using channels.\n");

    // Create paired transports
    let (client_transport, server_transport) = InProcessTransport::pair();

    // Start server in background task
    let server_handle = tokio::spawn(async move {
        println!("[Server] Starting in-process RPC server...\n");
        let codec = JsonCodec;
        let service = MathService;

        if let Err(e) = server::serve(service, server_transport, codec).await {
            eprintln!("[Server] Error: {}", e);
        }
    });

    // Give server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Create client
    let codec = JsonCodec;
    let client = client::Client::new(client_transport, codec);

    // Make RPC calls
    println!("[Client] Calling factorial(5)...");
    let result = client.factorial(5).await?;
    println!("[Client] Result: {}\n", result);

    println!("[Client] Calling fibonacci(10)...");
    let result = client.fibonacci(10).await?;
    println!("[Client] Result: {}\n", result);

    println!("[Client] Checking primes from 1 to 20...");
    for i in 1..=20 {
        let is_prime = client.is_prime(i).await?;
        if is_prime {
            println!("[Client] {} is prime", i);
        }
    }

    println!("\n[Client] Testing performance with 100 sequential calls...");
    let start = std::time::Instant::now();
    for i in 1..=100 {
        let _ = client.factorial(i % 20).await?;
    }
    let elapsed = start.elapsed();
    println!(
        "[Client] Completed 100 calls in {:.2}ms ({:.2}µs per call)",
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_micros() as f64 / 100.0
    );

    println!("\nDone!");

    server_handle.abort();

    Ok(())
}
