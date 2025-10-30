# rpc

Modular RPC framework for Rust. Write once, generate WIT/OpenAPI/TypeScript clients. Swap transports and codecs.

## Table of Contents

- [Features](#features)
- [Basic Usage](#basic-usage)
- [Stateful Services](#stateful-services)
- [Transport and Codec Flexibility](#transport-and-codec-flexibility)
- [Auto-generated Output](#auto-generated-output)
- [Install](#install)
- [Examples](#examples)
- [Packages](#packages)
- [Comparison](#comparison)
- [Experimental: Server-side WASM Composition](#experimental-server-side-wasm-composition)
  - [Overview](#overview)
  - [Writing a Kernel](#writing-a-kernel)
  - [Inline Kernel Compilation (Experimental)](#inline-kernel-compilation-experimental)
  - [Kernel Caching](#kernel-caching)
  - [CPU Time Limiting](#cpu-time-limiting)
  - [Example: Data Aggregation](#example-data-aggregation)
  - [Example: Conditional Logic](#example-conditional-logic)
  - [How WIT Generation Works](#how-wit-generation-works)
- [License](#license)

## Features

- Define RPC interfaces in Rust using `rpc!` macro
- Swap transports: WebSocket, stdio, or custom
- Swap codecs: JSON, MessagePack, or custom
- Generate OpenAPI specs and TypeScript clients
- Generate WIT definitions for WASM interop
- Built-in tracing support for all client/server calls

## Basic Usage

```rust
rpc! {
    extern "Rust" {
        fn add(a: i32, b: i32) -> i32;
    }
}

// Server
struct Service;
impl server::AsyncService for Service {
    async fn add(&self, a: i32, b: i32) -> i32 { a + b }
}

server::serve(Service, WebSocketTransport::bind("127.0.0.1:8080").await?, JsonCodec).await?;

// Client
let client = client::Client::new(
    WebSocketTransport::connect("ws://127.0.0.1:8080").await?,
    JsonCodec
);
let result = client.add(2, 3).await?;  // 5
```

## Stateful Services

Services use `&self` (not `&mut self`), enabling parallel request handling. For mutable state, use interior mutability:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

rpc! {
    extern "Rust" {
        fn increment() -> u64;
        fn get_count() -> u64;
    }
}

struct Counter {
    count: AtomicU64,
}

impl server::AsyncService for Counter {
    async fn increment(&self) -> u64 {
        self.count.fetch_add(1, Ordering::SeqCst) + 1
    }

    async fn get_count(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }
}

let service = Counter {
    count: AtomicU64::new(0),
};
server::serve(service, transport, codec).await?;
```

This pattern (similar to Axum) allows you to control locking granularity and enables concurrent request processing.

## Transport and Codec Flexibility

Swap transports and codecs:

```rust
server::serve(service, WebSocketTransport, JsonCodec).await?;
server::serve(service, WebSocketTransport, MessagePackCodec).await?;
server::serve(service, StdioTransport, JsonCodec).await?;
```

## Auto-generated Output

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
rpc-wasm-runtime = { git = "https://github.com/andrewgazelka/rpc" }  # for WASM kernels
```

Generated RPC code includes tracing spans for all client/server calls (via `rpc-core`'s re-exported `tracing`). Subscribe with `tracing-subscriber` to see structured logs with request IDs.

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

## Experimental: Server-side WASM Composition

This feature is experimental and under active development.

### Overview

Execute client logic server-side to reduce round-trips:

```rust
// Find friends-of-friends within 2 hops
// Without kernel: N round-trips (one per friend)
let user = client.get_user(id).await?;
let mut friends_of_friends = HashSet::new();
for friend_id in user.friends {
    let friend = client.get_user(friend_id).await?;
    friends_of_friends.extend(friend.friends);
}

// With kernel: 1 round-trip
// Load pre-compiled kernel (compiled separately to wasm32-wasip2)
let kernel_bytes = std::fs::read("friends_of_friends.wasm")?;

// Client sends kernel to server via execute_kernel RPC method
let friends_of_friends: HashSet<UserId> = client.execute_kernel(kernel_bytes).await?;
```

WASM kernel: 14KB, sandboxed via wasmtime. For a user with 50 friends, reduces 50 round-trips to 1. Client sends compiled WASM to server via RPC, server executes it and returns the result.

### Writing a Kernel

Kernels are separate Rust crates compiled to `wasm32-wasip2`:

```toml
# kernel/Cargo.toml
[package]
name = "friends-kernel"
edition = "2024"

[lib]
crate-type = ["cdylib"]  # Compile as WebAssembly component

[dependencies]
wit-bindgen = "0.33"

[profile.release]
opt-level = "z"      # Optimize for size
lto = true           # Link-time optimization
codegen-units = 1
strip = true
panic = "abort"
```

```rust
// kernel/src/lib.rs
wit_bindgen::generate!({
    world: "social-kernel",
    path: "../social.wit",  // Generated from your rpc! definition
});

use exports::rpc::kernel::kernel::Guest;

struct FriendsKernel;

impl Guest for FriendsKernel {
    fn run(id: UserId) -> HashSet<UserId> {
        let user = rpc::kernel::social::get_user(id);
        user.friends
            .into_iter()
            .flat_map(|fid| rpc::kernel::social::get_user(fid).friends)
            .collect()
    }
}

export!(FriendsKernel);
```

Compile and embed in your client:

```bash
cd kernel
cargo build --release --target wasm32-wasip2
```

```rust
// In your client code
const KERNEL: &[u8] = include_bytes!("../kernel/target/wasm32-wasip2/release/friends_kernel.wasm");

let friends_of_friends: HashSet<UserId> = client.execute_kernel(KERNEL).await?;
```

### Inline Kernel Compilation (Experimental)

For a more succinct workflow, you can use `include-wasm-rs` to compile kernels at build time without a separate crate:

```toml
[build-dependencies]
include-wasm-rs = "1.0"
```

```rust
// build.rs
fn main() {
    include_wasm::build_wasm("./kernels/friends.rs", "wasm32-wasip2");
}

// In your client code
const KERNEL: &[u8] = include_wasm!("friends");

let friends_of_friends: HashSet<UserId> = client.execute_kernel(KERNEL).await?;
```

This compiles the kernel as part of your main crate's build process, eliminating the need for a separate kernel crate.

### Kernel Caching

Servers cache WASM binaries to avoid re-uploading. The client library handles this automatically:

```rust
// First time: client sends full WASM binary (~14KB)
let result1 = client.execute_kernel(&kernel_bytes).await?;

// Subsequent calls: client only sends hash (~32 bytes)
let result2 = client.execute_kernel(&kernel_bytes).await?;
let result3 = client.execute_kernel(&kernel_bytes).await?;
```

The client computes a Blake3 hash of the kernel and tries hash-only execution first. If the server doesn't have it cached, the client automatically uploads the full binary.

Cache behavior:
- Server holds 100 binaries by default (configurable via LRU cache)
- Hash verification prevents tampering
- Frequently used kernels stay hot
- Client transparently handles cache misses

Network cost:
- First execution: ~14KB (full binary)
- Subsequent executions: ~32 bytes (hash only)
- After 2 executions: amortized 7KB per call
- After 10 executions: amortized 1.4KB per call
- After 100 executions: amortized 140 bytes per call

### CPU Time Limiting

Clients can optionally request execution timeouts:

```rust
use std::time::Duration;

// Client requests a 500ms timeout
let result = client.execute_kernel_with_timeout(kernel_bytes, Duration::from_millis(500)).await?;

// Client requests 10 second timeout (server may cap this to its configured maximum)
let result = client.execute_kernel_with_timeout(kernel_bytes, Duration::from_secs(10)).await?;
```

Timeout behavior:
- Server sets `max_timeout` when creating its `WasmRuntime` (e.g., 5 seconds)
- Actual timeout: `min(client_requested, server_max)`
- Prevents malicious clients from monopolizing resources
- Epoch interruption has approximately 10% overhead (vs 2-3x for fuel-based limiting)

### Example: Data Aggregation

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

### Example: Conditional Logic

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

### How WIT Generation Works

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

## License

MIT
