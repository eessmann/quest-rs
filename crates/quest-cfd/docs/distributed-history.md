# Streamed MPI temporal-history inverse

The `distributed_history` module connects row-addressable temporal DG, collective
matching preprocessing, distributed coherent RHS preparation, geometric reciprocal
QSP synthesis and native whole-unitary matching replay. It streams owned input rows
and scalar RHS queries. It never builds a complete history matrix, CSR, lifted RHS,
gate stream, permutation or gathered state. The example uses `StatelessCarleman`,
including every physical coordinate and every monomial through the selected order;
it retains no hierarchy powers or coefficient catalog.
The [stateless Carleman contract](../../../docs/research/stateless-carleman-recipe.md)
details exact index ordering, truncation and charged physical input. The same
adapter accepts the reviewed full-coordinate `KvnHistoryRecipe` described below.

Build against an MPI/subcommunicator-capable QuEST installation and the same MPI
implementation used to build it:

```sh
export QUEST_ROOT=/path/to/quest-install
export MPICC=mpicc
cargo build -p quest-cfd --features distributed --example distributed_history
mpiexec -n 4 target/debug/examples/distributed_history \
  --spectral-lower 0.5 --spectral-upper 3 --max-degree 511 \
  --inverse-repetitions 2
```

Adjust the executable path if `CARGO_TARGET_DIR` is set. These spectral numbers are
explicit **caller premises**, including in the runnable small example: this program
does not independently establish them. They must enclose the singular values of
the exact complete lifted temporal matrix, rather than those of the physical ODE
or one temporal block. The printed report preserves this provenance. Unsupported
size, work, degree or capacity requests return errors. There is no stored-reference
or dense-dilation fallback. `--uncertified-phases` deliberately disables phase
response certification and prints no response bound.

The physical Burgers polynomial ODE and initial physical vector remain replicated
and charged. The supplied Burgers factory is a bounded materialized physical
baseline, not an arbitrarily large physical mesh factory. The generic adapter also
accepts other immutable `HistoryRowDynamics` implementations with declared kernel,
row scratch and query costs. All ranks must describe one common generator; fixed
temporal metadata agreement binds horizon, cells, order, dimensions, column ordering
and ordinal slots before querying the RHS. It cannot certify arbitrary caller
generator coefficients. Coefficient ownership may differ by rank. Producer input
uses conjugate-transposed entries with unchanged stable ordinals to encode H†.

`initialize_rhs` admits the owner, width, controls and complete preparation before
resetting the native register. Unexpected errors after that explicit mutation
boundary abort the collective job. Zero RHS returns before sparse production,
synthesis or register mutation. Forward, literal adjoint and signed-control RHS
replay preserve the entire state, including inactive controls, matching failure,
color, response and padding sectors.

The reciprocal target satisfies `p(x) ≈ c/x` for singular values of H/α. The clean
success amplitudes after applying the inverse to b/‖b‖ therefore approximate
`(c α / ‖b‖) H⁻¹b`. Physical readout multiplies amplitudes by **‖b‖/(c α)** and squared
norms by its square. Scalar observable weights can additionally decode Carleman
scaling; the example multiplies degree-one amplitudes by the hierarchy scale.
`reduce_observable` reads fixed bounded local chunks, reduces five scalar sums and
reports total mass, inverse success, selected mass, conditional selected fraction,
physical selected squared norm and a complex linear observable. Successful logical
indices alone reach the caller's time/degree callback. It rejects nonfinite callback
values, intermediate sums and every final scaled component collectively. It does
not discard failed or padded amplitudes from the native state.

The report separates geometric approximation, exact converted-projector response
certification, conversion diagnostics and an optional explicit native-execution
amplitude-error premise. A conditional relative residual bound is emitted only
when both phase certification and that execution premise are supplied. Neither QSP
certification nor a caller singular-value premise proves Carleman truncation error,
DG accuracy, PDE convergence or numerical execution error.

## Costs and evidence

For padded logical dimension D>1, the portable coherent RHS baseline uses 5D−6
elementary gates and 2(D−1) coefficient queries, with distributed retained Walsh
coefficients and admitted compile/transport work. There is no free state-loading
oracle. Degree d native QSVT replays 2d complete matching oracles within 4d+5 semantic
steps; lowering, signed-control scans, completed-permutation routing and native
state work have their own runtime admission. The native simulator still stores its
rank-owned part of the full 2^q register. Streaming input does not remove that cost.
Offline reciprocal coefficients, synthesis and phase certification are replicated
scalar work proportional to the selected polynomial problem, independent of the
distributed history storage.

