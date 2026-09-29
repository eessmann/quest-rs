# Consolidation correctness and measurements

This record covers correctness fixes and storage consolidation against baseline
`9d614fb`. The native runtime, ideal DAG, structured SSA, OpenQASM frontends,
exact and numerical optimization passes, and checked external workers retain
their separate contracts. Caller changes are in the
[migration notes](2026-09-28-consolidation-migration.md).

## Corrected defects

| Severity | Finding | Correction and evidence |
| --- | --- | --- |
| P0 | Safe probability adapter entered an allocation-performing native overload before checking target count; 64 targets invoked undefined signed shifting | Validate counts, indices, duplicates and output size in the bridge, then call caller-buffer native overload. Original native sanitizer witness and focused bridge regressions retained; generator template and output change together |
| P1 | Structured synthesis composed a global error bound through empty SSA oracle placeholders despite numerical admission lacking unitary/norm evidence | Preserve local certificates; withhold the global bound through reachable captured oracle calls. Direct, nested, negatively controlled and adjoint `2I` witnesses retain local certificates and no global claim |
| P1 | Independently mutable generalized angles and matrices could describe different operations; MPI rebuilt matrices from angles | Immutable admitted payload; frozen execution matrix words and separate immutable source provenance; adversarial bit-pattern transport tests |
| P1 | Sparse memory admission omitted compressed pointer-array storage | Shared checked data/index/pointer accounting, including zero-entry and exact-boundary regressions |
| P2 | Accepted empty Laurent polynomials panicked in generalized synthesis | Normalize empty coefficients to zero consistently in production and offline paths, including positive offsets |
| P2 | Trace export failure replaced the primary computation failure | Preserve primary error as the source and retain the trace failure as secondary context |
| P2 | Explicitly broken native discovery made generator fixtures silently skip | Explicit configuration errors now fail tests; subprocess fixture verifies the selected invalid path appears in diagnostics |
| P2 | Long exact merges retained quadratic cumulative source histories | Immediate-input provenance DAG, immutable graph sharing and budgeted source expansion; long-chain and divergent-snapshot regressions |
| P2 | Typed expressions recursively cloned subtrees; dominance stored a set for every block | Shared expression nodes with construction/materialization budgets; per-region immediate dominators; measured allocation probes and 451-block regression |
| P2 | Block positions could be reused across edited publications without a snapshot identity | Fresh verified snapshot identities, checked block handles, and input/output identities on analysis reports |
| P2 | Remez enclosure subdivision and solver scratch lacked complete storage admission; work did not scale with evaluated expression size | Checked subdivision plus solver storage, fallible scratch allocation, cumulative size-scaled enclosure work and exact boundary regressions |

The initial shared-expression admission omitted retained `Arc<Node>` overhead
and allowed an infallible boolean constructor to bypass its budget.
A shallow-node regression reproduced the gap before correction. Admission now
bounds the conservative retained graph plus simultaneous syntax materialization;
boolean construction is fallible. Charging one unit per enclosure interval also
ignored the cost of the function AST and polynomial. The corrected charge
scales with checked AST/coefficient size and accounts for all four possible jet
evaluations per interval. A nonzero-work-limit regression failed before this
correction and passed afterward.

The [native probability witness](fixtures/native-probability/witness.cpp) links
the relevant QuEST source translation unit with shift sanitization. One target
exits normally; 64 targets trigger the sanitizer's `SIGILL` trap before native
input validation. The [recorded exits](data/2026-09-28-consolidation/probability-native-witness.json)
preserve this failure separately from the passing safe-bridge regressions. This
is a focused translation-unit witness, not a full native sanitizer campaign.

## Architecture and coverage map

