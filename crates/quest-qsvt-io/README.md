# quest-qsvt-io

QSP sequence and compiled-artifact JSON, inverse catalogs and optional serial HDF5 scientific interchange.

See the workspace mdBook guide and crate rustdoc for executable examples.

This Rust port draws on `quest-qsvt` revision `7fe7f740579b03c52a8cf48be6a31268b029c19f`. Its MIT notice is retained in `LICENSE-quest-qsvt`.

The inverse catalog was refreshed against revision
`568725f2bd488a03a4f98cdf92de924f17b2834a`; all 21 coefficient payloads are
byte-identical to the original port. Historical fixtures retain their original provenance.


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
