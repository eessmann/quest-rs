# Fixed initial weak-generator protocol

This example consumes the unchanged [initial diagnostic](configuration-weak.md)
for the complete five-coordinate periodic BDM1/P0 source. It does not solve a
history system, evolve a trajectory, execute a quantum circuit, or establish
continuum convergence. The seven-row campaign requires its own pinned execution;
three **executed focused serializer fixtures** are maintained separately below.

The source has viscosity 0.01. All rows use time zero, center
`[0.15, -0.1, 0.07, 0.11, -0.04]`, the original compact bump producer and at least
two distinct support nodes on **each** coordinate. Duplicate DG endpoint
coefficients retain their quadrature weights and amplitudes but cannot increase
the distinct-node count. A sampling rejection includes independently counted
actual nodes even though the unchanged policy returns an error before its counts.

| Fixed row ID | Configuration order/cells | Extent | Width | Full coefficients | Pre-execution expectation |
| --- | --- | ---: | ---: | ---: | --- |
| `p1-c3-e1-w1_2` | DG1 / 3 | 1 | 0.5 | 7,776 | May complete |
| `p1-c4-e1-w1_2` | DG1 / 4 | 1 | 0.5 | 32,768 | May complete |
| `p1-c5-e1-w1_2` | DG1 / 5 | 1 | 0.5 | 100,000 | Work rejection |
| `p2-c1-e1-w1_2` | DG2 / 1 | 1 | 0.5 | 243 | Sampling rejection |
| `p2-c3-e1-w1_2` | DG2 / 3 | 1 | 0.5 | 59,049 | Work rejection |
| `p1-c5-e5_3-w1_2` | DG1 / 5 | 5/3 | 0.5 | 100,000 | Work rejection |
| `p1-c3-e1-w3_5` | DG1 / 3 | 1 | 0.6 | 7,776 | May complete |

Expectations do not replace actual outcomes. No values, caps or policies change
after a rejection. The last row measures width sensitivity; broadening 0.5 to 0.6
is not a delta-limit refinement. Nominal fixed spacing in the domain pair differs
in binary64: `2/3` versus `(2*(5/3))/5`. Actual represented node/weight bits,
derivative entries, bounds, spacing and ensemble digests preserve that difference.

## Admission and identities

The fixed managed limits are 256 MiB, one billion source/query work units,
100 billion original physical work units and one million original physical calls.
Before grid or state allocation, the example admits

```text
n = cells * (order + 1), N = n^5 <= 100000
W_input = 1048576 + 16384*n + 4096*N
```

`W_input` is inside the original one-billion ceiling, not a new phase allowance.
The input ledger includes the unchanged bump and normalization, two existing
coordinate-moment traversals (one inside concentration), concentration/support
scans and streamed SHA-256. It passes `Some(W_input)` to the diagnostic after
admitting producer work itself. The diagnostic's constructor and row work remain
additional parts of the same per-request total. This arithmetic model is not a
CPU-instruction or allocator-runtime bound.

Before allocation, 64 KiB bounds the small axis/metadata phase; a separate 64 KiB
helper envelope covers fixed reports, hash contexts and small moment/point
scratch; another 64 KiB is reserved for serialization. Source construction uses
the diagnostic's 8 MiB envelope plus declared live producer owners. State
construction admits full `16*N` payload and the retained source before calling the
original producer. Returned grid and backing state capacities, four axis vectors,
SHA strings and support/statistic vector capacities are checked. Excess state
capacity is declared to the borrowed-state diagnostic. Point and temporary helper
payloads use the fixed audited envelope; allocator metadata, library/runtime and
host configuration are outside this managed-payload model.

The source external reservation retains producer grid/metadata/helper/serializer
storage conservatively. The diagnostic also charges the borrowed grid; this is a
safe repeated charge, not a claim of exact physical peak. Support/statistic results
are copied into fixed arrays; their measured capacities remain visible even after
the small vectors are dropped. `input_resources.peak_bytes` retains the largest
attempted/admitted phase, while the module receipt retains its own phase and
attempted original drift count. A work rejection after input construction preserves
the full ensemble digest and initial diagnostics, without a rate result.

Serde writes into a fixed mutable 64 KiB slice through `Cursor`; it never grows a
JSON buffer. A serializer error yields a failed process, whose raw bytes the runner
retains. Field names/schema are bounded, and no full state is serialized. Root
Cargo/configuration and the actual local compiler-artifact closure must be pinned
before/after build and execution, along with executable and runner/helper SHA-256.
The runner consumes that pinned executable; it does **not** establish build
provenance itself. Registry libraries/environment are outside the stated local
source scope. A source digest is provenance, not numerical correctness evidence.

