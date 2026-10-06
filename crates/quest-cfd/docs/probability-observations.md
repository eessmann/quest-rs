# Probability-weighted KvN observations and sampling costs

`PreparedHistoryInverse::reduce_probability_observable` reads the rank-owned
native register in admitted chunks. It retains every failure, color, response,
control and padding amplitude. Only successful logical history amplitudes reach
the scalar observable callback. The existing linear Carleman readout remains
available as `reduce_observable`.

For returned native amplitudes `z`, selected logical coordinates `S`, diagonal
values `g_i`, and inverse physical scale `s=||b||/(c alpha)`, the new report records

```
P_total = sum all |z_i|²
P_inverse_raw = sum successful logical |z_i|²
P_selected_raw = sum selected successful logical |z_i|²
M1 = sum selected g_i |z_i|²
M2 = sum selected g_i² |z_i|²
conditional expectation = M1/P_selected_raw
conditional variance = sum selected (g_i-expectation)² |z_i|² / P_selected_raw
physical quadratic functional = s² M1
physical selected squared norm = s² P_selected_raw
```

The two reported success probabilities divide their raw mass by `P_total`;
`selected_probability_given_inverse` instead divides by `P_inverse_raw`.
`total_probability_mass` exposes register normalization drift. An empty selected
sector has no conditional expectation or variance. The variance uses a second
centered pass. All moments, norms and final physical scalings must be finite.
These are binary64 simulator reductions, not certified numerical error bounds.
Physical scaling assumes the supplied register is the relevant inverse output;
the reducer does not establish that provenance for arbitrary caller-written states.

The callback is a pure deterministic `Fn(index) -> (selected, real_value)` with an
explicit common source identity. It must never call MPI. Both passes query it
in the same order, including zero-amplitude successful coordinates. An allocation-free
replay digest detects changes; that digest does not prove callback semantics.
Finite range checks apply to every selected value. Rank-local callback failures
or panics agree at the next common chunk boundary before any rank advances.

## Physical time and full-coordinate kinetic energy

`HistoryProjection::CoefficientProjection` is explicitly an algebraic history
projection with no physical-time claim. `TemporalNodeSelection::new` instead
binds the exact temporal recipe semantics, slab and local DG node. Current DG1
and DG2 histories store nodal coefficients, so selecting one complete nodal block
recovers its configuration amplitude directly. At a slab interface, the preceding
right trace and following left trace remain distinct selections at the same time.
This selector describes a nodal trace. The separate
[temporal interpolation encoding](temporal-observations.md) implements the
coherent linear combination, including interference between temporal coefficients.
The [composed CPU/MPI observation](distributed-temporal-observation.md) performs
inverse-workspace selection, coherent interpolation and this reducer in the
required order, with joint probabilities relative to the original register.

The descriptor reports the temporal quadrature weight, but an instantaneous
conditional expectation does not multiply by it. Likewise, a coefficient projection
is not a time-integrated observable average. Prepared owners retain and charge the
semantic words used to reject mismatched node descriptors.

`KvnKineticEnergy::periodic_bdm1` and `box_space` require one configuration axis
for every independent coordinate of the supplied complete mass-orthonormal chart.
They return the diagonal `g(a)=0.5*sum_i a_i²`, with fixed scalar scratch and no
configuration-wide table. This is integrated kinetic energy, not volume-normalized
mean energy. The resulting readout is the **conditional KvN ensemble expectation**
of kinetic energy, not the kinetic energy of the ensemble mean or a deterministic
trajectory. The constructors inherit numerical chart whitening and assume no
additional affine velocity lift. All actual axis nodes are inspected to bound
rounded scalar evaluations; retained grid bytes, construction and per-query work
are declared. They do not account for arbitrary curved or lifted physical models.

A typical consumer constructs the energy recipe before discarding its bounded
physical reference, selects an exact temporal node, and supplies:

```rust,ignore
let energy = KvnKineticEnergy::periodic_bdm1(&grid, &flow, recipe_limits)?;
let node = TemporalNodeSelection::new(&history, slab, local_node)?;
let request = ProbabilityReadoutRequest {
    range: energy.range(),
    projection: HistoryProjection::TemporalNode(node),
    source_identity: declared_energy_source_identity,
    limits: ProbabilityReadoutLimits {
        callback_retained_bytes: energy.retained_bytes(),
        callback_query_work: energy.query_work(),
        ..Default::default()
    },
};
let observed = prepared.reduce_probability_observable(&state, request, |i| {
    let configuration = node.configuration_index(i)
        .ok_or(CfdError::InvalidInput("expected selected time node"))?;
    Ok((true, energy.value(configuration)?))
})?;
```

