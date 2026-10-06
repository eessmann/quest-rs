# Constructed CFD encoding, preparation and sampling costs

`resource-build` constructs the bounded stored history and an actual sparse
matching encoding of **H†**, then streams its forward and adjoint primitive
counts. It also constructs the coherent RHS preparation table. The descriptor
supplies the actual normalization, data and workspace qubits, clean workspace,
compact left/right projectors, and source/construction identities. This replaces
the generic auxiliary-qubit allowance for these constructed cases. The existing
`estimate` command remains a representation-only estimate: its default allowance
of 16 is caller supplied and is not a constructed encoding resource count.

```sh
cargo run -p quest-cfd -- resource-build --case smoke \
  --configuration-cells 1 --configuration-order 2 --time-cells 1 \
  --horizon 0.001 --count-work 20000000
cargo run -p quest-cfd -- resource-build --case smoke \
  --configuration-cells 1 --configuration-order 1 --time-cells 1 \
  --horizon 0.001 --inverse-plan --sample-error 0.05 \
  --joint-success-lower-bound 0.001 \
  --success-provenance 'explicit illustrative hypothesis; not established for this source'
cargo run -p quest-cfd --example resource_curves -- --inverse-plan
cargo run -p quest-cfd --example resource_curves -- --count-work 20
```

The first command counts sources. `--inverse-plan` additionally constructs the
reciprocal polynomial, synthesizes binary64 QSP phases and counts the actual
inverse schedule. It does not execute that schedule. `--sample-error` requires an
explicit joint inverse/selection success lower bound and its provenance. The CLI
observable is a bounded conditional basis-coordinate population, not physical
velocity reconstruction. The library accepts a supplied deterministic scalar
selection/diagonal callback with explicit work, retained bytes and scratch bytes.
The callback is streamed twice for finite range and repeat-digest checks; no
observable table is allocated. Future statistical claims still depend on the
caller hypotheses documented in [probability observations](probability-observations.md).

Each attempted shot is charged a fresh coherent RHS preparation, an inverse,
register measurement and selection query. Only selected shots incur the
observable query work. Shot counts use the outward conditional Hoeffding/Chernoff
planner. A simulator success estimate cannot establish the supplied lower bound.
Unknown systematic bias, encoding error, coherent RHS preparation error,
projector-response error and execution-amplitude error remain unavailable, rather
than zero. The source descriptor's preparation bound of zero describes its fixed
computational projector input, not the numerical coherent RHS preparation.
Polynomial truncation and coefficient-rounding bounds are reported separately
from binary64 completion/reconstruction diagnostics. No overall CFD accuracy or
quantum execution certificate follows from successful construction.

## Recorded complete-chart comparison

The checked-in [successful receipt](../../../docs/verification/data/2026-10-05-cfd-resource-curves/results.json)
and [count-rejected receipt](../../../docs/verification/data/2026-10-05-cfd-resource-curves/rejected.json)
contain twelve actual constructions of the same complete five-coordinate,
two-triangle periodic BDM1 physical model with viscosity 0.01. The physical order is fixed at one, temporal DG order is one or two, the horizon
is 0.001, and the symmetric Carleman lift order is fixed at two. Configuration DG
order is one or two. All five independent physical coordinates are retained.
The stored history has two or three DG nodes per temporal cell, respectively.

| Lift | Configuration order | Time order | Time cells | History dimension | H nonzeros | α | Encoding qubits | Auxiliaries with response | Source forward primitives | RHS forward primitives | Inverse degree | Inverse forward primitives |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| KvN | 1 | 1 | 1 | 64 | 128 | 1 | 8 | 3 | 292 | 314 | 13 | 7,680 |
| KvN | 2 | 1 | 1 | 486 | 3,740 | 8 | 14 | 6 | 19,609 | 2,554 | 129 | 5,062,506 |
| Carleman | — | 1 | 1 | 40 | 194 | 16.14592 | 12 | 7 | 952 | 314 | 275 | 526,364 |
| KvN | 1 | 1 | 2 | 128 | 288 | 4 | 10 | 4 | 744 | 634 | 201 | 300,304 |
| KvN | 2 | 1 | 2 | 972 | 7,723 | 16 | 15 | 6 | 42,983 | 5,114 | 873 | 75,071,046 |
| Carleman | — | 1 | 2 | 80 | 408 | 32 | 13 | 7 | 2,093 | 634 | 1813 | 7,607,362 |
| KvN | 1 | 2 | 1 | 96 | 256 | 2.6666667 | 10 | 4 | 680 | 634 | 61 | 83,584 |
| KvN | 2 | 2 | 1 | 729 | 6,096 | 10.666667 | 15 | 6 | 34,477 | 5,114 | 275 | 18,969,530 |
| Carleman | — | 2 | 1 | 60 | 346 | 21.333333 | 12 | 7 | 1,701 | 314 | 575 | 1,966,522 |
| KvN | 1 | 2 | 2 | 192 | 544 | 4 | 11 | 4 | 1,576 | 1,274 | 419 | 1,324,892 |
| KvN | 2 | 2 | 2 | 1,458 | 12,435 | 16 | 16 | 6 | 82,469 | 10,234 | 1831 | 100,000,000 (partial) |
| Carleman | — | 2 | 2 | 120 | 712 | 32 | 13 | 7 | 3,685 | 634 | unavailable | unavailable |

