#!/usr/bin/env python3
"""Capped full-window box references; successful execution is not convergence."""
import argparse
import datetime
import json
import math
from pathlib import Path

from campaign import collect, sha256, report_finite as finite


def experiments():
    for case, reynolds, horizon in [
        ("tgv2d", 100, 10), ("tgv3d", 100, 20), ("tgv3d", 1600, 20),
        ("cavity2d", 100, 100), ("cavity2d", 1000, 100),
        ("cavity3d", 100, 100), ("cavity3d", 1000, 100),
    ]:
        mesh_values = (1, 2, 4) if case.endswith("2d") else (1, 2)
        for order in (1, 2):
            for mesh in mesh_values:
                yield {"case": case, "reynolds": reynolds, "horizon": horizon,
                       "order": order, "mesh": mesh, "dt": 0.01,
                       "steps": horizon * 100,
                       "axis": "physical-p" if order == 2 else
                               "baseline" if mesh == 1 else "physical-h"}
        yield {"case": case, "reynolds": reynolds, "horizon": horizon,
               "order": 1, "mesh": 2 if case.endswith("2d") else 1,
               "dt": 0.005, "steps": horizon * 200, "axis": "time"}
        yield {"case": case, "reynolds": reynolds, "horizon": horizon,
               "order": 2, "mesh": 4 if case.endswith("2d") else 1,
               "dt": 0.005, "steps": horizon * 200, "axis": "time"}


def cavity_stability_experiments():
    """Explicit follow-up to the failed Re100 p2 mesh4 window, with fixed caps."""
    for dt, steps in [(0.0025, 40000), (0.00125, 80000)]:
        yield {"case": "cavity2d", "reynolds": 100, "horizon": 100,
               "order": 2, "mesh": 4, "dt": dt, "steps": steps, "axis": "time"}


def valid_samples(samples):
    return (isinstance(samples, list) and bool(samples) and all(
        isinstance(sample, dict) and all(
            isinstance(sample.get(key), list) and len(sample[key]) == 3
            and all(type(x) in (float, int) for x in sample[key])
            for key in ("point", "velocity")) for sample in samples))


