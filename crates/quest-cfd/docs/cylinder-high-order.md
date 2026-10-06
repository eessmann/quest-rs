# Complete P2 cylinder snapshots

`CylinderPhysicalSource` constructs a bounded, nonperiodic BDM2/P1 model of the
planar DFG2D2 channel with a polygonal cylinder. It uses the shared affine geometry,
physical operator, complete mass chart, canonical lifting and original pressure
implementations. `prepare()` retains every homogeneous coordinate and adds the
minimum-mass stationary lifting of the full quadratic inlet trace. The natural
outlet prescribes `nu grad(u)n - p n = 0`; it does not constrain the outlet normal
velocity or subtract a pressure mean.

This is a supplied-state snapshot API. It has no time integrator, CLI case dispatch,
curved elements, three-dimensional cylinder, or developed-cycle acceptance claim.
The official DFG measurement cycle is in `[25,30]`; the inherited project manifest
window `[0,8]` is only an allowed snapshot-time domain. Historical project `[4,8]`
diagnostics are not official cycle acceptance. This comparison does not implement
the separately unresolved literal three-dimensional wake conditions.

Run the fixed example with the workspace's normal native dependency configuration:

```sh
cargo run -p quest-cfd --example cylinder_p2_snapshot
```

It declares a **2 billion aggregate work allowance**, preserving every default
component cap and the 256 MiB managed byte ceiling. It emits one completed or
rejected JSON record through a fixed 64 KiB output buffer. The public workflow
**default remains 1 billion** and rejects preparation of this fixture; the example
does not retry or raise component limits after rejection. Its initial homogeneous
coordinate vector is zero, so the snapshot is the complete inlet lifting, not a
computed transient solution.

## Versioned represented geometry

`explicit-rectangle-corner-priority-v1` is separate from the legacy cylinder source.
The legacy generator and its public entry points retain their floating-point
coordinates, connectivity and boundary convention. Regression hashes cover its
geometry and labels for the 4-, 8- and 16-sector inputs.

The new source uses exact cardinal and octant direction vectors where applicable,
inserts the four represented rectangle corners, and orders all rays by total
floating-point angle order, corner priority and original ordinal. Rays separated
by at most `1e-12` radians are coalesced with corner priority. This is an explicitly
reported **source-construction approximation**, not an exact geometry predicate.
Multiple corners in one cluster, ambiguous untagged equal side hits and a cluster
crossing the angle seam reject. Receipts retain the removed-ray count and maximum
removed angular separation.

Corner points are copied directly. Every other outer point receives its hit-side
coordinate directly from the represented rectangle bounds; only its free
coordinate is computed. Exterior labels require exact equality to a common
rectangle side. Inner circle points remain floating-point polygon vertices. All
resulting cells still pass the unchanged exact dyadic conformity, topology and
outward numerical-quality admission of the common affine owner.

The fixed 4-sector, one-layer source has eight retained rays, no coalescing,
16 vertices, 16 triangles, 14 Dirichlet edges and two natural edges. The complete
P2 broken dimension is 192, with 90 normal-trace and 48 divergence constraints;
the complete chart has 54 coordinates. These are measured construction results,
not hardcoded returned dimensions. The exported geometry contains the actual
binary64 vertices, connectivity, labels and conditions. Its fingerprint covers
that framed geometry and source-policy string; it is **not an operator identity**
and excludes viscosity, polynomial order and time data.

The independent rational proof of this exact exported source, its scope and
reproducer are in the [verification chapter](../../../docs/verification/2026-10-06-cylinder-p2.md).
That proof certifies the rational constraint rank for the represented geometry,
not the rank of a rounded runtime matrix. The coarse polygon's circle deviation
is reported separately; no spatial or polygonal-geometry convergence is inferred.

## Original physical observations

`PreparedCylinderPhysical::snapshot_at` consumes a complete supplied state and a
finite admitted time. It reconstructs the original pressure including the full
lifting, original acceleration and natural pressure level. Its continuity and
momentum residuals are numerical residuals, not exact certificates. The pressure
report explicitly uses the prescribed-traction gauge and has no mean-normalization
residual.

