# Fixed seven-row initial weak-generator experiment

The complete five-coordinate periodic BDM1/P0 diagnostic ran seven fixed requests
once each on 2026-10-06. Three completed, three exceeded the unchanged work budget,
and one failed the minimum-support policy. No request was retried or replaced.
The results expose unresolved sampling and generator errors; they establish no
configuration convergence, history solution or quantum result.

The [frozen protocol](../../crates/quest-cfd/docs/configuration-weak-experiment.md)
and [diagnostic theory](../../crates/quest-cfd/docs/configuration-weak.md) define
the inputs, formulas and resource model. The source retains all five physical
coordinates, with viscosity 0.01, time zero and bump center
`[0.15, -0.1, 0.07, 0.11, -0.04]`. Configuration domains are `[-extent, extent]^5`.
The width is 0.5 except for the final sensitivity request, which uses 0.6.

## Outcomes and costs

| Order / cells / extent / width | Complete state dimension | Outcome | Planned source work | Original force calls: attempted / planned |
| --- | ---: | --- | ---: | ---: |
| DG1 / 3 / 1 / 0.5 | 7,776 | Completed | 633,420,032 | 2,107 / 2,107 |
| DG1 / 4 / 1 / 0.5 | 32,768 | Completed | 838,195,456 | 2,107 / 2,107 |
| DG1 / 5 / 1 / 0.5 | 100,000 | Work rejected | 1,942,677,760 | 58 / 4,667 |
| DG2 / 1 / 1 / 0.5 | 243 | Sampling rejected | Source not constructed | 0 / 0 |
| DG2 / 3 / 1 / 0.5 | 59,049 | Work rejected | 2,272,068,448 | 58 / 6,309 |
| DG1 / 5 / 5/3 / 0.5 | 100,000 | Work rejected | 1,389,000,960 | 58 / 2,107 |
| DG1 / 3 / 1 / 0.6 | 7,776 | Completed | 633,420,032 | 2,107 / 2,107 |

The source-work ceiling remains one billion units, including declared input
construction, hashing and diagnostics. Managed payload is capped at 256 MiB.
Each child also has a 512 MiB address-space limit, 180-second timeout and 4 MiB
capture limit per file. These three notions of resource bounds are distinct.
Reported managed peak bounds range from 196,608 bytes for sampling rejection to
8,521,328 bytes. Measured child peak RSS ranges from 4,072 to 5,648 KiB. The build
uses optimization level zero; these timings are not release performance results.

Each state-built request validates every coefficient. Completed requests contract
1,024 exactly nonzero rows. Work-rejected requests retain their complete initial
state identities and diagnostics, but query no generator rows. Their 58 attempted
force callbacks belong to source extraction. The one-cell DG2 request has only
one distinct supported location per axis and rejects before source or full-state
allocation. A rejected planned cost is not work that executed.

## Sampling and weak-rate evidence

The initial continuous density is the square of the specified compact
half-density. Its exact mean is the center by symmetry. Sampled mean minus center
therefore measures an independent sampling error. The three-cell width-0.5 row
has first-coordinate bias 0.183333 and standard deviation divided by grid spacing
about `7.53e-7`. It passes minimum-two support while concentrating almost all
probability at one physical location. Duplicate DG endpoint coefficients do not
increase distinct spatial resolution.

Let `G` be the normalized discrete weak rate, `P` the original full physical
rate averaged over the same sampled density, and `E = G - P`.

| Completed request | Coordinate defect norm | Energy absolute defect | Energy G | Energy P |
| --- | ---: | ---: | ---: | ---: |
| DG1 / 3, width 0.5 | 1.412267 | 0.989841 | `3.42e-16` | -0.989841 |
| DG1 / 4, width 0.5 | 0.0924104 | 0.0116481 | -0.0557221 | -0.0440740 |
| DG1 / 3, width 0.6 | 0.917552 | 1.004635 | `-6.51e-17` | -1.004635 |

The nearly zero three-cell energy rates have an algebraic explanation. Both
widths occupy only the repeated locations `-1/3` and `+1/3` on each coordinate.
In exact mass-whitened coordinates, kinetic energy is consequently `5/18` on
every occupied coefficient. For any diagonal observable `M`, the normalized
rate is

\[
\frac{2\operatorname{Re}\sum_i\bar z_i(M_i-\langle M\rangle)(Lz)_i}
     {\sum_i|z_i|^2}.
\]

It vanishes whenever `M` is constant on the occupied support, for any generator.
Represented node asymmetry and floating-point mass/Cartesian integration leave
roundoff-scale discrepancies here. This is a sampling degeneracy, not evidence
that viscous physical energy is conserved. Small probability-rate or
Cartesian-vs-chart residuals do not bound the weak discretization error.

Two pairs supply finite sensitivity results: cells 3/4 and widths 0.5/0.6. The
receipts retain changes in `G`, `P` and `E` separately, with decomposition
roundoff at most `7.42e-17`. The large change in the sampled physical energy rate
between cells 3 and 4 prevents treating the reduced defect as a clean generator
convergence study. Broadening the bump improves some means while increasing the
energy defect. Cells 4/5 and the domain-extension rate comparison remain
unavailable because one request rejected. Their initial sampling data remain.

Every state-built row has zero exterior trace. Outer-cell occupation remains
large in some rows; neither is an integrated leakage certificate. The intended
fixed-spacing domain pair also has different binary64 node and ensemble hashes.
Its nominal spacing does not establish identical represented samples.

## Provenance and reproduction

The [raw receipt](data/2026-10-06-configuration-weak-campaign/receipt.json), SHA-256
`bf677ee747da930647cb05a3c4d4174399cb475d126d3f2b454eff76f27677de`, links all seven
unaltered stdout, stderr and timing files. The
[saved-output analysis](data/2026-10-06-configuration-weak-campaign/analysis.json)
retains full means, concentration and pair comparisons. The
[publication index](data/2026-10-06-configuration-weak-campaign/publication.json),
SHA-256 `2dadf47c30d8144fee16cb2a0531fee0eb5702c3ab4cd03ce0026d8b130f2cf1`,
binds 29 artifacts and states their scopes.

The [build attestation](data/2026-10-06-configuration-weak-campaign/build-attestation.json)
and [507-file source manifest](data/2026-10-06-configuration-weak-campaign/build-source.json)
cover the actual twelve local Cargo packages in the default example build.
The executable SHA-256 is
`481156a3201148eaefdbfb59fadb434071c6d53bfc87f02bfcb20ab0e23e2335`.
The [execution attestation](data/2026-10-06-configuration-weak-campaign/execution-attestation.json)
records unchanged source, executable, runner and decoder identities. External
registry sources, host environment and non-Rust package assets remain outside
this scope; it is not a complete reproducible-build certificate.

Independent saved-output review checked every schema, all raw hashes, the actual
package closure and sampling/comparison arithmetic without launching more
physical children. The earlier three serializer fixtures remain separately
labeled. Protocol tests comprise four Rust tests and eleven Python groups;
those passed independent review before this campaign. The campaign follows
checkpoint 07 and has no newer broad workspace acceptance attached.

Run protocol checks from the workspace root:

```sh
cargo test -p quest-cfd --example configuration_weak
python3 -B docs/verification/fixtures/quest-cfd/test_configuration_weak.py
```

To reproduce the fixed experiment, follow the protocol's build-pinning procedure
and invoke its runner with a pinned executable and a new results directory.
Sampling, configuration, domain, regularization, physical and temporal
convergence remain separate requirements.
