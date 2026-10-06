#!/usr/bin/env python3
"""Exactly three capped fixed rows. Keeps failed processes; never retries or relaxes limits."""
import hashlib
import json
import math
import os
from pathlib import Path
import resource
import signal
import subprocess
import sys
import time

# Reuse the maintained bounded strict decoder; do not duplicate a permissive parser.
sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "docs/verification/fixtures/quest-cfd"))
from campaign import decode_report, report_finite

STEPS = (2, 4, 8)
WORK = {2: 1733765504, 4: 1804827776, 8: 1946952320}


def shape(value, fields):
    if type(value) is not dict or set(value) != set(fields.split()):
        raise ValueError("unexpected or missing required fields")


def number(value, minimum=None):
    try:
        finite = type(value) in (int, float) and math.isfinite(value)
    except OverflowError:
        finite = False
    if not finite or minimum is not None and value < minimum:
        raise ValueError("invalid finite numerical payload")


def vector(value, count):
    if type(value) is not list or len(value) != count:
        raise ValueError("invalid complete vector")
    for word in value:
        number(word)


def full_schema(row):
    shape(row, "schema status request resources progress last_state energy_samples energy_calls_attempted diagnostic_drift_calls_attempted report error temporal_accuracy_assessed")
    shape(row["request"], "steps initial_condition")
    shape(row["progress"], "completed_steps attempted_step accepted_time drift_calls_attempted observer_calls_attempted failure_phase")
    shape(row["resources"], "cumulative_work peak_bytes last_stage attempted_stage_work max_work max_bytes")
    report = row["report"]
    shape(report, "dt end_time initial_integrated_energy state_change_norm quadratic_action_norm relative_quadratic_action quadratic_action_resolved quadratic_probe_calls integration_completed forward_accuracy_certified resolved_full_minus_linear_response snapshot")
    snapshot = report["snapshot"]
    shape(snapshot, "method case time reynolds independent_dimension geometry geometry_fingerprint maximum_geometry_deviation pressure mean_kinetic_energy enstrophy cylinder_force drag_coefficient lift_coefficient pressure_difference resources")
    shape(snapshot["geometry"], "source_policy requested_sectors retained_segments coalesced_rays maximum_coalesced_angle represented_rectangle_residual")
    shape(snapshot["pressure"], "mesh_resources pressure_coefficients normal_multipliers momentum_residual continuity_residual normalization pressure_integral normalization_residual")
    if row["temporal_accuracy_assessed"] is not False:
        raise ValueError("single row cannot claim temporal comparison")
    for key in ("dt", "end_time", "initial_integrated_energy", "state_change_norm", "quadratic_action_norm", "relative_quadratic_action"):
        number(report[key], 0.)
    for key in ("time", "mean_kinetic_energy", "enstrophy", "maximum_geometry_deviation"):
        number(snapshot[key], 0.)
    for key in ("drag_coefficient", "lift_coefficient", "pressure_difference"):
        number(snapshot[key])
    vector(snapshot["cylinder_force"], 3)
    vector(row["last_state"], 54)
    pressure = snapshot["pressure"]
    for key in ("momentum_residual", "continuity_residual"):
        number(pressure[key], 0.)
    number(pressure["pressure_integral"])
    mesh = pressure["mesh_resources"]
    shape(mesh, "source_retained_bytes external_retained_bytes additional_owner_bytes scratch_bytes peak_bytes work")
    if any(type(value) is not int or value <= 0 for value in mesh.values()):
        raise ValueError("invalid original pressure query receipt")
    if mesh["peak_bytes"] != sum(mesh[key] for key in ("source_retained_bytes", "external_retained_bytes", "additional_owner_bytes", "scratch_bytes")) or mesh["work"] != 505884672 or mesh["peak_bytes"] > row["resources"]["max_bytes"]:
        raise ValueError("inconsistent original pressure query budget")
    vector(pressure["normal_multipliers"], 90)
    if type(pressure["pressure_coefficients"]) is not list or len(pressure["pressure_coefficients"]) != 16:
        raise ValueError("incomplete pressure source")
    for cell in pressure["pressure_coefficients"]:
        vector(cell, 3)
    if type(row["energy_samples"]) is not list:
        raise ValueError("missing energy samples")
    for sample in row["energy_samples"]:
        shape(sample, "time integrated_energy")
        number(sample["time"], 0.)
        number(sample["integrated_energy"], 0.)
    geometry = snapshot["geometry"]
    expected = {"source_policy": "explicit-rectangle-corner-priority-v1", "requested_sectors": 4, "retained_segments": 8, "coalesced_rays": 0, "maximum_coalesced_angle": 0., "represented_rectangle_residual": 0.}
    if geometry != expected or snapshot["case"] != "shedding2d" or snapshot["method"] != "classical complete P2 RK4 trajectory; fixed short window":
        raise ValueError("changed physical source provenance")
    if row["resources"]["peak_bytes"] != 86413302 or row["resources"]["last_stage"] != "snapshot complete" or row["resources"]["attempted_stage_work"] is not None:
        raise ValueError("changed fixed whole-live reserve receipt")


