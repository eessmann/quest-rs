#!/usr/bin/env python3
"""Bounded single-host MPI capacity experiment; Linux process AS caps are real."""
import argparse
import json
import math
import os
import pathlib
import resource
import signal
import subprocess
import sys
import time

STAGES = ("preprocessing", "persistence", "load", "prepare", "execution")


# This fixture intentionally accepts one exact receipt shape from sparse_capacity.
# There is no extensible generic JSON acceptance contract.
INTEGER_FIELDS = (
    "rank", "ranks", "dimension", "repetitions", "process_address_space_cap_bytes",
    "rss_high_water_bytes", "address_space_high_water_bytes", "baseline_rss_bytes",
    "baseline_address_space_bytes", "model_rank_budget_bytes", "model_node_budget_bytes",
    "model_peak_bytes", "producer_managed_peak_upper_bound_bytes",
    "loaded_retained_upper_bound_bytes", "native_live_reserved_bytes",
    "local_input_entries", "global_input_entries", "local_native_amplitudes",
    "global_native_amplitudes", "producer_sent_payload_bytes", "producer_global_edges",
    "colors", "persisted_recipe_single_replay_communication_upper_bound_bytes",
    "execution_sent_bytes", "execution_received_bytes", "execution_coordination_calls",
    "execution_local_pair_candidates", "execution_batches",
)
RECEIPT_FIELDS = set(INTEGER_FIELDS) | {
    "model_peak_kind", "persistence_load_measured_communication", "norm",
    "maximum_sample_error", "execution_roundtrip_seconds", "stages",
}
STAGE_MEMORY_FIELDS = (
    "rss_endpoint_bytes", "rss_high_water_bytes", "address_space_endpoint_bytes",
    "address_space_high_water_bytes",
)
STAGE_FIELDS = {"seconds", *STAGE_MEMORY_FIELDS}
MAX_WORD = 2 * sys.maxsize + 1
MAX_RECEIPT_BYTES = 1024 * 1024


def unsigned(value, field, positive=False):
    if type(value) is not int or not int(positive) <= value <= MAX_WORD:
        raise ValueError(f"invalid integer receipt field {field}")
    return value


def finite(value, field, minimum=0):
    try:
        valid = type(value) in (int, float) and math.isfinite(value) and value >= minimum
    except OverflowError:
        valid = False
    if not valid:
        raise ValueError(f"invalid finite receipt field {field}")
    return value


