#!/usr/bin/env python3
"""Pinned benchmark inventory, exact fixture transport, and terminal accounting.

No numerical operations occur during discovery. Historical 1,488-row runs are
independent artifacts and never counted as this campaign's measurements.
"""
import argparse
from collections import Counter
import hashlib
import json
import math
import fcntl
import os
import re
import signal
import subprocess
import time
from pathlib import Path
import struct

CORPUS_SHA256 = "806b1557498debe5d409b214609ad30a74a86d529ae36045a2b3c428b9546e6d"
SOURCE_REVISIONS = {
    "rust_original": "0f95954e7951dca9a8d88a10c1a9764ecb58d91f",
    "quest_qsvt": "4fc35983138d07a990862a4d83ad16f2b737c98f",
    "softwarex": "245938a575deac7f232797efdade97ba442a366d",
}
INVERSE_ORDERS = (5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000,
                  50000, 100000, 200000, 500000, 1000000)
SOLVERS = tuple(f"random_basis_{basis}_d200" for basis in
                ("monomial", "chebyshev", "hermite", "laguerre", "jacobi", "laurent")) + (
    "industrial_inverse_default_kappa2_degree15", "industrial_filter_chebyshev_d127",
    "industrial_heat_chebyshev_d127")
TERMINAL = frozenset(("ok", "unsupported", "timeout", "accuracy_failure", "native_error",
                      "resource_limit", "interrupted", "preparation_failure"))