def validate(row, steps):
    """Validate completed protocol identity before deriving any cross-row comparison."""
    if type(row) is not dict or not report_finite(row):
        raise ValueError("invalid finite/depth/node report")
    full_schema(row)
    if row["schema"] != "quest-cylinder-p2-evolution-v1" or row["status"] != "completed":
        raise ValueError("not a completed protocol row")
    if row["request"] != {"steps": steps, "initial_condition": "PreparedMinimumMassCompatible"}:
        raise ValueError("changed experiment")
    p, r, s = row["progress"], row["report"], row["report"]["snapshot"]
    if (p["completed_steps"], p["drift_calls_attempted"], p["observer_calls_attempted"], p["failure_phase"]) != (steps, 4 * steps, steps, None):
        raise ValueError("incomplete integration progress")
    for value in (p["completed_steps"], p["drift_calls_attempted"], p["observer_calls_attempted"], row["energy_calls_attempted"], row["diagnostic_drift_calls_attempted"]):
        if type(value) is not int:
            raise ValueError("noninteger progress")
    if p["attempted_step"] is not None or row["error"] is not None:
        raise ValueError("unfinished completed row")
    if row["energy_calls_attempted"] != steps + 1 or row["diagnostic_drift_calls_attempted"] != 3:
        raise ValueError("incomplete diagnostics")
    if len(row["last_state"]) != 54 or any(type(x) not in (int, float) for x in row["last_state"]):
        raise ValueError("incomplete state")
    if len(row["energy_samples"]) != steps + 1 or any(x["integrated_energy"] < 0 for x in row["energy_samples"]):
        raise ValueError("invalid physical energy history")
    if r["end_time"] != 1e-4 or r["dt"] != 1e-4 / steps or p["accepted_time"] != 1e-4:
        raise ValueError("changed time interval")
    if any(type(r[key]) is not bool for key in ("integration_completed", "forward_accuracy_certified", "resolved_full_minus_linear_response", "quadratic_action_resolved")):
        raise ValueError("nonboolean claim")
    for index, sample in enumerate(row["energy_samples"]):
        expected = 0. if index == 0 else (index - 1) * r["dt"] + r["dt"]
        if sample["time"] != expected:
            raise ValueError("changed observation time")
    for key in ("state_change_norm", "quadratic_action_norm", "relative_quadratic_action"):
        if type(r[key]) not in (int, float) or r[key] < 0:
            raise ValueError("invalid diagnostic norm")
    if r["quadratic_probe_calls"] != 3 or r["quadratic_action_resolved"] != (r["relative_quadratic_action"] > 1e-10):
        raise ValueError("inconsistent quadratic diagnostic")
    if not r["integration_completed"] or r["forward_accuracy_certified"] or r["resolved_full_minus_linear_response"]:
        raise ValueError("unsupported scientific claim")
    if (s["geometry_fingerprint"], s["independent_dimension"], s["time"], s["reynolds"]) != (17853273085390264558, 54, 1e-4, 100):
        raise ValueError("changed full physical source")
    b = row["resources"]
    if b["cumulative_work"] != WORK[steps] or b["max_work"] != 2000000000 or b["max_bytes"] != 268435456 or b["peak_bytes"] > b["max_bytes"]:
        raise ValueError("changed resource contract")
    if any(type(b[key]) is not int or b[key] < 0 for key in ("cumulative_work", "max_work", "max_bytes", "peak_bytes")) or s["resources"] != b:
        raise ValueError("inconsistent resource receipt")
    if any(s["pressure"][key] < 0 for key in ("momentum_residual", "continuity_residual")):
        raise ValueError("negative residual")
    if len(s["pressure"]["pressure_coefficients"]) != 16 or any(len(p) != 3 for p in s["pressure"]["pressure_coefficients"]) or len(s["pressure"]["normal_multipliers"]) != 90:
        raise ValueError("incomplete pressure field")
    if s["pressure"]["normalization"] != "PrescribedMechanicalTraction" or s["pressure"]["normalization_residual"] is not None:
        raise ValueError("changed pressure level")
    return row


