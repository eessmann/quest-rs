# quest-optimizer-worker

Optional, one-request quantum optimization candidate executable. It reads a bounded versioned JSON envelope from stdin, writes one response to stdout, and exits. Use it through `quest-optimizer-client` to enforce Linux process limits and independently admit its result. The worker does not load QuEST or allocate native quantum registers.

Neither candidate engine is enabled by default. From the workspace root:

```sh
cargo build -p quest-optimizer-worker --features synthesis,zx --locked
cargo test -p quest-optimizer-worker --features synthesis,zx --locked
```

`synthesis` enables `quest-synthesis`, the direct Rust candidate engine. Its default adapter uses 256 request-owned working bits and deterministic logical work, coefficient, allocation, and output limits. Exact dyadic, rational-pi and affine-pi targets are retained through generation and independent full-phase certification. The engine label is `quest-synthesis-ross-selinger-lll-prime-norm-v1`; typed library failures map to distinct stable process failure codes. The same engine can be called without a process on macOS and Linux.

`zx` enables QuiZX 0.3.0. The adapter admits at most four qubits and 128 input gates, uses deterministic flow simplification, checks graph and output bounds, and preserves the ordered wire interface. Unsupported controlled gates are declined. Extraction is followed by project-owned exact matrix comparison and scalar-phase recovery; upstream simplification alone never authorizes a replacement.

The request seed is echoed in the response. Synthesis uses it for reproducibility; the deterministic ZX pass accepts it without running a stochastic pass. Requests and responses are capped at 64 KiB. Disabled features return capability failures. Every candidate remains untrusted at the process boundary and is certified again by the parent client.
