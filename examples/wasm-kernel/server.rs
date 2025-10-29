use rpc_macro::rpc;

rpc! {
    extern "Rust" {
        fn increment(value: u32) -> u32;
        fn get_value() -> u32;
    }
}

struct CounterService {
    value: u32,
}

impl server::AsyncService for CounterService {
    async fn increment(&mut self, value: u32) -> u32 {
        self.value = value + 1;
        self.value
    }

    async fn get_value(&mut self) -> u32 {
        self.value
    }
}

#[tokio::main]
async fn main() -> rpc_core::Result<()> {
    // Print WIT schema
    let wit = client::Client::<rpc_transport_stdio::StdioTransport, rpc_codec_json::JsonCodec>::wit_schema("counter");
    println!("Generated WIT schema:");
    println!("{}", wit);
    println!();

    let service = CounterService { value: 0 };
    let transport = rpc_transport_stdio::StdioTransport::new();
    let codec = rpc_codec_json::JsonCodec;

    println!("Counter service starting...");
    server::serve(service, transport, codec).await
}
