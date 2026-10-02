# Local benchmark result: 2026-10-02

The measured condition supports using Rug for this host: it was faster on all
fresh-output workloads at every measured precision. The complete production
Remez and QSP comparison is separate; this fixture does not establish its speedup.

Host: Apple M3 Pro, arm64, 18 GiB RAM, macOS Darwin 27.0.0. Toolchain:
`rustc 1.100.0-nightly (6bb1652a0 2026-09-22)`, LLVM 23.1.1, selected by the
project-local Nix/devenv environment. `CARGO_BUILD_JOBS=2`; optimized release,
thin LTO, one codegen unit. Both dependencies were compiled in the same fixture.
The parent task paused compilation during the timing window. All pre-timing
correctness witnesses passed and the timed command exited zero.

## Median process CPU time

Numbers are nanoseconds per operation, endpoint pair, or complete kernel. The
speedup is Astro fresh median divided by Rug fresh median; each median covers
three trials. Rug reuse rows are preserved separately in the summary CSV.

| Bits | Workload | Astro fresh ns | Rug fresh ns | Rug speedup |
|---:|---|---:|---:|---:|
| 128 | add | 49.30 | 26.11 | 1.89x |
| 128 | mul | 50.90 | 33.68 | 1.51x |
| 128 | div | 197.57 | 53.70 | 3.68x |
| 128 | exp_endpoints | 41,065.50 | 1,800.50 | 22.81x |
| 128 | ln_endpoints | 89,504.50 | 2,559.00 | 34.98x |
| 128 | sin_endpoints | 29,312.50 | 1,677.50 | 17.47x |
| 128 | cos_endpoints | 22,951.00 | 1,237.50 | 18.55x |
| 128 | matmul16 | 453,560.00 | 270,300.00 | 1.68x |
| 128 | qr16 | 673,180.00 | 301,920.00 | 2.23x |
| 256 | add | 52.45 | 30.66 | 1.71x |
| 256 | mul | 64.16 | 45.09 | 1.42x |
| 256 | div | 265.88 | 89.70 | 2.96x |
| 256 | exp_endpoints | 49,206.50 | 2,645.00 | 18.60x |
| 256 | ln_endpoints | 103,426.00 | 4,609.00 | 22.44x |
| 256 | sin_endpoints | 37,340.00 | 2,580.00 | 14.47x |
| 256 | cos_endpoints | 31,090.00 | 1,999.00 | 15.55x |
| 256 | matmul16 | 511,120.00 | 326,320.00 | 1.57x |
| 256 | qr16 | 749,900.00 | 370,640.00 | 2.02x |
| 512 | add | 54.52 | 32.73 | 1.67x |
| 512 | mul | 88.74 | 62.39 | 1.42x |
| 512 | div | 415.79 | 140.28 | 2.96x |
| 512 | exp_endpoints | 69,402.50 | 4,781.00 | 14.52x |
| 512 | ln_endpoints | 136,322.00 | 8,199.00 | 16.63x |
| 512 | sin_endpoints | 59,956.50 | 4,577.00 | 13.10x |
| 512 | cos_endpoints | 50,422.50 | 3,728.50 | 13.52x |
| 512 | matmul16 | 655,420.00 | 417,640.00 | 1.57x |
| 512 | qr16 | 958,520.00 | 471,660.00 | 2.03x |

## Features relevant to the decision

Rug offers explicitly directed rounding, in-place operations, assignment into
existing storage, and borrowed incomplete computations whose output precision is
chosen at assignment. These support the directed endpoints and reuse variants in
this fixture. Its fused operations and correctly rounded dot/sum helpers are
additional opportunities, but are not enabled in the matched comparison.
[Official Rug Float documentation](https://docs.rs/rug/latest/rug/struct.Float.html).

Astro rounds requested precision to whole machine words, so this comparison uses
word-aligned precisions. Its documentation recommends `RoundingMode::None` when
rounding error is acceptable; that is a different contract from the correctly
rounded and enclosing operations required here. Astro also supports allocation
with `no_std`.
[Official Astro BigFloat documentation](https://docs.rs/astro-float/latest/astro_float/struct.BigFloat.html),
[official Astro crate documentation](https://docs.rs/astro-float/latest/astro_float/).

## Limits

- This is a single Apple Silicon machine, one native build, and three trials.
  It is not Linux/HPC, MPI, cross-compiler, or deployment validation.
- CPU timing mitigates scheduling pauses but not thermal/frequency variation.
  No CPU affinity or frequency lock was applied. Some trial ranges exceed 15%;
  the raw and summary files preserve every observation. Reported ratios are
  descriptive medians, not statistical confidence bounds.
- Inputs cover full-significand positive values near one and a well-conditioned
  16 by 16 matrix. Extreme exponents, catastrophic cancellation, large argument
  reduction, ill-conditioned matrices, allocation pressure, and parallel scaling
  require separate evidence.
- QR uses modified Gram-Schmidt rather than production pivoted Householder.
  Production validation, full solver quality, API/error-policy preservation,
  and native GMP/MPFR build provisioning remain separate gates.
- 2048-bit MPFR provides independent numerical witnesses for Astro, while Rug
  calls MPFR itself. This checks Rug integration against more accurate values,
  not MPFR's implementation independently.
- Reuse figures are not a promise of production speedup: the current public
  backend interface owns operands and a migration must preserve its semantics.

## Preserved evidence

[Raw CSV](raw.csv), [summary CSV](summary.csv), [run/witness log](run.log), and
[project toolchain provenance](toolchain.log). The package lockfile is checked in.
Source and lockfile SHA-256 at measurement:

```
9a9aafc939246d785c90357fa9402c14ea15a0c51f5a470e04f138e047e429f6  src/main.rs
f677b335d399f61a90c0784beb3620c0407a8ad1835fe633b8724f10d9d4035d  Cargo.toml
2ab2ca5cec165513eddd4dc168f1047898df2e748863d104df63e840eff75c90  Cargo.lock
```
