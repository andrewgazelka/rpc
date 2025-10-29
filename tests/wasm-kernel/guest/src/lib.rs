// Generate bindings from WIT
wit_bindgen::generate!({
    world: "counter-kernel",
    path: "../counter.wit",
});

// Import the exported kernel interface
use exports::rpc::kernel::kernel::Guest;

struct CounterKernel;

impl Guest for CounterKernel {
    /// Run the kernel: call increment 10 times starting from 0
    fn run() -> u32 {
        let mut value = 0;

        // Call increment 10 times using the imported counter interface
        for _ in 0..10 {
            value = rpc::kernel::counter::increment(value);
        }

        value
    }
}

// Export the implementation
export!(CounterKernel);
