//! WebAssembly runtime for executing RPC kernels
//!
//! This crate provides a runtime for executing WebAssembly components that can call RPC methods.

use rpc_core::{Codec, Transport};
use std::sync::Arc;
use std::time::Duration;
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
    max_timeout: Duration,
}

impl<T, C> WasmRuntime<T, C>
where
    T: Transport + Send + 'static,
    C: Codec + Send + 'static,
{
    /// Create a new WASM runtime with a maximum execution timeout
    ///
    /// # Arguments
    /// * `max_timeout` - Maximum CPU time any kernel can use (enforced server-side)
    pub fn new(max_timeout: Duration) -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.async_support(true);
        // Enable epoch-based interruption for CPU time limiting
        config.epoch_interruption(true);

        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);

        // Add WASI support
        wasmtime_wasi::add_to_linker_async(&mut linker)?;

        Ok(Self { engine, linker, max_timeout })
    }

    /// Execute a WASM component kernel with timeout
    ///
    /// # Arguments
    /// * `component_bytes` - The compiled WASM component
    /// * `transport` - Transport for RPC calls
    /// * `codec` - Codec for serialization
    /// * `requested_timeout` - Optional client-requested timeout (capped at max_timeout)
    ///
    /// # Timeout Behavior
    /// The actual timeout used is `min(requested_timeout, max_timeout)`, ensuring the server
    /// always enforces its maximum limit regardless of client requests.
    pub async fn execute(
        &mut self,
        component_bytes: &[u8],
        transport: T,
        codec: C,
        requested_timeout: Option<Duration>,
    ) -> Result<Vec<u8>> {
        // Client can request shorter timeout, but not longer than server max
        let timeout = requested_timeout
            .map(|t| t.min(self.max_timeout))
            .unwrap_or(self.max_timeout);

        let component = Component::from_binary(&self.engine, component_bytes)?;

        let state = RuntimeState::new(transport, codec);
        let mut store = Store::new(&self.engine, state);

        // Set epoch deadline for timeout
        store.set_epoch_deadline(1);

        // Spawn background task to increment epoch after timeout
        let engine = self.engine.clone();
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            engine.increment_epoch();
        });

        // Instantiate the component
        let _instance = self.linker.instantiate_async(&mut store, &component).await?;

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
    /// Create a default runtime with 5 second timeout
    fn default() -> Self {
        Self::new(Duration::from_secs(5)).expect("Failed to create WASM runtime")
    }
}
