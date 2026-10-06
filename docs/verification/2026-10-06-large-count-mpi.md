# Real large-count logical-byte MPI transport

GNU job `537276` and Cray job `537277` completed real two-node transfers of
2,147,483,664 bytes in each direction. Each rank allocated and touched that
entire logical buffer. Each exchange used one 2,147,483,647-byte (`INT_MAX`)
frame followed by a 17-byte tail, with an empty opposite direction. Both ranks
verified every byte against a deterministic rank/absolute-offset pattern.
The [machine-readable summary and raw completion receipts](data/2026-10-06-large-count-mpi/summary.json)
record the checked counts, hashes, actual hosts, allocation and memory evidence.

This verifies chunking of a logical payload larger than the native signed-int
count limit. Every individual MPI count still fits `int`; no MPI-4 large-count
`_c` function was exercised. The current matching routing workspace remains
bounded to 64 pairs / 128 amplitude records / 5,120 bytes. It was not rerouted
through this new method. This is neither matching-kernel performance evidence
nor a beyond-memory capacity result.

## Implementation and local regression

`quest_sys::mpi::MpiCollectiveLane::send_receive_bytes_chunked` preserves the
existing single-message APIs. Its constant-memory metadata broadcasts check
reciprocal peers, tags, lengths and frame bounds across the communicator before
any receive buffer changes. Send and receive totals may differ, including zero.
The checked offset loop then exchanges at most `INT_MAX` bytes per frame. Native
failures or unexpected counts after transport starts follow the existing fatal
MPI policy. Callers collectively admit their buffer allocations before entry.

The focused four-rank regression passed, including two-rank split communicators,
unequal lengths, empty directions, tails under an injected seven-byte ceiling,
self exchange, invalid and nonreciprocal peers, mismatched tags/lengths, and
valid-but-different frame ceilings. One pair can have an empty exchange while
another needs several frames. Rejected metadata leaves receive sentinels intact.
This small-ceiling regression is separate from the actual `INT_MAX`-boundary
cluster runs. A local two-rank example with 1,048,593 bytes also passed.
Strict `quest-sys` Clippy and scoped formatting checks passed.

## Frozen build and execution scope

Both lanes used the immutable `8d85c0e8075cec3057b4da14055c73ebc35bb04ab4b01b155153f678bdfa0f4d`
baseline with a named three-file transport overlay: `quest-sys/src/mpi.rs`,
`quest-sys/examples/mpi_large_count.rs`, and `quest-sys/Cargo.toml`. Their exact
SHA-256 values are in the summary. Cargo's only lock change adds the existing
`serde_json` dev dependency to `quest-sys`; registry packages and versions are
unchanged. Each compiler used a separate source copy and Cargo target. The
example alone received `-C opt-level=2`; the admitted native and Rust dependency
profiles remained unchanged. No full-workspace rerun is claimed for this overlay.

The matching GNU/Cray module profiles include central HDF5 and installed native
QuEST revision `503552065045eaf89baba85e6cd6aad728525554`. Native QuEST source and
installed prefixes were unchanged. Each allocation requested two exclusive
nodes, one MPI rank and 288 physical CPU slots per node, with a twenty-minute
`short` limit. `OMP_NUM_THREADS=288` remained in the environment; this byte
transport probe does not execute or measure an OpenMP kernel or team.

Both lanes ran on distinct hosts `cs-n0000` and `cs-n0001`. MPI shared-memory
communicators independently confirmed exactly one rank per node. Each process
enforced an 8 GiB `RLIMIT_AS`, checked available memory and collectively admitted
`try_reserve_exact` before touching payloads. Peak RSS exceeded the allocated
logical buffer size, confirming resident touched storage. The cap covers the MPI
rank process, not all physical-node processes or filesystem cache.

| Compiler / job | Forward rank-max exchange (s) | Reverse rank-max exchange (s) | Supervisor (s) | Maximum rank RSS (bytes) | Maximum rank address space (bytes) |
| --- | ---: | ---: | ---: | ---: | ---: |
| GNU / 537276 | 0.108797510 | 0.088471245 | 9.227009280 | 2,281,435,136 | 2,417,692,672 |
| Cray / 537277 | 0.113788816 | 0.155425115 | 10.160349162 | 2,287,325,184 | 2,377,068,544 |

Exchange times include collective metadata admission and MPI payload transfer.
Pattern generation, initial memory touching and full-byte verification have
separate receipt fields. These are single observations per direction, not a
repeatability or throughput benchmark. Total bidirectional application payload
was 4,294,967,328 bytes per compiler lane. The supervisor imposed a 300-second
launch deadline, retained bounded logs and identified the owned Slurm steps
`537276.0` and `537277.0`; neither timed out or truncated its log.

## Preserved failures and reproduction

Initial GNU `537272` and Cray `537273` failed before compilation or MPI launch:
`cp -a` preserved the immutable baseline's read-only file modes in the new copy.
The corrected recipe makes only that independent copy writable before applying
the overlay. Both unsuccessful receipts are retained, and their diagnostic
hashes appear in the summary. The baseline snapshot was never modified.

Build the explicit probe with the matching installed native/MPI profile:

```sh
cargo rustc -p quest-sys --features mpi --example mpi_large_count -- -C opt-level=2
```

Launch `target/debug/examples/mpi_large_count 2147483664 "$RECEIPTS"
--require-multihost` under the bounded MPI supervisor on two exclusive nodes,
one MPI rank per node. The maintained example writes `rank-0.json` and
`rank-1.json`. The recorded external Slurm recipe verifies the baseline build
profile, admits only the hashed source overlay, captures its owned step, checks
both rank receipts and rechecks the original snapshot/profile after execution.
Its exact SHA-256 and both executable hashes are in the summary. Raw build,
launcher and source-identity receipts remain under the ignored
`target/portability-mpi-large-count/` directory and the campaign's
`$CAMPAIGN/probes/<probe>/results/<compiler>-job-<job-id>/` directories.