The complete nonlinear five-coordinate `kvn_recipe_collective` test uses this
consumer on the existing 243-configuration/486-history smoke. It is a circuit
and observable-semantics test, not resolved PDE or distribution convergence.

## Conditional sampling estimate

`plan_sampling` requires a finite observable range `[A,B]`, statistical absolute
error `epsilon>0`, failure probability `0<delta<1`, and a strictly positive
caller-supported lower bound `p_min` on the **joint** inverse and selected-projector
success probability. Nonempty evidence provenance is mandatory. The planner never
uses a simulator probability as a certificate and does not verify the caller's
range or probability premise. Fresh complete circuit repetitions must be i.i.d.

For the first `N` selected outcomes, Hoeffding's bounded-variable inequality gives
`P(|mean-E[g|]>=epsilon) <= 2 exp(-2 N epsilon²/(B-A)²)`. We assign half of `delta`
to this event and choose

```
N >= (B-A)² ln(4/delta)/(2 epsilon²), at least one selected shot.
```

This is the two-sided specialization of the bounded-independent-variable result
in [Hoeffding (1963), Probability Inequalities for Sums of Bounded Random Variables](https://www.cs.rpi.edu/academics/courses/spring06/random/hoefding.pdf),
[original publication](https://doi.org/10.1080/01621459.1963.10500830).

For `M` attempted shots and actual success probability `p>=p_min`, the success
count `X` is binomial with mean `mu=M*p`. An elementary exponential-moment bound
gives `P(X<=mu/2)<=exp(-mu/8)`: choose `t=ln 2` in Markov's inequality for
`exp(-t X)` and use `1+x<=exp(x)` and `ln 2<=3/4`. Therefore it suffices to require

```
M*p_min >= max(2N, 8 ln(2/delta)).
```

Then the probability of collecting fewer than `N` successes is at most `delta/2`.
The union bound gives total failure at most `delta`. This is our conservative
finite-sample Bernoulli specialization of the exponential-moment method associated
with [Chernoff's original report](https://statistics.stanford.edu/technical-reports/measure-asymptotic-efficiency-tests-hypothesis-based-sum-observations).
The first `N` success values have the conditional distribution independently of
how many unsuccessful trials precede them; failed attempts are not usable samples.

Arithmetic, logarithms and upper ceilings use outward intervals. Counts above
`2^53` reject, including a ceiling crossing that boundary; allowed binary64-to-integer
conversions are exact. Explicit selected and attempted shot limits also apply.
A constant observable conservatively asks for one selected shot. The estimate
executes no quantum measurement and establishes no stationarity, physical
convergence or deterministic-limit accuracy.

Statistical `epsilon` excludes fixed preparation, polynomial, physical,
configuration, temporal and floating-point biases. `systematic_bias_bound=None`
keeps their total unknown. A caller may separately supply a nonnegative bound,
which is added outward to statistical epsilon and remains a caller premise. It
must bound this **conditional normalized observable**, including postselection
amplification and the initial-ensemble to physical-target discrepancy. A raw
amplitude-error bound alone cannot serve as that bias bound.

`sampling_cost` reports one fresh coherent preparation and complete inverse per
attempt, the existing preparation elementary-gate count multiplied by attempts,
`2*degree` matching-oracle calls per attempt, measured register bits, declared
selection/decoding work on **every attempt**, and separately declared classical
observable work on the first N selected outcomes. These are estimates of repeated
circuits, not actual measurement records or free oracle calls. Matching-oracle
execution/compilation has its own resource receipt; it is not an elementary gate.
Thus the returned counters do not establish complete elementary-gate or end-to-end
CFD sampling costs. The pure planner alone counts shots, not gates.

## Admission and limits of this substage

Readout preflights both full amplitude passes, conservative callback queries and
all fixed/control reductions. It charges callback retained bytes/scratch, two
chunk buffers and fixed scalar scratch before native reads; the existing complete
native-cap plus external-owner rank/node admission remains in force. A temporary
external reservation protects this payload during both passes. Transport bounds
cover logical control packets, not MPI internal protocol or allocator metadata.
No global state, diagonal table or sampled-outcome array is allocated.

Focused tests cover known finite distributions with nonzero failure/padding/control
branches, rescaled unnormalized registers, changed callbacks, rejected range/source/
time semantics and one-less work/transport/byte budgets at MPI 1/2/4 and split
communicators. Independent full-chart energy evaluations cover both the five-coordinate
periodic fixture and a full BDM2 box. This closes a diagonal KvN observation and
conditional sampling-cost substage. Prepared bounded physical observables and the
composed inverse/time simulation have their own linked contracts. This reducer
does not execute an experimental measurement campaign or construct general coherent
pressure/traction value oracles.
