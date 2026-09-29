# Optimization correctness regressions

This record links corrected defects to source and regression coverage. Priority
ranks the impact of the resolved defects. Workspace validation and timing
measurements are in the [results record](2026-09-28-optimization-roadmap-results.md).

## P1 — semantic correctness and proof identity, resolved

| Finding and resolution | Source and regression evidence |
| --- | --- |
| Exact rewriting could erase a source obligation: separately constructed parameter inverses were initially missed, and parity/ZX extraction could remove an unbindable rational-π input. Bounded source-structure equality now proves inverse cancellation, while original finite conversion is checked before each replacement. Binding still checks every declared parameter, including one canceled algebraically. | [symbolic expression](../../crates/quest-symbolic/src/lib.rs), [circuit angle and exact pass](../../crates/quest-circuit/src/model.rs), [symbolic regressions](../../crates/quest-circuit/tests/symbolic_contract.rs), [parity regressions](../../crates/quest-circuit/tests/parity_contract.rs) |
| Affine rotation certificates needed the original `r+sπ` identity and full scalar phase. The independent enclosure keeps the full 4π period and signed rational inputs; parent certification checks the original target. Negative-control lifting now has a full-matrix `-I` witness, including scalar phase on only the selected branch. | [affine certifier](../../crates/quest-math/src/approx.rs), [controlled lift](../../crates/quest-math/src/controlled.rs), [negative/large target tests](../../crates/quest-math/tests/approximation.rs), [full-phase control test](../../crates/quest-math/tests/controlled.rs) |
| Immutable publication identity and mandatory order were initially too weak: an owner token alone conflated rewritten snapshots, distinct bindings shared an ID, and edge projection omitted executable order. Separate ideal/bound snapshot IDs now change on publication and bind; plan admission rejects backward mandatory edges. Candidate replacements mint fresh occurrences and provenance. | [program publication and order](../../crates/quest-circuit/src/program.rs), [projection and ledger](../../crates/quest-circuit/src/optimizer_contracts.rs), [optimizer contract tests](../../crates/quest-circuit/tests/optimizer_contract.rs) |
| Structured analysis could treat cross-region references as alias facts and undercharge backedge reachability. Snapshot/region membership is now required for alias queries, every control and target participates in coupled value versions, and CFG reachability is charged before traversal. Exact SSA cleanup re-verifies the result and preserves traps, calls and joins as fences. | [QuantumFlow](../../crates/quest-language/src/ssa/quantum_flow.rs), [analysis tests](../../crates/quest-language/tests/quantum_flow.rs), [structured rewrite](../../crates/quest-circuit/src/structured_optimize.rs), [structured tests](../../crates/quest-circuit/tests/structured_optimization.rs) |
| MITM and ZX proposals could otherwise lose full phase or useful longer candidates. MITM uses every normalized matrix entry, including scalar phase, exact `R†T` matching and safe remaining-depth/cost dominance; the parent reconstructs and verifies output. ZX baseline and expanded proposals remain separately selectable by the beam. Approximate MITM keeps the original angle identity and lifts signed controls through every certified gate. | [MITM matrix](../../crates/quest-math/src/matrix.rs), [representative dominance](../../crates/quest-optimizer-worker/src/mitm/representatives.rs), [parent verification](../../crates/quest-optimizer-client/src/lib.rs), [MITM tests](../../crates/quest-optimizer-worker/src/mitm/exact.rs), [ZX/beam contracts](../../crates/quest-circuit/tests/beam_contract.rs) |

## P2 — resource and transactional admission, resolved

