#!/usr/bin/env python3
"""Account for the remaining source suites and measure supported Rust contracts.

The Rust numerical Gram-admission measurements are a separate comparison, not
replicas of the C++ outward-certified unitary proof. Unsupported source contracts
never acquire replacement timings.
"""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import struct
import subprocess
import time

SPEC = importlib.util.spec_from_file_location("campaign", Path(__file__).parents[1] / "campaign.py")
campaign = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(campaign)

SOURCE_FILES = {
    "root_isolation": "test/qsp_tools/critical_root_isolation_benchmarks.cpp",
    "roots_of_unity": "test/qsp_tools/unit_tests_poly.cpp",
    "circuit": "test/qsvt_tools/benchmarks/circuit_benchmarks.cpp",
    "unitary": "test/qsvt_tools/benchmarks/circuit_benchmarks.cpp",
    "coordinate": "test/qsvt_tools/benchmarks/circuit_benchmarks.cpp",
}
MISSING = {
    "root_isolation": "Rust roots::cover has no source max_depth or independent per-item iteration and queue limits; its covered-box contract also differs from source certified root-containing intervals.",
    "roots_of_unity": "Source Eigen::VectorXd::Random(256) coefficients are not exported or frozen; neither roots-of-unity production entry point exists in quest-polynomial. A same-seed surrogate or handwritten replacement loop is not an exact source adapter.",
    "circuit": "Rust has no source-equivalent arbitrary ProjectorPhase expression DAG, freeze API, or prepared expression executor. TransformBuilder implements a different staged QSVT contract, including projection admission.",
    "unitary": "Rust NumericalOperator admits a floating-point Gram residual, not the source outward-certified scalar-dot proof with UnitaryProofWork counters. Exact identity inputs can be measured separately, without proof-boundary equivalence.",
    "coordinate": "Rust has no source-equivalent degree-8105 ProjectorReflection expression DAG or prepared-circuit native-call/storage counters. A synthesized transform or direct Z gate would change the preparation workload.",
}
SOURCE_LINES = {"root_isolation": 39, "roots_of_unity": 769, "circuit": 395,
                "unitary": 321, "coordinate": 356}


def identity_hash(dimension):
    digest = hashlib.sha256()
    for row in range(dimension):
        for column in range(dimension):
            digest.update(struct.pack("<dd", float(row == column), 0.0))
    return digest.hexdigest()


def source_manifest():
    rows = []
    for original in campaign.source_inventory([]):
        suite = original["suite"]
        if suite not in SOURCE_FILES:
            continue
        row = dict(original, id="quest_qsvt/" + original["case_id"],
                   row_kind="source_contract_diagnostic",
                   implementation="quest_qsvt", backend="source_defined", workers="source_defined",
                   source_boundary_equivalent=True, missing_rust_contract=MISSING[suite],
                   source_location=f"{SOURCE_FILES[suite]}:{SOURCE_LINES[suite]}",
                   input_status="source_defined_not_exported", input_sha256=None)
        if suite == "unitary":
            row.update(input_status="exact_identity", input_sha256=identity_hash(row["dimension"]),
                       input_encoding="row-major complex binary64 little endian", requested_accuracy=0.0)
        rows.append(row)
    return rows


def unitary_manifest(implementation):
    return [dict(id=f"{implementation}/numerical_unitary/dimension{dimension}",
                 suite="numerical_unitary_comparison", source_case_id=f"unitary/dimension{dimension}",
                 dimension=dimension, scope="numerical_storage_and_gram_admission",
                 implementation=implementation, backend="faer_binary64", workers=1,
                 protocol="criterion_10", sample_count=10, warmup_seconds=0.1,
                 measurement_seconds=0.2, timeout_seconds=14400, requested_accuracy=0.0,
                 source_boundary_equivalent=False,
                 comparison_contract="Same exact source identity matrix and zero residual tolerance; Rust numerical storage plus Gram admission, without C++ outward proof or scalar-work counters",
                 input_sha256=identity_hash(dimension), input_encoding="row-major complex binary64 little endian")
            for dimension in (64, 128, 256, 512)]


def check_native_failure(log):
    if not all(part in log for part in ("autodiff", "1.1.2", "CMake Error")):
        raise ValueError("expected a verified CMake failure for required autodiff 1.1.2")


def reported_phase(phase, log):
    return "measurement" if phase == "warmup" and "Collecting" in log else phase


