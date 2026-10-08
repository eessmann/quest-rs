#!/usr/bin/env python3
"""Run unchanged native Catch fixtures in an activated, memory-bounded devenv.

This follow-up has its own evidence schema. Catch aggregate statistics are not
Criterion raw samples. Existing campaign receipts are never overwritten.
"""
import argparse
from collections import Counter
import fcntl
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET

HELPER = Path(__file__).resolve().parents[1] / "campaign.py"
SPEC = importlib.util.spec_from_file_location("campaign", HELPER)
campaign = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(campaign)
LOADED_FILES = {str(path): campaign.sha256(path) for path in (Path(__file__).resolve(), HELPER)}
CALLER = getattr(sys.modules.get("__main__"), "__file__", None)
if CALLER and Path(CALLER).is_file():
    LOADED_FILES[str(Path(CALLER).resolve())] = campaign.sha256(Path(CALLER))
DEADLINE_SECONDS = 14400


def source_manifest():
    groups = [
        dict(name="roots", binary="test/qsp_tools/critical_point_benchmarks", filter="[root_isolation][telemetry]"),
        dict(name="unity", binary="test/qsp_tools/unit_tests_poly", filter="Performance benchmarks"),
        dict(name="unitary", binary="test/qsvt_tools/circuit_benchmarks", filter="[unitary_proof][slow]"),
        dict(name="circuit", binary="test/qsvt_tools/circuit_benchmarks", filter="[task23_benchmark][dag_scaling]"),
        dict(name="coordinate", binary="test/qsvt_tools/circuit_benchmarks", filter="[task23_benchmark][degree8105]"),
    ]
    stages = dict(construct="dynamic expression construction", freeze="freeze preconstructed expression",
                  cold_execute="cold execute including preparation", prepare="prepare", warm_execute="warm prepared execute")
    rows = []
    for source in campaign.source_inventory([]):
        suite = source["suite"]
        if suite not in {"root_isolation", "roots_of_unity", "unitary", "circuit", "coordinate"}:
            continue
        row = dict(source, id="quest_qsvt_cpp/" + source["case_id"], implementation="quest_qsvt_cpp",
                   protocol="catch2_10", raw_samples_exported=False, input_sha256=None,
                   input_identity_status="defined by unchanged pinned source; no runtime input export",
                   backend=None, workers=None, effective_backend_status="not exported by unchanged entrypoint",
                   workers_status="runtime default; OMP environment recorded separately", requested_accuracy=None,
                   deadline_scope="whole selectable Catch test case including setup and every enclosed workload",
                   accuracy_contract="unchanged source assertions and telemetry; no additional per-iteration validation")
        if suite == "root_isolation":
            row.update(group="roots", benchmark_name="root isolation " + source["case_id"].split("/")[1])
        elif suite == "roots_of_unity":
            row.update(group="unity", benchmark_name="Direct evaluation n=512" if source["case_id"].endswith("direct512") else "With roots output n=256",
                       input_status="upstream Eigen random coefficients are not exported")
        elif suite == "unitary":
            n = source["dimension"]
            row.update(group="unitary", benchmark_name=f"{n}x{n} certified unitary admission",
                       requested_accuracy=0.0, accuracy_contract="source certified identity admission at tolerance zero and scalar-work counter assertions")
        elif suite == "circuit":
            row.update(group="circuit", benchmark_name=f"degree {source['degree']}: {stages[source['scope']]}")
        elif source["protocol"] == "fixed_3":
            row.update(group="coordinate", benchmark_name=None, protocol="fixed_3", raw_samples_exported=True)
        else:
            row.update(group=None, benchmark_name=None, protocol="criterion_10",
                       reason="Unchanged source hardcodes three chrono samples; no Criterion or configurable Catch benchmark exists for this fixture")
        rows.append(row)
    return groups, rows