def softwarex_timeout(degree, pipeline):
    base = 90 if pipeline == "solver" else 120
    return min(14400, max(base, base + max(0, degree) // 80))


def source_inventory(cases):
    rows = []
    def add(suite, name, degree, scope, **extra):
        rows.append(dict(suite=suite, case_id=f"{suite}/{name}", degree=degree, scope=scope,
                         timeout_seconds=14400, protocol="criterion_10", **extra))
    for degree in INVERSE_ORDERS:
        add("named_inverse", f"order{degree}", degree, "inverse_only", source_preparation=(
            "weiss_mt19937" if degree <= 1000 else "forward_bounded_reflections"))
    for name in SOLVERS:
        degree = 200 if name.startswith("random") else 15 if "inverse" in name else 127
        add("named_solver", name, degree, "full_pipeline", source_boundary="named_solver")
    for case in cases:
        for pipeline, scope in (("solver", "full_pipeline"), ("kernel", "validated_roundtrip")):
            add("softwarex", f"{case['case_id']}/{pipeline}", case["degree"], scope,
                source_boundary=f"softwarex_{pipeline}", source_case_id=case["case_id"])
            rows[-1]["timeout_seconds"] = softwarex_timeout(case["degree"], pipeline)
    for name in ("root_free", "split", "repeated", "flat", "close"):
        add("root_isolation", name, None, "isolate_roots")
    for name in ("direct512", "with_roots256"):
        add("roots_of_unity", name, 255, "evaluate")
    for degree in (1, 16, 64, 128):
        for stage in ("construct", "freeze", "cold_execute", "prepare", "warm_execute"):
            add("circuit", f"degree{degree}/{stage}", degree, stage)
    for dimension in (64, 128, 256, 512):
        add("unitary", f"dimension{dimension}", None, "admit", dimension=dimension)
    for protocol in ("fixed_3", "criterion_10"):
        add("coordinate", f"degree8105/{protocol}", 8105, "prepare")
        rows[-1]["protocol"] = protocol
    return rows


def sha256(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def load_corpus(path):
    if sha256(path) != CORPUS_SHA256:
        raise ValueError("SoftwareX frozen corpus SHA-256 mismatch")
    cases = json.loads(Path(path).read_text())["cases"]
    if len(cases) != 62 or len({case["case_id"] for case in cases}) != 62:
        raise ValueError("SoftwareX corpus must contain 62 unique cases")
    return cases


def coefficient_payload(case):
    """IEEE754 transport retains every source/canonical coefficient and offset."""
    result = {key: case[key] for key in ("case_id", "degree", "source_basis", "source_parameters",
                                       "source_minimum_order", "canonical_minimum_order")}
    for prefix in ("source", "canonical"):
        real = case[f"{prefix}_coefficients_real"]
        imag = case[f"{prefix}_coefficients_imag"]
        if len(real) != len(imag) or not real:
            raise ValueError("malformed coefficient support")
        if not all(math.isfinite(x) for x in real + imag):
            raise ValueError("nonfinite coefficient")
        result[f"{prefix}_coefficients"] = list(zip(real, imag))
        result[f"{prefix}_coefficients_bits"] = [[struct.unpack("<Q", struct.pack("<d", re))[0],
                                                  struct.unpack("<Q", struct.pack("<d", im))[0]] for re, im in zip(real, imag)]
        exact = b"".join(struct.pack("<dd", re, im) for re, im in zip(real, imag))
        result[f"{prefix}_ieee754_sha256"] = hashlib.sha256(exact).hexdigest()
    return result


def atomic_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")
    temporary.replace(path)


def completion_summary(root, manifest, results):
    expected = [row["id"] for row in manifest]
    actual = [row["id"] for row in results]
    if len(set(expected)) != len(expected) or len(set(actual)) != len(actual):
        raise ValueError("duplicate manifest/result identity")
    if set(expected) != set(actual) or any(row.get("status") not in TERMINAL for row in results):
        raise ValueError("every expected row requires exactly one terminal result")
    expected_by_id = {row["id"]: row for row in manifest}
    for row in results:
        minimum_samples = 3 if expected_by_id[row["id"]].get("protocol") == "fixed_3" else 10
        if row["status"] == "ok" and not row.get("raw_samples"):
            raise ValueError("successful measurement requires raw samples")
        for artifact in row.get("raw_samples", []):
            path = (root / artifact["path"]).resolve()
            if not path.is_relative_to(root.resolve()) or not path.is_file() or sha256(path) != artifact["sha256"]:
                raise ValueError("raw sample artifact missing, changed, or outside campaign")
            sample = json.loads(path.read_text())
            times, iterations = sample.get("times", []), sample.get("iters", [])
            if len(times) < minimum_samples or len(times) != len(iterations) or not all(math.isfinite(x) and x > 0 for x in times + iterations):
                raise ValueError("measurement needs the protocol minimum of finite positive samples")
        if row.get("peak_rss_log"):
            rss = (root / row["peak_rss_log"]).resolve()
            if not rss.is_relative_to(root.resolve()) or not rss.is_file() or sha256(rss) != row["peak_rss_log_sha256"]:
                raise ValueError("memory artifact missing, changed, or outside campaign")
            expected_rss = None if row["status"] in {"timeout", "interrupted"} else read_peak_rss(rss)
            if row.get("peak_rss_kib") != expected_rss:
                raise ValueError("memory observation differs from its raw artifact")
        elif row.get("peak_rss_kib") is not None:
            raise ValueError("memory observation requires a raw artifact")
        if "log" in row and sha256(root / row["log"]) != row["log_sha256"]:
            raise ValueError("measurement log changed")
    summary = dict(accounting_complete=True,
                   performance_coverage_complete=all(row["status"] == "ok" for row in results),
                   expected_rows=len(expected), outcomes=dict(Counter(row["status"] for row in results)))
    return summary


def publish_completion(root, manifest, results):
    summary = completion_summary(root, manifest, results)
    atomic_json(root / "completion.json", summary)
    return summary


def prepare(corpus, output):
    cases = load_corpus(corpus)
    output.mkdir(parents=True, exist_ok=False)
    fixtures = output / "fixtures"
    fixtures.mkdir()
    hashes = {}
    shapes = {}
    for case in cases:
        path = fixtures / (case["case_id"] + ".json")
        atomic_json(path, coefficient_payload(case))
        hashes[case["case_id"]] = sha256(path)
        shapes[case["case_id"]] = len(case["canonical_coefficients_real"])
    inventory = source_inventory(cases)
    for row in inventory:
        case = row.get("source_case_id")
        row["input_sha256"] = hashes.get(case)
        row["coefficient_count"] = shapes.get(case)
        row["canonical_degree"] = shapes[case] - 1 if case else None
        row["input_status"] = "frozen" if case else "requires_source_export"
    atomic_json(output / "inventory.json", dict(sources=SOURCE_REVISIONS,
                softwarex_corpus_sha256=CORPUS_SHA256, workloads=inventory,
                historical_1488_campaign_included=False))
    return inventory


def classify_run(returncode, log, phase):
    if any(marker in log for marker in ("accuracy_failure", "does not establish requested tolerance", "NotEstablished", "Error: Contractivity")):
        return "accuracy_failure"
    if any(word in log for word in ("resource limit", "budget exceeded", "Budget(", "Budget {", "Disk quota exceeded", "Cannot allocate memory")):
        return "resource_limit"
    if returncode != 0:
        return "preparation_failure" if phase == "preparation" else "native_error"
    return "ok" if phase == "complete" and "QUEST_BENCH_PREFLIGHT" in log else "native_error"


def effective_memory_limits():
    cgroup = next(line.split(":", 2)[2].strip() for line in Path("/proc/self/cgroup").read_text().splitlines() if line.startswith("0::"))
    root = Path("/sys/fs/cgroup") / cgroup.lstrip("/")
    limits = {name: (root / name).read_text().strip() for name in ("memory.high", "memory.max")}
    if limits["memory.high"] == "max" or limits["memory.max"] == "max":
        raise RuntimeError("campaign requires an explicit systemd memory scope")
    if int(limits["memory.high"]) > 24 * 2**30 or int(limits["memory.max"]) > 28 * 2**30:
        raise RuntimeError("campaign memory limits exceed 24/28 GiB")
    return limits


def rust_manifest(inventory, implementation):
    rows = []
    for source in inventory:
        if source["suite"] != "softwarex":
            continue
        scopes = [source["scope"]]
        if source["source_boundary"] == "softwarex_kernel":
            scopes += ["inverse_only", "forward_only", "completion_inverse"]
        for scope in scopes:
            row = dict(source, scope=scope, implementation=implementation, backend="rustfft_scalar", workers=1,
                       protocol="criterion_10", sample_count=10, warmup_seconds=0.1, measurement_seconds=0.2,
                       input_representation="frozen canonical coefficients",
                       requested_accuracy=1e-12 if source["degree"] <= 10000 else 1e-10)
            row["effective_completion_tolerance"] = row["requested_accuracy"] / 8 if scope == "full_pipeline" else row["requested_accuracy"]
            row["preflight_tolerance"] = row["requested_accuracy"]
            row["accuracy_metric"] = "scattering_pair_coefficient_linf"
            row["production_limits"] = {"max_len": 1048576, "max_bytes": 536870912, "max_work": 68719476736}
            row["source_weiss_limits"] = {"max_grid": 1048576, "max_bytes": 1073741824, "max_work": 8589934592, "max_iterations": 10}
            row["source_boundary_equivalent"] = False
            row["comparison_contract"] = "Rust production stages on frozen canonical coefficients; original source solver/kernel boundaries remain separate"
            row["id"] = f"{implementation}/{source['case_id']}/{scope}"
            rows.append(row)
    return rows


def source_identity(workspace):
    files = [workspace / name for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml", ".config/nextest.toml") if (workspace / name).exists()]
    for crate in ("quest-qsp", "quest-numerics", "quest-polynomial", "vendor/mathcore"):
        directory = workspace / "crates" / crate
        files.extend(directory.rglob("*.rs"))
        files.extend(directory.rglob("Cargo.toml"))
    inventory = {str(path.relative_to(workspace)): sha256(path) for path in sorted(set(files))}
    digest = hashlib.sha256(json.dumps(inventory, sort_keys=True).encode()).hexdigest()
    return {"sha256": digest, "files": inventory}


def verify_source_identity(workspace, expected, output, row_id):
    actual = source_identity(workspace)
    if actual["sha256"] != expected["sha256"]:
        atomic_json(output / "infrastructure-failure.json", {"id": row_id, "status": "interrupted",
                    "phase": "source_verification", "expected_sha256": expected["sha256"],
                    "actual_sha256": actual["sha256"]})
        raise RuntimeError("measured source changed during campaign; preserve partial results and start a new lane")


RSS_SCOPE = "largest child process peak RSS across build/discovery/preflight/measurement"


def time_command(command, path):
    executable = Path("/usr/bin/time")
    return [str(executable), "-f", "QUEST_BENCH_MAX_RSS_KIB=%M", "-o", str(path), "--", *command] if executable.is_file() else command


def read_peak_rss(path):
    if not path.is_file():
        return None
    matches = re.findall(r"^QUEST_BENCH_MAX_RSS_KIB=(\d+)$", path.read_text(), re.MULTILINE)
    return int(matches[-1]) if matches else None


def stop_process_group(process):
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        code = process.wait(timeout=5)
    except (subprocess.TimeoutExpired, KeyboardInterrupt):
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        code = process.wait()
    # A root process can exit before children that ignore SIGTERM. Terminate
    # any remaining group members before releasing the shared build lock.
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    return code


def wait_bounded(process, timeout):
    """Return (exit code, timeout/interrupted/None) only after owned children stop."""
    try:
        return process.wait(timeout=timeout), None
    except subprocess.TimeoutExpired:
        return stop_process_group(process), "timeout"
    except KeyboardInterrupt:
        return stop_process_group(process), "interrupted"
    except BaseException:
        stop_process_group(process)
        raise


def run_rust(source, output, workspace, target, implementation):
    memory = effective_memory_limits()
    inventory = json.loads((source / "inventory.json").read_text())["workloads"]
    manifest = rust_manifest(inventory, implementation)
    output.mkdir(parents=True, exist_ok=False)
    atomic_json(output / "manifest.json", dict(memory_limits=memory, sources=SOURCE_REVISIONS, rows=manifest))
    tools = {}
    for label, command in (("rustc", ["rustc", "-vV"]), ("nextest", ["cargo", "nextest", "--version"])):
        tools[label] = subprocess.run(command, cwd=workspace, capture_output=True, text=True, check=True).stdout.strip()
    config = Path.home() / ".cargo/config.toml"
    tools["global_cargo_config_sha256"] = sha256(config) if config.exists() else None
    tools["cpu_model"] = next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")), None)
    tools["environment"] = {key: os.environ.get(key) for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RAYON_NUM_THREADS", "OMP_NUM_THREADS", "OMP_PROC_BIND")}
    atomic_json(output / "toolchain.json", tools)
    identity = source_identity(workspace)
    atomic_json(output / "source-identity.json", identity)
    results = []
    for number, row in enumerate(manifest):
        verify_source_identity(workspace, identity, output, row["id"])
        stem = f"{number:04d}"
        log_path = output / (stem + ".log")
        phase_path = output / (stem + ".phase")
        rss_path = output / (stem + ".rss")
        input_path = source / "fixtures" / (row["source_case_id"] + ".json")
        if sha256(input_path) != row["input_sha256"]:
            raise ValueError("exact benchmark input changed after manifest creation")
        env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="4",
                   QUEST_BENCH_INPUT=str(input_path), QUEST_BENCH_PHASE=str(phase_path),
                   QUEST_BENCH_SCOPE=row["scope"], CRITERION_HOME=str(output / stem / "criterion"))
        command = ["cargo", "nextest", "bench", "-p", "quest-qsp", "--bench", "replication",
                   "--features", "benchmark-support", "--offline", "-P", "replication", "-E", f"test(={row['scope']})"]
        # Waiting for other workspace builds does not consume the case deadline.
        with open("/tmp/quest-quality-build.lock", "a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            verify_source_identity(workspace, identity, output, row["id"])
            started = time.monotonic()
            with log_path.open("w") as log:
                process = subprocess.Popen(time_command(command, rss_path), cwd=workspace, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                returncode, stopped = wait_bounded(process, row["timeout_seconds"])
            verify_source_identity(workspace, identity, output, row["id"])
        elapsed = time.monotonic() - started
        phase = phase_path.read_text() if phase_path.exists() else "build_or_discovery"
        log = log_path.read_text(errors="replace")
        if phase == "warmup" and "Collecting" in log:
            phase = "measurement"
        status = stopped or classify_run(returncode, log, phase)
        accuracy = re.search(r"QUEST_BENCH_PREFLIGHT requested=(\S+) achieved=(\S+)", log)
        result = dict(id=row["id"], status=status, phase=phase, returncode=returncode,
                      elapsed_seconds=elapsed, peak_rss_kib=read_peak_rss(rss_path) if stopped is None else None,
                      peak_rss_scope=RSS_SCOPE, peak_rss_unit="KiB",
                      peak_rss_log=rss_path.name if rss_path.is_file() else None,
                      peak_rss_log_sha256=sha256(rss_path) if rss_path.is_file() else None,
                      log=log_path.name, log_sha256=sha256(log_path),
                      requested_accuracy=row["requested_accuracy"],
                      achieved_accuracy=float(accuracy[2]) if accuracy else None)
        if status == "ok":
            samples = list((output / stem).rglob("sample.json"))
            if not samples:
                result.update(status="native_error", reason="Criterion raw samples missing")
            else:
                result["raw_samples"] = [{"path": str(p.relative_to(output)), "sha256": sha256(p)} for p in samples]
        if phase == "build_or_discovery":
            atomic_json(output / "infrastructure-failure.json", result)
            raise RuntimeError("build or discovery failed before workload execution; campaign remains incomplete")
        results.append(result)
        with (output / "results.jsonl").open("a") as stream:
            stream.write(json.dumps(result, allow_nan=False) + "\n")
        print(f"[{number + 1}/{len(manifest)}] {result['status']} {row['id']}", flush=True)
        if stopped == "interrupted":
            raise KeyboardInterrupt
    verify_source_identity(workspace, identity, output, "completion")
    return publish_completion(output, manifest, results)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    materialize = sub.add_parser("prepare")
    materialize.add_argument("--corpus", type=Path, required=True)
    materialize.add_argument("--output", type=Path, required=True)
    runner = sub.add_parser("run-rust")
    for name in ("source", "output", "workspace", "target"):
        runner.add_argument("--" + name, type=Path, required=True)
    runner.add_argument("--implementation", required=True)
    args = parser.parse_args()
    if args.action == "prepare":
        print(json.dumps({"workloads": len(prepare(args.corpus, args.output))}))
    elif args.action == "run-rust":
        print(json.dumps(run_rust(args.source.resolve(), args.output.resolve(), args.workspace.resolve(),
                                  args.target.resolve(), args.implementation)))


if __name__ == "__main__":
    main()
