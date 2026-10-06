# Cirrus original-input capacity and scaling evidence

Thirteen cases completed: six GNU scaling points, one larger GNU point, three
Cray fixed-size points and three Cray weak-scaling points on two, four and eight
exclusive Cirrus nodes. Every run used one MPI process per node and 288 requested
OpenMP threads per process. All thirteen
passed execution and receipt validation. The oversized admission probe rejected
before creating input; strict capacity remains open. Execution and
capacity certification are separate outcomes: the canonical original input must
be strictly larger than every participating node's enforced rank-process address
space envelope before this harness can report capacity closure.

## Identity and execution profile

The GNU runs use immutable source digest
`6a6b4ee4a69aace3da90eafa8bc2bf0644e88eb21af5fef778b6b0b515bbc3c5`,
GNU build job `536644`, and its matching native/build profile. They reuse the
successfully built debug `sparse_capacity` executable. These timings are observations
of that debug configuration, not optimized production performance measurements.
The recorded profile uses GCC 14.2.1, Cray MPICH 8.1.32, centrally maintained
`cray-hdf5/1.14.3.5`, Rust nightly `282215592`, and native QuEST revision
`503552065045eaf89baba85e6cd6aad728525554`, with no Rust compiler flags added.
The executable SHA-256 is
`dc03b1449ca08c01bf8686e45b55e4416d8ad9c76a68c2e8830e97061b935e19`.
The broader compiler and workspace verification is recorded in
[the portability report](2026-10-06-portable-cirrus.md).

Jobs `536732`, `536733` and `536734` request two, four and eight nodes respectively,
with `--exclusive --ntasks-per-node=1 --cpus-per-task=288 --hint=nomultithread
--distribution=block:block`, the low-priority quality of service and no explicit
Slurm memory request. Their working directory is the shared campaign filesystem.
They run sequentially through `afterany` dependencies; the eight-node job also
waits for the separate one-node diagnostic `536731`. A failed predecessor cannot
turn an unrun scaling point into a pass.

Earlier jobs `536701`–`536703` all failed with exit 127 before MPI launch or
receipt creation: their non-login SSH submission environment had no `module`
function. Those failures remain in the evidence. The reruns use the named
`capacity-login-adapter.sh` diagnostic adapter, SHA-256
`ef8bc0893b9cf1e3ed33d5e6651cb9709db53644bad5e6bb1fb690ad04291e41`,
which initializes a login shell and executes the unchanged snapshot coordinator.
The immutable source and native executable are not patched by this adapter.
The maintained coordinator now requests a login shell directly, with a failing
then passing interpreter regression and all six coordinator fixture tests passing.

The runner enforces an 8 GiB hard and soft `RLIMIT_AS` for each participating MPI
process. Since there is one rank per verified shared-memory node, the enforced
rank-process envelope is also 8 GiB per node. The managed application allowance
is 4 GiB/rank, and the additional 287 OpenMP worker stacks are admitted separately
at 8 MiB each (2,407,530,496 bytes). MPI/library baseline address space plus both
allowances must fit before source generation. This is a cap on participating rank
processes, not a physical-node cgroup cap: launcher/coordinator processes,
filesystem cache and unrelated node processes are outside it.

`OMP_NUM_THREADS=288`, `OMP_PLACES=cores`, `OMP_PROC_BIND=close`,
`OMP_DYNAMIC=FALSE`, an explicit 8 MiB `OMP_STACKSIZE`, and
`SRUN_CPUS_PER_TASK=SLURM_CPUS_PER_TASK` accompany each step. The executable checks
native environment and register multithreading flags. Process thread counts
include MPI and sampling threads; the exact OpenMP team size is uninstrumented.
Native QuEST initialization and other eligible native operations can use OpenMP.
Source generation, sparse preprocessing/loading, matching pair arithmetic and MPI
routing remain serial. These runs therefore do not establish parallel sparse-kernel
speedup from the requested thread count.

## Workload and acceptance

Every rank creates only its owned source columns. Each column has a diagonal
coefficient `0.7 + 0.1i` and a coefficient `-0.2 + 0.05i` at row `column XOR 1`.
Each original nonzero occupies exactly 32 uncompressed bytes: two `u64` indices
and two `f64` coefficient components. There are `2N` nonzeros and exactly `64N`
canonical input bytes globally. No dense matrix or global source vector is built.
Input file lengths, allocated blocks and SHA-256 hashes are checked against the
rank receipts. HDF5 encoding size is reported separately and never counted toward
original-input capacity.

