# Portfolio source fingerprint repair — scoped verification

Four owning/portfolio source families used commutative addition of byte-FNV record
hashes. Public APIs reproduced equal source IDs for different phase-weighted
operators and disjoint tensor shifts. The affected ordered construction IDs still
differed; these fixtures do not demonstrate a whole-descriptor substitution.
The matching-record substitution and persisted version-two handling are covered
by the [separate matching verification](2026-10-06-sparse-record-fingerprints.md).

Each family now hashes its fixed semantic record through the domain-separated
SHA-256 helper before wrapping addition. Source IDs remain order insensitive;
construction IDs continue to bind the ordered whole-unitary recipe. The truncated
64-bit IDs are accidental-integrity provenance, not authentication or a proof of
operator equality. No numerical gates, operators, phase sequence or tolerances
were changed by this repair.

Legacy shifts/stencils cache immutable source IDs at construction. Their
`source_fingerprint_work()` getter retains the original modeled SHA component
on clones; child construction costs stay separate. Byte admission includes the
fixed 1,024-byte digest stack and cached fields, but the legacy byte-only policy
has no work ceiling. LCU pre-admits its own metadata hash work; arithmetic stencils
pre-admit their raw record and constructed-child hash work. All input offset-vector
capacities are admitted before the first arithmetic record. Work is modeled scalar
work, not measured cycles; allocator/native/RSS overhead is outside this model.

The [owner source manifest](data/2026-10-06-portfolio-source-fingerprints/source.json)
binds eight files. [Owner evidence](data/2026-10-06-portfolio-source-fingerprints/owner-focused.json)
records four meaningful identity failures and the separate whole-input capacity
failure followed by successful maintained regressions. Seven new tests and 34
existing affected tests pass across ten targets. Root independently repeated all
41 tests and strict scoped Clippy with the 26 owner/helper/matching hashes stable;
[the independent record](data/2026-10-06-portfolio-source-fingerprints/root-verification.json)
and [scope reference](data/2026-10-06-portfolio-source-fingerprints/shared-helper-reference.json)
retain their precise scope. The existing 40-bit compact test fixture's byte allowance
changed from 1,024 to 2,048 only to include new digest scratch; no campaign ceiling
was raised. Failed intermediate attempts are retained and qualified in the
[owner report](data/2026-10-06-portfolio-source-fingerprints/owner-report.md).

The [publication index](data/2026-10-06-portfolio-source-fingerprints/publication.json)
separates original private SHA values from normalized candidate bytes. Generic
source/numeric JSON is byte-original where applicable; host paths are normalized
only in marked copies. These scoped pure checks are not transitive build gates,
native inverse/readout evidence, multihost capacity or scientific convergence.
Historical receipts keep their tested identities. This chapter does not report
any ongoing persisted-consumer campaign outcome.
