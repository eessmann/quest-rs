# Generated complete physical force and collective drift

`BoxForceRecipe` generates the complete broken BDM1/BDM2 convection and SIP
momentum force on an implicit periodic or cavity box. It borrows a
`BoxConstraintRecipe`; geometry, neighbours and normals are computed from cell
indices. It retains no complete mesh, facet catalogue, global matrix, chart basis
or physical state. Fixed numerical quadrature tables cover the two triangle or
six tetrahedron permutations. Construction prepares the shared MathCore basis
once, evaluates its compiled kernels, and discards the polynomial objects.
Force evaluation thereafter uses only numerical tables.

Each query returns every component-major nodal force coefficient of one cell in
a fixed 30-word array. It fetches the owner and at most `d+1` neighbours into five
fixed blocks. The callback must observe one immutable physical state, including
repeated cell IDs; extra work or storage performed by an arbitrary callback is
the caller's responsibility. Invalid cell indices reject before any callback.
Unused output entries are zero; nonfinite supplied words and force overflow
reject rather than publishing a partial result.

Volume force is conservative convection minus viscous stiffness. Interior face
convection uses the average normal velocity and average component trace. This is
an explicit extension to arbitrary broken states, not an assumption that they
already satisfy continuity. On the complete normal/divergence chart it agrees
with the bounded dense reference's unique-left normal trace. SIP includes both
normal-gradient terms and penalty
`10*(p+1)²*face_measure/(d*cell_volume)`. Boundary averaging uses one rather than
one-half, with prescribed tangential velocity. The cavity lid is x-directed at
`y=L` in both 2D and 3D, matching the existing reference; all normal chart
constraints remain homogeneous.

## Collectively scheduled consumer

With the `distributed` feature, `PreparedBoxConstraints::prepare_force` binds
the force source to the exact borrowed geometry owner and retains its source-byte
reservation. Rank-local pointer ownership is checked locally; semantic geometry,
viscosity, lid and limit metadata agree collectively. The immutable returned
`PreparedBoxForce` exposes `drift(local_null)`.

Every rank must enter each drift in the same order with one agreed physical
state. It lifts only rank-owned complete cells, routes bounded neighbour blocks,
computes every owned cell force, drops the lifted velocity, then projects force
back to null coordinates. The result owns local force and null-coordinate slices,
their global ranges, resource metadata and live reservations. Full physical
coordinates are retained; the chart introduces no mode truncation.

Routing visits `ceil(cells/P)` owned-cell rounds, including idle rounds on empty
ranks. For each of `d+1` face slots, XOR peer rounds `1..P-1` exchange an 8-byte
requested-cell frame followed by a 248-byte echoed-cell/30-coefficient reply.
Idle requests still exchange fixed frames. A neighbour already owned locally is
read directly. No global cell scan or retained halo directory is needed.
Explicit power-of-two admission protects XOR peer indices. Transport uses the
existing MPI lane's private transport context, separate from metadata coordination
and native QuEST. All ranks finish the same protocol even after a malformed
request/reply or a cell arithmetic failure, then agree on rejection. Input
coordinates are never mutated; native transport failures retain native MPI fatal
semantics.

This API is a **standalone collective physical drift**. The new
[synchronized full-coordinate box KvN history source](2026-10-05-generated-box-kvn-history.md)
uses it only inside agreed global construction rounds, then hands MPI-free local
readers to the matching producer. Arbitrary independently advancing KvN generator
callbacks must still never invoke it: their scheduling does not guarantee common
collective order.

## Resources and admission

Pure construction checks source bytes, construction peak bytes, construction
work, full-cell query work and fixed scratch before basis preparation. Default
fixed construction work is 100 million elementary operations; construction
scratch is conservatively 8 MiB additional to the retained tables. One complete
cell query charges one million operations and 8 KiB scratch. Tables use actual
allocated capacities. Storage depends on dimension/order/permutations, not the
number of grid cells. A last-cell zero-state query on a logical six-billion-cell
grid is a bounded source/index regression only, not an executed grid or capacity
benchmark.

Collective preparation separately admits construction plus conservative control
work and transport and reserves the retained source/owner. Per-drift resources
charge both complete native chart queries, complete owned cell force work,
input/output selection and packing, all XOR/idle traffic and control operations.
Actual output-vector capacity is re-admitted before the first native query.
Source, output, temporary input/halo scratch and native query reservations overlap
in rank/node peak admission. The lift is released before the projection, while
both total query costs remain charged. Accessible input payload is included;
unrelated caller backing capacity and allocations before binding the prepared
force owner remain the caller's accounting obligation.

For `R=ceil(cells/P)`, `L=local_velocity_per_cell` and
`H=R*(d+1)*(P-1)`, the conservative per-drift model is:

```text
control work/transport = 16384*P²
source work = R*1,000,000 + 4096*H + 1024*R*L + control
native work = 2*chart.resources().query_work
halo directed payload = 256*P*H
total directed transport = 2*native_query_transport + halo + control
temporary bytes = accessible null input bytes + 8 KiB cell scratch + 8 KiB routing/control
output bytes = actual force capacity bytes + output-owner size
rank peak = current environment allocations + output + temporary + native query peak
node peak = maximum rank peak * admitted ranks_per_node
```

`max_prepare_work` and `max_work` admit maximum-rank arithmetic ceilings. Source
work uses the largest cell shard; the existing native query work already uses a
conservative global-size ceiling. Their sum safely bounds each rank, but is not
a rank-summed job total. Replicated constructor work is charged per rank. A caller
requiring an aggregate job-work admission must separately charge, conservatively,
`P` times the maximum-rank work ceiling. Transport fields are already global
directed-payload ceilings and should not be multiplied again for that purpose.

These are logical arithmetic/storage and directed-payload ceilings; allocator
metadata and MPI internals are excluded. The existing complete chart retains
distributed dense QR fill and global constraint metadata and charges its own
native query costs. This force consumer does not change those limitations.

## Local verification

Using an installed MPI-enabled QuEST and matching MPICH, from the repository root:

```sh
export QUEST_ROOT=/path/to/installed/quest
export MPICC=/path/to/mpich/bin/mpicc
export PATH=/path/to/mpich/bin:$PATH
cargo test -p quest-cfd --test force_recipe
cargo test -p quest-cfd --features distributed --test force_recipe --test force_collective -- --nocapture --test-threads=1
cargo clippy -p quest-cfd --features distributed --lib --test force_recipe --test force_collective --no-deps -- -D warnings
```

Three pure tests pass. They compare complete cell force and its chart projection
with independent dense assembly for both physical dimensions/orders and both
boundary types; isolate arbitrary-broken-state viscosity and boundary load; and
check fixed source storage, invalid indices, budgets and nonfinite inputs. The
bounded dense `PhysicalSpace::momentum_force` accessor validates the full broken
input and preserves its existing unique-left convective extension.

The native regression launches 1, 2, 4 and 8 ranks and a split 4-rank world into
two 2-rank groups. All lanes passed locally in 9.90 seconds. Cases include both
dimensions/orders/boundaries, a subdivided triangle mesh and empty cell owners.
Reference meshes, matrices and complete states appear only in these bounded
independent tests. Production methods use owned shards. Tests compare full force,
lifted drift, mass-orthonormal energy, energy rate and gauge-fixed pressure;
repeat outputs exactly; and verify reservations release after normal results,
malformed input, overflow, work/transport/rank/node rejection, wrong ownership,
and rank-dependent physical parameters or limits. Strict scoped Clippy passed.
This is local correctness evidence, not multi-host or campaign acceptance.
