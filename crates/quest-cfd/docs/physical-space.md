# Complete affine BDM1/P0 and BDM2/P1 physical spaces

`PhysicalSpace` assembles all broken vector polynomial coefficients on a uniformly triangulated square or tetrahedralized cube, then constructs the complete divergence-free, normal-continuous chart. The [closed affine constructor](affine-mesh.md) reuses that assembly for admitted unequal-cell triangular and tetrahedral meshes. `order=2` changes the physical space; it never routes to the first-order implementation. The original `SimplexBdm` remains an independent first-order reference.

## Space, mapping and ordering

On each affine simplex the velocity space is `[P_p]^d`, for p=1 or 2. Scalar first-order shape functions are barycentric coordinates. Quadratic shape functions are `lambda_i*(2*lambda_i-1)` at vertices and `4*lambda_i*lambda_j` on edges. Their construction and differentiation use shared MathCore sparse polynomials; lowered kernels supply every assembled value and gradient. The velocity ordering is cell-major, then Cartesian component, then scalar node: vertices first and lexicographically ordered vertex pairs second.

BDM has polynomial normal traces on facets, contravariant Piola mapping and interior moments. In degree two its local vector dimensions are 12 on triangles and 30 on tetrahedra; the interior contributions have dimensions 3 and 6. These definitions agree with the maintained [DefElement BDM entry](https://defelement.org/elements/brezzi-douglas-marini.html), which cites [Brezzi, Douglas and Marini (1985)](https://doi.org/10.1007/BF01389710). The DOI landing page was unavailable to the browsing tool; the definition entry was read directly.

For an affine map `x=A*xhat+b`, contravariant Piola is `u=A*uhat/det(A)`. This is an invertible map between the complete reference and physical vector polynomial spaces. A physical Cartesian nodal basis therefore spans precisely the same complete space, though it is not the canonical moment-dual basis. The tests independently check physical interpolation, divergence scaling and the surface-Jacobian normal-flux identity on sheared triangles and tetrahedra. Retaining the entire broken vector polynomial space preserves all interior modes.

Normal constraints use a common physical normal and the face-node permutation from mesh topology. Evaluating the degree-p trace at all facet interpolation nodes is unisolvent and equivalent to equality of every degree-p normal moment. Cell divergence constraints integrate against all P0 or P1 pressure functions. Since the divergence itself belongs to that pressure space, these constraints enforce pointwise polynomial divergence zero.

Rows are ordered by mesh faces and facet interpolation nodes, followed by cells and pressure functions. P1 pressure uses cell vertices in barycentric order. The unmodified constraint list includes the single closed-domain pressure-gauge dependency.

## Complete chart and operators

The scalar local mass is integrated and Cholesky factored once; physical cell volume scales it. Constraints are transformed with each cell mass inverse square root. Pivoted, twice-reorthogonalized row elimination determines the numerical rank, and a complete orthogonal complement retains exactly `broken_dimension-rank` independent coordinates. An ambiguity interval between 1e-11 and 1e-8 causes rejection. Every constructed chart is checked against the full constraint matrix and mass matrix; these are floating-point residual certificates, not symbolic proofs of rounded linear algebra.

Central conservative convection uses the volume term `integral u_i*(u.grad(v_i))` and the centered velocity trace multiplied by the single-valued normal velocity. An independent advective-volume implementation supplies a comparison. Their equivalence and the zero inviscid energy contraction are tested on complete random states in 2D and 3D.

Symmetric interior penalty viscosity uses volume gradients, both symmetric consistency terms and penalty `10*(p+1)^2/h_face`, where `h_face=d*min(cell_volume)/face_measure`. Boundary consistency uses the one-sided gradient and prescribed tangential velocity. The p1 operator agrees with the independent existing implementation, including cavity boundary forcing. The p2 tests verify dissipation, nonzero lid acceleration and mixed momentum reconstruction.

Tensor Gauss rules under Duffy mappings use four points per coordinate for polynomial operators. Volume convection has degree at most five and facet convection degree at most six; including the Duffy Jacobians, these rules integrate the assembled polynomials exactly in exact arithmetic. Projection and analytic-reference error measurements instead use eight points per coordinate to reduce nonpolynomial quadrature error. Reported analytic errors remain quadrature measurements.

Pressure recovery uses `force-M*acceleration`, eliminates one pressure coordinate using the volume-weighted zero-integral gauge, and solves the resulting independent constraint transpose through row QR. It reports physical pressure with the weak-gradient sign, normal multipliers, complete momentum residual, continuity residual and gauge residual. The full P1 pressure is retained; it is not replaced by a cell mean. `gradient_dissipation` reports `nu*integral |grad u|^2`; it excludes SIP face terms. Enstrophy reports half the integral of squared physical vorticity.

## Admission and evidence

The present constructor is an explicitly bounded dense reference, limited to 768 broken velocity coefficients before geometry allocation. It supports uniform order 1 or 2 on periodic and constant-lid cavity boxes. It rejects mixed boundaries and oversized spaces. Cell mass and transposed-constraint block accessors expose the reference assembly for independent distributed-chart comparisons; they do not constitute a production source that avoids global constraint storage. Distributed Householder charts are a separate implementation.

Classical RK4 preserves every independent coordinate. It admits at most one million steps and one billion conservatively modeled scalar work units, including dense and volume/facet terms. It rejects arithmetic overflow. This is a classical reference, with no inference of quantum execution from dimension or register estimates.

Focused tests establish:

| Mesh and physical order | Broken velocity dimension | Constraint rank | Complete independent dimension |
| --- | ---: | ---: | ---: |
| Periodic square, two triangles, p2 | 24 | 14 | 10 |
| Periodic cube, six tetrahedra, p2 | 180 | 95 | 85 |
| Cavity square, two triangles, p2 | 24 | 20 | 4 |
| Cavity cube, six tetrahedra, p2 | 180 | 131 | 49 |

For the divergence-free periodic field `(cos(2*pi*y),0,0)`, measured L2 projection errors are:

| Dimension | Subdivisions per axis | Physical order | L2 error |
| ---: | ---: | ---: | ---: |
| 2 | 1 | 1 | 7.071060445501e-1 |
| 2 | 2 | 1 | 8.504617451336e-2 |
| 2 | 1 | 2 | 1.950122099268e-1 |
| 2 | 2 | 2 | 8.344813488355e-2 |
| 2 | 4 | 2 | 8.153189274231e-3 |
| 3 | 1 | 1 | 5.540062010346e-1 |
| 3 | 2 | 1 | 8.443299733872e-2 |
| 3 | 1 | 2 | 1.916928738277e-1 |

These demonstrate h and p improvement, including the modest p improvement at the intermediate 2D mesh; no universal asymptotic rate is inferred. The 3D two-subdivision p2 mesh has 1440 broken coefficients and is explicitly rejected by this dense backend. RK4 temporal refinement is separately checked against a finer complete-state trajectory.

The [bounded affine API](affine-mesh.md) now supports unequal-cell closed Dirichlet manifolds and fully periodic axis-aligned boxes, with independent mass, pressure and inclusion tests. General mixed or natural closure, curved elements, mixed order, physical orders above two and generated distributed arbitrary meshes remain unsupported. Complete polynomial-time normal lifting, prescribed traces and body force are supported by the bounded `PolynomialBoundary` reference and the [generated owned-cell source and history](generated-time-boundaries.md), with full compatibility checks and the lifting derivative retained. Nonpolynomial time data and moving geometry remain unsupported. The separate simplex reference also supports [scalar affine boundary lifting](method-and-theory.md#scalar-time-dependent-boundary-lifting). The literal three-dimensional wake still lacks its required far-field pressure and convective-outlet closure. Those remain separate acceptance items. There is no coherent sparse pressure oracle claim. The surrounding full-coordinate KvN workflow is motivated by the [unitary KvN discretization paper](https://arxiv.org/html/2605.19187v1); this bounded physical reference is not presented as an exact reproduction of every spatial scheme in that paper.

The [bounded physical campaign](../../../docs/verification/2026-10-05-dual-history.md#physical-order-and-nonlinear-reference-campaign)
records short classical mesh/order/time/geometry probes, with explicit rejected
cases. It does not establish published-window profiles or force convergence.
The separate [bounded cylinder window](../../../docs/verification/2026-10-05-cylinder-window.md)
completed the classical eight-second DFG reference and `[4,8]` observation window.
Its coarse geometry yields nearly constant lift and no admitted Strouhal candidate;
published benchmark convergence remains open.

## Generated constraints and distributed complete charts

`BoxConstraintRecipe` constructs fixed element mass/divergence/normal tables and
implicit box topology for both orders. Scalar queries generate only the requested
cell coefficient. No complete mesh, constraint matrix or dense basis is retained.
All local facet traces are supplied, including duplicate interior constraints,
followed by every cell pressure mode. This preserves the full kernel while changing
multiplier coordinates. A decided numerical rank must agree with the independent
topological nullity; ambiguous rank remains an explicit outcome.

With `--features distributed`, `prepare_collective` consumes the source in
`CollectiveEnvironment::prepare_constraint_chart_from_fn`. Cells are mass-whitened
locally; distributed pivoted Householder QR retains local reflectors and dense local
fill, with replicated column labels. It does not construct global Q. Full null-tail
lifting, force projection and constraint/multiplier operations perform globally
coupled work. Source callback work, retained source bytes, query scratch and control
traffic are included in admission alongside factor storage and communication.

The implementation is tested against complete bounded dense projectors for both
orders in 2D/3D periodic/cavity boxes at 1/2/4/8 ranks and split communicators.
Huge implicit-grid scalar queries demonstrate fixed source storage only; infeasible
factorization still rejects. `PivotedDependentZero` multipliers are not physical
pressure. The physical gauge recovery below is a separately admitted operation;
generated nonlinear force assembly and actual multi-host capacity tests remain
separate obligations.


## Distributed physical pressure from a broken force

`PreparedBoxConstraints::recover_pressure` accepts only the local complete-cell
portion of an arbitrary broken momentum force. It retains all physical coordinates.
It uses four globally coupled chart operations: force projection into the null
coordinates, null lifting to the physical acceleration, generic multiplier recovery,
and reconstruction of the constraint force from the gauged multipliers.

For `M*a + C^T*lambda = f`, the reported physical pressure is `p=-lambda_p`.
The recipe supplies a null vector of `C^T` whose pressure entries are `-1` and
whose normal entries are the exact facet nodal-basis integrals, halved for duplicated
interior/periodic rows. Adding the volume-weighted mean of `lambda_p` times this
vector centers physical pressure while compensating every normal multiplier.
Thus the reported original momentum check uses the full original `C^T`, including
all duplicate facet rows. Pressure retains every P0/P1 coefficient; no cell-average
replacement occurs. Both the complete momentum residual and pressure integral are
checked against a caller-selected relative tolerance. These are floating-point
residual checks, not certified pressure error bounds.

Admission includes all four native query work/transport bounds, generation of the
local mass coefficients and gauge weights, and conservative control reductions.
The combined rank peak includes the retained source/chart, the accessible input
force payload, output and fixed scratch, and two simultaneous native query
reservations. The node bound multiplies the largest rank peak by the declared ranks
per node. A retained external reservation keeps the output allocation visible to
subsequent native admissions. MPI internals, allocator metadata and unrelated
caller-owned backing storage are outside this modeled payload bound.

Tests independently manufacture `f=-B^T*p + 0.13*M*q` from the bounded dense
reference's unique-face pressure constraints, quadrature mass and a full null-chart
column. They recover the known pressure minus its weighted mean for both orders,
both dimensions and both box boundary families on 1/2/4/8 ranks and split
communicators. A separate pure test verifies `C^T*g=0` for every broken momentum
coefficient. Collective malformed-input and one-less work/transport/rank/node
budgets reject without leaking reservations. This implements distributed pressure
recovery from supplied forces; it does not yet generate the nonlinear force or
provide a coherent quantum pressure oracle.

The supplied-force wrapper passed independent source review and a separate rerun
of the pure gauge and MPI 1/2/4/8-rank/split-communicator tests. Repeat its scoped checks with
`cargo test -p quest-cfd --features distributed --test constraint_pressure --test constraint_pressure_collective`.
The [manufactured collective fixture](../tests/constraint_pressure_collective.rs)
is a bounded independent pressure reference, rather than a nonlinear CFD run.

## Three-dimensional cavity sample diagnostics

Both BDM1 and BDM2 classical `cavity3d` snapshots include `cavity_3d`.
`cavity_3d_probes` samples all three velocity components on the planes `x=1/2`
and `y=1/2`, at the 81 interior combinations of the other coordinates in
`{0.1, 0.2, ..., 0.9}`. These transverse planes expose the velocity components
needed to inspect secondary circulation. Their spanwise-velocity RMS is the
unweighted RMS of `w` across the 162 samples, including the duplicated
intersection line. It is not a volume integral or a vortex-strength measure.

For 324 pairs with `x,y` on the same nine-point grid and `z` in
`{0.1,0.2,0.3,0.4}`, the partner is `(x,y,1-z)`. Reflection acts on a vector as
`(u,v,w) -> (u,v,-w)`, so the defect is
`(u(x,y,z)-u(x,y,1-z), v(x,y,z)-v(x,y,1-z), w(x,y,z)+w(x,y,1-z))`.
The report gives the RMS Euclidean defect per pair and its maximum. A nonzero
spanwise component can satisfy this symmetry exactly. Statistics must be finite;
malformed, nonfinite and overflowed callback results reject.

There are 810 scalar sample queries, evaluated by one bounded batch callback.
The helper retains only this fixed stencil and the two output planes. The
underlying full-state reconstruction, callback storage and evaluation work
remain the classical model's cost. The current references use their existing
first-containing-cell trace convention. On facets, the tangential DG trace and
mesh orientation can affect the sampled defect; this is not a certified norm
of the continuum symmetry. The fields are absent for all other families.

Three tests compare exact parity and broken parity against analytic sample
fields, reject invalid samples, and exercise complete BDM1/BDM2 cavity references.
These tests do not establish a steady Re100/Re1000 solution or replace the
published-window, spatial and temporal benchmark refinements.
