# Complete P2 cylinder evolution: retained temporal accuracy failure

The [bounded evolution API](../../crates/quest-cfd/docs/cylinder-p2-evolution.md)
completed all three fixed two/four/eight-step runs while retaining the entire
54-coordinate velocity chart. **The temporal sensitivity study failed its
predeclared accuracy criterion.** Small original-equation residuals and a
resolved quadratic action do not change that outcome.

## Fixed experiment and numerical result

The geometry, complete quadratic inlet lifting and mechanical-traction outlet
are the [separately verified P2 cylinder source](2026-10-06-cylinder-p2.md).
Each run starts from its canonical minimum-mass compatible velocity at time
zero and ends at `0.0001`. Zero homogeneous coordinates therefore do not mean
zero full velocity. The private RK4 helper is shared with the polynomial ODE
reference; the physical drift retains the original lifting derivative.

The [unaltered campaign receipt](data/2026-10-06-cylinder-p2-evolution/receipt.json)
binds each raw output and empty stderr stream. All runs passed the typed report
validator and completed under the same 512 MiB address-space cap, 180-second
deadline and 4 MiB captured-file limit. Their conservative aggregate work
allowance was 2 billion and managed storage allowance 256 MiB. The public
default and all component limits remain unchanged.

| Steps | Conservative cumulative work | Original momentum residual | Original continuity residual | Final quadratic-action norm |
| --- | ---: | ---: | ---: | ---: |
| 2 | 1,733,765,504 | `1.591e-14` | `1.458e-15` | `0.00631169` |
| 4 | 1,804,827,776 | `1.069e-14` | `1.430e-15` | `0.00892359` |
| 8 | 1,946,952,320 | `9.326e-15` | `1.451e-15` | `0.00894300` |

The modeled managed peak was 86,413,302 bytes in each run. Process RSS was
**not measured**; neither that model nor the enforced address-space ceiling is
a measured memory peak. Parent-observed elapsed time was approximately 0.515
seconds per child, including process supervision. These are execution records,
not performance comparisons.

The fixed sensitivity threshold is `1e-5*max(1,|fine|)` for scalar observables
and the corresponding Euclidean norm convention for the complete state.
Differences must also be nonincreasing, with a declared roundoff floor.

| Quantity | Scaled difference between four and eight steps | Meets fixed sensitivity threshold |
| --- | ---: | --- |
| Complete coordinate state | `3.15689e-5` | No |
| Drag coefficient | `2.20290e-3` | No |
| Lift coefficient | `4.93974e-4` | No |
| Pressure difference | `1.17814e-3` | No |
| Mean kinetic energy | `4.00881e-7` | Yes |
| Enstrophy | `8.34004e-6` | Yes |

The reported large coarse/fine difference ratios are empirical. They do not
establish the asymptotic order of this stiff physical experiment. No extra
physical run, tolerance change or cap increase followed the failed criterion.
All initial and accepted-step energies, final coordinates and original pressure
coefficients remain in the raw outputs.

## Source, executable and independent checks

The [build attestation](data/2026-10-06-cylinder-p2-evolution/build-attestation.json)
records the actual twelve local package compiler-artifact closure for the
default example. Its [504-file source manifest](data/2026-10-06-cylinder-p2-evolution/build-source.json)
was unchanged before and after the build and after the campaign. The executable
SHA-256 was
`c9baa91ec2e9ed36272a9d71b61886d5a6b4735cddf891d1b31e256f97a38fb4`
before and after execution. Unused native `quest` and `quest-sys` source was
excluded from this default-feature closure. Root manifests/configuration and
the relevant QSP/QSVT/MathCore dependencies were included.

This scoped record excludes external registry contents, environment and
non-Rust build assets; it is not reproducible-build certification. Two earlier
attestation attempts detected concurrent source changes and were rejected
before physical execution. Their changed paths and before/after hashes are
preserved in [publication provenance](data/2026-10-06-cylinder-p2-evolution/publication-provenance.json).
Private Cargo JSONL records contain host paths and are represented by their
unchanged hashes, rather than published verbatim.

Independent review passed 19 scoped Rust tests, five runner tests and strict
Clippy. The tests cover independent analytic RK4 order and time dependence,
zero-step compatibility, callback and observer failures, finite accepted-state
retention, original lifting derivatives, whole-interval overflow, cumulative
admission and vector capacity. Validator regressions reject malformed numbers,
duplicate keys, excessive nesting and incorrectly typed physical/resource
fields; a genuine serialized pressure receipt anchors the positive schema.

The [reviewed source manifest](data/2026-10-06-cylinder-p2-evolution/reviewed-source.json)
and [publication record](data/2026-10-06-cylinder-p2-evolution/publication.json)
retain the focused identity and private verification-log hashes. The reviewer
validated all three saved genuine reports and their stream hashes and
recomputed the comparison. Root independently recomputed all six differences,
work charges and callback counts. Neither check reran physical children.

## Reproduction and remaining acceptance

```sh
cargo build -p quest-cfd --example cylinder_p2_evolution
python3 -B crates/quest-cfd/examples/cylinder_p2_evolution.py \
  target/debug/examples/cylinder_p2_evolution new-evolution-results
python3 -B crates/quest-cfd/tests/test_cylinder_evolution_runner.py
```

Pass the actual executable path if the workspace uses a different Cargo target
layout. The runner requires a new output directory and retains rejected or
failed rows. Source-to-binary attestation is separate from its binary hash.

This establishes a bounded full-coordinate evolution consumer and an honest
failed temporal study. It does not establish forward accuracy, a resolved
full-minus-linear trajectory, spatial or geometry convergence, a developed
shedding cycle, or any quantum execution. In particular, the official DFG
measurement cycle in `[25,30]` remains unexecuted by this P2 consumer. The
polygonal geometry and natural-outlet backflow behavior remain independent
physical limitations.
