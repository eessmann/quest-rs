# Exact affine implementation provenance

The bounded affine engine and its regression tests were introduced as
quest-owned additions in the maintained MathCore 0.3.1 fork and extracted
into this crate in the 2026-09-29 architecture overhaul. The source baseline
is quest-rs commit `7148ccaa47e9e39506d62ba1e441d657040b83f8`, formerly
`crates/vendor/mathcore/src/exact/mod.rs` and `tests/exact_affine.rs`.

The 2026-10-05 consolidation moves the current Dashu exact engine and tests
into the maintained normal workspace dependency `quest-mathcore`, under
`crates/vendor/mathcore/src/exact`. This crate's private `affine` module is a
compatibility re-export. The original MathCore archive's MIT notice remains
in `LICENSE.mathcore` and the maintained package's `LICENSE`; its pinned
SHA-256 is `367d848a146b75a63af4fc2eafd6791327a662ccb4ff5965220fcfa7dfa1c96f`,
and upstream commit is `d13a333a6c69a231f210cdc3fb1c4843ee7bd2eb`.
The package provenance is recorded in `crates/vendor/mathcore/UPSTREAM.md`.

The independent source replay remains in this crate, separate from affine
canonicalization. Relocation does not erase original binding or finite
conversion obligations. Approximate CAS and legacy dependencies are absent.
Historical verification records remain evidence about their original trees.