def source_blocked(source, log, output):
    memory = campaign.effective_memory_limits()
    revision = subprocess.run(["git", "rev-parse", "HEAD"], cwd=source, check=True,
                              capture_output=True, text=True).stdout.strip()
    if revision != campaign.SOURCE_REVISIONS["quest_qsvt"]:
        raise ValueError("quest-qsvt revision differs from pinned source")
    clean = subprocess.run(["git", "diff", "--quiet", "HEAD", "--", *sorted(set(SOURCE_FILES.values()))],
                           cwd=source, check=False).returncode
    if clean != 0:
        raise ValueError("pinned benchmark source files have local modifications")
    check_native_failure(log.read_text())
    output.mkdir(parents=True, exist_ok=False)
    shutil.copyfile(log, output / "native-configure.log")
    log_hash = campaign.sha256(output / "native-configure.log")
    manifest = source_manifest()
    for row in manifest:
        row["source_file_sha256"] = campaign.sha256(source / SOURCE_FILES[row["suite"]])
    campaign.atomic_json(output / "manifest.json", dict(memory_limits=memory, sources=campaign.SOURCE_REVISIONS,
                         purpose="source_contract_diagnostics", additional_workload_count=0, rows=manifest))
    results = [dict(id=row["id"], status="preparation_failure", phase="native_configuration",
                    reason="Unmodified source configure requires unavailable autodiff 1.1.2; no fixture or benchmark was executed",
                    missing_rust_contract=row["missing_rust_contract"],
                    log="native-configure.log", log_sha256=log_hash)
               for row in manifest]
    (output / "results.jsonl").write_text("".join(json.dumps(row) + "\n" for row in results))
    summary = campaign.publish_completion(output, manifest, results)
    summary.update(purpose="source_contract_diagnostics", additional_workload_count=0)
    campaign.atomic_json(output / "completion.json", summary)
    return summary


def source_identity(workspace):
    paths = [workspace / "Cargo.toml", workspace / "Cargo.lock"]
    # Include every production module, also untracked files: indirect dependency
    # changes matter. Test/example-only edits do not change this executable.
    for manifest in (workspace / "crates").rglob("Cargo.toml"):
        paths.extend((manifest.parent / "src").rglob("*.rs"))
        if (manifest.parent / "build.rs").is_file():
            paths.append(manifest.parent / "build.rs")
    paths.extend((workspace / "crates").rglob("Cargo.toml"))
    paths.append(workspace / "crates/quest-compile/benches/source_unitary.rs")
    files = {str(path.relative_to(workspace)): campaign.sha256(path) for path in sorted(set(paths))}
    return dict(sha256=hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(), files=files)


def verify_controllers(directory, expected):
    actual = {name: campaign.sha256(directory / name)
              for name in ("source_suites/run.py", "campaign.py")}
    if actual != expected:
        raise RuntimeError("benchmark controller or imported helper changed after pinning")


