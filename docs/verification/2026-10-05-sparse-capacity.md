# Capped local sparse pipeline experiment

This is an actual small, single-host MPI experiment from source-owned coefficient generation through preprocessing, immutable publication, reload, and repeatedly applied native matching. It is not multi-host acceptance, a large-problem capacity certificate, or an optimized-build performance comparison.

The Rust example is `crates/quest/examples/sparse_capacity.rs`; the bounded Python driver and nine focused tests are in `docs/verification/fixtures/sparse-capacity/`. The example requires Linux `/proc`, MPI support in native QuEST, and Cargo features `qsvt-io,mpi`.

## Reproduce

Use a QuEST installation and MPI compiler/launcher with matching MPI ABI. The paths below are placeholders.

```bash
export QUEST_ROOT=/path/to/quest-install
export MPICC=/path/to/matching/mpicc
cargo build -p quest-rs --features qsvt-io,mpi --example sparse_capacity
python3 docs/verification/fixtures/sparse-capacity/test_run.py
python3 docs/verification/fixtures/sparse-capacity/run.py \
  --executable target/debug/examples/sparse_capacity \
  --mpiexec /path/to/matching/mpiexec \
  --output /tmp/sparse-capacity-new-run
```

The output directory must be new. Default configurations are `64:1,256:2,1024:4,64:8` (matrix dimension:ranks), three controlled forward/adjoint pairs, 16 MiB modeled application budget per rank, 128 MiB modeled application budget for the local node, and 2048 MiB actual address-space cap per rank. All ranks run on the same host in this experiment. Up to eight cases may be supplied. Configure cases, repetitions, budgets, timeout and process cap explicitly when changing this scope; dimensions are bounded to powers of two from 16 to 4096, rank counts to 1/2/4/8, repetitions to 1..8 and timeout to at most 600 seconds. These CLI bounds do not promise admission for every combination: producer stream/work, persistence/replay and native capacity guards remain active.

The driver uses MPICH Hydra `-hosts localhost` to keep launcher placement local. Other launcher command syntaxes are outside this fixture. The Python wrapper sets equal hard and soft Linux `RLIMIT_AS` before `exec` of each rank executable. The executable verifies `/proc/self/limits`. This caps process **address space**, including shared-library mappings and virtual reservations; it is not an RSS limit or a cgroup physical-node limit. The MPI launcher itself is outside the rank cap. Timeout signals the local launcher process group and records failure; an independent local MPI probe confirmed child ranks stop after signal delivery settles. `completion.json` is complete only after every expected rank supplies a valid receipt. Publication resources and receipts remain in each case directory for inspection; failed cases can retain unpublished files. No files are silently replaced.

The runner accepts the exact known native receipt/stage schema, including every memory observation and exactly one timing per repeated forward/adjoint pair. Rank JSON is limited to one MiB and rejects duplicate keys and nonstandard numeric constants. Integer fields reject booleans/floats and out-of-range values; errors and times must be finite and nonnegative. Counts bind two matching colors, the matching flag and outer control to exactly `8*dimension` native amplitudes, with exact source/rank partition coverage. Aggregated routing sent/received counters must agree. The fixture source alternates signed controls; its existing receipt format does not separately attest control masks or an independent operator certificate.

## Ownership and validation

Rank k directly strides columns k, k+P, ... and emits two coefficients per column with unique global ordinals. The sparse family has diagonal `0.7+0.1i` and off-diagonal `-0.2+0.05i` at row `column XOR 1`. Short permutation cycles keep portable restart admission bounded. No participant constructs a dense A, U, complete gate stream, or classical copy of the native state. In the one-rank baseline the sole source shard naturally owns all sparse coefficients.

The producer owns only local edges and endpoint records; publication streams logical buckets; reload retains source-owned records and destination-owned inverse records. A local matching-column snapshot is created under the simultaneous loaded-resource/clone policy, then the loaded owner is dropped. The producer is dropped before reload. Native preparation owns the snapshot, a native partition scratch register, and bounded routing packets. The caller register is also forced to a native local partition, with `local_amplitudes * ranks == global_amplitudes` checked. Modeled budgets and node placement are admitted before native preparation; the node envelope uses maximum rank allowance times local rank count, conservatively covering every stage's configured application storage allowance.

Each run initializes a distributed plus state, executes repeated whole-unitary forward/adjoint pairs with alternating negative and positive outer controls, then checks collective probability and at most eight local samples. These checks detect roundtrip drift without reading a whole partition. Existing matching differential tests establish the operator itself; a roundtrip check alone does not independently certify the sparse block, spectral behavior or solution accuracy.

## Counters and limits

