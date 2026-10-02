#!/usr/bin/env python3
"""Summarize the raw CPU and wall timings without external dependencies."""
import csv
import statistics
import sys
from collections import defaultdict

if len(sys.argv) not in (3,4):
    raise SystemExit("usage: summarize.py RAW.csv SUMMARY.csv [HISTORICAL_RAW.csv]")

groups = defaultdict(list)
with open(sys.argv[1], newline="") as source:
    for row in csv.DictReader(source):
        key = (int(row["bits"]), row["backend"], row["allocation"], row["workload"])
        groups[key].append(row)

historical=defaultdict(list)
if len(sys.argv)==4:
    with open(sys.argv[3],newline="") as source:
        for row in csv.DictReader(source):
            if row["allocation"]=="fresh":
                historical[int(row["bits"]),row["backend"],row["workload"]].append(float(row["cpu_ns_per_iteration"]))

with open(sys.argv[2], "w", newline="") as output:
    fields = ["bits", "backend", "allocation", "workload", "trials",
              "cpu_ns_median", "cpu_ns_min", "cpu_ns_max", "wall_ns_median",
              "wall_ns_min", "wall_ns_max", "historical_astro_cpu_speedup", "historical_rug_cpu_speedup"]
    writer = csv.DictWriter(output, fieldnames=fields)
    writer.writeheader()
    for (bits, backend, allocation, workload), rows in sorted(groups.items()):
        assert len(rows) == 3
        assert {row["trial"] for row in rows} == {"0", "1", "2"}
        cpu = [float(row["cpu_ns_per_iteration"]) for row in rows]
        wall = [float(row["wall_ns_per_iteration"]) for row in rows]
        comparisons=[statistics.median(historical[bits,label,workload])/statistics.median(cpu) if historical[bits,label,workload] else "" for label in ["astro","rug"]]
        writer.writerow(dict(zip(fields, [bits, backend, allocation, workload,
            len(rows), statistics.median(cpu), min(cpu), max(cpu),
            statistics.median(wall), min(wall), max(wall),
            *comparisons])))
