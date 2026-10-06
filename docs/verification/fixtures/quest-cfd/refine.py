#!/usr/bin/env python3
"""One-axis-at-a-time complete five-coordinate KvN diagnostic campaign."""
import argparse
import json
import math
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from campaign import collect, sha256, source_identity


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--cap-mib", type=int, default=512)
    args = parser.parse_args()
    if not math.isfinite(args.timeout) or args.timeout <= 0 or args.cap_mib <= 0:
        parser.error("positive finite limits required")
    root = Path(__file__).resolve().parents[4]
    binary = args.binary.resolve(strict=True)
    receipt = {
        "schema": "quest-cfd-full-kvn-campaign-v1",
        "source_tree_sha256": source_identity(root),
        "source_hash_scope": "tracked and untracked nonignored files excluding generated verification/data receipts",
        "binary_sha256": sha256(binary),
        "scope": "bounded classical full five-coordinate nonlinear DG validation",
        "quantum_execution": False, "convergence_certified": False,
        "physical_coordinates": 5,
        "baseline": {"cells": 3, "order": 1, "extent": 1, "width": 1.2,
                     "horizon": 0.001, "steps": 4, "minimum_support_samples": 2},
        "experiments": [],
        "limitations": [
            "Minimum support-node policy is necessary sampling evidence, not convergence",
            "Outer-cell occupation is not outward flux or an error bound",
            "Classical RK4 sensitivity is not temporal DG refinement",
            "Initial regularization bias includes sampled quadrature error",
            "Physical mesh is fixed; all independent coordinates are retained",
            "Address-space cap is per process, not a distributed capacity test",
        ],
    }
    experiments = [
        ("baseline", []),
        ("configuration-h", ["--cells", "2"]),
        ("configuration-h", ["--cells", "4"]),
        ("configuration-p", ["--order", "2"]),
        # Extend by complete cells on each side, preserving baseline h=2/3
        # and all original physical quadrature locations (up to binary64 roundoff).
        ("configuration-domain", ["--extent", str(5/3), "--cells", "5"]),
        ("configuration-domain", ["--extent", str(7/3), "--cells", "7"]),
        ("regularization", ["--width", "0.8"]),
        ("regularization", ["--width", "0.6"]),
        ("regularization", ["--width", "0.4"]),
        ("reference-integration", ["--steps", "8"]),
        ("concentration-window", ["--horizon", "0.01", "--steps", "40"]),
    ]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    for axis, arguments in experiments:
        result = collect(binary, arguments, args.cap_mib * 1024**2, args.timeout)
        receipt["experiments"].append({"axis": axis, **result})
        args.output.write_text(json.dumps(receipt, indent=2, allow_nan=False) + "\n")
        print(axis, arguments, "exit", result["exit_code"],
              "status", (result["report"] or {}).get("status"), flush=True)


if __name__ == "__main__":
    main()
