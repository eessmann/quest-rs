# Coherent encoding portfolio

`quest_qsvt::portfolio` supplies owning `ReplayEncoding` sources. They stream
portable `ReplayGate` primitives and reuse the shared QSVT schedule and native
primitive executor; they retain no gate list or dense preparation isometry.
Descriptors bind logical/projector layouts, clean workspace, source provenance,
whole-unitary construction, normalization and available error evidence. Clones
share immutable storage.

Numeric descriptor fingerprints hash each word's little-endian bytes. Feeding
whole binary64 words into an XOR/multiply hash would make paired sign changes
cancel, allowing distinct LCU operators to share both descriptor identities.
Regressions cover a DG1 history versus its adjoint and `I + iX` versus its
negative, including rejection of a schedule bound to the other source. These
64-bit values detect accidental mismatch; they are not collision-free proofs
or authentication. Persisted shard files retain their separate SHA-256 integrity
and canonical semantic digests. Historical numeric fingerprints made before
this correction are not rewritten or treated as a stable interchange identifier.

The [bounded comparison](portfolio-comparison.md) records normalization, actual
replay primitives, preparation, workspace, precision and constructor measurements
for eight eligible constructions of one complex circulant. Its tiny local timings
do not establish a general performance ranking.
The [cross-size resource curves](portfolio-resource-curves.md) construct six
encodings at N=4/8/16/32 and count both replay orientations without allocating a
state. They retain actual layouts, managed storage and rejected count budgets;
they remain distinct from native timing and accuracy-dependent CFD costs.

## Weighted and tensor composition

`LcuPlan` is the shared immutable selector/descriptor plan behind `WeightedLcu`.
It accepts a bounded owned Vec of raw complex weights and child descriptors;
it owns PREP coefficients, weight phases and a dense-to-original surviving-index
map, with no child records or circuits. Its error-generic `visit_mapped_steps`
streams `LcuStep::Gate` and `LcuStep::Child` events. Child events carry mapped
signed controls, while child targets are the mapping's `child_width` prefix.
Portable `WeightedLcu` lowers these events through its owning replay sources;
a native prepared owner can lower the same events through sharded children.
The pure plan alone does not claim that any distributed native child is prepared
or that a complete composite operation is admitted before mutation.

`LcuPlanLimits` bounds input term count, actual input Vec capacity, plan/PREP
application bytes, modeled metadata and preparation compilation work, and selector
primitives. `LcuPlanResources::primitive_gates` counts both PREP directions and
one scalar phase per nonzero term; it excludes every child primitive or native
dispatch. Metadata work uses `128 + 4*child_width` modeled scalar slots per
original input, covering validation, little-endian fingerprints, weight operations,
metadata copying and linear survivor filtering. This is a documented work model,
not a measured CPU instruction count. PREP compilation consumes the same remaining
allowance. No child source queries are made by plan construction or iteration.

Actual indices/phase/amplitude Vec capacities are checked before filling, and
amplitude-tree capacities are checked before coefficient transforms. Retained
bytes use actual coefficient capacities plus explicit scalar/Arc allowances;
constructor peaks include live input, temporary arrays and a 4096-byte selector
allowance. Allocator metadata, opaque source work and OS/native overhead remain
outside this application-byte model. `AmplitudePreparation::retained_bytes`
exposes its checked retained-capacity count independently of the logical table
entry count. Clones share immutable payload storage.

`WeightedLcu` additionally admits its input/source/output owners and scans both
child replay directions. Its cheap metadata-work lower bound rejects exhausted
construction limits before invoking child metadata callbacks; all descriptors,
including zero-weight descriptors, validate before any child gate scan. Plan
metadata/PREP work and child scans share one compile allowance, and the remaining
gate allowance decreases across children. Its construction peak conservatively
includes input children, future frozen child storage and the full selector peak.

