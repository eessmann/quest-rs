#!/usr/bin/env python3
"""Historical capacity validation and local capped MPI experiments.

Cirrus execution of this custom-limit runner is unsupported under the required
documented scheduler workflow. Slurm entry points reject before changing limits
or launching a child. Historical receipt validation remains available.

Hard RLIMIT_AS limits cover participating MPI rank processes, including their
MPI/OpenMP/observer threads, mapped libraries, state, shards and scratch. Their
sum on each shared-memory node is a rank-process address-space envelope, not a
verified whole-node cap: the coordinator, launchers and external helpers are
excluded. This runner currently has no whole-node enforcement evidence and
cannot close strict capacity. Sampled rank RSS sums and sums of rank high-water
marks remain separate observations.
"""
import argparse
import hashlib
import importlib.util
import json
import os
import pathlib
import resource
import subprocess
import sys

SPEC = importlib.util.spec_from_file_location("legacy_capacity", pathlib.Path(__file__).resolve().parent.parent / "sparse-capacity" / "run.py")
LEGACY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LEGACY)
SUPERVISOR_SPEC = importlib.util.spec_from_file_location("capacity_supervision", pathlib.Path(__file__).resolve().with_name("capacity_supervision.py"))
SUPERVISOR = importlib.util.module_from_spec(SUPERVISOR_SPEC)
SUPERVISOR_SPEC.loader.exec_module(SUPERVISOR)
run_job = SUPERVISOR.run_job
NODE_FIELDS = {"leader_rank", "local_rank", "local_size", "processor_name"}
SAMPLE_FIELDS = {"samples", "maximum_sum_rss_bytes", "maximum_sum_address_space_bytes",
                 "max_sample_span_seconds", "interval_milliseconds", "pids"}
EXTRA_FIELDS = {"schema_version", "node", "node_memory_sampling", "canonical_input_bytes", "input_storage_seconds",
                "local_canonical_input_bytes", "local_input_sha256", "persisted_encoding_file_bytes"}
RESOURCE_FIELDS = {"loading_route", "portable_replay_admission", "load_statistics", "native_array_payload"}
LOAD_PHASES = {"manifest_and_admission", "read_validate", "reverse_directory", "replay_admission", "total"}
LOAD_COUNTS = {"local_records", "local_reverse_records", "local_record_capacity", "local_reverse_capacity", "reverse_broadcasts"}
ADMISSION_COUNTS = {"next_record_calls", "forward_calls", "reverse_calls", "broadcasts"}
ARRAY_SCOPE = "native array payload only; excludes allocator, MPI, OpenMP stacks and other temporaries"
SLURM_UNSUPPORTED = (
    "Custom process-memory limits are unsupported in the documented Cirrus workflow; "
    "this capped-memory runner is local-only. Historical receipt validation remains available."
)


