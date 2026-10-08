#!/usr/bin/env python3
"""Compare complete lanes only where measured operation contracts match."""
import argparse
from collections import Counter
import json
import statistics
from campaign import completion_summary, sha256
from pathlib import Path

CONTRACT = ("input_sha256", "scope", "backend", "workers", "protocol", "requested_accuracy",
            "effective_completion_tolerance", "sample_count", "warmup_seconds", "measurement_seconds")


def matching_contract(original, current):
    return all(original.get(key) == current.get(key) for key in CONTRACT)


def load_lane(root):
    completion = json.loads((root / "completion.json").read_text())
    if not completion.get("accounting_complete"):
        raise ValueError("lane accounting must be complete")
    manifest = json.loads((root / "manifest.json").read_text())["rows"]
    results = [json.loads(line) for line in (root / "results.jsonl").read_text().splitlines()]
    if len(results) != len(manifest) or {row["id"] for row in results} != {row["id"] for row in manifest}:
        raise ValueError("manifest/result mismatch")
    completion_summary(root, manifest, results)
    by_id = {row["id"]: row for row in results}
    return {row["id"].split("/", 1)[1]: (row, by_id[row["id"]]) for row in manifest}


def point_estimate(root, result):
    sample = next(artifact for artifact in result["raw_samples"] if "/new/" in artifact["path"])
    data = json.loads((root / sample["path"]).read_text())
    return statistics.median(duration / iterations for duration, iterations in zip(data["times"], data["iters"]))


def outcome_diagnostic(root, result):
    """Interpret preserved terminal statuses without rewriting measured evidence."""
    if result["status"] == "ok":
        return {"category": "ok", "detail": None}
    log = ""
    if result.get("log"):
        path = (root / result["log"]).resolve()
        if not path.is_relative_to(root.resolve()) or sha256(path) != result["log_sha256"]:
            raise ValueError("diagnostic requires the verified original log")
        log = path.read_text(errors="replace")
    detail = next((line.strip() for line in log.splitlines() if line.strip().startswith("Error:")), result.get("reason"))
    if "Error: ContractivityViolation" in log:
        category = "numerical_admission_violation"
    elif "Error: Contractivity" in log:
        category = "numerical_admission_not_established"
    elif "Error: Numerics(Budget {" in log:
        category = "resource_limit"
    elif result.get("phase") == "configure":
        category = "native_dependency_failure"
    elif result["status"] == "native_error":
        category = "execution_failure"
    else:
        category = result["status"]
    return {"category": category, "detail": detail}


def compare(original_root, current_root):
    original = load_lane(original_root)
    current = load_lane(current_root)
    if original.keys() != current.keys():
        raise ValueError("matched lanes require identical workload identities")
    rows = []
    for identity, (old_manifest, old) in original.items():
        new_manifest, new = current[identity]
        item = {"case": identity, "original_status": old["status"], "current_status": new["status"],
                "original_diagnostic": outcome_diagnostic(original_root, old),
                "current_diagnostic": outcome_diagnostic(current_root, new)}
        if old["status"] == new["status"] == "ok" and matching_contract(old_manifest, new_manifest):
            before, after = point_estimate(original_root, old), point_estimate(current_root, new)
            item.update(original_median_ns=before, current_median_ns=after, median_ratio_original_over_current=before / after)
        else:
            item["comparison"] = "no timing ratio: failed outcome or mismatched operation contract"
        rows.append(item)
    return {"original_outcomes": dict(Counter(result["status"] for _, result in original.values())),
            "current_outcomes": dict(Counter(result["status"] for _, result in current.values())),
            "rows": rows,
            "interpretation": "Per-case median ratios are descriptive; no aggregate speedup or statistical significance is claimed."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--original", type=Path, required=True)
    parser.add_argument("--current", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(compare(args.original, args.current), indent=2, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
