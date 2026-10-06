"""Exercise bounded local/remote cleanup without invoking a real scheduler."""
import importlib.util
import json
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "capacity_supervision", pathlib.Path(__file__).with_name("capacity_supervision.py"))
RUN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUN)


class SupervisionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        self.log = self.root / "launcher.log"
        self.cancelled = self.root / "cancelled"
        self.queue = self.root / "queue"
        self.queue.write_text("731.9\n731.10\n")
        environment = {key: value for key, value in os.environ.items()
                       if not key.startswith("SLURM_") and key not in
                       ("QUEST_MPI_SUPERVISED_CHILD", "PMI_RANK", "PMIX_RANK",
                        "OMPI_COMM_WORLD_RANK", "MV2_COMM_WORLD_RANK")}
        environment.update({
            "PATH": str(self.root) + os.pathsep + os.environ.get("PATH", ""),
            "SLURM_JOB_ID": "731", "CANCELLED": str(self.cancelled),
            "QUEUE": str(self.queue), "REMOTE_PID": str(self.root / "remote-pid"),
        })
        self.patch = mock.patch.dict(os.environ, environment, clear=True)
        self.patch.start()
        self.addCleanup(self.patch.stop)
        self.script("scancel", "import os,sys,pathlib\n"
                    "pathlib.Path(os.environ['CANCELLED']).write_text('\\n'.join(sys.argv[1:]))\n"
                    "pathlib.Path(os.environ['QUEUE']).write_text('731.10\\n')\n")
        self.script("squeue", "import os,pathlib\nprint(pathlib.Path(os.environ['QUEUE']).read_text(),end='')\n")

    def script(self, name, body):
        path = self.root / name
        path.write_text(f"#!{sys.executable}\n{body}")
        path.chmod(0o700)
        return str(path)

    def slurm(self, prefix="", suffix="time.sleep(10)", job="731", step="9"):
        return self.script("srun", "import os,time\n" + prefix +
            f"print('0: QUEST_CAPACITY_STEP_' + os.environ['QUEST_CAPACITY_STEP_TOKEN'] + '={job}.{step}', flush=True)\n" + suffix + "\n")

    def receipt(self):
        return json.loads(pathlib.Path(str(self.log) + ".supervision.json").read_text())

    def test_batch_coordinator_admission_through_reexecuted_environment(self):
        cases = [
            ({}, True),
            ({"SLURM_JOB_ID": "731"}, True),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0"}, True),
            ({"SLURM_JOB_ID": "731", "SLURM_JOBID": "731", "SLURM_PROCID": "0", "SLURM_LOCALID": "0", "SLURM_NODEID": "0"}, True),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0", "SLURM_STEP_ID": "batch"}, True),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0", "SLURM_STEPID": "batch"}, True),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0", "SLURM_STEP_ID": "batch", "SLURM_STEPID": "batch"}, True),
            ({"SLURM_PROCID": "0"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "1"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "00"}, False),
            ({"SLURM_JOB_ID": "invalid", "SLURM_PROCID": "0"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_JOBID": "732", "SLURM_PROCID": "0"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_JOBID": "invalid", "SLURM_PROCID": "0"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0", "SLURM_STEP_ID": "0"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0", "SLURM_STEPID": "0"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0", "SLURM_STEP_ID": "batch", "SLURM_STEPID": "0"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0", "SLURM_STEP_ID": ""}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0", "SLURM_STEPID": "extern"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_STEP_ID": "0"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_STEP_ID": "batch"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_LOCALID": "0"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0", "SLURM_LOCALID": "1"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0", "SLURM_NODEID": "1"}, False),
            ({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0", "SLURM_LOCALID": "invalid"}, False),
        ]
        cases += [({"SLURM_JOB_ID": "731", "SLURM_PROCID": "0", marker: "0"}, False)
                  for marker in ("PMI_RANK", "PMIX_RANK", "OMPI_COMM_WORLD_RANK",
                                 "MV2_COMM_WORLD_RANK", "QUEST_MPI_SUPERVISED_CHILD")]
        probe = ("import importlib.util,sys; "
                 "spec=importlib.util.spec_from_file_location('supervisor',sys.argv[1]); "
                 "module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module); "
                 "module.run_job(['/bin/echo','coordinator-ok'],1,sys.argv[2])")
        for environment, accepted in cases:
            with self.subTest(environment=environment):
                self.log.unlink(missing_ok=True)
                result = subprocess.run([sys.executable, "-c", probe, RUN.__file__, str(self.log)],
                                        env=environment, capture_output=True, timeout=5)
                self.assertEqual(result.returncode == 0, accepted, result.stderr.decode())
                if accepted:
                    self.assertEqual(self.log.read_text(), "coordinator-ok\n")
                else:
                    self.assertFalse(self.log.exists())

    def test_timeout_cancels_only_recorded_step_and_confirms_disappearance(self):
        with self.assertRaisesRegex(RuntimeError, "timed out"):
            RUN.run_job([self.slurm()], .15, self.log)
        self.assertEqual(self.cancelled.read_text(), "--signal=KILL\n731.9")
        receipt = self.receipt()
        self.assertEqual(receipt["owned_step"], "731.9")
        self.assertTrue(receipt["cleanup"]["step_disappeared"])
        self.assertEqual(self.queue.read_text(), "731.10\n")

    def test_unknown_or_wrong_job_identity_never_cancels_allocation(self):
        for body in ("import time; time.sleep(10)",
                     "import os,time; print('QUEST_CAPACITY_STEP_' + os.environ['QUEST_CAPACITY_STEP_TOKEN'] + '=732.9',flush=True); time.sleep(10)"):
            with self.subTest(body=body), self.assertRaisesRegex(RuntimeError, "cleanup unverified"):
                RUN.run_job([self.script("srun", body)], .1, self.log)
            self.assertFalse(self.cancelled.exists())
            self.assertFalse(self.receipt()["cleanup"]["step_disappeared"])

    def test_identity_is_parsed_beyond_the_bounded_log(self):
        prefix = "os.write(1, b'x' * (RUN_LIMIT := 1048577) + b'\\n')\n"
        with self.assertRaisesRegex(RuntimeError, "bounded log"):
            RUN.run_job([self.slurm(prefix=prefix, suffix="pass")], 3, self.log)
        self.assertEqual(self.log.stat().st_size, 1048576)
        self.assertEqual(self.receipt()["owned_step"], "731.9")
        self.assertTrue(self.receipt()["cleanup"]["step_disappeared"])

    def test_cancellation_and_confirmation_are_deadline_bounded(self):
        self.script("scancel", "import time; time.sleep(20)")
        self.script("squeue", "import time; time.sleep(20)")
        start = time.monotonic()
        with mock.patch.object(RUN, "CLEANUP_SECONDS", .35), mock.patch.object(RUN, "CONTROL_SECONDS", .1):
            with self.assertRaisesRegex(RuntimeError, "cleanup unverified"):
                RUN.run_job([self.slurm()], .1, self.log)
        self.assertLess(time.monotonic() - start, 2)
        self.assertFalse(self.receipt()["cleanup"]["step_disappeared"])

    def test_failed_squeue_cannot_be_mistaken_for_absent_step(self):
        self.script("squeue", "import sys; sys.exit(1)")
        with mock.patch.object(RUN, "CLEANUP_SECONDS", .25):
            with self.assertRaisesRegex(RuntimeError, "cleanup unverified"):
                RUN.run_job([self.slurm()], .1, self.log)
        self.assertFalse(self.receipt()["cleanup"]["step_disappeared"])

    def test_success_keeps_arguments_and_generates_a_fresh_nonce(self):
        child = self.slurm(suffix="import sys; print(repr(sys.argv[1:]))")
        RUN.run_job([child, "a b", "$(unexpanded)"], 2, self.log)
        first = self.log.read_text().splitlines()[0]
        self.assertIn("['a b', '$(unexpanded)']", self.log.read_text())
        RUN.run_job([child], 2, self.log)
        self.assertNotEqual(first, self.log.read_text().splitlines()[0])
        self.assertIsNone(self.receipt()["cleanup"])
        self.assertFalse(self.cancelled.exists())

    def test_local_timeout_kills_descendants_after_parent_exits(self):
        marker = self.root / "survived"
        child = "import time,pathlib; time.sleep(.5); pathlib.Path(" + repr(str(marker)) + ").touch()"
        command = "import subprocess,sys; subprocess.Popen([sys.executable,'-c'," + repr(child) + "])"
        with self.assertRaisesRegex(RuntimeError, "timed out"):
            RUN.run_job([sys.executable, "-c", command], .1, self.log)
        time.sleep(.55)
        self.assertFalse(marker.exists())
        self.assertFalse(self.cancelled.exists())

    def test_remote_detached_process_is_terminated_by_owned_step_cleanup(self):
        prefix = "import subprocess,pathlib\np=subprocess.Popen([os.sys.executable,'-c','import time; time.sleep(20)'],start_new_session=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)\npathlib.Path(os.environ['REMOTE_PID']).write_text(str(p.pid))\n"
        self.script("scancel", "import os,signal,sys,pathlib\n"
            "pathlib.Path(os.environ['CANCELLED']).write_text('\\n'.join(sys.argv[1:]))\n"
            "os.killpg(int(pathlib.Path(os.environ['REMOTE_PID']).read_text()), signal.SIGKILL)\n"
            "pathlib.Path(os.environ['QUEUE']).write_text('731.10\\n')\n")
        try:
            with self.assertRaisesRegex(RuntimeError, "timed out"):
                RUN.run_job([self.slurm(prefix=prefix)], .15, self.log)
            pid = int((self.root / "remote-pid").read_text())
            status = pathlib.Path(f"/proc/{pid}/stat")
            for _ in range(20):
                if not status.exists() or status.read_text().split()[2] == "Z":
                    break
                time.sleep(.01)
            self.assertTrue(not status.exists() or status.read_text().split()[2] == "Z")
            self.assertTrue(self.receipt()["cleanup"]["step_disappeared"])
        finally:
            if (self.root / "remote-pid").exists():
                try:
                    os.killpg(int((self.root / "remote-pid").read_text()), signal.SIGKILL)
                except ProcessLookupError:
                    pass


if __name__ == "__main__":
    unittest.main()
