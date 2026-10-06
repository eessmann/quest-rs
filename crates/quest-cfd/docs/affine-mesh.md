# Bounded closed affine meshes

The [dated verification record](../../../docs/verification/2026-10-06-affine-mesh.md)
collects executed checks, resource examples and remaining acceptance limits.

`PhysicalSpace::from_mesh` accepts an explicit `AffineMeshView` at uniform BDM1/P0 or BDM2/P1 order in two or three dimensions. It uses the same mass, full constraints, complete mass-orthonormal chart, central convection and SIP viscosity assembly as `box_mesh`. Unequal cell volumes are supported. No chart coordinate, including periodic means, is removed to meet a budget. This is a bounded dense classical/reference constructor, not a distributed mesh source.

The initial topology contract is deliberately narrow:

- A connected embedded simplicial manifold with every exterior facet explicitly marked homogeneous Dirichlet. Vertex links must be paths/cycles in 2D and triangulated disks/spheres in 3D; 3D edge links must also be paths/cycles. Connected cell adjacency and at most two cells per facet alone are insufficient.
- A conforming triangulation of an axis-aligned box with every exterior facet paired to an opposite side by an explicit ordered vertex correspondence. The displacement is checked exactly on represented binary64 coordinates. Quotient vertex IDs are not collapsed, so ordinary torus loops are retained.

Mixed closure, unspecified/natural boundaries, general periodic seams (including sheared parallelograms), hanging faces, curved cells, mixed order, disconnected and nonmanifold sources are rejected. A zero-dimensional homogeneous velocity kernel is also unsupported by the inherited reference chart constructor; it is rejected, rather than replaced with a reduced state. This API does not implement the literal open wake boundary condition.

## Geometry and numerical evidence

The shared `MathCore` dyadic predicates validate each simplex and every unordered cell pair. Distinct cells may meet only at their complete shared simplex identified by source vertex IDs. Each periodic correspondence additionally uses exact displacement equality. The wrapper charges all standalone, pair and displacement slots, including fast-path calls. It also computes an outward interval lower bound on `abs(det J) / max_edge_L1^d`; a nonpositive, ambiguous, overflowing or insufficient lower bound rejects before basis assembly.

Exact geometric conformity is distinct from floating numerical chart rank and conditioning. Runtime rank/nullspace results are numerical evidence, not a universal exact rank certificate. The independent rational dimensions for the first four mesh rows below use invertible modular denominators, a full-rank minor modulo a prime and an exact oriented pressure/normal-trace row dependency. The fifth, red-refinement row is separate numerical dimension/inclusion evidence. The rational fixture argument is not applied to arbitrary caller meshes.

`source_identity` is versioned, framed geometry-only provenance: fixed little-endian u64 counts/IDs, source coordinate bits, ordered cells/facet correspondences and labels. Signed-zero bits are retained. Viscosity and polynomial order are excluded, so this identity alone is not an operator identity or a proof of source equality.

## Admission and receipts

`PhysicalMeshLimits` supplies separate counts, input bytes, geometry arithmetic precision/work, construction and pressure-query ceilings. Defaults are 256 MiB managed payload, 1 billion constructor work units and 1 billion pressure work units, with a hard maximum of 768 broken velocity coefficients. Input charging counts accessible slices and labels; callers declare hidden backing capacity or additional live owners in `external_retained_bytes`.

`max_geometry_bytes` bounds one `MathCore` predicate's arithmetic payload. The whole geometry phase, including accessible input, external owners, facet-plan capacity, the predicate payload and a conservative stack allowance, is reported as `geometry_peak_bytes` and checked against the overall `max_bytes`. Geometry work is aggregated across all calls and conditioning checks. It is not a free oracle.

The model reserves 80 MiB for the existing shared basis construction and temporary owners. It separately charges conservative nested topology frames at 128 KiB (the earlier 64 KiB proposal was too small when all nested frames were counted). The early 80 MiB byte admission already covers topology before the basis runs. Receipts conservatively retain the stack allowance in later phase envelopes; this is not a measurement of simultaneously live stack pages. Dense constraints, whitening/QR/nullspace arrays, mass-Gram certification and nested quadrature tables have explicit whole-live allowances. Returned `Vec`/`String` capacities are audited, together with admitted `MathCore` polynomial payloads. Big-integer allocator internals, allocator metadata, compiler stack layout and measured RSS are not exact outputs of this model.

The arithmetic resource contract assumes the supported immutable host multiplication/squaring profile: threshold values are absent or at most 32 ASCII bytes and satisfy the geometry layer's minimums (24 for multiplication and 30 for squaring). Malformed, oversized and unsupported values reject. Standard-library environment discovery and the underlying arithmetic library's own environment lookup may allocate or scan in proportion to an oversized host value before rejecting it. That host-configuration cost is **outside** the predicate arithmetic payload/work model; these limits do not hard-bound an arbitrary malformed host environment or total process memory. Operating-system memory/time limits remain separate.

