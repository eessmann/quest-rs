"""Contract tests for reproducible benchmark accounting."""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("campaign", Path(__file__).with_name("campaign.py"))
campaign = importlib.util.module_from_spec(SPEC) if SPEC else None
if SPEC and SPEC.loader and SPEC.origin and Path(SPEC.origin).exists():
    SPEC.loader.exec_module(campaign)


class CampaignContracts(unittest.TestCase):
    def test_inventory_keeps_all_source_workloads(self):
        self.assertTrue(hasattr(campaign, "source_inventory"), "source inventory is missing")
        rows = campaign.source_inventory([])
        counts = {suite: sum(r["suite"] == suite for r in rows) for suite in {r["suite"] for r in rows}}
        self.assertEqual(counts, {"named_inverse": 17, "named_solver": 9, "root_isolation": 5,
                                  "roots_of_unity": 2, "circuit": 20, "unitary": 4, "coordinate": 2})
        self.assertEqual(len({r["case_id"] for r in rows}), len(rows))

    def test_missing_terminal_result_never_creates_completion(self):
        self.assertTrue(hasattr(campaign, "publish_completion"), "completion gate is missing")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            with self.assertRaises(ValueError):
                campaign.publish_completion(path, [{"id": "one"}, {"id": "two"}], [{"id": "one", "status": "ok"}])
            self.assertFalse((path / "completion.json").exists())

    def test_duplicate_or_nonterminal_results_are_rejected(self):
        self.assertTrue(hasattr(campaign, "publish_completion"), "completion gate is missing")
        with tempfile.TemporaryDirectory() as directory:
            for results in ([{"id": "one", "status": "running"}],
                            [{"id": "one", "status": "ok"}] * 2):
                with self.assertRaises(ValueError):
                    campaign.publish_completion(Path(directory), [{"id": "one"}], results)

    def test_process_success_without_preflight_is_not_a_measurement(self):
        self.assertTrue(hasattr(campaign, "classify_run"), "run validation is missing")
        self.assertEqual(campaign.classify_run(0, "", "complete"), "native_error")
        self.assertEqual(campaign.classify_run(0, "QUEST_BENCH_PREFLIGHT requested=1e-12 achieved=0\n", "complete"), "ok")
        self.assertEqual(campaign.classify_run(1, "accuracy_failure: observed", "preflight"), "accuracy_failure")
        self.assertEqual(campaign.classify_run(1, "Error: Contractivity { upper: 1.12 }", "preflight"), "accuracy_failure")
        self.assertEqual(campaign.classify_run(1, "QSP resource limit: completion grid", "preparation"), "resource_limit")
        self.assertEqual(campaign.classify_run(1, 'Error: Numerics(Budget { resource: "bytes" })', "preparation"), "resource_limit")

    def test_success_requires_raw_samples_that_still_match_their_hashes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            with self.assertRaises(ValueError):
                campaign.publish_completion(path, [{"id": "one"}], [{"id": "one", "status": "ok"}])
            sample = path / "samples.json"
            sample.write_text('{"times": [1.0], "iters": [1.0]}')
            result = {"id": "one", "status": "ok", "raw_samples": [{"path": "samples.json", "sha256": "0" * 64}]}
            with self.assertRaises(ValueError):
                campaign.publish_completion(path, [{"id": "one"}], [result])
            self.assertFalse((path / "completion.json").exists())

    def test_fixture_transport_preserves_binary64_bits_including_negative_zero(self):
        import struct
        values = [-0.0, 5e-324, 1.0000000000000002]
        case = dict(case_id="bits", degree=2, source_basis="Monomial", source_parameters=[],
                    source_minimum_order=0, canonical_minimum_order=0,
                    source_coefficients_real=values, source_coefficients_imag=[0.0] * 3,
                    canonical_coefficients_real=values, canonical_coefficients_imag=[0.0] * 3)
        fixture = campaign.coefficient_payload(case)
        self.assertIn("canonical_coefficients_bits", fixture)
        restored = [struct.unpack("<d", struct.pack("<Q", pair[0]))[0] for pair in fixture["canonical_coefficients_bits"]]
        self.assertEqual(b"".join(struct.pack("<d", x) for x in values), b"".join(struct.pack("<d", x) for x in restored))

    def test_source_mutation_retains_failure_and_blocks_publication(self):
        self.assertTrue(hasattr(campaign, "verify_source_identity"), "locked source identity verification is missing")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text("original")
            expected = campaign.source_identity(root)
            (root / "Cargo.toml").write_text("changed")
            with self.assertRaises(RuntimeError):
                campaign.verify_source_identity(root, expected, root, "case")
            self.assertTrue((root / "infrastructure-failure.json").exists())
            self.assertFalse((root / "completion.json").exists())

    def test_interruption_terminates_the_owned_child_before_returning(self):
        self.assertTrue(hasattr(campaign, "wait_bounded"), "interrupt-safe process waiter is missing")
        import os
        import signal
        import subprocess
        import sys
        import threading
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"], start_new_session=True)
        timer = threading.Timer(0.1, lambda: os.kill(os.getpid(), signal.SIGINT))
        timer.start()
        try:
            code, outcome = campaign.wait_bounded(child, 10)
            self.assertEqual(outcome, "interrupted")
            self.assertIsNotNone(child.poll())
            self.assertLess(code, 0)
        finally:
            timer.cancel()
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()

    def test_external_rss_measurement_retains_missing_as_null(self):
        self.assertTrue(hasattr(campaign, "time_command"), "external RSS wrapper is missing")
        import subprocess
        import sys
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rss.txt"
            self.assertIsNone(campaign.read_peak_rss(path))
            subprocess.run(campaign.time_command([sys.executable, "-c", "pass"], path), check=True)
            self.assertGreater(campaign.read_peak_rss(path), 0)

    def test_completion_rejects_changed_memory_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rss = root / "memory.rss"
            rss.write_text("QUEST_BENCH_MAX_RSS_KIB=1024\n")
            result = {"id": "one", "status": "accuracy_failure", "peak_rss_kib": 1024,
                      "peak_rss_log": rss.name, "peak_rss_log_sha256": campaign.sha256(rss)}
            campaign.completion_summary(root, [{"id": "one"}], [result])
            rss.write_text("QUEST_BENCH_MAX_RSS_KIB=2048\n")
            with self.assertRaises(ValueError):
                campaign.completion_summary(root, [{"id": "one"}], [result])

    def test_source_deadlines_are_preserved(self):
        self.assertTrue(hasattr(campaign, "softwarex_timeout"), "source deadline is missing")
        self.assertEqual(campaign.softwarex_timeout(200, "solver"), 92)
        self.assertEqual(campaign.softwarex_timeout(200, "kernel"), 122)
        self.assertEqual(campaign.softwarex_timeout(1_000_000, "kernel"), 12620)

    def test_failure_counts_do_not_claim_performance_success(self):
        self.assertTrue(hasattr(campaign, "publish_completion"), "completion gate is missing")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            campaign.publish_completion(path, [{"id": "one"}], [{"id": "one", "status": "accuracy_failure"}])
            data = json.loads((path / "completion.json").read_text())
            self.assertTrue(data["accounting_complete"])
            self.assertFalse(data["performance_coverage_complete"])


if __name__ == "__main__":
    unittest.main()