The snapshot evaluates full lifted kinetic energy divided by actual polygonal
fluid volume, integrated enstrophy, front-minus-back incident pressure trace, and
force on the exact `cylinder` boundary label. Mechanical force uses
`p n - nu grad(u)n`, the opposite sign to the prescribed natural load. The force
coefficient normalization is `2/(U_mean^2 D)`. The shared box-side traction API
keeps its existing geometry restriction and signs; arbitrary labels use the new
explicit label selector rather than pretending the cylinder is a box side.

Independent tests integrate P1/P2 fields `(x,-y)` and `(-y,x)` on the stretched
triangle: kinetic energy `5/12`, enstrophy respectively `0` and `2`, and analytic
constant-pressure/viscous edge forces. A separate P2 nodal Simpson calculation
checks the exact quadratic inlet data and free outlet flux, both `0.41`. These
manufactured observation tests do not assert that every prescribed field/pressure
pair solves the momentum equation.

## Admission and ownership

The workflow ledger is cumulative across construction, lifting, preparation and
repeated snapshot queries. `cumulative_work` records charged completed/admitted
operations; `attempted_stage_work` preserves the latest planned envelope on a
rejection. A failed component that does not return its own detailed attempt record
retains the available component work and byte envelope. Such an envelope is not a
measurement of operations executed. A pressure or observation numerical failure
after admission still consumes its conservative query charge.

For a requested sector bound `S = angular + 4`, radial layers `L`, upper vertex
count `V = S(L+1)` and upper triangle count `C = 2SL`, source construction charges
`1_000_000 + 10_000(V+C+4S)` work before allocation. Inputs require 4–64 requested
sectors, 1–8 layers, Re 100 and `12C <= 768`; common affine caps may reject earlier.
This work model covers bounded ray ordering, elementary geometry, transcendental
calls, source hashing, manifest validation and adapter creation; it is not a CPU
instruction or wall-clock bound.

A conservative 128 KiB reserve covers generated geometry/manifest ownership,
source/wrapper metadata, generator scratch, borrowed-view adapters, full trace
setup, layout copies and the retained initial vector. Returned Vec/String
capacities are checked against that reserve before the next numerical stage.
Accessible affine view bytes are also counted by the common constructor; this
intentional overlap is conservative. Caller-declared `mesh.external_retained_bytes`
is added before source generation and remains live in the physical owner and all
subsequent component admissions. The example declares its 64 KiB serializer buffer
there. User-owned unused argument capacity remains the caller's responsibility.

Preparation charges `10_000 N` for trace/setup work, then the complete shared
canonical-lifting and polynomial-boundary constructor bounds. Neither component
gets a fresh aggregate budget. Snapshot pressure charges the complete exposed
`PolynomialBoundary::pressure_query_resources` bound, including original force,
lifting derivative and pressure QR. Observations additionally charge
`8 * drift_work + 1024 * C`: this bounds repeated complete-field reconstruction,
energy/enstrophy contractions, label scans, traction quadrature, two pressure
probes and scalar diagnostics. The label-traction and pressure-probe APIs retain
their own unchanged default caps. The observation live bound includes the time
owner, complete pressure capacities, accessible supplied state and reserve.
Repeated snapshots consume the same ledger and can reject even if an isolated
query would fit.

These are managed payload and arithmetic models inherited from the shared
reference implementation, not allocator metadata, process-memory limits, gate
counts or scalable distributed costs. In particular, the exact-geometry arithmetic
profile's environment lookup remains outside its payload model; unsupported host
configuration can allocate before rejection. Accessing the exposed physical model
or serializing the borrowed geometry outside this example is a separate caller
operation and is not retroactively charged to the workflow ledger.

The additive [bounded P2 evolution consumer](cylinder-p2-evolution.md) advances the
same prepared source over a separately declared very short interval. It does not
change this supplied-state API or the historical Stage C evidence.
