#!/usr/bin/env python3
"""Reconcile the new native183 follow-up without changing historical receipts."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from campaign import SOURCE_REVISIONS, atomic_json, completion_summary, sha256, source_inventory
from softwarex_unavailable import METADATA_SHA256


def diagnose(result):
    if result["status"] == "accuracy_failure" and result.get("measurement") and result.get("source_assertion_gate_passed") is False:
        return dict(category="group_assertion_invalidated_summary",
                    detail="A source assertion failed somewhere in the indivisible Catch test case; its final group gate invalidates this earlier summary. This is not evidence of a numerical accuracy failure in this individual workload.")
    if result["status"] == "unattempted" and result.get("process_returncode") not in (None, 0):
        return dict(category="no_timing_observed_after_group_failure",
                    detail="No benchmark start or timing was emitted; the group failed. Fixture preflight may have been attempted before the benchmark start event.")
    return dict(category=result["status"], detail=result.get("reason"))


def summarize(results):
    counts = Counter(row["status"] for row in results)
    return dict(schema_version=1, accounting_complete=True, expected_rows=len(results), outcomes=dict(counts),
                execution_coverage_complete=all(row["status"] in {"ok", "measured_summary_only"} for row in results),
                raw_performance_coverage_complete=all(row["status"] == "ok" for row in results),
                observed_summary_rows=counts["measured_summary_only"],
                raw_sample_rows=counts["ok"], unsupported_rows=counts["unsupported"],
                unattempted_rows=counts["unattempted"],
                diagnostic_outcomes=dict(Counter(diagnose(row)["category"] for row in results)),
                historical_811_receipt_modified=False, cross_language_timing_comparisons=0)


def combine(named, source, softwarex, output):
    metadata = Path(__file__).resolve().parents[1] / "softwarex-case-inventory.json"
    if sha256(metadata) != METADATA_SHA256:
        raise ValueError("pinned SoftwareX workload metadata changed")
    expected = source_inventory(json.loads(metadata.read_text()))
    roles = (
        ("named", named, {"named_inverse", "named_solver"}, "quest_qsvt/"),
        ("source", source, {"root_isolation", "roots_of_unity", "circuit", "unitary", "coordinate"}, "quest_qsvt_cpp/"),
        ("softwarex", softwarex, {"softwarex"}, "quest_qsvt/"),
    )
    manifests = {}
    for role, lane, suites, prefix in roles:
        manifest = json.loads((lane / "manifest.json").read_text())
        rows = manifest["rows"]
        identities = {prefix + row["case_id"]: row["case_id"] for row in expected if row["suite"] in suites}
        if len(rows) != len(identities) or len({row["id"] for row in rows}) != len(rows) or {row["id"]: row["case_id"] for row in rows} != identities:
            raise ValueError(f"{role} identities differ from the pinned source workload inventory")
        manifests[role] = manifest
    from run_named import named_manifest
    from run_source_suites import source_manifest, validate_lane
    for role, factory in (("named", named_manifest), ("source", source_manifest)):
        groups, rows = factory()
        actual = manifests[role]
        if ({row["id"]: row for row in actual["rows"]} != {row["id"]: row for row in rows}
                or actual.get("groups") != groups):
            raise ValueError(f"{role} routing or workload contract differs from the pinned execution manifest")
    expected_legacy = {row["case_id"]: row for row in expected if row["suite"] == "softwarex"}
    for row in manifests["softwarex"]["rows"]:
        if any(row.get(key) != value for key, value in expected_legacy[row["case_id"]].items()):
            raise ValueError("SoftwareX unsupported workload contract differs from pinned inventory")
    all_rows, all_results, receipts = [], [], []
    for role, lane, _, _ in roles:
        manifest = manifests[role]
        results = [json.loads(line) for line in (lane / "results.jsonl").read_text().splitlines()]
        marker = json.loads((lane / "completion.json").read_text())
        if role == "softwarex":
            if marker != completion_summary(lane, manifest["rows"], results):
                raise ValueError("SoftwareX capability completion marker differs from its evidence")
            if any(row["status"] != "unsupported" for row in results):
                raise ValueError("SoftwareX legacy capability lane cannot contain substitute measurements")
            if manifest["sources"] != SOURCE_REVISIONS or manifest["metadata_sha256"] != METADATA_SHA256:
                raise ValueError("SoftwareX capability provenance differs from the pinned source")
            for artifact in manifest["source_evidence"]:
                evidence = (lane / artifact["artifact"]).resolve()
                if not evidence.is_relative_to(lane.resolve()) or sha256(evidence) != artifact["sha256"]:
                    raise ValueError("SoftwareX capability source evidence changed")
            if sha256(lane / "evidence" / "softwarex_unavailable.py") != manifest["controller_sha256"]:
                raise ValueError("SoftwareX capability controller snapshot changed")
        else:
            if marker != validate_lane(lane):
                raise ValueError("native completion marker differs from preserved evidence")
            identity = json.loads((lane / "identity.json").read_text())
            if identity["source_revision"] != SOURCE_REVISIONS["quest_qsvt"]:
                raise ValueError("native lane source revision changed")
            digest = hashlib.sha256(json.dumps(identity["files"], sort_keys=True).encode()).hexdigest()
            if digest != identity["sha256"]:
                raise ValueError("native identity inventory digest is inconsistent")
        if not marker["accounting_complete"]:
            raise ValueError("native lane has no complete accounting receipt")
        all_rows.extend(dict(row, source_lane=role) for row in manifest["rows"])
        all_results.extend(dict(row, source_lane=role, diagnostic=diagnose(row)) for row in results)
        files = ["manifest.json", "results.jsonl", "completion.json"]
        if role != "softwarex":
            files += ["identity.json", "processes.json"]
        receipts.append(dict(role=role, lane=lane.name, rows=len(results),
                             files={name: sha256(lane / name) for name in files}))
    if len(all_results) != 183 or len({row["id"] for row in all_results}) != 183:
        raise ValueError("new native follow-up requires exactly 183 distinct diagnosed workloads")
    summary = summarize(all_results)
    output.mkdir(parents=True, exist_ok=False)
    atomic_json(output / "manifest.json", dict(sources=SOURCE_REVISIONS, rows=all_rows, lanes=receipts))
    (output / "results.jsonl").write_text("".join(json.dumps(row, allow_nan=False) + "\n" for row in all_results))
    summary["manifest_sha256"] = sha256(output / "manifest.json")
    summary["results_sha256"] = sha256(output / "results.jsonl")
    atomic_json(output / "completion.json", summary)
    return summary


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("named", "source", "softwarex", "output"):
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(combine(args.named, args.source, args.softwarex, args.output)))
