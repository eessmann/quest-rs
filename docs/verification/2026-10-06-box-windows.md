# Full-window box reference campaign

The optimized campaign completed 42 of 48 declared classical attempts under an
actual 512 MiB process address-space cap and a 45-second deadline per attempt.
Two separately declared smaller-step cavity runs then completed. All 50 attempts
retain every independent velocity coordinate admitted by their full physical
chart. They establish neither physical benchmark convergence nor quantum execution.

The [release receipt](data/2026-10-06-box-windows/release.json) contains final
snapshots, pressure/constraint diagnostics and cavity profiles. The
[cavity time-step follow-up](data/2026-10-06-box-windows/cavity-stability.json)
preserves the same physical problem and binary. The
[derived comparisons](data/2026-10-06-box-windows/comparisons.json) compare only
adjacent admitted values along one axis, holding case, Reynolds convention,
horizon and the other approximation parameters fixed. Profile differences are
maximum component changes at identical sample points, not spatial norms.

## Scope and results

| Family | Reynolds numbers | Final time | Declared physical meshes/orders |
| --- | --- | --- | --- |
| 2D Taylor–Green | 100 | 10 | 1, 2, 4 subdivisions; BDM1/P0 and BDM2/P1|
| 3D Taylor–Green | 100, 1600 | 20 | 1, 2 subdivisions; BDM1/P0 and BDM2/P1|
| 2D cavity | 100, 1000 | 100 | 1, 2, 4 subdivisions; BDM1/P0 and BDM2/P1|
| 3D cavity | 100, 1000 | 100 | 1, 2 subdivisions; BDM1/P0 and BDM2/P1|

A mesh value counts subdivisions on each box axis; the implementation splits
squares into triangles and cubes into tetrahedra. Baseline RK4 step 0.01 and
selected independent step 0.005 runs reach the same final times. These are
final-window snapshots, not averages over the manifests' observation windows or
complete time histories. Initial states, boundary conditions and Reynolds
conventions are those of the hashed case manifests.

The 2D Taylor–Green BDM2 analytic velocity L2 errors at T=10 decrease from
3.59197809 to 2.56530519 to 0.687566975 as subdivisions increase 1→2→4.
The finest step-halving changes this error by 3.924e-10 and mean energy by 5.056e-12.
These are measured differences, not a validated temporal error enclosure or a
resolved continuum solution. BDM1 errors are 3.54402972,3.59631023 and 2.44402912;
that coarser sequence is not monotone. The nonlinear 3D Taylor–Green case has no
analytic error field asserted by this implementation.

Four rejected attempts are 3D BDM2 mesh 2 cases: the bounded reference constructor
admits at most 768 broken velocity coefficients. Two others are 2D Re100 BDM2
mesh 4 at steps 0.01 and 0.005; both report full-force overflow. The separately
frozen follow-up keeps the same 121-coordinate physical chart, T=100, work and
memory caps, and uses steps 0.0025/0.00125. Both complete in 19.60/38.27 seconds.
Their mean energies are 0.03256163777227853 and 0.03256163777227731; sampled
centerline velocity components differ by at most 1.638e-14. Their steady residuals
are 4.815e-15 and 1.105e-14. This is evidence of time-step sensitivity of the
failed runs and agreement of the final successful snapshots, not a formal
stability proof or a bound on the unresolved physical error.

For comparison, Re1000 BDM2 mesh 4 completes at the original two steps. Its
T=100 steady residual is about 3.095e-7; sampled centerline components change by
2.254e-13 under step-halving. The fact that a discrete cavity reaches a small
steady residual does not establish agreement with published profiles. The 3D
receipts retain sampled secondary circulation and reflection defects; neither
finite samples nor one tetrahedral orientation prove continuum symmetry.

The 48 release attempts total 78.413 seconds of child wall time; the largest
reported process RSS is 10,760 KiB. Build, Python parent and comparison work are
separate. No MPI, full lifted history, circuit preparation or quantum sampling
is part of this campaign. [Cylinder-window evidence](2026-10-05-cylinder-window.md)
is separate; the literal 3D wake pressure/outlet definition is still unresolved.

## Explicit admission and reproducibility

The higher-order RK4 API now accepts an explicit modeled integration-work limit.
`PhysicalSpace::integrate_rk4` and the original reference entry point retain their
previous 1-billion-unit default. `integrate_rk4_with_work_limit` and the reference
CLI's `--max-classical-work` admit an explicit alternative before reconstructing
coefficients. The campaign declares 2 trillion units per higher-order attempt;
actual modeled work is recorded. The flag rejects unsupported reference modes,
and neither the 1-million-step ceiling nor the 768-coefficient chart ceiling is
changed. This is a work model, not FLOP telemetry or a measured memory limit.

```sh
cargo build --release -p quest-cfd
cp target/release/quest-cfd /tmp/quest-cfd-box-reference
python3 -B docs/verification/fixtures/quest-cfd/box_windows.py \
  /tmp/quest-cfd-box-reference /tmp/box-windows.json \
  --cap-mib 512 --timeout 45 --max-classical-work 2000000000000
python3 -B docs/verification/fixtures/quest-cfd/box_windows.py \
  /tmp/quest-cfd-box-reference /tmp/cavity-stability.json \
  --cavity-stability-study --cap-mib 512 --timeout 45 \
  --max-classical-work 2000000000000
python3 -B -m unittest discover \
  -s docs/verification/fixtures/quest-cfd -p test_box_windows.py
```

The actual immutable optimized binary has SHA-256
`65e24fd6237d59a8d16cc6a6bfd8e301a016b1dbbf18cd4bb8861c71b4022af6`.
Receipts hash binary, runner, collector and manifests, and verify the binary
unchanged after execution. Those identities are not a complete build attestation.
Paths in the example are generic; copy the built executable before running other
Cargo commands that may replace workspace binary links.

## Preserved failures and validator correction

The [initial debug receipt](data/2026-10-06-box-windows/initial-debug.json)
contains 41 attempts with original classifications. Its first validator
incorrectly expected nested BDM2 observations, while the actual serializer
flattens them. [Revalidation](data/2026-10-06-box-windows/initial-debug-revalidated.json)
corrects exactly one successful run's classification without rerunning physics:
21 executed and 20 rejected. The next
[explicit-work debug campaign](data/2026-10-06-box-windows/higher-work-debug.json)
contains 17 attempts, 9 completed and 8 rejected; slower debug timeouts remain
historical evidence and are not relabelled as physical failures.

Independent review subsequently reproduced wrong-order and missing-diagnostic
acceptance, plus a malformed deeply nested JSON report escaping the validator
with a recursion exception. Current validation requires the exact BDM reference
method/order, pressure/constraint diagnostics, supported analytic errors and
cavity outputs. The bounded collector rejects duplicate JSON keys, nonfinite
values and excess depth/nodes; traversal does not recurse in Python. Twelve
regressions include an actual malformed child process. Historical receipts keep
their original runner hashes; [semantic revalidation](data/2026-10-06-box-windows/revalidation.json)
is separate from execution and changed none of the release/follow-up classifications.

The full release/follow-up receipts and all 45 derived fixed-axis comparisons
were independently checked. They remain coarse classical evidence. No phase,
Carleman order, configuration-grid, geometry, observable-bias or physical
convergence gate is closed by these runs.
