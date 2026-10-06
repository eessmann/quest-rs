# Prepared weighted matching on CPU/MPI

The owning runtime now composes complete prepared matching unitaries on one
CPU state-vector register. `Environment::prepare_matching_lcu` and the collective
method of the same name consume ordered `(Complex64, PreparedMatching)` children
plus a physical selector mapping and `MatchingLcuLimits`. Each child must have
the same environment, register width and operand mapping. A bounded, replicated
`LcuPlan` owns selector PREP, complex branch phases and immutable selection
metadata. Each selected branch invokes the child's complete U, including its
failure, color and padded sectors. The adjoint applies the standalone complete
U†. Signed outer controls and spectator qubits are preserved.

This is an ownership-reuse constructor, not a raw-shard pipeline constructor.
Source generation, distributed preprocessing, persistence/loading and preparation
of each child have already occurred under their own budgets. Composition limits
admit the accessible input capacities, selector compilation and current live
owners; they do not retrospectively admit or time earlier phases. The focused
MPI differential constructs genuinely sharded input iterators, produces each
matching shard, prepares children and composes them. Its output separates producer
work/payload/time, child preparation time and composition work/time. Complete
matrices, unitaries and state vectors appear only in its tiny independent test
reference; the runtime owns local native partitions and sparse child resources.

## Admission and execution

Before PREP, a complete MPI-free local dryrun validates every primitive and child,
checked work, native dispatch and routing-payload ceilings. All ranks agree that
result, including caught local panics, before a fixed common selected-child loop
enters each child's own admission lane. The parent releases its lane first.
Every child checks current live node capacity, so a restrictive late child can
reject without changing the input. Constructor admission first agrees raw
weights, child metadata/order, mappings and limits. After a common caught
validation/count pass, it compares actual normalization, PREP norm and roundoff,
and every forward/adjoint event's exact bytes. Equal input floats alone are not
used as evidence of equal compiled streams.

`apply` repeats whole-operation admission immediately before emission. An MPI
error or panic after emission aborts the existing MPI job, including late child
re-entry failure. Bounded subprocess tests verify the fatal boundary. CPU native
errors can return after mutation: the register's state is then unspecified; no
rollback or success receipt is promised. Owner, mapping, budget and supported
primitive failures caught by admission leave the state unchanged.

Native capability checks preserve the actual supported paths. In QuEST 4.3.0,
H, Ry and X reach `validateAndApplyAnyCtrlAnyTargUnitaryMatrix` with `CompMatr1`,
which calls `validate_mixedAmpsFitInNode` for two mixed amplitudes. Replay rejects
these primitives on a distributed partition containing one amplitude. A scalar
phase also rejects that partition when its dispatch emits X for negative
controls. Positive-controlled/global diagonal phases remain supported. Matching
with color Hadamards rejects that partition; color-free custom indexed matching
remains supported. The eight-rank/three-qubit test checks both admitted and
rejected routes before PREP. Native QuEST is unchanged.

## Resource meaning

All original children remain owned and charged, including zero-weight children;
SELECT omits zero weights. A nonzero weight whose PREP mass underflows remains a
selected complete child with its complex branch phase. K original owners retain
K independent scratch registers. No scratch fusion is claimed.

Admission counts actual owned Rust Vec capacities, explicit scalar/Arc metadata
allowances, the complete declared selector-construction byte ceiling during
compilation, a reusable primitive executor, and an explicit 16 KiB stack allowance
covering `equal`'s known 8 KiB buffer and wrapper frames. Guards reconcile temporary
and retained capacities and count simultaneous environment-registered owners,
including the input register. Unregistered caller-owned objects need their own
admission. Node admission uses the largest rank's accounted payload times the
caller's conservative `ranks_per_node`; it does not discover physical placement.

The resource report distinguishes K local complex-state payloads from the
native allocation allowance reserved by the environment (four copies per state)
and from Rust capacity/metadata allowances. These reservations are not measured
native allocation, allocator overhead, process RSS or OS memory. Native temporary
workspace, MPI protocol and native internal wire are unmeasured. Actual process
caps remain a separate mechanism.

