# Dashu primitive workloads

This standalone pure-Rust fixture retains the deterministic inputs, operation counts and matrix/modified Gram-Schmidt workloads from the archived Astro/Rug benchmark. It measures native Dashu contexts with fresh outputs and locally owned constant caches. Dashu subtraction may retain a guard digit, so compare at matched solver accuracy as well as nominal precision.

Run the numerical inclusion/interchange tests before timing. This executable is a timing fixture, not an independent arithmetic oracle. The solver comparison in `../mp-solvers` measures complete Remez and offline QSP behavior.

Primitive ratios compare nominal bit precision; the matrix and modified Gram-Schmidt workloads do not establish matched residual accuracy. Solver acceptance bounds provide the matched-accuracy evidence separately.

```sh
CARGO_BUILD_JOBS=2 devenv shell -- cargo run --release --manifest-path docs/verification/fixtures/mp-backends/Cargo.toml > dashu-primitives.csv
```

Historical measurements in `results/2026-10-02` retain their original backend provenance. Their exact source, manifest and lock are archived in `docs/verification/data/2026-10-02-dashu-consolidation/historical-astro-rug-primitive-source.tar.gz`; they are not active project dependencies. Comparisons against those rows are historical, not interleaved same-run measurements.