| Crates | Responsibility |
| --- | --- |
| `quest-rs`, `quest-sys`, `quest-build`, `xtask` | Environment lifetimes, sealed register kinds, transactional preparation, non-panicking cleanup, generated boundary coverage, installed CMake configuration and final executable linking |
| `quest-language`, `quest-qasm`, `quest-macros` | Classical SSA, verified publication identity, bounded expressions/dominance, gate registry, capture evaluation order, diagnostics, OpenQASM 3.1 simulator profile and renamed macro dependencies |
| `quest-circuit` | Finite ideal DAG versus substantive structured lowering, ordered targets/signed controls, exact phase, numerical admission, rewrite provenance and bounded analyses |
| `quest-math`, `quest-optimizer-protocol`, `quest-optimizer-client`, `quest-optimizer-worker` | Independent equivalence/error certificates, bounded worker messages/processes and candidate rejection; mathematical evidence remains distinct from execution occurrences |
| `quest-numerics`, `quest-polynomial`, `quest-qsp` | Numerical policy, polynomial support, synthesis stage invariants, solver scratch and subdivision storage; production, offline construction and certification remain separate |
| `quest-qsvt`, `quest-qsvt-io`, `quest-qsvt-cli` | Encoding/layout contracts, admitted source/execution payloads, direct/projected continuation, sparse/HDF5 admission, deterministic MPI transport and primary error reporting |

## Baseline measurements

The standalone [probe](fixtures/consolidation/run.py) runs identical source against
the selected repository tree. It uses a counting system allocator and records
requested bytes, live retained bytes, peak live requested bytes and allocation
calls. Counts exclude allocator bookkeeping and are not RSS. Measurements are
single-process debug runs; elapsed times are diagnostic, not a performance claim.

Baseline source is `9d614fb`; the raw
[allocation results](data/2026-09-28-consolidation/baseline-allocations.jsonl)
include rejected admission. The 4096-rotation merge retains 134,547,032 bytes.
Doubling from 2048 to 4096 grows retained bytes from 33,719,896 to 134,547,032. Building a depth-14
self-shared expression performs 196,572 allocation calls and retains 5,914,448
bytes. The 451-block program is rejected by the old quadratic working-storage
forecast under default limits.

The consolidated tree uses the same probe and toolchain. Raw
[results](data/2026-09-28-consolidation/consolidated-allocations.jsonl) and
[measurement metadata](data/2026-09-28-consolidation/metadata.json) accompany this
record. These are representation probes, not end-to-end simulator benchmarks.

| Workflow | Before | After |
| --- | ---: | ---: |
| 2,048-rotation merge, retained bytes | 33,719,896 | 411,216 |
| 4,096-rotation merge, retained bytes | 134,547,032 | 820,816 |
| 4,096-rotation merge, total requested bytes | 277,417,424 | 9,866,576 |
| 4,096-rotation merge, allocations | 218,504 | 198,053 |
| Depth-14 shared expression, retained bytes | 5,914,448 | 1,890 |
| Depth-14 shared expression, allocations | 196,572 | 19 |
| 91-block SSA admission, allocations | 16,367 | 9,186 |
| 451-block SSA admission | Rejected by storage forecast | Admitted; 669,155 retained bytes |

Doubling merge length now approximately doubles retained bytes, instead of
quadrupling them. Repeated expression construction shares nodes; materializing
separate execution occurrences still costs their expanded size and is budgeted.
The larger SSA case is newly admitted, so its elapsed time is not a before/after
speed comparison. Removing redundant verification reduces allocations in the
smaller case without changing its published representation size.

## Duplicate rules and API complexity

| Area | Consolidation |
| --- | --- |
| Complex FFI values | Two CXX value definitions become one; remove 9 Rust conversion buffers, 13 scalar conversion locals and 5 result conversions; retain required native `qcomp` conversion |
| Native ownership cleanup | Seven copies of admission/destruction/accounting/failure handling become one helper with seven explicit destructor/resource-kind mappings |
| Generated controlled Pauli adapters | Six adapters across four output locations come from two checked family descriptors; exceptional Gadget/Str adapters remain explicit |
| Native discovery | Two package evaluations on Cargo discovery become one authoritative CMake configure/File API evaluation; remove `cmake-package` and unused transitive dependencies |
| Ideal planning | Remove the empty public `LoweredProgram` stage; consuming `BoundProgram::plan()` retains its checks |
| Gate and operand semantics | Shared checked gate registry adapter, operand traversal and remapping replace independent rules in compiler/runtime paths |
| Runtime preparation | Shared control scratch, reset channel, matrix packing and allocation admission replace repeated helpers |
| Numerical continuation | One direct/projected enum replaces paired optional authorities in pure, admitted and prepared transforms |
| Synthesis reporting | Share checked scalar storage and typed completion/report assembly; retain separate arithmetic kernels and independent certification |

