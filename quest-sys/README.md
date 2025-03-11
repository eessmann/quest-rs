# quest-rs-sys

Low-level Rust bindings to the [QuEST](https://github.com/QuEST-Kit/QuEST) (Quantum Exact Simulation Toolkit) C++ library.

These bindings are intended to be a thin wrapper around the C++ API, with minimal abstractions. For a more idiomatic Rust interface, see the `quest-rs` crate.

## Features

- Access to the core QuEST functionality
- Bindings generated using the `cxx` crate
- Support for various QuEST build configurations through Cargo features

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
quest-rs-sys = "0.1.0"
```

### Feature Flags

- `openmp` - Enable OpenMP multithreading
- `mpi` - Enable MPI distributed computing
- `cuda` - Enable CUDA GPU acceleration
- `cuquantum` - Enable NVIDIA cuQuantum library (requires `cuda`)
- `hip` - Enable AMD HIP GPU acceleration
- `force-source-build` - Always build QuEST from source instead of using system-installed version

### System-installed QuEST

By default, the crate will first try to find an existing QuEST installation on your system:

1. Using a custom path specified in `QUEST_DIR` or `QUEST_ROOT` environment variables
2. Using pkg-config
3. Checking common system library locations

If QuEST is not found, the crate will automatically download and build QuEST from source.

You can force building from source by enabling the `force-source-build` feature.

## Example

```rust
use quest_rs_sys::safe::{self, create_qureg, init_zero_state, apply_hadamard, measure_qubit};
use std::pin::Pin;

fn main() {
    // Initialize QuEST environment
    safe::init_quest_env();
    
    // Create a 2-qubit register
    let mut qureg = create_qureg(2);
    let qureg_pin = Pin::new(qureg.as_mut().unwrap());
    
    // Initialize to |00⟩
    init_zero_state(qureg_pin);
    
    // Apply Hadamard to first qubit to get (|00⟩ + |10⟩)/√2
    apply_hadamard(qureg_pin, 0);
    
    // Measure first qubit (should get 0 or 1 with 50% probability)
    let result = measure_qubit(qureg_pin, 0);
    println!("Measurement result: {}", result);
    
    // Clean up
    safe::finalize_quest_env();
}
```

## Building

The crate uses CMake to build the QuEST library, so you'll need:

- A C++20 compatible compiler
- CMake 3.21+

Additional requirements based on features:
- OpenMP for the `openmp` feature
- MPI for the `mpi` feature
- CUDA for the `cuda` feature
- cuQuantum for the `cuquantum` feature
- AMD ROCm for the `hip` feature

## License
- MIT license

## Acknowledgements

This crate is a thin wrapper around the QuEST library, which is developed by the [QuEST team](https://github.com/QuEST-Kit/QuEST).