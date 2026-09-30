# Exact affine implementation provenance

The bounded affine engine and its regression tests were introduced as
quest-owned additions in the maintained mathcore 0.3.1 fork and extracted into
this crate in the 2026-09-29 architecture overhaul. The source baseline is
quest-rs commit 7148ccaa47e9e39506d62ba1e441d657040b83f8, formerly
`crates/vendor/mathcore/src/exact/mod.rs` and `tests/exact_affine.rs`.

The original mathcore archive was MIT licensed (notice retained in
LICENSE.mathcore), SHA-256
367d848a146b75a63af4fc2eafd6791327a662ccb4ff5965220fcfa7dfa1c96f,
upstream commit d13a333a6c69a231f210cdc3fb1c4843ee7bd2eb. Its approximate CAS,
parsers, evaluators and legacy dependency graph are not part of this crate.

The independent source replay remains separate from affine canonicalization.
Extraction does not erase original binding or finite-conversion obligations.
Historical verification records describe the original checkout and remain
historical evidence rather than instructions for the current architecture.
