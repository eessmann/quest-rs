"""Receipt checks must reject incomplete/uncapped jobs and false node peaks."""
import importlib.util
import json
import tempfile
import pathlib
import subprocess
import sys
import unittest

SPEC = importlib.util.spec_from_file_location("capacity_run", pathlib.Path(__file__).with_name("run.py"))
RUN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUN)


class ReceiptTests(unittest.TestCase):
    def receipt(self, rank):
        return dict(rank=rank, ranks=2, dimension=64, repetitions=2,
                    process_address_space_cap_bytes=65536, rss_high_water_bytes=8192,
                    model_rank_budget_bytes=16384, model_node_budget_bytes=32768,
                    model_peak_bytes=16384, local_input_entries=64, global_input_entries=128,
                    address_space_high_water_bytes=32768, baseline_rss_bytes=2048,
                    baseline_address_space_bytes=8192,
                    model_peak_kind="conservative admitted stage envelope, not measured peak",
                    producer_managed_peak_upper_bound_bytes=256, native_live_reserved_bytes=8192,
                    loaded_retained_upper_bound_bytes=128, producer_global_edges=128, colors=2,
                    local_native_amplitudes=256, global_native_amplitudes=512,
                    norm=1.0, maximum_sample_error=1e-15,
                    stages={key: dict(seconds=.1, rss_endpoint_bytes=4096,
                        rss_high_water_bytes=8192, address_space_endpoint_bytes=16384,
                        address_space_high_water_bytes=32768) for key in RUN.STAGES},
                    execution_sent_bytes=32, execution_received_bytes=32,
                    producer_sent_payload_bytes=16, execution_coordination_calls=4,
                    execution_local_pair_candidates=256, execution_batches=8,
                    execution_roundtrip_seconds=[.03, .03],
                    persisted_recipe_single_replay_communication_upper_bound_bytes=0,
                    persistence_load_measured_communication="not instrumented; recipe replay bound is not a load measurement")

    def summarize(self, rows, node_budget=32768):
        return RUN.summarize(rows, 2, 64, 2, 65536, 16384, node_budget)

    def test_reports_sum_of_high_water_marks_as_bound(self):
        result = self.summarize([self.receipt(0), self.receipt(1)])
        self.assertEqual(result["sum_rank_rss_high_water_bound_bytes"], 16384)
        self.assertNotIn("node_peak_rss_bytes", result)
        self.assertEqual(result["execution_sent_bytes"], 64)

    def test_rejects_missing_duplicate_cap_and_semantic_failures(self):
        for field, value in (("rank", 0), ("process_address_space_cap_bytes", 8192),
                             ("norm", .9), ("norm", float("nan")),
                             ("address_space_high_water_bytes", 65537), ("maximum_sample_error", 1e-3),
                             ("model_peak_bytes", 16385), ("producer_managed_peak_upper_bound_bytes", 16385),
                             ("producer_global_edges", 127), ("local_input_entries", 128),
                             ("local_native_amplitudes", 512)):
            rows = [self.receipt(0), self.receipt(1)]
            rows[1][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.summarize(rows)
        with self.assertRaises(ValueError):
            self.summarize([self.receipt(0)])

    def test_enforces_node_placement_envelope_and_stage_completeness(self):
        with self.assertRaises(ValueError):
            self.summarize([self.receipt(0), self.receipt(1)], 32767)
        rows = [self.receipt(0), self.receipt(1)]
        del rows[1]["stages"]["load"]
        with self.assertRaises(ValueError):
            self.summarize(rows)

    def test_rejects_reproduced_incomplete_nonfinite_and_empty_receipts(self):
        mutations = {
            "missing stage memory": lambda r: r["stages"].update({k: {"seconds": .1} for k in RUN.STAGES}),
            "missing roundtrip timings": lambda r: r.pop("execution_roundtrip_seconds"),
            "nonfinite routing count": lambda r: r.update(execution_sent_bytes=float("nan")),
            "empty native layout": lambda r: r.update(local_native_amplitudes=0, global_native_amplitudes=0),
            "negative sample error": lambda r: r.update(maximum_sample_error=-1),
        }
        for name, mutate in mutations.items():
            rows = [self.receipt(0), self.receipt(1)]
            mutate(rows[0])
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.summarize(rows)

    def test_rejects_wrong_types_unknown_fields_and_invalid_counters(self):
        for field, value in (("rank", 1.0), ("ranks", True), ("execution_sent_bytes", -1),
                             ("execution_batches", True), ("rss_high_water_bytes", 8192.0),
                             ("colors", 4), ("global_input_entries", 0),
                             ("execution_roundtrip_seconds", [.03]),
                             ("execution_roundtrip_seconds", [.03, float("inf")]),
                             ("execution_roundtrip_seconds", [.03, -.03]),
                             ("execution_roundtrip_seconds", [.1, .1]),
                             ("execution_received_bytes", 31), ("extra_field", "unchecked")):
            rows = [self.receipt(0), self.receipt(1)]
            rows[1][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                self.summarize(rows)
        for malformed in (None, {}, [None, {}], "receipts"):
            with self.subTest(rows=malformed), self.assertRaises(ValueError):
                self.summarize(malformed)
        rows = [self.receipt(0), self.receipt(1)]
        rows[0]["stages"]["load"]["rss_endpoint_bytes"] = float("inf")
        with self.assertRaises(ValueError):
            self.summarize(rows)


class JsonAndCompletionTests(unittest.TestCase):
    def test_rejects_nonstandard_duplicate_and_oversized_json(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "rank.json"
            for content in ('{"norm": NaN}', '{"rank": 0, "rank": 1}',
                            '{"stages":{"load": {"seconds": Infinity}}}', '{bad',
                            ' ' * (RUN.MAX_RECEIPT_BYTES + 1)):
                path.write_text(content)
                with self.subTest(content=content[:40]), self.assertRaises(ValueError):
                    RUN.read_receipt(path)

    def test_malformed_rank_receipt_retains_incomplete_completion(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            launcher = root / "fake-mpiexec"
            launcher.write_text("#!/usr/bin/env python3\n"
                "import pathlib,sys\n"
                "folder=pathlib.Path(next(s for s in sys.argv if 'case-0-n64-p2' in s))\n"
                "for rank in range(2): (folder/f'rank-{rank}.json').write_text('{}')\n")
            launcher.chmod(0o755)
            output = root / "receipts"
            result = subprocess.run([sys.executable, str(pathlib.Path(__file__).with_name("run.py")),
                "--executable", sys.executable, "--mpiexec", str(launcher),
                "--output", str(output), "--cases", "64:2"], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            completion = json.loads((output / "completion.json").read_text())
            self.assertFalse(completion["complete"])
            self.assertEqual(completion["cases"], [])
            self.assertIn("schema", completion["failure"])
            self.assertTrue((output / "case-0-n64-p2" / "rank-0.json").exists())


class LauncherTests(unittest.TestCase):
    def test_wrapper_applies_real_equal_hard_soft_limits_before_exec(self):
        cap = 128 * 1024**2
        result = RUN.run_job([sys.executable, str(pathlib.Path(__file__).with_name("run.py")),
                              "--rank-wrapper", str(cap), sys.executable, "-c",
                              "import resource;print(*resource.getrlimit(resource.RLIMIT_AS))"], 5)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout.strip(), f"{cap} {cap}")

    def test_timeout_preserves_partial_output_and_stops_process_group(self):
        with self.assertRaises(subprocess.TimeoutExpired) as caught:
            RUN.run_job([sys.executable, "-c", "import time;print('started',flush=True);time.sleep(30)"], .1)
        self.assertIn("started", caught.exception.stdout)


if __name__ == "__main__":
    unittest.main()