`from_mesh_with_receipt` returns the latest attempted/planned phase and available evidence even when later admission or numerical work rejects. Phase names distinguish a planned geometry envelope from admission and validation. Rank, independent dimension, retained bytes and quality are optional until established; no successful source is returned on rejection. No failure automatically enlarges its limits.

Let `C` be cells, `I=C(d+1)` incidences, `s` scalar velocity nodes, `L=d*s`, `N=C*L`, `F` logical facets, `R` constraint rows, `V=C*4^d` and `H=F*4^(d-1)`. The constructor's declared work is

```text
160 Mi + 1,000,000 + 64 I² + 128 I C²
+ 32 R² N + 12 N³ + 256 (V+H) L² + 64 R N + 64 N² s
+ aggregate exact-geometry/conditioning work.
```

These are conservative admitted arithmetic units, not processor instructions or measured elapsed time. The separate original-pressure work envelope is `128 R² N + 2*(64 N² + 512*(V+H)*L)`, with source-retained bytes and QR/momentum/query scratch. The constructor's borrowed mesh input may be dropped before a pressure call and is not automatically retained in that query receipt; `external_retained_bytes` persists as the caller's declaration. Polynomial-boundary pressure additionally charges its actual retained coefficients, preparation scratch and repeated drift work before starting that preparation.

## Full pressure and boundary semantics

`geometry_kind()` distinguishes these sources from historical box selectors. `boundary()` remains legacy layout metadata (`Mixed` for explicit meshes); a private validated closed-topology proof controls admission to `PolynomialBoundary`. No arbitrary mesh is silently labeled a cavity. Box-side traction selectors reject non-box geometry, even if an explicit mesh happens to describe a box. Existing generic physical pressure probes remain available under their own query limits.

Pressure is recovered from the original momentum equation, with `p = -lambda_pressure`, using the complete force and acceleration. Its gauge is the physical volume-weighted integral, not an unweighted coefficient mean. `PolynomialBoundary` preserves supplied lifting, its time derivative and all homogeneous coordinates. Its pressure query retains the original `M ell_dot` contribution and accounts for the lifting owner. This does not introduce natural-boundary pressure closure or general mesh traction selectors.

## Independent bounded fixtures

| Source | BDM1: broken / rank / full coordinates | BDM2: broken / rank / full coordinates |
|---|---:|---:|
| Two unequal trapezoid triangles, areas 1 and 1/2 | 12 / 11 / 1 | 24 / 20 / 4 |
| Two unequal bipyramid tetrahedra, volumes 1/6 and 1/3 | 24 / 22 / 2 | 60 / 49 / 11 |
| Periodic four-triangle unit square, center (0.3,0.4) | 24 / 15 / 9 | 48 / 29 / 19 |
| Periodic twelve-tetrahedron unit cube, strip cut at 1/3 | 144 / 83 / 61 | 360 / 191 / 169 |
| Red refinement of the closed two-triangle source | 48 / 39 / 9 | 96 / 71 / 25 |

The triangle source has vertices `(0,0),(2,0),(1,1),(0,1)` and diagonal from the first to third vertex. The bipyramid has shared base `(0,0,0),(1,0,0),(0,1,0)` and tips `(0,0,1),(0,0,-2)`. All monomial barycentric mass integrals are independently evaluated using `integral lambda^alpha = d! volume product(alpha!)/(d+sum(alpha))!`, including all chart cross-products. Red refinement and order elevation reproduce every coarse full-coordinate field and its mass energy.

Manufactured body gradients recover `p=x-7/9` on the trapezoid and `p=z+1/4` on the bipyramid, including signs, P0/P1 coefficients, original momentum residual and weighted gauge. Periodic constant forcing accelerates retained means. Additional tests cover nonzero skew central convection, time-dependent shear lifting and its original acceleration, mapping rejection, connected but pinched links, rejected geometry receipts and overcapacity pressure owners. These are algebraic/reference checks, not resolved flow convergence.

The twelve-tetrahedron BDM2 source exceeds the default 1-billion construction budget. Its test declares 2-billion construction and pressure caps before execution, without retrying a failed request at larger limits. All other hard bounds are unchanged. The default rejection is retained as evidence.

## Runnable exact fixture derivation

Run the standalone standard-library derivation from the repository root:

```sh
python3 -B docs/verification/fixtures/quest-cfd/affine_exact.py
```

The [script](../../../docs/verification/fixtures/quest-cfd/affine_exact.py), [fixed rational fixture results](../../../docs/verification/data/2026-10-06-affine-mesh/exact-fixtures.json), and [execution receipt](../../../docs/verification/data/2026-10-06-affine-mesh/exact-reproduction.json) provide all eight order/source rank calculations for the four unrefined fixtures above. The script independently re-eliminates its pivot minor, verifies the exact row dependency and invertible denominators, and publishes the complete rational P1/P2 unit-mass matrices with a positive LDL check. Its rational coordinates describe ideal fixtures, not an assertion that every decimal binary64 input equals that rational source. The red-refinement test remains separate Rust evidence. The captured run is a bounded derivation, not a flow-convergence result.