def parse_xml_report(text):
    """Retain closed benchmark records even when a killed process truncates XML."""
    parser = ET.XMLPullParser(events=("start", "end"))
    report = dict(benchmarks={}, started=[], failures=0, complete=False, parse_error=None,
                  captured_stdout="", successful_test_cases=0)
    try:
        parser.feed(text)
    except ET.ParseError as error:
        report["parse_error"] = str(error)
    try:
        for event, element in parser.read_events():
            if event == "start" and element.tag == "BenchmarkResults":
                name = element.get("name", "")
                if name in report["started"]:
                    raise ValueError("duplicate benchmark name in native report")
                report["started"].append(name)
            if event != "end":
                continue
            if element.tag == "BenchmarkResults":
                mean = element.find("mean")
                report["benchmarks"][element.get("name", "")] = dict(
                    attributes=dict(element.attrib), mean=dict(mean.attrib) if mean is not None else None,
                    failed=element.find("BenchmarkFailure") is not None)
            elif element.tag == "OverallResults":
                report["failures"] += int(element.get("failures", "0"))
            elif element.tag == "OverallResult" and element.get("success") == "false":
                report["failures"] += 1
            elif element.tag == "OverallResult" and element.get("success") == "true":
                report["successful_test_cases"] += 1
            elif element.tag == "StdOut":
                report["captured_stdout"] += "".join(element.itertext())
            elif element.tag == "Catch2TestRun":
                report["complete"] = True
        parser.close()
    except (ET.ParseError, ValueError) as error:
        if isinstance(error, ValueError) and not isinstance(error, ET.ParseError):
            raise
        report["parse_error"] = str(error)
    return report


def measured_summary(benchmark):
    try:
        count = int(benchmark["attributes"]["samples"])
        iterations = int(benchmark["attributes"]["iterations"])
        mean = float(benchmark["mean"]["value"])
        low = float(benchmark["mean"]["lowerBound"])
        high = float(benchmark["mean"]["upperBound"])
        if benchmark["failed"] or count < 10 or iterations < 1 or not all(math.isfinite(x) and x > 0 for x in (mean, low, high)) or not low <= mean <= high:
            return None
        return dict(sample_count=count, iterations_per_sample=iterations, mean_ns=mean,
                    mean_confidence_interval_ns=[low, high], raw_samples_available=False)
    except (KeyError, ValueError, TypeError):
        return None


def classify_group(rows, report, stdout, returncode, stopped):
    valid = returncode == 0 and stopped is None and report["complete"] and not report["parse_error"] and report["failures"] == 0 and report["successful_test_cases"] == 1
    results = []
    for row in rows:
        result = dict(id=row["id"], status="unattempted", process_returncode=returncode,
                      process_stop_reason=stopped, source_assertion_gate_passed=valid,
                      phase="not_observed", process_phase="unknown; unchanged source does not emit explicit phase markers")
        if row["protocol"] == "fixed_3":
            captured = stdout if "degree 8105 coordinate preparation samples (ns):" in stdout else report["captured_stdout"]
            matches = re.findall(r"degree 8105 coordinate preparation samples \(ns\): (\d+), (\d+), (\d+)\s*(?:\n|$)", captured)
            telemetry = "QSVT_BENCHMARK_STORAGE_V1 benchmark=degree8105 degree=8105 " in captured
            if len(matches) == 1 and all(int(x) > 0 for x in matches[0]):
                result.update(raw_times_ns=[int(x) for x in matches[0]], iterations_per_sample=1,
                              status="ok" if valid and telemetry else "observed_samples_unvalidated", phase="complete")
            else:
                result.update(status=stopped or "native_error", phase="unknown", reason="Source did not emit exactly three positive coordinate preparation samples")
        else:
            name = row["benchmark_name"]
            benchmark = report["benchmarks"].get(name)
            if benchmark is not None:
                result["phase"] = "complete"
                summary = measured_summary(benchmark)
                if summary is None:
                    result.update(status="native_error", reason="Named benchmark lacks a valid completed ten-sample summary")
                else:
                    result.update(measurement=summary, status="measured_summary_only" if valid else "observed_summary_unvalidated")
                if report["failures"]:
                    result["status"] = "accuracy_failure"
            elif name in report["started"]:
                result.update(status=stopped or "native_error", phase="warmup_or_measurement_unknown", reason="Named benchmark started without a completed summary; source does not distinguish warmup and measurement")
            else:
                result["reason"] = "No named benchmark start or completed result was observed; process success alone is insufficient"
        results.append(result)
    return results


