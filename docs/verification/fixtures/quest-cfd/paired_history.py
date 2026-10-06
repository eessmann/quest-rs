#!/usr/bin/env python3
"""Fixed, bounded same-ensemble classical history comparison.

Run one immutable supplied binary in fourteen separate capped subprocesses.
These paired diagnostics certify neither physical nor hierarchy convergence.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import resource
import shutil
import subprocess
import tempfile
from campaign import collect, decode_report, report_finite, sha256

LIMITS = {"max_bytes": 268435456, "max_work": 10000000000,
          "max_source_work": 1000000000, "max_physical_work": 100000000000,
          "max_drift_calls": 1000000}
ADDRESS_SPACE_BYTES = 512 * 1024**2
OUTPUT_BYTES = 4 * 1024**2
TIMEOUT_SECONDS = 180
SCHEMA = "quest-cfd-paired-history-row-v1"
TRUNCATION = "ensemble hierarchy truncation unverified; compare independent order differences, not a single-trajectory certificate"
FIXTURE = {"physical_source": "periodic unit-square two-triangle BDM1/P0",
           "viscosity": .01, "physical_dimension": 5, "horizon": .01,
           "configuration": {"lower": -.2, "upper": .2, "cells": 1, "order": 2, "nodes": 243},
           "initial_center": [.04, -.03, .02, .05, -.01], "initial_width": .55,
           "observation_times": [.0025, .01],
           "carleman_scale_rule": "2 * maximum supported sample physical norm",
           "independent_rk4_steps": [256, 512]}


def requests():
    yield {"EnsembleReference": {"steps": 256}}
    yield {"EnsembleReference": {"steps": 512}}
    for lift in ["Kvn", *({"Carleman": {"order": r}} for r in (2, 3, 4))]:
        for cells, order in [(1, 1), (2, 1), (1, 2)]:
            yield {"History": {"lift": lift, "time_cells": cells, "time_order": order}}


def shape(value, keys):
    if type(value) is not dict or set(value) != set(keys):
        raise ValueError("unexpected or missing schema fields")


def number(value, minimum=None, maximum=None):
    try:
        finite = type(value) in (int, float) and math.isfinite(value)
    except OverflowError:
        finite = False
    if not finite:
        raise ValueError("required finite numeric field")
    if minimum is not None and value < minimum or maximum is not None and value > maximum:
        raise ValueError("numeric field outside admitted range")
    return value


def integer(value, minimum=0, maximum=None):
    if type(value) is not int:
        raise ValueError("required integer counter")
    return number(value, minimum, maximum)


def vector(value):
    if type(value) is not list or len(value) != 5:
        raise ValueError("complete five-coordinate vector required")
    for x in value:
        number(x)


def dimension(request):
    if request_key(request) not in {request_key(x) for x in requests()}:
        raise ValueError("request outside frozen fourteen rows")
    if "EnsembleReference" in request:
        return None
    history = request["History"]
    lift = history["lift"]
    n = 243 if lift == "Kvn" else {2: 20, 3: 55, 4: 125}[lift["Carleman"]["order"]]
    return n * history["time_cells"] * (history["time_order"] + 1)


def request_key(request):
    try:
        return json.dumps(request, sort_keys=True, allow_nan=False)
    except (TypeError, ValueError, RecursionError) as error:
        raise ValueError("invalid request JSON") from error


def process_admission(collected):
    if not report_finite(collected) or type(collected.get("exit_code")) is not int or collected.get("exit_code") != 0 or collected.get("timed_out") is not False:
        raise ValueError("child did not exit successfully within the deadline")
    if collected.get("valid_report") is not True:
        raise ValueError("child output is not a bounded unambiguous finite report")
    integer(collected.get("stdout_bytes"), 1, OUTPUT_BYTES)
    integer(collected.get("peak_rss_kib"), 0)
    number(collected.get("elapsed_seconds"), 0)
    if collected.get("address_space_cap_bytes") != ADDRESS_SPACE_BYTES:
        raise ValueError("wrong child address-space cap")


def validate_rejected(request, collected):
    process_admission(collected)
    report = collected["report"]
    shape(report, ("status", "request", "limits", "error"))
    if report["status"] != "rejected" or request_key(report["request"]) != request_key(request) or request_key(report["limits"]) != request_key(LIMITS):
        raise ValueError("rejection does not bind the requested row and caps")
    if type(report["error"]) is not str or not report["error"] or len(report["error"]) > 4096:
        raise ValueError("bounded explicit numerical rejection required")
    dimension(request)
    return report


def validate(request, collected):
    """Admit a complete typed row, with no truthy integers or unknown schemas."""
    process_admission(collected)
    expected_dimension = dimension(request)
    report = collected["report"]
    shape(report, ("status", "row"))
    if report["status"] != "completed":
        raise ValueError("completed row required")
    row = report["row"]
    required = ("schema", "request", "physical_dimension", "history_dimension", "initial",
                "observations", "history_relative_residual", "modeled_peak_bytes",
                "history_reference_work", "source_query_work", "physical_reference_work",
                "physical_drift_calls", "constructor_work_allowance", "extraction_probe_error",
                "nonlinear_initial_action", "limits", "elapsed_seconds", "quantum_execution",
                "convergence_certified", "truncation_evidence")
    shape(row, (*required, "history_assembly_work_allowance") if type(row) is dict and "history_assembly_work_allowance" in row else required)
    if "history_assembly_work_allowance" in row:
        allowance = row["history_assembly_work_allowance"]
        if type(allowance) is not int or allowance != (1000000000 if expected_dimension is not None else 0):
            raise ValueError("separate history assembly allowance mismatch")
    if row["schema"] != SCHEMA or request_key(row["request"]) != request_key(request) or request_key(row["limits"]) != request_key(LIMITS):
        raise ValueError("row schema, request or hard admission mismatch")
    if type(row["physical_dimension"]) is not int or row["physical_dimension"] != 5:
        raise ValueError("complete physical dimension mismatch")
    if row["history_dimension"] != expected_dimension or (expected_dimension is not None and type(row["history_dimension"]) is not int):
        raise ValueError("history dimension mismatch")
    if row["quantum_execution"] is not False or row["convergence_certified"] is not False or row["truncation_evidence"] != TRUNCATION:
        raise ValueError("unsupported execution or convergence claim")
    number(row["elapsed_seconds"], 0)
    integer(row["modeled_peak_bytes"], 8 * 1024**2, LIMITS["max_bytes"])
    for key, cap in [("history_reference_work", "max_work"), ("source_query_work", "max_source_work"),
                     ("physical_reference_work", "max_physical_work"), ("physical_drift_calls", "max_drift_calls")]:
        integer(row[key], 0, LIMITS[cap])
    if type(row["constructor_work_allowance"]) is not int or row["constructor_work_allowance"] != 100000000:
        raise ValueError("constructor allowance mismatch")
    initial = row["initial"]
    shape(initial, ("sample_count", "support_count", "identity", "coordinate_mean", "covariance_trace",
                    "energy", "probability", "outer_occupation", "scale", "moment_reconstruction_error"))
    if type(initial["sample_count"]) is not int or initial["sample_count"] != 243:
        raise ValueError("complete ordered sample ensemble required")
    integer(initial["support_count"], 1, 243)
    if type(initial["identity"]) is not str or re.fullmatch(r"fnv1a64:[0-9a-f]{16}", initial["identity"]) is None:
        raise ValueError("missing canonical ensemble identity")
    vector(initial["coordinate_mean"])
    for key in ("covariance_trace", "energy"):
        number(initial[key], 0)
    number(initial["probability"], 1 - 1e-12, 1 + 1e-12)
    number(initial["outer_occupation"], 0, 1 + 1e-12)
    if number(initial["scale"], 0) == 0:
        raise ValueError("positive hierarchy scale required")
    history = request.get("History")
    if history and history["lift"] != "Kvn":
        number(initial["moment_reconstruction_error"], 0)
    elif initial["moment_reconstruction_error"] is not None:
        raise ValueError("unperformed Carleman reconstruction must remain unavailable")
    if history:
        for key in ("history_reference_work", "source_query_work", "physical_drift_calls"):
            integer(row[key], 1)
        if row["physical_reference_work"] != 100000 * row["physical_drift_calls"]:
            raise ValueError("history physical work does not match admitted drift schedule")
        number(row["history_relative_residual"], 0, 1e-10)
        number(row["extraction_probe_error"], 0)
        if history["lift"] == "Kvn":
            if number(row["nonlinear_initial_action"], 1e-12) == 1e-12:
                raise ValueError("nonlinear action must exceed the source threshold")
        elif row["nonlinear_initial_action"] is not None:
            raise ValueError("unexpected Carleman nonlinear action field")
    else:
        if any(row[k] is not None for k in ("history_relative_residual", "extraction_probe_error", "nonlinear_initial_action")):
            raise ValueError("reference row contains unavailable history evidence")
        steps = request["EnsembleReference"]["steps"]
        calls = 4 * 243 * steps
        if row["history_reference_work"] != 0 or row["source_query_work"] != 0 or row["physical_drift_calls"] != calls or row["physical_reference_work"] != calls * 100000 + 243 * steps * 1024:
            raise ValueError("physical-reference work schedule mismatch")
    observations = row["observations"]
    if type(observations) is not list or len(observations) != 2:
        raise ValueError("exact interior and final observations required")
    for index, observation in enumerate(observations):
        shape(observation, ("time", "slab", "side", "coordinate_mean", "energy", "probability",
                            "imaginary_residue", "interpolation_norm_upper"))
        if number(observation["time"]) != (.0025, .01)[index]:
            raise ValueError("wrong physical observation time")
        vector(observation["coordinate_mean"])
        carleman = history and history["lift"] != "Kvn"
        number(observation["energy"], None if carleman else 0)
        number(observation["imaginary_residue"], 0)
        if history:
            expected_slab = 0 if index == 0 else history["time_cells"] - 1
            if type(observation["slab"]) is not int or observation["slab"] != expected_slab or observation["side"] != ("Interior", "SlabRight")[index]:
                raise ValueError("wrong interpolation slab or trace side")
            if number(observation["interpolation_norm_upper"], 0) == 0:
                raise ValueError("interpolation norm must be positive")
        elif observation["slab"] is not None or observation["side"] != "classical physical time" or observation["interpolation_norm_upper"] is not None:
            raise ValueError("reference observation masquerades as DG interpolation")
        if history and history["lift"] == "Kvn":
            if number(observation["probability"], 0) == 0:
                raise ValueError("KvN readout probability must be positive")
        elif observation["probability"] is not None:
            raise ValueError("classical trajectories and Carleman moments have no interpolated probability")
    return row


def collect_row(binary, request, timeout=TIMEOUT_SECONDS):
    """Reuse reviewed process-group collector, with inherited file-size ceilings.

    Called serially: temporarily lower only this process's soft file-size limit,
    restore it before publishing. Each child inherits the 4MiB stdout/stderr cap.
    The unchanged collector independently bounds decoding to 4MiB.
    """
    dimension(request)
    number(timeout, 0, TIMEOUT_SECONDS)
    if timeout == 0:
        raise ValueError("positive child timeout required")
    before = resource.getrlimit(resource.RLIMIT_FSIZE)
    if before[1] != resource.RLIM_INFINITY and before[1] < OUTPUT_BYTES:
        raise ValueError("inherited hard file-size limit cannot support frozen fixture")
    try:
        resource.setrlimit(resource.RLIMIT_FSIZE, (OUTPUT_BYTES, before[1]))
        record = collect(binary, ["--request-json", json.dumps(request, separators=(",", ":"))],
                         ADDRESS_SPACE_BYTES, timeout)
    finally:
        resource.setrlimit(resource.RLIMIT_FSIZE, before)
    record["output_file_cap_bytes"] = OUTPUT_BYTES
    record["timeout_seconds"] = timeout
    try:
        if type(record.get("report")) is dict and record["report"].get("status") == "rejected":
            validate_rejected(request, record)
            record["outcome"] = "rejected"
        else:
            validate(request, record)
            record["outcome"] = "completed"
            row = record["report"]["row"]
            record["approximation_diagnostics"] = [
                {"time": observation["time"], "unphysical_negative_carleman_energy_estimate": observation["energy"]}
                for observation in row["observations"] if observation["energy"] < 0]
        record["validation_error"] = None
    except (ValueError, TypeError, KeyError, OverflowError) as error:
        record["outcome"] = "timeout" if record["timed_out"] else "invalid"
        record["validation_error"] = str(error)
    return record


def comparisons(records):
    """Only actual complete same-ensemble rows contribute paired differences."""
    admitted = {}
    withheld = []
    common = None
    for index, record in enumerate(records):
        if record["outcome"] != "completed":
            continue
        row = validate(record["request"], record)
        initial = {k: v for k, v in row["initial"].items() if k != "moment_reconstruction_error"}
        if common is None:
            common = initial
        if initial != common:
            withheld.append({"row": index, "reason": "common initial ensemble identity or diagnostics mismatch"})
        else:
            admitted[index] = row
    result = []
    pairs = [("physical-reference-step-sensitivity", 0, 1)]
    for i in range(2, 14):
        pairs.append(("history-vs-512-step-ensemble", i, 1))
    for base in (2, 5, 8, 11):
        pairs += [("DG1-slab-difference", base, base + 1), ("DG-order-difference", base, base + 2)]
    for variant in range(3):
        pairs += [("hierarchy-order-difference", 5 + variant, 8 + variant),
                  ("hierarchy-order-difference", 8 + variant, 11 + variant)]
        pairs += [("KvN-vs-Carleman-same-temporal-grid", 2 + variant, base + variant)
                  for base in (5, 8, 11)]
    for label, a, b in pairs:
        if a not in admitted or b not in admitted:
            continue
        differences = []
        finite = True
        for left, right in zip(admitted[a]["observations"], admitted[b]["observations"]):
            delta = [x - y for x, y in zip(left["coordinate_mean"], right["coordinate_mean"])]
            entry = {"time": left["time"], "coordinate_mean_l2_difference": math.hypot(*delta),
                     "coordinate_mean_max_abs_difference": max(map(abs, delta)),
                     "energy_abs_difference": abs(left["energy"] - right["energy"])}
            if not report_finite(entry):
                finite = False
                break
            differences.append(entry)
        if not finite:
            withheld.append({"left_row": a, "right_row": b, "reason": "paired arithmetic is not finite"})
            continue
        result.append({"kind": label, "left_row": a, "right_row": b,
                       "ensemble_identity": admitted[a]["initial"]["identity"],
                       "observations": differences, "certified_error_bound": False})
    return {"pairs": result, "withheld": withheld}


def source_snapshot(root):
    """Relative Rust/Cargo inputs; docs, receipts, native binaries are separate."""
    result = subprocess.run(["git", "ls-files", "-co", "--exclude-standard", "-z"],
                            cwd=root, check=True, capture_output=True)
    files = []
    digest = hashlib.sha256()
    for name in sorted(set(result.stdout.split(b"\0")) - {b""}):
        path = root / os.fsdecode(name)
        if path.is_file() and (path.suffix in (".rs", ".toml") or path.name == "Cargo.lock"):
            identity = sha256(path)
            files.append({"path": os.fsdecode(name), "sha256": identity})
            digest.update(name + b"\0" + identity.encode("ascii") + b"\n")
    return {"sha256": digest.hexdigest(), "file_count": len(files), "files": files,
            "scope": "git-listed tracked/untracked nonignored Rust/TOML/Cargo.lock inputs; native libraries, docs and receipts excluded"}


def publish(path, receipt):
    if not report_finite(receipt):
        raise ValueError("receipt is not bounded and finite")
    path.parent.mkdir(parents=True, exist_ok=True)
    staging = path.with_name(path.name + ".partial")
    staging.write_text(json.dumps(receipt, indent=2, allow_nan=False) + "\n")
    staging.replace(path)


def admit_build_attestation(path, binary, root):
    if path.stat().st_size > OUTPUT_BYTES:
        raise ValueError("oversized build attestation")
    attestation = decode_report(path.read_bytes())
    shape(attestation, ("schema", "command", "source_manifest_sha256", "source_unchanged_during_build",
                        "source_manifest", "binary_sha256", "profile", "features"))
    if attestation["schema"] != "quest-cfd-paired-build-v1" or attestation["source_unchanged_during_build"] is not True or attestation["binary_sha256"] != sha256(binary):
        raise ValueError("build attestation does not bind the frozen binary and unchanged sources")
    if attestation["command"] != ["cargo", "build", "--release", "-p", "quest-cfd", "--example", "paired_history"] or attestation["profile"] != "release":
        raise ValueError("unexpected paired example build")
    manifest = attestation["source_manifest"]
    shape(manifest, ("scope", "sha256", "files"))
    if type(manifest["scope"]) is not str or not manifest["scope"] or type(attestation["features"]) is not str or not attestation["features"]:
        raise ValueError("build provenance scope required")
    if type(manifest["sha256"]) is not str or re.fullmatch(r"[0-9a-f]{64}", manifest["sha256"]) is None or manifest["sha256"] != attestation["source_manifest_sha256"]:
        raise ValueError("build source manifest identity mismatch")
    if type(manifest["files"]) is not list or not 1 <= len(manifest["files"]) <= 10000:
        raise ValueError("bounded source manifest required")
    seen, changed = set(), []
    for entry in manifest["files"]:
        shape(entry, ("path", "sha256"))
        name = entry["path"]
        if type(name) is not str or not name or Path(name).is_absolute() or ".." in Path(name).parts or name in seen:
            raise ValueError("unique relative build source path required")
        if type(entry["sha256"]) is not str or re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]) is None:
            raise ValueError("source content SHA required")
        seen.add(name)
        source = root / name
        if not source.is_file() or sha256(source) != entry["sha256"]:
            changed.append(name)
    return {"attestation": attestation, "attestation_sha256": sha256(path),
            "current_source_matches_build_manifest": not changed, "changed_since_build": changed,
            "verification_scope": "supplied frozen build attestation, binary SHA and per-file current source hashes; aggregate manifest hash retains producer definition"}


def run(binary, output, root, build_attestation=None):
    """Pin executable bytes once; publish after every frozen child, including failures."""
    source = source_snapshot(root)
    receipt = {"schema": "quest-cfd-paired-history-campaign-v1", "complete": False,
               "source_before": source, "source_after": None, "source_unchanged": None,
               "source_binary_binding": "supplied binary SHA and source snapshot at execution; source-to-binary build attestation is not inferred",
               "binary_sha256": sha256(binary), "binary_unchanged": None,
               "fixture": FIXTURE,
               "build_provenance": admit_build_attestation(build_attestation, binary, root) if build_attestation else None,
               "address_space_cap_bytes": ADDRESS_SPACE_BYTES, "output_file_cap_bytes": OUTPUT_BYTES,
               "timeout_seconds_per_child": TIMEOUT_SECONDS, "managed_limits": LIMITS,
               "quantum_execution": False, "physical_convergence_certified": False,
               "ensemble_hierarchy_truncation_certified": False,
               "remaining": ["continuum/configuration/physical convergence", "uniform ensemble hierarchy truncation bound",
                             "quantum execution and sampling", "multi-host capacity"],
               "requests": list(requests()), "records": [], "comparisons": {"pairs": [], "withheld": []}}
    receipt["outcome_counts"] = {k: 0 for k in ("completed", "rejected", "timeout", "invalid")}
    receipt["completion_meaning"] = "all fourteen frozen child attempts recorded; rejected/timeout/invalid outcomes are retained, not passes"
    with tempfile.TemporaryDirectory(prefix="quest-cfd-paired-binary-") as directory:
        pinned = Path(directory) / "paired_history"
        shutil.copyfile(binary, pinned)
        pinned.chmod(0o500)
        if sha256(pinned) != receipt["binary_sha256"]:
            raise ValueError("binary changed while pinning")
        publish(output, receipt)
        for index, request in enumerate(receipt["requests"]):
            record = collect_row(pinned, request)
            record["request"] = request
            receipt["records"].append(record)
            receipt["outcome_counts"][record["outcome"]] += 1
            receipt["comparisons"] = comparisons(receipt["records"])
            publish(output, receipt)
            print(f"row {index + 1}/14: {record['outcome']}", flush=True)
        receipt["binary_unchanged"] = sha256(pinned) == receipt["binary_sha256"]
    receipt["source_after"] = source_snapshot(root)
    receipt["source_unchanged"] = receipt["source_after"] == source
    receipt["complete"] = len(receipt["records"]) == 14 and receipt["binary_unchanged"]
    publish(output, receipt)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--build-attestation", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error("executable binary required")
    run(binary, args.output, Path(__file__).resolve().parents[4], args.build_attestation)


if __name__ == "__main__":
    main()
