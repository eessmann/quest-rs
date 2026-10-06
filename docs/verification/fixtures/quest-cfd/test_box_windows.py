#!/usr/bin/env python3
"""Semantic acceptance of bounded classical box-window receipts."""
import copy
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import box_windows


class WindowValidation(unittest.TestCase):
    def setUp(self):
        self.spec = {"case": "tgv2d", "reynolds": 100, "order": 1,
                     "mesh": 2, "dt": 0.01, "steps": 1000, "horizon": 10.0}
        obs = {"case": "tgv2d", "reynolds": 100, "time": 10.0,
               "independent_dimension": 17, "mean_kinetic_energy": 0.1,
               "mean_enstrophy": 0.2, "mean_gradient_dissipation": 0.004,
               "steady_residual_l2": 0.01, "analytic_velocity_error_l2": 0.2,
               "analytic_pressure_error_l2": 0.1, "pressure": {"continuity_residual": 1e-12, "momentum_residual": 1e-12,
                            "pressure_mean_residual": 1e-12},
               "method": "classical RK4 of the complete BDM1/P0 DG ODE"}
        self.record = {"exit_code": 0, "timed_out": False,
                       "report": {"status": "classical-reference-executed",
                                  "quantum_execution": False,
                                  "benchmark_convergence_established": False,
                                  "reference": obs}}

    def test_valid_low_and_high_order(self):
        self.assertEqual(box_windows.validate(self.spec, self.record)["time"], 10.)
        spec = {**self.spec, "order": 2}
        wrapped = copy.deepcopy(self.record)
        wrapped["report"]["reference"] = {"physical_order": 2,
                                          **self.record["report"]["reference"]}
        wrapped["report"]["reference"]["method"] = "classical RK4 of the complete BDM2/P1 DG ODE"
        wrapped["report"]["reference"]["pressure"]["gauge_residual"] = 1e-12
        self.assertEqual(box_windows.validate(spec, wrapped)["independent_dimension"], 17)
        wrapped["report"]["reference"]["physical_order"] = 1
        with self.assertRaises(ValueError):
            box_windows.validate(spec, wrapped)

    def test_wrong_case_window_or_success_is_rejected(self):
        for key, value in [("case", "cavity2d"), ("time", 0.0002),
                           ("reynolds", 1600), ("independent_dimension", 0)]:
            r = copy.deepcopy(self.record)
            r["report"]["reference"][key] = value
            with self.assertRaises(ValueError):
                box_windows.validate(self.spec, r)
        for key, value in [("exit_code", 1), ("timed_out", True)]:
            with self.assertRaises(ValueError):
                box_windows.validate(self.spec, {**self.record, key: value})

    def test_nonfinite_and_false_execution_claims_reject(self):
        r = copy.deepcopy(self.record)
        r["report"]["reference"]["pressure"]["residual"] = float("nan")
        with self.assertRaises(ValueError):
            box_windows.validate(self.spec, r)
        for key, value in [("quantum_execution", True),
                           ("benchmark_convergence_established", True),
                           ("status", "construction-only")]:
            r = copy.deepcopy(self.record)
            r["report"][key] = value
            with self.assertRaises(ValueError):
                box_windows.validate(self.spec, r)

    def test_variants_reach_frozen_windows_and_change_one_axis(self):
        experiments = list(box_windows.experiments())
        self.assertGreater(len(experiments), 10)
        for x in experiments:
            self.assertAlmostEqual(x["dt"] * x["steps"], x["horizon"])
            self.assertIn(x["axis"], ("baseline", "physical-h", "physical-p", "time"))
            self.assertEqual(x["horizon"], 10 if x["case"] == "tgv2d"
                             else 20 if x["case"] == "tgv3d" else 100)
            if x["axis"] == "time":
                peers = [y for y in experiments if all(y[k] == x[k] for k in
                         ("case", "reynolds", "order", "mesh", "horizon")) and y["axis"] != "time"]
                self.assertTrue(peers)
                self.assertAlmostEqual(x["dt"] * 2, peers[0]["dt"])

    def test_explicit_work_limit_must_match_and_cover_reported_work(self):
        spec = {**self.spec, "order": 2, "max_classical_work": 1000}
        r = copy.deepcopy(self.record)
        r["report"]["reference"].update(physical_order=2,
            integration_work_limit=1000, integration_modeled_work=800,
            method="classical RK4 of the complete BDM2/P1 DG ODE")
        r["report"]["reference"]["pressure"]["gauge_residual"] = 1e-12
        box_windows.validate(spec, r)
        for fields in [{"integration_work_limit": 1001},
                       {"integration_modeled_work": 1001},
                       {"integration_modeled_work": True}]:
            bad = copy.deepcopy(r)
            bad["report"]["reference"].update(fields)
            with self.assertRaises(ValueError):
                box_windows.validate(spec, bad)

    def test_stability_followup_keeps_the_failed_physical_problem(self):
        variants = list(box_windows.cavity_stability_experiments())
        self.assertEqual([v["dt"] for v in variants], [0.0025, 0.00125])
        for v in variants:
            self.assertEqual((v["case"], v["reynolds"], v["order"], v["mesh"]),
                             ("cavity2d", 100, 2, 4))
            self.assertEqual(v["dt"] * v["steps"], 100)
            self.assertEqual(v["axis"], "time")

    def test_wrong_order_and_missing_required_diagnostics_reject(self):
        for changes in [{"physical_order": 2}, {"method": "a different solver"},
                        {"pressure": None}, {"analytic_velocity_error_l2": None},
                        {"analytic_pressure_error_l2": -1}]:
            r = copy.deepcopy(self.record)
            r["report"]["reference"].update(changes)
            with self.assertRaises(ValueError):
                box_windows.validate(self.spec, r)

    def test_deep_child_report_is_rejected_without_python_recursion(self):
        r = copy.deepcopy(self.record)
        junk = 0
        for _ in range(600):
            junk = [junk]
        r["report"]["junk"] = junk
        with self.assertRaises(ValueError):
            box_windows.validate(self.spec, r)

    def test_collector_parser_rejects_ambiguous_and_unbounded_json(self):
        from campaign import decode_report
        self.assertEqual(decode_report(b'{"x": 1}'), {"x": 1})
        for payload in [b'{"x":1,"x":2}', b'{"x":NaN}', b'{"x":1e999}',
                        b'{"x":' + b'[' * 2000 + b'0' + b']' * 2000 + b'}']:
            with self.assertRaises(ValueError):
                decode_report(payload)

    def test_actual_collector_rejects_deep_child_output(self):
        from campaign import collect
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "nested-child"
            payload = '{"x":' + '[' * 600 + '0' + ']' * 600 + '}'
            binary.write_text("#!/usr/bin/python3\nprint(" + repr(payload) + ")\n")
            binary.chmod(0o700)
            result = collect(binary, [], 128 * 1024**2, 5)
            self.assertEqual(result["exit_code"], 0)
            self.assertFalse(result["valid_report"])
            self.assertIsNone(result["report"])

    def test_cavity_diagnostics_require_numeric_samples_and_metrics(self):
        spec = {**self.spec, "case": "cavity3d"}
        r = copy.deepcopy(self.record)
        point = {"point": [0., 0., 0.], "velocity": [0., 0., 0.]}
        r["report"]["reference"].update(case="cavity3d", centerline_profiles=[point],
            cavity_3d={"reflection_maximum_defect": 0., "reflection_rms_defect": 0.,
                       "sampled_spanwise_velocity_rms": 0., "reflection_pairs": 1,
                       "sample_queries": 2, "x_midplane": [point], "y_midplane": [point]})
        box_windows.validate(spec, r)
        for key, value in [("centerline_profiles", True), ("cavity_3d", {}),
                           ("centerline_profiles", [{"point": [0.], "velocity": [0.]}])]:
            bad = copy.deepcopy(r)
            bad["report"]["reference"][key] = value
            with self.assertRaises(ValueError):
                box_windows.validate(spec, bad)
        for key, value in [("reflection_pairs", True), ("reflection_rms_defect", -1),
                           ("x_midplane", []), ("sample_queries", 0)]:
            bad = copy.deepcopy(r)
            bad["report"]["reference"]["cavity_3d"][key] = value
            with self.assertRaises(ValueError):
                box_windows.validate(spec, bad)

    def test_nonfinite_child_is_saved_as_rejection_in_valid_json(self):
        bad = copy.deepcopy(self.record)
        bad["report"]["reference"]["pressure"]["residual"] = float("nan")
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "fixture"
            binary.write_bytes(b"unused synthetic executable")
            output = Path(directory) / "result.json"
            with (mock.patch.object(box_windows, "collect", return_value=bad),
                  mock.patch.object(box_windows, "experiments", return_value=[self.spec]),
                  mock.patch("sys.argv", ["box_windows", str(binary), str(output)]),
                  contextlib.redirect_stdout(io.StringIO())):
                box_windows.main()
            text = output.read_text()
            self.assertNotIn("NaN", text)
            record = json.loads(text)["experiments"][0]
            self.assertEqual(record["status"], "rejected-failed-or-invalid")
            self.assertTrue(record["execution"]["nonfinite_report_omitted"])
            self.assertIsNone(record["execution"]["report"])


if __name__ == "__main__":
    unittest.main()
