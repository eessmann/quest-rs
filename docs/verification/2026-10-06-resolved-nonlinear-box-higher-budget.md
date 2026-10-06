# Resolved coarse nonlinear generated-box inverse signal

The separately approved higher-budget native trial passed on one and two local
MPI ranks. For each rank count, the complete coarse nonlinear history response
exceeded inverse error by more than four orders of magnitude. Both runs retained
the mandatory projector-phase certificate and passed the all-sector
forward/adjoint roundtrip.

The [numeric receipt](data/2026-10-06-resolved-nonlinear-box/higher-budget.json)
and [exact build-source manifest](data/2026-10-06-resolved-nonlinear-box/higher-budget-source.json)
identify this experiment. The earlier [fixed64MiB rejection](2026-10-06-resolved-nonlinear-box.md)
and its receipt remain unchanged: that attempt reached no inverse execution.
This higher-budget attempt was explicitly approved after its certificate storage
and simultaneous owners were reviewed; it was not an automatic retry.

## Discrete signal and inverse error

The complete five-coordinate periodic BDM1 physical chart, zero viscosity,
243-node configuration DG2 grid and off-center bump are unchanged. One temporal
DG1 slab now spans T=0.1, giving486 history coefficients. The construction uses
collective complete physical drift in agreed global row rounds, then independently
consumes local immutable COO spools to produce the matching encoding of H adjoint.
No full basis, configuration coefficient tensor, CSR or native state is gathered
by this production path.

A bounded independent reference retains the complete small physical chart,
weighted configuration generator and486-coordinate stored history. It aligns
with each native QR chart, verifies quadratic convection by polarization, and
compares every generated row against that reference. All ranks require the
reference spool digest to match the subsequently constructed producer input.
This comparison adds another11,016 collective drift calls per run, alongside
the actual producer source's11,016 calls; that repeated construction is charged
and reported separately from inverse replay.

Let x0=(z0,z0) be the exact zero-generator DG1 history. Let xr be the rounded
independent nonlinear reference and xq the recovered native logical success
amplitudes after physical rescaling. Define S=||xr-x0||/||x0||,
E=||xq-xr||/||x0|| and Q=||xq-x0||/||x0||. The test does not renormalize the
recovered success branch to hide an amplitude error. An outward complex residual
and generated/reference matrix perturbation bound give a separate reference
forward allowance A.

| Local MPI ranks | S, nonlinear signal lower | E, inverse error upper | A, reference allowance upper | Inverse relative residual |
| --- | ---: | ---: | ---: | ---: |
| 1 | 7.57169540449309e-4 | 3.277937244506261e-8 | 3.768074114767363e-15 | 3.278914534296887e-8 |
| 2 | 1.5215902462357982e-3 | 6.108231047071118e-8 | 3.8265773256085435e-15 | 6.111003015693383e-8 |

Both pass the unchanged requirements S>=4e-4, E<=2e-5,
S>=10(E+A), Q>=S-E-A and residual<1e-5. Endpoint signal lower bounds are
1.0705802564112412e-3 and2.151542755957922e-3. Independent source/reference
matrix Frobenius differences are below1.16e-15; reference residuals are below
7e-16 relatively. These finite-vector checks establish empirical native
separation with an outward reference allowance, not a uniform total quantum
execution error theorem.

Each actual matching encoding has normalization8, degree449 and15 qubits.
Reciprocal approximation bound is2.3769796259754337e-7 at requested tolerance1e-6.
The independently certified projector response bound is2.960774582083121e-14.
Native execution/RHS uniform error remains unavailable, and the observed inverse
success probability near0.0072333 is not a certified sampling lower bound.

## Fixed admission and measured process costs

This attempt keeps synthesis64 MiB, mandatory certification256 MiB per owner,
native64 MiB/rank, physical source512 MiB/rank and1 GiB/node. History inverse
admission is1 GiB/rank and2 GiB/node for at most two local ranks. The backend
conservatively charges native environment capacity, retained source/polynomial,
synthesis and two simultaneous certificate-owner caps. Both ranks execute under
verified hard and soft3 GiB `RLIMIT_AS`, set before their binary starts. Each child
has a600-second deadline; the two-rank case starts only after the complete
one-rank case passes. No further budget or criterion relaxation occurred.

| Local MPI ranks | Rank trial time | Projector certification time, rank0 | Three inverse replay time, rank0 | Per-process peak RSS |
| --- | ---: | ---: | ---: | --- |
| 1 | 274.55 s | 186.08 s | 83.62 s | 303.95 MiB |
| 2 | 324.63 s | 227.50 s | 90.30 s | 304.46 and303.47 MiB |

The numeric artifact retains exact per-rank `/proc` peak RSS and limits, GNU
`time` process observations, coherent RHS compilation/initialization, streamed
preprocessing, synthesis, certification and inverse timings. The two separate
process peaks sum to637,460,480bytes; this is a conservative sum, not a measured
simultaneous node RSS or shared-page count. Per-process address-space limits do
not enforce node RSS/cgroup limits. Wall times include collective waiting and
uncontrolled local host load; this is not a speedup campaign.

The classical reference is independently bounded at512 coordinates,128 MiB and
3 billion modeled operations. Its reported peak is about3.78 MiB for this small
history, with dense reference buffers confined to the test. Native readout retains
486 success coefficients and checks other branches in64-amplitude chunks; it
never copies the complete native state. No failed quantum operation falls back to
a classical solution.

## Scope and reproduction

These rank counts use different native QR charts. The tensor grid and fixed
chart-coordinate bump are not rotation-invariant, so the two runs are different
represented operators/ensembles, each checked against its own aligned reference.
They are not a same-A equivalence or equal-accuracy comparison across ranks.
The width0.55 bump touches configuration boundaries. Configuration/domain,
physical-mesh, temporal-refinement and continuum convergence remain open.
This is local native simulation, not multihost or fault-tolerant hardware evidence.

Run the separately named ignored fixture on Linux with installed matching MPI:

```sh
export QUEST_ROOT=/path/to/installed/quest
export MPICC=/path/to/matching/mpicc
export PATH=/path/to/matching/mpi/bin:$PATH
cargo test -p quest-cfd --features distributed --lib \
  box_kvn_history::tests::higher_budget_resolved_nonlinear_generated_history_inverse \
  -- --ignored --nocapture --test-threads=1
```

The wrapper requires GNU `/usr/bin/time`, `sh`, `timeout` and Linux `/proc` for
its explicit process-cap/measurement contract. Two pure regression tests and
strict distributed library/test Clippy passed before source freeze. The tested
binary was copied immutably; all685 Rust/TOML/Cargo.lock hashes matched before
and after its build. Later repository changes do not retroactively change this
source snapshot. The public receipts retain binary and installed QuEST/MPI hashes.
