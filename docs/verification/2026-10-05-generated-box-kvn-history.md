# Generated complete box drift into distributed KvN histories

The generated complete physical force now feeds a safely scheduled full-coordinate
KvN history and the existing direct QSVT inverse. This closes the source scheduling
and ownership connection. The local tests validate an active nonlinear operator
and its inverse circuit; they **do not resolve a nonlinear physical response**,
establish configuration-boundary convergence, or demonstrate multihost capacity.

`prepare_box_kvn_history_inverse` in `quest_cfd::box_kvn_history` accepts an exact
`PreparedBoxForce` owner, its environment, a complete `ConfigurationGrid`, the
horizon/time cells/DG order, an independently admitted scalar initial recipe,
independent spectral bounds, and separate source/history policies. The generated
force is autonomous with fixed viscosity and lid load. Every mass-orthonormal
physical coordinate is retained. Its prepared MathCore kernels run numerically;
there is no symbolic work or nonlinear classical substitution in inverse replay.

## Scheduling and ownership

Only the private construction adapter calls collective physics. All ranks traverse
all global temporal rows in the same order. For each generator row it decodes
only its actual QR-tail coordinate range into an owned point shard, evaluates one
left drift, retains only its owned left acceleration shard, and drops that query
owner before evaluating right drifts. Scalar acceleration components are
broadcast from their unique coordinate owner. The implementation independently
checks coverage against actual native chart ranges, including empty owners; it
does not assume a balanced partition of the null dimension.

The numerical coefficient is the existing combined expression
`-.5 * (F_left[j] + F_right[j]) * D_ab * sqrt(w_a/w_b)`. The existing temporal
recipe supplies its DG1/DG2 signs, masses, causal interface terms and input
ordinals. There is no replicated physical mesh, global point/drift catalogue,
complete history matrix or full RHS in this production source.

Only the assigned history-row owner writes each row to a bounded `FileRunStore`
spool. Ranks agree allocation before entering a physical row, finish that row's
collectives even after a local write failure, and agree errors before the next
row. File creation, finish and open also agree. The immutable owner-local reader
checks shape, strictly increasing ordinals, finite coefficients, record count and
SHA-256 digest. It performs **no MPI**, so independently advancing producer
readers are safe. The common backend alone transposes/conjugates H to H adjoint
for the matching inverse. Files and live reservations release on success or error.
Zero RHS skips the factory; input/grid validation remains charged.

This is classical globally scheduled preparation, not a free coherent query
oracle. The compiled inverse and coherent RHS are reusable after physical source
and spool construction finish. Arbitrary independently advanced KvN callbacks
must still never call the collective physical drift.

## Admission and identities

For history dimension N and maximum generator row length K, the worst drift count
is `N*(K+1)`. Every call includes both native chart queries, complete physical
force work, halo/control traffic and source index/scalar work. Admission exposes
both a maximum-rank arithmetic ceiling and the conservative aggregate ceiling
`P*maximum_rank_work`; transport ceilings are global directed payload. Preparation
validation/hash/ownership work has its own pre-scan ceiling, including zero RHS.
Already completed chart/force preparation is a separate stage and remains the
caller's responsibility.

Rank/node memory checks include existing environment owners, actual point/left
Vec capacities, the borrowed axis recipe and scratch path, row-buffer capacity,
actual file-owner path capacity, fixed control scratch and native query overlap.
Every rank-dependent checked sum agrees before a following broadcast. Independent
local/global/node disk ceilings count worst raw 40-byte records, including zeros
and duplicate contributions. These are managed application-byte/work models;
MPI/kernel/filesystem metadata and actual process RSS are not measured here. The
fixture uses a 64 MiB native budget and a 512 MiB history/source rank ceiling; its
memory node ceiling is the unrestrictive `usize::MAX`, with node placement
conservatively equal to the communicator size. Disk node/global ceilings are
1 GiB. No OS memory cap or node-capacity claim follows from these tests.

`recipe_metadata_sha256` hashes geometry/force/grid/temporal parameters; it does
**not** bind the actual floating QR basis, rank policy or a represented matrix.
The per-owner spool digest binds its raw H records. The produced matching header,
available as `prepared_inverse.report().source_identity`, identifies actual
canonical H-adjoint coefficients; it is a 64-bit numerical source identity,
not a cryptographic security certificate. A finite tensor grid is not invariant
under chart rotation. Each rank-count fixture therefore uses its own same-chart
independent reference, rather than asserting raw histories equal across different
QR bases.

