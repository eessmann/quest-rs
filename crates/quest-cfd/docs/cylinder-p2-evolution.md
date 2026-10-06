# Bounded complete P2 cylinder evolution

The additive `PreparedCylinderPhysical::evolve_with_receipt` API advances the
original prepared polynomial-boundary dynamics in all 54 coordinates. It shares
one private time-aware RK4 implementation with `PolynomialOde::integrate_rk4`.
The [supplied-state snapshot API](cylinder-high-order.md), geometry, existing
polynomial validation and default budgets retain their prior contracts.

The request admits only two, four or eight steps on `[0,0.0001]` for the fixed
4-sector, one-layer Re100 P2 fixture. `PreparedMinimumMassCompatible` requires an
exact match to the prepared initial chart vector; this represents the full
minimum-mass compatible lifting, not zero velocity everywhere in the interior.
`SuppliedComplete` admits a finite full vector with explicit caller provenance.
Neither policy removes coordinates or claims a steady initial flow. The original
lifting derivative participates at each actual RK4 stage time.

## Attempts, costs and failures

The wrapper pre-admits all integration, initial/accepted-step energy queries,
three final quadratic-action probes and exactly one final original-pressure and
physical-observation snapshot. There is no initial pressure solve. The integration
and diagnostic envelope is conservatively charged before callbacks, including any
unused portion after numerical failure; actual attempted drift/energy/probe counts
are reported separately. These counters scope direct RK4, initial/accepted-step energy and quadratic-probe callbacks; they do not count every internal force evaluation inside the final pressure bundle. The final snapshot consumes its existing composite charge once.

Using the existing drift bound `D=7,105,536` and `m=54`, the complete work charge is

`1,634,281,088 + (4 + 5*steps)*D + 64*m*steps`.

It gives 1,733,765,504 / 1,804,827,776 / 1,946,952,320 work for two/four/eight steps,
under the example's declared 2 billion allowance. All component caps and the public
1-billion workflow default remain unchanged. The default cannot prepare this full
fixture. A repeated attempt uses the same remaining ledger; no retry receives a
fresh work budget.

A separate 64 KiB integration reserve covers complete state/stage/candidate/probe
vectors, retained progress and energy samples. The helper audits actual returned
Vec capacities before subsequent callbacks and computes the final stage combination
in a separate candidate before accepting it. Physical callback scratch is covered
by the prepared source's admitted query peak. The complete last state and final
pressure arrays stay live through output. The example declares another 64 KiB
serializer buffer as external storage before source construction. Borrowed input
slices expose only accessible bytes; callers must declare unused backing capacity
and other live owners via the source's `mesh.external_retained_bytes`. No API can
infer hidden caller ownership from a slice.

An attempt always records its last finite accepted state once integration has
started. Drift/candidate failure preserves the prior state. An observer failure
retains the newly accepted state and completed-step count but marks diagnostic
failure. Preflight rejection has zero callback counts and an empty state. The
absolute time interval is checked before callbacks, including overflow of the final
stage arithmetic. `PolynomialOde` retains positive-finite-dt validation and its
zero-step behavior, including signed-zero initial data.

Managed payload/work admission is not an OS memory bound or wall-clock prediction.
Unknown allocator/environment costs retain the underlying affine/MathCore scope
qualifications. A killed process may have no returned numerical attempt; the runner
records timeout and raw capture rather than inventing a last state.

## Executing the fixed three-row comparison

Build the example with the normal native dependency configuration, then give the
runner the resulting immutable binary and a new output directory:

```sh
cargo build -p quest-cfd --example cylinder_p2_evolution
python3 -B crates/quest-cfd/examples/cylinder_p2_evolution.py \
  target/debug/examples/cylinder_p2_evolution new-evolution-results
```

The runner executes exactly the 2/4/8 rows once, serially, with 512 MiB address
space, 180 seconds and 4 MiB captured-file limits per child. It pins the binary
hash, retains raw JSON/stderr and incremental process receipts, and never changes
caps or accuracy thresholds after failure. Repository-specific Cargo target-layout
configuration may place the executable elsewhere; pass that actual binary path.
Source-to-binary build attestation is a separate publication artifact, not implied
by the runner's binary hash.

A completed integration is separate from accuracy. The runner requires original
pressure/continuity residuals no larger than `1e-8`, and finer/coarser final-state
and observable sensitivity no larger than `1e-5*max(1,|fine|)`, with nonincreasing
differences up to a `1e-12` scaled roundoff floor. It compares complete coordinates,
drag, lift, pressure difference, mean kinetic energy and enstrophy. Difference
ratios below the resolution floor are absent; no fourth-order conclusion is forced.
Failed sensitivity remains `accuracy-failure` even when every integration completed.

The three final drift calls measure
`F2(a,a) = (F(a)+F(-a)-2F(0))/2`; its scaled resolution floor is `1e-10`.
This is an algebraic quadratic-action diagnostic, not a full-minus-linear trajectory
comparison, forward-error certificate or spatial convergence proof. Independent
analytic nonlinear/nonautonomous ODE tests verify the shared integrator's order,
and a polynomial lifting test checks cancellation by the original ell_dot term.

This very short experiment does not execute the historical `[4,8]` project window
or the [official DFG2D2 developed cycle](https://wwwold.mathematik.tu-dortmund.de/~featflow/en/benchmarks/cfdbenchmarking/flow/dfg_benchmark2_re100.html)
in `[25,30]`. Natural zero traction does not guarantee stability under backflow;
no stabilization has been silently added. The coarse polygon and explicit RK4
remain independent physical/time approximation limits. Official-window evolution,
configuration-space refinement and the literal three-dimensional wake remain
separate acceptance work.
