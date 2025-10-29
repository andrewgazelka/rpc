//! WebAssembly runtime for executing RPC kernels
//!
//! This crate provides a runtime for executing WebAssembly components that can call RPC methods.

use rpc_core::{Codec, Transport};
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};

/// Error types for WASM runtime
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("WASM error: {0}")]
    Wasm(#[from] wasmtime::Error),

    #[error("RPC error: {0}")]
    Rpc(#[from] rpc_core::Error),

    #[error("Component error: {0}")]
    Component(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Runtime state that holds the RPC client and WASI context
pub struct RuntimeState<T, C>
where
    T: Transport + 'static,
    C: Codec + 'static,
{
    wasi: WasiCtx,
    transport: Arc<Mutex<T>>,
    codec: Arc<C>,
}

impl<T, C> RuntimeState<T, C>
where
    T: Transport + 'static,
    C: Codec + 'static,
{
    pub fn new(transport: T, codec: C) -> Self {
        Self {
            wasi: WasiCtxBuilder::new().build(),
            transport: Arc::new(Mutex::new(transport)),
            codec: Arc::new(codec),
        }
    }
}

impl<T, C> WasiView for RuntimeState<T, C>
where
    T: Transport + 'static,
    C: Codec + 'static,
{
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }

    fn table(&mut self) -> &mut wasmtime::component::ResourceTable {
        unimplemented!("Resource table not yet implemented")
    }
}

/// WASM kernel runtime
pub struct WasmRuntime<T, C>
where
    T: Transport + 'static,
    C: Codec + 'static,
{
    engine: Engine,
    linker: Linker<RuntimeState<T, C>>,
}

impl<T, C> WasmRuntime<T, C>
where
    T: Transport + Send + 'static,
    C: Codec + Send + 'static,
{
    /// Create a new WASM runtime
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.async_support(true);

        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);

        // Add WASI support
        wasmtime_wasi::add_to_linker_async(&mut linker)?;

        Ok(Self { engine, linker })
    }

    /// Execute a WASM component kernel
    pub async fn execute(
        &mut self,
        component_bytes: &[u8],
        transport: T,
        codec: C,
    ) -> Result<Vec<u8>> {
        let component = Component::from_binary(&self.engine, component_bytes)?;

        let state = RuntimeState::new(transport, codec);
        let mut store = Store::new(&self.engine, state);

        // Instantiate the component
        let instance = self.linker.instantiate_async(&mut store, &component).await?;

        // TODO: Call the kernel's exported function and get result
        // For now, return empty result
        Ok(Vec::new())
    }
}

impl<T, C> Default for WasmRuntime<T, C>
where
    T: Transport + Send + 'static,
    C: Codec + Send + 'static,
{
    fn default() -> Self {
        Self::new().expect("Failed to create WASM runtime")
    }
}