## Local correctness and numerical evidence

The bounded reference alone gathers native Q, aligns full physical states with
independent dense `PhysicalSpace`, transforms acceleration back to that native
chart and assembles the complete weighted generator and history. Production
construction performs no such gather. Whole H/RHS equality passes on 1/2/4 ranks
and two split 2-rank communicators. The one-cell configuration DG1/axis-two
32-node case has exactly cancelling derivative duplicates: it tests protocol,
DG1/DG2 time signs, cleanup and empty owners, not nonlinear quantum dynamics.
The meaningful configuration DG2 case has all five physical coordinates,
243 configuration nodes and 486 DG1 history coefficients; whole-matrix errors
are below `1.7e-16` on 1/2 ranks.

The active-operator inverse uses a shifted compact bump, center
`[.04,-.03,.02,.05,-.01]`, width `.55`, coordinate interval `[-.3,.3]`, periodic
BDM1/P0 on the full two-triangle box of extent 1.7, viscosity zero and horizon
`.001`. Its support reaches the configuration boundary; this is an operator
accuracy fixture, not a boundary-resolved flow benchmark. Quadratic convection is
isolated independently as `(F(a)+F(-a)-2F(0))/2` before assembling its weighted
KvN action on the actual initial amplitude.

| Observation | MPI 1 | MPI 2 |
|---|---:|---:|
| `||G_NL z0||` | `1.1662697e-5` | `2.3439877e-5` |
| Relative nonlinear action | `.01070363` | `.02151233` |
| Actual / maximum drift calls | `11016 / 12636` | `11016 / 12636` |
| QSVT normalization alpha | `8` | `8` |
| Declared spectral interval | `[.5, 1.001571623]` | `[.5, 1.001232826]` |
| Degree / native qubits | `129 / 15` | `129 / 15` |
| Reciprocal scale | `.0118345267083` | `.0118345267083` |
| Recovered `||Hx-b||/||b||` | `.00115813804` | `.00115814200` |
| Relative full solution error | `.00115813802` | `.00115814193` |
| Inverse success probability | `.017885670685` | `.017885670690` |

Spectral evidence is the bounded reference's analytic temporal slab coercivity
and outward sparse Hermitian-part bound. Approximation tolerance `.03`, maximum
degree 2047 and the physical residual target `.01` were set before measurement.
Both runs pass that inverse-plumbing target and preserve a reusable whole-unitary
forward/adjoint identity on nonzero failure/control/padding sectors using bounded
64-amplitude chunks. No unconditional floating-execution error certificate is
asserted. The indicative nonlinear change over this short horizon is roughly
`1e-5`–`2e-5`, smaller than the measured solve error about `1.16e-3`.
**The physical nonlinear response is therefore not resolved by this receipt.**

The first 4,096-entry memory-only producer buffer rejected the nonzero source;
the successful bounded attempt explicitly admits 16,384 entries. Center-only
bump runs are stationary-RHS schedule checks only. The final active-operator
campaign completed in 95.46 seconds locally, with 180-second limits per child;
these debug timings include collective waiting and are not performance claims.
The [machine receipt](data/2026-10-05-generated-box-kvn-history/receipt.json)
separates the historical science binary from the later admission-only correction
and current prepared-source identities.

## Reproduction

```sh
export QUEST_ROOT=/path/to/installed/quest
export MPICC=/path/to/mpich/bin/mpicc
export PATH=/path/to/mpich/bin:$PATH
cargo test -p quest-cfd --features distributed --lib box_kvn_history::tests -- --nocapture --test-threads=1
cargo test -p quest-cfd --features distributed --lib box_kvn_history::tests::nonlinear_generated_history_inverse -- --ignored --nocapture --test-threads=1
cargo test -p quest-qsvt-io --test sparse_stream
cargo clippy -p quest-cfd --features distributed --lib --tests --no-deps -- -D warnings
```

The expensive active-operator campaign is explicitly ignored by ordinary test
runs and was run separately here. Focused MPI tests also reject source work,
aggregate work, transport/disk/metadata/shape policies, malformed time data,
one-owner create/write errors and digest corruption; all associated files and
reservations release. A metadata-only two-rank near-`usize::MAX` reservation
reproduced a pre-broadcast overflow hang before review and now rejects together
without allocating a huge state. The independent exact probe also passes.
