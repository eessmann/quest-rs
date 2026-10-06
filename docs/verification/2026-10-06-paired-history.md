# Common-ensemble paired history campaign

Across the first campaign and one separately approved accounting follow-up, 13 of the 14 distinct requested configurations have completed. The two-slab KvN configuration remains rejected by the unchanged source-work cap. All 17 actual process attempts are retained in two separate receipts.

The first frozen campaign attempted all 14 requests once: 11 completed, three rejected, no timeouts and no malformed child results. The completed rows are two independent physical RK4 ensembles and nine Carleman histories. All three KvN histories rejected before history assembly because the independent nonlinear-action diagnostic requested 1,063,872 sparse work units against its private 1,000,000-unit cap. No KvN/Carleman outcome comparison was produced in this attempt. `complete: true` in the receipt means every frozen request was recorded, not every calculation succeeded.

The [unaltered first receipt](data/2026-10-06-paired-history/attempt-1.json) retains the rejection messages, process outcomes, measured RSS, numerical resource receipts and 22 available comparisons. The [separate build attestation](data/2026-10-06-paired-history/attempt-1-build.json) binds binary SHA-256 `e6fa8c4fb2e54a0eae1ca1c72bfb2d71653339007e143d866079347497ecfaf0` to its unchanged build-source manifest. The execution snapshot was also unchanged during the campaign; it differed from the earlier build manifest only in the separately evolving `tests/configuration_flux.rs`. Those two source manifests use explicitly different scopes and are not interchangeable.

## Fixed physical and ensemble semantics

The source is the complete five-coordinate periodic two-triangle BDM1/P0 unit-square model, viscosity 0.01, with horizon 0.01. Every calculation uses the same 243 sampled initial probabilities on five configuration axes in [-0.2, 0.2], one DG2 cell per axis, compact-bump center [0.04, -0.03, 0.02, 0.05, -0.01] and width 0.55. The Carleman initial vector averages every retained monomial over those probabilities; it is not the lift of the ensemble mean. All completed initial ensemble identities equal `fnv1a64:40f644eebafcddf6` (noncryptographic provenance).

The retained hierarchy orders are 2, 3 and 4. Each has one DG1 slab, two DG1 slabs and one DG2 slab, with observations at 0.0025 and 0.01. The first time is an interior temporal interpolation; the final time is an explicit right trace. KvN recovery is implemented by interpolating complex amplitudes before squaring, preserving interference, but no completed KvN recovery is available from this first attempt. The two reference rows integrate every sampled physical state with 256 and 512 RK4 steps, retaining the original probabilities and original physical energy callback.

## Actual bounded outcomes

All nine completed histories have relative stored-system residual at most 4.76e-16. The largest reported numerical payload peak is 28,195,040 bytes; largest process peak RSS among these rows is 7,204 KiB. These different measures do not certify one another. The RK4 step difference at the final time is 5.72e-17 in coordinate-mean L2 and 2.09e-17 in energy. This small difference is not a reference-error certificate.

For example, order-4 Carleman with one DG2 slab differs from the 512-step reference at final time by 2.24e-12 in coordinate-mean L2 and 6.68e-11 in energy. Its interior-time energy difference is 1.71e-9. The receipt also preserves the larger DG1 differences and hierarchy-order differences. These are bounded sensitivity measurements at one coarse physical/configuration fixture, not a convergence or total-error theorem.

Every child retained the declared 256 MiB numerical payload cap, 10 billion history-elimination work cap, 1 billion source-query work cap, 100 billion physical-reference work cap and one million drift-call cap. The actual runner imposed 512 MiB address space, 180 seconds and 4 MiB captured-output file limits per child. No rejection was retried or converted into a completed outcome in this receipt.

The first failure is a separately reviewable local admission mismatch: the fixed diagnostic can emit at most 6,075 triplets including retained SBP endpoint duplicate terms. A checked bound is derived from the actual derivative-row lengths and the shared sparse canonicalization model. The reviewed correction charges all diagnostic assembly, sorting, matvec and norm work against the existing source budget before allocation. The separately pinned follow-up below preserves the original rejection evidence.

## Separately pinned KvN follow-up

The [three-request follow-up](data/2026-10-06-paired-history/kvn-followup.json) used binary SHA-256 `031df42e5ff2b5aa32dfc9558d11342e59307419f48f7b34890d0857a34ad73c`, with a [separate unchanged before/after build attestation](data/2026-10-06-paired-history/kvn-followup-build.json). It attempted only the three previously rejected KvN requests, once each, under the same physical fixture, accuracy and outer resource caps. DG1 with one slab and DG2 with one slab completed; DG1 with two slabs rejected. No further retry occurred.

The remaining rejection is the complete source-query admission: `4 * 243 * 2 * 513696 + 746496 + 3637726 = 1,003,009,246`, exceeding the declared 1,000,000,000 ceiling. The final term explicitly includes nonlinear diagnostic construction, canonicalization, matvec and norm work. The raw child returned its generic fixed-fixture admission error; the [separate source-formula explanation](data/2026-10-06-paired-history/followup-admission-explanation.json) records this derivation without rewriting that outcome.

| Completed KvN history | Unknowns | Source work | Relative residual | Numerical payload peak | Process peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| One DG1 slab | 486 | 503,696,734 | 2.39e-16 | 24,047,440 bytes | 7,240 KiB |
| One DG2 slab | 729 | 753,601,822 | 5.44e-16 | 27,935,440 bytes | 11,972 KiB |

Both have independently polarized nonlinear initial action norm 0.01551455. That establishes activity in this complete-coordinate discrete fixture, not a resolved nonlinear physical solution. The separately reported stored-history construction/interval-bound allowance is 1 billion work units per stage; source queries and Gaussian elimination have their own counters. These counters are not an aggregate job-work receipt.

The follow-up contains nine additional identity-checked comparisons, using the original physical and Carleman rows under explicit distinct-binary provenance. At final time the DG2 KvN result differs from the 512-step physical ensemble by 5.61e-5 in coordinate-mean L2 and 1.78e-4 in energy; its energy differs from order-4 DG2 Carleman by 1.78e-4. The DG1/DG2 KvN final energy difference is 6.27e-7, much smaller than the physical-reference discrepancy. This narrow time comparison does not resolve the occupied coarse configuration boundary or certify lift agreement. Interior-time amplitude norm squared is approximately 1.00088 for DG1 and 1.000000043 for DG2; the conditional observables normalize these interpolated amplitudes, and this raw norm is not a postselection probability.

## Evidence limits

This is bounded classical validation. It executes no native quantum circuit or sampled quantum measurements. A stored linear residual is not a forward-error certificate. Ensemble hierarchy truncation remains unverified; deterministic single-trajectory certificates do not apply to these averaged moments. The occupied periodic configuration boundary, physical mesh, time discretization and configuration discretization have no continuum convergence certificate. Literal open-wake pressure closure, mixed physical meshes and multi-host capacity remain separate work.

The executable/API and accounting definitions are in [the paired-history module guide](../../crates/quest-cfd/docs/paired-history.md). The portable runner is [paired_history.py](fixtures/quest-cfd/paired_history.py); it retains failures and withholds comparisons when common ensemble identity is absent.

The original execution used runner SHA-256 `c4b7703349f052f76b28ff4b4390bdf85c7c83496228ba2ed78c73c8a632f411`. A later parser-only review fix revalidated all saved outcomes and comparisons without executing any child again. This validation does not change the original execution identity.