def validate_resource_load(row):
    if (row["loading_route"] != "resource-only-native" or row["portable_replay_admission"] != "unrun" or
            row["persisted_recipe_single_replay_communication_upper_bound_bytes"] is not None):
        raise ValueError("resource-only receipt falsely claims portable replay admission")
    stats = row["load_statistics"]
    if type(stats) is not dict or set(stats) != LOAD_COUNTS | {"phase_seconds", "admission", "replay_admitted"}:
        raise ValueError("invalid resource load statistics schema")
    if stats["replay_admitted"] is not False:
        raise ValueError("resource-only load unexpectedly admitted portable replay")
    phases, admission = stats["phase_seconds"], stats["admission"]
    if type(phases) is not dict or set(phases) != LOAD_PHASES:
        raise ValueError("invalid load phase timings")
    for name, value in phases.items():
        LEGACY.finite(value, name)
    if (phases["replay_admission"] != 0 or
            sum(value for name, value in phases.items() if name != "total") > phases["total"] + 1e-12 or
            phases["total"] > row["stages"]["load"]["seconds"] + 1e-12):
        raise ValueError("load phase timings exceed enclosing stage or imply replay admission")
    for name in LOAD_COUNTS:
        LEGACY.unsigned(stats[name], name)
    if (stats["local_records"] != row["local_input_entries"] or
            stats["local_reverse_records"] != row["local_input_entries"] or
            stats["local_record_capacity"] < stats["local_records"] or
            stats["local_reverse_capacity"] < stats["local_reverse_records"]):
        raise ValueError("resource load records/capacities do not cover owned source")
    if type(admission) is not dict or set(admission) != ADMISSION_COUNTS:
        raise ValueError("invalid portable admission counters")
    for name, value in admission.items():
        if LEGACY.unsigned(value, name) != 0:
            raise ValueError("resource-only load observed portable admission calls")
    payload = row["native_array_payload"]
    payload_fields = {"scope", "input", "scratch", "total_host_bytes", "total_device_bytes"}
    buffer_schema = row["schema_version"] == 5
    if buffer_schema:
        payload_fields.add("scratch_mode")
    if (type(payload) is not dict or set(payload) != payload_fields
            or payload["scope"] != ARRAY_SCOPE):
        raise ValueError("invalid native array payload schema or measurement scope")
    borrowed = buffer_schema and row["ranks"] > 1
    if buffer_schema and (payload["scratch_mode"] != (
            "borrowed-input-communication-buffer" if borrowed else "owned-register")
            or (borrowed and payload["scratch"] is not None)):
        raise ValueError("wrong native permutation scratch ownership")
    registers = [payload["input"]] if borrowed else [payload["input"], payload["scratch"]]
    for register in registers:
        if type(register) is not dict or set(register) != {"host_bytes", "device_bytes"}:
            raise ValueError("invalid native register array payload")
        for name, value in register.items():
            LEGACY.unsigned(value, name)
        # This harness requests CPU deployment; native MPI auto-deployment is
        # disabled at P=1. Distributed CPU Quregs own amplitude and MPI arrays.
        expected = row["local_native_amplitudes"] * 16 * (2 if row["ranks"] > 1 else 1)
        if register["host_bytes"] != expected or register["device_bytes"] != 0:
            raise ValueError("native array payload differs from controlled CPU deployment")
    for location in ("host", "device"):
        key = f"{location}_bytes"
        total = LEGACY.unsigned(payload[f"total_{key}"], f"total_{key}")
        if total != sum(register[key] for register in registers):
            raise ValueError("native array payload total mismatch")
    if payload["total_host_bytes"] > row["native_live_reserved_bytes"]:
        raise ValueError("native array payload exceeds conservative live reservation")