For D amplitudes, P ranks, local L = D/P, flag bit f and batch size B = 64, the
batched matching loop has at most

- `P * ceil((L/2)/B)` rounds when f is local;
- `(P/2) * ceil(L/B)` rounds when f is global.

Each round has four peer packet passes of at most 5128 bytes per peer. The
per-rank send and receive ceilings are each `4*T*(P-1)*5128`; aggregate sends
multiply by P. Checked arithmetic, layout/sort/search and native-pass ceilings
are admitted before controls can skip rounds. Modeled work is a conservative
scalar-operation envelope; native dispatch counts differ from portable primitive
gates and from elementary/Clifford+T counts. Saturating runtime telemetry is not
used as admission evidence.

Constructor communication is separate from repeated apply. If E is the actual
combined forward/adjoint event count, compiled equality uses `29 + 4*E` scalar
collective calls. Each 48-byte event uses two broadcasts and two agreements,
including an eight-byte length word. Raw-input equality contributes
`3 + ceil(raw_bytes/8192)` calls. The comparison broadcast-payload report counts
`(P-1)*(56*E + 112 + 8 + raw_bytes)` bytes. A separate generous checked ceiling
covers other constructor scalar collectives. Agreement/coordinator payloads,
MPI protocol and native internal communication are excluded from that byte
report. There is no batching or speedup claim.

## Focused reproduction

Use an installed CPU/MPI QuEST and the same MPI ABI for `MPICC` and `mpiexec`:

```sh
export QUEST_ROOT=/path/to/installed/quest
export MPICC=/path/to/matching/mpicc
cargo test -p quest-rs --features qsvt \
  --test matching_cost --test matching_runtime --test matching_lcu
cargo test -p quest-rs --features qsvt,mpi \
  --test matching_lcu_collective -- --nocapture --test-threads=1
cargo test -p quest-rs --features qsvt,mpi --lib weighted_matching_ \
  -- --nocapture --test-threads=1
cargo clippy -p quest-rs --features qsvt,mpi --lib --tests -- -D warnings
```

The CPU differential covers three nonzero terms with a padded selector label and
five retained owners with an intervening zero weight and a genuinely underflowed
nonzero PREP mass. Both use arbitrary full 512-amplitude states, complex weights,
reversed operands, failure/color/padding sectors, a spectator, positive/negative
controls and standalone adjoints. The same three-term differential runs on
1/2/4/8 ranks and two independent split communicators; the five-owner case runs
on 1/2 ranks. Comparison tolerance is 2e-12 per complex amplitude.

Admission regressions include a one-rank raw-weight mismatch, a compiled phase
mismatch, a late local error/panic, zero application-work allowance, one-rank live
capacity exhaustion, an actual restrictive final child's routing capacity, and
native one-amplitude partitions. Error and panic injection after a nonzero PREP
rotation abort within bounded subprocess deadlines on 1/2 ranks. This is local
CPU/MPICH functional evidence, not a multi-host capacity campaign, distributed
tensor composition, generalized loaded-resource portfolio, inverse-accuracy
campaign or completed CFD research programme.

The final post-selector-correction focused run passed four CPU cost/runtime/differential tests (the LCU
case took 20.65 s), the seven-request weighted MPI matrix (73.54 s), two bounded
fault/admission parent tests (7.10 s), and the existing five-request matching MPI
matrix (15.38 s). Strict native library/integration-test Clippy passed. Timings
are debug-profile local test observations including cold reference calculations,
not execution benchmarks. Shared QSVT plan verification is separate; this chapter
does not replace the programme's integrated checkpoint receipts.

The sanitized [focused receipt](data/2026-10-06-distributed-weighted-matching/focused.json)
binds nineteen scoped source/dependency files to owner and independent log hashes.
The independent review reran CPU (20.32 s), the seven-request weighted MPI matrix
(73.57 s), the rejection/abort matrix (7.07 s) and strict Clippy. Source bytes
matched before and after; this is a focused content manifest, not a complete
build attestation. Earlier 20.58/73.58/6.58-second runs are historical pre-selector
review-correction observations and are superseded by the final focused receipt.
