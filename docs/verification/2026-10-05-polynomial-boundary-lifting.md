# Complete polynomial-time boundary lifting

`physical_space::PolynomialBoundary` is a bounded reference owner for the complete
BDM1/P0 and BDM2/P1 spaces in two and three dimensions. It borrows the immutable
`PhysicalSpace` chart and retains every independent physical coordinate. It accepts
full polynomial-time broken lifting, full exterior facet velocity, and full
cell-local body force. This foundation extends the earlier scalar affine BDM1
adapter; it is not a generated distributed boundary producer or a new wake model.

For each coefficient of `t^k`, `BoundaryTimeCoefficient` stores:

- Every lifting coefficient in cell/component/scalar-node order.
- Every vector trace value at each exterior facet's P1 or P2 nodes, including all
  P2 edge midpoints. `boundary_facets()` returns the outward normals and ordering.
- Every coefficient of the physical body-force acceleration in the full local
  vector basis. Its weak load is the cell mass matrix times that field.

`velocity_nodes()` exposes the corresponding bounded nodal physical positions.
These convenience helpers allocate bounded reference geometry; they are not
constant-storage or production mesh recipes. Prescribed data replaces the base
space's stationary lid data explicitly. Periodic spaces accept no exterior trace.

## Original coordinates and continuity

The physical velocity is

`u(t,a) = Q a + ell(t)`.

The supplied lifting remains intact, including its homogeneous components. The
constructor checks all original normal-jump and integrated divergence rows for
every time coefficient; exterior rows must equal the supplied full trace's normal
component. The numerical admission band is `1e-9 * max(1, coefficient magnitude)`.
A failed condition rejects the input rather than projecting it onto a homogeneous
space. P1/P2 divergence lies in the retained P0/P1 pressure space, and every normal
trace polynomial node participates. Full query residuals subtract the prescribed
exterior trace in original broken coordinates. Nonfinite input and intermediate
constraint values reject explicitly.

The complete acceleration is

`a_dot = Q^T [force(u, g, f) - M ell_dot]`.

The force includes conservative volume convection, central full trace convection,
both SIP normal-gradient terms and penalty, and the volume body-force load. Fixed
MathCore basis kernels prepare the full exterior facet interpolation tables once;
numerical queries use these tables and finite time powers. They do not perform
symbolic algebra or use callback polynomiality assumptions.

Nonzero prescribed normal velocity is supported only for a coefficientwise
compatible closed-box lifting. For the exact conforming polynomial fields, the
inviscid discrete energy identity is

`u^T force_convection = -1/2 integral_boundary (u dot n) (u dot g)`.

Interior central flux terms cancel against the volume boundary terms. An
independent advective-volume form agrees with the conservative form. The tests
use `ell(x,y)=(x+0.3,-y)` in both 2D and 3D, plus a homogeneous lifting component;
the resulting boundary work is nonzero. This central prescribed-trace extension
is not an upwind inflow/outflow algorithm or a boundary-stability certificate.
Natural outflow and mixed boundary geometry remain rejected.

## Pressure and temporal forcing

Pressure recovery uses the full original momentum residual

`force - M (Q a_dot + ell_dot)`.

The shared physical row-QR recovery retains every P0/P1 cell pressure mode and
normal-trace multiplier. It fixes the closed-domain volume-weighted zero-mean
pressure gauge and reports full momentum, continuity and gauge defects. A
homogeneous acceleration or a homogeneous continuity residual cannot stand in for
the supplied lifted physical state.

`PolynomialOde::from_polynomial_boundary` snapshots the known quadratic force
jointly in every physical coordinate and every supplied time-power data mode.
Shared MathCore polynomials then replace mode `k` by `t^k`, coalesce coefficients,
and subtract the derivative of the recorded lifting projection. Time is an
external final symbol, never an extra physical coordinate. Recorded coefficients
are exact binary64 dyadics; extraction probes and the roundoff-scale estimate do
not prove exact agreement with the original floating assembly. Independent
physical-time probes must pass before the snapshot is returned.

The existing stateless Carleman/history adapter evaluates the prepared coefficient
kernels at the actual DG1/DG2 temporal nodes. Tests compare its source RHS with the
full physical residual at both endpoints and the DG2 midpoint. This proves the
forcing-time plumbing; it does not certify hierarchy truncation or a completed
time-dependent quantum CFD campaign.

## Resource admission and evidence

`BoundaryLimits` separately caps time degree, construction, each numerical query,
pressure recovery and managed peak bytes. `BoundaryResources` charges actual input
Vec capacities, prepared tables, retained projections, the borrowed complete
physical source and numerical/pressure scratch. `PhysicalSpace::retained_bytes()`
includes actual geometry, matrix, chart and numerical-table capacities plus
admitted MathCore payloads. The fixed facet-basis construction allowance is 100
million modeled work units and 4 MiB of additional transient storage. These are
conservative logical ceilings, not measured runtime or RSS. Allocator and operating-system metadata and process RSS are excluded.

Extraction has a separate `BoundaryExtractionLimits`: complete direct residual
queries, exact polynomial preparation, joint/transformed polynomial overlap and
source memory are admitted before extraction. With joint width `J`, the preflight
charges at most `2*J*J+29` physical queries plus a polynomial preparation charge
per complete joint term; the byte ceiling includes four polynomial-owner
allowances and the entire physical source/query peak. Actual residual query counts
remain in the snapshot evidence. The snapshot joint width is at most
64, including the external data modes. The bounded physical model still admits up
to 768 broken coefficients. A complete 85-coordinate periodic 3D BDM2 query passes
and retains the last coordinate; its snapshot rejects the full joint width. It
does not substitute a reduced chart to meet the limit.

Focused commands from the repository root:

```sh
cargo test -p quest-cfd --test physical_boundary --test physical_order -- --nocapture
QUEST_ROOT=/path/to/installed/quest MPICC=/path/to/matching/mpicc \
  cargo clippy -p quest-cfd --features distributed --lib \
  --test physical_boundary --test physical_order --no-deps -- -D warnings
```

Five new tests cover complete 2D/3D P1/P2 nonzero-normal boundary work, lifting
acceleration and physical pressure; independent stationary assemblies with
changing lid/body data at four physical times; cubic-time extraction and actual
DG1/DG2 source nodes; all 85 periodic 3D BDM2 coordinates; malformed data, every
resource ceiling, actual excess Vec capacity and finite-word overflow rejection.
Eight existing physical-space tests cover the shared pressure refactor and the
previous autonomous operator behavior.

General mixed and nonuniform geometry, arbitrary nonpolynomial time callbacks,
boundary convergence and a literal 3D wake remain separate obligations. The
autonomous pipeline and the dated evidence above retain their original scope.
The later [generated polynomial-time box source and history](../../crates/quest-cfd/docs/generated-time-boundaries.md)
now implement complete owned-cell lifting, prescribed traces, body forces,
original-coordinate pressure and exact temporal-node drift evaluation without a
global field or chart matrix. Its separate focused MPI evidence does not promote
this earlier bounded-reference record to a time-dependent quantum solve.
