# Optimization roadmap migration

This document records the implemented interfaces and migration requirements.
Final integration validation and measurements are recorded separately in the
[validation results](2026-09-28-optimization-roadmap-results.md).

## Exact symbolic angles

`Angle` remains the public circuit type. Its exact representation now delegates
to the MIT `quest-mathcore` fork through the required `quest-symbolic` crate.
There is no Symbolica dependency. The exact-only dependency selection excludes
mathcore's legacy floating CAS. Fork provenance and patch boundaries are in
[`UPSTREAM.md`](../../crates/vendor/mathcore/UPSTREAM.md).

Exact parameter construction and negation are now fallible, because they can
exceed explicit symbolic resource limits. Propagate their errors:

```rust
use quest_circuit::{Angle, Gate, ProgramBuilder};

let mut circuit = ProgramBuilder::new(1, 0)?;
let parameter = circuit.parameter("theta")?;
let angle = Angle::parameter(parameter)?;
circuit.gate(Gate::Rx(angle.negated()?), &[circuit.qubit(0)?], &[])?;
let plan = circuit.finish()?.bind(&[(parameter, 0.25)])?.plan()?;
# Ok::<(), quest_circuit::Error>(())
```

`Gate::adjoint()` similarly returns `Result<Gate>`. `Angle::added`,
`Angle::scaled_ratio` and `Angle::affine` provide checked exact affine operations.
Exact rational radians, rational multiples of pi and affine combinations remain
distinct from opaque `Angle::radians(f64)` leaves. The OpenQASM evaluator keeps
its existing floating and modular-angle rules.

Normalization does not erase input obligations. A cancelled parameter still
belongs to its original program and requires a finite binding. Composing two
angles retains each source's conversion requirements; constructing a single
affine target admits the combined rational-plus-pi expression. Rewrites cannot
make a previously failing input bind successfully by deleting an overflowing
source conversion.

Bound instructions retain immutable exact angle target sidecars. Native gates
still receive binary64 radians; independent synthesis certificates refer to the
original dyadic, rational-pi or affine-pi target. Numerical execution error is
not included in a mathematical synthesis bound.

## Native deployment and costs

`Register::deployment()` and `CollectiveRegister::deployment()` expose immutable
metadata sampled from the allocated native register. Use
`deployment.compiler_snapshot()` to construct the compiler target. Environment
GPU capability alone does not identify a register's actual execution mode.

Native V1 uses the specified dimensionless rational score and defaults to one
expected execution. Its preparation component counts logical forward/adjoint
matrix payload bytes, excluding native padding, device mirrors and allocator
overhead. These forecasts predict preferences; native timings are measured
separately. Communication left unknown by the model prevents comparisons that
change its ordered semantic component.

The Clifford+T objective has its own lexicographic ordering. Local and global
approximation modes are explicit; required-global composition rejects inputs
whose oracle, numerical or control-flow evidence is insufficient. Existing
individual optimization-pass APIs remain available.

## Consuming optimization and analysis

`Optimizer::from_ideal(program, bindings, options)`, `from_bound`, and
`from_verified_structured` admit their inputs before bounded search. The ideal
entrypoint keeps the original ideal program as the binding-domain authority;
the published bound program carries the optimized execution. Rebinding that
original source repeats its original finite-conversion checks. See the runnable
[`optimize` example](../../crates/quest/examples/optimize.rs) for an actual
register-derived target.

`BoundProgram::commutation_schedule` produces a proposal tied to its immutable
snapshot. `schedule_and_fuse` uses identical terminal fusion policy for baseline
and candidate, preserves mandatory paths, and publishes only a strict Native V1
improvement. Rounding changes remain explicit. `VerifiedStructuredProgram::fuse_terminal`
publishes newly verified SSA and synthetic oracle payloads together. User-gate
bodies remain symbolic for inverse/negative-power calls; dynamic operands, traps,
calls and effects delimit eligible windows. Required-global mode skips numerical
fusion.

Structured `Optimizer::search` also runs bounded value-flow analysis and exact
SSA cleanup before terminal fusion. It compares the original and candidate
under the same terminal policy, separately for every changed block, without
guessing branch frequencies. Dynamic/opaque block costs and unknown MPI traffic
remain conservative boundaries. `search_report().structured()` exposes selected
rewrite provenance, analysis usage, terminal rounding and skipped stages.
Structured worker beam search has no adapter yet; that omission is explicit in
the report. Worker-enabled search is available for ideal and bound finite
circuits through `search_with_workers`.

`language::ssa::QuantumFlow::analyze` returns a bounded derived view rather than
replacing executable SSA. Version and block handles belong to one snapshot;
alias queries also require the queried places to belong to that block's region.
Cross-region references and dynamic indices remain conservative. Every control
and target consumes and produces a quantum value version; multiwire operations
remain coupled.

`resynthesize_linear_candidate`, `parity_candidate_from` and the ZX candidate
APIs can return a longer exact proposal for combined search. Existing individual
optimization APIs keep their shortening rules. Binding-specific affine parity
proposals use `parity_bound_candidate_from` and require the original ideal/bound
pair; new overflow retains the original window.

## Optional worker protocol

Worker protocol version 2 requires matching rebuilt clients and executables.
The `mitm` feature is optional, alongside `synthesis` and `zx`. The parent always
reconstructs and verifies worker output. MITM matrix keys retain full scalar
phase; no phase quotient is used. Approximate targets retain their original
binary64, rational-pi or affine-pi identity. Incomplete, exhausted, unresolved and
no-candidate responses are distinct; a candidate response alone is not proof.

Build the optional engines with `cargo build -p quest-optimizer-worker
--all-features --release`. Run the [`optimize_workers`
example](../../crates/quest/examples/optimize_workers.rs) with `cargo run -p
quest-rs --features workers --example optimize_workers --
/absolute/path/to/quest-optimizer-worker`. It obtains deployment from its native
register, bounds worker admissions and prints certificates and completion status.

The ZX adapter exposes baseline and expanded proposals separately for combined
search, while retaining its previous standalone best-of-two selection. Its
additional graph-rule work limit does not claim to count internal operations of
upstream simplification/extraction; those paths also have process time/memory,
graph-size and output-size bounds.
