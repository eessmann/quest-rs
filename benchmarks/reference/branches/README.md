# Pinned public QSP branch references

These adapters use public APIs from immutable main and develop archives. They
are separate from the older `7fe7f740` historical adapters in the parent folder.
CMake checks each archive's SHA256, extracts a fresh copy inside the build
folder, and compiles that exact copy. It never compiles the dirty C++ checkout.

- main `a932e7e081ac3766cad19ad6f8f4b920c8fa7fcf`: public
  `NLFTSolver::find_qsp_angles` and `find_gqsp_control_gates`.
- develop `4fc35983138d07a990862a4d83ad16f2b737c98f`: public
  `make_polynomial_phase_request`, `make_polynomial_control_request`, and
  `run_qsp(SequencedExecutor, request)`.

`capabilities.json` records source/build provenance, observed capabilities and
certificate scope. `data/` contains actual binary64 source coefficients,
exported angles/complete controls including terminal K, normalization, and
reference certificate metadata. The Rust comparison reconstructs all four
complex matrix entries at 129 domain points, with no global-phase alignment,
and checks responses against the original targets. It independently certifies
both Rust RHW and inverse-NLFT exports and reports their full-entry bounds.
The sampled cross-language difference is not a uniform error certificate.

main returned numerical failures for the two constant fixtures; this remains
visible as `UNAVAILABLE`, rather than deleting fixtures or inventing outputs.
develop passed all seven fixtures. Both branches accepted the explicit tiny
imaginary/mixed-parity projection probe, while Rust rejects that source exactly.
This admission difference is intentional. main exposes no independent export
certificate. develop exposes an exported-phase certificate for Wx and a
**pre-lowering** reflection certificate for generalized controls; the adapter
never relabels the latter as a certificate for the emitted controls. Rust retains
its stronger independent checks of the actual frozen controls.

## Reproduce on the recorded macOS environment

Use the pinned develop archive's `devenv shell` environment (Clang 22.1.8,
libc++, Boost 1.90 and HPX 1.11). From that archive directory, configure a fresh
build for each branch. Replace `/path/to/quest-rs` with this Rust checkout:

```sh
devenv shell -- cmake -S /path/to/quest-rs/benchmarks/reference/branches \
  -B /tmp/qsp-reference-main -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DREFERENCE_BRANCH=main \
  -DREFERENCE_SOURCE=/tmp/quest-architecture-reference-20260929/main
devenv shell -- cmake --build /tmp/qsp-reference-main --target branch_qsp -j2

devenv shell -- cmake -S /path/to/quest-rs/benchmarks/reference/branches \
  -B /tmp/qsp-reference-develop -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DREFERENCE_BRANCH=develop \
  -DREFERENCE_SOURCE=/tmp/quest-architecture-reference-20260929/develop
devenv shell -- cmake --build /tmp/qsp-reference-develop --target branch_qsp -j2
```

The supplied `REFERENCE_SOURCE` locates the sibling hash-checked `main.tar` or
`develop.tar`; only the fresh verified extraction is compiled. The main archive
requests legacy separate Boost header-only package configs. This adapter maps
those package targets to the environment's `Boost::headers`; it does not alter
reference implementation files. Upstream CMake emits a harmless missing-.git
provenance diagnostic for archives and h5pp policy warnings. Revision identity
comes from the verified archive, not that empty build-tree Git string.

From this Rust checkout:

```sh
/tmp/qsp-reference-main/branch_qsp > /tmp/main-qsp.txt
/tmp/qsp-reference-develop/branch_qsp > /tmp/develop-qsp.txt
devenv shell -- cargo run -p quest-qsp --features certification \
  --example compare_cpp_branches -- /tmp/main-qsp.txt
devenv shell -- cargo run -p quest-qsp --features certification \
  --example compare_cpp_branches -- /tmp/develop-qsp.txt
```

This is local CPU/sequential numerical evidence. It does not establish native
QSVT execution, distributed/GPU behavior, Linux acceptance, or performance parity.
