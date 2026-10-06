# Cumulative routing telemetry for prepared composition

Prepared MPI matching LCU and QSVT transforms now expose cumulative routing
receipts for each completed application. Independent review and focused tests
passed on 2026-10-06. Native QuEST and the matching routing algorithm are unchanged.

The [method and API](../research/prepared-routing-telemetry.md) define the measured
scope. The [seven-file source manifest](data/2026-10-06-prepared-routing-telemetry/source.json)
has SHA-256 `f36e4176ac8a8dda83bbf632ad78e05cfa9bae4a274a3494bd211fc8567b4638`
and content aggregate `814ecdea1f18c5b4e11bb68466cd1152fedc6a0f912b5f45c39d729b8fc44fe4`.
All seven files stayed unchanged through independent verification. This scoped
identity excludes dependencies and does not attest a complete executable build.
The [focused receipt](data/2026-10-06-prepared-routing-telemetry/focused.json),
SHA-256 `b422929a7e6383d635f551d0ab423653c48350173fda367416699b063153f4be`,
retains independent and owner test scopes and private log hashes.

## What the receipt establishes

An application clears the previous receipt before preflight. Only complete
execution installs a new receipt. A recoverable admission failure leaves no
current receipt and preserves the register; errors after emission retain the
existing fatal MPI boundary. Standalone admission does not reset a historical
receipt because it does not execute an application.

Each selected child contributes its router counters immediately after success.
Each transform query contributes its complete source receipt. Seven fields are
summed and two peak fields use maxima. A limit value or overflow marks the
result as inexact lower bounds. Retained zero-weight owners contribute no child
events. A degree-zero transform reports exact zero matching queries even though
its other primitives execute.

The rollup allocates and communicates nothing. Owner reservations include the
receipt storage, and preflight charges 128 modeled arithmetic units per child
or source event. These are conservative admission units, not measured processor
instructions. Tight budgets can consequently reject more conservatively.

## Executed checks

Independent verification passed 12 core and collective-failure tests in 18.28
seconds and the new runtime test parent in 2.93 seconds. That parent executes
five MPI jobs: 1/2/4/8 ranks and a four-rank world split into two independent
two-rank groups. It compares totals with separately dispatched native children
and repeated source applications, including adjoints, signed controls, omitted
zero weights, a live-budget rejection and degree-zero queries. Strict scoped
Clippy and formatting also passed.

The owner separately passed four existing CPU/MPI whole-unitary test parents.
Those bounded arbitrary-state references cover forward and standalone adjoint
action, failure flags, padding and controls. They remain separate from the new
counter fixture, whose states start from a basis vector. Historical API,
resource-arithmetic and at-limit failures are preserved as failed evidence.

With matching native QuEST and MPI configured, run from the workspace root:

```sh
cargo test -p quest-rs --features qsvt,mpi --test routing_telemetry
cargo test -p quest-rs --features qsvt,mpi --lib qsvt::matching_lcu -- --test-threads=1
cargo clippy -p quest-rs --features qsvt-io,mpi --lib --tests --no-deps -- -D warnings
```

The measured scope is local matching application payload, including its count
frames. PREP, response/projector, native internal, constructor, separate
coordinator and MPI protocol traffic are excluded; they are not measured zero.
Global reductions, process memory and aggregate node peaks need their own
accounting. This focused stage follows checkpoint 07. It does not close broad
workspace, integrated persisted inverse, actual large-count or multi-host
acceptance, nor any scientific accuracy requirement.
