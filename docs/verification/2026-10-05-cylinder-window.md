# Bounded classical cylinder force-window execution

This runnable example advances the complete bounded DFG 2D-2 BDM1/P0 reference
continuously with RK4, then measures a compact force and pressure trace. It is a
classical local reference execution, not a quantum execution or a converged
benchmark. The frozen manifest specifies integration `[0,8]` and observation
`[4,8]`. A changed contained window requires `--short-window` and remains a
diagnostic override.

From the repository root on Linux:

```sh
cargo build -p quest-cfd --example cylinder_window
cargo test -p quest-cfd --test cylinder_window_example
cargo clippy -p quest-cfd --example cylinder_window --test cylinder_window_example --no-deps -- -D warnings
python3 docs/verification/fixtures/cylinder-window/test_run.py
python3 docs/verification/fixtures/cylinder-window/run.py \
  --binary target/debug/examples/cylinder_window \
  --output /tmp/cylinder-full-window.json \
  --process-as-mib 512 --timeout 120 \
  -- --max-work 1000000000000
```

The output path must be new. The runner applies Linux hard/soft `RLIMIT_AS` to
the example process, records its reported RSS and address-space high-water
marks, and kills its process group at the timeout. The Python observer is outside
that cap. `RLIMIT_AS` limits address space, not node memory or RSS. Managed
reference/trace budgets are separate from the process cap and allocator overhead.
The runner spools output to temporary files with live size checks and a per-file
64 MiB `RLIMIT_FSIZE` backstop. Completion rejects stdout of 64 MiB or more and
stderr above 4 MiB; only the last 64 KiB of stderr is retained, with truncation
labelled. Thus an otherwise admitted very large trace can still be rejected by
the explicit receipt-size limit. JSON depth is capped at 16; duplicate object
keys, nonfinite constants and overflowing decimal exponents are rejected before
retaining the parsed native receipt.
The checked-in receipts record debug binaries; their timings are local evidence,
not release-performance predictions.

Default parameters are Reynolds 100, four angular sectors, one radial layer,
`dt=1e-4`, 80,000 steps, stride 100, work budget `1e12`, managed-byte budget
64 MiB, trace-byte budget 64 KiB, and maximum admitted residual `1e-6`.
All full physical chart coordinates remain present. The example stores one live
state and RK4 scratch, one pressure observation's scratch, and scalar force
samples; it does not retain a trajectory of state vectors or restart the initial
condition for each sample. Both observation endpoints are included, with a final
shorter sampling interval when necessary. Excessive or nonfinite residuals fail
the run instead of silently dropping a sample.

Force coefficients use full pressure plus unsymmetrized viscous traction,
`Cd/Cl=2F/(rho*U_mean²*D)`, with `rho=1`, `U_mean=1`, and `D=0.1`.
Pressure difference is front minus back at `(0.15,0.2)` and `(0.25,0.2)`.
Each probe uses the arithmetic mean of incident fluid P0 traces, with the
natural-outflow pressure level. This explicitly records the convention where a
probe lies on a face. Geometry error is the maximum deviation of a polygonal
cylinder edge from the circular radius.

Statistics are time-weighted trapezoidal means, lift RMS and lift standard
deviation. A Strouhal candidate requires at least two complete periods between
upward crossings of the window-mean lift, at least eight samples per shortest period, relative
period variation at most 0.1, and lift amplitude above `1e-10`. An admitted
crossing rate would still not certify periodicity, absence of aliasing, or
benchmark convergence. The example always reports `convergence_certified=false`
and `quantum_execution=false`.

## Local full-window evidence

The [historical full-window receipt](data/2026-10-05-cylinder-window/full8-coarse.json)
completed all 80,000 steps and 401 observations in 84.385 seconds under an actual
512 MiB address-space cap and 120-second timeout. It retained 20 independent
physical coordinates, 96 broken velocity coefficients and constraint rank 76.