Both compiler profiles hold `N=65,536` constant for fixed-size scaling. Weak
scaling holds 8,192 columns per rank: `N=16,384`, `32,768`, `65,536` on two, four
and eight nodes. The GNU two-node
campaign additionally executes `N=32,768` through geometric doubling. Each point
runs three controlled forward/adjoint roundtrips, alternating the control value,
then validates total norm and bounded local amplitude samples. Original source
storage, preprocessing, persistence, load, preparation and execution are timed
separately. Execution routing and producer payload counters are measured;
persistence/load wire bytes remain uninstrumented.

The MPI shared-memory communicator establishes each rank's actual node group,
and distinct consistent processor names establish multi-host coverage. A node
leader samples exactly its group members' `/proc` counters every 20 ms. Maximum
sampled sums of RSS/address space and sums of rank RSS high-water marks are
reported separately; neither is presented as a unique physical-page peak.

## GNU scaling evidence

All six completed cases were independently revalidated locally against the copied
full original-input files, SHA-256 hashes, v3 rank schemas, threading configuration,
node groups, caps, numerical results and wrapper completion markers. An independent
read-only audit also decoded every copied COO32 record and checked the HDF5 file
lengths and summarized measurements. Every rank reported norm
`0.9999999999999999`; the largest sampled roundtrip error was
`1.735e-18`. Both native multithreading flags were true. Every rank reported two
process threads before native preparation, 289 after preparation, and 288 after
the sampler was joined; these counts do not independently measure OpenMP team size.

| Job | Nodes / MPI ranks | Completed dimensions | Actual MPI processor names |
| --- | ---: | --- | --- |
| 536732 | 2 / 2 | 16,384; 32,768; 65,536 | `cs-n0434`, `cs-n0435` |
| 536733 | 4 / 4 | 32,768; 65,536 | `cs-n0434` through `cs-n0437` |
| 536734 | 8 / 8 | 65,536 | `cs-n0434` through `cs-n0439`, `cs-n0446`, `cs-n0447` |

Each host was a distinct MPI shared-memory group with exactly one member and its
own verified 8 GiB process cap. The rank receipt includes its group leader and
local rank, rather than deriving node coverage from the requested Slurm count.

| Nodes | N | Original nonzeros | Original bytes | Persisted HDF5 encoding bytes |
| ---: | ---: | ---: | ---: | ---: |
| 2 | 16,384 | 32,768 | 1,048,576 | 2,635,056 |
| 2 | 32,768 | 65,536 | 2,097,152 | 5,256,496 |
| 2 | 65,536 | 131,072 | 4,194,304 | 10,499,376 |
| 4 | 32,768 | 65,536 | 2,097,152 | 5,270,112 |
| 4 | 65,536 | 131,072 | 4,194,304 | 10,512,992 |
| 8 | 65,536 | 131,072 | 4,194,304 | 10,540,224 |

The source-storage maxima were 0.023–0.045 seconds. The following stage durations
are the maximum reported rank duration in seconds; execution includes all three
controlled forward/adjoint roundtrips.

| Nodes | N | Preprocess | Persist | Load | Prepare | Execute |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 16,384 | 0.439 | 0.102 | 6.321 | 0.150 | 1.074 |
| 2 | 32,768 | 0.835 | 0.107 | 12.578 | 0.316 | 2.144 |
| 2 | 65,536 | 1.683 | 0.192 | 25.238 | 0.488 | 4.421 |
| 4 | 32,768 | 0.622 | 0.120 | 29.846 | 0.229 | 2.108 |
| 4 | 65,536 | 1.138 | 0.111 | 59.643 | 0.416 | 4.260 |
| 8 | 65,536 | 0.947 | 0.123 | 142.492 | 0.534 | 5.313 |

For each repeated roundtrip, the maximum time across ranks is taken first; the
table below gives the median of those three maxima. Launcher time covers the
whole bounded MPI invocation, including setup, storage, all stages and validation.

| Nodes | Strong N | Strong median roundtrip (s) | Strong launcher (s) | Weak N | Weak median roundtrip (s) | Weak launcher (s) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 65,536 | 1.473 | 32.479 | 16,384 | 0.360 | 10.008 |
| 4 | 65,536 | 1.421 | 66.500 | 32,768 | 0.704 | 35.139 |
| 8 | 65,536 | 1.774 | 152.139 | 65,536 | 1.774 | 152.139 |

These observations do not demonstrate end-to-end strong scaling: the fixed-size
launcher duration increased from 32.48 to 152.14 seconds. Preprocessing became
faster, while loading became substantially slower; prepared roundtrip latency
stayed between 1.42 and 1.77 seconds. Weak-scaling roundtrip latency also increased.
The load wire traffic is uninstrumented, so stage timing alone does not identify
a communication or computation cause. This is one allocation per node count and
three repetitions within each case, not an independent-run performance study.

