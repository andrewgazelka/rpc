# RPC Framework - Architecture Documentation

## Overview

This is a highly modular, decomposed RPC framework for Rust that separates concerns into pluggable components:

- **Transport layer**: How messages are sent (WebSocket, HTTP, TCP, etc.)
- **Codec layer**: How data is serialized (JSON, MessagePack, Protobuf, etc.)
- **RPC layer**: Type-safe method dispatch via procedural macros

## Design Philosophy

### 1. Separation of Concerns

Each crate has a single responsibility:

- `rpc-core`: Core traits (`Transport`, `Codec`) with zero dependencies on specific implementations
- `rpc-codec-*`: Individual codec implementations
- `rpc-transport-*`: Individual transport implementations
- `rpc-macro`: Code generation for type-safe RPC
- `rpc-server`: Thin glue layer

### 2. Pluggability

Any codec can work with any transport:

```rust
// Mix and match:
serve(service, WebSocketTransport, JsonCodec)
serve(service, WebSocketTransport, MessagePackCodec)
serve(service, HttpTransport, JsonCodec)
serve(service, HttpTransport, MessagePackCodec)
```

### 3. Type Safety

The `rpc!` macro generates:
- Fully typed client with async methods
- Server trait to implement
- Dispatch logic matching requests to methods

No string-based method lookups at the user level.

## Architecture

### Core Abstractions

#### Transport Trait

```rust
pub trait Transport: Send {
    async fn send(&mut self, msg: Message) -> Result<()>;
    async fn recv(&mut self) -> Result<Message>;
    async fn close(&mut self) -> Result<()>;
}
```

Responsibilities:
- Send/receive opaque `Message` (just `Vec<u8>`)
- No knowledge of serialization format
- Handle connection lifecycle

#### Codec Trait

```rust
pub trait Codec: Send + Sync {
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>>;
    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T>;
}
```

Responsibilities:
- Serialize/deserialize using serde
- No knowledge of transport mechanism
- Stateless (can be cloned/shared)

### Macro-Generated Code

The `rpc!` macro generates two modules:

#### Client Module

```rust
pub mod client {
    pub struct Client<T, C> {
        transport: Arc<Mutex<T>>,
        codec: Arc<C>,
        next_id: Arc<AtomicU64>,
    }

    impl<T, C> Client<T, C> {
        pub async fn method_name(&self, args...) -> Result<ReturnType> {
            // 1. Encode request
            // 2. Send via transport
            // 3. Receive response
            // 4. Decode and return
        }
    }
}
```

#### Server Module

```rust
pub mod server {
    pub trait Server: Send + Sync {
        async fn method_name(&self, args...) -> ReturnType;
    }

    pub async fn serve<S, T, C>(
        server: S,
        transport: T,
        codec: C,
    ) -> Result<()> {
        loop {
            // 1. Receive message
            // 2. Decode RpcRequest
            // 3. Match on method name
            // 4. Call server trait method
            // 5. Encode response
            // 6. Send back
        }
    }
}
```

## Request/Response Protocol

### Request Structure

```rust
pub struct RpcRequest {
    pub id: u64,           // For matching responses
    pub method: String,    // Method name
    pub params: Vec<u8>,   // Serialized parameters
}
```

### Response Structure

```rust
pub struct RpcResponse {
    pub id: u64,              // Matches request ID
    pub result: ResponseResult,
}

pub enum ResponseResult {
    Ok(Vec<u8>),    // Success with serialized result
    Err(String),    // Error message
}
```

## Adding New Components

### Adding a New Codec

1. Create `rpc-codec-foo` crate
2. Implement `Codec` trait:

```rust
pub struct FooCodec;

impl Codec for FooCodec {
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        // Your serialization logic
    }

    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T> {
        // Your deserialization logic
    }
}
```

3. Add tests
4. Done! Works with all transports

### Adding a New Transport

1. Create `rpc-transport-foo` crate
2. Implement `Transport` trait:

```rust
pub struct FooTransport { /* ... */ }

impl Transport for FooTransport {
    async fn send(&mut self, msg: Message) -> Result<()> {
        // Your send logic
    }

    async fn recv(&mut self) -> Result<Message> {
        // Your receive logic
    }

    async fn close(&mut self) -> Result<()> {
        // Cleanup
    }
}
```

3. Add tests
4. Done! Works with all codecs

## Testing Strategy

### Unit Tests
- Each codec tests encode/decode roundtrips
- Transport tests message send/receive
- Core tests trait implementations

### Integration Tests
- `rpc-server` crate contains end-to-end tests
- Test different codec/transport combinations
- Verify type safety and error handling

### Running Tests

```bash
cargo nextest run
```

Uses [cargo-nextest](https://nexte.st/) for faster, cleaner test output.

## Performance Considerations

### Zero-Copy Where Possible
- `Message` wraps `Vec<u8>` (no copies between layers)
- Codecs use direct serde serialization
- Transport implementations can use `bytes::Bytes` internally

### Async Throughout
- All I/O is async (tokio-based)
- Client uses `Arc<Mutex<T>>` for shared transport
- Server processes requests sequentially (can be parallelized)

### Request IDs
- Client uses atomic counter for request IDs
- Enables future request/response matching
- Room for concurrent requests (not yet implemented)

## Future Enhancements

### Streaming
- Add `fn method() -> impl Stream<Item = T>` support
- Server sends multiple responses for one request
- Client returns `impl Stream`

### Bidirectional Calls
- Server can call client methods
- Useful for pub/sub, notifications

### Concurrent Requests
- Client can send multiple requests without waiting
- Match responses via request ID

### More Transports
- `rpc-transport-http`: HTTP/REST transport
- `rpc-transport-tcp`: Raw TCP transport
- `rpc-transport-ipc`: Unix domain sockets

### More Codecs
- `rpc-codec-protobuf`: Protocol Buffers
- `rpc-codec-bincode`: Bincode
- `rpc-codec-cbor`: CBOR

## Crate Dependency Graph

```
rpc-server
├── rpc-core
├── rpc-macro
└── (test deps)
    ├── rpc-codec-json
    ├── rpc-codec-msgpack
    └── rpc-transport-ws

rpc-transport-ws
└── rpc-core

rpc-codec-json
└── rpc-core

rpc-codec-msgpack
└── rpc-core

rpc-macro
├── syn
├── quote
└── proc-macro2

rpc-core
├── thiserror
├── serde
└── futures
```

## Key Files

- `crates/rpc-core/src/lib.rs`: Core traits and types
- `crates/rpc-macro/src/lib.rs`: Procedural macro implementation
- `crates/rpc-server/src/lib.rs`: Integration tests and examples
- `Cargo.toml`: Workspace configuration

## Design Decisions

### Why Separate Codec Crates?

- **Granular dependencies**: Users only pay for what they use
- **Independent evolution**: Update JSON codec without touching MessagePack
- **Clear boundaries**: Each codec is self-contained

### Why Procedural Macro?

- **Type safety**: Generate code based on function signatures
- **Ergonomics**: Single source of truth for API definition
- **Compile-time errors**: Catch mistakes early

### Why Native Async Traits?

- **Modern Rust**: Uses Rust 1.75+ native async trait methods
- **Zero dependencies**: No macro overhead for async traits
- **Simplicity**: Clean trait definitions without attribute macros

### vs Alternatives

- **tonic**: Great for gRPC, but coupled to Protobuf
- **tarpc**: Excellent framework, but less modular
- **jsonrpc**: JSON-specific, can't swap codecs

This framework prioritizes **modularity and pluggability** over feature completeness.

## License

MIT
