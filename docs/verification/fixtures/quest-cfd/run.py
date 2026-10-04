#!/usr/bin/env python3
"""Collect coarse classical/resource receipts; never labels these benchmark convergence."""
import argparse
import json
import os
from pathlib import Path
import resource
import subprocess
import tempfile

CONFIGURATIONS = [
    ("tgv2d", 100), ("tgv3d", 100), ("tgv3d", 1600),
    ("cavity2d", 100), ("cavity2d", 1000),
    ("cavity3d", 100), ("cavity3d", 1000),
    ("shedding2d", 100), ("shedding3d", 300),
]


def collect(binary, arguments, cap=None):
    def limit():
        resource.setrlimit(resource.RLIMIT_AS, (cap, cap))

    with tempfile.TemporaryDirectory(prefix="quest-cfd-receipt-") as directory:
        timing = Path(directory) / "timing.txt"
        command = ["/usr/bin/time", "-f", "%e %M", "-o", str(timing),
                   str(binary), *arguments]
        result = subprocess.run(command, capture_output=True, text=True, timeout=180,
                                env={**os.environ, "OMP_NUM_THREADS": "1"},
                                preexec_fn=limit if cap else None, check=False)
        try:
            report = json.loads(result.stdout)
        except json.JSONDecodeError:
            report = {"invalid_report": result.stdout, "stderr": result.stderr}
        elapsed, peak = timing.read_text().splitlines()[-1].split()
        return {"arguments": arguments, "exit_code": result.returncode,
                "elapsed_seconds": float(elapsed), "peak_rss_kib": int(peak),
                "address_space_cap_bytes": cap, "report": report}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    references, estimates = [], []
    for case, reynolds in CONFIGURATIONS:
        common = ["--case", case, "--reynolds", str(reynolds)]
        references.append(collect(binary, ["reference", *common, "--dt", "0.0001", "--steps", "2"]))
        estimates.append(collect(binary, ["estimate", *common]))
    capacity = [
        collect(binary, ["estimate", "--case", "tgv3d", "--mesh", "100"], 256 * 1024**2),
        collect(binary, ["build", "--case", "smoke", "--max-dimension", "128"]),
        collect(binary, ["solve", "--case", "smoke", "--max-state-query-work", "100"]),
    ]
    receipt = {"schema": 1, "date": "2026-10-04", "scope": "single-host coarse snapshots and full tensor resource estimates",
               "benchmark_convergence_established": False, "multi_host_capacity_established": False,
               "references": references, "estimates": estimates, "capacity": capacity}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