Receipts include actual producer edges/colors/work/transport, coherent RHS gate,
query, table and message counts, normalization α, degree, physical scale, managed
memory envelopes and wall times. Stage times distinguish physical/stateless source
creation, RHS compilation, sparse preprocessing, reciprocal construction, phase
synthesis, projector certification, native preparation, initial-state preparation,
last and cumulative inverse replays, and scalar readout. Repetitions reuse the
prepared owner, inserting literal adjoints between forward applications. The
cumulative replay counter includes those adjoints. Times are rank-local observations
including collective waits; they are not speedup or critical-path measurements.

Capacity admission combines the full native environment cap, external retained
leases, row/query scratch, readout chunks and offline scalar stages. It charges
opaque synthesis candidates by the full admitted synthesis cap, and simultaneous
certificates by twice their cap. These conservative envelopes may count overlapping
storage twice and reject otherwise feasible requests. Node limits use the maximum
rank envelope times an explicit ranks-per-node declaration; the default assumes
every communicator rank shares one node. This is managed application accounting,
not RSS, allocator overhead, MPI-internal storage or a user allocator quota.

The focused tests compare a bounded independent stored complex forced history and
classical solution against the streamed path at 1/2/4/8 ranks and split groups.
They independently check orientation and physical scaling, and exercise arbitrary
native workspace/padding states, signed controls, forward/adjoint round trips,
collective callback errors, metadata mismatch, rejected budgets, failed preflight,
overflow and zero RHS. Stored histories and reference solutions occur only in
these bounded tests. Local MPI equivalence does not establish multihost behavior,
theorem-certified CFD convergence or a quantum speed advantage.

## Generated full-coordinate KvN consumer

[`KvnHistoryRecipe`](../src/kvn_recipe.rs) borrows the complete configuration axis
grid and a prepared `PolynomialOde`, with exactly one configuration axis per physical
coordinate. Each row evaluates the full physical drift at its point and each DG
derivative neighbor to emit the same mass-scaled central split generator as the
stored reference. It retains no configuration-wide drift table or generator.
`CompactBumpRecipe` supplies mass-weighted initial amplitudes by scalar query;
construction rejects absent sampled support or complete floating underflow.
Neither callback performs symbolic lowering during replay.

Run the focused pure and collective consumers with:

```sh
cargo test -p quest-cfd --test kvn_recipe
cargo test -p quest-cfd --features distributed --test kvn_recipe_collective -- --nocapture
```

The independently rerun [consumer test](../tests/kvn_recipe_collective.rs) used
actual MPI 1 and 2 jobs, all five nonlinear BDM1 physical coordinates, one
configuration DG2 cell per axis (243 coefficients), and a DG1 history of dimension
486 over T=0.001. Both lanes used inverse degree 115 and returned endpoint/initial
squared-norm ratio 0.9993106540029578, with probability and projector-response
checks. Their spectral intervals remain labeled caller premises. These are
consumer/circuit checks, not resolved configuration transport or physical accuracy.
The broader 1/2/4/8/split control, padding and adjoint evidence belongs to the
generic history tests, not additional KvN-specific executions.

That smoke declared 513696 operations per row query, a 16 MiB native environment
cap, a 512 MiB application rank cap and a 1 GiB node cap; its conservative managed
rank envelope was 285424673 bytes. The 16384-record no-spill input bound admitted
13122 generated contributions; smaller source/work allowances rejected. The
complete physical polynomial, grid, repeated drift queries, coefficient loading,
offline synthesis and exponential native state remain charged. Addressing a
very large tensor index with bounded row scratch does not admit its full
preprocessing or simulation. The [KvN refinement chapter](kvn-refinement.md)
separately records classical resolution/concentration diagnostics, and the
[sparse capacity receipt](../../../docs/verification/2026-10-05-sparse-capacity.md)
separately measures a capped local source/persistence/native pipeline.


## Scheduled generated box physics

The [generated complete box KvN history source](../../../docs/verification/2026-10-05-generated-box-kvn-history.md)
connects the full distributed constraint/force owner to the direct matching inverse.
Private agreed global rows perform collective physical drift and owner-local file
spooling; independently consumed readers contain no MPI callbacks. Every physical
coordinate remains present. The receipt validates an active nonlinear operator and
inverse circuit, while its short-horizon nonlinear physical response remains below
the measured solve error. It supplies no configuration-boundary or multihost claim.