def validate(spec, result):
    """Validate the executed scope without inventing physical error tolerances."""
    if result.get("exit_code") != 0 or result.get("timed_out"):
        raise ValueError("child rejected, failed or exceeded its deadline")
    report = result.get("report")
    if not isinstance(report, dict) or not finite(report):
        raise ValueError("missing or nonfinite report")
    if (report.get("status") != "classical-reference-executed"
            or report.get("quantum_execution") is not False
            or report.get("benchmark_convergence_established") is not False):
        raise ValueError("incorrect execution or convergence classification")
    obs = report.get("reference")
    if not isinstance(obs, dict):
        raise ValueError("missing physical observations")
    # HigherOrderSnapshot uses serde(flatten), retaining observations here.
    order = spec["order"]
    expected_method = f"classical RK4 of the complete BDM{order}/P{order - 1} DG ODE"
    if obs.get("physical_order", 1) != order or obs.get("method") != expected_method:
        raise ValueError("incorrect physical order or reference method")
    if obs.get("case") != spec["case"] or obs.get("reynolds") != spec["reynolds"]:
        raise ValueError("wrong case or Reynolds convention")
    time = obs.get("time")
    if (not isinstance(time, (float, int)) or isinstance(time, bool)
            or not math.isclose(time, spec["horizon"], rel_tol=1e-12, abs_tol=1e-12)):
        raise ValueError("wrong physical observation time")
    dimension = obs.get("independent_dimension")
    if type(dimension) is not int or dimension <= 0:
        raise ValueError("missing complete coordinate count")
    if "max_classical_work" in spec:
        work = obs.get("integration_modeled_work")
        if (obs.get("integration_work_limit") != spec["max_classical_work"]
                or type(work) is not int or not 0 < work <= spec["max_classical_work"]):
            raise ValueError("missing or violated explicit integration-work allowance")
    keys = ("mean_kinetic_energy", "mean_enstrophy", "mean_gradient_dissipation",
            "steady_residual_l2")
    if any(type(obs.get(k)) not in (float, int) or obs[k] < 0 for k in keys):
        raise ValueError("missing or invalid physical diagnostics")
    pressure = obs.get("pressure")
    pressure_keys = ("continuity_residual", "momentum_residual",
                     "pressure_mean_residual" if order == 1 else "gauge_residual")
    if (not isinstance(pressure, dict) or any(type(pressure.get(k)) not in (float, int)
                                              or pressure[k] < 0 for k in pressure_keys)):
        raise ValueError("missing or invalid pressure/constraint diagnostics")
    if spec["case"] == "tgv2d" and any(type(obs.get(k)) not in (float, int) or obs[k] < 0
            for k in ("analytic_velocity_error_l2", "analytic_pressure_error_l2")):
        raise ValueError("missing or invalid Taylor-Green analytic errors")
    if spec["case"].startswith("cavity") and not valid_samples(obs.get("centerline_profiles")):
        raise ValueError("missing cavity profiles")
    if spec["case"] == "cavity3d":
        cavity = obs.get("cavity_3d")
        metrics = ("reflection_maximum_defect", "reflection_rms_defect",
                   "sampled_spanwise_velocity_rms")
        if (not isinstance(cavity, dict)
                or any(type(cavity.get(k)) not in (float, int) or cavity[k] < 0 for k in metrics)
                or any(type(cavity.get(k)) is not int or cavity[k] <= 0
                       for k in ("reflection_pairs", "sample_queries"))
                or any(not valid_samples(cavity.get(k)) for k in ("x_midplane", "y_midplane"))):
            raise ValueError("missing transverse/symmetry diagnostics")
    return {k: obs.get(k) for k in ("time", "independent_dimension", *keys,
            "analytic_velocity_error_l2", "analytic_pressure_error_l2",
            "primary_vortex_candidate")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--cap-mib", type=int, default=512)
    parser.add_argument("--timeout", type=float, default=60)
    scope = parser.add_mutually_exclusive_group()
    scope.add_argument("--higher-order-only", action="store_true")
    scope.add_argument("--cavity-stability-study", action="store_true")
    parser.add_argument("--max-classical-work", type=int)
    args = parser.parse_args()
    if args.cap_mib <= 0 or not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("positive cap and timeout required")
    if args.max_classical_work is not None and args.max_classical_work <= 0:
        parser.error("positive explicit classical work allowance required")
    binary = args.binary.resolve(strict=True)
    root = Path(__file__).resolve().parents[4]
    manifest_files = [root / "crates/quest-cfd/cases" / (name + ".json")
                      for name in ("tgv2d", "tgv3d", "cavity2d", "cavity3d")]
    receipt = {
        "schema": "quest-cfd-full-box-windows.v1",
        "recorded_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "binary_sha256": sha256(binary),
        "runner_sha256": sha256(Path(__file__)),
        "collector_sha256": sha256(Path(__file__).with_name("campaign.py")),
        "manifest_sha256": {p.name: sha256(p) for p in manifest_files},
        "provenance_scope": "binary, runner and frozen manifest identities; no full build attestation",
        "scope": "classical complete-coordinate final-window snapshots on one local host",
        "quantum_execution": False, "benchmark_convergence_established": False,
        "experiments": [],
        "limitations": [
            "These are final-window snapshots, not time-averaged observation-window statistics.",
            "Every admitted physical coordinate is retained; mesh refinements change their count.",
            "Explicit RK4 dt=.01/.005 can reject or be inaccurate; temporal differences remain separate.",
            "A completed coarse cavity or high-Re Taylor-Green run is not a resolved published benchmark.",
            "Physical mesh/order and temporal variants must be compared at fixed other parameters.",
            "Timeout, overflow, resource rejection and malformed output remain failed attempts.",
            "Cylinder geometry/window evidence is recorded separately; literal3D wake remains unresolved.",
            "No lift, circuit, sampling, multi-host, hardware or quantum advantage claim.",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    variants = cavity_stability_experiments() if args.cavity_stability_study else experiments()
    for spec in variants:
        if args.higher_order_only and spec["order"] != 2:
            continue
        arguments = ["reference", "--case", spec["case"], "--reynolds", str(spec["reynolds"]),
                     "--mesh", str(spec["mesh"]), "--physical-order", str(spec["order"]),
                     "--dt", str(spec["dt"]), "--steps", str(spec["steps"])]
        if spec["order"] == 2 and args.max_classical_work is not None:
            arguments += ["--max-classical-work", str(args.max_classical_work)]
            spec = {**spec, "max_classical_work": args.max_classical_work}
        record = {**spec, "execution": collect(binary, arguments, args.cap_mib * 1024**2,
                                               args.timeout)}
        try:
            record["diagnostics"] = validate(spec, record["execution"])
            record["status"] = "classical-window-executed"
        except ValueError as error:
            record["status"] = "rejected-failed-or-invalid"
            record["validation_error"] = str(error)
        if not finite(record["execution"].get("report")):
            # Preserve rejection in valid JSON rather than fail while saving NaN.
            record["execution"]["report"] = None
            record["execution"]["nonfinite_report_omitted"] = True
        receipt["experiments"].append(record)
        receipt["complete"] = False
        args.output.write_text(json.dumps(receipt, indent=2, allow_nan=False) + "\n")
        print(f"{spec['case']} Re{spec['reynolds']} p{spec['order']} mesh{spec['mesh']} "
              f"dt{spec['dt']}: {record['status']}", flush=True)
    receipt["complete"] = True
    receipt["binary_unchanged_after"] = sha256(binary) == receipt["binary_sha256"]
    if not receipt["binary_unchanged_after"]:
        receipt["integrity_failure"] = "binary changed during campaign; results are not accepted"
    args.output.write_text(json.dumps(receipt, indent=2, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
