# Dashu consolidation evidence

This record covers the Dashu-only port layered on the completed static numerical
architecture. Project-owned integers, rationals and multiprecision floats use
Dashu; the independent QSP algorithms and exact-angle semantics remain distinct.
See the [migration and retention ledger](../../2026-10-02-dashu-migration.md).

## Environment and source identity

- Project-local `devenv shell`, two Cargo build jobs, Apple M3 Pro / aarch64 Darwin.
- Nightly 2026-09-23, rustc `1.100.0-nightly`, revision
  `6bb1652a020e80cef79332741d89e996d71933c9`, LLVM 23.1.1.
  [Full compiler receipt](toolchain.txt).
- Published pins: dashu-base/ratio 0.6.1 and dashu-int/float 0.6.2.
  [Lock inventory](locked-arithmetic.json).
- [Source fingerprints](sources.json) identify the working tree independently
  of its eventual Git commit. The starting committed tree was `001a2b6`;
  the performance baseline is the later frozen, uncommitted static architecture
  with Astro arithmetic, not that earlier commit.
- `pre-port-source.tar.gz` preserves that baseline. Extract into an empty
  directory to rerun the complete solver/project fixtures. The separate
  `historical-astro-rug-primitive-source.tar.gz` preserves the original primitive
  experiment. [Archive hashes](archives.json) distinguish source identities and all receipt bundles.

## Validation

[validation-summary.json](validation-summary.json) records exact commands, exit
codes and wall times. Compressed command logs are adjacent to this file.
Owner handoffs and source reviews retain their status at the time of writing;
references there to coordinator checks being pending are superseded by this
final validation summary and the final performance receipts. Platform gaps remain.

| Gate | Observed result |
| --- | --- |
| Workspace with supported numerical, worker, serialization and native features | 1,010 passed; eight skipped |
| Workspace doctests | 56 passed; one existing ignored example |
| Workspace all-target Clippy with `-D warnings` | Passed |
| Rustdoc with `RUSTDOCFLAGS=-D warnings`, book, formatting | Passed |
| Pure CLI and scalar kernels | Passed |
| Compiler without optional features, synthesis-only, workers-only | Strict library Clippy passed in each configuration |
| Direct synthesis without process workers | Passed |
| QSP certification without offline synthesis | Strict all-target Clippy passed |
| Regenerated binding comparison | Passed |
| External native consumers | CPU and OpenMP numerical/deployment checks passed, including renamed dependencies |
| Degree 8,192 interval FFT and degree 8,105 offline/parallel fixtures | All three explicitly passed in release mode |

The workspace skips comprise the three separately executed scale fixtures and
five process-execution tests that require Linux. Linux/MPI/accelerator execution
and performance remain unverified here. The pinned nightly emits its known
`generic_const_exprs`/next-solver diagnostic; the feature remains incomplete.

Final configuration checks cover corrected import gates for pure compiler,
synthesis-only and worker-only builds. The final 1,010-test workspace run includes
the paired-trigonometry and native input-watch regressions. Both MP scale
fixtures were rerun after the trigonometric changes; the unchanged binary64
parallel scale fixture retains its earlier successful release receipt.

Coverage includes directed rounding, exact decimal and binary64 imports, ties,
signed zero, subnormals, overflow midpoint, retained cancellation digits,
bit-granular 65-bit artifacts, interval domains/extrema, root coverage and
exhaustion, exact wire normalization, full responses/phases, clean ancillas and
exact replay. MP Remez certifies below the binary64 enclosure floor and separately
rejects binary64 exports whose rounding exceeds the requested bound.

## Reviews and performance

- [Independent numerical review](numerical-review.md)
- [Architecture review](architecture-review.md) and
  [final configuration/cleanup addendum](architecture-review-final.md)
- [Benchmark design review](benchmark-review.md)
- [Source-based performance investigation](performance-investigation.md)
- [Performance measurements](performance.md)

The exact-rational oracle has independent mathematics but shares Dashu integer
arithmetic. Frozen CPython decimal/libmpdec reference enclosures provide external
arithmetic witnesses; they are not a claim of an exhaustive second-library oracle.
Historical Rug measurements retain their original source and backend provenance;
there is no completed whole-solver Rug baseline.

The final measurement archives are `mp-solvers-final.tar.gz` and
`project-final.tar.gz`. Their before/after source manifests agree. The separately
named pre-optimization archives preserve the first Dashu measurements; they are
not substituted for the final results. The primitive archive records the active
Dashu-only fixture, with historical Astro/Rug comparisons kept separately.
