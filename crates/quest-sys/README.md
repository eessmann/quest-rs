# quest-rs-sys

Low-level Rust bindings to the [QuEST](https://github.com/QuEST-Kit/QuEST) (Quantum Exact Simulation Toolkit) C++ library.

These bindings are intended to be a thin wrapper around the C++ API, with minimal abstractions. For a more idiomatic Rust interface, see the `quest-rs` crate.

## Prerequisites

This crate requires a prebuilt QuEST installation. You must install QuEST on your system before using this crate.

### Installing QuEST

1. Clone and build the QuEST repository:

```bash
git clone https://github.com/QuEST-Kit/QuEST.git
cd QuEST
mkdir build && cd build
cmake -DCMAKE_INSTALL_PREFIX=/path/to/install/quest ..
cmake --build . --target install
```

For more details, see the [QuEST documentation](https://github.com/QuEST-Kit/QuEST#readme).

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
quest-rs-sys = "0.1.0"
```

### Finding the QuEST Installation

The build system will try to find QuEST in the following order:

1. Using `QUEST_DIR` or `QUEST_ROOT` environment variables
2. Using `CMAKE_PREFIX_PATH` environment variable
3. Using pkg-config
4. In standard system directories

If QuEST cannot be found, the build will fail with an error message.

Example with environment variables:

```bash
export QUEST_DIR=/path/to/quest/installation
cargo build
```

### Feature Flags

- `openmp` - Enable OpenMP multithreading
- `mpi` - Enable MPI distributed computing
- `cuda` - Enable CUDA GPU acceleration
- `cuquantum` - Enable NVIDIA cuQuantum library (requires `cuda`)
- `hip` - Enable AMD HIP GPU acceleration
- `build-from-source` - Build QuEST from source (not recommended; prefer system installation)

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

## Build Requirements

- A C++20 compatible compiler
- CMake 3.15+
- Preinstalled QuEST library

Additional requirements based on features:
- OpenMP development libraries for the `openmp` feature
- MPI development libraries for the `mpi` feature
- CUDA toolkit for the `cuda` feature
- cuQuantum for the `cuquantum` feature
- AMD ROCm for the `hip` feature

## Troubleshooting

If you encounter build errors:

1. Make sure QuEST is properly installed
2. Set `QUEST_DIR` or `QUEST_ROOT` to point to your QuEST installation
3. For additional debug information, set the environment variable `QUEST_DEBUG=1`

## License

This crate is licensed under the MIT License. Note that QuEST itself has its own license.

## Acknowledgements

This crate is a thin wrapper around the QuEST library, which is developed by the [QuEST team](https://github.com/QuEST-Kit/QuEST).