The row schema is `quest-configuration-weak-row-v1`; its request includes the ID.
Top-level `plan`, `input_resources`, `sampling`, `represented_grid`, `initial`,
`source`, `diagnostic_resources`, progress counters and `diagnostic` distinguish
unexecuted/null phases from zero values. Versioned, length-framed little-endian
SHA-256 streams bind physical chart/viscosity, represented grid/derivative rows
including duplicates, and every normalized complex amplitude. Full state hashes
are streamed, without a state copy or full drift/generator table. Floating signed
zero bits remain part of provenance; physical node equality follows the original
policy.

## What comparisons mean

For the compact half-density
`psi = C exp(-sum_j 1/(1-((a_j-c_j)/width)^2))` inside support, zero outside,
probability is proportional to `|psi|^2`. Strict interior support and symmetry give
continuous initial means exactly `c_j`. Each row reports sampled mean minus center
as initial quadrature/sampling bias. It does not infer continuum variance or energy.
The effective coefficient count and maximum probability include duplicated DG
coefficients; they are not distinct spatial resolution measures. Minimum-two
admission alone does not establish concentration resolution.

Only pairs with two validated completed rows and matching physical source identities
receive rates. The predetermined pairs are cells 3/4, cells 4/5, the intended fixed
spacing domain pair, and width 0.5/0.6. Let `G` be normalized discrete weak rate,
`P` the independently sampled original physical rate, and `E=G-P`. Each pair reports
`Delta G`, `Delta P`, `Delta E`, signed defects and the decomposition residual.
Changing `P` changes the sampled reference ensemble; `Delta G` alone is not a pure
generator error. Coordinate-vector and integrated-energy scales remain separate.

A reference rate is numerically resolved only above the predetermined
`1e-10*max(1, initial-moment magnitude)` scale. Defect ratios also require the
baseline defect above `1e-12*max(1,|P_left|,|P_right|)`. Conserved coordinates may
have unresolved individual rates. Nonlinear mean-square action has a separate
resolution flag and does not prove trajectory accuracy. No comparison reports a
convergence order or fitted success tolerance. Sampled means and identities remain
available for a pair whose later diagnostic rejects; its rate comparison is absent.

## Reproduction and validation

```sh
cargo test -p quest-cfd --example configuration_weak
python3 -B docs/verification/fixtures/quest-cfd/test_configuration_weak.py
cargo clippy -p quest-cfd --example configuration_weak --no-deps -- -D warnings
```

After separately verifying a stable build and authorizing the campaign, invoke the
strict runner with a pinned binary and a new directory:

```sh
python3 -B docs/verification/fixtures/quest-cfd/configuration_weak.py \
  /path/to/pinned/configuration_weak /path/to/new-results
```

It runs exactly seven serial children, without retry: 512 MiB address space,
180 seconds per child and 4 MiB per captured file. Each outcome preserves raw
stdout/stderr, hashes, elapsed time and available `/usr/bin/time` RSS. Malformed, negative or unavailable timing is null; raw timing file identity and
SHA-256 remain in the receipt. Timeout kills the process group. The supervising Python process is outside
the child limits. Incremental finite receipts use temporary-file replacement (no fsync durability
claim). Typed construction prefixes and partial numerical failures are validated
against their phase-specific owners, callbacks and progress; they never enter rate
comparisons. Malformed or unsupported payloads remain raw failed rows.

The strict parser reuses the common bounded JSON decoder and rejects duplicate
keys, nonfinite/deep structures, bool-as-number, unknown/missing fields, malformed
nested payloads and inconsistent whole-live receipts. Fake children test timeout,
output caps and continuation through all seven malformed outcomes. Genuine
[focused serializer fixtures](../../../docs/verification/fixtures/quest-cfd/configuration_weak_fixtures/)
cover completed, sampling-rejected and work-rejected Rust outputs, plus a separate
unit-test-only injected grid failure through the real Rust wrapper/serializer. The
failure test does not construct a physical source. Physical-fixture provenance
records exactly three executions, a stable 507-file local source scope and one
pinned binary; they are not the seven-row campaign. Subsequent fixed-writer helper
extraction, failure-phase classification and unit/parser tests do not change the
fixture's historical build identity. A later campaign needs a fresh reviewed pin, not a claim that those
historical bytes came from a newer source snapshot.
