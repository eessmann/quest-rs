# Historical matching fingerprint failure

The [four-column public API reproduction](public-api-red.rs) uses fixed in-memory reference columns and no native execution. On the old source it admitted negative-i phases under the positive-i header; both descriptors were identical. The [behavioral RED log](public-api-red.log) records the failed assertion. Under v2 the changed payload rejects earlier, so this historical program is not a v2 success test.

[Exact formulas and old source hashes](diagnosis.json) distinguish the additive byte-wise FNV cancellation from the earlier whole-word sign-bit fingerprint problem. The historical dimensions2/4/8/32 are pure formula checks; they do not execute the N32 persisted weighted inverse. The [version1 behavioral RED](version-1-red.log) records the old reader accepting a manifest that the maintained test required to reject.

The [independent Python SHA reference](reference.json) uses explicit LE bytes and fixed fields; it distinguishes opposite-i column phases and all three fixed source families. The final maintained native matrices separately check producer/restart semantics.

Original hashes and normalized payload hashes appear in [provenance](provenance.json). The original private evidence was not rewritten.
