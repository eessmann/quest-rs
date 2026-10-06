# Bounded mixed Dirichlet and mechanical-traction spaces

`PhysicalSpace::from_mixed_mesh` and its attempted-receipt variant construct the complete BDM1/P0 or BDM2/P1 space on connected, nonperiodic affine triangle/tetrahedron manifolds. `MixedAffineMeshView` supplies every exterior facet, its label, and either `ExteriorCondition::Dirichlet` or `NaturalMechanicalTraction`. Both kinds must occur. The existing exact conformity, manifold links, interval quality admission, scalar basis, quadrature and complete mass chart are shared with [closed affine meshes](affine-mesh.md).

This is a bounded classical/reference source. Curved, hanging, mixed-order, periodic-plus-natural, all-natural and distributed arbitrary meshes are unsupported. There is no DFG consumer in this stage. The unresolved literal 3D wake is a separate boundary specification; this closure does not replace it.

## Boundary and pressure meaning

The prescribed traction is

```text
tau = nu grad(u) n - p n
```

for the outward **fluid-domain** normal and unsymmetrized gradient stress. Its weak load is `+ integral tau dot v`. It is not multiplied by viscosity again. Natural faces have no Dirichlet SIP terms and no normal-velocity constraint. The central transported trace there is the interior velocity. Interior and Dirichlet operators keep their previous conventions. Observed force on a solid uses the opposite sign `p n - nu grad(u)n`.