def _compare(rows):
    """Empirical sensitivity is separate from executed integration and action resolution."""
    if len(rows) != 3:
        return {"status": "unavailable", "reason": "requires all three validated completed rows"}
    for row, steps in zip(rows, STEPS):
        validate(row, steps)
    residual_ok = all(max(r["report"]["snapshot"]["pressure"][key] for key in ("continuity_residual", "momentum_residual")) <= 1e-8 for r in rows)
    metrics = {}
    state = [r["last_state"] for r in rows]
    a = math.hypot(*(x - y for x, y in zip(state[0], state[1])))
    b = math.hypot(*(x - y for x, y in zip(state[1], state[2])))
    metrics["full_coordinate_state"] = (a, b, max(1., math.hypot(*state[2])))
    for key in ("drag_coefficient", "lift_coefficient", "pressure_difference", "mean_kinetic_energy", "enstrophy"):
        x = [r["report"]["snapshot"][key] for r in rows]
        metrics[key] = (abs(x[0] - x[1]), abs(x[1] - x[2]), max(1., abs(x[2])))
    if any(not math.isfinite(value) for triple in metrics.values() for value in triple):
        raise ValueError("comparison arithmetic overflow")
    diagnostics = {key: {"difference_2_4": a, "difference_4_8": b,
                         "ratio": a / b if b > 1e-12 * scale else None,
                         "fine_scaled_difference": b / scale,
                         "sensitivity_pass": b <= 1e-5 * scale and b <= a + 1e-12 * scale}
                   for key, (a, b, scale) in metrics.items()}
    return {"status": "sensitivity-pass" if residual_ok and all(x["sensitivity_pass"] for x in diagnostics.values()) else "accuracy-failure",
            "original_residual_pass": residual_ok, "metrics": diagnostics,
            "quadratic_action_resolved_in_each_row": all(r["report"]["quadratic_action_resolved"] for r in rows),
            "resolved_full_minus_linear_response": False, "forward_accuracy_certified": False,
            "spatial_convergence_certified": False, "developed_cycle_acceptance": False}


def compare(rows):
    try:
        return _compare(rows)
    except (ValueError, OverflowError, TypeError, KeyError, RecursionError):
        return {"status": "accuracy-failure", "reason": "invalid or nonfinite comparison arithmetic"}


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (512 * 1024**2, 512 * 1024**2))
    resource.setrlimit(resource.RLIMIT_FSIZE, (4 * 1024**2, 4 * 1024**2))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: cylinder_p2_evolution.py PINNED_BINARY NEW_OUTPUT_DIRECTORY")
    binary = Path(sys.argv[1]).resolve()
    output = Path(sys.argv[2])
    output.mkdir(exist_ok=False)
    identity = digest(binary)
    receipt = {"schema": "quest-cylinder-p2-evolution-campaign-v1", "binary_sha256": identity,
               "bounds": {"address_space_bytes": 512 * 1024**2, "child_seconds": 180, "captured_file_bytes": 4 * 1024**2},
               "rows": [], "comparison": {"status": "unavailable"}}
    completed = []
    for steps in STEPS:
        if digest(binary) != identity:
            raise RuntimeError("binary changed; refusing further execution")
        stdout, stderr = output / f"steps-{steps}.json", output / f"steps-{steps}.stderr"
        started = time.monotonic()
        with stdout.open("xb") as out, stderr.open("xb") as err:
            child = subprocess.Popen([str(binary), str(steps)], stdout=out, stderr=err,
                                     preexec_fn=limits, start_new_session=True)
            status = "exited"
            try:
                code = child.wait(timeout=180)
            except subprocess.TimeoutExpired:
                status = "timeout"
                os.killpg(child.pid, signal.SIGKILL)
                code = child.wait()
        row = {"steps": steps, "process_status": status, "exit_code": code,
               "elapsed_seconds": time.monotonic() - started,
               "stdout": stdout.name, "stdout_sha256": digest(stdout), "stderr": stderr.name,
               "stderr_sha256": digest(stderr), "validated_completed": False}
        if status == "exited" and code == 0:
            try:
                parsed = decode_report(stdout.read_text())
                if type(parsed) is not dict:
                    raise ValueError("expected report object")
                status = parsed.get("status")
                row["protocol_status"] = status if type(status) is str and status in ("completed", "rejected", "failed-attempt") else "malformed"
                validate(parsed, steps)
                row["validated_completed"] = True
                completed.append(parsed)
            except (ValueError, KeyError, TypeError, OSError, OverflowError, RecursionError) as error:
                row["validation_error"] = str(error)
        receipt["rows"].append(row)
        (output / "receipt.json").write_text(json.dumps(receipt, indent=2, allow_nan=False) + "\n")
    receipt["comparison"] = compare(completed)
    receipt["binary_sha256_after"] = digest(binary)
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