| Finding and resolution | Source and regression evidence |
| --- | --- |
| The new exact fork initially let fast paths bypass receiving-context coefficient caps, charged a canonical vector's length instead of retained capacity, and sorted imported terms before charging work. Publication now revalidates coefficients, charges actual capacity, compacts imports in place, uses fallible output reservation, and precharges the sort. Shared staged/retained counters include counted export. | [exact domain](../../crates/vendor/mathcore/src/exact/mod.rs), [fork regressions](../../crates/vendor/mathcore/tests/exact_affine.rs) |
| Parent affine synthesis preflight checked coefficient bits but not the caller's byte/scratch budget before launching a worker; half-angle denominator growth also needed a one-bit forecast. Shared target admission now precedes process launch. A sentinel worker proves rejected inputs never start it. | [target admission](../../crates/quest-math/src/approx.rs), [client preflight](../../crates/quest-optimizer-client/src/lib.rs), [sentinel tests](../../crates/quest-optimizer-client/tests/process_contract.rs) |
| The optimizer's aggregate ledger initially missed nested oracle traversal, variable input payloads, failure work, and impossible deployment shapes. Bounded traversal charges each visit before expansion; failed work allowances remain spent; ideal, bound and structured retained inputs include their variable payloads; deployment validates local state size and rank partition. Its default 10 million work/256 MiB budget is distinct from the fork's 16 million per-context work cap. Native V1 uses local **state bytes** and an ordered opaque MPI trace. | [contracts and ledger](../../crates/quest-circuit/src/optimizer_contracts.rs), [dispatch recipes](../../crates/quest-circuit/src/dispatch_recipe.rs), [contract tests](../../crates/quest-circuit/tests/optimizer_contract.rs), [recipe tests](../../crates/quest-circuit/tests/dispatch_recipe.rs) |
| Shared dispatch inventory initially risked overflow, exponential recursive discovery, and uncharged profile scratch. It now admits ordered gate/scalar/matrix recipes and temporary plus retained storage before growth. Cost assembly treats density uncontrolled scalar phase as a dispatch with no state pass and counts numerical density application once per side. | [recipe implementation](../../crates/quest-circuit/src/dispatch_recipe.rs), [recipe tests](../../crates/quest-circuit/tests/dispatch_recipe.rs), [native deployment tests](../../crates/quest/tests/deployment.rs) |
| Terminal and structured publication needed conservative fallback. Changed unknown-MPI communication is `Unscorable`; terminal fusion compares original and candidate with the same policy and publishes only a strict improvement. Structured cleanup releases measured scratch rather than retaining a full 64 MiB lease, and baseline/candidate admission failure retains the last complete publication. Synthetic oracle node preflight now shares the verifier's count and final SSA/oracle-bank publication is atomic. | [terminal pass](../../crates/quest-circuit/src/terminal.rs), [terminal tests](../../crates/quest-circuit/tests/terminal_contract.rs), [structured pipeline](../../crates/quest-circuit/src/structured_pipeline.rs), [structured terminal](../../crates/quest-circuit/src/structured_terminal.rs), [SSA edit test](../../crates/quest-language/tests/edit_contract.rs) |
| Warm conversion caches initially reduced logical binding forecasts, allowing an identical cloned source to cross an optimizer budget differently after binding. Symbolic forecasts now charge the same conservative source work regardless of cache occupancy; finite-conversion memoization remains. Cold/warm forecast and bounded admission regressions reproduced the defect before correction. | [angle forecast](../../crates/quest-circuit/src/model.rs), [symbolic regression](../../crates/quest-circuit/tests/symbolic_contract.rs), [admission regression](../../crates/quest-circuit/tests/optimizer_contract.rs) |
| Candidate generation and worker admission now preserve caps and prior results. Parity charges rebinding/replay; ZX charges additional graph-rule work and protects the pinned gadget predicate. Exact MITM charges operand and source replay before a window scan; both MITM adapters distinguish post-worker `RejectedOutput` from preflight limits. The beam retains proof leases and cumulative rational error history, counts bounded worker admissions, treats shortlist exhaustion as incomplete, and keeps the best complete result after budget exhaustion or timeout. | [parity](../../crates/quest-circuit/src/parity.rs), [ZX worker](../../crates/quest-optimizer-worker/src/zx.rs), [exact adapter](../../crates/quest-circuit/src/beam_mitm.rs), [beam](../../crates/quest-circuit/src/beam.rs), [beam contracts](../../crates/quest-circuit/tests/beam_contract.rs), [timeout test](../../crates/quest-circuit/tests/beam_timeout_contract.rs) |

## P3 — measurement integrity, resolved

The measurement harness now keeps failed configurations and raw partial rows
without publishing comparative medians or reuse break-even for incomplete
campaigns. It validates every expected stage/sample and MPI rank, separates a
valid but incomplete optimizer result from a failed measurement, and computes
break-even with exact rational time arithmetic. The MPI facade fixture compares
baseline one-qubit probabilities on zero and plus inputs with collective error
agreement; the separate direct-bridge MPI witness checks full complex entries.
See the [runner](fixtures/optimization-roadmap/run_native_optimization.py),
[summarizer](fixtures/optimization-roadmap/summarize.py),
[MPI fixture](fixtures/optimization-roadmap/native_mpi_optimization.rs), and
[campaign-free harness tests](fixtures/optimization-roadmap/test_measurement_harness.py).

## Supported boundaries

- The [vendored fork](../../crates/vendor/mathcore/UPSTREAM.md) adds a separate
  exact affine domain. Its legacy CAS algorithms remain unchanged and
  approximate; they were not audited or repaired. Feature gates isolate them
  from the exact-only
  `quest-symbolic` dependency graph.
  Quest-owned affine summaries and conversion checks are independent of the
  fork's canonicalizer, and parent math certification independently reconstructs
  worker output. A fork equality result is not its own proof oracle.
- Exact target sidecars and mathematical certificates do not include binary64
  native execution error. Required-global approximation rejects unsupported
  numerical, oracle and dynamic-control composition. Structured worker search
  remains explicitly skipped; the verified structured pipeline uses static
  exact cleanup and terminal fusion instead. Unknown distributed communication
  remains incomparable when its ordered semantic content changes.
- Native V1 preparation bytes are logical forward/adjoint matrix payload, not
  GPU mirrors, padding or RSS. The beam's exact MITM requests use the two-qubit
  depth-six preset even for one-wire windows; finite completion does not prove
  global optimality. ZX's additional-rule budget excludes internal upstream
  simplification/extraction operations, which are separately process-bounded.
  The collective MPI facade fixture's marginals cannot independently detect a
  global-phase or correlation error; full-complex native witnesses are separate.
