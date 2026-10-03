# quest-qsvt-io

QSP sequence and compiled-artifact JSON, inverse catalogs and serial HDF5 scientific interchange.

See the workspace mdBook guide and crate rustdoc for executable examples.

This Rust port draws on `quest-qsvt` revision `7fe7f740579b03c52a8cf48be6a31268b029c19f`. Its MIT notice is retained in `LICENSE-quest-qsvt`.

## PennyLane inverse catalog

`InverseCatalog::bundled(IoPolicy)` returns an owned, fallible catalog decoded
from the unchanged [PennyLane inverse dataset](https://pennylane.ai/datasets/inverse).
Use `families()`, `find(kappa, epsilon)`, and `source()` in place of the former
static lookup functions. `CatalogFamily::polynomial(policy)` materializes bounded
Chebyshev coefficients, and `coefficients()` exposes their exact binary64 values.
The current 21 families retain the original coefficient bits through degree 8105.

Serial HDF5 is a required dependency. Loading checks the manifest digest and
schema, uses a secure temporary file, then closes and removes that file before
returning owned coefficients. It works offline without a repository or installed
data directory; each explicit load requires writable temporary storage. The
caller owns reuse. `InverseCatalog::open(path, source, policy)` snapshots and
validates an external file against an explicit manifest using the same reader.
`max_bytes` bounds source size and aggregate decoded storage independently;
`max_coefficients` bounds each family and the number of families. These model
Rust payload storage, not all native HDF5 allocator overhead.

The dataset is by Guillermo Alonso and licensed CC BY-SA 4.0; its attribution
and license are packaged in `data/pennylane`. Rust source remains MIT. Upstream
angle arrays are preserved in the original HDF5 file but are not interpreted.
Dataset labels and hashes do not establish numerical certificates.

Maintenance is explicit:

```sh
cargo run -p xtask -- check-qsvt-catalog  # offline digest and schema validation
cargo run -p xtask -- fetch-qsvt-catalog  # fetch the manifest-pinned bytes
```

A snapshot update requires a reviewed manifest checksum and provenance update.
The fetch command validates the full candidate before replacing a valid file;
normal builds and runtime catalog loading never download data. Historical C++
phase fixtures retain their original provenance.

## Compiled QSP artifacts

With `certification`, `read_compiled_qsp_json` validates versioned exact-bit QSP
artifacts and independently reconstructs their saved payload under caller-owned
verification limits. `read_qsp_json` recognizes this format separately from raw
source/sequence input and uses the default verification policy;
`read_qsp_json_with_tolerance` makes its tolerance explicit. Unsupported builds
return a feature diagnostic and never downgrade a compiled artifact to raw input.

`QspInput::Compiled` owns immutable original artifact bytes and fresh numerical
evidence. Execution-wire serialization preserves those bytes; workers independently
validate and recertify them without resynthesis. Historical receipts and file
hashes alone do not establish accuracy. `PolynomialConversion` likewise retains
its original admitted source and exposes immutable polynomial/bound getters.
