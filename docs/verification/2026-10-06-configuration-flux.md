# Instantaneous configuration boundary flux

`quest_cfd::configuration_flux::boundary_flux` evaluates exterior LGL traces of a complete mass-weighted configuration state with a prepared full `PolynomialOde` at an explicit physical time. All chart coordinates remain in the tensor grid and the drift. This is a bounded local numerical diagnostic; it does not supply distributed, coherent or constant-cost access to a complete state.

For a node `i` on a face normal to coordinate `j`, the normalized face quadrature contribution is

```text
trace_i = (|z_i|² / sum_k |z_k|²) / w_j
signed_rate_i = trace_i * n_j * F_j(time, a_i)
```

Here `w_j` is the one-dimensional quadrature weight, including the element Jacobian. Dividing the tensor mass by this weight leaves the tangential face quadrature. Lower faces have outward normal −1 and upper faces +1. Only axis indices zero and last are exterior; duplicated internal DG facets are excluded. Corner nodes contribute to each incident face, with one evaluation of the full drift per exterior tensor node. There is no global drift or point table and no symbolic computation inside the numeric traversal.

Each lower/upper face, each axis, and the whole domain report positive outward rate, positive inward magnitude and their signed difference. Rates have units inverse time. A face's `normalized_trace` has units inverse coordinate length and is not a probability mass. The result also records the unnormalized norm squared and normalized **outer-cell occupation**, using the existing occupation convention; occupation and exterior traces select different sets of nodes.

The status is `NumericalInstantaneous`. Periodic configuration evolution is unchanged: positive outward diagnostic flux does not imply loss of the represented state norm. These values are neither integrated escape probabilities, leakage bounds, absorbing-boundary evolution nor convergence certificates. Configuration extent, spatial/order/time refinement and comparison with an appropriate physical diagnostic remain scientific obligations.

## Admission and accounting

The defaults are 256 MiB modeled peak bytes, one billion modeled operations and one million drift evaluations. Callers may provide fixed explicit ceilings; rejection never enlarges them. Checked arithmetic computes the exact exterior-node count `B = N − (n − 2)^m` and face-contribution count `C = 2mN/n`, where `N = n^m`. Zero-amplitude exterior nodes still evaluate the full drift and count toward `B`.

Before scans or allocations, the diagnostic admits the construction-validation and full tensor traversal allowance. Its validation allowance is `128 * ode.retained_bytes() + 1024n + 128m + 4096`. The immutable uniform P1/P2 axis grid has at most five derivative entries per row; the cached complete ODE allowance covers the coefficient and exponent storage scanned by admission. It then reuses `KvnHistoryRecipe` input validation and query accounting. Each of the `B` actual drift calls is conservatively charged **an entire KvN row-query allowance**, even though no generator row is evaluated. This deliberate overcount avoids a second coefficient-work engine. Traversal costs are `256N(m+1) + 64C`; all validation, traversal and query work is summed before any drift evaluation. The units are conservative arithmetic/index/validation operations, not CPU cycles or measured timings.

Peak bytes include actual retained grid capacities, the ODE's inherited conservative complete prepared-input allowance, the borrowed KvN descriptor, all accessible state entries, the result descriptor and actual axis-vector capacity, actual reusable-point capacity, and the inherited conservative KvN query scratch envelope. Planned capacities are admitted before allocation; actual point/result capacities are checked before evaluation. Returned full-drift capacity is also checked against its query envelope. Only coordinate-sized point/drift scratch and coordinate-sized result storage are added; there is no configuration-wide temporary array. Already completed grid/ODE construction has its own admission and is not counted as newly performed symbolic work.

The state API accepts a slice. Its receipt charges `state.len() * size_of::<Complex64>()`; inaccessible spare allocation behind that slice belongs to the caller's accounting. Allocator metadata, unrelated caller allocations and real process RSS are outside this modeled receipt. This is an explicit accessible-payload policy, not a claim that the receipt measures the whole process. Resource receipts distinguish validation, traversal and query work, exact calls/contributions, accessible state bytes, retained input bytes, actual result capacity, scratch allowance and the total peak.

Wrong full coordinate/state shape, nonfinite input/time/drift/arithmetic, zero or unrepresentable norm, allocation failure, overflow and exceeded limits reject the diagnostic. Inputs remain immutable, including on failure.

## Focused evidence

The new analytic regression first failed with the unimplemented diagnostic. Eight maintained tests now cover constant density and velocity across multiple cells/orders, compressive drift, explicit time forcing, nonconstant polynomial density, complex unit phases, exclusion of duplicate internal facets, exact corner/call counts, nonlinear coupling into the fifth coordinate, malformed input/kernel errors, exact resource ceilings and the accessible-slice spare-capacity policy. The constant-density case gives outward = inward = `|v|/L` per coordinate and net zero; the compressive case has inward flux and zero outward flux. The independent fifth-coordinate example has `F_4=a_0²+2a_4`, giving lower/upper outward rates `5/6` and `7/6` on `[-1,1]^5`.

Reproduce the focused checks with:

```sh
cargo test -p quest-cfd --test configuration_flux --test kvn_recipe
cargo clippy -p quest-cfd --lib --test configuration_flux --no-deps -- -D warnings
```

These checks exercise bounded local diagnostics and the unchanged generated KvN resource engine. They do not establish configuration-boundary convergence or modify the paired-history campaign schema.
