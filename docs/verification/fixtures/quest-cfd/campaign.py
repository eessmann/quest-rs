#!/usr/bin/env python3
"""Bounded independent CFD refinements and full-coordinate representation curves.

This is a local construction/classical campaign. It cannot assert continuum or
published-window convergence, quantum execution, or multi-host capacity.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import resource
import signal
import subprocess
import tempfile
import time

CONFIGURATIONS = [
    ("tgv2d", 100), ("tgv3d", 100), ("tgv3d", 1600),
    ("cavity2d", 100), ("cavity2d", 1000),
    ("cavity3d", 100), ("cavity3d", 1000),
    ("shedding2d", 100), ("shedding3d", 300),
]


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def report_finite(value):
    """Bounded-depth traversal without recursive Python calls or a wide stack."""
    stack = [iter((value,))]
    sentinel = object()
    nodes = 0
    while stack:
        item = next(stack[-1], sentinel)
        if item is sentinel:
            stack.pop()
            continue
        nodes += 1
        if nodes > 1_000_000:
            return False
        if isinstance(item, (dict, list)):
            if len(stack) >= 64:
                return False
            stack.append(iter(item.values() if isinstance(item, dict) else item))
        elif isinstance(item, float) and not math.isfinite(item):
            return False
    return True


def decode_report(payload):
    """Reject ambiguous JSON and bounded malformed child output explicitly."""
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate report key")
            result[key] = value
        return result

    def invalid_constant(_):
        raise ValueError("nonfinite report constant")

    try:
        result = json.loads(payload, object_pairs_hook=unique,
                            parse_constant=invalid_constant)
    except RecursionError as error:
        raise ValueError("report exceeds JSON decoder depth") from error
    if not report_finite(result):
        raise ValueError("report exceeds finite/depth/node limits")
    return result


def source_identity(root):
    result = subprocess.run(["git", "ls-files", "-co", "--exclude-standard", "-z"],
                            cwd=root, check=True, capture_output=True)
    digest = hashlib.sha256()
    for name in sorted(set(result.stdout.split(b"\0")) - {b""}):
        if name.startswith(b"docs/verification/data/"):
            continue
        path = root / os.fsdecode(name)
        if path.is_file():
            digest.update(name + b"\0" + bytes.fromhex(sha256(path)))
    return digest.hexdigest()


def collect(binary, arguments, cap, timeout):
    def limit():
        resource.setrlimit(resource.RLIMIT_AS, (cap, cap))
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

    with tempfile.TemporaryDirectory(prefix="quest-cfd-campaign-") as directory:
        timing = Path(directory) / "timing.txt"
        # File capture keeps a malformed child's output out of the Python heap.
        with (Path(directory) / "stdout").open("w+b") as out, (Path(directory) / "stderr").open("w+b") as err:
            started = time.monotonic()
            child = subprocess.Popen(["/usr/bin/time", "-f", "%e %M", "-o", str(timing),
                                      str(binary), *arguments], stdout=out, stderr=err,
                                     env={**os.environ, "OMP_NUM_THREADS": "1"},
                                     preexec_fn=limit, start_new_session=True)
            timed_out = False
            try:
                child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            elapsed = time.monotonic() - started
            output_bytes = out.tell()
            out.seek(0)
            try:
                report = decode_report(out.read(4 * 1024**2)) if output_bytes <= 4 * 1024**2 else None
            except (ValueError, UnicodeDecodeError):
                report = None
            peak = None
            if timing.exists():
                fields = timing.read_text().splitlines()
                if fields and len(fields[-1].split()) == 2:
                    try:
                        peak = int(fields[-1].split()[1])
                    except ValueError:
                        pass
        return {"arguments": arguments, "exit_code": child.returncode, "timed_out": timed_out,
                "elapsed_seconds": elapsed, "peak_rss_kib": peak,
                "address_space_cap_bytes": cap, "stdout_bytes": output_bytes,
                "report": report, "valid_report": report is not None}


def experiments():
    for case, re in CONFIGURATIONS:
        base = ["--case", case, "--reynolds", str(re)]
        variants = [("baseline", [], 0.0001, 2)]
        if case.startswith(("tgv", "cavity")):
            variants += [("physical-h", ["--mesh", "2"], 0.0001, 2),
                         ("physical-p", ["--physical-order", "2"], 0.0001, 2),
                         ("time", [], 0.00005, 4)]
        elif case == "shedding2d":
            variants += [("radial-mesh", ["--cylinder-layers", "2"], 0.0001, 2),
                         ("geometry", ["--cylinder-sectors", "8"], 0.0001, 2),
                         ("time", [], 0.00005, 4)]
        for axis, extra, dt, steps in variants:
            yield {"case": case, "reynolds": re, "axis": axis, "base": base + extra,
                   "dt": dt, "steps": steps, "window": "short transient only"}
    for case, order in [("burgers", 1), ("kdv", 2)]:
        for axis, extra, dt, steps in [
            ("baseline", [], 0.0001, 1000),
            ("physical-h", ["--cells", "8"], 0.0001, 1000),
            ("physical-p", ["--physical-order", str(order + 1)], 0.0001, 1000),
            ("time", [], 0.00005, 2000),
        ]:
            yield {"case": case, "axis": axis, "base": ["--case", case, *extra],
                   "dt": dt, "steps": steps, "window": "frozen demonstration T=0.1"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--cap-mib", type=int, default=512)
    parser.add_argument("--timeout", type=float, default=120)
    args = parser.parse_args()
    if args.cap_mib <= 0 or not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("positive memory and timeout limits required")
    root = Path(__file__).resolve().parents[4]
    binary = args.binary.resolve(strict=True)
    receipt = {"schema": 2, "source_tree_sha256": source_identity(root),
               "source_hash_scope": "tracked and untracked nonignored files excluding generated verification/data receipts",
               "binary_sha256": sha256(binary), "scope": "single-host bounded classical and representation studies",
               "quantum_execution": False, "benchmark_convergence_established": False,
               "multi_host_capacity_established": False,
               "ancilla_policy": "caller allowance 16; no circuit or accuracy admission",
               "manifest_sha256": {p.name: sha256(p) for p in sorted((root / "crates/quest-cfd/cases").glob("*.json"))},
               "experiments": [], "carleman_order_studies": []}
    cap = args.cap_mib * 1024**2
    for exp in experiments():
        record = {key: value for key, value in exp.items() if key != "base"}
        common = exp["base"]
        reference = ["reference", *common, "--dt", str(exp["dt"]), "--steps", str(exp["steps"])]
        record["classical"] = collect(binary, reference, cap, args.timeout)
        # Both lifts retain every physical coordinate. These fixed-order counts
        # have no accuracy-dependent choice of mesh, order, ancillas or precision.
        temporal = ["--time-elements", "2"] if exp["case"] in ("burgers", "kdv") else []
        record["kvn"] = collect(binary, ["estimate", *common, *temporal], cap, args.timeout)
        record["carleman"] = [collect(binary, ["estimate", *common, *temporal,
                "--lift", "carleman", "--carleman-order", str(order)], cap, args.timeout)
                for order in (1, 2, 4, 8)]
        receipt["experiments"].append(record)
        print(f"{exp['case']} {exp['axis']} reference exit={record['classical']['exit_code']}", flush=True)
    for case in ("burgers", "kdv"):
        for order in (1, 2, 3):
            report = collect(binary, ["reference", "--case", case, "--lift", "carleman",
                "--carleman-order", str(order), "--dt", "0.0001", "--steps", "1000"], cap, args.timeout)
            receipt["carleman_order_studies"].append({"case": case, "order": order, **report})
            print(f"{case} lift-order={order} exit={report['exit_code']}", flush=True)
    receipt["remaining_acceptance"] = [
        "Published-window CFD statistics, resolved mesh/geometry and convergence tolerances",
        "Literal 3D wake pressure/outlet closure and execution",
        "KvN independent width/domain/configuration refinement including leakage and concentration",
        "Error-dependent quantum circuit cost and sampling evidence",
        "Actual multi-host input larger than each node memory cap and actual large-count transport",
    ]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(receipt, indent=2, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
