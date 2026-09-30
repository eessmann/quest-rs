# quest-synthesis

Direct, bounded Rust Clifford+T synthesis. Candidate construction lives here;
exact arithmetic and independent certificates live in `quest-math`. The library
has no native simulator dependency, global floating-point precision, process
requirement, or wall-clock search decisions.

`synthesize_matrix` accepts an admitted dense `ExactMatrix` over the existing
canonical `D[omega]` ring. It performs deterministic column reduction by
sqrt(2)-denominator residues. Multiqubit reduction uses determinant-one row
operations. Gray paths lower each two-level operation into elementary H, S, T,
Pauli, and CNOT gates; there are no terminal arbitrary-control gates. Recursive
controlled-iX commutators require no hidden workspace wires.

The default `AllowOneClean` policy allocates one new highest-numbered clean wire
only when the determinant requires it. `NoAncilla` admits every eighth-root
determinant on one qubit, powers of i on two, signs on three, and determinant one
on four or more. An `AncillaRequired` error is distinct from resource exhaustion.
No occupied wire is reset. Independent proof replay checks reduction to identity,
the complete partition of the emitted word, every elementary lowering, and the
composed clean-input/clean-return identity `C J = J U`.

`normalize_one_qubit` produces the Matsumoto–Amano form
`(T | epsilon) (HT | SHT)* C`, using the exact SO(3) channel denominator. The
Clifford suffix is a deterministic full-phase H/S word, including scalar phase.

`approximate_rotation` accepts exact dyadic radians, rational multiples of pi,
and affine `r + s*pi` targets on X, Y, or Z. Its Ross–Selinger candidate equation
is solved using an exact rational LLL basis in the four-dimensional Minkowski
embedding and bounded sphere enumeration. This is an explicit alternative
implementation of the grid enumeration step, not a claim to implement the
paper's particular grid-operator reduction. Norm equations use exact small-norm
enumeration or bounded algebraic gcd/unit correction. Unresolved factorization
candidates are skipped; exhausted search never claims mathematical impossibility.
The library does not claim globally optimal T-count for approximations.

Every returned rotation candidate is independently certified against the exact
target and dyadic epsilon, including its full scalar phase. In particular,
`Rz(pi/4)` is not replaced by T. Working precision, seed, limits, grid exponent,
logical work, and the algorithm identifier are retained with the result. The
independent certifier can certify at a lower precision than the generator's
request-owned working precision; both are exposed distinctly.

All loops charge deterministic logical work. Coefficient, matrix, proof, output,
and allocation limits are checked. A request-owned `CancellationToken` can
interrupt search. Errors distinguish invalid input, nonunitarity, ancilla need,
precision uncertainty, cancellation, work/resource exhaustion, and rejected
certificates. Serialized candidate/proof data must be re-admitted with
`quest_math::verify_synthesis`; a producer's metadata is never a certificate.

## Mathematical provenance

The implementation was written from the following mathematics, without
translating or incorporating newsynth's GPL Haskell source:

- Giles and Selinger, *Exact synthesis of multi-qubit Clifford+T circuits*,
  [arXiv:1212.0506](https://arxiv.org/abs/1212.0506), lemmas 19–21, 24 and
  corollary 25.
- Ross and Selinger, *Optimal ancilla-free Clifford+T approximation of
  z-rotations*, [arXiv:1403.2975](https://arxiv.org/abs/1403.2975), grid candidate
  and norm equations. The rational LLL enumerator is a separate implementation
  choice and carries no claim to the original grid routine's complexity.
- Giles and Selinger, *Remarks on Matsumoto and Amano's normal form for
  single-qubit Clifford+T operators*,
  [arXiv:1312.6584](https://arxiv.org/abs/1312.6584), section 4.

The reference/oracle version is
[newsynth 0.4.1.0](https://hackage.haskell.org/package/newsynth-0.4.1.0), not an
implementation dependency. Its separate license remains with that upstream
reference. See `tests/fixtures/README.md` for pinned source/build receipts and
commands; `tests/newsynth.rs` verifies its full-phase output and exact matrices.

## Validation

The grid enumerator intersects its weighted lattice sphere with the necessary
coefficient-norm ball and a conservative radial bound from exact enclosures.
This prunes infeasible branches at special angles without changing the target
or epsilon. A conservative grid storage allowance remains reserved while norm
solving, exact synthesis and certification run; callbacks receive the remaining
byte allowance. Public results retain the original request limits. These are
modeled category bounds, not a universal heap-peak or allocator guarantee.

```sh
devenv shell -- cargo test -p quest-math -p quest-synthesis -p quest-optimizer-client
devenv shell -- cargo clippy -p quest-math -p quest-synthesis --lib
```

Tests cover exact phases, canonical normalization, signed controls, nonadjacent
Gray paths, two/three/four-qubit determinant restrictions, ancilla return and
leakage, corrupted proofs and partitions, affine and dyadic approximations,
10^-12 rotation certificates, concurrent request determinism, and resource and
cancellation outcomes. Process resource enforcement remains an optional Linux
client feature; direct synthesis is available on macOS and Linux.