def capacity_evidence(rows, nodes, ranks_per_node, canonical_bytes):
    if nodes not in (1, 2, 4, 8) or ranks_per_node not in (1, 2, 4, 8) or nodes * ranks_per_node > 32:
        raise ValueError("unsupported node/rank placement")
    LEGACY.unsigned(canonical_bytes, "canonical input bytes", positive=True)
    if len(rows) != nodes * ranks_per_node or sorted(row["rank"] for row in rows) != list(range(len(rows))):
        raise ValueError("missing or duplicate rank coverage")
    groups = {}
    for row in rows:
        node = row["node"]
        if type(node) is not dict or set(node) != NODE_FIELDS:
            raise ValueError("invalid MPI shared-memory topology schema")
        for field in ("leader_rank", "local_rank", "local_size"):
            LEGACY.unsigned(node[field], field)
        host = node["processor_name"]
        if type(host) is not str or not 1 <= len(host.encode()) <= 256 or any(ord(c) < 32 for c in host):
            raise ValueError("invalid MPI processor name")
        groups.setdefault(node["leader_rank"], []).append(row)
    if len(groups) != nodes:
        raise ValueError("actual MPI shared-memory node count differs from requested placement")
    hosts = set()
    evidence = []
    for leader, members in sorted(groups.items()):
        names = {row["node"]["processor_name"] for row in members}
        if len(names) != 1 or hosts.intersection(names):
            raise ValueError("MPI node groups do not have distinct consistent hostnames")
        hosts.update(names)
        local_ranks = sorted(row["node"]["local_rank"] for row in members)
        if (len(members) != ranks_per_node or local_ranks != list(range(ranks_per_node)) or
                any(row["node"]["local_size"] != ranks_per_node for row in members)):
            raise ValueError("incomplete shared-memory rank coverage")
        owners = [row for row in members if row["node"]["local_rank"] == 0]
        if owners[0]["rank"] != leader:
            raise ValueError("shared-memory leader does not identify local rank zero")
        enforced_cap = sum(LEGACY.unsigned(row["process_address_space_cap_bytes"], "rank cap", True) for row in members)
        sample = owners[0]["node_memory_sampling"]
        if type(sample) is not dict or set(sample) != SAMPLE_FIELDS:
            raise ValueError("missing or invalid node memory sampling")
        for field in SAMPLE_FIELDS - {"max_sample_span_seconds"}:
            LEGACY.unsigned(sample[field], field, True)
        LEGACY.finite(sample["max_sample_span_seconds"], "sample span")
        if (sample["pids"] != ranks_per_node or sample["interval_milliseconds"] != 20 or
                sample["maximum_sum_rss_bytes"] > sample["maximum_sum_address_space_bytes"] or
                sample["maximum_sum_address_space_bytes"] > enforced_cap or
                any(row["node_memory_sampling"] is not None for row in members if row["rank"] != leader)):
            raise ValueError("node memory sampling exceeds verified process envelope or has duplicate observers")
        evidence.append(dict(processor_name=next(iter(names)), shared_memory_leader_rank=leader,
            ranks=sorted(row["rank"] for row in members),
            enforced_rank_process_address_space_cap_bytes=enforced_cap,
            sampled_maximum_sum_rank_rss_bytes=sample["maximum_sum_rss_bytes"],
            sampled_maximum_sum_rank_address_space_bytes=sample["maximum_sum_address_space_bytes"],
            whole_node_enforced_memory_cap_bytes=None, sampling=sample))
    # MPI topology and rank RLIMIT_AS receipts cannot certify a cap covering the
    # allocation coordinator, launchers, external helpers or their descendants.
    return dict(capacity_closed=False, whole_node_enforcement_verified=False,
                original_input_exceeds_rank_process_envelope=all(
                    canonical_bytes > node["enforced_rank_process_address_space_cap_bytes"]
                    for node in evidence),
                rank_process_envelope_scope="participating MPI rank processes only; excludes coordinator, launchers and external helpers; not a whole-node memory cap",
                canonical_stored_input_bytes=canonical_bytes, nodes=evidence)


THREADING_SCOPE = "native QuEST operations may use OpenMP; source generation, producer, loading, matching pair arithmetic and MPI routing remain serial"
THREADING_FIELDS = {"requested_threads", "omp_stack_bytes_per_worker", "omp_stack_allowance_bytes",
    "native_environment_multithreaded", "native_register_multithreaded", "baseline_process_threads",
    "prepared_process_threads", "final_process_threads", "omp_num_threads", "omp_places", "omp_proc_bind",
    "omp_dynamic", "omp_stacksize", "native_openmp_team_size", "scope"}


