# Stateless full symmetric Carleman generation

`quest_cfd::carleman_recipe::StatelessCarleman` provides row-addressable normalized symmetric dynamics without retaining the hierarchy's powers, index dictionary or coefficient-recipe list. It retains an `Arc<PolynomialOde>` containing every physical equation and already compiled coefficient kernel. Those complete physical forms still contribute their full modeled storage to admission; this does not make physical polynomial construction distributed or free.

For physical state `a`, positive scale `s`, and a multi-index `alpha` of degree `k`, the coordinate is

`z_alpha = c_alpha * product_i (a_i/s)^alpha_i`, with `c_alpha = sqrt(k! / product_i alpha_i!)`.

The hierarchy contains every coordinate of degrees one through the admitted order `r`. Its dimension is `binomial(m+r,r)-1`. Degree zero is an external source. Mean, auxiliary and conserved physical coordinates remain present, including their higher-degree monomials. The degree-one block maps physical axis `i` to hierarchy index `i`.

For one physical coefficient term `F_i(a,t) = f_i,p(t) * a^p`, differentiating `z_alpha` gives target

`beta = alpha - e_i + p`, and factor `alpha_i * c_alpha/c_beta * s^(degree(p)-1)`.

Targets above order `r` are omitted as explicit Carleman truncation. A degree-zero target contributes to the external source with `c_0=1` and `z_0=1`; all other targets become generator entries. Entries retain the stored implementation's stable axis/coefficient order and preserve duplicate contributions. Time-dependent coefficient values come from existing prepared MathCore kernels. No differentiation, multiplication, lowering, or symbolic expression manipulation occurs inside the row/source query loops.

## Exact indices and bounded query storage

Indices are grouped by increasing degree. Within each degree, complete exponent vectors are in descending lexicographic order, matching `SymmetricCarleman`. Rank and unrank count weak-composition suffixes with binomial arithmetic. They use iterative loops and checked `u128` intermediates, never a catalogue or recursive enumeration. Intermediate overflow is an explicit rejection. The existing arbitrary-width `carleman::symmetric_dimension` remains available for estimates; executable indices must fit `usize`.

A row visit allocates two exponent buffers of length `m`; coefficient evaluation uses one bounded physical-symbol input vector at a time. No row coefficient list is retained by this source. Rank uses a borrowed exponent slice; unrank returns a caller-owned length-`m` buffer. `lift_entry(row, physical_state)` evaluates one initial coordinate. `source_entry(time,row)` evaluates one external RHS entry. Multiple simultaneous calls, retained unrank buffers and visitor-owned row storage require the caller's aggregate admission.

`CarlemanRecipeLimits` separately bounds executable dimension, physical dimension, order (at most 64), physical coefficient-term count, bytes, construction work, index work and complete scalar/row query work. Lift depth is governed by this recipe's budget after physical kernels have been compiled; it does not require new symbolic monomials under the physical compiler's degree limit. Resource planning charges complete physical ODE forms, owner metadata, temporary exponent and coefficient-input buffers, borrowed physical initial data, index selection/binomial work, stable coefficient scans, normalization, and prepared kernel evaluations. Warm or repeated queries do not reduce their logical admission.

`CarlemanRecipeResources` exposes these conservative counts. They model wrapper-managed application storage and logical work, not allocator telemetry, measured runtime or a numerical error certificate. Numerical coefficient/normalization overflow or underflow is checked when the relevant scalar query runs; construction does not preflight every row of a huge hierarchy. A downstream sparse producer must finish its full admitted coefficient checks before quantum execution.

## History integration

The source implements `HistoryRowDynamics`, so the same causal DG1/DG2 `TemporalHistoryRecipe` accepts it directly:

```rust
let hierarchy = StatelessCarleman::new(physical_ode, order, scale, limits)?;
let history = TemporalHistoryRecipe::new(&hierarchy, horizon, slabs, temporal_order, history_limits)?;
let rows = history.rows(owned_history_rows)?;
let rhs = history.rhs_value(global_row, |i| {
    Ok(hierarchy.lift_entry(i, &complete_physical_initial_state)?.into())
})?;
```

This interface does not allocate a full lifted initial vector, history matrix or RHS. The downstream matching producer, distributed coherent preparation and native register retain their independent ownership, transport, state-memory and total-work admission. Spectral bounds, solve certificates and Carleman truncation assessments also remain separate.

## Focused evidence and limits

Tests compare powers, indices, row entries, external sources and scalar lifts with stored `SymmetricCarleman`, including quadratic coupling, time forcing and a conserved mean coordinate. An independent ordered-tensor hierarchy checks the generator on arbitrary hierarchy states. Complete DG1/DG2 temporal rows and scalar RHS match the stored source. Further tests cover scalar order 64, multiple physical dimensions, sampled rank/unrank roundtrips, degree-one recovery and explicit budget/domain failures.

A sampled source with `m=64`, `r=10`, and `D_r=718406958840` retains 144 modeled owner bytes and declares 2120 query bytes. Its complete borrowed physical ODE contributes 2294784 modeled bytes. These owner/query byte counts are identical to order one for the same physical ODE; index/query work grows with order and is charged. Only selected scalar/local history rows were generated in this case. This is evidence of bounded recipe storage, not evidence that a quantum simulation of that dimension fits, completes, or improves performance.