At fixed `N=65,536`, total producer payload was 104,857,600 bytes on every node
count. Native execution sent and received equal global byte totals:
110,886,912 on two nodes, 169,869,312 on four, and 214,695,936 on eight. These are
measured application routing counters for all three roundtrips, not total network
wire traffic or publication/load traffic.

For the fixed-size points, the following values are the largest per-node values
across the participating nodes. MiB and GiB use powers of two.

| Nodes | Maximum sampled rank RSS (MiB) | Maximum sampled rank AS (GiB) | Maximum rank RSS high-water mark (MiB) | Enforced cap/node (GiB) |
| ---: | ---: | ---: | ---: | ---: |
| 2 | 238.359 | 2.614620 | 238.359 | 8 |
| 4 | 223.176 | 2.591614 | 223.176 | 8 |
| 8 | 213.590 | 2.585011 | 213.590 | 8 |

The maximum sampling spans were 0.047 ms, 0.314 ms and 1.784 ms respectively.
There were 1,599, 3,269 and 7,445–7,447 samples per node. Because each node owns one
rank here, a node's sum of rank high-water marks has one term; its equality with
the sampled maximum in these receipts does not turn sampling into a continuous
physical-memory measurement.

## Capacity boundary and larger attempts

The largest completed original input is now 8 MiB, one 1,024th of the 8 GiB
cap on each node. The strict original-input inequality therefore fails on every
completed point, independently of successful execution and node coverage. The
larger HDF5 representation cannot change that conclusion.

Eight-node GNU growth job `536793` completed `N=131,072`, continuing the doubling
sequence from `65,536`, with the same binary, caps and three roundtrips. It created
262,144 original nonzeros in 8,388,608 bytes; HDF5 encoding occupied 21,025,984 bytes.
Full copied source shards and receipts passed local revalidation. Launcher time
was 301.274 seconds within the 480-second case timeout. Source storage took
0.040 seconds; preprocessing 1.876, persistence 0.166, load 284.613, preparation
1.028 and all three roundtrips 10.761 seconds. The median rank-maximum roundtrip
was 3.589 seconds, norm remained `0.9999999999999999`, and maximum sampled error
was `6.508e-19`. Actual hosts were the same eight hosts as job `536734`.

On eight nodes, producer managed peak admission increased from 8,213,504 to
16,339,968 bytes/rank, and native live reservations from 9,339,848 to 18,645,968,
as original input doubled from 4 MiB to 8 MiB. At the larger point, either declared
per-rank reservation already exceeds the entire original input. These are
admission-accounting values rather than measured physical peaks. They show why
this implementation does not establish a useful input-to-envelope ratio on eight
nodes; they do not establish a largest admissible dimension.

Eight-node GNU admission job `536794` requested `N=2^28`, with a 120-second
timeout and four-minute allocation limit. It exited 1 after 2.410 seconds,
without timing out or truncating the log. All eight ranks reported
`source/state minimum exceeds managed rank allowance`. The harness's minimum
native-state admission estimate is 8 GiB/rank at that dimension, above its 4 GiB
managed allowance. The check occurs before source creation: the copied case
directory contains only `launcher.log` and its supervision JSON, with **zero
original-input files, zero original-input bytes, no HDF5 data and no completed
rank receipts**. The campaign correctly records `complete=false`,
`capacity_closed=false`, and no completed cases. This is a verified admission
rejection, not a completed oversized execution.

If materialized, the requested source would have occupied 16 GiB. That number
never counts as stored input. The supervisor captured owned step `536794.0`,
issued step-scoped cancellation with exit status zero, and confirmed through
`squeue` that the step had disappeared while the batch and extern steps remained.
This confirms bounded cleanup after admission failure; it does not claim that the
cancellation command killed ranks which might already have exited.

Intermediate dimensions between `131,072` and `2^28` were not executed, so no
largest admissible dimension is claimed. Neither the completed growth point nor
the early admission rejection exercises MPI payload counts above the legacy
integer count limit; that lane remains unrun.

## Cray hybrid execution and fixed-size scaling

