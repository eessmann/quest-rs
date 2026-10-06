# Additive DG2 configuration weak-rate diagnostic

One DG2/two-cell initial diagnostic completed within the previously fixed caps.
Its energy weak rate is nonzero, removing the constant-energy support degeneracy
identified in the [seven-row campaign](2026-10-06-configuration-weak-campaign.md).
The energy-rate discrepancy is larger than the saved DG1/four-cell result, and
the sampled physical expectation changes with the quadrature weights. This is
a sensitivity result, without a configuration-convergence, history-solve or
quantum-execution claim.

## Fixed experiment and retained result

The [method and runnable protocol](../../crates/quest-cfd/docs/configuration-weak-energy-shell-v2.md)
defines this separate example. The original seven requests and outputs are
unchanged. The new request retains all five physical coordinates and uses
configuration DG2 with two cells on each `[-1,1]` axis, width `0.5`, center
`[0.15,-0.1,0.07,0.11,-0.04]`, viscosity `0.01` and time zero. The minimum-two
distinct-sample rule and nonlinear witness remain enabled.

| Recorded quantity | Result |
| --- | ---: |
| Retained and validated complex coefficients | 7,776 |
| Visited nonzero coefficients | 243 |
| Distinct occupied physical points | 32 |
| Input preparation work | 32,997,376 |
| Source work, including declared input preparation | 319,676,832 |
| Original physical work / attempted calls | 54,500,000 / 545 |
| Modeled managed peak bytes | 8,520,872 |
| Observed GNU time peak RSS | 4,128 KiB |
| Observed child / parent elapsed seconds | 0.09 / 0.1150 |

Duplicate zero endpoints remain separate DG coefficients with their original
weights; no physical coordinate or configuration coefficient was removed.
Work is the conservative admission model, not a processor instruction count.
Managed limits remain 256 MiB, one billion source-work units, 100 billion
physical-work units and one million physical calls. The child separately had
512 MiB address-space, 180-second and 4 MiB captured-file caps. Process RSS and
managed accounting describe different quantities.

Exactly one build and one physical child were attempted, with no retry, larger
cap or replacement reference. See the byte-original
[row](data/2026-10-06-configuration-weak-energy-shell-v2/p2-c2-e1-w1_2.json)
and [receipt](data/2026-10-06-configuration-weak-energy-shell-v2/receipt.json).

## Weak-rate and sampling interpretation

Let `G` be the normalized weak-generator rate, `P` the independently evaluated
expectation of the original physical rate under the sampled ensemble, and
`E = G - P`.

| Integrated energy rate | Saved DG1/four cells | DG2/two cells |
| --- | ---: | ---: |
| `G` | -0.0557220626638904 | -0.1072917589814428 |
| `P` | -0.0440739815528260 | -0.0847214096997221 |
| `E` | -0.0116480811110644 | -0.0225703492817207 |

The defect magnitude increases by a factor of `1.9376882`; its ratio to the new
sampled physical energy rate is approximately `0.266407`. Both grids have the
same distinct occupied physical locations, but aggregate weights at
`[-0.5,0,0.5]` change from `[0.5,0.5,0.5]` to `[2/3,1/3,2/3]`. Their sampled
ensembles differ. The comparison therefore cannot isolate stencil error or
serve as pure h/p refinement. All six coordinate/energy comparisons retain
`Delta G`, `Delta P` and `Delta E`; maximum decomposition roundoff is about
`1.39e-17`.

Two independent saved-only calculations reconstruct separable probabilities
from the represented nodes and weights. The 80-digit check reproduces means
within `1.95e-16`, variances within `1.25e-16` and the energy expectation within
`9.03e-17`. These are numerical cross-checks, not interval enclosures. Neither
calculation reruns the generator or a physical trajectory. The
[analysis](data/2026-10-06-configuration-weak-energy-shell-v2/analysis.json)
and [independent review](data/2026-10-06-configuration-weak-energy-shell-v2/independent-review.json)
retain the full comparison.

The sample-support rule passes while concentration remains unresolved. The
last coordinate has sampled standard deviation divided by distinct spacing
about `0.00575645`; its mean is about `-0.00001657`, compared with the specified
continuum center `-0.04`. Outer-cell occupation is one because both cells touch
the exterior. The exterior trace is exactly zero. Neither occupation nor a
zero initial trace is an integrated leakage bound. The nonzero nonlinear
witness, `1.84621e-6`, is an initial diagnostic and does not certify future
dynamics.

The accompanying complex-amplitude regression establishes why normalized
rates matter: for a deliberately non-skew generator with norm squared seven
and norm derivative minus two, constant occupied energy has zero normalized
rate; a nonconstant observable gives `-36/49`. The production diagnostic is
unchanged by this added test. The DG2 occupied energies are not constant, and
its measured energy rate is nonzero, but sampling and weak discretization
errors still require independent refinement.

## Source, runner and verification scope

The default example build reports twelve local packages and 509 source and
configuration files. Their lists and bytes, reviewed protocol files, runtime
helpers, saved reference and executable identities were unchanged across the
build and execution. The pinned executable SHA-256 is
`727ea4196c7c03fb20332dba5f0caa8467e5e64cf3d5ba1ece06b3152267227d`.
It used optimization level zero, debug assertions and overflow checks; these
timings are not release-performance measurements. Unchanged Cargo artifacts
may be reused. Resolved compiler-command hashes identify the rustup launcher;
toolchain versions are recorded separately. External registry contents,
unused native packages, non-Rust assets and arbitrary environment are outside
this scoped consistency check.

Source review caught a runner fault in which a completed child could be lost
from the receipt if its executable disappeared before a post-run hash read.
The runner now saves the captured row first, records unavailable identities,
and only compares numerical results after successful unchanged provenance.
Reference hashing and decoding consume the same bounded byte snapshot. The
self-removing-child regression reproduces the original fault and passes with
the correction; the historical failure remains retained.

Independent focused verification passed five library tests, six shared
producer tests through each example entry point, eleven historical Python
groups, eight additive Python groups and strict selected Clippy. The two
example invocations run the same six tests; they are not twelve independent
mathematical cases. The [verification index](data/2026-10-06-configuration-weak-energy-shell-v2/verification-references.json)
records commands and exact log identities. These scoped results do not replace
later workspace acceptance.

The [publication index](data/2026-10-06-configuration-weak-energy-shell-v2/publication.json)
binds all sixteen artifacts. Numerical row, receipt and streams remain
byte-original. Public build/execution context and compiler records replace
host path prefixes with generic placeholders and preserve the original
private hashes separately; those original hashes do not hash the normalized
public bytes. The original seven-row campaign remains immutable.
