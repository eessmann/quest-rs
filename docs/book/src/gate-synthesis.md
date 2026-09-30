# Explicit Clifford+T synthesis

Native QuEST execution applies arbitrary-angle gates directly. Request discrete
synthesis during explicit compilation when studying gate sets, resource costs,
or exact circuit identities. Macro expansion performs bounded checking and
simplification; it does not run an expensive numerical search.

`quest-synthesis` generates candidates directly on macOS and Linux, without an
external process. `quest-math` owns the independently checked certificates.
`quest_compile::NativeSynthesis` connects this library to compiler passes;
external workers remain an explicitly selected alternative. A successful
candidate search is insufficient until the independent checker accepts the
requested target, tolerance and full-phase emitted operator.

## Exact and approximate mathematics

`normalize_one_qubit` produces a deterministic Matsumoto–Amano normal form while
preserving scalar phase. `synthesize_matrix` reduces a checked exact matrix over
`D[omega]` with deterministic column order and residue pairing. Basis-index
operations become elementary Clifford+T gates through checked Gray paths and
controlled constructions. Arbitrary multi-controls are not terminal operations.

`approximate_rotation` accepts rational multiples of pi, dyadic radians, and
exact affine `r + s*pi` angles. The grid search uses rational LLL reduction and
bounded enumeration in the Minkowski embedding; it implements the Ross–Selinger
candidate/norm equations with a different enumeration algorithm from newsynth.
It does not promise optimal T-count. The exact binary64 tolerance value is
retained, and no floating-point number is guessed to be an exact multiple of pi.
In particular, replacing `Rz(pi/4)` by T loses a scalar phase and is rejected by
full-operator verification.

## Clean ancilla and determinant policy

`AllowOneClean` permits at most one reusable, new highest-numbered wire. It must
start in zero and return to zero: the certified identity is `C J = J U`, with
`J` the zero-ancilla embedding. No occupied wire is reset. `NoAncilla` checks the
Giles–Selinger determinant restrictions and returns `AncillaRequired` when the
logical matrix needs the clean wire.

Both an explicitly supplied matrix and its elementary circuit can grow
exponentially with qubit count. Bounded in-place replay checks reduction traces,
permutations, gate templates and composition; small dense reconstructions remain
independent test oracles.

## Resource and evidence contracts

Each request owns precision, seed, cancellation, work and storage limits.
Search choices depend on deterministic logical work, not elapsed time.
Coefficient growth, matrix/proof storage, reduction steps and output gates are
bounded. Errors distinguish invalid mathematics, ancilla need, exhausted work,
unresolved precision, cancellation and rejected certificates. Exhausting a
search does not prove mathematical impossibility.

The old rsgridsynth dependency has been removed. Pinned newsynth 0.4.1.0 is an
external reference executable: tests compare full-phase words and reconstructed
operators, including affine targets and rejection of corrupted words. Its GPL
source is not incorporated into the Rust implementation. The library README
records mathematical sources, algorithm differences and reference build hashes.

[Measured compilation, preparation, execution, synthesis, certification, memory
and gate counts](https://github.com/eessmann/quest-rs/blob/main/benchmarks/architecture/README.md)
are recorded separately. Those macOS measurements demonstrate reuse for the
recorded workloads; they do not establish cross-platform performance claims.