| Observation | Measured value |
| --- | ---: |
| Mean Cd | 6.771790840326831 |
| Mean Cl | -6.028855977654911 |
| Lift RMS | 6.0288559776549135 |
| Lift standard deviation | 4.061082885875831e-9 |
| Mean pressure difference | 7.504546233419452 |
| Strouhal candidate | unavailable: insufficient complete upward-crossing periods |
| Maximum polygon geometry deviation | 0.012927373322406078 m |
| RSS high-water mark | 5,419,008 bytes |
| Address-space high-water mark | 7,421,952 bytes |
| Managed peak envelope | 12,857,888 bytes |
| Force trace storage | 12,832 bytes |
| Aggregate modeled work | 681,978,153,088 elementary operations |

This eight-segment cylinder has radial deviation approximately 25.9% of the
cylinder radius. Its nearly constant, asymmetric lift and unavailable frequency
are retained as observed outcomes. The receipt is not accepted DFG force,
pressure, shedding, geometry-refinement or temporal-convergence evidence.

The work model admits the entire construction, integration, pressure sampling
and analysis before advancing the state. For the conservative broken-velocity
bound `N=12*(angular_sectors+4)*radial_layers` and sample count `S`, it charges:

```text
drift       = 128*N² + 8192*N
construction= 256*N³
integration = steps*(4*drift + 32*N²)
observations= S*(64*N³ + 8*drift)
analysis    = 128*S
managed peak= 256*N² + 65536*N + 4 MiB + actual trace capacity bytes
```

These are conservative logical operation/storage envelopes, not measured CPU
instruction counts. The bounded dense reference rejects `N>768`; it makes no
large-mesh scalability claim. Pressure scratch is discarded after each sample.

## Short runs, failure evidence and receipt admission

The [fresh short receipt](data/2026-10-05-cylinder-window/tiny-final.json) uses
five steps of `1e-6` with four samples, including the final stride remainder:

```sh
python3 docs/verification/fixtures/cylinder-window/run.py \
  --binary target/debug/examples/cylinder_window \
  --output /tmp/cylinder-short-window.json \
  -- --short-window --dt 0.000001 --steps 5 --stride 2 \
     --observation-start 0 --observation-end 0.000005
```

The Rust regression compares its final force and pressure with an independent
continuous RK4 reference over the same complete state. Other regressions reject
invalid windows/time, inadequate aggregate work/storage and excessive residuals.
The Python tests revalidate real short and historical full receipts and reject
24 semantic mutations plus malformed, nonfinite, duplicate-key, excessive-depth
and oversized subprocess output. Five Python tests cover those boundaries.
Completion requires a
known schema, matching requested parameters, finite admitted values, all final
steps, the exact sampling schedule, consistent work/storage envelopes, pressure
residuals within policy, and trace-consistent statistics/crossing policy. Exit
code zero alone cannot establish completion. Stderr progress precedes the current
sample append and can lag the authoritative final receipt by one observation.

An intentional [one-second timeout](data/2026-10-05-cylinder-window/timeout-final.json)
retains its last reported partial progress and `completed=false`.
A [work-budget rejection](data/2026-10-05-cylinder-window/budget-rejected-final.json)
records rejection before integration. Neither is a successful force window.

The [startup coarse-step](data/2026-10-05-cylinder-window/startup-dt-coarse.json)
and [startup half-step](data/2026-10-05-cylinder-window/startup-dt-fine.json)
receipts cover only `[0,0.001]`, with the same six sample times. Mean Cd changes
from 26.13581544466537 to 26.11245617929956; mean pressure difference changes
from 37.85993870690947 to 37.8279790383305. These are startup sensitivity checks,
not a refinement study for the frozen observation window.

Every receipt preserves its tested binary SHA-256 and independently hashed
selected sources, manifest and lockfile. This identifies evidence snapshots; it
does not attest that a prebuilt binary was compiled from those source hashes.
The historical eight-second and startup runs preceded the final trace-capacity
guard and later source validation/probe-budget refinements. Their identities
remain unchanged. The fresh tiny execution and current regressions verify the
updated guards; the eight-second calculation was not repeated solely for those
validation changes. Historical numerical receipts were also checked by the
strict completion validator without claiming a new execution.