def run_unitary(workspace, target, output, implementation, controller_pin):
    memory = campaign.effective_memory_limits()
    controllers = json.loads(controller_pin.read_text())
    controller_directory = Path(__file__).resolve().parents[1]
    verify_controllers(controller_directory, controllers["files"])
    manifest = unitary_manifest(implementation)
    output.mkdir(parents=True, exist_ok=False)
    campaign.atomic_json(output / "controller-identity.json", controllers)
    campaign.atomic_json(output / "manifest.json", dict(memory_limits=memory, sources=campaign.SOURCE_REVISIONS, rows=manifest))
    versions = {" ".join(command): subprocess.run(command, check=True, capture_output=True,
                                                  text=True, timeout=30).stdout.strip()
                for command in (["rustc", "--version", "--verbose"], ["cargo", "--version"],
                                ["cargo", "nextest", "--version"])}
    campaign.atomic_json(output / "environment.json", dict(toolchain=versions,
                         architecture=platform.machine(), operating_system=platform.system(),
                         kernel=platform.release(), build_jobs=4, numerical_workers=1,
                         cargo_profile="release", requested_package="quest-compile",
                         requested_features="default", target_directory=str(target),
                         compiler_controls={key: os.environ.get(key) for key in
                                            ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER")}))
    identity = source_identity(workspace)
    campaign.atomic_json(output / "source-identity.json", identity)
    results = []
    for number, row in enumerate(manifest):
        stem = f"{number:04d}"
        log_path = output / f"{stem}.log"
        phase_path = output / f"{stem}.phase"
        rss_path = output / f"{stem}.rss"
        env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="4",
                   QUEST_BENCH_DIMENSION=str(row["dimension"]), QUEST_BENCH_PHASE=str(phase_path),
                   CRITERION_HOME=str(output / stem / "criterion"), RAYON_NUM_THREADS="1")
        command = ["cargo", "nextest", "bench", "--offline", "-p", "quest-compile", "--bench", "source_unitary",
                   "-E", f"test(=dimension{row['dimension']})"]
        with open("/tmp/quest-quality-build.lock", "a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            verify_controllers(controller_directory, controllers["files"])
            if source_identity(workspace)["sha256"] != identity["sha256"]:
                raise RuntimeError("production tree changed; preserve partial evidence and restart lane")
            started = time.monotonic()
            with log_path.open("w") as log:
                process = subprocess.Popen(campaign.time_command(command, rss_path), cwd=workspace, env=env, stdout=log,
                                           stderr=subprocess.STDOUT, start_new_session=True)
                returncode, stopped = campaign.wait_bounded(process, row["timeout_seconds"])
        phase = phase_path.read_text() if phase_path.exists() else "build_or_discovery"
        text = log_path.read_text(errors="replace")
        phase = reported_phase(phase, text)
        status = stopped or campaign.classify_run(returncode, text, phase)
        if stopped is None and returncode != 0 and "unitarity residual" in text:
            status = "accuracy_failure"
        if source_identity(workspace)["sha256"] != identity["sha256"]:
            raise RuntimeError("production tree changed during measurement; preserve evidence and restart lane")
        verify_controllers(controller_directory, controllers["files"])
        result = dict(id=row["id"], status=status, phase=phase, returncode=returncode,
                      elapsed_seconds=time.monotonic() - started, log=log_path.name,
                      log_sha256=campaign.sha256(log_path), requested_accuracy=0.0,
                      peak_rss_kib=campaign.read_peak_rss(rss_path) if stopped is None else None,
                      peak_rss_scope=campaign.RSS_SCOPE, peak_rss_unit="KiB",
                      peak_rss_log=rss_path.name if rss_path.is_file() else None,
                      peak_rss_log_sha256=campaign.sha256(rss_path) if rss_path.is_file() else None)
        accuracy = re.search(r"QUEST_BENCH_PREFLIGHT requested=0 achieved=(\S+)", text)
        result["achieved_accuracy"] = float(accuracy[1]) if accuracy else None
        if status == "ok":
            samples = list((output / stem).rglob("sample.json"))
            result["raw_samples"] = [dict(path=str(path.relative_to(output)), sha256=campaign.sha256(path)) for path in samples]
            if not samples:
                result.update(status="native_error", reason="Criterion samples missing")
        if phase == "build_or_discovery" and stopped != "interrupted":
            campaign.atomic_json(output / "infrastructure-failure.json", result)
            raise RuntimeError("build/discovery failed; fix the harness before measuring production")
        results.append(result)
        with (output / "results.jsonl").open("a") as stream:
            stream.write(json.dumps(result, allow_nan=False) + "\n")
        print(f"[{number + 1}/{len(manifest)}] {result['status']} {row['id']}", flush=True)
        if stopped == "interrupted":
            raise KeyboardInterrupt("interrupted unitary row retained; remaining cases unattempted")
    if source_identity(workspace)["sha256"] != identity["sha256"]:
        raise RuntimeError("production tree changed before completion; lane remains incomplete")
    verify_controllers(controller_directory, controllers["files"])
    return campaign.publish_completion(output, manifest, results)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    blocked = sub.add_parser("source-blocked")
    for name in ("source", "log", "output"):
        blocked.add_argument("--" + name, type=Path, required=True)
    unitary = sub.add_parser("unitary")
    for name in ("workspace", "target", "output"):
        unitary.add_argument("--" + name, type=Path, required=True)
    unitary.add_argument("--implementation", required=True)
    unitary.add_argument("--controller-pin", type=Path, required=True)
    args = parser.parse_args()
    if args.action == "source-blocked":
        result = source_blocked(args.source.resolve(), args.log.resolve(), args.output.resolve())
    else:
        result = run_unitary(args.workspace.resolve(), args.target.resolve(), args.output.resolve(), args.implementation,
                             args.controller_pin.resolve())
    print(json.dumps(result))


if __name__ == "__main__":
    main()
