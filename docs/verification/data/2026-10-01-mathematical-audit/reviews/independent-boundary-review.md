# Independent circuit and consolidation boundary review

Reviewed root-owned changes independently of my Task3 implementation. Read-only
inspection covered quest-synthesis/grid.rs and the containment proof ledger;
shared ROTATION_ALGORITHM use in synthesis, compiler and worker; the offline
Budget/Policy retry classifier; completion_outer, complex_hermitianized,
offline_retry_policy and generator_provenance regressions; and the audit plan,
ledger and available validation logs. No heavy gates or implementation edits.

## Findings

No actionable correctness defect or existing final-validation overclaim found
in this scoped diff.

## Mathematical and behavioral checks

- The cap comparator is an independent exact Q(sqrt(2)) comparison. Opposite
  signs are handled before squaring; two negative operands reverse the squared
  comparison correctly. The feasible predicate independently imposes both
  physical and bullet disk constraints. The test exercises exact-cap points,
  rather than defining success through the producer's sphere predicate.
- The sphere proof matches Grid's factors 8/epsilon²,4/epsilon,2. The exact cap
  gives radial displacement <=1, tangential magnitude <=sqrt(8) and bullet-pair
  norm <=2, hence13. Both feasible embeddings imply coefficient squared norm
  <=1. The target coordinate-width admission and one-unit sqrt-half enclosure
  bound floor-midpoint errors as stated: replacing the algebraic root changes
  each complex embedding by <=2h; target midpoint error <=h; dot/cross error
  <=3h+2h²<=4h. With h=epsilon²/4096 and epsilon<=1, the conservative2/3/3
  bounds give22<36. Flooring the midpoint does not require an unjustified
  nearest-rounding assumption. Exact LLL row operations preserve a unimodular
  basis. The regression uses target z=1 and small denominator levels; the
  analytic inequalities, not that finite fixture, support arbitrary admitted
  target angles. The ledger explicitly makes this distinction.
- The linear completion oracle independently solves a0²+a1²=.75 and
  a0*a1=-.12. Choosing the larger positive a0 puts the zero outside the closed
  unit disc, while the reflected choice has the same boundary autocorrelation.
  Both algorithm choices run the same independent test. Its scope is correctly
  described as a linear outer-root fixture, without claiming that exported
  completion residuals certify high-degree outerness.
- For rectangular B, p(H)=iH has top-right iB and bottom-left iB†. The expected
  lower-left fixture entries are .1+.2i and -.2+.3i, preserving the coefficient
  i rather than conjugating it. The reordered operand layout additionally
  checks that block interpretation does not accidentally depend on canonical
  qubit order.
- Verifier Budget and Policy are terminal without swallowing the error. The
  attempt is appended and original source/last export retained before return.
  Recomputing an offline candidate under an explicit precision policy can
  change numerical verification bounds, so numerical retries remain bounded
  and appropriate. The new budget regression asserts one attempt and retained
  provenance. No automatic algorithm/precision-route switch was introduced.
- One stable ROTATION_ALGORITHM string now supplies Approximation and
  NativeSynthesis metadata. The worker's existing quest-synthesis prefix plus
  that string reproduces its former correct engine identity. The compiler's
  former inconsistent ross-selinger-rust-v1 identity is corrected. Metadata
  sharing does not share producer arithmetic with independent recertification.
  The new equality fixture directly tests the exposed producer/compiler
  identities; the worker use is established by source inspection.

## Evidence and completion boundary

The circuit log and initial workspace Nextest log show the new cap, provenance,
outer-factor, retry and complex-block tests passing. The large-degree log
contains actual successful8192 interval-FFT certification, original8105 offline
export/certification, and deterministic1/2/4-worker8105 checks. These are
numerical/verification evidence, not proofs of lattice-search completeness,
global T-count optimality, arbitrary user-callback consistency or hardware
execution costs. The ledger preserves these boundaries and keeps unavailable
Linux/MPI/GPU evidence separate.

At inspection time workspace-nextest.log still contains the initial875-test
run with873 passes and2 failures. Root subsequently reported targeted fixes
for both (offline expression-depth storage boundary and Rayon-worker warmup),
and warm-parallel-stress.log independently records20/20 passes. Final integrated
rerun is intentionally pending additional independent-review resource/runtime
fixes. The audit ledger presently says final validation is pending, so it does
not misrepresent that initial run as an integrated pass. The final receipt must
use the latest rerun rather than silently quoting only the873 successes.

This review does not approve newly proposed native failure-report or DAG-budget
changes that were not in the scoped inspected diff, and does not review my own
Task3 code. Utility and generic-function math have separate reviewers.