`WeightedLcu<E>` accepts weighted children with the same layout and left/right
compact projectors. With `m_t = |w_t| alpha_t`, its preparation input is
`sqrt(m_t)`. SELECT applies the entire child unitary under its label, including a
scalar phase `arg(w_t)` on every child sector. UNPREP is the inverse of that same
preparation. The clean-label block is the weighted sum with nominal
`alpha = sum m_t`. Zero weights are removed; empty/all-zero supplied sums reject
because there is no positive preparation norm. Cancellation among nonzero terms
is supported. A retained nonzero weight is charged and replayed on its whole label
sector even if the binary64 product `|w_t| alpha_t` underflows to zero. Its prepared
amplitude is then zero; no independent real-arithmetic accuracy certificate is
inferred from that underflowed mass.

The implemented normalization is the squared compiled preparation norm, so the
binary64 coefficient mass and preparation normalization describe the same block.
`normalization_roundoff` reports its absolute discrepancy from the nominal
binary64 sum. This is a diagnostic, not an independently certified real-arithmetic
rounding bound. Preparation and encoding error attestations remain `None` when
binary64 synthesis supplies no independent certificate. Dummy labels have zero
input preparation mass and a deterministic identity SELECT completion.

`TensorProduct<A,B>` uses the first factor's logical index as the fastest-varying
index. Factors retain separate physical workspace; both orientations reverse
whole child order correctly. `KroneckerSum<A,B>` uses this layout and lifted
children for `A` on the fast factor and `B` on the slow factor. Tensor factors
require square full power-of-two system projectors: arbitrary Cartesian products
of truncated logical ranges cannot be expressed by the current single compact
range descriptor and are explicitly rejected. Weighted composition itself also
supports compatible rectangular projectors. In a Kronecker sum each inactive factor
uses an explicit XOR bridge from its right fixed workspace value to its left
fixed workspace value. Thus distinct generic left/right workspace embeddings
retain the intended system identity; bridge gates, controls, adjoints and
construction provenance are accounted explicitly.

