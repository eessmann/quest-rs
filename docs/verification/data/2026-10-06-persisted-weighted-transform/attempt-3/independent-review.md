# Persisted weighted transform attempt 3: independent saved-only audit

Verdict: **approved artifact integrity and bounded P1 numerical evidence; campaign incomplete**. No builds, scientific children, MPI jobs, or source changes were performed for this audit. Subsequent source fixes and attempt 4 do not alter this verdict or attempt 3's historical evidence.

## Outcomes and scope

The fixed seven-job record contains publish at eight ranks, compile at one rank, and replay at one rank completed; replay at two, four, and eight ranks and the four-rank split job each timed out at approximately 180 seconds. All eight timeout streams are empty. Wrapper elapsed time is 722.8618813700014 seconds; driver elapsed time is 722.6245635080049 seconds. The terminal status remains `incomplete-or-accuracy-failure`.

The saved rows specify 2 GiB address-space admission, 180 seconds per child, and 4 MiB each for stdout and stderr, captured in chunks no larger than 64 KiB. All 14 streams match their recorded sizes and hashes and are below those limits. No output-limit, incomplete-capture, truncation, or driver-interruption flag is set. The outer deadline is 1500 seconds with 10 seconds forced-cleanup grace. A null global file-size-limit field means the runner did not set it; it does not prove an unlimited inherited host limit.

There is no progress trace locating a timed-out job inside execution. The separate source diagnosis `persisted-weighted-readout-tag-diagnosis.md` establishes mismatched partner tags in the historical readout call if reached. It does not establish that readout was the observed stopping stage. No multi-rank inverse accuracy, scaling, or multi-host acceptance follows from attempt 3.

## Historical provenance

All four saved complete build/execution context snapshots are byte-identical, SHA-256 `525bcb4c3c8e3bee35ada4415f4a1764029c180dd723a8810508b8d68f452621`. The source map contains 975 paths, with canonical path/hash digest `32b45c0812f6be968872c2338aeebae50ceed04108da99f63f82fa9616eea95f`. Actual compiler artifacts show 16 local packages, successful Cargo completion, and a debug profile (`opt_level=0`, debug assertions and overflow checks enabled). This is not a release-profile performance result.

Both preserved executable copies hash to `b0be8c8f588370348b8ec45e61da09c6b53b6de9186ff1ed60b52a1d67712b24`. The historical 16-file consumer manifest hashes to `06feb45c9e1a56e5e04ccfdacd91f20a4e23bf90a8c54c7e6b11f722c7f7e320`. Its method document lies outside the compiler-source scanner; it is bound through the consumer manifest and runner source checks, rather than the 975-file compiler-source map. The audit does not require the subsequently edited current tree to match historical sources.

All 29 immutable dataset files (three manifests, 24 HDF5 buckets, freeze record, and phase record), ten rank receipts, and 14 captured streams were independently hashed and bound to the saved receipt. Rank receipts were also checked against stdout receipt indices and consolidated rows. HDF5 contents were integrity-checked as complete files; this audit does not independently reconstruct every stored matrix coefficient. External registry packages and every dynamic dependency/environment value are not completely pinned, so this is not a hermetic build claim.

All three v2 matching manifests have distinct source identities and distinct record digests. The persisted version is 2 with construction `completed-matching-columns-v2`; old v1 artifacts were not silently reinterpreted.

## P1 numerical and phase evidence

All three fresh-RHS calls visit all 32 logical coordinates. Forward, adjoint, and repeated forward residuals are respectively 2.962203156220422e-5, 2.962203156364751e-5, and 2.962203156220422e-5. Relative vector errors agree at roundoff. Total probability differs from one by at most 1.02e-14. These satisfy the fixed residual 1e-3, vector 2e-3, and probability 1e-10 criteria. Both forward readout payloads are exactly equal.

The saved degree-33, 34-angle symmetric Wx sequence and coefficient record share the declared phase/file identities. Independently evaluating the Chebyshev coefficients at x=0.5099019513592784 gives 0.15313643661118856; direct two-by-two Wx multiplication gives imaginary U00=0.1531364366111887, a difference of 1.3877787807814457e-16. Imaginary U00 is the documented symmetric-Wx response, not real U00. The corresponding relative scalar inverse error is 2.9622031557208217e-5, consistent with the saved residuals. Saved success probabilities agree with the finite-polynomial prediction to at most 3.16e-16. This is independent scalar consistency evidence, not a uniform implemented-source/PREP/native error certificate; the saved certificate is null.

Each application reports 66 source queries, 198 child events, 1584 batches/indexed reads/indexed writes, 101376 local pair candidates, and zero inter-rank routing bytes at P1. Per-call work is 981729024; the three-call cumulative total is 2945187072, within the declared 24-billion ceiling. The reported managed rank peak is 2514056 bytes. Native P1 RSS/high-water is 311513088 bytes and address space is 1086644224 bytes; these are process observations, distinct from managed state memory. No analogous endpoint receipt exists for the timeout processes. The Python runner did not independently measure their RSS.

## Reproduction and audit artifacts

Saved-only command: `python3 <private-artifacts>/quest-persisted-attempt3-independent-audit.py` (exit 0). The script imports only a hash-checked historical-compatible parser and calls saved-row validation; it does not call the campaign driver or spawn scientific jobs.

- Audit script SHA-256: `f9520ad81dd1bbba3e4dd6882915c15210b40e33c3458525ab8d8a3ff7376cfa`.
- Passing log `<private-artifacts>/quest-persisted-attempt3-independent-audit.log`, SHA-256 `5f36d2b58ee77fee355357bae88d5fa7a16c27d1c69df4a52de90023ecd0946d`.
- Detailed audit `persisted-weighted-attempt3-independent-audit.json`, SHA-256 `4c17b2c1586a67aa9bf4311254ee9c1177a17a61bb6edea6ad4ae47b6811e876`.
- Original campaign receipt SHA-256: `f6a2edd59755625155b401e3de859c838bbe85be836b31c431ba527e4e26cfca`.
- Original execution attestation SHA-256: `a472fb29272747e9ff711ace8fa14dfb59a193ba12c5e70dc26322377fda44cb`.

Two audit-script development failures are preserved separately: `...-initial-schema-error.log` records the initially incorrect assumption that the method document belonged to the compiler-source map; `...-phase-component-error.log` records the initially incorrect real-versus-imaginary symmetric-Wx comparison. These are audit tooling errors, not additional campaign attempts or physical outcomes. Both were corrected before the final saved-only audit passed. No original campaign bytes were changed. No public artifacts were published by this reviewer.
