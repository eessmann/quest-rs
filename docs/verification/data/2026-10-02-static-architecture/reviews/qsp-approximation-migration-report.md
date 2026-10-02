# QSP approximation consolidation evidence

Removed `crates/quest-qsp/src/offline/remez.rs` (1,221 lines), its module and public exports, and `OfflineError::{ApproximationNotEstablished,Polynomial,Numerics}`. There is no legacy forwarding shim. Removed the approximation-only `Context::{exp,ln,sin,cos}` and the now-unused `number::{negative,positive}` helpers. Source arithmetic for synthesis and the independent verifier remain unchanged.

The retained QSP `Context` owns synthesis precision, the synthesis constant cache, precision-weighted work/storage accounting, coefficient/grid admission and cached complex FFT roots. Its consumers remain the original offline synthesis FFT, complex-number and factorization kernels. It no longer provides an approximation backend.

Migrated the executable tutorial, its smoke tests, the README feature/approximation text and offline module documentation to `quest_polynomial::RemezRequest`. Examples explicitly choose binary64 or MP candidate/enclosure arithmetic and request binary64 export before proof. They obtain the exact pre-certified complex64 boundary through `binary64_polynomial`; QSP parity/contractivity admission remains explicit. Uniform-error and minimax-gap certificates are described and exercised separately.

All ten original synthesis/offline integration test bodies are preserved (verified against HEAD, ignoring formatting). This includes the existing ignored degree-8105 scale test. Eight migrated approximation regression tests retain meaningful behavior from the removed engine's public and private tests:

- Work and storage rejection retains the owned original request.
- A quadratic degree-one approximation uses MP and exports certified binary64 coefficients.
- Exponential exchange distinguishes total uniform error from the minimax gap.
- Invalid derivative domains reject and unsuccessful proofs retain a finite MP candidate.
- Precision retries restart from an exact rational target; no implicit binary64 export is permitted.
- Exhausted precision schedules retain every attempt and the exact decimal target.
- MP analytic derivatives and captured binary64 constants preserve input semantics and export bits.
- The public MP QR kernel solves an independent integer-reference system.

Obsolete tests of dynamic expression DAGs, the old private byte formula and empirical exchange tolerance were replaced with bounded typed-expression work/storage tests and exact-input precision tests. No synthesis or directed precision regression was removed.

## Red/green validation

All commands ran in the project `devenv shell`.

- Before deletion: scoped offline/tutorial build failed at old `offline/remez.rs` imports of removed `quest_polynomial::{Backend,Expr}`. Log: `qsp-migration-red.log`.
- `cargo test -p quest-qsp --features offline-synthesis --lib --test offline --test tutorials`: **44 passed, 2 existing ignored** (lib24, offline17, tutorials3). Includes existing finite binary64 precision import/round-trip, subnormal/midpoint directed rounding, and offline number precision tests. Log: `qsp-migration-green.log`.
- `cargo test -p quest-qsp --no-default-features --test tutorials`: **1 passed**. Log: `qsp-migration-default.log`.
- `cargo clippy -p quest-qsp --features offline-synthesis --test offline --test tutorials --example qsp_tutorials --message-format short`: passed. Only inherited Rust nightly `generic_const_exprs`/next-solver compatibility warning remains. Log: `qsp-migration-clippy.log`.
- Scoped `rustfmt --check` and `git diff --check`: passed.
- No `OfflineRemez`, `OfflineApproximation`, `ApproximationNotEstablished`, or `RemezBuilder` references remain under `crates/quest-qsp`.
- No changed paths under `crates/quest-qsp/src/{certification,precision}`.

No commits or workspace-wide Cargo/format commands were run. The ignored scale fixture and Linux/HPC performance were not claimed as executed.
