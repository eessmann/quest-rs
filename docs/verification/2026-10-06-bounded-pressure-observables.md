# Bounded full pressure and mechanical-traction observables

This stage extends the existing complete BDM1/P0 and BDM2/P1 bounded physical reference. It supplies pressure sampling, exterior mechanical force, autonomous prepared captures and fixed-time polynomial-boundary captures. It neither constructs a distributed pressure source nor establishes physical convergence or force coefficients.

## Physical conventions

`PhysicalSpace::sample_pressures` accepts cell-major P0 constants or P1 vertex-barycentric coefficients from `PhysicalPressureRecovery`. At a facet, edge or vertex it returns the arithmetic mean of every incident fluid-cell trace, with the reference's `1e-10` barycentric inclusion tolerance. Interior P1 evaluation is barycentric interpolation. No smoothing, volume weighting of incident traces or change of gauge occurs. Closed-domain recovery retains volume-weighted zero mean.

`boundary_force` integrates the interior physical stress quantity

`p n - nu grad(u) n`

over a typed `BoxBoundarySide`, with the outward fluid-domain normal and mechanical fluid-on-boundary sign. Prepared observables use the canonical labels `x-min`, `x-max`, `y-min`, `y-max`, `z-min`, `z-max`. There is no implicit match to the box mesh's empty face labels. Periodic boxes have no exterior force sides. The existing fixed facet quadrature, prepared P1/P2 velocity gradients and retained cell barycentrics supply P0/P1 pressure traces without symbolic work inside queries.

This convention uses the existing unsymmetrized Laplacian viscosity. It excludes symmetric-gradient stress, SIP penalties, convective momentum flux and drag/lift nondimensionalization. For a permeable prescribed face, it is mechanical traction rather than the complete control-volume momentum flux.

## Full original coordinates and fixed time

Autonomous capture calls the existing full physical pressure recovery. `PreparedPhysicalObservable::polynomial_boundary(problem,time,kind,limits)` calls the original boundary recovery with

`force - M(Q a_dot + ell_dot)`.

Mechanical traction uses `coefficients_at(time,a)`, including every supplied homogeneous and inhomogeneous component of the lifting. A homogeneous acceleration or state is not substituted. Time is an explicit finite parameter; negative time remains allowed by the existing polynomial reference contract. It is not another physical/configuration coordinate.

At a fixed time, full momentum force and acceleration are quadratic in all chart coordinates. The mixed pressure recovery is linear in their residual; pressure and mechanical traction are therefore at most quadratic. Complete numerical polarization captures these functions into exact recorded binary64 dyadics and shared floating/interval kernels. This does not prove exact agreement with the original floating assembly or a uniform physical error bound. Seven unused validation states enforce the supplied discrepancy tolerance. Every declared coordinate remains, including variables absent from nonzero terms.

`provenance()` records source category, physical dimension, velocity order, pressure degree, fixed time, gauge and pressure/stress conventions. It is semantic metadata, not a cryptographic or mathematical proof of chart identity. The original model can be released after successful preparation; the resulting snapshot cannot silently change time.

## Admission

Direct probes reuse `PressureProbeLimits`; mechanical force has `MechanicalTractionLimits`. Checked limits precede pressure scans and physical coefficient reconstruction, and actual output/coefficient capacities receive a further check. Supplied unused buffer capacities and the prepared mesh remain caller-owned for direct box queries. Timed traction conservatively also charges the complete boundary peak envelope.

For `m` coordinates, capture admits at most `1+m+m(m+1)/2` terms and `2m*m+8` reference evaluations. Its limits include source Vec capacities, full pressure recovery scratch, coefficient/time reconstruction, facet integration and overlap with exact polynomials and both lowered kernels. Existing conservative cubic pressure work is combined with the timed owner's declared pressure/drift work; every reference call is charged. Source construction already completed before capture is separate.

Every capture ceiling now rejects before the first physical reference callback. The preflight includes a conservative retained-kernel/query bound; actual capacities are checked again after lowering. A regression initially showed `max_bytes=0` invoking the reference three times before rejection; it now rejects with zero callbacks. Review additionally exposed the older simplex adapter's caller-owned source exclusion: `SimplexBdm::retained_bytes()` now counts its complete geometry, chart, dense SIP, lifting and boundary payloads, and capture admits these actual capacities. A regression inflating unused chart-row capacity verifies the exact byte increment and rejection under the prior preparation ceiling. No pressure factorization cache is introduced.

These bounds model managed numerical payload/arithmetic, excluding allocator, operating-system and MPI metadata or measured RSS. More generous bounds do not prove execution capacity or physical accuracy.

## Focused evidence and limits

Six new tests cover:

- Independent P0/P1 incident traces, affine P1 interpolation and oriented constant/affine pressure face integrals in 2D/3D, P1/P2.
- Original recovery for a manufactured compatible time-dependent field `ell=t*(x+0.3,-y,0)` with supplied full trace and body load `ell_dot+(ell dot grad)ell+grad p`, where `p=(1+t)*kappa*(x-1/2)` for P2. P1 uses `kappa=0` because its pressure space is P0. The recovered P1 pressure and original momentum/gauge agree within test tolerances; x-max mechanical force equals `kappa*(1+t)/2 - nu*t` on the unit box.
- Complete autonomous/fixed-time captures for 2D P1/P2 and 3D P1, checked on additional full-coordinate states, including the final coordinate.
- Full 49-coordinate 3D P2 direct pressure and traction queries, plus explicit default capture-work rejection. An admitted larger 3D P2 prepared capture has not been executed by this receipt.
- Nonfinite time/data, malformed pressure modes, absent sides, unsupported labels and work/storage/source-overlap failures. Private callback-count regressions verify pre-callback admission.

Reproduce from the repository root:

```sh
cargo test -p quest-cfd --test physical_pressure_observation \
  --test physical_observation --test physical_boundary --test physical_order
cargo test -p quest-cfd --lib physical_observation::tests
cargo clippy -p quest-cfd --lib --test physical_pressure_observation \
  --test physical_observation --test physical_boundary --test physical_order \
  --no-deps -- -D warnings
```

The [existing observable contract](../../crates/quest-cfd/docs/physical-observation-recovery.md) retains probability-weighted ensemble semantics and systematic-error limits. Generated distributed pressure/traction sources, mixed/nonuniform/outflow geometry, literal wake boundaries, uniform capture-error certification, measurement campaigns and resolved physical force/pressure convergence remain open.
