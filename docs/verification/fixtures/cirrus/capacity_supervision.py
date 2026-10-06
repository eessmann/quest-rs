"""Bounded MPI launch supervision with the Rust MpiTest ownership semantics.

The rank-zero wrapper prints QUEST_CAPACITY_STEP_<nonce>=<job>.<step> before
native entry. Only that observed step may be cancelled. An unknown step never
falls back to cancelling its allocation. Cleanup adds at most CLEANUP_SECONDS,
plus bounded local process reaping, to the requested execution deadline.
"""
import json
import math
import os
import pathlib
import re
import secrets
import selectors
import signal
import subprocess
import time

MAX_LOG = 1024 * 1024
CLEANUP_SECONDS = 6.0
CONTROL_SECONDS = 2.0


class _StepIdentity:
    def __init__(self, token, job):
        self.prefix = f"QUEST_CAPACITY_STEP_{token}={job}.".encode()
        self.job = job
        self.pending = bytearray()
        self.oversized = False
        self.step = None

    def feed(self, chunk):
        for byte in chunk:
            if byte == 10:
                if not self.oversized and self.prefix in self.pending:
                    suffix = bytes(self.pending).split(self.prefix, 1)[1]
                    if suffix and all(48 <= value <= 57 for value in suffix):
                        candidate = self.job + "." + suffix.decode("ascii")
                        if self.step is None:
                            self.step = candidate
                        elif self.step != candidate:
                            raise RuntimeError("conflicting owned Slurm step identities")
                self.pending.clear()
                self.oversized = False
            elif len(self.pending) < 4096:
                self.pending.append(byte)
            else:
                self.oversized = True


class _Capture:
    def __init__(self, limit, sink=None, identity=None):
        self.limit = limit
        self.sink = sink
        self.identity = identity
        self.data = bytearray()
        self.truncated = False

    def feed(self, chunk):
        if self.identity is not None:
            self.identity.feed(chunk)
        keep = chunk[:max(0, self.limit - len(self.data))]
        self.data.extend(keep)
        self.truncated |= len(keep) != len(chunk)
        if self.sink is not None and keep:
            self.sink.write(keep)
            self.sink.flush()


def _kill_group(process):
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait(timeout=2)


def _run(command, timeout, environment, capture):
    """Bound the process, pipe-holding descendants, output storage and reaping."""
    start = time.monotonic()
    process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, env=environment, start_new_session=True)
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(process.stdout, selectors.EVENT_READ)
            while selector.get_map() or process.poll() is None:
                remaining = timeout - (time.monotonic() - start)
                if remaining <= 0:
                    return process.poll(), True
                for key, _ in selector.select(min(.05, remaining)):
                    chunk = os.read(key.fd, 65536)
                    if chunk:
                        capture.feed(chunk)
                    else:
                        selector.unregister(key.fileobj)
            return process.returncode, False
    finally:
        _kill_group(process)
        process.stdout.close()


def _control(command, deadline, environment):
    capture = _Capture(8192)
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        return dict(exit_status=None, timed_out=True, truncated=False, output="cleanup deadline elapsed")
    try:
        code, timed_out = _run(command, min(CONTROL_SECONDS, remaining), environment, capture)
        return dict(exit_status=code, timed_out=timed_out, truncated=capture.truncated,
                    output=capture.data.decode("utf-8", errors="replace"))
    except (OSError, subprocess.SubprocessError) as error:
        return dict(exit_status=None, timed_out=False, truncated=capture.truncated, output=str(error))


def _cleanup(identity, environment):
    if identity is None:
        return None
    result = dict(owned_step=identity.step, step_disappeared=False,
                  cancellation=None, confirmation=None)
    if identity.step is None:
        result["detail"] = "missing owned Slurm step identity; remote cleanup unverified"
        return result
    # Do not inherit scheduler filters or a foreign cluster override which could
    # hide our step or redirect a numeric job identity to another cluster.
    environment = {key: value for key, value in environment.items()
        if not key.startswith(("SCANCEL_", "SQUEUE_")) and key != "SLURM_CLUSTERS"}
    deadline = time.monotonic() + CLEANUP_SECONDS
    result["cancellation"] = _control(
        ["scancel", "--signal=KILL", identity.step], deadline, environment)
    valid_row = re.compile(re.escape(identity.job) + r"\.(?:[0-9]+|batch|extern|interactive)")
    while time.monotonic() < deadline:
        query = _control(["squeue", "--local", "--steps", f"--jobs={identity.job}",
                          "--noheader", "--format=%i"], deadline, environment)
        result["confirmation"] = query
        rows = [line.strip() for line in query["output"].splitlines() if line.strip()]
        if (query["exit_status"] == 0 and not query["timed_out"] and not query["truncated"]
                and all(valid_row.fullmatch(row) for row in rows)
                and identity.step not in rows):
            result["step_disappeared"] = True
            result["detail"] = "owned Slurm step disappeared; sibling steps were not cancelled"
            return result
        remaining = deadline - time.monotonic()
        if remaining > 0:
            time.sleep(min(.2, remaining))
    result["detail"] = "owned Slurm step disappearance not confirmed; remote cleanup unverified"
    return result


