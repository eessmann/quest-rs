"""Comparisons require matching input, scope, backend, accuracy and protocol."""
import importlib.util
from pathlib import Path
import unittest
import tempfile
import json

spec = importlib.util.spec_from_file_location("report", Path(__file__).with_name("report.py"))
report = importlib.util.module_from_spec(spec)
if Path(spec.origin).exists():
    spec.loader.exec_module(report)


class ComparisonContracts(unittest.TestCase):
    def test_medians_use_verified_raw_samples_not_mutable_estimates(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "row/new").mkdir(parents=True)
            (root / "row/new/sample.json").write_text(json.dumps({"times": list(range(1, 11)), "iters": [1] * 10}))
            (root / "row/new/estimates.json").write_text('{"median": {"point_estimate": 999}}')
            result = {"raw_samples": [{"path": "row/new/sample.json"}]}
            self.assertEqual(report.point_estimate(root, result), 5.5)

    def test_admission_upper_bound_is_not_reported_as_a_native_crash_or_proven_violation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = root / "row.log"
            log.write_text("Error: Contractivity { upper: 1.12 }\n")
            from campaign import sha256
            result = {"status": "native_error", "phase": "preflight", "log": log.name,
                      "log_sha256": sha256(log)}
            self.assertEqual(report.outcome_diagnostic(root, result)["category"], "numerical_admission_not_established")
            self.assertEqual(result["status"], "native_error")
            log.write_text('Error: Numerics(Budget { resource: "bytes" })\n')
            result.update(status="preparation_failure", log_sha256=sha256(log))
            self.assertEqual(report.outcome_diagnostic(root, result)["category"], "resource_limit")

    def test_mismatched_scope_input_or_tolerance_cannot_form_speedup(self):
        self.assertTrue(hasattr(report, "matching_contract"), "comparison contract is missing")
        original = dict(input_sha256="one", scope="inverse_only", backend="scalar", workers=1,
                        protocol="criterion_10", requested_accuracy=1e-12,
                        effective_completion_tolerance=1e-12)
        self.assertTrue(report.matching_contract(original, dict(original)))
        for name, value in (("input_sha256", "two"), ("scope", "forward_only"),
                            ("requested_accuracy", 1e-10), ("workers", 2)):
            changed = dict(original, **{name: value})
            self.assertFalse(report.matching_contract(original, changed))


if __name__ == "__main__":
    unittest.main()
