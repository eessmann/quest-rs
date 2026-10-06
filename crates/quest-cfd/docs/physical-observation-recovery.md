# Bounded physical observable and temporal recovery

`PreparedPhysicalObservable` captures the complete constrained physical coordinates of an already-built `PhysicalSpace` or `SimplexBdm`. It prepares shared MathCore sparse polynomials and floating/interval kernels. This is a bounded numerical reference preparation, not a scalable free physical oracle or a model reduction. The original model can be released after preparation.

The implemented observables are:

- Integrated enstrophy, `0.5 * integral |curl u|^2`, for full p1/p2 2D/3D box spaces and the full BDM1 simplex reference. There is no division by volume.
- A velocity component at a fixed physical point, using the reference's first-containing-cell DG trace.
- A pressure difference for full BDM1/P0 and BDM2/P1 box recovery, and the existing BDM1 simplex recovery. Interface probes average all incident cell pressure traces arithmetically. Closed-domain pressure uses the existing volume-weighted gauge; natural outflow fixes the pressure level on the cylinder reference.
- A mechanical boundary-force component with convention `integral(p n - nu grad(u) n)`. Simplex references use their existing labels; boxes use explicit `x-min/x-max/y-min/y-max/z-min/z-max` side names and typed `BoxBoundarySide` direct queries. This is unsymmetrized viscous traction with outward fluid-domain normal, not symmetric Cauchy stress, SIP penalty, convective momentum flux, drag coefficient, or lift coefficient.

`PreparedPhysicalObservable::polynomial_boundary` captures pressure or mechanical force at an explicit finite physical time, using the complete supplied lifting, prescribed trace, body force and original acceleration including `ell_dot`. Its fixed time, source category, physical/pressure degrees and gauge/trace/stress conventions remain in `provenance()`. Preparing a different time requires another snapshot. These metadata do not prove chart identity. The existing BDM1 simplex snapshot retains fixed boundary data (`scale=1`, `rate=0`). A scalable generated distributed pressure-observable source remains unimplemented by this module. See the [bounded pressure/traction evidence](../../../docs/verification/2026-10-06-bounded-pressure-observables.md).

## What the numerical snapshot proves

Velocity probes are affine and enstrophy, recovered pressure and boundary force are at most quadratic in the complete chart coordinates under these fixed reference contracts. Symmetric polarization uses zero, positive/negative unit coordinates, and four signed mixed-coordinate samples. All floating coefficients are then represented as exact binary64 dyadics in MathCore. Every coordinate remains declared, including variables absent from all nonzero terms.

Seven additional states compare the captured polynomial against the original physical reference before returning it. The receipt records the maximum scaled discrepancy and the specified acceptance tolerance. This is sampled consistency evidence, not a uniform bound on capture error or physical discretization error. Tests use further states, independent physical quadrature/reconstruction, and an analytic constant-velocity field. No coefficient threshold or truncation is applied.

`configuration(grid, limits)` requires the same complete coordinate count and constructs no global configuration-value table. It evaluates the interval kernel over the entire finite configuration box. Its finite range encloses the stored polynomial and the corresponding floating evaluation; it does not turn sampled physical capture error into a certified physical-error bound. Interval dependency can make the range loose. One scalar callback decodes a tensor index into a retained full-coordinate buffer and evaluates the prepared kernel.

This callback can supply the diagonal value for `PreparedHistoryInverse::reduce_probability_observable`: pass its retained bytes, query bytes and query work into the reducer's declared callback costs, and provide a deterministic source identity/provenance binding the chosen model, snapshot and grid. The callback is deterministic and contains no collective calls. The `RefCell` buffer makes this particular wrapper single-threaded; it reports reentrant borrowing as an error. Statistical confidence remains conditional on the existing sampling premises. Capture, discretization, temporal, preparation and inverse errors are separate systematic errors of the conditional normalized observable.

A probability-weighted result is a conditional KvN ensemble expectation. It is not automatically the observable of a deterministic physical state, and it is not the observable evaluated at the ensemble mean. No actual quantum measurements are executed by these pure recipes.

## Admission and receipts

For `m` complete coordinates, quadratic capture admits up to `1+m+m(m+1)/2` terms and `8+2m+2m(m-1)` reference evaluations (including the seven validation states). Affine capture admits `1+m` terms and `8+2m` evaluations. Checked arithmetic rejects overflow before capture.