def validate_threading(row, threads, omp_stack_bytes, caps):
    report = row["threading"]
    if type(report) is not dict or set(report) != THREADING_FIELDS:
        raise ValueError("invalid threading receipt schema")
    expected = dict(requested_threads=threads, omp_stack_bytes_per_worker=omp_stack_bytes,
        omp_stack_allowance_bytes=(threads - 1) * omp_stack_bytes,
        native_environment_multithreaded=threads > 1, native_register_multithreaded=threads > 1,
        omp_num_threads=str(threads), omp_places="cores", omp_proc_bind="close", omp_dynamic="FALSE",
        omp_stacksize=f"{omp_stack_bytes}B", native_openmp_team_size=None, scope=THREADING_SCOPE)
    if any(type(report[key]) is not type(value) or report[key] != value for key, value in expected.items()):
        raise ValueError("requested/actual native threading configuration mismatch")
    for field in ("baseline_process_threads", "prepared_process_threads", "final_process_threads"):
        LEGACY.unsigned(report[field], field, True)
    if row["baseline_address_space_bytes"] + caps[1] + report["omp_stack_allowance_bytes"] > caps[0]:
        raise ValueError("MPI baseline, managed memory and OpenMP stacks exceed process cap")


def launch_command(launcher, executable, directory, dimension, repetitions, caps, nodes, ranks_per_node,
                   threads=1, omp_stack_bytes=8 * 1024**2):
    ranks = nodes * ranks_per_node
    if pathlib.Path(launcher).name == "srun":
        if ranks_per_node != 1:
            raise ValueError("Slurm capacity profile requires exactly one MPI rank per node")
        command = [launcher, f"--nodes={nodes}", f"--ntasks={ranks}",
                   "--ntasks-per-node=1", f"--cpus-per-task={threads}", "--exclusive",
                   "--hint=nomultithread", "--distribution=block:block",
                   "--kill-on-bad-exit=1", "--cpu-bind=cores"]
    else:
        if nodes != 1:
            raise ValueError("multi-node campaigns require srun placement")
        command = [launcher, "-n", str(ranks)]
    return command + [sys.executable, str(pathlib.Path(__file__).resolve()), "--rank-wrapper",
        str(caps[0]), str(executable), str(directory), str(dimension), str(repetitions),
        *(str(cap) for cap in caps), str(nodes), str(ranks_per_node), str(threads), str(omp_stack_bytes)]


