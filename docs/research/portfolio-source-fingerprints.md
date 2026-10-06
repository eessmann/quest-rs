# Commutative portfolio source fingerprints

The owning shift/stencil encodings, weighted LCU selector plan, and arithmetic
stencil now hash each semantic source record with the shared domain-separated
SHA-256 helper before wrapping addition. Each family uses a distinct version-two
domain. The resulting 64-bit source identity is order insensitive; the existing
ordered construction identity continues to bind selector labels and the complete
unitary recipe. These are accidental-integrity provenance fields, not authentication
or proof that two arbitrary representations encode the same operator. Persisted
file and semantic SHA-256 integrity remain separate.

Byte-wise FNV alone did not make commutative addition safe. Public APIs reproduced
source collisions between `iI-iS` and `-iI+iS`, between phase-swapped arithmetic
stencils, and between two different disjoint-register permutations. Their ordered
construction identities still differed. The matching-record substitution defect
and its persisted-format handling are described in
[the shared record-fingerprint contract](sparse-record-fingerprints.md).

The fixed digest engine uses a 1,024-byte modeled stack allowance; its deterministic
work model charges 8,192 scalar units per padded SHA block. These cover the digest
engine and the fixed record iterators used here, rather than arbitrary user iterator
callbacks, allocator overhead, RSS or native backend storage. No coefficient, source
matrix, gate stream or basis table is built for hashing.

`LcuPlan` admits its metadata hash work before the descriptor loop, retains exact
zero-weight provenance, and keeps nonzero underflow branches. Its existing 4,096-byte
construction allowance includes digest scratch. This plan's compilation allowance
covers its own metadata and PREP; previously constructed children have separate
construction costs. `ArithmeticStencil` admits all input offset-vector capacities
before its first record, and includes its own raw record hashing plus the source
hashing of the child shifts it constructs in its shared compilation allowance.
Preparation and forward/adjoint count passes consume the remaining allowance.

Legacy `TensorShiftEncoding` and `StructuredStencilEncoding` cache immutable source
IDs during construction. `source_fingerprint_work()` records the original modeled
SHA component even on clones; stencil work covers its own records, with prior child
shift construction accounted separately. Their byte-only `NumericalPolicy` includes
fixed digest scratch and the added cached fields, but provides no work ceiling.
Descriptor reads reuse the cache; this does not make prior source construction free
or bound arbitrary repeated public descriptor calls. Ordered construction metadata
continues to derive from the immutable recipe.

Focused public regressions preserve reorder-invariant source IDs, distinguish the
changed operators, retain ordered construction differences, check clone/read cache
stability and digest costs, and reject known work/whole-input capacity excess. Existing
bounded gate-arithmetic, projected-block and whole-unitary tests remain numerical
behavior checks. This metadata repair provides no new inverse, capacity campaign,
multihost or scientific accuracy result. Historical receipts retain their tested IDs;
new construction uses the corrected identities.

```sh
cargo test -p quest-qsvt --test commutative_identities
```
