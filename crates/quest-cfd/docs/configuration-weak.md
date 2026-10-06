# Initial configuration weak-generator diagnostic

`configuration_weak::PeriodicWeakSource` owns the original complete periodic
BDM1/P0 two-triangle source and its shared polynomial snapshot. All five physical
coordinates, including conserved means, remain present. `diagnose` validates every
entry of a supplied full configuration state, then contracts generated `KvN` rows
without storing a generator, drift table, commutator or action vector.

This is a bounded classical initial-state diagnostic. It is not a history solve,
quantum measurement, proof of convergence, or substitute for width/domain/history
refinement. An arbitrary supplied complex state is supported when admitted; the
API does not infer an initial regularization or certify its resolution.

## API and numerical meaning

`PeriodicWeakSource::prepare(viscosity, external_retained_bytes, limits)` returns
an owning result and a construction receipt even when preparation fails. The
original full model and extraction evidence have read-only accessors.

`source.diagnose(grid, state, request, limits)` returns an outcome, phase,
validated/visited row counts, and cumulative resource receipt. `WeakRequest`
contains finite time, additional external live bytes, an optional caller-declared
input-preparation work charge and an optional original-force nonlinear witness.
`None` for preparation work means unknown/outside this borrowed-input operation,
not verified zero. Adding a charge cannot retrospectively admit earlier allocations;
an enclosing producer must preflight its grid and state construction itself.

For each of the five coordinates and integrated kinetic energy, the report gives
initial expectation, raw skew-identity rate, normalized rate, original sampled
physical rate, and absolute/scaled defect. With `q = z* z`, the raw rate is

```
2 Re sum_i conj(z_i) M_i (Lz)_i / q.
```

The normalized rate additionally subtracts `<M> * qdot/q`, where
`qdot = 2 Re sum_i conj(z_i) (Lz)_i`. Exact skew-Hermiticity makes qdot zero;
floating evaluation does not assume bitwise cancellation. Row contraction preserves
complex interference. Scaled defects use `max(1, abs(physical_rate))`, so a zero
conserved-mean rate does not create a meaningless relative error.

The physical side evaluates the original constrained DG force at each retained
sample. Kinetic energy and its directional derivative are independently integrated
from the twelve Cartesian affine velocity coefficients on the two triangles,
using analytic monomial moments. The report compares those values with the
mass-coordinate expressions and retains the original chart and coefficient
extraction residuals. These are numerical checks, not exact assembly certificates.
The source fingerprint covers the fixed recipe version, viscosity and full chart;
it is noncryptographic provenance. Source/build SHA evidence belongs to an external
execution record, not this fingerprint.

The optional nonlinear witness is the sampled expectation of `||F2(a,a)||²`,
using original-force polarization. It is not a full-minus-linear trajectory
comparison. A nonlinear energy rate need not be nonzero: the central periodic
incompressible convection conserves kinetic energy.

## Boundary and sampling scope

Only a row with **both** real and imaginary amplitude components exactly zero is
omitted. An amplitude whose squared norm underflows remains a queried row. All
state entries and all physical coordinates are retained. This omission is valid
for the contracted initial rate; it is not a sparse-support time evolution rule.

`zero_exterior_trace` checks sampled amplitudes on exterior LGL nodes. It is separate
from outer-cell occupation and is not a flux calculation or continuum support
certificate. Ordinary coordinate/energy observables are not periodic on the
artificial configuration torus; nonzero boundary traces can contaminate comparison
with the original physical expectation. Coordinate standard deviations, maximum
coefficient probability and effective coefficient count remain diagnostics only.
Duplicate endpoint coefficients are not distinct spatial samples.

Call `regularization_resolution` separately when an explicit compact bump and
minimum-node policy are intended. At the proposed center `[.15,-.1,.07,.11,-.04]`
and width `.5`, one-cell DG2 has only one distinct support point per coordinate and
rejects a minimum of two. DG1 with three/four/five cells and DG2 with three cells
pass that necessary count, but their sampled distribution can still be very narrow.
No minimum-node or concentration threshold is weakened by this module.

## Admission and failures

Defaults are 256 MiB managed payload, 1 billion source/query arithmetic units,
100 billion original physical arithmetic units and 1 million original-force calls.
They are hard upper bounds in this initial bounded module. Each invocation charges
source construction plus one diagnostic; it does not decrement a lifetime budget.
Cache reuse never makes construction logically free.

Preparation reserves an audited fixed 8 MiB and 100 million source-work envelope,
including the complete numerical source, polynomial lowering and temporary owners.
Its 58 original force probes have a separate 100,000-unit bound per call. Attempted
calls are counted through the shared extraction callback even when it fails early.
The complete symbolic input remains limited to six symbols, 256 terms, degree four,
256 coefficient bits, 1 MiB and 1 million symbolic work units per shared admission.
Actual retained model capacities and inherited polynomial payload receipts are
checked against the envelope. These are managed arithmetic/payload bounds, not
allocator metadata or process-memory measurements.

Before scans and generated row queries, the consumer admits source/grid metadata,
full accessible state bytes, fixed result owners, a conservative 16 KiB scratch
allowance, and declared external owners. Source-work charges include:

- Construction and any caller-declared preparation work.
- `4096*(axis_dimension + full_dimension)` for complete validation/readout scans.
- `128*polynomial_retained_bytes + 4096` for recipe metadata/kernel validation.
- For each exactly nonzero row: the actual recipe's row-work allowance,
  `64*maximum_row_entries`, and 16,384 units for physical-coordinate readout,
  Cartesian moments, normalization, concentration and optional polarization sums.

The original physical calls are separately pre-admitted: one per supported row,
or two per supported row plus one zero evaluation for the nonlinear witness.
Each costs the independently audited 100,000-unit arithmetic allowance. All
arithmetic uses checked integer admission; every published floating result is
finite-checked. Actual returned point/force/signed-probe Vec capacities are audited
against query scratch while simultaneously live. No phase starts with a fresh
allowance. Historical construction peak remains in a later rejection receipt.

The borrowed state contributes its accessible slice payload. Hidden backing Vec
capacity, earlier retained outputs and other owners must be declared externally;
the API cannot inspect them. Shared numerical payload models and safe allocator
behavior do not hard-bound arbitrary host environment, allocator metadata or OS
memory. Any future campaign requires separate process/output/time caps.

The necessary support count does not guarantee budget admission. In particular,
the complete DG2 three-cell candidate is rejected before row contraction under
the unchanged source cap. No campaign has been run as part of this module stage.

## Independent maintained checks

Tests cover exact constant/affine transport of compact piecewise-polynomial
amplitudes, a manually assembled complex commutator, an intentionally non-skew
normalization correction, and a nonzero-boundary-trace example with the expected
periodic mismatch. Full five-coordinate tests compare rates with independently
assembled original-force sparse rows, include a nonzero last-coordinate action,
and compare analytic Cartesian mass moments with independent triangle quadrature.
Other tests retain tiny nonzero amplitudes, full tensor support rejection, failed
extraction counters, malformed/zero states, caller-declared storage/preparation
and one-unit-lower work, physical-call and byte limits.
