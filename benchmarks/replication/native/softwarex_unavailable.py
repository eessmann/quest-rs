#!/usr/bin/env python3
"""Record the unavailable legacy executable contract without running substitutes."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import re
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from campaign import SOURCE_REVISIONS, atomic_json, publish_completion, sha256, source_inventory

METADATA_SHA256 = "66b7773cba0cdc4dceca173cbd40590be3941749c7bd0e5e9bf86665d2410ad3"
REASON = ("The pinned SoftwareX runner requires --pipeline solver|kernel and --repeat; "
          "the pinned native executable exposes --scope with five stage contracts and "
          "requires an attested --input-root bundle. No unchanged executable implements "
          "the requested legacy boundary. Reconstructing its timer or relabeling a modern "
          "scope is outside the approved execution contract.")


def verify_boundary(native, legacy):
    if not all(f'"{flag}"' in legacy for flag in ("--pipeline", "--repeat", "--corpus", "--case-id")):
        raise ValueError("expected SoftwareX legacy CLI evidence is absent")
    if any(f'"{flag}"' in native for flag in ("--pipeline", "--repeat")):
        raise ValueError("legacy option present; reassess native capability before diagnosing unavailable")
    if not all(f'"{flag}"' in native for flag in ("--scope", "--input-root")):
        raise ValueError("expected modern native CLI evidence is absent")


def matches_committed(content, committed):
    if content == committed:
        return True
    pointer = re.fullmatch(rb"version https://git-lfs.github.com/spec/v1\noid sha256:([0-9a-f]{64})\nsize ([0-9]+)\n", committed)
    return bool(pointer and hashlib.sha256(content).hexdigest() == pointer[1].decode()
                and len(content) == int(pointer[2]))


def write_unavailable(quest_source, softwarex_source, output):
    specifications = (
        (quest_source, "quest_qsvt", "test/qsp_tools/nlft_bench_harness.cpp", "native-harness.cpp", [83, 99]),
        (softwarex_source, "softwarex", "benchmarks/nlft/run_campaign.py", "softwarex-runner.py", [208, 220]),
    )
    observed = []
    for root, role, relative, stored, lines in specifications:
        revision = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()
        if revision != SOURCE_REVISIONS[role]:
            raise ValueError("source revision differs from the pinned benchmark contract")
        content = (root / relative).read_bytes()
        committed = subprocess.check_output(["git", "-C", str(root), "show", f"{revision}:{relative}"])
        if not matches_committed(content, committed):
            raise ValueError("source evidence has uncommitted changes")
        observed.append((root / relative, stored, dict(source=role, revision=revision,
                        path=relative, lines=lines, artifact=f"evidence/{stored}", sha256=sha256(root / relative))))
    verify_boundary(observed[0][0].read_text(), observed[1][0].read_text())
    inventory = Path(__file__).resolve().parents[1] / "softwarex-case-inventory.json"
    if sha256(inventory) != METADATA_SHA256:
        raise ValueError("pinned SoftwareX case metadata changed")
    cases = json.loads(inventory.read_text())
    rows = [dict(row, id=f"quest_qsvt/{row['case_id']}", group=None,
                 measurement_started=False, capability="legacy_executable_boundary_unavailable")
            for row in source_inventory(cases) if row["suite"] == "softwarex"]
    output.mkdir(parents=True, exist_ok=False)
    (output / "evidence").mkdir()
    for path, stored, _ in observed:
        shutil.copyfile(path, output / "evidence" / stored)
    shutil.copyfile(Path(__file__), output / "evidence" / "softwarex_unavailable.py")
    atomic_json(output / "manifest.json", dict(sources=SOURCE_REVISIONS, rows=rows,
                purpose="capability_diagnosis; no native benchmark process launched",
                source_evidence=[item[2] for item in observed], metadata_sha256=METADATA_SHA256,
                controller_sha256=sha256(Path(__file__))))
    results = [dict(id=row["id"], status="unsupported", phase="capability",
                    reason=REASON, measurement_started=False, raw_samples=[]) for row in rows]
    (output / "results.jsonl").write_text("".join(json.dumps(row) + "\n" for row in results))
    return publish_completion(output, rows, results)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--softwarex", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(write_unavailable(args.source, args.softwarex, args.output)))
