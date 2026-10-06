# Measured bounded portfolio comparison

Run `cargo run -p quest-qsvt --example portfolio_comparison` and
`cargo test -p quest-qsvt --test portfolio_comparison`. One four-site circulant
**A=0.5 I₄+0.125 i S₄²** (S increments modulo four) also equals
I₂⊗(0.5 I₂+0.125 i X). Every compared scheme is eligible for this same matrix.
This particularly simple fixture does not establish support for every structure.

The independent test checks all sixteen extracted entries and normalizations.
A bounded scalar primitive simulator streams each source onto four basis states
and reads that source's compact left/right projectors. No complete U is built or
compared between constructions. The largest temporary reference state has 4096
complex amplitudes; the fixture has eight nonzeros. These reference resources are
bounded verification costs separate from the encoding's payload receipts.

[Recorded output](data/portfolio-comparison.txt) contains actual emitted primitive
and control counts, constructor inventories, timings and extracted entries from a
local 64-bit Linux debug-build run:

| Construction | α | Workspace | Gates | PREP/UNPREP | SELECT/other | Retained bytes | Peak envelope | Constructor µs |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Uniform matching | 1 | 2 | 22 | 2 | 20 | 1360 | 4608 | 45.4 |
| Per-matching bounds | ≈0.625 | 2 | 30 | 8 | 22 | 11552 | 17480 | 128.4 |
| SCC Base | 1 | 3 | 7 | 2 | 5 | 784 | 8976 | 21.1 |
| SCC eligible PREP | ≈0.625 | 3 | 11 | 8 | 3 | 992 | 9184 | 12.5 |
| Explicit sparse QROM | 1 | 10 | 64 | 2 | 62 | 1424 | 6120 | 38.1 |
| Weighted unitary LCU | ≈0.625 | 1 | 11 | 8 | 3 | 824 | 5144 | 19.4 |
| Tensor product | ≈0.625 | 1 | 11 | 8 | 3 | 1288 | 5384 | 15.5 |
| Kronecker sum | ≈0.625 | 1 | 17 | 12 | 5 | 2840 | 7512 | 22.7 |

All counts describe one whole forward replay. PREP/UNPREP includes known child
preparations for tensor and Kronecker constructions. SELECT/other is the remaining
actual primitive count, including scalar phases and flag/permutation/location work.
Raw inventories include X/H/Ry/Phase, controlled primitives and summed control
occurrences. These are portable signed-control primitives, not decomposed T,
hardware or native-dispatch costs. Identity arithmetic is specific to the supplied
source, not a free CSR lookup.

The sparse baseline performs four reversible QROM lookups/unlookups, retains
24 table entries and uses two-bit magnitude and phase precision. This particular
ratio 1/4 and quarter-turn phase fit those words exactly; remaining block discrepancy
is observed binary64 gate evaluation. This does not certify arbitrary coefficients
at two-bit precision. Other schemes use analytic/binary64 parameters without a
quantized value table; `precision_bits=0` does not mean zero numerical error.

Named queries are not interchangeable: weighted composition records two child
SELECT calls; sparse access records four QROM operations; arithmetic streams gates.
Per-matching additionally reports 52 directory queries per replay and 218 admission
queries. Modeled constructor work is 1888 for per-matching, 50/78 for SCC Base/PREP,
1896 for sparse access, 26 for weighted LCU, 22 for the tensor outer constructor and
38 for the Kronecker outer constructor. Child construction and preparation work
are not recursively summed by every constructor. The raw output preserves that
scope, not a complete comparable native-work estimate. Uniform matching exposes no
modeled compile-work/table-entry counter; these fields remain unavailable.

The shared CSR takes 232 bytes and about 29.6µs to construct in this run; its cost
is printed separately. Constructor timings include child creation. Per-matching
includes the measured prerequisite uniform base and its derivation, excluding the
basis extraction between stages. Extraction is timed separately; sparse extraction
takes about 6.1ms because the scalar simulator scans a larger workspace register.
Single tiny debug timings are noisy observations, not reliable speed rankings,
native benchmarks or a performance campaign across sizes.

Memory figures follow each constructor's managed payload/scratch inventory. They
exclude allocator overhead, RSS and the reference state. Shared input inclusion
and child-envelope scope follow each constructor's accounting; the common input
is printed separately and overlaps must not be added blindly. No allocator profiler
was run. Per-matching reduces α while increasing gates/directories here; structured
PREP reduces α with explicit preparation cost. The PREP switch uses only the
label-preserving identity documented in [the portfolio](portfolio.md). Sparse
lookup/value/unlookup work and precision remain visible. Native performance,
larger-size scaling and independent real-arithmetic error certification remain
outside this bounded comparison.