def summarize_case(directory, ranks, n, repetitions, caps, nodes, ranks_per_node,
                   threads=None, omp_stack_bytes=8 * 1024**2):
    rows = [LEGACY.read_receipt(directory / f"rank-{rank}.json") for rank in range(ranks)]
    canonical = 64 * n
    base_rows = []
    resource_version = rows[0].get("schema_version") if rows and type(rows[0]) is dict else None
    resource_only = resource_version in (4, 5)
    version = resource_version if resource_only else (3 if threads is not None else 2)
    fields = LEGACY.RECEIPT_FIELDS | EXTRA_FIELDS | ({"threading"} if threads is not None else set())
    if resource_only:
        fields |= RESOURCE_FIELDS
    for expected_rank, row in enumerate(rows):
        if type(row) is not dict or set(row) != fields or type(row["schema_version"]) is not int or row["schema_version"] != version:
            raise ValueError("invalid or mixed capacity receipt schema")
        if LEGACY.unsigned(row["rank"], "rank") != expected_rank:
            raise ValueError("rank receipt does not match its owned path")
        if (row["canonical_input_bytes"] != canonical or
                row["local_canonical_input_bytes"] != row["local_input_entries"] * 32):
            raise ValueError("canonical sparse input byte count mismatch")
        for field in ("canonical_input_bytes", "local_canonical_input_bytes", "persisted_encoding_file_bytes"):
            LEGACY.unsigned(row[field], field, field != "persisted_encoding_file_bytes")
        LEGACY.finite(row["input_storage_seconds"], "input storage seconds")
        source = directory / f"input-rank-{row['rank']}.coo32"
        stat = source.stat()
        if source.is_symlink() or stat.st_size != row["local_canonical_input_bytes"] or stat.st_blocks * 512 < stat.st_size:
            raise ValueError("input shard is missing, truncated, sparse, or compressed")
        digest = hashlib.sha256()
        with source.open("rb") as stream:
            while chunk := stream.read(65536):
                digest.update(chunk)
        if digest.hexdigest() != row["local_input_sha256"]:
            raise ValueError("stored sparse input hash mismatch")
        if threads is not None:
            validate_threading(row, threads, omp_stack_bytes, caps)
        base = {key: value for key, value in row.items() if key in LEGACY.RECEIPT_FIELDS}
        base_rows.append(base)
    summary = LEGACY.summarize(base_rows, ranks, n, repetitions, *caps,
        max_ranks=32, max_dimension=LEGACY.MAX_WORD // 256, ranks_per_node=ranks_per_node, resource_only=resource_only)
    if resource_only:
        for row in rows:
            validate_resource_load(row)
        summary["loading_route"] = "resource-only-native"
        summary["portable_replay_admission"] = "unrun"
        summary["load_phase_max_seconds"] = {phase: max(row["load_statistics"]["phase_seconds"][phase] for row in rows) for phase in LOAD_PHASES}
        summary["load_reverse_broadcasts"] = sum(row["load_statistics"]["reverse_broadcasts"] for row in rows)
    summary.update(capacity_evidence(rows, nodes, ranks_per_node, canonical))
    summary["rank_receipts"] = rows
    summary["persisted_encoding_file_bytes"] = sum(row["persisted_encoding_file_bytes"] for row in rows)
    for node in summary["nodes"]:
        node["sum_rank_rss_high_water_bound_bytes"] = sum(rows[r]["rss_high_water_bytes"] for r in node["ranks"])
    return summary


def file_sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(65536):
            digest.update(chunk)
    return digest.hexdigest()


def main():
    if any(name in os.environ for name in ("SLURM_JOB_ID", "SLURM_JOBID", "SLURM_STEP_ID", "SLURM_PROCID")):
        raise SystemExit(SLURM_UNSUPPORTED)
    if len(sys.argv) > 1 and sys.argv[1] == "--rank-wrapper":
        cap = int(sys.argv[2])
        if os.environ.get("SLURM_PROCID") == "0" and "QUEST_CAPACITY_STEP_TOKEN" in os.environ:
            print(f'QUEST_CAPACITY_STEP_{os.environ["QUEST_CAPACITY_STEP_TOKEN"]}={os.environ["SLURM_JOB_ID"]}.{os.environ["SLURM_STEP_ID"]}', file=sys.stderr, flush=True)
        resource.setrlimit(resource.RLIMIT_AS, (cap, cap))
        threads, stack = int(sys.argv[-2]), int(sys.argv[-1])
        os.environ.update(OMP_NUM_THREADS=str(threads), OMP_PLACES="cores", OMP_PROC_BIND="close",
                          OMP_DYNAMIC="FALSE", OMP_STACKSIZE=f"{stack}B")
        if "SLURM_CPUS_PER_TASK" in os.environ:
            os.environ["SRUN_CPUS_PER_TASK"] = os.environ["SLURM_CPUS_PER_TASK"]
        os.execv(sys.argv[3], sys.argv[3:])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--launcher", default="mpiexec", help="local MPI launcher; srun execution is unsupported")
    parser.add_argument("--nodes", type=int, default=1, help="local execution requires one physical node")
    parser.add_argument("--ranks-per-node", type=int, default=1, help="local MPI allows 1/2/4/8")
    parser.add_argument("--threads", type=int, default=None, help="OpenMP threads; allocated Slurm CPUs (288 if unset), or one locally")
    parser.add_argument("--omp-stack-mib", type=int, default=8)
    parser.add_argument("--start-dimension", type=int, default=1024)
    parser.add_argument("--max-dimension", type=int, default=1 << 29)
    parser.add_argument("--max-cases", type=int, default=20)
    parser.add_argument("--repetitions", type=int, default=2)
    parser.add_argument("--process-as-mib", type=int, default=8192)
    parser.add_argument("--model-rank-mib", type=int, default=4096)
    parser.add_argument("--timeout", type=int, default=3600)
    args = parser.parse_args()
    if pathlib.Path(args.launcher).name == "srun":
        raise SystemExit(SLURM_UNSUPPORTED)
    slurm = pathlib.Path(args.launcher).name == "srun"
    if args.threads is None:
        args.threads = int(os.environ.get("SLURM_CPUS_PER_TASK", "288")) if slurm else 1
    if slurm:
        os.environ["SRUN_CPUS_PER_TASK"] = os.environ.get("SLURM_CPUS_PER_TASK", str(args.threads))
    if (not 1 <= args.threads <= 1024 or not 1 <= args.omp_stack_mib <= 1024 or
            (slurm and args.ranks_per_node != 1) or args.nodes not in (1, 2, 4, 8) or args.ranks_per_node not in (1, 2, 4, 8) or
            args.nodes * args.ranks_per_node > 32 or not 1 <= args.max_cases <= 32 or
            not 1 <= args.repetitions <= 8 or not 1 <= args.timeout <= 86400 or
            not 0 < args.model_rank_mib < args.process_as_mib <= 1048576 or
            any(n < 16 or n & (n - 1) or n > LEGACY.MAX_WORD // 256
                for n in (args.start_dimension, args.max_dimension)) or
            args.start_dimension > args.max_dimension):
        parser.error("invalid bounded campaign dimensions, placement, caps, or repetitions")
    caps = [args.process_as_mib << 20, args.model_rank_mib << 20,
            (args.model_rank_mib * args.ranks_per_node) << 20]
    executable = args.executable.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=False)
    campaign = dict(schema_version=5, complete=False, capacity_closed=False,
        whole_node_enforcement_verified=False, cases=[],
        cap_scope=__doc__.strip(), requested_nodes=args.nodes, ranks_per_node=args.ranks_per_node,
        requested_threads_per_rank=args.threads, omp_stack_bytes_per_worker=args.omp_stack_mib << 20,
        canonical_input_format="little-endian row:u64,column:u64,real:f64,imaginary:f64; no ordinals or padding",
        process_address_space_cap_bytes=caps[0], model_rank_budget_bytes=caps[1],
        executable_sha256=file_sha256(executable))
    n = args.start_dimension
    try:
        for ordinal in range(args.max_cases):
            directory = (args.output / f"case-{ordinal}-n{n}").resolve()
            directory.mkdir()
            command = launch_command(args.launcher, executable, directory, n, args.repetitions,
                                     caps, args.nodes, args.ranks_per_node, args.threads, args.omp_stack_mib << 20)
            seconds = run_job(command, args.timeout, directory / "launcher.log")
            result = summarize_case(directory, args.nodes * args.ranks_per_node, n,
                                    args.repetitions, caps, args.nodes, args.ranks_per_node, args.threads, args.omp_stack_mib << 20)
            result["launcher_seconds"] = seconds
            campaign["cases"].append(result)
            print(json.dumps({k: v for k, v in result.items() if k != "rank_receipts"}), flush=True)
            if result["capacity_closed"]:
                campaign["capacity_closed"] = True
                break
            if n >= args.max_dimension:
                break
            n *= 2
        if file_sha256(executable) != campaign["executable_sha256"]:
            raise ValueError("native executable changed during the campaign")
        campaign["complete"] = True
        if not campaign["capacity_closed"]:
            campaign["capacity_status"] = "open: whole-node memory enforcement has not been verified"
        else:
            campaign["capacity_status"] = "closed under verified whole-node memory enforcement"
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
        campaign["failure"] = str(error)
        campaign["failed_dimension"] = n
        campaign["capacity_status"] = "open: construction, admission, execution, or verification did not complete"
        raise
    finally:
        (args.output / "completion.json").write_text(json.dumps(campaign, indent=2, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