def require_coordinator(environment):
    """Allow the sbatch rank-zero context while rejecting MPI tasks and ambiguity."""
    for marker in ("QUEST_MPI_SUPERVISED_CHILD", "PMI_RANK", "PMIX_RANK",
                   "OMPI_COMM_WORLD_RANK", "MV2_COMM_WORLD_RANK"):
        if marker in environment:
            raise RuntimeError(f"nested MPI launch refused: {marker} is set")
    task_markers = ("SLURM_PROCID", "SLURM_LOCALID", "SLURM_NODEID",
                    "SLURM_STEP_ID", "SLURM_STEPID")
    if not any(marker in environment for marker in task_markers):
        return
    job = environment.get("SLURM_JOB_ID", "")
    batch = (re.fullmatch(r"[0-9]+", job) is not None
             and environment.get("SLURM_JOBID", job) == job
             and environment.get("SLURM_PROCID") == "0"
             and all(environment.get(marker, "0") == "0"
                     for marker in ("SLURM_LOCALID", "SLURM_NODEID"))
             and all(environment.get(marker, "batch") == "batch"
                     for marker in ("SLURM_STEP_ID", "SLURM_STEPID")))
    if not batch:
        raise RuntimeError("nested MPI launch refused: Slurm task metadata is not "
                           "an unambiguous rank-zero batch coordinator")


def run_job(command, timeout, log_path):
    """Return elapsed seconds or raise with bounded logs and a cleanup receipt.

    A companion <log_path>.supervision.json records status, truncation, owned
    step, cancellation result and scheduler confirmation even on failure.
    The caller must stop its campaign if remote cleanup is unverified.
    """
    if not command or not math.isfinite(timeout) or timeout <= 0:
        raise ValueError("a command and positive finite timeout are required")
    environment = dict(os.environ)
    require_coordinator(environment)
    identity = None
    if pathlib.Path(command[0]).name == "srun":
        job = environment.get("SLURM_JOB_ID", "")
        if re.fullmatch(r"[0-9]+", job) is None:
            raise ValueError("Slurm supervision requires a decimal SLURM_JOB_ID allocation")
        token = secrets.token_hex(24)
        environment["QUEST_CAPACITY_STEP_TOKEN"] = token
        identity = _StepIdentity(token, job)
    log_path = pathlib.Path(log_path)
    started = time.monotonic()
    receipt = dict(schema_version=1, exit_status=None, timed_out=False,
                   log_truncated=False, owned_step=None, cleanup=None, error=None)
    capture = None
    try:
        with log_path.open("wb") as log:
            capture = _Capture(MAX_LOG, log, identity)
            code, timed_out = _run(command, timeout, environment, capture)
            receipt.update(exit_status=code, timed_out=timed_out)
            if timed_out:
                raise RuntimeError("MPI capacity job timed out")
            if capture.truncated:
                raise RuntimeError("MPI launcher exceeded bounded log size")
            if code:
                raise RuntimeError(f"MPI capacity job exited {code}")
            if identity is not None and identity.step is None:
                raise RuntimeError("successful Slurm launcher did not report its owned step")
            log.flush()
            os.fsync(log.fileno())
    except BaseException as error:
        receipt["cleanup"] = _cleanup(identity, environment)
        receipt["error"] = str(error)
        if receipt["cleanup"] is not None and not receipt["cleanup"]["step_disappeared"]:
            receipt["error"] += "; " + receipt["cleanup"]["detail"]
        if isinstance(error, Exception):
            raise RuntimeError(receipt["error"]) from error
        raise
    finally:
        receipt["elapsed_seconds"] = time.monotonic() - started
        receipt["log_truncated"] = capture.truncated if capture is not None else False
        receipt["owned_step"] = identity.step if identity is not None else None
        receipt_path = pathlib.Path(str(log_path) + ".supervision.json")
        with receipt_path.open("w", encoding="utf-8") as output:
            json.dump(receipt, output, indent=2)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
    return receipt["elapsed_seconds"]