`PhysicalObservableLimits` separately limits coordinates, reference evaluations, preparation bytes/work, retained-plus-query bytes, query work and sampled validation tolerance. Preparation envelopes cover numerical reference evaluation, complete exponent vectors, exact rational trees, both lowered kernels, coefficient conversion and overlapping buffers. Reference arithmetic is bounded conservatively using the full broken velocity dimension `N`: quadratic work for enstrophy/probes and cubic work for pressure recovery. Timed pressure captures also use the boundary owner's pressure/drift work and peak envelope. Actual complete box, boundary and simplex source capacities are included during preparation; already-completed source construction work remains separate. `SimplexBdm::retained_bytes()` counts its full chart, dense SIP, lifting, geometry and boundary Vec/String capacities. These are modeled arithmetic bounds, not timings or allocator metadata bounds.

All capture ceilings, including a conservative retained-kernel/query upper bound, reject before the first physical reference callback. Actual retained capacities are checked again after lowering. Full 3D P2 direct pressure/traction queries pass; default capture-work admission rejects its complete 49-coordinate cavity owner rather than preparing a smaller chart. The focused prepared-capture evidence covers 2D P1/P2 and 3D P1. Raising a declared ceiling is not execution or accuracy evidence for the unrun larger capture.

Actual kernel/exponent capacities and owned string capacity are charged. An oversized label is rejected before any reference callback. The MathCore `PolynomialKernel::variables` and `retained_bytes` accessors expose the complete input count and its admitted retained payload; they do not expose an allocation-free evaluation fiction. Receipts count the loop over all `m` input coordinates for every term, even for sparse exponents. Configuration-wrapper retained bytes include borrowed kernel/grid storage and its actual point-buffer capacity; query bytes and work remain additional. The grid wrapper also exposes its separate one-time range-enclosure work ceiling. Shared borrowed inputs may be counted conservatively more than once by enclosing receipts.

The larger full 3D p2 and cylinder-pressure test fixtures explicitly raise the preparation-work ceiling. A default budget rejection is an honest unsupported-resource outcome, not permission to retain fewer coordinates.

## Recovering amplitudes at a non-nodal physical time

`TemporalInterpolation::new(history, slab, fraction, limits)` selects a fraction `s` of an explicit slab. It implements the stored nodal Lagrange map

```
DG1: w = [1-s, s]
DG2: w = [(1-s)(1-2s), 4s(1-s), s(2s-1)]
psi_i(t) = sum_a w_a psi[(slab*q+a)*m+i].
```

The explicit slab distinguishes the two DG traces at a shared endpoint: `(slab,1)` and `(slab+1,0)` have the same physical time and different coefficients. The API exposes the endpoint side and full history semantics. It never silently averages the traces.

The scalar `amplitude` method is a bounded classical/reference query. It queries all temporal coefficients, preserves their complex phases and therefore their interference, and rejects nonfinite results. For example, opposite midpoint amplitudes cancel even though a probability mixture would remain nonzero. Apply the inverse's physical amplitude scale exactly once, before or after this linear map. Probabilities from separate time coefficients cannot reconstruct this interference.

`norm_upper_bound` outwardly bounds the Euclidean norm of the stored interpolation weights, which is the rectangular map's operator norm. It bounds amplification of whole-history amplitude error. It excludes consistency error from time discretization, arithmetic in the final scalar sum, and amplification from later conditional normalization. `TemporalInterpolationLimits` admits the callback's declared retained storage, scratch and repeated per-amplitude work; no callback is invoked for an invalid coordinate.

The separate [`TemporalEncoding` implementation](temporal-observations.md) uses this same rectangular map for coherent projection and has its own normalization, work/storage and whole-unitary tests. The scalar interpolation method itself is not a quantum implementation. Coherent temporal projection followed by a diagonal probability observable requires conditioning on the projection's success as well as the original inverse success; a temporal mixture is not substituted for that operation.

The [CPU/MPI composed postselection simulation](distributed-temporal-observation.md) now connects an existing prepared inverse, the coherent temporal encoding and these typed diagonals. Its projection and probability semantics are explicit; no sampled measurements are executed.