Stage durations include the closing participant agreement, and cover preprocessing, publication, load plus local snapshot, native environment/register/preparation plus initialization, and repeated execution. Receipt creation and semantic validation are outside the execution timing. Per-pair local timings are also recorded. Debug timings are observations, not optimized throughput estimates.

Each stage records endpoint RSS and virtual size, plus cumulative process-lifetime RSS and virtual high-water marks from `/proc/self/status`. The final receipt reports the same process counters, baseline after MPI initialization, the verified process cap, native live reservations, producer managed peak upper bound and loaded retained-resource bound. Managed accounting excludes allocator overhead, MPI and native-library internals, OS pages, driver memory and benchmark observation/receipt overhead; those remain inside the real rank process cap. `model_peak_bytes` is the conservative configured admitted stage envelope, explicitly labeled as such, not a measured allocation peak.

The driver reports the **sum of per-rank RSS high-water marks as an upper bound**, not simultaneous node RSS. Shared pages can be counted multiple times. There is no measured physical-node peak or enforced physical-node RSS cap here.

Communication reporting has deliberate coverage limits:

- Producer `sent_bytes` measures application request/reply payload; it excludes count handshakes and collective metadata.
- Native execution routing sent/received bytes include its packet/count protocol; coordination call counts are reported separately. Native Hadamard/clone communication, MPI wire overhead and separate collectives are excluded.
- Publication and load traffic are **not instrumented**. The persisted recipe's single-replay communication upper bound is also reported, explicitly as modeled future replay admission, not observed load traffic. No total pipeline/network byte claim is made.

## Recorded local evidence

[Public numeric receipts](data/2026-10-05-sparse-capacity/results.json) preserve the original staged successes, tighter-cap success and insufficient-cap failure, together with strict independent revalidation and a new capped two-rank review run. The original runs did not record binary/build-tree hashes; these remain explicitly unavailable. The new review run records genuine binary and fixture-source hashes observed unchanged before/after execution, without asserting a dependency/build-tree attestation. The native build used matching system MPICH and installed QuEST 4.3.0, with no native checkout changes. All runs used the Cargo debug profile.

| Matrix dimension | Ranks | Native global amplitudes | Max preprocessing s | Max persistence s | Max load s | Max prepare s | Max repeated execution s |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 64 | 1 | 512 | 0.00255 | 0.00662 | 0.00544 | 0.00037 | 0.00213 |
| 256 | 2 | 2048 | 0.00425 | 0.00522 | 0.03065 | 0.00070 | 0.01100 |
| 1024 | 4 | 8192 | 0.00941 | 0.00544 | 0.22151 | 0.00241 | 0.04152 |
| 64 | 8 | 512 | 0.00291 | 0.00407 | 0.04204 | 0.00089 | 0.00409 |

These four cases passed under 2048 MiB process caps. Max rank RSS was approximately 293.34/296.28/299.78/301.69 MiB; max rank virtual high-water was 1031.55/1034.69/1038.30/1041.17 MiB. Summed rank RSS high-water bounds were approximately 293.34/592.29/1197.75/2410.67 MiB. Those process figures include the native installation's linked dependencies and differ from the application memory model.

Measured aggregate producer payload bytes were 0/409600/1638400/102400, and aggregate repeated-execution routing sent bytes were 0/433152/2654208/231168, with matching received totals. Across all cases sample error was at most 2.78e-17 and probability error at most 2.23e-16.

An additional 1024/4 run passed under a tighter **1152 MiB** hard/soft process cap with the same modeled application budgets. An intentionally insufficient **128 MiB** cap rejected native startup: the loader could not map `libcublas.so.13`, MPI exited 127, and completion remained false with no successful rank receipts. This is cap failure evidence for the installed linked native dependencies, not producer/native-algorithm admission success. An initial pilot also rejected an incorrectly overlapping stream/transport model before register creation; the corrected driver gives stream storage half the producer rank envelope.

Independent review reproduced five malformed receipts accepted by the original validator: missing stage memory observations, missing repetition timings, NaN routing counts, zero-sized native layout, and negative sample error. New behavioral regressions now reject each case. Nine focused tests pass, including exact schema/type checks, bounded strict JSON and preservation of an incomplete completion artifact after malformed rank output. All five original successful cases pass the strengthened validator. A fresh 64/2 run with two controlled forward/adjoint pairs also passes under 2048 MiB real process caps and the same 16/128 MiB modeled rank/node budgets. It records 102400 producer payload bytes and 72192 sent/received routing bytes, with approximately 0.00174 seconds native execution and 0.570 seconds launcher time. These are local debug observations, not throughput or multihost evidence.

Multi-host transport, large physical CFD/Carleman workloads, external-sort campaigns, independent wire telemetry, optimized timings, simultaneous physical-node RSS and GPU execution are outside this experiment. Small successful shards and bounded counts must not be extrapolated to huge execution capacity.
