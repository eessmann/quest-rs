import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

PATH = Path(__file__).with_name("run_named_inverse.py")
SPEC = importlib.util.spec_from_file_location("named_inverse", PATH)
inverse = importlib.util.module_from_spec(SPEC)
if PATH.exists():
    SPEC.loader.exec_module(inverse)


class NamedInverseContracts(unittest.TestCase):
    def test_timeout_after_collection_reports_measurement_phase(self):
        self.assertTrue(hasattr(inverse, "reported_phase"), "measurement phase transition missing")
        self.assertEqual(inverse.reported_phase("warmup", "Collecting 10 samples"), "measurement")

    def test_fixture_cannot_relax_recorded_source_tolerance(self):
        value = dict(degree=5, canonical_coefficients_bits=[[0, 0]] * 6,
                     conjugate_complement_bits=[[0, 0]] * 6,
                     requested_tolerance_bits=4607182418800017408)
        with self.assertRaises(ValueError):
            inverse.validate_pair(value, 5)
        del value["requested_tolerance_bits"]
        value["expected_reflections_bits"] = [[0, 0]]
        with self.assertRaises(ValueError):
            inverse.validate_pair(value, 5)

    def test_source_attribution_requires_receipt_artifact_hashes(self):
        self.assertTrue(hasattr(inverse, "verify_export_receipt"), "export receipt binding missing")
        receipt = dict(source_revision="4fc35983138d07a990862a4d83ad16f2b737c98f",
                       export_exit_code=0, artifacts={"named-fixtures/order5.json": "known"})
        with self.assertRaises(ValueError):
            inverse.verify_export_receipt(receipt, {"named-fixtures/order5.json": "changed"})

    def test_criterion_base_copy_is_not_a_second_measurement(self):
        self.assertTrue(hasattr(inverse, "inverse_samples"), "new sample selection missing")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("base", "new"):
                path = root / "criterion" / "inverse_only" / name
                path.mkdir(parents=True)
                (path / "sample.json").write_text('{}')
            self.assertEqual(inverse.inverse_samples(root), [root / "criterion" / "inverse_only" / "new" / "sample.json"])

    def test_snapshot_keeps_exact_bytes_and_accounts_for_missing_exports(self):
        self.assertTrue(hasattr(inverse, "prepare_inputs"), "exact input snapshot missing")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            exports = root / "exports"
            exports.mkdir()
            payload = json.dumps(dict(degree=5, canonical_coefficients_bits=[[0, 9223372036854775808]] * 6,
                                      conjugate_complement_bits=[[4607182418800017408, 0]] + [[0, 0]] * 5)).encode()
            (exports / "order5.json").write_bytes(payload)
            rows = inverse.prepare_inputs(exports, root / "snapshot")["rows"]
            self.assertEqual(len(rows), 17)
            self.assertEqual(sum(row["input_status"] == "available" for row in rows), 1)
            self.assertEqual(sum(row["input_status"] == "not_exported" for row in rows), 16)
            self.assertEqual((root / "snapshot" / "fixtures" / "order5.json").read_bytes(), payload)
            self.assertEqual(rows[0]["requested_accuracy"], 1e-12)
            self.assertIn("roundtrip", rows[0]["preflight_contract"])
            (root / "snapshot" / "fixtures" / "order5.json").write_bytes(payload + b' ')
            with self.assertRaisesRegex(ValueError, "changed"):
                inverse.load_inputs(root / "snapshot")

    def test_invalid_pair_cannot_enter_a_measurement_manifest(self):
        self.assertTrue(hasattr(inverse, "validate_pair"), "fixture validation missing")
        with self.assertRaises(ValueError):
            inverse.validate_pair(dict(degree=5, canonical_coefficients_bits=[[0, 0]] * 6,
                                       conjugate_complement_bits=[[0, 0]]), 5)
        with self.assertRaises(ValueError):
            inverse.validate_pair(dict(degree=2000, canonical_coefficients_bits=[[0, 0]] * 2001,
                                       conjugate_complement_bits=[[0, 0]] * 2001), 2000)


if __name__ == "__main__":
    unittest.main()
