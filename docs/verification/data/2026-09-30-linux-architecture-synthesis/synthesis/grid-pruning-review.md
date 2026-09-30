# Independent grid-pruning review

Reviewed on 2026-09-30 by the QSP implementation agent, independently of the synthesis owner. This is a source/proof review; command acceptance is recorded separately by the synthesis owner. No competing source edits or test processes were used for this review.

**Verdict:** no unresolved correctness findings at the source hashes below. The retained-value admission finding and direction-coverage recommendation were resolved. The subsequently authorized live-grid reservation was also reviewed.

## Necessary pruning conditions

Write the unscaled cyclotomic coefficients as c=(a,b,c,d), with denominator factor F=2^e. The physical and bullet embeddings have squared norms whose sum is 2 times the coefficient squared norm. Requiring each embedding to lie in its radius-F disk therefore implies sum(c_i^2) <= F^2. The second exact Gram–Schmidt decomposition applies this necessary ball in original coefficient space after the unimodular transform. Its intersection with the original weighted sphere cannot remove a feasible point.

For scale S, the target midpoint differs from its unit-norm target by at most target_width/S in L1 norm. Replacing sqrt(1/2) by the root enclosure midpoint changes the physical point by at most 2*root_width/S after division by F: |b-d|+|b+d|=2*max(|b|,|d|), and the coefficient ball bounds each magnitude by F. Thus the product (1+target_width/S)*(1+2*root_width/S) is a conservative upper bound on their radial dot product. The implementation converts this bound to the weighted radial coordinate and subtracts the sphere center.

At a partial enumeration node, the retained high-basis Gram–Schmidt error has a fixed radial component. Cauchy–Schwarz bounds the magnitude of the remaining low-basis correction by sqrt(remaining_weighted * sum(low_vector_radial_coordinate^2 / low_norm)). Pruning occurs only for positive radial excess whose square strictly exceeds this reach. Equality is preserved. Omitting a final radial test at the leaf can retain extra candidates but cannot remove a valid one; later exact feasibility and independent full-phase certification still apply.

All these calculations are exact integer/rational calculations. The target norm assumption is supplied by the private Grid caller's rotation enclosure, not by an arbitrary external grid target API.

## Admission and restoration

The initial review found inline coefficient-radius and scaled-radial-headroom rationals retained without explicit admission. The final code admits those values, factor, radius, intermediate radial excess/reach, integer search windows, coefficient costs, and recursive retained values.

The fixed grid now records its existing conservative scratch reservation. Budget::with_reserved_bytes subtracts it while the full search and candidate callback execute. It captures the callback Result before restoring the byte allowance, so both success and every ordinary error path restore the original value. Failed checked subtraction occurs before assignment and leaves the field unchanged. Work usage remains cumulative. A successful Approximation restores the original request limits; exact candidate, target, tolerance and certificate precision are unchanged. ApproxCertificate contains no saved byte-limit field to become stale.

This is a conservative modeled reservation and category admission, not a proof of total allocator or process RSS usage. It does not add a universal heap-accounting claim. Search still has explicit finite work/exponent limits and may exhaust them without asserting mathematical impossibility.

## Regression evidence inspected

- The exhaustive small-grid oracle independently enumerates the original coefficient box, filters both exact embedding disks and the original weighted sphere, and compares retained sets for e=0,1,2. Directions cover 0, pi, pi/2, pi/4, pi/7, -pi/4 and 5pi/4.
- The quarter-pi test at tolerance 1e-12 compares repeated same-seed results/work, independently certifies the full phase, rejects a scalar-phase corruption, and rejects a work allowance one below the observed usage.
- A tiny-byte regression provides exactly the conservative live-grid reservation and requires candidate allocation failure; normal success checks original request limits, target and tolerance identity.
- A Budget unit regression checks restoration after both success and cancellation error while preserving cumulative work.
- Inspected focused green logs and strict all-target Clippy completion. The owner's post-reservation full acceptance runner is the authoritative wider test receipt; this review does not independently claim its completion.

## Reviewed source SHA256

| Source | SHA256 |
| --- | --- |
| crates/quest-synthesis/src/grid.rs | bcfa29fe33bdd8532ffb71dbfc1fa561f2b932f6022d8ab55c49a7c8c49d1a80 |
| crates/quest-synthesis/src/approximation.rs | 80047143fccfc3117345c9ec3ef2a0d98737cea6264c80cba2b20ee489e2441e |
| crates/quest-synthesis/src/lib.rs | fa7d86af32606bf1391df531c8431df08339f3c9777938a279cb46be3067681d |
| crates/quest-synthesis/tests/approximation.rs | c10f320013457afcfc0de62c957cd0f160dd23ebee6078c048ff9ab28fdbf339 |
| crates/quest-synthesis/tests/resources.rs | 1ddabc7d76d4d5753a5f93c3f13f1b1efe9438bd988b08ec4c3ecbf9a2593339 |