The linear-combination construction follows the state-preparation-pair calculus
of [Gilyén et al., arXiv:1806.01838, Definition 51/Lemma 52](https://arxiv.org/pdf/1806.01838).
The local preparation baseline is the existing amplitude-tree/Gray-code recipe;
its stored coefficients, compilation work and every emitted gate are counted.
Dry-run scans share a remaining modeled work allowance across both orientations,
all children and prior charged construction work. Exhausted allowances reject
before entering replay. A failing callback can observe one extra source emission;
opaque source computation between callbacks retains its own admission contract.

## Arithmetic structured maps

`ArithmeticStencil` accepts power-of-two axis widths, signed offsets and repeated
complex coefficients. The first axis varies fastest. `Boundary::Periodic` gives
circulants and periodic tensor stencils; `Boundary::Zero` gives bounded-band
Toeplitz and nonwrapping tensor stencils. Empty/zero-only terms, duplicate supplied
offset tuples, and offsets spanning an entire axis reject explicitly.

Labels are `(d,m)`: `d` selects a coefficient/offset tuple and `m` is the input
system coordinate. The column map is identity. The row map streams conditional
modular tensor additions. Zero boundaries set a separate out-of-range flag on
the union of invalid-axis ranges using disjoint arithmetic bit cubes; they do
not enumerate padded system indices. This implementation keeps both the data
and range flags for both boundary conventions rather than claiming minimal flags.

`StructuredScheme::Base` uses uniform label Hadamards, a data-flag rotation with
success coefficient `|w_d|/beta`, and a complex success-sector phase. Padded labels
have zero success coefficient. Its normalization is `K beta`, where `K` is the
padded number of supplied nonzero terms and `beta=max |w_d|`.

`StructuredScheme::Prep` is restricted by construction to these label-preserving
maps: arithmetic row maps never change `d`, so coefficient loading commutes with
them. It uses `sqrt(|w_d|)` preparation, a label-conditioned complex phase and
UNPREP, with nominal normalization `sum |w_d|`. Out-of-range flags remain explicit.
There is no generic PREP switch for arbitrary maps lacking this identity.

The label maps and range flag derive from [Sünderhauf, Campbell and Camps,
arXiv:2302.10949v2, sections 2.1, 2.3 and 3.2](https://arxiv.org/pdf/2302.10949v2).
The implementation's complex coefficients extend the paper's stated real-value
presentation through explicit phases; extracted-block and whole-sector tests
verify that extension. There is no preamplification or hierarchical-compression
implementation or claim.

## Stored matching bounds

`PerMatchingBounds` derives independent immutable per-color resource columns from
an admitted local `MatchingEncoding`. Each color uses its own maximum magnitude
`beta_t`, its original completed permutation and recomputed flag angles. Weighted
composition with unit weights has nominal normalization `sum beta_t`. Source
identity stays that of the original matrix; construction identity changes.
Existing producer/persisted normalization contracts are unchanged. This constructor
requires a bounded stored local matching plan and accounts its simultaneously live
storage during compilation; it is not distributed recoloring or rescaling.

## Explicit sparse access

`Qrom` implements XOR lookup and literal reversed unlookup, including dirty output
registers, padded zero addresses, signed controls and arbitrary operand mapping.
Its storage and primitive count are explicit; scanning a table is never a free
oracle. It rejects retained input capacity, table, work or gate budgets before
returning an owning source.

`SparseAccessEncoding` compiles a supplied bounded local CSR/CSC matrix into three
QROM tables. Column/slot locations include inverse row ranks, and row/slot locations
include inverse column ranks. Unoccupied slots receive deterministic bijective
completion. Lookup, bitwise register swaps and inverse unlookup realize the actual
full permutation while returning initially clean location scratch to zero.
The value table contains a `p+1`-bit normalized magnitude (including endpoint one)
and a `p`-bit phase fraction of a full turn. Precision `p=2..24` is supported.
Value lookup, controlled data rotations/phases and value unlookup restore clean
value scratch. Dirty workspace sectors still undergo a fixed unitary.

Four actual QROM queries occur per whole replay: two value queries and two location
queries. Row and column sparsity are both padded to the same power of two `S`, so
`alpha=S beta`. This does not claim the optimal asymmetric `sqrt(s_r s_c) beta`
normalization or black-box gate costs of Gilyén et al.'s Lemma 48. The stored-table
construction is a concrete sparse-access baseline with explicit inverse ranks.
`coefficient_error_estimate()` reports the observed binary64 discrepancy multiplied
by `S`; it excludes gate-synthesis error and is not a theorem certificate.

Three tables contain `3 N S` words in total. Compilation is admitted before
allocating them, including sorting, source storage, degree arrays, forward/inverse
maps, payloads, frozen rotations, ownership conversion and replay scans. This is
an explicitly bounded local baseline, not a requirement for the scalable MPI
producer, and not distributed QROM or free access to a global CSR matrix.

## Cost comparison from tested fixtures

Counts are actual portable `ReplayGate` primitives, including explicit zero-angle
operations. Multi-control X/phase is one primitive here; this is not a T-count or
native elapsed-time claim. `oracle_queries` means QROM query sites or child SELECT
calls per replay; `directory_queries` records known underlying resource lookups.
`admission_queries` separately records integrity/dry-run directory calls.

| Fixture/construction | Normalization | Gates | PREP gates | Workspace qubits | Table entries | Queries per replay |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 3-term 4-coordinate circulant, base | 4 | 17 | 4 | 4 | 3 coefficients | arithmetic gates, no QROM |
| Same circulant, PREP | 1.7236067977499792 | 37 | 28 | 4 | 3 coefficients | arithmetic gates, no QROM |
| 2x3 complex matrix, common matching bound | 12 | 18 | 4 label H | 3 | original stored plan | stored matching replay |
| Same matrix, per-color bounds | 3.5 | 44 | 28 | 3 | 5 completed columns + 3 terms | 3 SELECT calls + 39 directory lookups |
| 1x2 complex matrix, QROM `p=2` | 2 | 29 | 2 label H | 9 | 12 words | 4 QROM queries |

The circulant PREP compile work is 203 units versus 115 for base, with a reported
normalization discrepancy of `2.220446049250313e-16`. Per-color compilation reports
164 directory admission queries and 1086 modeled work units. The QROM fixture
reports 852 compile work units and 5264 bytes of simultaneous construction storage.
These fixture observations compare normalization, gate/preparation counts,
workspace, stored data and queries; they do not infer a faster complete solver
from normalization or polynomial degree alone.
