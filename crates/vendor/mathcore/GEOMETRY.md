# Bounded exact affine simplex predicates

`geometry::DyadicScale::admit` scans binary64 coordinates as their exact dyadic
values. It admits dimensions two and three, requires zero unused z components
for two dimensions, and clears one common binary scale. Signed zeros coincide.
The default scaled integer magnitude limit is 64 bits; the hard limit is 128.
Large exponent spreads reject before big-integer allocation. Coordinates are
never rounded, snapped or projected onto a nearby mesh.

The scale owns only constant-size metadata. It does not bind a particular
coordinate source or retain a geometry catalogue. Every query validates its
points against the admitted exponent and actual coordinate width. Callers must
account for accessible input storage and retained owner capacities separately.
`source_work()` charges complete source scanning once; `pair_work()` and
`pair_peak_bytes()` charge every operation, including repeated calls, individual
cell checks, displacement comparisons and AABB shortcuts. Aggregate mesh work,
source/assembly overlap and topology buffers belong to the mesh owner.

`validate_simplex` checks shape, repeated IDs/coincident vertices and exact
nondegeneracy. `validate_pair` additionally checks common-ID coordinates,
coincident points with distinct IDs, duplicate complete cells and geometric
conformity. Both simplices are validated before a separation shortcut. A legal
intersection is exactly the convex hull of shared physical vertex IDs.
`matches_displacement` compares two exact endpoint differences. It does not
round an anchor displacement back to binary64; represented 1.1 minus represented
0.1 need not equal represented 1.0.

The division-free pair algorithm tests original vertices plus edge/facet
crossings: 15 distinct candidate intersections for triangles and 56 for
tetrahedra. The symmetric triangle implementation evaluates both edge/edge
directions (six vertices plus 18 trials, at most 24 evaluations), duplicating
the nine edge intersections. Tetrahedra use eight vertices plus 48 directed
edge/facet trials, at most 56 evaluations. Duplicates do not affect exact tests. Oriented
determinants provide homogeneous barycentric numerators. For an edge u,v
crossing facet k, use D=b_k(u)-b_k(v) and
q_j=b_k(u)b_j(v)-b_k(v)b_j(u). Feasibility uses the signs of D and q_j. Membership
in the shared hull uses exact zero weights at nonshared edge endpoints. No
candidate coordinates or intersection catalogue is constructed.

Completeness includes lower-dimensional contacts. A bounded intersection is the
convex hull of its vertices; at each vertex the active plane normals span the
ambient dimension, selecting an independent d-plane subset. Those subsets are
exactly original vertices or edge/facet crossings. A singular/coplanar subset can
be skipped because an independent subset still selects every extreme point.
Shared vertices already lie in both simplices, so requiring every feasible
candidate in their hull proves equality in both directions.

For scaled coordinate magnitude K, differences have at most K+1 bits. Define
L=d(K+1)+ceil(log2(d!)); determinants have at most L bits, and the largest
crossing expression has at most 2L+2 bits. At d=3,K=128 this is 782 bits. The
coefficient cap is independently admitted, with a 4096-bit hard maximum.

The implementation evaluates 36 determinants: two standalone cell checks, two
orientation-sign checks, and 32 replacement determinants. The 48 edge/facet trials
use at most 288 additional products, giving a 720-product upper bound, still
within the admitted 1024 multiplication slots. Additions/subtractions/shifts
also remain below 1024 slots.

The work model admits 1024 integer multiplies, 1024 add/subtract/shift slots and
2048 sign/control slots per query. With n=ceil(L/32), o=ceil((2L+2)/32), the
modeled allowance is

`1024*16*(n+1)^2 + 1024*8*(o+1) + 2048*16`.

This gives 1,196,032 at d=3,K=64 and 3,457,024 at d=3,K=128. It is a conservative
word-operation model, not measured execution time. Source scanning has its own
checked allowance `128*N*(d+1)+4096` and precedes construction.

Each operation admits a 64KiB arithmetic payload envelope. Peak production
liveness is below 128 integer objects: 24 original coordinates, 32 cross
numerators, 12 replacement coordinates, nine determinant differences, and staged
expression products/sums. At the supported widths, compact limb buffers are
under 160 bytes each, leaving generous room for objects and fixed metadata. No
recursive multiplication scratch is used under the admitted profile. This is a
conservative payload envelope, not measured allocator capacity or process RSS;
allocator/OS overhead and real process caps remain separate.

The profile uses pinned dashu-int 0.6.2 with 32/64-bit `Word`. At each operation,
`DASHU_THRESHOLD_SIMPLE_MUL` must be absent or parse to at least 24 and
`DASHU_THRESHOLD_SIMPLE_SQR` absent or parse to at least 30. Present values must
be at most 32 ASCII bytes; oversized, non-ASCII and malformed values reject
before numeric parsing. Equal multiply
operands can take the square path. Pinned threshold functions read environment
values on each call without caching; configuration must stay immutable during
the operation. These gates apply regardless of downstream tuning-feature
unification and conservatively reject low overrides even if a build ignores
them. Multiplied operands fit at most 13 32-bit words, ensuring simple multiply
or square with zero recursive scratch. Unsupported word profiles reject.

Environment discovery is a real resource-contract limitation. Safe standard
library retrieval clones an override before its length is available. Dashu's
tuning retrieval also allocates and parses process configuration. This platform
lookup/parse overhead is outside the arithmetic payload and modeled work,
including for accepted bounded values. Malformed or oversized host configuration
can allocate/scan proportionally before rejection: geometry `max_bytes` and
`max_work` are not hard bounds on that unsupported configuration or on all
temporary/process memory. Cheap shape, declared budget and source-scan admission
checks precede discovery where possible. Supported immutable values have at
most 32 bytes per override, but this is not full process-capacity acceptance.
Actual OS/process caps remain separate; no allocation-free environment lookup
or new bigint engine is claimed.

This is a geometric conformity predicate. It does not establish a manifold
boundary, connected domain, periodic quotient, intended domain coverage,
floating Jacobian conditioning, stable quadrature or physical discretization.
Those checks must precede numerical assembly under separately admitted limits.
An exact nonzero determinant is insufficient for numerical conditioning.

The maintained tests compare against independent homogeneous Cramer enumeration
of 15/56 active-plane subsets, including legal contacts, hanging intersections,
one-bit gaps/overlaps, width/budget rejection and subprocess arithmetic profiles.
The shared decoder refactor preserves the exact rational importer's original
raw-exponent width admission, including signed-zero rational semantics.
