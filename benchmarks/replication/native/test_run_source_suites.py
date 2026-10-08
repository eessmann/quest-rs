import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest


CONTROLLER = Path(__file__).with_name("run_source_suites.py")


class NativeSourceRunnerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not CONTROLLER.is_file():
            raise AssertionError("native source suite runner has not been implemented")
        spec = importlib.util.spec_from_file_location("source_runner", CONTROLLER)
        cls.runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.runner)

    def rows(self):
        return [dict(id=name, benchmark_name=name, protocol="catch2_10")
                for name in ("first", "second", "third")]

    def report(self, ending=True):
        text = ('<Catch2TestRun><TestCase><BenchmarkResults name="first" samples="10" '
                'iterations="2"><mean value="125" lowerBound="120" upperBound="130"/>'
                '</BenchmarkResults>')
        if ending:
            text += ('<OverallResult success="true"/></TestCase>'
                     '<OverallResults successes="1" failures="0"/></Catch2TestRun>')
        return text

    def test_successful_process_does_not_invent_missing_loop_measurements(self):
        report = self.runner.parse_xml_report(self.report())
        results = self.runner.classify_group(self.rows(), report, "", 0, None)
        self.assertEqual([r["status"] for r in results],
                         ["measured_summary_only", "unattempted", "unattempted"])
        self.assertEqual(results[0]["measurement"]["mean_ns"], 125)
        self.assertNotIn("raw_samples", results[0])

    def test_timeout_retains_completed_and_started_observations_separately(self):
        report = self.runner.parse_xml_report(self.report(False) +
                    '<BenchmarkResults name="second" samples="10" iterations="1">')
        results = self.runner.classify_group(self.rows(), report, "", -15, "timeout")
        self.assertEqual([r["status"] for r in results],
                         ["observed_summary_unvalidated", "timeout", "unattempted"])

    def test_assertion_failure_invalidates_completed_measurements(self):
        report = self.runner.parse_xml_report(self.report().replace('failures="0"', 'failures="1"'))
        results = self.runner.classify_group(self.rows(), report, "", 1, None)
        self.assertEqual(results[0]["status"], "accuracy_failure")

    def test_duplicate_benchmark_names_are_rejected(self):
        duplicate = self.report().replace('<OverallResult ',
            '<BenchmarkResults name="first" samples="10" iterations="1">'
            '<mean value="1" lowerBound="1" upperBound="1"/></BenchmarkResults><OverallResult ')
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.runner.parse_xml_report(duplicate)

    def test_invalid_or_insufficient_sample_summary_is_not_a_measurement(self):
        for changed in (self.report().replace('samples="10"', 'samples="9"'),
                        self.report().replace('value="125"', 'value="nan"')):
            results = self.runner.classify_group(self.rows(), self.runner.parse_xml_report(changed), "", 0, None)
            self.assertEqual(results[0]["status"], "native_error")

    def test_coordinate_retains_only_three_actual_stdout_measurements(self):
        rows = [dict(id="coordinate", protocol="fixed_3", benchmark_name=None)]
        output = ('degree 8105 coordinate preparation samples (ns): 123, 234, 345\n'
                  'QSVT_BENCHMARK_STORAGE_V1 benchmark=degree8105 degree=8105 graph_capacity_bytes=9\n')
        result = self.runner.classify_group(rows, self.runner.parse_xml_report(self.report()), output, 0, None)[0]
        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["raw_times_ns"], [123, 234, 345])
        result = self.runner.classify_group(rows, self.runner.parse_xml_report(self.report()), output.replace('123, 234, 345', '123, 234'), 0, None)[0]
        self.assertNotEqual(result["status"], "ok")

    def test_coordinate_can_use_stdout_captured_by_the_xml_reporter(self):
        rows = [dict(id="coordinate", protocol="fixed_3", benchmark_name=None)]
        captured = ('degree 8105 coordinate preparation samples (ns): 123, 234, 345\n'
                    'QSVT_BENCHMARK_STORAGE_V1 benchmark=degree8105 degree=8105 graph_capacity_bytes=9\n')
        report = self.runner.parse_xml_report(self.report().replace('<OverallResult ', f'<StdOut>{captured}</StdOut><OverallResult '))
        result = self.runner.classify_group(rows, report, "console has no captured stdout", 0, None)[0]
        self.assertEqual(result["raw_times_ns"], [123, 234, 345])
        self.assertEqual(result["status"], "ok")

    def test_partial_report_names_phase_uncertainty(self):
        report = self.runner.parse_xml_report(self.report(False) + '<BenchmarkResults name="second" samples="10" iterations="1">')
        results = self.runner.classify_group(self.rows(), report, "", -15, "timeout")
        self.assertEqual(results[1]["phase"], "warmup_or_measurement_unknown")
        self.assertEqual(results[2]["phase"], "not_observed")

    def test_source_manifest_has_thirty_two_executable_rows_and_one_unsupported(self):
        groups, rows = self.runner.source_manifest()
        self.assertEqual(len(groups), 5)
        self.assertEqual(len(rows), 33)
        unsupported = [r for r in rows if r["group"] is None]
        self.assertEqual([r["case_id"] for r in unsupported], ["coordinate/degree8105/criterion_10"])
        self.assertEqual(sum(r["protocol"] == "catch2_10" for r in rows), 31)

    def test_identity_guard_rejects_changed_executable(self):
        with tempfile.TemporaryDirectory() as temporary:
            executable = Path(temporary) / "binary"
            executable.write_bytes(b"original")
            expected = {str(executable): self.runner.campaign.sha256(executable)}
            self.runner.verify_files(expected)
            executable.write_bytes(b"changed")
            with self.assertRaisesRegex(RuntimeError, "identity"):
                self.runner.verify_files(expected)

    def test_bounded_identity_probe_terminates_a_stalled_child(self):
        with self.assertRaisesRegex(RuntimeError, "failed"):
            self.runner.command_text([sys.executable, "-c", "import time; time.sleep(30)"], timeout=0.05)

    def test_lane_validation_rejects_changed_raw_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "group.xml").write_text(self.report())
            (root / "manifest.json").write_text(json.dumps(dict(rows=[dict(id="first", group="group")], groups=[dict(name="group")])))
            (root / "results.jsonl").write_text(json.dumps(dict(id="first", status="measured_summary_only", group="group")) + "\n")
            artifact = dict(path="group.xml", sha256=self.runner.campaign.sha256(root / "group.xml"))
            (root / "processes.json").write_text(json.dumps([dict(group="group", artifacts=[artifact])]))
            (root / "identity.json").write_text(json.dumps(dict(controller_snapshots={})))
            (root / "group.xml").write_text("changed")
            with self.assertRaisesRegex(ValueError, "artifact"):
                self.runner.validate_lane(root)

    def test_lane_validation_binds_rss_value_and_does_not_count_unsupported_as_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "group.xml").write_text(self.report())
            (root / "group.log").write_text("")
            (root / "group.rss").write_text("QUEST_BENCH_MAX_RSS_KIB=1234\n")
            rows = [dict(id="first", group="group", protocol="catch2_10", benchmark_name="first"),
                    dict(id="not-executable", group=None, protocol="criterion_10")]
            (root / "manifest.json").write_text(json.dumps(dict(rows=rows, groups=[dict(name="group")])))
            results = self.runner.classify_group(rows[:1], self.runner.parse_xml_report(self.report()), "", 0, None)
            results.append(dict(id="not-executable", status="unsupported"))
            (root / "results.jsonl").write_text("".join(json.dumps(row) + "\n" for row in results))
            artifacts = [dict(path=f"group.{suffix}", sha256=self.runner.campaign.sha256(root / f"group.{suffix}")) for suffix in ("xml", "log", "rss")]
            process = dict(group="group", artifacts=artifacts, returncode=0, stopped=None, peak_rss_kib=1234)
            (root / "processes.json").write_text(json.dumps([process]))
            (root / "identity.json").write_text(json.dumps(dict(controller_snapshots={})))
            summary = self.runner.validate_lane(root)
            self.assertFalse(summary["execution_coverage_complete"])
            self.assertFalse(summary["performance_coverage_complete"])
            process["peak_rss_kib"] = 9999
            (root / "processes.json").write_text(json.dumps([process]))
            with self.assertRaisesRegex(ValueError, "RSS"):
                self.runner.validate_lane(root)


if __name__ == "__main__":
    unittest.main()