def read_receipt(path):
    def constant(value):
        raise ValueError(f"nonstandard JSON numeric constant {value}")

    def object_fields(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON receipt field {key}")
            result[key] = value
        return result

    with path.open("rb") as source:
        content = source.read(MAX_RECEIPT_BYTES + 1)
    if len(content) > MAX_RECEIPT_BYTES:
        raise ValueError("rank receipt exceeds bounded JSON size")
    try:
        return json.loads(content, parse_constant=constant, object_pairs_hook=object_fields)
    except (json.JSONDecodeError, UnicodeDecodeError, RecursionError, OverflowError) as error:
        raise ValueError("malformed JSON receipt") from error


def summarize(rows, ranks, dimension, repetitions, as_cap, rank_budget, node_budget,
              *, max_ranks=8, max_dimension=4096, ranks_per_node=None, resource_only=False):
    for field, value in (("ranks", ranks), ("dimension", dimension),
                         ("repetitions", repetitions), ("as_cap", as_cap),
                         ("rank_budget", rank_budget), ("node_budget", node_budget)):
        unsigned(value, field, positive=True)
    if (ranks < 1 or ranks > max_ranks or ranks & (ranks - 1) or not 16 <= dimension <= max_dimension or
            dimension & (dimension - 1) or not 1 <= repetitions <= 8):
        raise ValueError("unsupported receipt configuration")
    if rank_budget * (ranks if ranks_per_node is None else ranks_per_node) > node_budget:
        raise ValueError("local node modeled placement envelope exceeds budget")
    if type(rows) is not list or len(rows) != ranks:
        raise ValueError("missing rank receipts")
    if any(type(row) is not dict or set(row) != RECEIPT_FIELDS for row in rows):
        raise ValueError("incomplete or unknown receipt schema")
    for row in rows:
        for field in INTEGER_FIELDS:
            if resource_only and field == "persisted_recipe_single_replay_communication_upper_bound_bytes":
                if row[field] is not None:
                    raise ValueError("resource-only load has no admitted portable replay bound")
                continue
            unsigned(row[field], field)
    if sorted(row["rank"] for row in rows) != list(range(ranks)):
        raise ValueError("missing or duplicate rank receipt")
    for row in rows:
        expected = dict(ranks=ranks, dimension=dimension, repetitions=repetitions,
                        process_address_space_cap_bytes=as_cap,
                        model_rank_budget_bytes=rank_budget, model_node_budget_bytes=node_budget)
        if any(row[key] != value for key, value in expected.items()):
            raise ValueError("receipt configuration/cap mismatch")
        if (row["model_peak_kind"] != "conservative admitted stage envelope, not measured peak" or
                row["persistence_load_measured_communication"] != (
                    "logical broadcast calls only; wire bytes and MPI-internal communication unmeasured"
                    if resource_only else "not instrumented; recipe replay bound is not a load measurement")):
            raise ValueError("receipt measurement scope mismatch")
        for key in ("model_peak_bytes", "producer_managed_peak_upper_bound_bytes",
                    "native_live_reserved_bytes", "loaded_retained_upper_bound_bytes"):
            if row[key] > rank_budget:
                raise ValueError("modeled rank memory envelope exceeded")
        if row["model_peak_bytes"] != rank_budget:
            raise ValueError("configured stage envelope mismatch")
        for key in ("rss_high_water_bytes", "address_space_high_water_bytes",
                    "baseline_rss_bytes", "baseline_address_space_bytes"):
            if not 0 < row[key] <= as_cap:
                raise ValueError("rank memory observation outside process cap")
        if (row["baseline_rss_bytes"] > row["rss_high_water_bytes"] or
                row["baseline_address_space_bytes"] > row["address_space_high_water_bytes"]):
            raise ValueError("memory high water does not cover baseline")
        if row["producer_global_edges"] != 2 * dimension or row["global_input_entries"] != 2 * dimension:
            raise ValueError("incomplete or duplicated source edges")
        if row["local_input_entries"] != 2 * len(range(row["rank"], dimension, ranks)):
            raise ValueError("generation was not source sharded")
        # Two matching colors, one matching flag and one outer signed-control bit.
        if (row["colors"] != 2 or row["global_native_amplitudes"] != 8 * dimension or
                row["local_native_amplitudes"] * ranks != 8 * dimension):
            raise ValueError("wrong matching/control layout or native partition")
        if row["native_live_reserved_bytes"] < 2 * row["local_native_amplitudes"] * 16:
            raise ValueError("native reservation does not cover caller/scratch partitions")
        finite(row["norm"], "norm")
        finite(row["maximum_sample_error"], "maximum_sample_error")
        if abs(row["norm"] - 1) > 1e-10 or row["maximum_sample_error"] > 1e-10:
            raise ValueError("native forward/adjoint validation failed")
        if type(row["stages"]) is not dict or set(row["stages"]) != set(STAGES):
            raise ValueError("incomplete stage receipt")
        for stage in row["stages"].values():
            if type(stage) is not dict or set(stage) != STAGE_FIELDS:
                raise ValueError("incomplete or unknown stage schema")
            finite(stage["seconds"], "stage seconds")
            for field in STAGE_MEMORY_FIELDS:
                unsigned(stage[field], field, positive=True)
                if stage[field] > as_cap:
                    raise ValueError("stage memory observation exceeds process cap")
            if (stage["rss_high_water_bytes"] > row["rss_high_water_bytes"] or
                    stage["address_space_high_water_bytes"] > row["address_space_high_water_bytes"]):
                raise ValueError("final high water does not cover stage observation")
        times = row["execution_roundtrip_seconds"]
        if type(times) is not list or len(times) != repetitions:
            raise ValueError("incomplete repeated replay timing receipt")
        for duration in times:
            finite(duration, "roundtrip seconds")
        total = row["stages"]["execution"]["seconds"]
        if sum(times) > total + 1e-12 * max(1, total):
            raise ValueError("roundtrip timings exceed complete execution stage")
    sent = sum(row["execution_sent_bytes"] for row in rows)
    received = sum(row["execution_received_bytes"] for row in rows)
    if sent != received:
        raise ValueError("aggregate native routing sent/received counters mismatch")
    return dict(dimension=dimension, ranks=ranks, repetitions=repetitions,
                sum_rank_rss_high_water_bound_bytes=sum(r["rss_high_water_bytes"] for r in rows),
                stage_max_seconds={s: max(r["stages"][s]["seconds"] for r in rows) for s in STAGES},
                execution_sent_bytes=sent, execution_received_bytes=received,
                producer_sent_payload_bytes=sum(r["producer_sent_payload_bytes"] for r in rows),
                rank_receipts=rows)


def run_job(command, timeout):
    # Kill the entire local launcher/process group on timeout, including its proxies.
    with subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          text=True, start_new_session=True) as process:
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
            raise subprocess.TimeoutExpired(command, timeout, output=stdout, stderr=stderr)
        return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--rank-wrapper":
        cap = int(sys.argv[2])
        resource.setrlimit(resource.RLIMIT_AS, (cap, cap))
        os.execv(sys.argv[3], sys.argv[3:])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--mpiexec", default="mpiexec")
    parser.add_argument("--cases", default="64:1,256:2,1024:4,64:8")
    parser.add_argument("--repetitions", type=int, default=3)
    parser.add_argument("--process-as-mib", type=int, default=2048)
    parser.add_argument("--model-rank-mib", type=int, default=16)
    parser.add_argument("--model-node-mib", type=int, default=128)
    parser.add_argument("--timeout", type=int, default=180)
    args = parser.parse_args()
    try:
        cases = [tuple(map(int, case.split(":"))) for case in args.cases.split(",")]
    except ValueError:
        parser.error("cases require dimension:ranks pairs")
    if not 1 <= len(cases) <= 8 or any(len(case) != 2 for case in cases):
        parser.error("one to eight dimension:ranks cases required")
    if (not 1 <= args.repetitions <= 8 or not 1 <= args.timeout <= 600 or
            min(args.process_as_mib, args.model_rank_mib, args.model_node_mib) <= 0):
        parser.error("positive caps and bounded repetitions/timeout required")
    caps = [value * 1024**2 for value in
            (args.process_as_mib, args.model_rank_mib, args.model_node_mib)]
    for n, p in cases:
        if not (16 <= n <= 4096 and n & (n - 1) == 0 and p in (1, 2, 4, 8)):
            parser.error("cases require power-of-two dimensions 16..4096 and ranks 1/2/4/8")
        if caps[1] * p > caps[2]:
            parser.error("modeled rank envelopes exceed local-node budget")
    executable = str(args.executable.resolve(strict=True))
    args.output.mkdir(parents=True, exist_ok=False)
    campaign = dict(scope="single-host local experiment; no multi-host acceptance",
                    cap_kind="per-rank Linux RLIMIT_AS hard/soft address space",
                    process_address_space_cap_bytes=caps[0], model_rank_budget_bytes=caps[1],
                    model_node_budget_bytes=caps[2], cases=[], complete=False)
    try:
        for ordinal, (n, p) in enumerate(cases):
            directory = (args.output / f"case-{ordinal}-n{n}-p{p}").resolve()
            directory.mkdir()
            command = [args.mpiexec, "-hosts", "localhost", "-n", str(p), sys.executable,
                       str(pathlib.Path(__file__).resolve()), "--rank-wrapper", str(caps[0]),
                       executable, str(directory), str(n), str(args.repetitions),
                       *(str(cap) for cap in caps)]
            start = time.monotonic()
            try:
                result = run_job(command, args.timeout)
            except subprocess.TimeoutExpired as error:
                (directory / "launcher.log").write_text((error.stdout or "") + (error.stderr or ""))
                raise RuntimeError(f"case {ordinal} timeout; local process group stopped") from error
            # Paths in launcher diagnostics can be private; these local artifacts are not checked in.
            (directory / "launcher.log").write_text(result.stdout + result.stderr)
            if result.returncode:
                raise RuntimeError(f"case {ordinal} MPI exit {result.returncode}; see local launcher.log")
            rows = [read_receipt(directory / f"rank-{r}.json") for r in range(p)]
            summary = summarize(rows, p, n, args.repetitions, *caps)
            summary["launcher_seconds"] = time.monotonic() - start
            campaign["cases"].append(summary)
            print(json.dumps({key: value for key, value in summary.items() if key != "rank_receipts"}), flush=True)
        campaign["complete"] = True
    except (RuntimeError, ValueError, OSError, subprocess.TimeoutExpired) as error:
        campaign["failure"] = str(error)
        raise
    finally:
        (args.output / "completion.json").write_text(json.dumps(campaign, indent=2, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
