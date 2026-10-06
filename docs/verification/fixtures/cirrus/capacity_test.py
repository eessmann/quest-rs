"""Reject false capacity closure from host counts, summed peaks, or derived bytes."""
import importlib.util
import pathlib
import os
import signal
import subprocess
import json
import time
import sys
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("cirrus_capacity", pathlib.Path(__file__).with_name("capacity.py"))
RUN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUN)


class CapacityTests(unittest.TestCase):
    def test_slurm_rank_entry_rejects_custom_memory_limits_before_execution(self):
        with mock.patch.dict(os.environ, {"SLURM_JOB_ID": "123"}, clear=True), \
             mock.patch.object(sys, "argv", ["capacity.py", "--rank-wrapper", "2147483648", "/bin/true", "2", "8388608"]), \
             mock.patch.object(RUN.resource, "setrlimit") as limits, \
             mock.patch.object(RUN.os, "execv") as execute:
            with self.assertRaisesRegex(SystemExit, "documented Cirrus workflow"):
                RUN.main()
            limits.assert_not_called()
            execute.assert_not_called()

    def test_srun_coordinator_rejects_custom_memory_campaign_before_output(self):
        with tempfile.TemporaryDirectory() as root:
            output = pathlib.Path(root) / "not-created"
            with mock.patch.dict(os.environ, {}, clear=True), \
                 mock.patch.object(sys, "argv", ["capacity.py", "--executable", "/bin/true", "--output", str(output), "--launcher", "/usr/bin/srun"]), \
                 mock.patch.object(RUN, "run_job") as execute:
                with self.assertRaisesRegex(SystemExit, "documented Cirrus workflow"):
                    RUN.main()
                execute.assert_not_called()
                self.assertFalse(output.exists())

    def rows(self):
        return [dict(rank=r, process_address_space_cap_bytes=1024,
                     node=dict(leader_rank=(r // 2) * 2, local_rank=r % 2,
                               local_size=2, processor_name=f"node-{r // 2}"),
                     node_memory_sampling=(dict(samples=5, maximum_sum_rss_bytes=1500,
                         maximum_sum_address_space_bytes=1900, max_sample_span_seconds=.001,
                         interval_milliseconds=20, pids=2) if r % 2 == 0 else None))
                for r in range(4)]

    def test_rank_process_envelope_cannot_close_unverified_whole_node_capacity(self):
        result = RUN.capacity_evidence(self.rows(), 2, 2, 2049)
        self.assertFalse(result["capacity_closed"])
        self.assertTrue(result["original_input_exceeds_rank_process_envelope"])
        self.assertFalse(result["whole_node_enforcement_verified"])
        self.assertIsNone(result["nodes"][0]["whole_node_enforced_memory_cap_bytes"])
        self.assertIn("coordinator", result["rank_process_envelope_scope"])
        self.assertEqual(result["nodes"][0]["enforced_rank_process_address_space_cap_bytes"], 2048)
        self.assertEqual(result["nodes"][0]["sampled_maximum_sum_rank_rss_bytes"], 1500)
        self.assertNotIn("node_peak_rss_bytes", result["nodes"][0])

    def test_threshold_equality_is_open(self):
        result = RUN.capacity_evidence(self.rows(), 2, 2, 2048)
        self.assertFalse(result["capacity_closed"])
        self.assertFalse(result["original_input_exceeds_rank_process_envelope"])

    def test_rank_process_threshold_must_exceed_every_node_rank_sum(self):
        rows = self.rows()
        rows[2]["process_address_space_cap_bytes"] = 4096
        result = RUN.capacity_evidence(rows, 2, 2, 4097)
        self.assertFalse(result["original_input_exceeds_rank_process_envelope"])
        self.assertFalse(result["capacity_closed"])

    def test_unverified_node_enforcement_claim_is_not_trusted(self):
        rows = self.rows()
        for row in rows:
            row["whole_node_enforcement_verified"] = True
            row["whole_node_enforced_memory_cap_bytes"] = 2048
        result = RUN.capacity_evidence(rows, 2, 2, 4096)
        self.assertFalse(result["whole_node_enforcement_verified"])
        self.assertFalse(result["capacity_closed"])

    def test_rejects_duplicate_hosts_or_incomplete_shared_memory_groups(self):
        for mutate in (lambda rows: rows[2]["node"].update(processor_name="node-0"),
                       lambda rows: rows[1]["node"].update(local_rank=0),
                       lambda rows: rows[2]["node"].update(leader_rank=0),
                       lambda rows: rows[2]["node_memory_sampling"].update(maximum_sum_rss_bytes=2049),
                       lambda rows: rows[2].update(node_memory_sampling=None)):
            rows = self.rows()
            mutate(rows)
            with self.assertRaises(ValueError):
                RUN.capacity_evidence(rows, 2, 2, 4096)

    def test_slurm_launch_preserves_multihost_placement(self):
        command = RUN.launch_command("srun", "/bin/example", "/tmp/output", 8192, 2,
                                     [2048, 1024, 4096], 8, 1)
        self.assertIn("--nodes=8", command)
        self.assertIn("--ntasks=8", command)
        self.assertIn("--ntasks-per-node=1", command)
        self.assertIn("--exclusive", command)
        self.assertIn("--hint=nomultithread", command)
        self.assertIn("--distribution=block:block", command)
        self.assertNotIn("localhost", command)
        self.assertFalse(any(word.startswith("--mem") for word in command))

    def test_slurm_rejects_multiple_mpi_ranks_per_node(self):
        with self.assertRaises(ValueError):
            RUN.launch_command("srun", "/bin/example", "/tmp/output", 8192, 2,
                               [2048, 1024, 4096], 8, 4)

    def test_rank_wrapper_applies_the_requested_openmp_environment(self):
        script = "import os,json; print(json.dumps({k:os.environ.get(k) for k in ('OMP_NUM_THREADS','OMP_PLACES','OMP_PROC_BIND','OMP_DYNAMIC','OMP_STACKSIZE','SRUN_CPUS_PER_TASK')}))"
        environment = dict(os.environ, SLURM_CPUS_PER_TASK="2")
        environment.pop("SLURM_PROCID", None)
        result = subprocess.run([sys.executable, str(pathlib.Path(RUN.__file__)), "--rank-wrapper",
            "2147483648", sys.executable, "-c", script, "2", "8388608"],
            env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
        self.assertEqual(json.loads(result.stdout), dict(OMP_NUM_THREADS="2", OMP_PLACES="cores",
            OMP_PROC_BIND="close", OMP_DYNAMIC="FALSE", OMP_STACKSIZE="8388608B", SRUN_CPUS_PER_TASK="2"))

    def test_slurm_thread_allocation_is_explicit(self):
        command = RUN.launch_command("srun", "/bin/example", "/tmp/output", 8192, 2,
                                     [2048, 1024, 1024], 8, 1, threads=288)
        self.assertIn("--cpus-per-task=288", command)
        self.assertEqual(command[-4:], ["8", "1", "288", "8388608"])

    def test_actual_native_threading_and_stack_budget_are_required(self):
        row = dict(baseline_address_space_bytes=9, threading=dict(requested_threads=2,
            omp_stack_bytes_per_worker=8, omp_stack_allowance_bytes=8,
            native_environment_multithreaded=True, native_register_multithreaded=True,
            baseline_process_threads=2, prepared_process_threads=3, final_process_threads=2,
            omp_num_threads="2", omp_places="cores", omp_proc_bind="close", omp_dynamic="FALSE",
            omp_stacksize="8B", native_openmp_team_size=None, scope=RUN.THREADING_SCOPE))
        RUN.validate_threading(row, 2, 8, [28, 10, 10])
        with self.assertRaises(ValueError):
            RUN.validate_threading(row, 2, 8, [26, 10, 10])
        row["threading"]["native_register_multithreaded"] = False
        with self.assertRaises(ValueError):
            RUN.validate_threading(row, 2, 8, [28, 10, 10])

    def test_bounded_launcher_capture_stops_excessive_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = pathlib.Path(temporary) / "launcher.log"
            with self.assertRaisesRegex(RuntimeError, "bounded log"):
                RUN.run_job([sys.executable, "-c", "import os; os.write(1, b'x' * (2 * 1024 * 1024))"], 10, log)
            self.assertEqual(log.stat().st_size, 1048576)

    def test_launcher_timeout_preserves_bounded_diagnostics(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = pathlib.Path(temporary) / "launcher.log"
            with self.assertRaisesRegex(RuntimeError, "timed out"):
                RUN.run_job([sys.executable, "-c", "import time; print('started', flush=True); time.sleep(5)"], .1, log)
            self.assertEqual(log.read_text(), "started\n")

    def test_timeout_kills_children_after_launcher_has_already_exited(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = pathlib.Path(temporary) / "launcher.log"
            script = "import subprocess,sys; p=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); print(p.pid,flush=True)"
            with self.assertRaisesRegex(RuntimeError, "timed out"):
                RUN.run_job([sys.executable, "-c", script], .2, log)
            pid = int(log.read_text().strip())
            def alive():
                try:
                    return pathlib.Path(f"/proc/{pid}/stat").read_text().split()[2] != "Z"
                except FileNotFoundError:
                    return False
            try:
                for _ in range(10):
                    if not alive():
                        break
                    time.sleep(.02)
                self.assertFalse(alive(), "orphaned MPI child survived launcher-group timeout")
            finally:
                if alive():
                    os.kill(pid, signal.SIGKILL)


if __name__ == "__main__":
    unittest.main()
