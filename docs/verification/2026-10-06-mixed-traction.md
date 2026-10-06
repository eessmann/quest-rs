# Mixed traction boundaries: exact fixtures and bounded runtime foundation

The [standalone reproducer](fixtures/quest-cfd/mixed_traction_exact.py) verifies
four complete mixed-boundary constraint dimensions and one convective energy
identity using Python's standard library. The
[results](data/2026-10-06-mixed-traction/exact-fixtures.json),
[execution receipt](data/2026-10-06-mixed-traction/exact-reproduction.json) and
[measurement log](data/2026-10-06-mixed-traction/exact-reproduction.log) record
the actual execution. These exact fixtures are independent of the Rust
finite-element assembly. They do not certify a runtime binary64 matrix,
physical convergence, a quantum circuit or distributed capacity.

## Complete rank with an open facet

The fixtures have Dirichlet velocity on the coordinate-plane facets and
prescribed mechanical traction on the opposite facet. The triangle has vertices
`(0,0),(2,0),(0,1)`; the tetrahedron is the unit coordinate simplex. For velocity
order `p`, all vector polynomials of degree at most `p` are retained. Normal
trace constraints apply only on the Dirichlet facets, and every discontinuous
pressure divergence moment remains.

| Fixture | Velocity/pressure | Broken velocity width | Constraint rows and rank | Independent velocity width |
| --- | --- | --- | --- | --- |
| Triangle | BDM1/P0 | 6 | 5 | 1 |
| Triangle | BDM2/P1 | 12 | 9 | 3 |
| Tetrahedron | BDM1/P0 | 12 | 10 | 2 |
| Tetrahedron | BDM2/P1 | 30 | 22 | 8 |

There is no closed-domain pressure dependency in these matrices. A natural
facet leaves its normal velocity free; removing a pressure row or imposing
zero mean pressure would change this boundary-value problem.

The verifier assembles rational Cartesian monomials, scaled rational facet
normals and exact volume moments. Cartesian and nodal velocity bases span the
same complete polynomial space; Cartesian and barycentric pressure test bases
are also related by an invertible transformation. Nonzero row scaling or
orientation changes cannot change rank.

It verifies that `1,000,000,007` is prime, checks every rational denominator is
invertible modulo that prime, and computes a full-row-rank modular minor. A
separate determinant elimination checks that selected minor is nonzero. This
gives a rational lower rank bound equal to the number of rows, which is also
the elementary upper bound. The proof checks survive `python -O`.

The shared [exact helper](fixtures/quest-cfd/affine_exact.py) supplies elementary
determinant, moment and modular-elimination routines. Both script hashes are
recorded. Neither script imports production CFD or MathCore code: the verifier
remains independent of the algebra and numerical kernels being checked.

## Natural convection carries energy

On the triangle, take `u=a(x,-y)` with zero viscosity. It is divergence-free and
has zero normal velocity on the coordinate axes. Parametrize the open edge by
`x=2s, y=1-s`, with outward surface-normal measure `n ds=(1,2)ds`. Then

```text
integral_open (u.n)|u|² / a³
  = integral_0^1 (4s-2)(5s²-2s+1) ds
  = integral_0^1 (20s³-18s²+8s-2) ds
  = 1.
```

The conservative central convective force therefore has power `-a³/2`.
Changing the sign of `a` reverses the net energy transport. This identity is a
test for the actual natural-boundary flux; it provides no backflow-stability
guarantee.

The mechanical traction convention is `tau=nu grad(u)n-p n`, using the outward
normal of the fluid domain. It enters the weak force with a plus sign. The
DFG 2D-2 reference prescribes zero traction at its outlet and uses this
unsymmetrized stress. Its suggested developed-cycle comparison is between
25 and 30 seconds. The existing project `[4,8]` cylinder records remain coarse
diagnostics, without that developed-cycle acceptance. See the
[official DFG definition](https://wwwold.mathematik.tu-dortmund.de/~featflow/en/benchmarks/cfdbenchmarking/flow/dfg_benchmark2_re100.html)
and the [historical cylinder evidence](2026-10-05-cylinder-window.md).

## Reproduction and limits

Run from the workspace root:

```sh
python3 docs/verification/fixtures/quest-cfd/mixed_traction_exact.py
python3 -O docs/verification/fixtures/quest-cfd/mixed_traction_exact.py
```

All non-timing fields matched across ordinary, optimized and independent
reviewer executions. The measured run used a 30-second timeout and reported
17,184 KiB peak resident memory. No operating-system memory cap was imposed.
The script accepts no geometry, degree or modulus arguments; its scope is the
four fixed fixtures and energy polynomial above. Affine-equivalent runtime
fixtures may have the same ranks while having different matrices and hashes.

These records establish the independent exact fixture calculations. The
separate runtime evidence below concerns numerical assembly and admission; it
is not implied by the exact reproducer.

## Reviewed runtime foundation

The [method and API chapter](../../crates/quest-cfd/docs/mixed-traction.md)
describes the implemented BDM1/P0 and BDM2/P1 mixed exterior conditions,
canonical mass-minimum lifting, polynomial-time loads and absolute pressure
reconstruction. It requires at least one velocity boundary and one prescribed
mechanical-traction boundary on a supported connected affine mesh.

The [runtime receipt](data/2026-10-06-mixed-traction/runtime-foundation.json)
records the 12 reviewed production/test hashes, command, elapsed time and
[complete test log](data/2026-10-06-mixed-traction/runtime-foundation.log).
All 17 focused tests passed; the recorded hashes matched before and after the
root reproduction. This is a dated foundation snapshot. A later cylinder
consumer may change shared files and must record its own verification.

```sh
export QUEST_ROOT=/path/to/matching/quest
export MPICC=/path/to/matching/mpicc
cargo test -p quest-cfd \
  --test mixed_physical_mesh \
  --test mixed_physical_boundary \
  --test physical_boundary
```

The five mixed-mesh tests cover complete one-/two-cell dimensions, unequal
cells, natural convection power, absence of an artificial natural-face SIP
penalty and malformed/admission cases. Seven mixed-boundary tests cover
nonzero pressure levels and gradients, original-coordinate acceleration,
complete polynomial extraction, batch canonical lifting and resource limits.
Five existing closed/periodic boundary tests preserve their contracts.

Independent review reproduced and corrected two defects before this receipt:

- Natural exterior normal velocity was incorrectly included in a zero-trace
  continuity diagnostic. The diagnostic now follows the actual constraint
  rows, which leave that velocity free.
- The polynomial-time owner omitted declared external mesh storage from its
  live peak. Both planned and final admission now include it. A 64 MiB
  external owner therefore cannot pass the previously incomplete approximately
  4 MiB allowance; the maintained regression checks this rejection.

Independent public-API probes additionally checked the P1 minimum-mass field
`ell=(1-4x/5,4y/5)` for axis data `g=(1+x,-y)`, and nonzero contributions from
both boundary types. The former follows from minimizing
`1+4a/3+5a²/6` for `u=(1+a x,-a y)`. The latter has natural flux `101/125` and
weak Dirichlet term `-11/15`, giving convective power `-14/375`. These small
manufactured checks support the QR orientation and boundary signs; they do
not establish a physical refinement rate.

No operating-system memory cap was applied to this focused run. Its resource
tests exercise declared payload admission, rather than proving process RSS
bounds. Whole-workspace verification, distributed arbitrary mixed meshes,
backflow stability, curved boundaries, literal 3D wake conditions, developed
DFG shedding and quantum execution remain separate gates.