def verify_files(expected):
    for filename, digest in expected.items():
        path = Path(filename)
        if not path.is_file() or campaign.sha256(path) != digest:
            raise RuntimeError(f"source/compiler/controller/binary identity changed: {filename}")


def command_text(command, cwd=None, timeout=30):
    with tempfile.TemporaryFile() as output:
        process = subprocess.Popen(command, cwd=cwd, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
        code, stopped = campaign.wait_bounded(process, timeout)
        output.seek(0)
        text = output.read().decode(errors="replace")
    if stopped or code:
        raise RuntimeError(f"identity command failed: {command!r}: {text}")
    return text


def capture_identity(source, build, groups, launcher):
    revision = command_text(["git", "rev-parse", "HEAD"], source).strip()
    if revision != campaign.SOURCE_REVISIONS["quest_qsvt"]:
        raise RuntimeError("upstream source revision differs from the pinned benchmark source")
    command_text(["git", "diff", "--quiet", "HEAD", "--"], source)
    cache_path = build / "CMakeCache.txt"
    cache = dict(re.findall(r"^([^/#][^:\n]*):[^=\n]+=(.*)$", cache_path.read_text(), re.MULTILINE))
    if Path(cache["CMAKE_HOME_DIRECTORY"]).resolve() != source:
        raise RuntimeError("native build does not refer to the pinned upstream source directory")
    compiler = Path(cache["CMAKE_CXX_COMPILER"]).resolve(strict=True)
    files = [source / name for name in command_text(["git", "ls-files", "-z"], source).split("\0") if name]
    files += [cache_path, compiler, *(build / group["binary"] for group in groups)]
    files += list((build / "CMakeFiles").glob("*/CMakeCXXCompiler.cmake"))
    files += [path for path in (build / "build.ninja", build / "compile_commands.json") if path.is_file()]
    files += [Path(name) for name in LOADED_FILES]
    caller = getattr(sys.modules.get("__main__"), "__file__", None)
    if caller:
        files.append(Path(caller).resolve())
    launcher_path = None
    if launcher:
        found = shutil.which(launcher[0])
        if found is None:
            raise RuntimeError("requested native launcher is not available in the activated environment")
        launcher_path = str(Path(found).resolve(strict=True))
        files.append(Path(launcher_path))
    fingerprints = {str(path.resolve()): campaign.sha256(path) for path in sorted(set(files))}
    verify_files(LOADED_FILES)
    return dict(source_revision=revision, files=fingerprints, compiler=str(compiler),
                binaries={group["name"]: str((build / group["binary"]).resolve()) for group in groups},
                compiler_version=command_text([str(compiler), "--version"]), launcher=launcher,
                launcher_path=launcher_path, cmake_cache=cache,
                sha256=hashlib.sha256(json.dumps(fingerprints, sort_keys=True).encode()).hexdigest())


def run_groups(source, build, output, groups, rows, launcher=()):
    """Shared native-only ledger; groups are indivisible upstream test cases."""
    source, build, output = (Path(path).resolve() for path in (source, build, output))
    memory = campaign.effective_memory_limits()
    if len({row["id"] for row in rows}) != len(rows) or len({group["name"] for group in groups}) != len(groups):
        raise ValueError("native manifest identities must be unique")
    names = {group["name"] for group in groups}
    if any(not re.fullmatch(r"[A-Za-z0-9_-]+", name) for name in names):
        raise ValueError("group names must be safe artifact basenames")
    if any(row["group"] is not None and row["group"] not in names for row in rows):
        raise ValueError("row refers to an unknown execution group")
    output.mkdir(parents=True, exist_ok=False)
    with open("/tmp/quest-quality-build.lock", "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        identity = capture_identity(source, build, groups, list(launcher))
        controllers = output / "controllers"
        controllers.mkdir()
        identity["controller_snapshots"] = {}
        for filename in identity["files"]:
            if filename in LOADED_FILES or filename == str(Path(getattr(sys.modules.get("__main__"), "__file__", __file__)).resolve()):
                shutil.copyfile(filename, controllers / Path(filename).name)
                identity["controller_snapshots"][str(Path("controllers") / Path(filename).name)] = identity["files"][filename]
        campaign.atomic_json(output / "identity.json", identity)
        verify_files(identity["files"])
    campaign.atomic_json(output / "manifest.json", dict(schema_version=1, purpose="unchanged_native_source_followup",
                         memory_limits=memory, groups=groups, rows=rows, timeout_seconds_per_selectable_group=DEADLINE_SECONDS,
                         environment={key: os.environ.get(key) for key in ("OMP_NUM_THREADS", "OMP_PROC_BIND", "OMP_PLACES", "CMAKE_PREFIX_PATH", "LD_LIBRARY_PATH")},
                         backend_claim="compiled QuEST capabilities are recorded in CMake metadata; actual automatic deployment is not inferred"))
    results = [dict(id=row["id"], status="unsupported", reason=row["reason"]) for row in rows if row["group"] is None]
    process_results = []
    interrupted = False
    for group in groups:
        prefix = group["name"]
        xml, report_json, log_path, rss = [output / f"{prefix}.{suffix}" for suffix in ("xml", "json", "log", "rss")]
        selected = [row for row in rows if row["group"] == prefix]
        binary = identity["binaries"][prefix]
        actual_launcher = [identity["launcher_path"], *launcher[1:]] if launcher else []
        command = [*actual_launcher, binary, group["filter"], "--benchmark-samples", "10", "--benchmark-warmup-time", "100",
                   "--benchmark-confidence-interval", "0.95", "--reporter", f"xml::out={xml}",
                   "--reporter", f"json::out={report_json}", "--reporter", "console::out=-", *group.get("extra_args", [])]
        with open("/tmp/quest-quality-build.lock", "a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            verify_files(identity["files"])
            started = time.monotonic()
            with log_path.open("w") as log:
                process = subprocess.Popen(campaign.time_command(command, rss), cwd=build, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                returncode, stopped = campaign.wait_bounded(process, DEADLINE_SECONDS)
            verify_files(identity["files"])
        stdout = log_path.read_text(errors="replace")
        parsed = parse_xml_report(xml.read_text(errors="replace") if xml.is_file() else "")
        artifacts = [dict(path=path.name, sha256=campaign.sha256(path)) for path in (xml, report_json, log_path, rss) if path.is_file()]
        observation = dict(group=prefix, command=command, returncode=returncode, stopped=stopped,
                           elapsed_seconds=time.monotonic() - started, artifacts=artifacts,
                           peak_rss_kib=campaign.read_peak_rss(rss) if stopped is None else None,
                           peak_rss_scope="largest child process peak RSS for entire selectable group; not per benchmark or aggregate cgroup",
                           report_complete=parsed["complete"], report_parse_error=parsed["parse_error"], assertion_failures=parsed["failures"])
        process_results.append(observation)
        results.extend(dict(result, group=prefix, artifacts=artifacts) for result in classify_group(selected, parsed, stdout, returncode, stopped))
        campaign.atomic_json(output / "processes.json", process_results)
        (output / "results.jsonl").write_text("".join(json.dumps(row, allow_nan=False) + "\n" for row in results))
        print(f"{prefix}: {dict(Counter(row['status'] for row in results if row.get('group') == prefix))}", flush=True)
        if stopped == "interrupted":
            interrupted = True
            break
    if interrupted:
        recorded = {row["id"] for row in results}
        results.extend(dict(id=row["id"], status="unattempted", reason="Campaign interrupted before selectable group execution") for row in rows if row["id"] not in recorded)
        (output / "results.jsonl").write_text("".join(json.dumps(row, allow_nan=False) + "\n" for row in results))
    verify_files(identity["files"])
    for observation in process_results:
        for artifact in observation["artifacts"]:
            if campaign.sha256(output / artifact["path"]) != artifact["sha256"]:
                raise RuntimeError("native output artifact changed before publication")
    summary = validate_lane(output)
    campaign.atomic_json(output / "completion.json", summary)
    return summary


def validate_lane(root):
    """Reparse preserved observations and verify their hashes before accounting."""
    root = Path(root).resolve()
    manifest = json.loads((root / "manifest.json").read_text())
    results = [json.loads(line) for line in (root / "results.jsonl").read_text().splitlines()]
    processes = json.loads((root / "processes.json").read_text())
    identity = json.loads((root / "identity.json").read_text())
    rows = manifest["rows"]
    expected = {row["id"] for row in rows}
    if len(expected) != len(rows) or len({row["id"] for row in results}) != len(results) or {row["id"] for row in results} != expected:
        raise ValueError("every native manifest identity requires one result")
    def verify_artifact(name, digest):
        path = (root / name).resolve()
        if not path.is_relative_to(root) or not path.is_file() or campaign.sha256(path) != digest:
            raise ValueError("native artifact is missing, changed, or outside lane")
    for process in processes:
        for artifact in process["artifacts"]:
            verify_artifact(artifact["path"], artifact["sha256"])
    for name, digest in identity["controller_snapshots"].items():
        verify_artifact(name, digest)
    actual = {row["id"]: row for row in results}
    observed_groups = set()
    expected_groups = {group["name"] for group in manifest["groups"]}
    for process in processes:
        group = process["group"]
        if group in observed_groups or group not in expected_groups:
            raise ValueError("duplicate or unknown native execution group")
        observed_groups.add(group)
        selected = [row for row in rows if row["group"] == group]
        paths = {artifact["path"] for artifact in process["artifacts"]}
        rss = root / f"{group}.rss"
        expected_rss = campaign.read_peak_rss(rss) if rss.name in paths and process["stopped"] is None else None
        if process.get("peak_rss_kib") != expected_rss:
            raise ValueError("native process RSS differs from its hashed observation")
        report_path = root / f"{group}.xml"
        report = parse_xml_report(report_path.read_text(errors="replace") if report_path.name in paths else "")
        log = root / f"{group}.log"
        if log.name not in paths:
            raise ValueError("native group requires a hashed process log")
        inferred = classify_group(selected, report, log.read_text(errors="replace"), process["returncode"], process["stopped"])
        for row in inferred:
            if any(actual[row["id"]].get(key) != value for key, value in row.items()):
                raise ValueError("native result differs from its preserved report")
    for row in rows:
        if row["group"] is None:
            if actual[row["id"]]["status"] != "unsupported":
                raise ValueError("unsupported source protocol cannot become a measurement")
        elif row["group"] not in observed_groups and actual[row["id"]]["status"] != "unattempted":
            raise ValueError("unexecuted group cannot claim an observed result")
    return dict(schema_version=1, accounting_complete=True, expected_rows=len(rows),
                outcomes=dict(Counter(row["status"] for row in results)),
                execution_complete=all(row["status"] not in {"unattempted", "interrupted"} for row in results),
                execution_coverage_complete=all(row["status"] in {"ok", "measured_summary_only"} for row in results),
                performance_coverage_complete=all(row["status"] == "ok" for row in results),
                raw_performance_coverage_complete=all(row["status"] == "ok" for row in results),
                interrupted=any(process["stopped"] == "interrupted" for process in processes))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("source", "build", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--launcher", nargs="+", default=[], help="Installed launcher argv, e.g. quest-with-nvidia")
    args = parser.parse_args()
    groups, rows = source_manifest()
    print(json.dumps(run_groups(args.source.resolve(), args.build.resolve(), args.output.resolve(), groups, rows, args.launcher)))


if __name__ == "__main__":
    main()
