# rpc

**Decomposed, pluggable RPC framework**

Schema generation powered by [schema](https://github.com/andrewgazelka/schema) - like serde, but for structure instead of serialization.

## Why?

|  | [gRPC](https://grpc.io) | [JSON-RPC](https://www.jsonrpc.org) | [Cap'n Proto](https://capnproto.org) | [Cap'n Web](https://capnproto.org/rpc.html) | [tarpc](https://github.com/google/tarpc) | [tRPC](https://trpc.io) | [UniFFI](https://mozilla.github.io/uniffi-rs/) | **This** |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Rust source of truth** | ❌ .proto | ✅ | ❌ .capnp | ❌ JS only | ✅ | ❌ TS only | ❌ .udl | ✅ |
| **Swap transport** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| **Swap codec** | ❌ | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ | ✅ |
| **Bidirectional RPC** | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| **Streaming** | ✅ | ❌ | ✅ | ✅ | ✅ | ✅ | ❌ | 🚧 |
| **Call from web** | ❌ proxy | ✅ | ❌ | ✅ | ❌ | ✅ | ❌ | ✅ |
| **Cross-language clients** | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ TS only | ✅ | 🚧 |
| **Server-side composition** | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | 🚧 |
| **Direct FFI bindings** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ | 🚧 |

**No schemas. No boilerplate. No compromises.**

## Goals

| Goal | Status | Description |
|------|--------|-------------|
| **Rust-first** | ✅ | Rust structs and traits are the source of truth - no IDL files or code generation scripts |
| **Any serde type** | ✅ | Works with any type that implements `Serialize`/`Deserialize` |
| **Bidirectional RPC** | ✅ | Server can call client methods, not just client-to-server |
| **Streaming** | 🚧 | Support streaming requests and responses |
| **Any transport** | ✅ | WebSocket, HTTP, Stdio, in-process channels, custom transports |
| **Decoupled evolution** | 🚧 | Server changes don't require client recompilation - client and server evolve independently |
| **Schema generation** | ✅ | Generate OpenAPI specs and TypeScript clients from RPC definitions via `schema` crate |
| **Observability** | 📋 | LLM-first observability via RPC - query transaction history, inspect logs, view request/response pairs |
| **Language agnostic** | 🚧 | TypeScript clients via codegen, Python/Go/etc. planned |

### Low Priority (Needs More Thought)

| Goal | Description | Concerns |
|------|-------------|----------|
| **Server-side composition** | Execute multiple RPCs server-side via scripting/WASM to reduce round-trips | WASM bundle size, complexity, execution time limits needed |

## Usage

```rust
use schema::Schema;

// Define your API
rpc! {
    extern "Rust" {
        fn add(a: i32, b: i32) -> i32;
        fn greet(name: String) -> String;
    }
}

// Implement server
struct MyService;

impl server::Server for MyService {
    async fn add(&self, a: i32, b: i32) -> i32 { a + b }
    async fn greet(&self, name: String) -> String {
        format!("Hello, {}!", name)
    }
}

// Run (pick ANY transport + codec combo)
let listener = WebSocketListener::bind("127.0.0.1:8080").await?;
let transport = listener.accept().await?;
server::serve(MyService, transport, MessagePackCodec).await?;

// Call from client
let transport = WebSocketTransport::connect("ws://127.0.0.1:8080").await?;
let client = client::Client::new(transport, MessagePackCodec);
let result = client.add(2, 3).await?;  // => 5
```

### Generate OpenAPI & TypeScript

```rust
// Get schema from RPC definition
let schemas = client::Client::<AnyTransport, AnyCodec>::schema();

// Generate OpenAPI 3.0 spec
let openapi = generate_openapi_spec("My API", "1.0.0", schemas);

// Generate TypeScript client
let ts_client = generate_typescript_client("MyApiClient", "http://localhost:8080", schemas);
```

Output TypeScript:

```typescript
export class MyApiClient {
  async add(arg0: number, arg1: number): Promise<number> {
    return this.request<number>('add', [arg0, arg1]);
  }

  async greet(arg0: string): Promise<string> {
    return this.request<string>('greet', [arg0]);
  }
}
```

## Features

- **Modular**: Separate crates for transport, codec, core
- **Pluggable**: Mix & match any transport with any codec
- **Type-safe**: Macro generates fully-typed client/server
- **Modern**: Native async traits (Rust 1.75+)
- **Schema-aware**: Automatic OpenAPI & TypeScript generation
- **Flexible transports**: WebSocket, Stdio, in-process, HTTP (planned)

## Packages

| Crate | Purpose | Size |
|-------|---------|------|
| `rpc-core` | Traits only | < 100 LOC |
| `rpc-macro` | Code generation | ~200 LOC |
| `rpc-codec-json` | JSON codec | ~50 LOC |
| `rpc-codec-msgpack` | MessagePack codec | ~50 LOC |
| `rpc-transport-ws` | WebSocket transport | ~100 LOC |
| `rpc-transport-stdio` | Stdio transport | ~80 LOC |
| `rpc-transport-inprocess` | In-process channels | ~60 LOC |
| `rpc-openapi` | OpenAPI & TS generation | ~400 LOC |
| `rpc-server` | Runtime glue | ~10 LOC |

Total: **~1,050 LOC**

## Examples

### Swap codecs

```rust
- let codec = JsonCodec;
+ let codec = MessagePackCodec;
```

### Add transports

```rust
impl Transport for HttpTransport {
    async fn send(&mut self, msg: Message) -> Result<()> { ... }
    async fn recv(&mut self) -> Result<Message> { ... }
    async fn close(&mut self) -> Result<()> { ... }
}
```

## License

MIT
