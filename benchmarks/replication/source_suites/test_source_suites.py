"""Contract checks for exact-source coverage and separate Rust measurements."""
import hashlib
import importlib.util
import struct
import tempfile
import unittest
from pathlib import Path

PATH = Path(__file__).with_name("run.py")
SPEC = importlib.util.spec_from_file_location("source_suites", PATH)
runner = importlib.util.module_from_spec(SPEC) if PATH.exists() else None
if runner is not None:
    SPEC.loader.exec_module(runner)


class SourceSuites(unittest.TestCase):
    def test_every_exact_source_boundary_remains_explicit(self):
        self.assertIsNotNone(runner, "source-suite controller is missing")
        rows = runner.source_manifest()
        self.assertEqual(len(rows), 33)
        self.assertEqual(len({row["id"] for row in rows}), 33)
        self.assertEqual({row["protocol"] for row in rows if row["suite"] == "coordinate"},
                         {"fixed_3", "criterion_10"})
        for row in rows:
            self.assertTrue(row["missing_rust_contract"])
            self.assertTrue(row["source_location"])

    def test_unitary_comparison_does_not_claim_certified_proof_equivalence(self):
        self.assertIsNotNone(runner, "source-suite controller is missing")
        rows = runner.unitary_manifest("original")
        self.assertEqual([row["dimension"] for row in rows], [64, 128, 256, 512])
        for row in rows:
            self.assertFalse(row["source_boundary_equivalent"])
            self.assertEqual(row["requested_accuracy"], 0.0)
            self.assertEqual(row["scope"], "numerical_storage_and_gram_admission")
            dimension = row["dimension"]
            payload = b"".join(struct.pack("<dd", float(r == c), 0.0)
                               for r in range(dimension) for c in range(dimension))
            self.assertEqual(row["input_sha256"], hashlib.sha256(payload).hexdigest())

    def test_unverified_native_failure_cannot_complete_source_rows(self):
        self.assertIsNotNone(runner, "source-suite controller is missing")
        with self.assertRaisesRegex(ValueError, "autodiff"):
            runner.check_native_failure("configuration stopped for an unknown reason")

    def test_timeout_phase_tracks_criterion_measurement(self):
        self.assertTrue(hasattr(runner, "reported_phase"), "measurement phase tracking missing")
        self.assertEqual(runner.reported_phase("warmup", "Benchmarking: Warming up"), "warmup")
        self.assertEqual(runner.reported_phase("warmup", "Benchmarking: Collecting 10 samples"),
                         "measurement")
        self.assertEqual(runner.reported_phase("complete", "Benchmarking: Collecting 10 samples"),
                         "complete")

    def test_pinned_controller_rejects_changed_imported_helper(self):
        self.assertTrue(hasattr(runner, "verify_controllers"), "controller provenance guard missing")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "source_suites").mkdir()
            (root / "source_suites/run.py").write_text("controller version one\n")
            helper = root / "campaign.py"
            helper.write_text("helper version one\n")
            pin = {"source_suites/run.py": hashlib.sha256((root / "source_suites/run.py").read_bytes()).hexdigest(),
                   "campaign.py": hashlib.sha256(helper.read_bytes()).hexdigest()}
            runner.verify_controllers(root, pin)
            helper.write_text("changed helper\n")
            with self.assertRaisesRegex(RuntimeError, "controller.*changed"):
                runner.verify_controllers(root, pin)


if __name__ == "__main__":
    unittest.main()
