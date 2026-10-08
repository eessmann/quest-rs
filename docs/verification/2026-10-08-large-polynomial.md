# Large-polynomial resource and capacity verification — 2026-10-08

The final capacity run completed **37 cases**: all 17 preserved native inverse
checks and all 20 independently prepared forward cases passed their stated final
checks. Each forward case also passed a second call using retained plans and
scratch. All 37 cases released every tracked buffer and planner reservation.

This record implements the capacity part of the
[approved plan](../plans/2026-10-08-large-polynomial-benchmarks.md). The
[resource API migration guide](../resource-api-migration.md) explains the caller
changes. Historical verification documents and exploratory receipts are retained.
Current workspace acceptance is recorded separately in the
[large-polynomial workspace matrix](2026-10-08-large-polynomial-workspace.md).

## Evidence and identity

The portable [completion receipt](data/2026-10-08-large-polynomial/capacity-final/run-1/completion.json)
binds the [37-case summary](data/2026-10-08-large-polynomial/capacity-final/run-1/summary.json)
to these SHA-256 identities:

| Artefact | SHA-256 |
| --- | --- |
| Executed release binary | `6e435fcba2d7ba09becc52c4e3c45df1501be98aaa43a76fd5227581502819a1` |
| Matched corpus manifest | `b7b9842066ced783599a21bba61169629f1e232115dfde52256bbe3833f343a6` |
| Exact native fixture index | `bb9a94a52ff5e09832c4b2cafd72408945253b04afed44f5cefc2962eff84202` |
| Final summary | `5728c8391f4f16a7d8e4015b1fe203be2e724d61cef081557d60f0f339939162` |

The [preservation index](data/2026-10-08-large-polynomial/preservation.json)
records source and portable hashes for every copied receipt. JSON results,
completion and summary retain their exact bytes. GNU `time -v` reports retain
their observations, with only the command header replaced by generic paths.
Per-case stderr is retained. The
[acceptance audit](data/2026-10-08-large-polynomial/acceptance-audit.json)
checks completion/summary identity, original source checks, forward reuse,
resource ceilings and final releases. This audit describes the retained capacity
run; it does not create a matched publication verdict.

The original native fixtures are archived in the SoftwareX companion repository
at `data/benchmarks/nlft/20261008-matched-v1/native-regression/`. They are gzip
copies of unchanged IEEE-754-bit JSON, with source and archive hashes in
`index.json`; no replacement inputs were generated for this regression lane.
The [portable index](data/2026-10-08-large-polynomial/native-index.json) records
upstream revision `4fc35983138d07a990862a4d83ad16f2b737c98f`. Independent forward
inputs are identified separately in the
[matched corpus manifest](data/2026-10-08-large-polynomial/corpus-manifest.json).

## Exactly which checks passed

The native source contract is defined in
`test/qsp_tools/nlft_inverse_benchmark_fixture.hpp` at the recorded upstream
revision. Its large forward-prepared fixtures compare the recovered **reflection
sequence** with the generating sequence. Their component tolerance does not
apply to a later reconstructed scattering pair.

| Lane | Cases | Checks retained | Result |
| --- | ---: | --- | --- |
| Native inverse, degrees 5–1,000 | 8 | Finite, size-preserving output; no expected reflection sequence is supplied | 8 passed |
| Native inverse, degrees 2,000–1,000,000 | 9 | Finite, size-preserving output and original maximum complex reflection-error tolerance | 9 passed |
| Independent forward, degrees 0–1,000,000 | 20 | Physical pair coefficient maximum error against frozen independently prepared pairs, at `1e-10` | 20 passed |
| Forward workspace reuse | 20 additional calls | Same coefficient gate; unchanged cached plan count and scratch bytes | 20 passed |
| All final capacity cases | 37 | Both tracked live-buffer and live-planner bytes zero after owners drop | 37 passed |

Forward input reflections are independently supplied, not produced by an inverse
implementation under comparison. Native inverse reconstruction and forward
comparison both meet the separate fixed `1e-10` coefficient envelope in this run.
That is one coefficient metric: the matched protocol additionally requires its
mixed absolute/relative L2 residual, 32 held-out response checks, oracle status,
and validation of each actual timed output. Those campaign checks are pending at
the time of this record. The original source tolerances remain unchanged.

## Million-degree results

