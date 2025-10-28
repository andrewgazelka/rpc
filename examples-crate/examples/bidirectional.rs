//! Bidirectional RPC example.
//!
//! Demonstrates server calling client methods (not just client calling server).
//! This is useful for callbacks, notifications, and pub/sub patterns.
//! Run with: cargo run --example bidirectional

use rpc_codec_json::JsonCodec;
use rpc_server::rpc;
use rpc_transport_inprocess::InProcessTransport;

// Define server-side RPC interface (client calls these)
rpc! {
    extern "Rust" {
        fn process_task(task_id: u64, data: String) -> String;
        fn get_status() -> String;
    }
}

// Define client-side RPC interface (server calls these)
mod callback {
    use rpc_server::rpc;

    rpc! {
        extern "Rust" {
            fn on_progress(task_id: u64, percent: f64) -> ();
            fn on_complete(task_id: u64, result: String) -> ();
            fn log_message(level: String, message: String) -> ();
        }
    }
}

// Server implementation
struct TaskProcessor;

impl server::AsyncService for TaskProcessor {
    async fn process_task(&self, task_id: u64, data: String) -> String {
        println!("[Server] Processing task {} with data: {}", task_id, data);
        format!("Processed: {}", data)
    }

    async fn get_status(&self) -> String {
        println!("[Server] Status requested");
        "Server is running".to_string()
    }
}

// Client implementation (receives callbacks from server)
struct ClientCallbacks;

impl callback::server::AsyncService for ClientCallbacks {
    async fn on_progress(&self, task_id: u64, percent: f64) {
        println!(
            "[Client Callback] Task {} progress: {:.1}%",
            task_id, percent
        );
    }

    async fn on_complete(&self, task_id: u64, result: String) {
        println!("[Client Callback] Task {} completed: {}", task_id, result);
    }

    async fn log_message(&self, level: String, message: String) {
        println!("[Client Callback] [{}] {}", level, message);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Bidirectional RPC Example\n");
    println!(
        "This demonstrates a server calling client methods for callbacks and notifications.\n"
    );

    // Create two pairs of transports (one for each direction)
    let (client_to_server_transport, server_side_transport) = InProcessTransport::pair();
    let (server_to_client_transport, client_side_callback_transport) = InProcessTransport::pair();

    // Start main server (handles client requests)
    let server_handle = tokio::spawn(async move {
        println!("[Server] Starting main RPC server...\n");
        let codec = JsonCodec;
        let service = TaskProcessor;

        if let Err(e) = server::serve(service, server_side_transport, codec).await {
            eprintln!("[Server] Error: {}", e);
        }
    });

    // Start client callback handler (handles server callbacks)
    let callback_handle = tokio::spawn(async move {
        println!("[Client] Starting callback handler...\n");
        let codec = JsonCodec;
        let callbacks = ClientCallbacks;

        if let Err(e) =
            callback::server::serve(callbacks, client_side_callback_transport, codec).await
        {
            eprintln!("[Client Callback Handler] Error: {}", e);
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Create client (calls server)
    let codec = JsonCodec;
    let client = client::Client::new(client_to_server_transport, codec);

    // Create server-to-client RPC handle (server uses this to call client)
    let callback_codec = JsonCodec;
    let callback_client = callback::client::Client::new(server_to_client_transport, callback_codec);

    println!("=== Demonstrating bidirectional communication ===\n");

    // Client calls server
    println!("[Client] Calling get_status()...");
    let status = client.get_status().await?;
    println!("[Client] Server status: {}\n", status);

    // Simulate server sending callbacks to client
    println!("[Server] Sending progress updates to client...");
    callback_client
        .log_message("INFO".to_string(), "Starting task processing".to_string())
        .await?;

    for percent in [0.0, 25.0, 50.0, 75.0, 100.0] {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        callback_client.on_progress(1, percent).await?;
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    callback_client
        .on_complete(1, "Task completed successfully".to_string())
        .await?;

    println!();

    // Client makes another call
    println!("[Client] Calling process_task()...");
    let result = client.process_task(1, "important data".to_string()).await?;
    println!("[Client] Result: {}\n", result);

    // More server callbacks
    callback_client
        .log_message("INFO".to_string(), "All tasks completed".to_string())
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("\n=== Summary ===");
    println!("This example demonstrated:");
    println!("1. Client calling server methods (process_task, get_status)");
    println!("2. Server calling client methods (on_progress, on_complete, log_message)");
    println!("3. Bidirectional communication using two transport pairs");
    println!("\nDone!");

    server_handle.abort();
    callback_handle.abort();

    Ok(())
}