These are resource curves, not an equal-accuracy comparison. One-cell periodic
configuration DG1 cancels its generator exactly; those two rows are explicitly
marked as temporal-cost fixtures at both temporal orders. Configuration DG2
retains the nonlinear generator.
The initial KvN bump touches the configuration boundary, so none of these rows
establishes boundary-resolved physical accuracy. Carleman truncation accuracy is
also unestablished. KvN sampling uses the actual full-coordinate conditional
kinetic-energy recipe. Carleman sampling uses first degree-one monomial
population conditioned on the degree-one/last-node sector; this is neither
amplitude reconstruction nor the same observable as KvN energy.

The recorded sampling lower bound 0.001 is an **illustrative external hypothesis**,
not a certified probability for these cases. Both lifts request absolute
statistical error 0.05 and failure probability 0.05, but their different supplied
observable ranges produce different shot counts. The receipt preserves all
premises and missing bias bounds. It reports the physical inverse rescaling
`||b||/(α c)` separately; it does not confuse conditional normalized probabilities
with rescaled linear amplitudes or quadratic physical functionals.

Ten inverse rows complete their requested counts. Temporal DG2/configuration DG2
with two cells reaches the 100 million primitive limit during forward replay;
its adjoint count remains unavailable. Temporal DG2 Carleman with two cells
rejects the reciprocal-polynomial residual at the degree cap (0.08408 exceeds
0.03). Both retain their actual source and RHS descriptors/costs, and sampling
remains unavailable for these incomplete inverse stages. The separate 20-work
receipt records all twelve count-rejected rows.

The source width is an actual descriptor width. The descriptor's `inverse_qubits`
and `auxiliary_qubits_including_signal` include the known extra response bit for
the inverse layout; they are layout counts even when synthesis is not requested,
not evidence of an allocated register. An accepted inverse stage reports its
own actual schedule width.

## Work and memory boundaries

The baseline stores a complete bounded sparse H and RHS. Physical/lift/history
construction is timed separately; its peak allocation is unavailable. It is not
the streamed MPI construction path described in [distributed history](distributed-history.md).
No quantum state, dense unitary or expanded primitive list is created by the
resource reporter. Only an independent N=4 test extracts the H† block from a
16-amplitude reference.

`max_bytes` admits the whole live managed numerical payload of the borrowed
history, declared external and callback owners, source/preparation resources,
and constructor/synthesis envelopes. Callback scratch is admitted even if the
optional inverse later fails. The zero-RHS library path also admits already live
payload, then skips every circuit constructor. Output strings/JSON, allocator
bookkeeping, opaque backend storage, RSS and preceding history-build peaks are
outside this payload model. Spectral, reciprocal-polynomial and QSP constructors do not expose attempted
peak allocation on error. Before any requested inverse construction starts, the
reporter therefore retains the complete 256 MiB admitted envelope, including
rejected spectral/polynomial/synthesis attempts. Failed construction also retains
its elapsed time. This is not a measured 256 MiB allocation.

Sparse greedy coloring can scan O(E² K), K≤E; completion scans edges and performs
bounded cycle searches. The reporter checks the model ceiling
`E³ + 32 (E+1)² (index_bits+1)` before constructing the matching source. This
counts a conservative scan/primitive model, not CPU instructions. The matching
constructor also performs an internal forward primitive-count pass; its count
and elapsed construction time are reported separately. RHS table compilation
reports its actual declared coefficient/work/storage counts. QSP synthesis has
separate degree, work and completion-grid limits.

A single counting-work budget is shared by source/RHS forward and adjoint
streams, inverse step/query inspection and inverse forward/adjoint streams.
The 20-work rejected fixture keeps the successfully compiled descriptor and RHS
resources, partial counts and rejection reasons. It does not label partial counts
as complete costs. Optional inverse/sampling failures likewise retain earlier
source evidence. Source/storage construction failures remain errors.

Counts describe portable X, H, Ry and scalar Phase primitives with signed control
masks. The receipt includes control occurrences and maximum control width.
Arbitrarily controlled primitives are not generally one/two-qubit gates and have
no implemented Clifford+T, hardware-depth or native dispatch-work decomposition
in this report. Recording elapsed debug-build timings does not establish a
hardware performance advantage. The receipt records current source and binary
hashes with an explicit provenance scope; these hashes are not a reproducible
build attestation. Numerical fingerprints distinguish source and construction;
persisted cryptographic integrity is a separate mechanism.