This is not a claim that every file is shorter. Added budget checks, source
identity and regression coverage are deliberate complexity needed to preserve
the guarantees while removing duplicated rules and retained data.

## Validation scope and limitations

| Final check | Result |
| --- | --- |
| Workspace build, default and all features, locked | Passed |
| Workspace Nextest, all features, locked | 604 passed; 3 explicitly ignored scale tests |
| Separate workspace doctests, all features | 58 passed; 1 ignored build-script snippet exercised by independent consumers |
| Workspace Clippy, all targets/features, `-D warnings` | Passed |
| Rust formatting and diff whitespace | Passed |
| Generated binding freshness | Passed |
| Direct/facade/renamed/wrapper consumers outside Cargo | All 4 passed RUNPATH, dependency closure and numerical checks |
| Explicit CPU/OpenMP/GPU register execution | All 3 passed complete complex-amplitude and norm checks |
| MPI integration | Passed actual 2/4-rank and subgroup tests |

Host coverage is Linux x86-64 GNU, pinned `nightly-2026-09-06`, GNU C++ 16.2.1,
QuEST fork 4.3.0 with double precision and deprecated APIs disabled, MPICH 5.0.1,
and existing serial HDF5 1.14.6. The installed QuEST package enables MPI,
subcommunicators, OpenMP, CUDA and cuQuantum. It was reused without modification.

The separate feature matrix passes pure circuit/CLI checking with deliberately
invalid native paths, certification (48 tests), synthesis-only worker (11),
ZX-only worker (12), HDF5 IO (20), and MPI CLI without default features (16).
Offline synthesis has its own scoped test run as well as the integrated suite.
Raw commands and exit status are in
[feature-matrix.json](data/2026-09-28-consolidation/feature-matrix.json).

Four fresh independent consumer layouts—direct bridge, facade, renamed facade,
and wrapper library—built and executed outside Cargo with `LD_LIBRARY_PATH`,
`LD_PRELOAD` and `LD_AUDIT` cleared. Each passed numerical checks, ELF `RUNPATH`
inspection and `ldd` dependency resolution. This validates the final-target
`quest-build` helper on this installed closure; Cargo metadata alone does not
promise arbitrary downstream deployment. Indirect native dependencies remain
the installed native library's responsibility.

The standalone backend probe explicitly selects CPU, OpenMP or GPU for both the
environment and register; all three passed every complex-amplitude and norm
check. The [backend results](data/2026-09-28-consolidation/native-modes.json)
record successful execution of these deployment checks. GPU residency was
asserted on the allocated register, rather than inferred from build flags or
automatic small-register deployment. Multi-rank integration tests exercise actual two/four-rank
MPI execution, collective mismatch recovery and subgroup schedules.

Earlier sandboxed native attempts failed because MPICH could not create its
local sockets; host execution passed. The Homebrew MPI compiler wrapper referred
to an unavailable GCC executable, so validation used `MPICH_CC=/usr/bin/gcc`;
the bridge independently checked its ABI and loaded-library identity. No claim
is made for another platform, remote CI, cluster-scale MPI, GPU-aware MPI, or a
new numerical optimization algorithm.

Three pre-existing large-scale tests are explicitly ignored by the normal suite:
degree-8192 interval FFT, dense catalog Remez, and degree-8105 parallel acceptance.
They were not run in this pass. Existing degree-8105 pinned C++ phase comparison
does run and passes. Allocation measurements count requested storage, not RSS;
production budget estimates are conservative models, not allocator-exact peak
guarantees.
