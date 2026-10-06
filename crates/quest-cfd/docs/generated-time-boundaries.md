# Complete distributed polynomial-time box data

The generated uniform-box path supports prescribed polynomial-time lifting, full exterior Dirichlet traces and body acceleration for BDM1/P0 and BDM2/P1 in two and three dimensions. It retains every coordinate of the existing mass-orthonormal distributed constraint chart. It does not construct a global mesh, lifting, chart matrix or reduced polynomial drift tensor.

`BoxTimeDataShard` owns exactly one rank's complete physical cells. Construct its range from `prepared.chart().local_row_range()` divided by `geometry.local_velocity_dimension()`. This is the physical cell partition; the full null-coordinate partition is different. Pass all `(degree+1)*owned_cells` `BoxTimeCoefficient` records in time-power-major, owned-cell order. Degree 0 through 8 and a finite closed time interval are supported.

Each coefficient contains full broken lifting and body-acceleration entries in component/scalar-node order. Exterior trace coefficients use the same cell ordering, with only that face's scalar nodes active. Use `facet_mode_count`, `facet_velocity_node` and `facet_is_exterior` to populate them. Every inactive component, off-facet node and interior-face trace slot must be zero. No omitted-mode convention is available. Body entries are acceleration coefficients; the force evaluation multiplies by the complete cell mass.

The local constructor checks shapes, finite values, actual vector capacities and numerical kernel budgets. It prepares time powers and analytic derivatives through shared MathCore kernels. Local construction is not a global compatibility certificate. Every rank must complete local source construction successfully before jointly entering `PreparedBoxConstraints::prepare_time_force`; applications must collectively agree their own input/import errors. This method checks every coefficient's divergence, paired normal trace and prescribed exterior normal trace through a fixed bounded halo schedule. Empty-owned ranks still participate. It retains the numerical coefficient defect, its sum-of-time-powers interval envelope and net outward-flux diagnostic. These floating diagnostics are not outward-rounding proofs of exact incompressibility.

For a common exact time and complete local chart coordinate vector, `PreparedTimeBoxForce::drift_at` evaluates

```text
u = Q a + ell(t)
f = central-conservative-DG/SIP(u, g(t)) + M b(t)
h = f - M ell_dot(t)
a_dot = Q^T h.
```

It keeps `f`, `h`, `ell_dot` and the null acceleration separately in `DistributedTimeBoxDrift`. All homogeneous components supplied in `ell` are preserved. Exterior data replace the entire prescribed trace; they are not increments to the autonomous lid value. The velocity halo exchanges the full `Q a + ell(t)`.

Periodic boxes enforce the periodic normal identification. Closed-box geometry uses prescribed Dirichlet/SIP data on every exterior face. Nonzero normal data are permitted only when the complete supplied lifting passes divergence and normal compatibility. Balanced total outward flux alone is insufficient. This contract does not implement a Neumann farfield, prescribed pressure outlet or convective wake boundary.

`recover_pressure` on the prepared time source recovers the physical pressure from `h`, applies the existing `p=-lambda_pressure` sign and zero volume-weighted mean gauge with compensating normal multipliers, then independently checks

```text
f - M (Q a_dot + ell_dot) - C^T lambda.
```

The source fingerprint and exact in-process chart owner must match the drift result. Pressure preflight also collectively agrees the exact time bits and a rank-ordered byte-wise digest of the complete input coordinate shards. This rejects accidental mixing of results from different collective queries while allowing equivalent repeated queries; the digest is noncryptographic provenance, not a collision-free or numerical certificate. Passing original `f` to the autonomous homogeneous-acceleration recovery is not the time-dependent pressure contract. Pressure outputs and residuals remain numerical; generic QR multipliers alone are not physical pressure coefficients.

`prepare_time_box_kvn_history_inverse` uses this validated source under the existing globally synchronized row/spool schedule. Every left and right drift evaluation receives the exact physical quadrature time supplied by temporal DG. External time remains a parameter, not an added physical coordinate. The weighted generator remains

```text
L_ij(t) = -0.5 (F_axis(i,t) + F_axis(j,t)) D_ij sqrt(W_i/W_j).
```

Body forcing modifies `F`; it does not create an additive KvN amplitude RHS. The amplitude source is zero except for the initial history trace. The factory rejects a source interval that does not cover the full horizon, including on the zero-RHS path. The existing autonomous factory remains available. The internal dispatch is sealed; arbitrary callbacks cannot inject unscheduled MPI calls into independently advancing stored-row readers.

Temporal quadrature evaluates the nonautonomous operator at the actual nodal times. It is the existing mass-lumped DG discretization, not exact integration of arbitrary high-degree time products. Likewise, finite-dimensional coefficient compatibility does not establish resolved physical boundary behavior or configuration-window convergence.

Resources separately report maximum-rank arithmetic, a conservative aggregate job ceiling and global payload transport. Retained source guards include actual owned coefficient/kernel capacities. Query admission includes original/effective force and derivative outputs, local input/velocity/kernel/halo scratch and overlapping native chart-query envelopes; pressure adds two original-residual queries to the four-query recovery. Source preparation, repeated time evaluation, complete constraint checks and the global history row schedule are charged. Per-query input-state hashing adds a conservative `128*(maximum_owned_rows + ranks + 4)` maximum-rank work allowance; the fixed control envelope includes the bounded digest broadcasts and pressure identity agreement. Native QR fill and pressure coupling remain nonlocal and are not assumed cheap. These are managed payload and work models, not allocator/RSS or measured CPU-instruction counts.

The source/force/pressure stages have independent implementation review. The history consumer has also passed independent root review and a separate complete-history MPI rerun; focused tests and strict library/test Clippy pass. These scoped results do not replace final integrated workspace acceptance.

The focused MPI test uses complete bounded dense references only for independent comparison. It tests both spatial orders and dimensions, periodic and nonzero-normal Dirichlet cases, a body force with nonzero last-coordinate projection, retained homogeneous lifting, original pressure residuals, empty-owned ranks, invalid metadata, mixed prior time/state results, equivalent repeated queries, budgets and guard lifetimes on 1/2/4/8 ranks and a split communicator. Pure source tests also exercise degree8 and strict owned subranges. The history test aligns the actual distributed chart before comparing complete temporal DG1/DG2 matrices; tensor configuration grids are not invariant under an unrecorded chart rotation.

```sh
cargo test -p quest-cfd --features distributed \
  --test box_time_data --test box_time_collective -- --nocapture --test-threads=1
cargo test -p quest-cfd --features distributed --lib \
  generated_time_history_uses_every_exact_quadrature_time -- --nocapture --test-threads=1
```

Use a matching installed QuEST/MPI runtime. The history fixture declares a 64MiB native environment budget and an explicit scalar-query-work allowance; the history API admits the complete native capacity alongside source storage. Large work caps in the fixture authorize conservative accounting, not a claim that classical preparation is free or that a large physical problem is feasible. A rank owns only its shard; on a one-rank communicator that share naturally covers the complete field.
