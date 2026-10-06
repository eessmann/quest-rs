# Full-coordinate KvN refinement diagnostics

Run one bounded experiment with

```sh
cargo run -p quest-cfd --example kvn_refinement -- --cells 3 --order 1
```

Run the capped local campaign after building that example:

```sh
python3 docs/verification/fixtures/quest-cfd/refine.py \
  target/debug/examples/kvn_refinement /tmp/kvn-refinement.json
```

The physical system is the complete five-coordinate periodic two-triangle BDM1
fixture, including mean-flow coordinates and nonlinear convection, at viscosity
0.01. The initial center is `(0.15,-0.1,0.07,0.11,-0.04)`. The baseline is central
configuration DG1 on three cells per coordinate, domain `[-1,1]^5`, compact bump
width 1.2, horizon 0.001 and four classical RK4 reference steps. It is a coarse
discretization experiment, not a resolved Navier–Stokes benchmark.

The comparison evolves the full mass-weighted configuration amplitude and an
independent ensemble of trajectories of the identical nonlinear DG ODE. Each
trajectory begins at an initial configuration quadrature point with that point's
normalized probability weight. Coordinate means and physical kinetic energy are
weak observables; their discrepancies do not bound full amplitude or field error.
Classical RK4 time refinement checks the reference integration. It does not replace
the separate temporal DG history studies.

`regularization_resolution` requires a selected minimum number of distinct
physical nodes in every factor of the bump support. Coincident DG facet nodes
count once. Nonzero sampled support can fail this policy: a bump narrower than
grid spacing can put all sampled mass at one point. Passing two or three samples
is only a necessary diagnostic. It neither bounds quadrature error nor proves a
deterministic-limit approximation.

`concentration` reports coordinate standard deviations divided by distinct-node
spacing, maximum nodal probability, effective nodal coefficient count and outer-cell
occupation. An optional positive minimum standard-deviation policy rejects
concentrated states; the reference campaign retains diagnostics without such a
cutoff so deterioration remains visible. Effective nodal count includes distinct
DG coefficients sharing a point. Ordinary coordinate variance is unreliable for
a distribution wrapping around the artificial periodic boundary. Neither variance
nor outer-cell occupation is a certified leakage error or an outward flux.

The driver varies configuration cell count, polynomial order and regularization
width separately. Domain extensions add one full cell on each side while holding
the original cell width and quadrature locations fixed. The larger domain may
exceed admission; rejection is retained rather than dropping physical coordinates.
The deterministic-center trajectory is also recorded. Its difference from the
sampled ensemble mixes regularization and initial quadrature error until independent
configuration refinement separates them. No incompatible error norms are summed.

The first [recorded campaign](../../../docs/verification/data/2026-10-05-cfd-history/kvn-refinement.json)
contains two exploratory domain changes at fixed cell count, which also change
spacing. These are coupled probes, not isolated domain evidence. The corrected
[fixed-spacing domain runs](../../../docs/verification/data/2026-10-05-cfd-history/kvn-fixed-spacing-domain.json)
are separate receipts; the current driver uses that corrected protocol. All receipts
record the binary identity, arguments, memory cap and process RSS separately from
managed storage limits. A successful JSON experiment has `classical_validation`
status; model/admission failures have `rejected` status even when the example exits
normally after writing its report. Neither is quantum execution.

At fixed width/domain, coordinate-mean error decreased from about `3.80e-4`
(two DG1 cells) to `1.81e-4` (three) and `9.21e-5` (four). Three DG2 cells gave
`1.74e-5`. Kinetic-energy error was not monotone under every refinement; outer-cell
occupation remained substantial. Width 0.4 failed the support sampling policy.
Doubling reference time steps barely changed these weak errors, so reference
integration was not their dominant observed source at this short horizon.
These observations identify unresolved configuration/domain/regularization errors;
they do not establish a converged physical or quantum solution.