Cray jobs `536815`, `536816` and `536817` completed `N=65,536` on two, four and
eight nodes respectively, with three roundtrips, the same caps and placement as
the GNU points. They used successful build job `536808` from immutable source
`8d85c0e8075cec3057b4da14055c73ebc35bb04ab4b01b155153f678bdfa0f4d`
and executable SHA-256
`ead3182f24a42742f7a88f25cf2468b77190769abfbde010d7247be5b92a39c7`.
The fresh matching native prefix uses Cray clang 19.0.0, the same native QuEST
revision, Cray MPICH 8.1.32 and centrally maintained `cray-hdf5/1.14.3.5`, with
`cray-libsci` absent. The recorded Rust linker policy is
`-C linker-features=-lld -C link-arg=-mno-daz-ftz`, with the same Rust nightly.
The maintained login-shell coordinator ran directly; these jobs needed no
external login adapter or snapshot modification.

All three Cray cases passed local revalidation of their complete copied source
shards and rank receipts. Their original COO32 shards are byte-identical to the
GNU shards at the same node count. Each source has 131,072 nonzeros in 4,194,304
bytes; HDF5 encoding lengths also match the corresponding GNU case. MPI reported
the same distinct host sets listed above for each node count, with one rank per
shared-memory group. Native environment/register threading flags were true and
process thread counts were again 2, 289 and 288 at the recorded stages. Norm was
`0.9999999999999999` and maximum sample error `8.674e-19` throughout.

| Nodes | Preprocess (s) | Persist (s) | Load (s) | Prepare (s) | Three roundtrips (s) | Median rank-max roundtrip (s) | Launcher (s) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 1.882 | 0.238 | 27.529 | 0.527 | 4.696 | 1.563 | 37.283 |
| 4 | 1.297 | 0.187 | 64.564 | 0.526 | 4.565 | 1.521 | 73.468 |
| 8 | 0.994 | 0.125 | 154.681 | 0.559 | 5.701 | 1.890 | 165.142 |

Cray producer payload and native execution routing totals equal the GNU totals
at each node count. These points demonstrate completed hybrid deployment and
matching execution on each compiler profile, but do not demonstrate end-to-end
strong scaling: launcher duration increases from 37.28 to 165.14 seconds.
The GNU and Cray source snapshots differ in cleanup and harness fixes; this is
not a controlled attribution of timing differences solely to the compiler.

| Nodes | Maximum sampled rank RSS (MiB) | Maximum sampled rank AS (GiB) | Maximum rank RSS high-water mark (MiB) |
| ---: | ---: | ---: | ---: |
| 2 | 191.699 | 2.575996 | 191.699 |
| 4 | 170.766 | 2.552505 | 170.766 |
| 8 | 166.773 | 2.547310 | 166.773 |

Every Cray node retained the verified 8 GiB participating-rank address-space cap.
As with GNU, these sampled and high-water values have distinct meanings and do
not establish a continuous physical-node peak. All three Cray capacity outcomes
remain open because their 4 MiB original input is below each 8 GiB cap.

## Cray weak scaling with a frozen executable

Jobs `536862`, `536863` and `536864` completed weak-scaling points on two, four
and eight nodes, holding 8,192 columns per rank. All use three roundtrips and the
same source, matching native prefix, build profile and caps as the Cray fixed-size
points above. These three runs completed without timeout or log truncation.

Preflight detected that the mutable Cargo example path no longer had the earlier
fixed-size executable hash. The cause of that replacement is not established.
This separate lane uses one verified read-only binary copy with SHA-256
`f3645280b7339a4ece6add7654ecc6aa546b7dc6f7b72913400754387e58fc1e`,
with its own eight-node endpoint; no mixed-binary weak-scaling comparison is made.
A recorded external coordinator adapter, SHA-256
`2a927b9859097f1b17ba0b518745f5a22a512dff3bb4d52a2c197c442cd18cba`,
changes only executable selection after the original source/build/profile checks.
It verifies a bounded read-only copy and reuses that copy across the three jobs.
Neither the source snapshot nor the Cargo target is modified.

| Job | Nodes / MPI ranks | N | Original nonzeros | Original bytes | HDF5 encoding bytes | Actual MPI processor names |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 536862 | 2 / 2 | 16,384 | 32,768 | 1,048,576 | 2,635,056 | `cs-n0264`, `cs-n0265` |
| 536863 | 4 / 4 | 32,768 | 65,536 | 2,097,152 | 5,270,112 | `cs-n0264`, `cs-n0265`, `cs-n0268`, `cs-n0269` |
| 536864 | 8 / 8 | 65,536 | 131,072 | 4,194,304 | 10,540,224 | `cs-n0434` through `cs-n0439`, `cs-n0446`, `cs-n0447` |

All three cases passed full receipt validation and streaming source decoding.
Original-shard hashes and HDF5 lengths equal the corresponding GNU weak points.
Every actual shared-memory group has one rank with an enforced 8 GiB cap.
Native environment/register multithreading flags are true; process thread counts
are 2, 289 and 288 at the recorded stages, with the same team-size and serial-work
limitations described above. Norm is `0.9999999999999999`, and maximum sampled
amplitude error is `1.735e-18`.

