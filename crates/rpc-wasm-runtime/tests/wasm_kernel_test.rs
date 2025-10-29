//! Integration test for WASM kernel execution with RPC client imports

use rpc_codec_json::JsonCodec;
use rpc_server::rpc;
use rpc_transport_inprocess::InProcessTransport;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker, bindgen};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};

// Define the counter RPC interface
rpc! {
    extern "Rust" {
        fn increment(value: u32) -> u32;
        fn get_value() -> u32;
    }
}

// Counter service implementation
struct CounterService {
    value: Arc<Mutex<u32>>,
}

impl server::AsyncService for CounterService {
    async fn increment(&mut self, value: u32) -> u32 {
        let new_value = value + 1;
        *self.value.lock().await = new_value;
        new_value
    }

    async fn get_value(&mut self) -> u32 {
        *self.value.lock().await
    }
}

// Generate host-side bindings from WIT
bindgen!({
    path: "counter.wit",
    world: "counter-kernel",
    async: true,
});

// Host state that provides RPC client to WASM
struct HostState {
    wasi: WasiCtx,
    client: Arc<Mutex<client::Client<InProcessTransport, JsonCodec>>>,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }

    fn table(&mut self) -> &mut wasmtime::component::ResourceTable {
        unimplemented!("Resource table not needed for this test")
    }
}

// Implement the counter interface host functions
#[async_trait::async_trait]
impl rpc::kernel::counter::Host for HostState {
    async fn increment(&mut self, value: u32) -> u32 {
        self.client
            .lock()
            .await
            .increment(value)
            .await
            .expect("RPC call failed")
    }

    async fn get_value(&mut self) -> u32 {
        self.client
            .lock()
            .await
            .get_value()
            .await
            .expect("RPC call failed")
    }
}

#[tokio::test]
async fn test_wasm_kernel_execution() {
    // Load the WASM component
    let wasm_bytes = include_bytes!("../../../tests/wasm-kernel/guest/target/wasm32-wasip2/release/counter_kernel_guest.wasm");

    // Create RPC client and server with in-process transport
    let (client_transport, server_transport) = InProcessTransport::pair();

    // Start the server in a background task
    let counter_value = Arc::new(Mutex::new(0u32));
    let service = CounterService {
        value: counter_value.clone(),
    };

    tokio::spawn(async move {
        server::serve(service, server_transport, JsonCodec)
            .await
            .expect("Server failed");
    });

    // Create RPC client
    let rpc_client = client::Client::new(client_transport, JsonCodec);

    // Set up wasmtime engine and component
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.async_support(true);
    let engine = Engine::new(&config).unwrap();

    let component = Component::from_binary(&engine, wasm_bytes).unwrap();

    // Set up the linker with host functions
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker).unwrap();
    CounterKernel::add_to_linker(&mut linker, |state: &mut HostState| state).unwrap();

    // Create host state with RPC client
    let host_state = HostState {
        wasi: WasiCtxBuilder::new().build(),
        client: Arc::new(Mutex::new(rpc_client)),
    };

    let mut store = Store::new(&engine, host_state);

    // Instantiate and run the kernel
    let bindings = CounterKernel::instantiate_async(&mut store, &component, &linker)
        .await
        .unwrap();

    // Call the kernel's run function
    let result: u32 = bindings
        .rpc_kernel_kernel()
        .call_run(&mut store)
        .await
        .unwrap();

    // The kernel should call increment() 10 times starting from 0
    // Expected result: 10
    assert_eq!(result, 10, "Kernel should return 10 after incrementing 10 times");

    // Verify the server's internal state was updated
    let final_value = *counter_value.lock().await;
    assert_eq!(final_value, 10, "Server counter should be 10");

    println!("✓ WASM kernel executed successfully!");
    println!("  - Kernel result: {}", result);
    println!("  - Server state: {}", final_value);
}
