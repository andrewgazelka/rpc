# rpc

RPC framework with server-side WASM composition. Write Rust once, get WIT/OpenAPI/TypeScript clients.

## Server-side composition

```rust
// 100 network round-trips
for i in 0..100 {
    client.increment(i).await?;
}

// 1 network round-trip - execute kernel server-side
wit_bindgen::generate!({ world: "counter-kernel" });

impl Guest for Kernel {
    fn run() -> u32 {
        let mut value = 0;
        for _ in 0..100 {
            value = counter::increment(value);
        }
        value
    }
}

runtime.execute(kernel_bytes, transport, codec).await?;
```

WASM kernel: 14KB, sandboxed via wasmtime.

## Kernel caching

Servers cache WASM binaries by Blake3 hash to avoid re-uploading:

```rust
use rpc_wasm_runtime::{WasmRuntime, Blake3Hash};

// Client computes hash
let hash = Blake3Hash::hash(&wasm_bytes);

// Try to execute (may fail if not cached)
match runtime.execute_by_hash(hash, transport, codec, None).await {
    Ok(result) => result,
    Err(Error::KernelNotFound(_)) => {
        // Store binary first, then retry
        runtime.store_kernel(hash, wasm_bytes)?;
        runtime.execute_by_hash(hash, transport, codec, None).await?
    }
    Err(e) => return Err(e),
}
```

Cache behavior:
- LRU cache holds 100 binaries by default (configurable)
- Hash verification prevents tampering
- Frequently used kernels stay hot
- Reduces network overhead for repeated executions

## CPU time limiting

Servers enforce maximum kernel execution time using epoch-based interruption:

```rust
use std::time::Duration;
use rpc_wasm_runtime::WasmRuntime;

// Server creates runtime with maximum 5 second timeout (enforced server-side)
let mut runtime = WasmRuntime::new(Duration::from_secs(5))?;

// Client can request shorter timeout, but not longer than server maximum
runtime.execute_by_hash(hash, transport, codec, Some(Duration::from_millis(500))).await?;

// Server maximum is enforced if client requests longer or no timeout
runtime.execute_by_hash(hash, transport, codec, Some(Duration::from_secs(10))).await?;  // Capped at 5s
runtime.execute_by_hash(hash, transport, codec, None).await?;  // Uses 5s default
```

Timeout behavior:
- Server sets `max_timeout` when creating `WasmRuntime`
- Actual timeout: `min(client_requested, server_max)`
- Prevents malicious clients from monopolizing resources
- Epoch interruption has approximately 10% overhead (vs 2-3x for fuel-based limiting)

## Example: data aggregation

```rust
// 4 round-trips
let user = client.get_user(id).await?;
let posts = client.get_posts(user.id).await?;
let comments = client.get_comments(user.id).await?;
let likes = client.get_likes(user.id).await?;

// 1 round-trip
impl Guest for Kernel {
    fn run(id: UserId) -> UserProfile {
        let user = api.get_user(id);
        let posts = api.get_posts(user.id);
        let comments = api.get_comments(user.id);
        let likes = api.get_likes(user.id);
        UserProfile { user, posts, comments, likes }
    }
}
```

## Example: conditional logic

```rust
// 2-3 round-trips depending on balance
let balance = client.get_balance(account).await?;
if balance > amount {
    client.transfer(account, dest, amount).await?;
} else {
    client.request_overdraft(account, amount).await?;
}

// 1 round-trip
impl Guest for Kernel {
    fn run(account: AccountId, dest: AccountId, amount: u64) -> TransferResult {
        let balance = banking.get_balance(account);
        if balance > amount {
            banking.transfer(account, dest, amount)
        } else {
            banking.request_overdraft(account, amount)
        }
    }
}
```

## How WIT generation works

```rust
// Define RPC interface
rpc! {
    extern "Rust" {
        fn increment(value: u32) -> u32;
        fn get_value() -> u32;
    }
}

// Generate WIT from schema
let wit = client::Client::<T, C>::wit_schema("counter");

// Output:
// interface counter {
//     increment: func(value: u32) -> u32
//     get-value: func() -> u32
// }
```

Write WASM kernel against the WIT interface. Compile to wasm32-wasip2. Execute server-side.

## Basic usage

```rust
rpc! {
    extern "Rust" {
        fn add(a: i32, b: i32) -> i32;
    }
}

// Server
struct Service;
impl server::AsyncService for Service {
    async fn add(&mut self, a: i32, b: i32) -> i32 { a + b }
}

server::serve(Service, WebSocketTransport::bind("127.0.0.1:8080").await?, JsonCodec).await?;

// Client
let client = client::Client::new(
    WebSocketTransport::connect("ws://127.0.0.1:8080").await?,
    JsonCodec
);
let result = client.add(2, 3).await?;  // 5
```

Swap transports and codecs:

```rust
server::serve(service, WebSocketTransport, JsonCodec).await?;
server::serve(service, WebSocketTransport, MessagePackCodec).await?;
server::serve(service, StdioTransport, JsonCodec).await?;
```

## Auto-generated output

From one `rpc!` definition:

- Rust client (typed, async)
- Rust server (typed, async)
- OpenAPI 3.0 spec
- TypeScript client
- WIT definition

```rust
let schemas = client::Client::<T, C>::schema();
let openapi = generate_openapi_spec("API", "1.0.0", schemas.clone());
let typescript = generate_typescript_client("Client", "http://localhost", schemas);
let wit = client::Client::<T, C>::wit_schema("interface-name");
```

## Install

```toml
[dependencies]
rpc-server = { git = "https://github.com/andrewgazelka/rpc" }
rpc-codec-json = { git = "https://github.com/andrewgazelka/rpc" }
rpc-transport-ws = { git = "https://github.com/andrewgazelka/rpc" }
rpc-wasm-runtime = { git = "https://github.com/andrewgazelka/rpc" }  # for kernels
```

## Examples

- `examples-crate/examples/basic_websocket.rs`
- `examples-crate/examples/codec_mixing.rs`
- `examples-crate/examples/bidirectional.rs`
- `examples-crate/examples/wit_gen.rs`
- `crates/rpc-wasm-runtime/tests/wasm_kernel_test.rs`

## Packages

| Package | Lines |
|---------|-------|
| rpc-core | ~100 |
| rpc-macro | ~330 |
| rpc-wasm-runtime | ~125 |
| rpc-codec-json | ~50 |
| rpc-codec-msgpack | ~50 |
| rpc-transport-ws | ~100 |
| rpc-transport-stdio | ~80 |
| rpc-openapi | ~400 |

Total: ~1,235 LOC

Schema system: [github.com/andrewgazelka/schema](https://github.com/andrewgazelka/schema) (~800 LOC)

## Comparison

|  | gRPC | tarpc | rpc |
|---|:---:|:---:|:---:|
| Rust source of truth | ❌ | ✅ | ✅ |
| Swap transport | ❌ | ❌ | ✅ |
| Swap codec | ❌ | ✅ | ✅ |
| WASM kernels | ❌ | ❌ | ✅ |
| WIT generation | ❌ | ❌ | ✅ |
| OpenAPI generation | ❌ | ❌ | ✅ |

## License

MIT
