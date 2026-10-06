"""Independent protocol mutations: comparison cannot consume changed/truncated claims."""
import copy
import importlib.util
from pathlib import Path
import unittest
import subprocess
import tempfile
import sys

spec = importlib.util.spec_from_file_location("runner", Path(__file__).parents[1] / "examples/cylinder_p2_evolution.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def fixture(n):
    resource = {"cumulative_work": runner.WORK[n], "max_work": 2000000000, "max_bytes": 268435456, "peak_bytes": 86413302, "last_stage": "snapshot complete", "attempted_stage_work": None}
    snapshot = {"resources": resource, "geometry_fingerprint": 17853273085390264558, "independent_dimension": 54, "time": 1e-4, "reynolds": 100,
                "method": "classical complete P2 RK4 trajectory; fixed short window", "case": "shedding2d", "maximum_geometry_deviation": 0.01,
                "geometry": {"source_policy":"explicit-rectangle-corner-priority-v1", "requested_sectors":4, "retained_segments":8, "coalesced_rays":0,"maximum_coalesced_angle":0.,"represented_rectangle_residual":0.},
                "cylinder_force":[0.,0.,0.],
                "pressure": {"pressure_integral":0., "normalization": "PrescribedMechanicalTraction", "normalization_residual": None,
                             "momentum_residual": 1e-14, "continuity_residual": 1e-14,
                             "pressure_coefficients": [[0.] * 3 for _ in range(16)], "normal_multipliers": [0.] * 90}}
    for key in ("drag_coefficient", "lift_coefficient", "pressure_difference", "mean_kinetic_energy", "enstrophy"):
        snapshot[key] = 1.
    report = {"snapshot": snapshot, "initial_integrated_energy":1., "end_time": 1e-4, "dt": 1e-4 / n, "integration_completed": True,
              "forward_accuracy_certified": False, "resolved_full_minus_linear_response": False,
              "quadratic_action_resolved": False, "quadratic_probe_calls": 3,
              "state_change_norm": 0., "quadratic_action_norm": 0., "relative_quadratic_action": 0.}
    # The immutable real Rust Stage C serialization anchors shared pressure metadata.
    source = Path(__file__).resolve().parents[3] / "docs/verification/data/2026-10-06-cylinder-p2/runtime-snapshot.json"
    real_snapshot = runner.decode_report(source.read_text())["snapshot"]
    snapshot["pressure"]["mesh_resources"] = real_snapshot["pressure"]["mesh_resources"]
    if set(snapshot) != set(real_snapshot) or set(snapshot["pressure"]) != set(real_snapshot["pressure"]):
        raise AssertionError("synthetic outer fixture diverged from actual shared Rust serde schema")
    return {"schema": "quest-cylinder-p2-evolution-v1", "status": "completed", "request": {"steps": n, "initial_condition": "PreparedMinimumMassCompatible"},
            "progress": {"completed_steps": n, "drift_calls_attempted": n * 4, "observer_calls_attempted": n,
                         "accepted_time": 1e-4, "failure_phase": None, "attempted_step": None},
            "energy_calls_attempted": n + 1, "diagnostic_drift_calls_attempted": 3, "last_state": [0.] * 54,
            "energy_samples": [{"time": 0. if i == 0 else (i - 1) * report["dt"] + report["dt"], "integrated_energy": 1.} for i in range(n + 1)],
            "resources": resource, "report": report, "error": None, "temporal_accuracy_assessed":False}


class Runner(unittest.TestCase):
    def test_validated_comparison_and_separate_accuracy_failure(self):
        rows = [fixture(n) for n in runner.STEPS]
        self.assertEqual(runner.compare(rows)["status"], "sensitivity-pass")
        rows[2]["report"]["snapshot"]["pressure"]["momentum_residual"] = 1e-7
        self.assertEqual(runner.compare(rows)["status"], "accuracy-failure")
        self.assertEqual(runner.compare(rows[:2])["status"], "unavailable")

    def test_malformed_children_preserve_all_fixed_rows(self):
        for payload in ('{"status":1e999}', '[' * 1100 + '0' + ']' * 1100):
            with tempfile.TemporaryDirectory() as directory:
                binary = Path(directory) / "fake-row"
                binary.write_text("#!/usr/bin/python3\nprint(" + repr(payload) + ")\n")
                binary.chmod(0o700)
                output = Path(directory) / "results"
                result = subprocess.run([sys.executable, "-B", str(Path(runner.__file__)), str(binary), str(output)], capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr.decode())
                receipt = runner.json.loads((output / "receipt.json").read_text())
                self.assertEqual(len(receipt["rows"]), 3)
                self.assertTrue(all(not row["validated_completed"] for row in receipt["rows"]))

    def test_strict_shared_decoder_rejects_duplicate_keys_and_depth(self):
        for payload in ('{"status":"completed","status":"completed"}', '{"x":NaN}', '{"x":1e999}', '[' * 1100 + '0' + ']' * 1100):
            with self.subTest(payload=payload[:40]), self.assertRaises(ValueError):
                runner.decode_report(payload)

    def test_nonphysical_payloads_cannot_be_completed_rows(self):
        for path, value in [(('report', 'snapshot', 'mean_kinetic_energy'), 'corrupt'),
                            (('resources', 'peak_bytes'), 0),
                            (('temporal_accuracy_assessed',), True)]:
            row = fixture(2)
            owner = row
            for key in path[:-1]:
                owner = owner[key]
            owner[path[-1]] = value
            with self.subTest(path=path), self.assertRaises(ValueError):
                runner.validate(row, 2)
        row = fixture(2)
        row['report']['snapshot']['pressure']['pressure_coefficients'][0][0] = 'corrupt'
        with self.assertRaises(ValueError):
            runner.validate(row, 2)
        row = fixture(2)
        row['report']['snapshot']['pressure']['normal_multipliers'][0] = True
        with self.assertRaises(ValueError):
            runner.validate(row, 2)

    def test_changed_protocols_reject(self):
        mutations = [("last_state", [0.] * 53), ("status", "failed-attempt"), ("energy_calls_attempted", True),
                     ("diagnostic_drift_calls_attempted", 2), ("error", "failed"), ("last_state", [float("nan")] * 54)]
        for key, value in mutations:
            row = fixture(2)
            row[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                runner.validate(row, 2)
        for branch, key, value in [("request", "steps", 4), ("progress", "accepted_time", 30.),
                                   ("report", "forward_accuracy_certified", True), ("resources", "max_work", 3000000000)]:
            row = copy.deepcopy(fixture(2))
            row[branch][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                runner.validate(row, 2)


if __name__ == "__main__":
    unittest.main()
