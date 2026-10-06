"""Receipt admission tests use actual captured executions, never rerun the long trace."""
import copy
import importlib.util
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
spec = importlib.util.spec_from_file_location("cylinder_run", HERE / "run.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
DATA = ROOT / "docs/verification/data/2026-10-05-cylinder-window"


class ReceiptAdmission(unittest.TestCase):
    def setUp(self):
        self.receipt = json.loads((DATA / "tiny-final.json").read_text())

    def validate(self, native):
        runner.validate_completed(native, self.receipt["parameters"],
                                  self.receipt["process_address_space_cap_bytes"], ROOT)

    def test_actual_small_and_historical_full_window(self):
        for name in ("tiny-final.json", "full8-coarse.json"):
            receipt = json.loads((DATA / name).read_text())
            runner.validate_completed(receipt["native_receipt"], receipt["parameters"],
                                      receipt["process_address_space_cap_bytes"], ROOT)

    def test_malformed_completion_is_rejected(self):
        mutations = {
            "missing result": lambda r: r.update(result=None),
            "empty trace": lambda r: r["result"].update(trace=[]),
            "zero progress": lambda r: r["progress"].update(completed_steps=0, time=0),
            "convergence claim": lambda r: r.update(convergence_certified=True),
            "quantum claim": lambda r: r.update(quantum_execution=True),
            "wrong schema": lambda r: r.update(schema="other"),
            "negative timing": lambda r: r.update(elapsed_seconds=-1),
            "wrong steps": lambda r: r["parameters"].update(steps=1),
            "missing statistics": lambda r: r["result"].update(statistics=None),
            "NaN force": lambda r: r["result"]["trace"][0].update(drag=float("nan")),
            "infinite pressure": lambda r: r["result"]["trace"][0].update(pressure_difference=float("inf")),
            "negative RMS": lambda r: r["result"]["statistics"].update(lift_rms=-1),
            "invented mean": lambda r: r["result"]["statistics"].update(mean_drag=100),
            "wrong trace time": lambda r: r["result"]["trace"][-1].update(time=1),
            "wrong count": lambda r: r["progress"].update(observations_completed=1),
            "missing work": lambda r: r["result"]["admission"].pop("aggregate_work"),
            "negative residual": lambda r: r["result"].update(maximum_boundary_residual=-1),
            "high residual": lambda r: r["result"].update(maximum_momentum_residual=1),
            "zero RSS": lambda r: r["process"].update(rss_high_water_bytes=0),
            "wrong cap": lambda r: r["process"].update(address_space_cap_bytes=1),
            "bool count": lambda r: r["progress"].update(completed_steps=True),
            "unknown field": lambda r: r.update(unrecognized=1),
            "wrong manifest": lambda r: r["result"]["manifest"].update(id="steady2d"),
            "invented frequency": lambda r: r["result"]["statistics"].update(frequency={"strouhal": 0.2}),
        }
        for name, mutate in mutations.items():
            with self.subTest(name=name):
                native = copy.deepcopy(self.receipt["native_receipt"])
                mutate(native)
                with self.assertRaises(ValueError):
                    self.validate(native)

    def test_requested_parameters_are_bound(self):
        with self.assertRaises(ValueError):
            runner.validate_completed(self.receipt["native_receipt"], ["--steps", "1"],
                                      self.receipt["process_address_space_cap_bytes"], ROOT)

    def test_process_success_with_invalid_receipt_is_not_completion(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = pathlib.Path(directory)
            original = json.dumps(self.receipt["native_receipt"])
            payloads = {
                "null_result": json.dumps({**self.receipt["native_receipt"], "result": None}),
                "nan": json.dumps({**self.receipt["native_receipt"], "elapsed_seconds": float("nan")}),
                "overflow": original.replace('"elapsed_seconds": ' + str(self.receipt["native_receipt"]["elapsed_seconds"]), '"elapsed_seconds": 1e999'),
                "duplicate": original.replace('{"schema":', '{"schema":"other","schema":', 1),
                "depth": '['*1000+'0'+']'*1000,
            }
            for name, payload in payloads.items():
                emitter = directory / (name + ".py")
                emitter.write_text("#!/usr/bin/env python3\nprint(" + repr(payload) + ")\n")
                emitter.chmod(0o700)
                output = directory / (name + ".json")
                process = subprocess.run([sys.executable, str(HERE / "run.py"), "--binary", str(emitter),
                                          "--output", str(output), "--", *self.receipt["parameters"]],
                                         capture_output=True, text=True, check=False, timeout=10)
                receipt = json.loads(output.read_text())
                self.assertEqual(process.returncode, 1)
                self.assertEqual(receipt["status"], "invalid_receipt")
                self.assertIs(receipt["completed"], False)
                self.assertIn("receipt_error", receipt)

    def test_output_spool_caps_and_tail(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = pathlib.Path(directory)
            for name, program in (
                    ("stdout", "import os\nfor _ in range(80): os.write(1,b'x'*1048576)"),
                    ("stderr", "import os,time\nfor _ in range(5): os.write(2,b'x'*1048576)\ntime.sleep(0.2)")):
                emitter = directory / (name + ".py")
                emitter.write_text("#!/usr/bin/env python3\n" + program + "\n")
                emitter.chmod(0o700)
                output = directory / (name + ".json")
                process = subprocess.run([sys.executable, str(HERE / "run.py"), "--binary", str(emitter),
                                          "--output", str(output)], capture_output=True,
                                         text=True, check=False, timeout=10)
                receipt = json.loads(output.read_text())
                self.assertEqual(process.returncode, 1)
                self.assertEqual(receipt["status"], "output_limit")
                self.assertIs(receipt["completed"], False)
                self.assertLessEqual(len(receipt["diagnostics"]), runner.STDERR_TAIL)
                if name == "stderr":
                    self.assertIs(receipt["stderr_tail_truncated"], True)


if __name__ == "__main__":
    unittest.main()
