#!/usr/bin/env python3
"""Run the pinned, unchanged named native Catch test cases in a new lane.

The source has four selectable test cases, containing 26 benchmark workloads.
This runner does not split those source loops or reconstruct their algorithms.
"""
import argparse
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from campaign import source_inventory


def named_manifest():
    definitions = (
        ("inverse_quick", "Benchmark: Inverse Transformation (quick)"),
        ("inverse_full", "Benchmark: Inverse Transformation (full)"),
        ("solver_random", "Benchmark: NLFT solver random basis families"),
        ("solver_industrial", "Benchmark: NLFT solver industrial families"),
    )
    groups = [dict(name=name, filter=selection,
                   binary="test/qsp_tools/unit_tests_nlft",
                   extra_args=["--rng-seed", "2723225244"])
              for name, selection in definitions]
    rows = []
    for source in source_inventory([]):
        if source["suite"] not in ("named_inverse", "named_solver"):
            continue
        degree = source["degree"]
        if source["suite"] == "named_inverse":
            group = "inverse_quick" if degree <= 20000 else "inverse_full"
            title = f"Order: {degree}" if degree <= 1000 else f"Forward-prepared order: {degree}"
            preparation = ("source prepare_weiss_inverse_fixture(target, 1e-12)"
                           if degree <= 1000 else "source prepare_forward_inverse_fixture(degree)")
            tolerance = 1e-12 if degree <= 1000 else None
            accuracy = ("source certified Weiss residual upper bound and inverse preflight"
                        if degree <= 1000 else "source forward_fixture_component_tolerance(target.size())")
        else:
            title = source["case_id"].split("/", 1)[1]
            group = "solver_random" if title.startswith("random_basis_") else "solver_industrial"
            preparation = "source run_solver_once followed by validate_control_report before timing"
            tolerance = 1e-12
            accuracy = "source evidence-bearing control report and exact source validation"
        rows.append(dict(source, id=f"quest_qsvt/{source['case_id']}", group=group,
                         benchmark_name=title, protocol="catch2_10",
                         requested_accuracy=tolerance, accuracy_contract=accuracy,
                         source_preparation=preparation, input_sha256=None,
                         input_identity_status="source-generated; exact coefficients not exported by unchanged test",
                         seed=2723225244 if source["suite"] == "named_inverse" and degree <= 1000 else
                         424242 if source["suite"] == "named_solver" else None,
                         deadline_scope="whole Catch test case including setup and every enclosed workload"))
    return groups, rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--build", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--launcher", nargs=argparse.REMAINDER, default=[])
    args = parser.parse_args()
    # Import execution only after metadata/argument handling. Discovery does not
    # invoke QuEST, initialise Catch, or construct source fixtures.
    from run_source_suites import run_groups
    groups, rows = named_manifest()
    run_groups(args.source.resolve(), args.build.resolve(), args.output.resolve(), groups, rows, args.launcher)


if __name__ == "__main__":
    main()
