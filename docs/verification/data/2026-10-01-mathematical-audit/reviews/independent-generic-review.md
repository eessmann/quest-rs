# Independent review: generic functions and Remez

Reviewed the actual `quest-polynomial/{function,backend,typed,remez}.rs`,
`quest-qsp/src/offline/remez.rs`, relevant `precision.rs` exact interchange,
plan Task 1, the task-1 report and analytic tests. This reviewer did not implement
those files. No implementation edits or heavy verification gates were performed.

## Findings

### [P1] Meter trusted expression admission before expanding shared DAGs

`quest-polynomial/src/remez.rs:95-99` evaluates the whole target interval before
resource admission. `quest-qsp/src/offline/remez.rs:114-120` evaluates the full
second-order interval jet before `.policy()` can apply its runtime budget.
`ScalarBackend::visit()` uses Backend's default no-op, and Function's evaluator
only limits depth to 256 (`function.rs:164-180`). Depth does not bound logical
node visits in shared expression DAGs.

A concrete compact input is `let mut e=Expr::variable();` followed by 70 repeated
`e=e.clone()+e;`. It allocates only about 70 Arc nodes and has depth 71, but its
recursive evaluator visits roughly 2^71 nodes. Cached metadata already reports
saturated nodes (`usize::MAX`), but both admission paths ignore that count and
start evaluation anyway. Domain admission can therefore hang before reaching
any configured work budget. Native candidate admission also models only QR
`count^3*iterations`, omitting trusted expression visits in interval isolation,
bounds and reference evaluations. This gap predates parts of the rewrite but
remains material to the approved bounded-work requirement.

Recommendation: use cached sealed-expression metadata to reject excessive
logical work before any numerical domain evaluation, and meter/pre-admit the
native evaluator's repeated work against the native policy. Offline numerical
derivative-domain validation can be deferred to policy admission after metadata
and limits are checked, or use an explicit finite admission cap in the earlier
transition. Preserve the documented distinction that open callable execution
is caller-controlled. Add a compact 70-level DAG regression asserting prompt
budget rejection, without actually expanding its tree.

The owner and parent have been notified with precise lines and are fixing this.

### [P2] Add retained-target failure reporting to the native owning builder

`RemezBuilder<ReadyRemez<E>>::run(self)` (`remez.rs:133-138`) calls the numerical
core while borrowing `self.state.target`. On any error the consumed builder is
dropped; `Error` contains no original target or domain. The new generic success
report retains its target, but native failure reporting does not satisfy the
approved ownership-through-failure contract. A caller can clone a Function
beforehand, but this is not a retained failure result and is unavailable for
non-Clone custom callable targets.

Recommendation: add `run_reported`/`try_run` returning a private-constructed
`RemezFailure<E>` with original Function<E>, domain/options and underlying Error.
Keep legacy `run()` as an error-only compatibility adapter. Route the static
degree wrapper through the same reporting core. If required for the open
callable route, retain its target and explicit premise in a distinct conditional
failure report. Offline terminal Numerical failures already retain an exact
compatibility expression through structural erasure; this preserves captured
constant bits although it intentionally erases the static expression type.

The parent has been notified; no broad API redesign is needed.

## Verified by source inspection, no additional findings

- `Expression` is sealed through a non-public supertrait module; open Backend,
  Callable and GenericCallable do not allow a downstream evaluator to forge
  a trusted Function expression. Trusted and conditional result payloads have
  private constructors/data. Conditional bounds retain and expose the premise;
  there is no unconditional conversion.
- Shared JetBackend product, reciprocal and chain rules implement first and
  second derivatives correctly: product cross term 2*a'*b', reciprocal
  derivatives -v^2 and 2*v^3, sqrt derivatives 1/(2*sqrt(a)) and
  -1/(4*sqrt(a)^3), plus correct ln/exp/sin/cos chain rules. Producer MP and
  verifier intervals share structural AD but use separate arithmetic policies.
- The offline Context delegates the same sealed expression/AD traversal and
  charges visit/arithmetic operations. Each precision retry reuses the original
  generic target, rebuilds numerical state and preserves cumulative work.
  Successful export is checked against the original target by interval
  arithmetic. Exchange gap is correctly labeled empirical, not a minimax proof.
- Backend point injection calls exact_from_f64; source bits determine integer
  significand and exponent, including subnormals and signed zero. No decimal
  reparsing or candidate-to-target replacement was found.
- Offline interval enclosure covers all accepted subdivisions, bounds original
  function minus frozen exported polynomial, and charges its modeled worst-case
  jet visits before executing them. The newly added recursive 32-scalar-per-depth
  storage allowance at maximum precision is conservative for the shared AD
  evaluation temporaries reviewed here, separate from caller-owned input data.
- Typed const metadata describes structural nodes/depth/operations/domain
  requirements and does not declare runtime convergence or positive arguments.
  StaticDegree's N+1/N+2 generic const bounds check representable dimensions;
  runtime options reset degree to N. Fixed-array borrowing is checked without
  copying or allocating a degree-sized stack array.
- Default Function/Expr, function! and Function::new(value.into()) compatibility
  are preserved. Constant-only typed functions, captured f64 bits and custom
  non-Copy backends are covered by focused tests inspected here.

Review limited to the named generic-function/Remez scope. Own utility code was
not reviewed in this report. Parent owns integrated regression verification.

## Repair reinspection

The generic owner has addressed both findings; reviewed the actual fixes without
editing them. Native domain admission precharges cached metadata. LimitedFunction
then cumulatively precharges each value/jet plus polynomial work, reserves QR
work, retains admission work, and rejects oversized shared DAGs before expansion.
Standalone native root isolation also uses this wrapper. Offline domain now
admits interval geometry only; policy checks storage and cached jet work before
numerical derivative validation, and carries that charge into solve.

Native, static-degree and conditional callable builders now provide additive
`run_reported()` methods with privately constructed retained-target failure
reports. The legacy error-only `run()` adapter explicitly documents that it
discards the request on failure. Conditional failure retains its premise.
The owner added focused regressions for 70-level DAGs, tiny domain/run budgets,
retained exact constants/static requests, and conditional targets/premises.
No remaining blocker found in the repaired generic-function/Remez scope;
parent's integration gates provide the final execution evidence.
