# Dashu-only arbitrary-precision consolidation

Implement the user-approved Dashu-only plan. Preserve the existing static numerical architecture and replace the partial Rug migration without resetting prior work. All project-owned arbitrary-precision values use native Dashu types. No Malachite implementation or backend-selection machinery. Mathematical guarantees remain mandatory; exact-angle semantics and the independent QSP verifier remain distinct.

## Tasks

1. Verify and pin the published Dashu components; preserve the pre-port evidence baseline.
2. Port numerics and polynomial integration: native binary values, explicit directed contexts, fallible arithmetic, exact imports, exact dyadic binary64 exports, guard-bit and storage accounting, exact-rational test oracle.
3. Port QSP generation and independent verification, preserving inverse NLFT defaults, bounded retries, full responses and precision/artifact admission including 65 bits.
4. Port exact language, symbolic, math and synthesis arithmetic to IBig/UBig/RBig, preserving signed division, canonical rational identity, ring and norm contracts.
5. Migrate compiler and worker consumers and deliberate decimal wire encodings; increment affected protocol/structured artifacts from 2 to 3 without legacy decoders.
6. Remove obsolete dependencies, shims, abandoned port code and active benchmark dependencies. Retain historical results with original provenance and optional upstream-private pure-Rust num dependencies.
7. Validate primitive inclusion and interchange boundaries, full solvers, exact replay, all supported workspace configurations and native consumers. Benchmark matched accuracy and record toolchain, storage, allocation, runtime and build evidence. Review numerical and architectural changes independently.

## Delivery

Use the project-local nightly environment and preserve uncommitted existing work. The C++ checkout and Zotero remain read-only. Any commits are unsigned; no push. Pure Rust is required even if measured performance is lower. Linux/MPI/accelerator evidence unavailable locally remains explicitly pending.
