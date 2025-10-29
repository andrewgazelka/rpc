//! WIT (WebAssembly Interface Type) generation example.
//!
//! Demonstrates generating WIT interface definitions from RPC schemas.
//! Run with: cargo run --example wit_gen

use rpc_server::rpc;

// Define RPC interface for a simple counter service
rpc! {
    extern "Rust" {
        fn increment(value: u32) -> u32;
        fn get_value() -> u32;
        fn reset() -> ();
        fn add(a: i32, b: i32) -> i32;
    }
}

fn main() {
    println!("WIT Generation Example\n");
    println!("======================\n");

    // Get the WIT schema from the generated client
    type AnyTransport = rpc_transport_inprocess::InProcessTransport;
    type AnyCodec = rpc_codec_json::JsonCodec;
    let wit = client::Client::<AnyTransport, AnyCodec>::wit_schema("counter");

    println!("=== Generated WIT Interface ===\n");
    println!("{}", wit);

    println!("\n=== Usage Instructions ===");
    println!("1. Save the WIT definition to 'counter.wit'");
    println!("2. Use it to define WebAssembly component interfaces");
    println!("3. Compile WASM components that implement or use this interface");
    println!("\nExample WIT usage in a world definition:");
    println!("  world counter-service {{");
    println!("    import counter");
    println!("    export kernel");
    println!("  }}");
}