This convention agrees with the [official DFG2D2 outlet](https://wwwold.mathematik.tu-dortmund.de/~featflow/en/benchmarks/cfdbenchmarking/flow/dfg_benchmark2_re100.html), but agreement of boundary definitions is not benchmark convergence. The official developed-cycle comparison uses 25–30 seconds; historical project observations over 4–8 seconds remain coarse diagnostics.

Let `t` be the number of complete velocity facet nodes, `q` the pressure modes per cell, and `N` the original broken velocity dimension. All `R=t(F_interior+F_D)+q C` rows are retained. The supported mixed space requires measured full row rank `R`; its complete kernel has dimension `N-R`. The closed pressure dependency is absent. No velocity coordinate or pressure coefficient is removed. A retained per-face map records normal-row offsets and the separate Dirichlet/natural table indices.

`reconstruct_pressure_general` on `PhysicalSpace` and `PolynomialBoundary` returns the original momentum and continuity residuals plus:

- `PrescribedMechanicalTraction`: all pressure coefficients, signed pressure integral, and `normalization_residual=None`. The supplied traction fixes the pressure level; no mean is subtracted.
- `ZeroVolumeMean`: the unchanged volume-weighted closed gauge, including its numerical residual.

The existing `reconstruct_pressure` keeps its closed mean-zero contract and rejects mixed sources. Pressure coefficients from the general report can be sampled with the existing bounded `sample_pressures_with_limits`. Box-side force selectors remain box-only. No new arbitrary-boundary force-observation selector is claimed here.

## Time data and canonical lifting

`BoundaryTimeCoefficient` retains all broken lifting and body-acceleration coefficients. Its prescribed values use `dirichlet_facets()` order and **all** P1/P2 facet nodes. `NaturalTractionTimeCoefficient` uses `natural_traction_facets()` order. `PolynomialBoundary::with_natural_traction` requires matching time-mode counts and exact nested shapes; an empty traction owner denotes zero load. Values are coefficients of powers of physical time. Nonfinite data and nonzero unused 2D components reject.

The supplied full lifting is preserved in `u=Q a+ell(t)`, including any homogeneous component. Every divergence, interior continuity and prescribed Dirichlet normal condition is checked coefficientwise. Natural velocity traces are free and can balance prescribed inflow. Drift includes `-Q^T M ell_dot`. General pressure uses the **original** force and acceleration `Q a_dot+ell_dot`. Shared MathCore polynomial extraction consumes this full drift, including all natural-load and time modes.

`canonical_liftings(&Vec<Vec<Vec<[f64; 3]>>>, limits)` provides a particular field for an entire batch of Dirichlet trace modes. It factors `A=C B`, where `B^T M B=I`, once. Row-normalized QR stores `A_normalized=T Z^T`; the lifting solves `T y=d/row_scale` forward and returns `ell=B Z y`. Pressure uses the distinct transpose solve `T^T scaled_lambda=Z^T force`. Neither forms normal equations. The returned fields have numerical certificates for `C ell=d` and `Q^T M ell=0`. These are minimum-mass particular fields; they do not replace or alter explicit user-supplied liftings.

## Energy and evidence

The actual central convective power for an exactly divergence-free, normal-continuous field is

```text
u^T F_conv = -1/2 integral_N u_n |u|^2
             -1/2 integral_D u_n (u dot g).
```

The second term uses the weak prescribed trace, not an assumed exact tangential boundary value. Backflow can inject energy; no backflow stability guarantee is asserted. Full energy balance with a nonhomogeneous lifting also includes Dirichlet reaction work.

The [fixed rational verification](../../../docs/verification/2026-10-06-mixed-traction.md) proves the single-simplex rank expectations and an exact natural energy polynomial independently of production assembly. Maintained Rust tests additionally cover:

- Single and unequal two-cell triangles/tetrahedra at both orders, all coordinates, independent rational barycentric mass tables, and an independently assembled BDM1 comparison in original coefficient coordinates.
- Constant nonzero pressure from `tau=-P n`, affine pressure from a body gradient, and the original momentum residual without mean subtraction.
- Polynomial-time extension `ell=t(x,-y,0)`, its full acceleration, matching body force and traction at off-node times, and a retained last homogeneous coordinate.
- Minimum-mass lifting with nonzero prescribed inlet flux balanced by the free natural trace; batched work and actual excess-capacity rejection.
- Exact signed natural energy transport and separately integrated volume/Dirichlet SIP dissipation, shared polynomial extraction, malformed loads, pressure admission and unchanged closed regressions.

These tiny numerical checks are not spatial/time convergence, developed wake acceptance, a numerical rank certificate for arbitrary meshes, or quantum execution.

## Managed resources

Construction keeps the existing 256 MiB / 1 billion default admission and broken dimension ceiling 768. A fixed 32 KiB borrowed mixed-view adapter is charged throughout its lifetime, including the geometry phase; caller source slices and the retained face map are separate. Actual constraint counts enter the existing complete QR/chart/mass-Gram ledger. Geometry-only source identity includes framed mixed-kind data but excludes viscosity/order, and is not an operator fingerprint. Failed attempts preserve only available planned/admitted/executed evidence.

For canonical lifting, with batch size `K`, scalar cell width `s`, and trace width `t`, the checked whole-batch arithmetic bound is

```text
128 R^2 N + 64 R N s + 128 K (R^2 + R N + N^2 + F_D t d).
```

It covers twice-reorthogonalized factorization, whitening, repeated forward solves, full unwhitening, mass application and original residual checks. The scratch allowance is

```text
8 (3 R N + R^2 + 32 N + 8 R)
+ (4 R + 4 N) sizeof(Vec<f64>) + 4096 bytes.
```

Whole-live admission adds the retained physical source, mesh-declared plus explicitly additional external storage, actual outer/nested input capacities, and every returned `K*N` coefficient buffer. QR rows/basis/triangular/scales and repeated RHS/white/field/mass actual capacities are reconciled against scratch; returned capacities are also checked. The default batch is at most nine modes (an explicit limit can admit up to 65), with 256 MiB / 1 billion work. Limits never reset per mode.

Natural load ownership counts actual outer and nested capacities. The time-owner peak includes mesh-declared external storage, reported separately when nonzero; this propagates into full polynomial extraction admission. In addition to the existing time-source ledger, the source charges `128 K F_N L + 512 H L` work per drift (and constructor), where `L` is full local velocity width and `H` all volume/facet quadrature samples. This bounds complete mode/trace interpolation and natural quadrature loads. Pressure retains its combined source/time-owner, original-force/acceleration, full-row QR and repeated-query envelopes, admitted before preparation; actual QR capacities are reconciled. Old box receipts keep their existing optional mesh-field behavior.

These are conservative managed arithmetic/payload models, not RSS, allocator metadata, instruction counts or process-enforced quotas. Existing MathCore host-profile environment lookup limitations and separate OS-budget requirements still apply. No oracle, field table or repeated source query is declared free.

Focused commands:

```sh
cargo test -p quest-cfd --test mixed_physical_mesh --test mixed_physical_boundary
cargo test -p quest-cfd --test physical_mesh --test physical_order --test physical_boundary --test physical_pressure_observation
cargo clippy -p quest-cfd --lib --test mixed_physical_mesh --test mixed_physical_boundary --no-deps -- -D warnings
python3 -B docs/verification/fixtures/quest-cfd/mixed_traction_exact.py
```

The bounded [complete P2 cylinder snapshot consumer](cylinder-high-order.md) applies this same mixed closure to a separately versioned represented rectangle/polygon source. It provides supplied-state observations, without time integration or developed-cycle acceptance.