Degree 1,000,000 means 1,000,001 coefficient/reflection slots. The explicit profile
allows FFT length 2^21, 8,589,934,592 modelled peak bytes and 137,438,953,472
cumulative work units. Completion has its own 2^22 grid gate and was not executed
by this capacity run.

| Observation | Native inverse plus reconstruction | Independent forward, then reuse |
| --- | ---: | ---: |
| Maximum recovered-reflection error | `2.543718508157025e-18` | Not an inverse operation |
| Original reflection tolerance | `1.9037152025719928e-11` | Not applicable |
| Maximum pair coefficient error | `5.409406757634787e-11` | `2.37999074032036e-11` |
| Modelled peak bytes | 7,051,541,504 | 2,472,142,960 |
| Measured process peak RSS bytes | 904,462,336 | 481,669,120 |
| Final cumulative work units | 97,027,214,126 | 84,665,567,762 |
| Tracked live bytes after all owners drop | 0 | 0 |

The forward workspace retains 20 plans and 100,662,192 scratch bytes, unchanged
by the repeat call. Work increases from 42,332,783,881 to 84,665,567,762; reuse does
not reset the ledger or make the second execution free. Plan estimates and
scratch remain charged while their workspace owns them.

Modelled memory includes owned buffers and retained opaque-planner estimates;
it is not an allocator quota or a prediction of RSS. GNU `time` observes the
whole adapter, including input loading and both forward calls where applicable.
Neither its wall time nor these resource counts are operation-performance or
cross-language speedup measurements. Both million-degree coefficient errors are
below `1e-10` and above `1e-12`; this does not establish the full matched-protocol
main or strict verdict.

## Preserve the additional reconstruction failures

The earlier [exploratory summary](data/2026-10-08-large-polynomial/exploratory/all17/summary.json)
used binary `20397ad9551d946d267480d8bba1d91ab53bb9a99c7883615ed74a65806f5b7f`.
It also compared reconstructed pair coefficients against the native fixture's
reflection tolerance. That extra Rust check failed at degrees 500,000 and
1,000,000:

| Degree | Extra pair residual | Reflection tolerance applied by the extra check | Recovered-reflection error |
| --- | ---: | ---: | ---: |
| 500,000 | `2.6365501293566437e-11` | `1.8127658636068528e-11` | `2.7097613656756847e-18` |
| 1,000,000 | `5.409406757634787e-11` | `1.9037152025719928e-11` | `2.543718508157025e-18` |

The final receipts retain both failures in `additional_reconstruction_status`.
Their top-level `status: ok` and the completion receipt's `all_passed: true`
refer to the explicitly stated final checks, not every recorded diagnostic.
No source fixture tolerance was relaxed, and these two pair residuals must not
be described as failures of the upstream recovered-reflection contract.

A retained [100-decimal-digit normalisation diagnostic](data/2026-10-08-large-polynomial/exploratory/native-million-normalization-mp100.json)
compares the million-degree fixture's constant coefficient with
`product((1 + |gamma|^2)^(-1/2))`. The difference is about `5.5044465e-11`.
This necessary-identity diagnostic helps explain the distinction between exact
reflection recovery and pair reconstruction; it is not a formal certificate.

## Reproduction and remaining boundaries

Use the existing locked native environment and disk-backed `TMPDIR`. Serialize
native builds and executions with the shared lock inside a scope limited to
24 GiB MemoryHigh and 28 GiB MemoryMax. Build the
`large_polynomial_acceptance` release example with
`quest-qsp/benchmark-support,simd`, then run the retained capacity controller:

```sh
systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G \
  flock /tmp/quest-quality-build.lock \
  python benchmarks/replication/large_capacity.py \
    --binary <release>/examples/large_polynomial_acceptance \
    --corpus <softwarex>/data/benchmarks/nlft/20261008-matched-v1/corpus \
    --native-regression <softwarex>/data/benchmarks/nlft/20261008-matched-v1/native-regression \
    --output <new-capacity-run>
```

The runner verifies both compressed and decompressed native fixture hashes and
each independent input hash. It retains per-case JSON, stderr and whole-process
memory observations before writing a completion record. Preserve each run in a
new directory. The executable and manifest hashes bind this record to the
observed run, rather than to an assumed future source tree.

The remaining gates are independent validation of actual timed outputs, frozen
pilot/publication provenance, and 20 paired process blocks for eligible runtime
ratios. Certified polynomial completion, arbitrary large or ill-conditioned
inputs, other hosts, and performance speedups are not established by this run.
