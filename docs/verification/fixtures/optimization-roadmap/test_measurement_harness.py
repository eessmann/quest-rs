"""Small, campaign-free checks for completion and summary accounting."""
import importlib.util
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest


HERE = Path(__file__).parent


def load(name):
    spec = importlib.util.spec_from_file_location(name, HERE / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class MeasurementHarnessTest(unittest.TestCase):
    def test_compiler_samples_require_every_corpus_stage_and_sample(self):
        runner = load("run_baseline")
        for fixture, count in [('baseline', 200), ('roadmap', 150), ('workers', 100)]:
            keys = sorted(runner.expected_keys(fixture))
            self.assertEqual(len(keys), count)
            rows = [{'case': case, 'stage': stage, 'sample': sample} for case, stage, sample in keys]
            runner.validate_sample_keys(rows, fixture)
            rows[0] = dict(rows[1])
            with self.assertRaisesRegex(ValueError, "sample keys"):
                runner.validate_sample_keys(rows, fixture)

    def test_failed_compiler_build_has_summary_without_samples(self):
        summarize = load("summarize")
        with TemporaryDirectory() as temporary:
            run = Path(temporary)
            (run / "completion.json").write_text(json.dumps({"status": "failed", "error": "build failed"}))
            self.assertEqual(summarize.compiler(run)["groups"], [])

    def test_failed_native_campaign_retains_rows_without_comparison(self):
        summarize = load("summarize")
        with TemporaryDirectory() as temporary:
            run = Path(temporary)
            (run / "completion.json").write_text(json.dumps({"status": "failed", "cases": [{"mode": "cpu", "kind": "sv", "nodes": 1, "status": "failed"}]}))
            records = [
                {"case": "qft6", "stage": f"{stage}/{component}", "sample": sample, "status": "complete", "elapsed_ns": 100, "detail": {"completion": "WorkLimit"}}
                for stage in ("unchanged", "beam")
                for component, count in (("search", 1), ("preparation", 5), ("warm_execution", 5))
                for sample in range(count)
            ]
            (run / "cpu-sv-1.jsonl").write_text("".join(json.dumps(row) + "\n" for row in records))
            stages = summarize.native(run)["configurations"][0]["stages"]
            beam = next(row for row in stages if row["stage"] == "beam")
            self.assertNotIn("execution_time_ratio", beam)
            self.assertNotIn("reuse_break_even_including_search", beam)
            self.assertEqual(len(beam["raw_rows"]), 11)

    def test_mpi_group_requires_unique_expected_ranks(self):
        runner = load("run_native_optimization")
        key = {"case": "qft6/rank0", "stage": "unchanged/search", "sample": 0, "status": "complete", "elapsed_ns": 1}
        with self.assertRaisesRegex(ValueError, "rank"):
            runner.max_rank_rows([key, dict(key)], 2)

    def test_complete_measurement_can_benchmark_admitted_work_limit(self):
        summarize = load("summarize")
        with TemporaryDirectory() as temporary:
            run = Path(temporary)
            (run / "completion.json").write_text(json.dumps({"status": "complete", "cases": [{"mode": "cpu", "kind": "sv", "nodes": 1, "status": "complete"}]}))
            records = [
                {"case": "qft6", "stage": f"{stage}/{component}", "sample": sample, "status": "complete",
                 "elapsed_ns": 100 if stage == "unchanged" else 50,
                 "detail": {"completion": "WorkLimit" if stage == "beam" else "Complete"}}
                for stage in ("unchanged", "beam")
                for component, count in (("search", 1), ("preparation", 5), ("warm_execution", 5))
                for sample in range(count)
            ]
            (run / "cpu-sv-1.jsonl").write_text("".join(json.dumps(row) + "\n" for row in records))
            stages = summarize.native(run)["configurations"][0]["stages"]
            beam = next(row for row in stages if row["stage"] == "beam")
            self.assertEqual(beam["measurement_status"], "complete")
            self.assertEqual(beam["optimizer_completion"], ["WorkLimit"])
            self.assertEqual(beam["execution_time_ratio"], 0.5)

    def test_missing_rank_sample_key_cannot_be_hidden_by_same_total_count(self):
        runner = load("run_native_optimization")
        expected = runner.expected_keys('mpi', 2)
        samples = [{"case": case, "stage": stage, "sample": sample}
                   for case, stage, sample in expected]
        samples[0] = dict(samples[1])
        with self.assertRaisesRegex(ValueError, "sample keys"):
            runner.validate_sample_keys(samples, 'mpi', 2)

    def test_break_even_rounds_exact_tenth_nanosecond_saving(self):
        summarize = load("summarize")
        with TemporaryDirectory() as temporary:
            run = Path(temporary)
            (run / "completion.json").write_text(json.dumps({"status": "complete", "cases": [{"mode": "cpu", "kind": "sv", "nodes": 1, "status": "complete"}]}))
            records = []
            for stage in ("unchanged", "beam"):
                for component, count in (("search", 1), ("preparation", 5), ("warm_execution", 5)):
                    elapsed = (20 if stage == "unchanged" else 19) if component == "warm_execution" else (1 if stage == "beam" and component == "search" else 0)
                    records.extend({"case": "qft6", "stage": f"{stage}/{component}", "sample": sample,
                                    "status": "complete", "elapsed_ns": elapsed} for sample in range(count))
            (run / "cpu-sv-1.jsonl").write_text("".join(json.dumps(row) + "\n" for row in records))
            beam = next(row for row in summarize.native(run)["configurations"][0]["stages"] if row["stage"] == "beam")
            self.assertEqual(beam["reuse_break_even_including_search"], 10)


if __name__ == "__main__":
    unittest.main()
