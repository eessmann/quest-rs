# Constructed encoding resource curves

This bounded example constructs six actual encodings of
**A_N = 0.5 I_N + 0.125 i S_N^(N/2)** for N=4,8,16,32, where S increments modulo N.
It streams each complete forward and adjoint into a checked counting visitor.
There is no native execution, stored gate stream, Hilbert-space allocation,
extracted block or solve in this example. The separate
[N=4 comparison](portfolio-comparison.md) supplies independent bounded block
extraction and is rerun alongside the resource tests.

```sh
cargo run -p quest-qsvt --example portfolio_resource_curves
cargo run -p quest-qsvt --example portfolio_resource_curves -- --count-work 20
cargo test -p quest-qsvt --test portfolio_resource_curves --test portfolio_comparison
```

The [complete counted output](data/portfolio-resource-curves.txt) and
[rejected-count output](data/portfolio-resource-curves-rejected.txt) record actual
local 64-bit Linux Cargo debug runs. Each row carries the constructed descriptor:
qubits, system/work/clean masks, left/right fixed masks/values and logical ranges,
normalization, error attestations, declared source identity and particular unitary
construction identity. Identical represented A does not require identical U,
projectors or provenance fingerprints across schemes. These identities detect
construction/source distinctions; they are not cryptographic build attestations.

| Construction | α | Total qubits at N=4/8/16/32 | Workspace | Forward gates (adjoint equal) |
| --- | --- | --- | --- | --- |
| Uniform matching | 1 | 4 / 5 / 6 / 7 | 2 / 2 / 2 / 2 | 22 / 40 / 76 / 148 |
| Per-matching bounds | ≈0.625 | 4 / 5 / 6 / 7 | 2 / 2 / 2 / 2 | 30 / 48 / 84 / 156 |
| SCC base | 1 | 5 / 6 / 7 / 8 | 3 / 3 / 3 / 3 | 7 / 7 / 7 / 7 |
| SCC eligible PREP | ≈0.625 | 5 / 6 / 7 / 8 | 3 / 3 / 3 / 3 | 11 / 11 / 11 / 11 |
| Explicit sparse QROM | 1 | 12 / 14 / 16 / 18 | 10 / 11 / 12 / 13 | 64 / 131 / 278 / 601 |
| Weighted shift LCU | ≈0.625 | 3 / 4 / 5 / 6 | 1 / 1 / 1 / 1 | 11 / 11 / 11 / 11 |

The constant arithmetic counts are specific to this half-turn shift: it toggles
one system bit at every size. They are not general increment, stencil or CFD
gate bounds. All primitives are portable X/H/Ry/scalar Phase with signed controls;
control counts/occurrences are separately recorded. Counts are not decomposed
T gates, hardware depth, native dispatch work or coherent qRAM costs.

PREP/UNPREP costs per orientation are 2,8,2,8,2,8 respectively; the remaining
primitives include SELECT, scalar phases, matching completion and QROM
location/value/unlookup work. Each portfolio inventory separately reports named
queries, resource-directory/admission queries, table entries, precision,
preparation compile work and constructor compile work. Uniform matching has no
public compile-work/table/query inventory; its missing fields stay unavailable.
Named query types differ and parent constructor work does not universally sum
all child construction/preparation work. Per-matching construction timing includes
the prerequisite uniform source; its peak is the maximum of prerequisite and
derived constructor envelopes, whose latter already accounts for the base.
No complete, universally comparable native-work total is inferred.

Sparse access uses two-bit magnitude/phase words and four QROM lookups/unlookups.
For this fixture, ratio 1/4 and the quarter-turn phase fit those words; binary64
gate evaluation and preparation still lack an independent arithmetic certificate.
Analytic/binary64 schemes reporting zero quantized precision bits do not have
zero numerical error. No equal-accuracy or physical convergence claim is made.

| Construction | Retained bytes at N=4 / N=32 | Constructor peak envelope at N=4 / N=32 |
| --- | --- | --- |
| Uniform matching | 1360 / 8752 | 4608 / 33280 |
| Per-matching bounds | 11552 / 16032 | 17480 / 33280 |
| SCC base | 784 / 784 | 8976 / 8976 |
| SCC eligible PREP | 992 / 992 | 9184 / 9184 |
| Explicit sparse QROM | 1424 / 4112 | 6120 / 20232 |
| Weighted shift LCU | 824 / 824 | 5144 / 5144 |

Memory numbers retain each constructor's own managed payload/scratch scope. Shared
input/child inclusion follows those contracts and is not added blindly. The common
canonical CSR has 2N nonzeros and retains 232/456/904/1800 bytes; it is timed and
reported separately. CSR creation has explicit dimension/entry/byte/work limits.
Scalar descriptors/receipt vectors and output formatting, allocator overhead and
RSS are outside the displayed encoding envelopes. Schemes are constructed and
dropped sequentially, apart from the required per-matching/base overlap. A larger
descriptor width alone is not a native state-memory admission.

Constructor, canonical-source and counting wall times are retained in the raw
output. One tiny debug observation per source is neither a speed ranking nor a
cross-size optimized runtime campaign. Actual primitive and storage counts are
the comparison here; native timing, memory telemetry and equal-accuracy baselines
remain separate acceptance.

Each constructed source has a counting-work allowance shared across forward and
adjoint (default 100000 accepted visitor calls), plus a per-orientation gate limit
(default 50000). Exhaustion before replay avoids entering the source. A callback
can observe one further emission that triggers rejection; `observed_emissions`
records it, while admitted count work stays within the bound. Opaque source work
between callbacks remains governed by the source's independent constructor/replay
contracts; this is not a universal instruction or wall-time bound. Sources are
already compiled even when counting rejects, so their full descriptors,
construction inventories and timings remain visible and partial counts are
labeled `count_rejected`. Constructor admission failure propagates as an error.

At allowance 20, SCC base completes both seven-gate orientations at every size;
the other schemes retain explicit count rejection. Tests exercise zero allowance,
one-less total work, exhausted allowance before the second orientation, independent
per-orientation limits and rejected construction bytes. These retained failures
are admission evidence, not successful circuit/resource claims.
