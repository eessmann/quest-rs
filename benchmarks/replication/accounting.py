#!/usr/bin/env python3
"""Account for native prerequisites and combine complete, disjoint run lanes."""
import argparse
import json
from pathlib import Path
import shutil
from campaign import SOURCE_REVISIONS, atomic_json, completion_summary, publish_completion, sha256, source_inventory, rust_manifest


def native_lane(source, configure_log, output):
    log = configure_log.read_text()
    if 'package configuration file provided by "autodiff"' not in log or "Configuring incomplete" not in log:
        raise ValueError("the observed native configure prerequisite failure is required")
    inventory = json.loads((source / "inventory.json").read_text())["workloads"]
    output.mkdir(parents=True, exist_ok=False)
    shutil.copy2(configure_log, output / "configure.log")
    manifest, results = [], []
    for source_row in inventory:
        row = dict(source_row, id="quest_qsvt_cpp/" + source_row["case_id"],
                   implementation="quest_qsvt_cpp", backend="requested_pocketfft", workers=1,
                   measurement_started=False)
        manifest.append(row)
        results.append(dict(id=row["id"], status="native_error", phase="configure", returncode=1,
                            reason="required exact autodiff 1.1.2 CMake package unavailable",
                            input_status=row["input_status"], log="configure.log",
                            log_sha256=sha256(output / "configure.log"), raw_samples=[]))
    atomic_json(output / "manifest.json", dict(sources=SOURCE_REVISIONS, rows=manifest))
    (output / "results.jsonl").write_text("".join(json.dumps(row) + "\n" for row in results))
    return publish_completion(output, manifest, results)


def combined(lanes, diagnostics, output):
    if len(lanes) != 5:
        raise ValueError("require exactly native, original/current canonical, and original/current unitary lanes")
    roles = {}
    for lane in lanes:
        rows = json.loads((lane / "manifest.json").read_text())["rows"]
        implementations = {row.get("implementation") for row in rows}
        if len(implementations) != 1:
            raise ValueError("each lane must have exactly one implementation role")
        role = next(iter(implementations))
        if role in roles:
            raise ValueError("duplicate implementation role cannot replace a required lane")
        roles[role] = rows
    required = {"quest_qsvt_cpp", "original_0f95954", "current", "rust_original", "rust_current"}
    if set(roles) != required:
        raise ValueError("require native, original/current canonical, and original/current unitary roles")
    inventory_path = Path(__file__).with_name("softwarex-case-inventory.json")
    if sha256(inventory_path) != "66b7773cba0cdc4dceca173cbd40590be3941749c7bd0e5e9bf86665d2410ad3":
        raise ValueError("pinned SoftwareX case metadata changed; regenerate only from the checksum-verified corpus")
    cases = json.loads(inventory_path.read_text())
    expected_native = source_inventory(cases)
    if {row["id"] for row in roles["quest_qsvt_cpp"]} != {"quest_qsvt_cpp/" + row["case_id"] for row in expected_native}:
        raise ValueError("native workload identities do not match the pinned source inventory")
    for role in ("original_0f95954", "current"):
        expected_ids = {row["id"] for row in rust_manifest(expected_native, role)}
        if {row["id"] for row in roles[role]} != expected_ids:
            raise ValueError("canonical workload identities do not match the pinned five-scope matrix")
    for role in ("rust_original", "rust_current"):
        if {row["id"] for row in roles[role]} != {f"{role}/numerical_unitary/dimension{dimension}" for dimension in (64, 128, 256, 512)}:
            raise ValueError("unitary workload identities do not match the source dimensions")
    from report import matching_contract
    for before_role, after_role in (("original_0f95954", "current"), ("rust_original", "rust_current")):
        before = {row["id"].split("/", 1)[1]: row for row in roles[before_role]}
        after = {row["id"].split("/", 1)[1]: row for row in roles[after_role]}
        if any(not matching_contract(before[name], after[name]) for name in before):
            raise ValueError("original/current lane operation contracts differ")
    manifest, results = [], []
    lane_receipts = []
    for lane in lanes:
        marker = json.loads((lane / "completion.json").read_text())
        if not marker.get("accounting_complete"):
            raise ValueError(f"incomplete lane: {lane.name}")
        rows = json.loads((lane / "manifest.json").read_text())["rows"]
        outcomes = [json.loads(line) for line in (lane / "results.jsonl").read_text().splitlines()]
        completion_summary(lane, rows, outcomes)
        manifest.extend(rows)
        results.extend({key: value for key, value in row.items() if key not in ("raw_samples", "log", "log_sha256", "peak_rss_log", "peak_rss_log_sha256")} for row in outcomes)
        lane_receipts.append({"lane": lane.name, "rows": len(rows),
                              "manifest_sha256": sha256(lane / "manifest.json"),
                              "results_sha256": sha256(lane / "results.jsonl"),
                              "completion_sha256": sha256(lane / "completion.json")})
    if sorted(lane["rows"] for lane in lane_receipts) != [4, 4, 183, 310, 310]:
        raise ValueError("required lane cardinalities are 183, 310, 310, 4, 4")
    identifiers = [row["id"] for row in manifest]
    if len(identifiers) != len(set(identifiers)) or {row["id"] for row in results} != set(identifiers):
        raise ValueError("overlapping lanes or missing terminal records")
    if len(manifest) != 811:
        raise ValueError("expected native183 + canonical Rust620 + unitary Rust8 = 811 rows")
    output.mkdir(parents=True, exist_ok=False)
    atomic_json(output / "manifest.json", {"sources": SOURCE_REVISIONS, "rows": manifest})
    (output / "results.jsonl").write_text("".join(json.dumps(row) + "\n" for row in results))
    from collections import Counter
    diagnostic_rows = json.loads((diagnostics / "manifest.json").read_text())["rows"]
    diagnostic_results = [json.loads(line) for line in (diagnostics / "results.jsonl").read_text().splitlines()]
    diagnostic_ids = {"quest_qsvt/" + row["case_id"] for row in source_inventory([])
                      if row["suite"] in {"root_isolation", "roots_of_unity", "circuit", "unitary", "coordinate"}}
    if {row["id"] for row in diagnostic_rows} != diagnostic_ids or len(diagnostic_rows) != 33:
        raise ValueError("required source contract diagnostic inventory has 33 entries")
    completion_summary(diagnostics, diagnostic_rows, diagnostic_results)
    diagnostic_receipt = {"name": diagnostics.name, "manifest_sha256": sha256(diagnostics / "manifest.json"),
                          "results_sha256": sha256(diagnostics / "results.jsonl"), "included_in_workload_count": False}
    marker = dict(accounting_complete=True, performance_coverage_complete=all(row["status"] == "ok" for row in results),
                  expected_rows=811, outcomes=dict(Counter(row["status"] for row in results)),
                  lanes=lane_receipts, capability_diagnostics=diagnostic_receipt,
                  historical_1488_campaign_included=False)
    atomic_json(output / "completion.json", marker)
    return marker


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    native = sub.add_parser("native")
    native.add_argument("--source", type=Path, required=True)
    native.add_argument("--configure-log", type=Path, required=True)
    native.add_argument("--output", type=Path, required=True)
    combine = sub.add_parser("combine")
    combine.add_argument("--lane", type=Path, action="append", required=True)
    combine.add_argument("--diagnostics", type=Path, required=True)
    combine.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = native_lane(args.source, args.configure_log, args.output) if args.action == "native" else combined(args.lane, args.diagnostics, args.output)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
