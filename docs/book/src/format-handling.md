# Format handling and catalog provenance

Common container syntax belongs to established libraries. Application schemas,
scientific conventions, numerical validation and resource admission remain
explicit project responsibilities. This is the format audit completed for the
PennyLane migration on 2026-10-03.

| Area | Implementation and contract |
| --- | --- |
| Inverse catalog | The unchanged official PennyLane HDF5 snapshot and a Serde JSON manifest replace C++ initializer extraction and generated coefficient shards. Owned catalog loading uses high-level `hdf5-metno` readers. |
| QSP source/execution JSON | Serde transport models define scalar/complex values, fixed matrix shapes and known fields. Explicit admission rejects duplicates and competing payloads, while frozen matrices may carry source angle provenance. Polynomial metadata is retained. |
| QSP and compiler artifacts | Serde owns JSON syntax; exact integer words, canonical field order, digest binding and independent numerical validation remain unchanged. Bounded writers stop serialization before exceeding encoded-byte limits. |
| Optimizer wire | Existing Serde DTOs and bounded codecs remain; encoded overflow reports the same budget category as decoded overflow. |
| Frontend templates | Serde produces the existing versioned representation through bounded output; loaded executable graphs are verified again. |
| Native-consumer manifests | Serde and `toml` read the toolchain table and write complete Cargo and toolchain documents. JSON string escaping is not used for TOML. |
| Measurement CSV | Maintained Rust examples and benchmark fixtures use `csv::Writer`; escaping, headers and records are delegated to the crate. Measurement snapshots precede serialization. |
| Python controllers | Standard-library `json`, `csv` and `tomllib` remain. Maintained scratch-manifest writers use `tomli-w`, declared in the fixture requirements. |
| C++ reference reports | The maintained adapters use the existing Glaze JSON dependency. Their upstream revision pins, report fields and numerical execution stay explicit. |
| Rust example/benchmark JSON | Catalog validation and execution reports use Serde, retain nullable fields and numeric values, and reject nonfinite metrics. Reporting remains outside measurement boundaries. |
| Matrix/state/block HDF5 | Existing high-level dataset and attribute APIs remain, including h5py/h5pp complex layouts, sparse ordering, shape checks and finite-value admission. |
| CMake metadata and shell arguments | Existing `cmake-file-api` and `shlex` implementations remain. |

The pinned PennyLane file has 21 coefficient families and 38,124 coefficients,
with maximum degree 8105. Its SHA-256 is
`dccb518a24395d73af9ab701a922431f4600f553e904cc57507b49863a6d30a3`.
All original catalog coefficient words were checked against this source. The
reader does not infer a mathematical certificate from an epsilon label and does
not assign execution conventions to the upstream phase arrays.

Catalog reports replace C++ `source_revision` with dataset `source` metadata.
Consumers now own an `InverseCatalog`; the old static APIs and optional HDF5
feature have been removed. JSON-only IO and no-default-feature CLI builds also
link serial HDF5. Pure numerical crates retain their existing dependency boundary.
The IO crate documents offline loading, temporary storage and maintenance commands.

## Deliberate boundaries

QASM syntax, exact decimal/rational encodings, generated Rust/C++ source, Cargo
build directives, versioned binary execution payloads and the reference CASE
stream are domain formats. Substituting JSON or a generic binary codec would
change their contracts rather than remove a common-format implementation.

Recorded receipts and explicitly frozen consolidation/optimization-roadmap
fixtures remain historical evidence. Their old serialization is inventoried but
not rewritten. New timing output preserves column order, LF records, integer
widths and existing decimal precision; no historical benchmark is recalculated.

The audit also identified separate existing domain-admission limitations: C
configuration-header text scans do not evaluate conditional preprocessing; the
reference CASE parser has permissive state/trailing-token handling; and native
variable-length HDF5 string reads can allocate before the project checks the
string length. These are separate from this common-format migration. Existing
HDF5 writer publication semantics are unchanged. They should be addressed with
focused behavioral changes and regression tests, not a format-library swap.
