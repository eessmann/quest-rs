# Independent utility and proof review

Reviewed the current working-tree implementations of contractors.rs, norm.rs,
symmetric Laurent conversion, Pauli decomposition, the lattice-containment
argument against grid.rs and quest-math dyadic conversion, and offline verifier
retry termination. Did not review my own generic function/Remez implementation.
No heavy verification gates or implementation edits were performed.

## Actionable finding

### [P2] Vector contractor byte admission omits live Vec storage

`crates/quest-numerics/src/contractors.rs:307-315` admits only interval-cell
multiples. With dimension=1 and max_boxes=1 it admits max_bytes=128
(`(4*1*1 + 4*1*1)*sizeof(Interval)`). Nevertheless the implementation allocates
nested vectors and branch lists at lines 362-393 and 408-435. On this 64-bit
platform, before even counting callback allocations, its live wrapper-owned
payload includes c (16 bytes), a's row-vector header (24) and row (16), b (16),
the displacement (16), and the branches vector's Vec header element (24): 112
bytes. A child branch (16) and even a minimally sized next list (24) bring that
to 152; the temporary division-image buffer also coexists. Actual geometric
capacity growth can make this larger. A valid one-dimensional linear example
therefore exceeds an explicitly admitted 128-byte limit. This is a resource
contract failure, not a root-exclusion/uniqueness counterexample.

Model nested Vec headers and simultaneous old/new branch lists (including
capacity growth), or use bounded flat/fallibly reserved storage. Add a tight
one-dimensional byte-boundary regression, independent of the payload-only
formula. The callback allocation exclusion does not cover a, b, c, child or
next, all created by the wrapper.

## Mathematical conclusions

- Extended division preserves both branches when the denominator straddles
  zero. Exact-zero numerators retain the displacement box. Scalar uniqueness
  requires derivative exclusion of zero or an appropriate Krawczyk contraction;
  exact center roots are handled consistently under the documented callback
  premise. Vector uniqueness uses a separate strict Krawczyk inclusion and
  infinity-norm contraction; this also forces the square preconditioner to be
  nonsingular, so singular Gauss-Seidel contraction is not mistaken for root
  existence. No unsound root evidence found.
- Real-segment norms use outward complex rectangles and sample lower bounds.
  Disc upper bounds correctly apply maximum modulus only after pole exclusion;
  the slightly extended angle interval still parametrizes the same circle.
  Point-parameter rectangle lower bounds remain lower bounds for an actual
  circle point. Budget/resolution exits retain the cover and explicit status.
  Pole-location uncertainty is reported rather than turned into finite proof.
  No unsound norm enclosure found.
- Inversion symmetry uses equality without conjugation and the factor two is
  appropriate for `z^k+z^-k=2T_k`. Absent coefficients are treated as zeros.
  Overflow rejects, and multiplication by two does not lose subnormals.
  Source polynomial cloning is Arc-backed and effective support is cached;
  these do not create a padded-source scan/copy budget bypass.
- Pauli masks match the trace convention, including Y's sign, tensor ordering,
  and non-Hermitian matrices. The overflow retry is bounded and normalization
  is a power of two. As documented, coefficients are ordinary rounded binary64
  results, not exact trace certificates. Mixed huge/small cancellation can
  still lose a representable tiny trace; this is a numerical conditioning
  limitation rather than threshold chopping or a false interval certificate.
- The sphere argument matches the actual coordinate factors 8/epsilon^2,
  4/epsilon and 2. Feasible embeddings imply coefficient norm <=1; the ideal
  squared bound is 13. The dyadic sqrt-half construction preserves one-grid-unit
  width. Floor-midpoint error is bounded by that width, so the stated h bound
  remains valid without assuming rounding to nearest. Target/root perturbation
  gives dot/cross error <=3h+2h^2<=4h, hence the documented conservative 22<36
  squared-radius margin. The upstream caller enforces 0<epsilon<=1. No missing
  containment condition found in this derivation.
- Offline certification Budget/Policy termination is correctly terminal at
  the point where the attempt and last export have already been retained.
  Numerical certification failures still follow the explicit bounded precision
  retry policy. No retry-contract issue found in the changed branch.

Approval is withheld for the vector contractor byte-budget finding; no other
blocking mathematical issue was identified in this review scope.

## Resolution review: vector contractor storage

Reinspected the repaired admission and allocation paths without running a heavy
gate. The finding is resolved. Wrapper buffers now use fallible exact reservation;
old/new branch headers are both admitted, extended division always reserves its
two interval cells, and successful translation reuses the final branch buffers.
The conservative live-payload formula is

- interval cells: n^2 + 4n + 2*n*max_boxes + 2;
- nested Vec header elements: n + 2*max_boxes + 1.

This includes the two fallback input copies while both branch generations and
the division scratch remain live. A child allocated before insertion fits the
already admitted next-generation bound, since branch admission occurs before
that allocation. For n=1 and max_boxes=1 the formula is nine 16-byte interval
cells plus four 24-byte headers = 240 bytes. The new regression independently
checks rejection at 239 and admission at 240; source inspection confirms the
branch buffers no longer grow geometrically beyond those modeled capacities.
Callback allocations, fixed stack objects and allocator bookkeeping remain the
explicitly excluded categories.

Approval granted for this review scope after that repair; no remaining defect
was identified in the repaired storage model. Parent's running integrated gates
remain the execution evidence for the final tree.
