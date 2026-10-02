#!/usr/bin/env python3
"""Print three-or-more-trial numerical runtime medians and allocation statistics."""
import argparse
import csv
import statistics
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--receipts", type=Path, required=True)
args = parser.parse_args()
rows = {}
for label in ["baseline", "baseline-typed", "current", "mp"]:
    files = sorted(args.receipts.glob(f"{label}-trial*.csv"))
    if len(files) < 3:
        parser.error(f"fewer than three trials for {label}")
    trials = []
    for path in files:
        with path.open() as stream:
            trials.append({row["workload"]: row for row in csv.DictReader(stream)})
    rows[label] = {}
    for name in trials[0]:
        times = [int(t[name]["nanoseconds"]) / int(t[name]["iterations"]) for t in trials]
        allocations = [int(t[name]["allocations"]) / int(t[name]["iterations"]) for t in trials]
        peaks = [int(t[name]["peak_live_extra_bytes"]) for t in trials]
        rows[label][name] = (statistics.median(times), min(times), max(times), statistics.median(allocations), max(peaks))
print("representation,workload,median_ns_per_call,min_ns_per_call,max_ns_per_call,median_allocations_per_call,max_peak_extra_bytes")
for label, workloads in rows.items():
    for name, values in workloads.items():
        print(",".join(map(str, [label, name, *values])))
print("workload,current_over_baseline_dynamic,current_over_baseline_typed")
for name, values in rows["current"].items():
    print(f'{name},{values[0] / rows["baseline"][name][0]},{values[0] / rows["baseline-typed"][name][0]}')
