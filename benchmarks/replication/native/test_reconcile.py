import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

PATH = Path(__file__).with_name("reconcile.py")
SPEC = importlib.util.spec_from_file_location("native_reconcile", PATH)
reconcile = importlib.util.module_from_spec(SPEC)
if PATH.exists():
    SPEC.loader.exec_module(reconcile)


class ReconcileContracts(unittest.TestCase):
    def test_group_assertion_failure_is_not_five_proven_accuracy_failures(self):
        self.assertTrue(hasattr(reconcile, "diagnose"), "group gate interpretation missing")
        row = dict(status="accuracy_failure", measurement={"mean_ns": 1}, source_assertion_gate_passed=False)
        self.assertEqual(reconcile.diagnose(row)["category"], "group_assertion_invalidated_summary")
        self.assertEqual(row["status"], "accuracy_failure")

    def test_expected_id_cannot_be_routed_to_a_cheaper_benchmark(self):
        from run_named import named_manifest
        from run_source_suites import source_manifest
        from campaign import source_inventory
        cases = json.loads(PATH.parent.parent.joinpath("softwarex-case-inventory.json").read_text())
        named_groups, named_rows = named_manifest()
        source_groups, source_rows = source_manifest()
        for row in named_rows:
            if row["case_id"] == "named_inverse/order1000000":
                row.update(group="inverse_quick", benchmark_name="Order: 5")
        legacy = [dict(row, id="quest_qsvt/" + row["case_id"]) for row in source_inventory(cases) if row["suite"] == "softwarex"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lanes = []
            for role, groups, rows in (("named", named_groups, named_rows), ("source", source_groups, source_rows), ("softwarex", [], legacy)):
                lane = root / role
                lane.mkdir()
                (lane / "manifest.json").write_text(json.dumps(dict(groups=groups, rows=rows)))
                lanes.append(lane)
            try:
                with self.assertRaisesRegex(ValueError, "contract|routing"):
                    reconcile.combine(*lanes, root / "combined")
            except FileNotFoundError:
                self.fail("changed routing was accepted and artifact loading began")
            self.assertFalse((root / "combined" / "completion.json").exists())

    def test_same_cardinalities_cannot_replace_expected_workloads(self):
        self.assertTrue(hasattr(reconcile, "combine"), "native reconciliation missing")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lanes = []
            for role, count in (("named", 26), ("source", 33), ("softwarex", 124)):
                lane = root / role
                lane.mkdir()
                (lane / "manifest.json").write_text(json.dumps(dict(rows=[dict(id=f"wrong/{i}", case_id=f"wrong/{i}") for i in range(count)])))
                lanes.append(lane)
            with self.assertRaisesRegex(ValueError, "identit"):
                reconcile.combine(*lanes, root / "combined")
            self.assertFalse((root / "combined" / "completion.json").exists())

    def test_unattempted_is_accounted_but_never_execution_coverage(self):
        self.assertTrue(hasattr(reconcile, "summarize"), "native summary missing")
        result = reconcile.summarize([dict(status="measured_summary_only"), dict(status="unattempted"), dict(status="unsupported")])
        self.assertTrue(result["accounting_complete"])
        self.assertFalse(result["execution_coverage_complete"])
        self.assertFalse(result["raw_performance_coverage_complete"])
        self.assertEqual(result["observed_summary_rows"], 1)
        self.assertEqual(result["unattempted_rows"], 1)

    def test_unsupported_rows_never_claim_full_execution_coverage(self):
        result = reconcile.summarize([dict(status="measured_summary_only"), dict(status="unsupported")])
        self.assertFalse(result["execution_coverage_complete"])
        self.assertFalse(result["raw_performance_coverage_complete"])


if __name__ == "__main__":
    unittest.main()