| Nodes | Preprocess (s) | Persist (s) | Load (s) | Prepare (s) | Three roundtrips (s) | Median rank-max roundtrip (s) | Launcher (s) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | 0.454 | 0.094 | 6.577 | 0.171 | 1.070 | 0.356 | 9.871 |
| 4 | 0.620 | 0.115 | 32.341 | 0.279 | 2.216 | 0.739 | 37.618 |
| 8 | 0.971 | 0.116 | 156.374 | 0.546 | 5.728 | 1.906 | 166.730 |

Source-storage maxima were 0.027, 0.026 and 0.029 seconds. Producer payload totals
were 26,214,400, 52,428,800 and 104,857,600 bytes; native execution sent and received
27,721,728, 84,934,656 and 214,695,936 bytes respectively. They match the corresponding
GNU weak-point counters. These are measured application payloads, with the same
uninstrumented publication/load wire-traffic boundary as the fixed-size points.

| Nodes | Maximum sampled rank RSS (MiB) | Maximum sampled rank AS (GiB) | Maximum rank RSS high-water mark (MiB) |
| ---: | ---: | ---: | ---: |
| 2 | 165.469 | 2.547096 | 165.469 |
| 4 | 166.812 | 2.547104 | 166.812 |
| 8 | 171.117 | 2.547119 | 171.117 |

These weak points show increasing launcher and roundtrip latency as node count
grows; they do not demonstrate constant latency for fixed work per node. Capacity
remains open: the stored original source at each point is smaller than every
node's participating-rank cap.

## Receipt retention and final validation

A subsequent [storage-bound analysis](../research/distributed-capacity-memory.md)
shows that the native array payload alone prevents this controlled, two-register
profile from exceeding its per-rank cap with original input on eight or fewer
nodes. New native telemetry distinguishes array payload from reservations and
process memory. Producer lifetime-accounting and loader follow-ups have separate
source identities; they do not alter the historical measurements below.

All thirteen completed cases additionally passed a bounded streaming decode of every
original COO32 record: 1,441,792 nonzeros across the repeated cases. This audit
checked exact column ownership, row indices and both coefficient components,
plus cross-compiler original-shard hash identity. It separately verified the
oversized case directory contains no data files and its step cleanup was confirmed.
An independent read-only review of the final three weak points decoded their
229,376 source records and checked binary identities, hashes, node ownership,
caps, numerical results and all summarized tables; it found no discrepancies.

Raw receipts are retained under the ignored local directory
`target/capacity-verification/cirrus-scaling/` and the campaign's
`$CAMPAIGN/capacity-receipts/<compiler>/job-<job-id>.<unique-suffix>/` directories
for each compiler campaign. Exact submissions, working-directory corrections,
Slurm state, immutable source/build
identities, complete source shards, per-rank receipts and launcher supervision
records accompany the summarized evidence.

The campaign JSON files, including the incomplete admission attempt, are bound
by these SHA-256 values:

| Job | `completion.json` SHA-256 |
| --- | --- |
| 536732 | `a697790f6b22440028e8beabf0c87a64f86f3425fec2159b04f4935c931d53e5` |
| 536733 | `7b8b328aa7f043039535750d9d1baaeaa6047a881393177026774b0ca130f27e` |
| 536734 | `25184ea9b9af6480cef4adc344c7f1afe4bb9a9ebf7ebd3c067e84950f1ad646` |
| 536793 | `423ae1709c11089ba5059085bf9c9ad4b6f3c7b34ee0dab374c54481810cf904` |
| 536794 | `d259f5aceea2bec6822ed15056ac049bf99e59e5a8032245b4b3e52961c0dabb` |
| 536815 | `c51cfd1d527f0f74500bd1784373f644b0a76be66b6ff115e098a96ec4bbdd68` |
| 536816 | `22942874ef7c20062a69a71ad195cd903442a3763b9571f97a325ea21180213a` |
| 536817 | `ae6f93bd9668abfbcd5b444704d8fa85e064feb736e24196d922603531fe8b22` |
| 536862 | `ca6736256fab0799027f3aa0a4f6bf12a71e3ccde70544f76816404778c19f9f` |
| 536863 | `0af5ae1e14f08bdcac984bb9c21984fac26d952599d91e8b87cc7f049119b5d7` |
| 536864 | `54650f14b1c9b5e779a57cb9ead0708096e879963093d1392566b91804aec582` |
