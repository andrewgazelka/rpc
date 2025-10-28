//! OpenAPI and TypeScript generation example.
//!
//! Demonstrates generating OpenAPI specs and TypeScript clients from RPC schemas.
//! Run with: cargo run --example openapi_gen

use rpc_openapi::MethodSchema as RpcMethodSchema;
use rpc_openapi::{generate_openapi_spec, generate_typescript_client};
use rpc_server::rpc;
use std::collections::HashMap;

// Define RPC interface
rpc! {
    extern "Rust" {
        fn add(a: i32, b: i32) -> i32;
        fn greet(name: String) -> String;
        fn get_user(id: u64) -> Option<rpc_examples::User>;
    }
}

fn main() {
    println!("OpenAPI and TypeScript Generation Example\n");
    println!("==========================================\n");

    // Get the schema from the generated client
    type AnyTransport = rpc_transport_inprocess::InProcessTransport;
    type AnyCodec = rpc_codec_json::JsonCodec;
    let schemas = client::Client::<AnyTransport, AnyCodec>::schema();

    // Convert to the format expected by rpc-openapi
    let mut method_schemas = HashMap::new();
    for (name, schema) in schemas {
        method_schemas.insert(
            name.clone(),
            RpcMethodSchema {
                name,
                params: schema.params,
                returns: schema.returns,
            },
        );
    }

    // Generate OpenAPI spec
    println!("=== OpenAPI 3.0 Specification ===\n");
    let openapi_spec = generate_openapi_spec("My RPC API", "1.0.0", method_schemas.clone());
    let openapi_json = serde_json::to_string_pretty(&openapi_spec).unwrap();
    println!("{}\n", openapi_json);

    // Generate TypeScript client
    println!("=== TypeScript Client ===\n");
    let ts_client =
        generate_typescript_client("MyApiClient", "http://localhost:8080", method_schemas);
    println!("{}", ts_client);

    println!("\n=== Usage Instructions ===");
    println!("1. Save the OpenAPI spec to 'openapi.json'");
    println!("2. Save the TypeScript client to 'client.ts'");
    println!("3. Use the TypeScript client in your frontend:");
    println!("\n   const client = new MyApiClient('http://localhost:8080');");
    println!("   const result = await client.add(5, 3);");
    println!("   console.log(result); // 8");
}
