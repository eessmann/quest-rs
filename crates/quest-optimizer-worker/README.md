# quest-optimizer-worker

Optional, one-request quantum optimization candidate executable. It reads a bounded versioned JSON envelope from stdin, writes one response to stdout, and exits. Use it through `quest-optimizer-client` to enforce Linux process limits and independently admit its result. The worker does not load QuEST or allocate native quantum registers.

Neither candidate engine is enabled by default. From the workspace root:

```sh
cargo build -p quest-optimizer-worker --features synthesis,zx --locked
cargo test -p quest-optimizer-worker --features synthesis,zx --locked
```

`synthesis` enables the project-hardened fork `quest-rsgridsynth` version `0.2.2-quest.1`, vendored at `crates/vendor/rsgridsynth` and imported through the `rsgridsynth` dependency alias. It is not an unmodified upstream 0.2.2 package. The fork installs working precision before constructing floating constants, accepts exact target strings, bounds grid search and output, and retains full phase. The adapter uses at least 256 and at most 1024 working bits, limits rational input coefficients to 4096 bits, handles X/Y basis changes explicitly, and obtains a project interval certificate for every candidate. The protocol engine label remains `rsgridsynth-0.2.2-quest.1`.

`zx` enables QuiZX 0.3.0. The adapter admits at most four qubits and 128 input gates, uses deterministic flow simplification, checks graph and output bounds, and preserves the ordered wire interface. Unsupported controlled gates are declined. Extraction is followed by project-owned exact matrix comparison and scalar-phase recovery; upstream simplification alone never authorizes a replacement.

The request seed is echoed in the response. Synthesis uses it for reproducibility; the deterministic ZX pass accepts it without running a stochastic pass. Requests and responses are capped at 64 KiB. Disabled features return capability failures. Every candidate remains untrusted at the process boundary and is certified again by the parent client.
