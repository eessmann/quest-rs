# Complete-coordinate initial weak-generator verification

The bounded `PeriodicWeakSource` diagnostic passed independent review and focused
checks on 2026-10-06. It retains the original five-coordinate periodic BDM1/P0
system and the complete supplied configuration state. This stage checks an
initial classical rate calculation; it executes no history solve or quantum
circuit and establishes no configuration or physical convergence.

The [method and API](../../crates/quest-cfd/docs/configuration-weak.md) describe
normalization, the original-force reference, Cartesian energy integration and
admission. The [four-file source manifest](data/2026-10-06-configuration-weak/source.json)
has SHA-256 `62ec097130b6b3edf2b6333efa4bdccf0b8747cd293f9d12c61ecc0cecb5e046`.
The [focused receipt](data/2026-10-06-configuration-weak/focused.json), SHA-256
`95afc1a138160ef20cfe248b347c85d83f865d244f1512a983a8d4acabed305c`, records
private log hashes and scopes. These are focused identities, not a complete
source-to-binary attestation. This additive stage follows the frozen workspace
checkpoint 07; that checkpoint does not cover this module.

## Numerical and resource checks

For supplied complex amplitudes, the calculation contracts generated rows
directly, preserving their duplicate contributions and interference. It reports
the raw skew-identity rate, the probability derivative and the corrected
normalized rate separately. It compares all five coordinate rates and integrated
kinetic-energy rate with the original constrained force at the same samples.
Energy and power use Cartesian triangle moments rather than relying solely on
the mass-coordinate norm shortcut. Production stores no generator, commutator
or action vector.

Four independent analytic tests and eight integration tests passed. They cover
constant/affine transport, a manually assembled complex matrix, normalization
correction, nonzero boundary contamination, independent triangle quadrature,
nonzero fifth-coordinate action, original-force polarization, underflowing but
nonzero amplitudes, failed-extraction counters and admission boundaries. The
reviewer reran these twelve tests and strict scoped Clippy; the implementation
owner also ran fourteen affected existing tests. The compiler's inherited
`generic_const_exprs`/next-solver warning remains visible; it is not a new Clippy
exception.

One genuine development finding concerned failed source extraction: reserved
force calls were initially reported as attempted calls. A counted callback now
preserves actual attempts even when extraction fails early. The receipt also
retains failed test-fixture attempts: an incorrect hand-assembled periodic matrix
and a full-model probe on a conserved coordinate. Corrections to those oracles
do not constitute changes to the production generator or relaxed tolerances.

The full DG2/three-cell rejection was independently reproduced:

| Quantity | Recorded value |
| --- | ---: |
| Complete configuration dimension / validated entries | 59,049 |
| Exactly nonzero contracted rows | 3,125 |
| Planned source work / unchanged ceiling | 2,029,007,712 / 1,000,000,000 |
| Row-query work allowance | 513,696 |
| Planned original-force calls / actual attempts | 3,183 / 58 |
| Visited generator rows | 0 |
| Accessible supplied-state payload | 944,784 bytes |
| Retained historical construction peak | 8,388,608 bytes |

This is a resource rejection, not a numerical refinement result. Only rows whose
real and imaginary components are both exactly zero can be omitted from this
instantaneous contraction. Every state entry is validated; this rule does not
remove physical coordinates or define a sparse-support evolution scheme.

## Reproduction and remaining work

Run from the workspace root:

```sh
cargo test -p quest-cfd --lib configuration_weak::tests
cargo test -p quest-cfd --test configuration_weak
cargo test -p quest-cfd --test configuration_diagnostics --test configuration_flux --test kvn_recipe
cargo clippy -p quest-cfd --lib --tests --no-deps -- -D warnings
```

The API charges construction plus one request on each invocation. Prior input
preparation is unknown when its optional caller declaration is absent, and the
borrowed slice cannot disclose hidden backing capacity. An enclosing producer
must admit its allocations and charge construction before calling the diagnostic.
Managed payload/work bounds do not imply measured RSS or OS process caps.

Zero exterior trace, outer-cell occupation, distinct support nodes, concentration
and the nonlinear witness answer different questions. None independently proves
sampling resolution or boundary-flux control. A separately admitted experiment
must still measure configuration, width and domain dependence; temporal histories,
trajectory error, quantum execution and total observable accuracy remain separate
acceptance requirements.
