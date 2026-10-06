# Paired histories of one complete physical ensemble

`paired_history` is an explicitly bounded classical validation fixture. It compares full five-coordinate periodic BDM1 dynamics through KvN and normalized symmetric Carleman histories. It assembles stored histories and uses the admitted direct classical reference solve; it allocates no quantum register and is never a quantum fallback.

The fixed physical viscosity is 0.01 and the horizon is 0.01. A shifted tensor bump on the 243-node configuration DG2 grid supplies normalized sampled probabilities. The Carleman initial vector contains the averages of **all** lifted monomials under those probabilities. Classical reference trajectories start at those same full-coordinate samples with the same weights. Lifting the ensemble mean would change this nonlinear problem; the implementation does not do that.

The common observables are all five coordinate means and integrated kinetic energy. KvN readout first interpolates the complex temporal amplitudes and then forms normalized probabilities, using the original `PeriodicBdm1::energy`. Carleman means and energy are recovered linearly from degree 1 and degree 2 moments; their squared norm is not a probability normalization. A finite negative Carleman energy remains a failed physical approximation diagnostic, not a malformed numerical solve.

Observations are at T/4 (inside a slab, away from the temporal nodes) and at the final right trace. Temporal DG1 with one/two slabs and DG2 with one slab are evaluated independently. Carleman orders 2/3/4 retain 20/55/125 complete monomials with one common scale. The 243-node KvN histories have 486/972/729 unknowns. The two ensemble references use 256/512 RK4 steps.

```sh
cargo build --release -p quest-cfd --example paired_history
python3 docs/verification/fixtures/quest-cfd/paired_history.py --help
```

The example accepts exactly one typed request, for example:

```sh
target/release/examples/paired_history --request-json \
  '{"History":{"lift":{"Carleman":{"order":2}},"time_cells":1,"time_order":1}}'
```

Unknown physical, mesh or scale fields reject. The subprocess runner freezes all 14 requests, binds the executable/source and common ensemble identities, and preserves successful, rejected, timed-out and malformed-output rows. A successful standalone example is not evidence that OS caps were applied; the runner supplies those controls.

## Admission and error interpretation

The history-elimination cap is 10 billion modeled work units and whole-live numerical storage is 256 MiB. Physical reference integration has a separate 100 billion work-unit cap and a one million drift-call cap, and repeated source, initial-moment and readout queries have a 1 billion work cap. Stored history assembly and its interval-bound evaluation receive a separate 1 billion work allowance, reported as `history_assembly_work_allowance` (zero for the physical reference). This repeated stage allowance is not included in source-query or Gaussian-elimination work. Construction and direct-solve overlaps are charged separately; the reported modeled peak retains the larger bound, including duplicate triplet construction. These are conservative managed-payload/arithmetic models, not measured RSS or processor instructions. The runner separately enforces 512 MiB address space, 180 seconds per child and 4 MiB captured-file limits.

The initial construction includes fixed full physical assembly, bounded MathCore extraction, complete grid and sampled ensemble storage. The read-only `PeriodicBdm1::retained_bytes` accessor counts actual hidden Vec capacities. The implementation subtracts live source/ensemble/lift owners before admitting the direct reference solve. Scalar fingerprints use byte-wise FNV provenance; the external SHA manifests separately bind source and binary bytes.

A small residual does not prove small forward error. Reference step sensitivity, temporal-layout differences and hierarchy-order differences are reported separately, without adding them into a rigorous total-error estimate. The single-trajectory Carleman truncation/reconstruction theorem is not applied to ensemble moments. The physical mesh is fixed; configuration boundary occupation is intentionally unresolved. This stage does not claim regularization/window/PDE convergence, a resolved nonlinear response, native inverse execution or quantum sampling.

The [first actual capped campaign](../../../docs/verification/2026-10-06-paired-history.md) preserves all 14 attempts: two references and nine Carleman rows completed; all three KvN rows rejected the independent nonlinear diagnostic’s private sparse-work cap. The separately pinned three-request follow-up completed DG1 and DG2 with one slab; the two-slab KvN row remains rejected because aggregate source work is 1,003,009,246, above the unchanged one-billion cap. The published comparisons retain a substantial KvN/physical-reference discrepancy and make no convergence claim.

The reviewed follow-up admission correction derives the nonlinear diagnostic sparse-work bound from retained derivative rows, including endpoint duplicate contributions. Its construction, canonicalization, matvec and norm work are added to source-query admission before the diagnostic allocates. The first receipt remains immutable; the actual follow-up has its own pinned binary and explicitly labeled receipt.
