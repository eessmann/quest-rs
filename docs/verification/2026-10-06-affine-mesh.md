# Closed affine DG mesh verification

This stage adds a bounded complete-coordinate `PhysicalSpace::from_mesh`
constructor and reusable exact geometry predicates in MathCore. The
[method and API contract](../../crates/quest-cfd/docs/affine-mesh.md) describes
the topology, pressure, admission and ownership rules. The
[MathCore derivation](../../crates/vendor/mathcore/GEOMETRY.md) separates exact
geometric conformity from floating conditioning and physical discretization.

The supported sources are connected closed Dirichlet simplicial manifolds and
fully periodic axis-aligned boxes, at uniform BDM1/P0 or BDM2/P1 order in two or
three dimensions. Assembly reuses the existing basis, complete constraints,
mass chart, central convection and SIP operators. Mixed or natural boundaries,
general periodic seams, curved or hanging geometry, mixed order and
zero-dimensional reference charts reject explicitly. Arbitrary generated
distributed geometry and the literal open wake are outside this stage.

## Executed evidence

Independent root execution passed 12 new affine integration tests, five existing
boundary tests, eight physical-order tests, six pressure-observation tests and
the private pinched-link topology test. A separate reviewer reran the new mesh,
physical-order and topology tests and inspected the frozen source and resource
formulas. Strict scoped Clippy passed.

The maintained MathCore geometry suite passed 11 tests, including independent
Cramer intersection checks, exact contact, one-bit gaps and overlaps, repeated
admission, width limits and subprocess arithmetic-profile checks. Separate
rational review covered 320 general pairs and 64 cases at the hard coordinate
width. These tests do not constitute a universal numerical-rank or continuum
convergence proof.

The physical checks include unequal-cell mass matrices and every chart Gram
entry, complete kernel dimensions, retained periodic uniform flows, nonzero
energy-conserving central convection, closed SIP dissipation, volume-weighted
pressure gauges and original momentum reconstruction. Polynomial boundary data
retain the lifting derivative and every homogeneous coordinate. Refinement and
order elevation reproduce every coarse coordinate and its mass energy.

The [standalone exact fixture script](fixtures/quest-cfd/affine_exact.py)
independently assembles four fixed ideal-rational meshes in Cartesian monomials.
For both orders it verifies a nonzero exact row dependency and an invertible
modular minor, with a checked prime and invertible rational denominators. Equal
upper and lower bounds certify the rational fixture rank. It also reproduces
the normalized P2 mass matrices and positive exact LDL pivots. Its
[raw results](data/2026-10-06-affine-mesh/exact-fixtures.json) and
[execution receipt](data/2026-10-06-affine-mesh/exact-reproduction.json) retain
source identity and measurements. Root and independent reviewer executions
reproduced all non-timing fields. The fifth, red-refinement row in the method
chapter is numerical inclusion evidence, outside these four rational fixtures.

Run from the workspace root, with the installed QuEST selected through
`QUEST_ROOT`:

```sh
cargo test -p quest-mathcore --test geometry
cargo test -p quest-cfd --test physical_mesh --test physical_order --test physical_boundary --test physical_pressure_observation
cargo test -p quest-cfd --lib mesh_topology
python3 -B docs/verification/fixtures/quest-cfd/affine_exact.py > affine-exact.json
```

## Admission evidence and limits

The unequal twelve-tetrahedron periodic cube retains 61 BDM1 and 169 BDM2
coordinates. With the fixture's declared 16 KiB external owner, the final
construction receipts report:

| Order | Construction work | Managed construction envelope | Retained source |
| --- | ---: | ---: | ---: |
| BDM1 | 387,690,240 | 85,458,496 bytes | 730,116 bytes |
| BDM2 | 1,608,128,256 | 90,712,768 bytes | 3,087,326 bytes |

Both orders charge 100,990,144 geometry work units. BDM2 rejects the default
one-billion construction ceiling; its separate admitted fixture declares
two-billion construction and pressure ceilings before execution. No coordinate
is removed and no failure automatically raises a limit. The byte ceiling
remains 256 MiB. These are modeled arithmetic and payload allowances, not
processor instructions, measured RSS or proof that allocator internals have
that exact footprint.

Review corrected ambiguous source framing, loss of failed-attempt receipts,
phase labels, facet-plan capacity accounting and combined lifted-pressure
preflight. The nested topology allowance was increased from the design's
64 KiB to 128 KiB after auditing simultaneous fixed arrays. Existing limits
remain unchanged, and old box pressure receipts omit the new optional field.

Host arithmetic configuration has a separate limitation. Safe environment
retrieval can clone an oversized threshold value before rejecting it; a review
probe observed a 1 MiB allocation with a 1 MiB malformed value. Cheap insufficient
source/query budgets now reject before that lookup, and supported immutable
threshold values are absent or at most 32 ASCII bytes with checked minima.
Host lookup remains outside the arithmetic payload/work model. A geometry
receipt is therefore not a hard process-memory bound for arbitrary malformed
host configuration; actual operating-system caps remain separate.

No quantum circuit was executed by this affine stage. Its rank, mass, pressure
and inclusion results are classical construction and discretization evidence.
They do not close physical benchmark convergence, distributed arbitrary-mesh
capacity, configuration/lift refinement or multi-host acceptance.
