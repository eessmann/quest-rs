#!/usr/bin/env python3
"""Separate Rust inverse measurements on a sealed snapshot of native exact pairs."""
import argparse
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import campaign

CONTROLLERS = (Path(__file__).resolve(), Path(campaign.__file__).resolve())
LOADED_HASHES = {str(path): campaign.sha256(path) for path in CONTROLLERS}
PREFLIGHT = ("Existing Rust inverse followed by forward roundtrip coefficient-linf check; "
             "also checks expected source reflections when exported. This is stronger than "
             "the native Weiss fixture's finite size-preserving inverse preflight.")


def validate_pair(value, degree):
    if value.get("degree") != degree:
        raise ValueError("source degree differs from named fixture identity")
    keys = ["canonical_coefficients_bits", "conjugate_complement_bits"]
    if degree > 1000 or "expected_reflections_bits" in value:
        keys.append("expected_reflections_bits")
    hashes = {}
    for key in keys:
        values = value.get(key)
        if not isinstance(values, list) or len(values) != degree + 1:
            raise ValueError("exact source pair/reflection support is missing or inconsistent")
        digest = hashlib.sha256()
        for coefficient in values:
            if (not isinstance(coefficient, list) or len(coefficient) != 2
                    or any(type(bits) is not int or not 0 <= bits < 2**64 for bits in coefficient)):
                raise ValueError("invalid binary64 coefficient bits")
            encoded = struct.pack("<QQ", *coefficient)
            if not all(math.isfinite(number) for number in struct.unpack("<dd", encoded)):
                raise ValueError("nonfinite exact source coefficient")
            digest.update(encoded)
        hashes[key] = digest.hexdigest()
    if degree > 1000:
        bits = value.get("requested_tolerance_bits")
        if type(bits) is not int or not 0 <= bits < 2**64:
            raise ValueError("source forward fixture tolerance is missing")
        tolerance = struct.unpack("<d", struct.pack("<Q", bits))[0]
        if not math.isfinite(tolerance) or tolerance <= 0:
            raise ValueError("invalid source fixture tolerance")
    else:
        tolerance = 1e-12
        if ("requested_tolerance_bits" in value
                and value["requested_tolerance_bits"] != struct.unpack("<Q", struct.pack("<d", tolerance))[0]):
            raise ValueError("low-order fixture cannot override the recorded source tolerance")
    return tolerance, hashes


def prepare_inputs(exports, output):
    output.mkdir(parents=True, exist_ok=False)
    (output / "fixtures").mkdir()
    rows = []
    for degree in campaign.INVERSE_ORDERS:
        row = dict(case_id=f"named_inverse/order{degree}", degree=degree, scope="inverse_only",
                   protocol="criterion_10", sample_count=10, warmup_seconds=0.1,
                   measurement_seconds=0.2, timeout_seconds=14400, backend="rustfft_scalar", workers=1,
                   preflight_contract=PREFLIGHT, input_status="not_exported", input_sha256=None,
                   requested_accuracy=None, source_boundary_equivalent=False,
                   comparison_contract="Original/current Rust inverse on identical source-exported scattering pair; no comparison to native aggregate timings")
        path = exports / f"order{degree}.json"
        if path.is_file():
            payload = path.read_bytes()
            digest = hashlib.sha256(payload).hexdigest()
            if campaign.sha256(path) != digest:
                raise ValueError("source export changed while snapshotting")
            try:
                tolerance, hashes = validate_pair(json.loads(payload), degree)
            except (ValueError, TypeError, KeyError) as error:
                row.update(input_status="invalid_export", reason=str(error), rejected_export_sha256=digest)
                (output / "fixtures" / f"rejected-order{degree}.json").write_bytes(payload)
            else:
                (output / "fixtures" / path.name).write_bytes(payload)
                row.update(input_status="available", input_sha256=digest,
                           input_file=f"fixtures/{path.name}", requested_accuracy=tolerance,
                           coefficient_sha256=hashes)
        else:
            row["reason"] = "No exact source export exists; fixture preparation was not completed or not reached"
        rows.append(row)
    manifest = dict(schema_version=1, sources=campaign.SOURCE_REVISIONS, rows=rows,
                    producer="unchanged source fixture helpers called by the external exact-bit exporter")
    campaign.atomic_json(output / "manifest.json", manifest)
    return manifest


def load_inputs(bundle):
    manifest = json.loads((bundle / "manifest.json").read_text())
    rows = manifest["rows"]
    if (manifest["sources"] != campaign.SOURCE_REVISIONS
            or [row["degree"] for row in rows] != list(campaign.INVERSE_ORDERS)
            or any(row["case_id"] != f"named_inverse/order{row['degree']}" or row["scope"] != "inverse_only" for row in rows)):
        raise ValueError("named inverse snapshot identities changed")
    for row in rows:
        if row["input_status"] == "available":
            path = (bundle / row["input_file"]).resolve()
            if not path.is_relative_to(bundle.resolve()) or campaign.sha256(path) != row["input_sha256"]:
                raise ValueError("exact source fixture changed after snapshot")
            tolerance, hashes = validate_pair(json.loads(path.read_text()), row["degree"])
            if tolerance != row["requested_accuracy"] or hashes != row["coefficient_sha256"]:
                raise ValueError("exact source fixture contract changed after snapshot")
        elif row["input_status"] not in {"not_exported", "invalid_export"}:
            raise ValueError("unknown fixture availability status")
    if "export_provenance_sha256" in manifest:
        if (campaign.sha256(bundle / "export-provenance.json") != manifest["export_provenance_sha256"]
                or campaign.sha256(bundle / "export-receipt.json") != manifest["export_receipt_sha256"]):
            raise ValueError("sealed export provenance changed")
        provenance = json.loads((bundle / "export-provenance.json").read_text())
        receipt = json.loads((bundle / "export-receipt.json").read_text())
        observed = {f"named-fixtures/order{row['degree']}.json": row["input_sha256"]
                    for row in rows if row["input_status"] == "available"}
        observed.update({"out/build/nix-release/export_named": provenance["exporter_sha256"],
                         "export-run.log": provenance["log_sha256"]})
        verify_export_receipt(receipt, observed)
    return manifest


def verify_export_receipt(receipt, observed):
    if receipt.get("source_revision") != campaign.SOURCE_REVISIONS["quest_qsvt"] or receipt.get("export_exit_code") != 0:
        raise ValueError("completed source export receipt is missing or not pinned")
    if any(receipt.get("artifacts", {}).get(name) != digest for name, digest in observed.items()):
        raise ValueError("exact export artifact does not match the completed source receipt")


def reported_phase(phase, log):
    return "measurement" if phase == "warmup" and "Collecting" in log else phase


def verify_controllers():
    if any(campaign.sha256(Path(name)) != digest for name, digest in LOADED_HASHES.items()):
        raise RuntimeError("executed named inverse controller/helper changed")


def inverse_samples(root):
    samples = list(root.rglob("new/sample.json"))
    return samples if len(samples) == 1 and samples[0].parent.parent.name == "inverse_only" else []


def bounded_output(command, workspace):
    import tempfile
    with tempfile.TemporaryFile() as stream:
        process = subprocess.Popen(command, cwd=workspace, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
        code, stopped = campaign.wait_bounded(process, 30)
        stream.seek(0)
        output = stream.read().decode(errors="replace")
    if code or stopped:
        raise RuntimeError("bounded toolchain probe failed: " + output)
    return output


def run(bundle, workspace, target, output, implementation, reference_lane):
    memory = campaign.effective_memory_limits()
    with open("/tmp/quest-quality-build.lock", "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        inputs = load_inputs(bundle)
    if "export_provenance_sha256" not in inputs:
        raise ValueError("measurement requires a receipt-bound exact source snapshot")
    input_manifest_hash = campaign.sha256(bundle / "manifest.json")
    export_provenance = json.loads((bundle / "export-provenance.json").read_text())
    if export_provenance["source"] != campaign.SOURCE_REVISIONS["quest_qsvt"]:
        raise ValueError("source exporter provenance differs from the pinned source")
    bundle_files = {name: campaign.sha256(bundle / name) for name in
                    ("manifest.json", "export-provenance.json", "export-receipt.json", "export.log", "exporter-source.cpp", "exporter-CMakeLists.txt")}
    for name, key in (("export.log", "log_sha256"), ("exporter-source.cpp", "adapter_sha256"), ("exporter-CMakeLists.txt", "cmake_sha256")):
        if bundle_files[name] != export_provenance[key]:
            raise ValueError("source exporter evidence changed")
    reference = json.loads((reference_lane / "manifest.json").read_text())
    expected_role = "original_0f95954" if implementation == "original" else "current"
    if {row["implementation"] for row in reference["rows"]} != {expected_role}:
        raise ValueError("original/current source reference role differs from the requested lane")
    identity = campaign.source_identity(workspace)
    if identity != json.loads((reference_lane / "source-identity.json").read_text()):
        raise ValueError("Rust source differs from the previously verified original/current tree")
    output.mkdir(parents=True, exist_ok=False)
    (output / "controllers").mkdir()
    verify_controllers()
    for path in CONTROLLERS:
        shutil.copyfile(path, output / "controllers" / path.name)
    shutil.copyfile(bundle / "manifest.json", output / "input-manifest.json")
    shutil.copyfile(bundle / "export-provenance.json", output / "export-provenance.json")
    rows = [dict(row, id=f"{implementation}/named_inverse/order{row['degree']}/inverse_only",
                 implementation=implementation) for row in inputs["rows"]]
    campaign.atomic_json(output / "manifest.json", dict(schema_version=1, rows=rows,
                         memory_limits=memory, input_manifest_sha256=input_manifest_hash,
                         input_bundle=bundle.name, input_bundle_evidence_sha256=bundle_files,
                         source_reference_lane=reference_lane.name,
                         source_reference_identity_sha256=campaign.sha256(reference_lane / "source-identity.json"),
                         controller_sha256={path.name: LOADED_HASHES[str(path)] for path in CONTROLLERS}))
    campaign.atomic_json(output / "source-identity.json", identity)
    with open("/tmp/quest-quality-build.lock", "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        tools = {"rustc": bounded_output(["rustc", "-vV"], workspace),
                 "nextest": bounded_output(["cargo", "nextest", "--version"], workspace),
                 "environment": {key: os.environ.get(key) for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "QUEST_ROOT", "RAYON_NUM_THREADS", "OMP_NUM_THREADS")}}
        campaign.atomic_json(output / "toolchain.json", tools)
    results = []
    for index, row in enumerate(rows):
        verify_controllers()
        campaign.verify_source_identity(workspace, identity, output, row["id"])
        if campaign.sha256(bundle / "manifest.json") != input_manifest_hash:
            raise ValueError("sealed source input manifest changed")
        if row["input_status"] != "available":
            result = dict(id=row["id"], status="preparation_failure", phase="source_input_unavailable",
                          reason=row["reason"], measurement_started=False, raw_samples=[])
        else:
            stem = f"{index:04d}"
            log_path, rss, phase_path = [output / f"{stem}.{suffix}" for suffix in ("log", "rss", "phase")]
            fixture = bundle / row["input_file"]
            env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="4",
                       QUEST_BENCH_INPUT=str(fixture), QUEST_BENCH_PHASE=str(phase_path),
                       QUEST_BENCH_SCOPE="inverse_only", CRITERION_HOME=str(output / stem / "criterion"))
            command = ["cargo", "nextest", "bench", "-p", "quest-qsp", "--bench", "replication",
                       "--features", "benchmark-support", "--offline", "--locked", "-P", "replication", "-E", "test(=inverse_only)"]
            with open("/tmp/quest-quality-build.lock", "a") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX)
                verify_controllers()
                campaign.verify_source_identity(workspace, identity, output, row["id"])
                if campaign.sha256(fixture) != row["input_sha256"]:
                    raise ValueError("exact source pair changed before measurement")
                started = time.monotonic()
                with log_path.open("w") as log:
                    process = subprocess.Popen(campaign.time_command(command, rss), cwd=workspace, env=env,
                                               stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                    returncode, stopped = campaign.wait_bounded(process, row["timeout_seconds"])
                campaign.verify_source_identity(workspace, identity, output, row["id"])
                verify_controllers()
                if campaign.sha256(fixture) != row["input_sha256"]:
                    raise ValueError("exact source pair changed during measurement")
            log = log_path.read_text(errors="replace")
            phase = reported_phase(phase_path.read_text() if phase_path.exists() else "build_or_discovery", log)
            status = stopped or campaign.classify_run(returncode, log, phase)
            accuracy = re.search(r"QUEST_BENCH_PREFLIGHT requested=(\S+) achieved=(\S+)", log)
            result = dict(id=row["id"], status=status, phase=phase, returncode=returncode,
                          elapsed_seconds=time.monotonic() - started, log=log_path.name,
                          log_sha256=campaign.sha256(log_path), peak_rss_kib=campaign.read_peak_rss(rss) if stopped is None else None,
                          peak_rss_scope=campaign.RSS_SCOPE, peak_rss_unit="KiB",
                          peak_rss_log=rss.name if rss.exists() else None,
                          peak_rss_log_sha256=campaign.sha256(rss) if rss.exists() else None,
                          requested_accuracy=row["requested_accuracy"],
                          achieved_accuracy=float(accuracy[2]) if accuracy else None)
            if status == "ok":
                samples = inverse_samples(output / stem)
                if not samples:
                    result.update(status="native_error", reason="expected exactly one inverse_only Criterion sample artifact")
                else:
                    result["raw_samples"] = [dict(path=str(samples[0].relative_to(output)), sha256=campaign.sha256(samples[0]))]
            if phase == "build_or_discovery":
                campaign.atomic_json(output / "infrastructure-failure.json", result)
                raise RuntimeError("build/discovery failed before the named inverse workload; lane incomplete")
        results.append(result)
        (output / "results.jsonl").write_text("".join(json.dumps(item, allow_nan=False) + "\n" for item in results))
        print(f"[{index + 1}/17] {row['id']}: {result['status']}", flush=True)
        if result["status"] == "interrupted":
            raise KeyboardInterrupt
    verify_controllers()
    campaign.verify_source_identity(workspace, identity, output, "completion")
    if any(campaign.sha256(bundle / name) != digest for name, digest in bundle_files.items()):
        raise ValueError("sealed source input manifest or exporter evidence changed before completion")
    with open("/tmp/quest-quality-build.lock", "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        load_inputs(bundle)
        verify_controllers()
        campaign.verify_source_identity(workspace, identity, output, "completion")
        if any(campaign.sha256(bundle / name) != digest for name, digest in bundle_files.items()):
            raise ValueError("sealed source evidence changed while waiting for completion lock")
        return campaign.publish_completion(output, rows, results)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    prepare = commands.add_parser("prepare")
    for name in ("exports", "output", "exporter", "export_log", "export_receipt"):
        prepare.add_argument("--" + name.replace("_", "-"), required=True, type=Path)
    runner = commands.add_parser("run")
    for name in ("bundle", "workspace", "target", "output", "reference_lane"):
        runner.add_argument("--" + name.replace("_", "-"), required=True, type=Path)
    runner.add_argument("--implementation", choices=("original", "current"), required=True)
    args = parser.parse_args()
    if args.action == "prepare":
        with open("/tmp/quest-quality-build.lock", "a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            receipt = json.loads(args.export_receipt.read_text())
            observed = {"out/build/nix-release/export_named": campaign.sha256(args.exporter),
                        "export-run.log": campaign.sha256(args.export_log)}
            verify_export_receipt(receipt, observed)
            manifest = prepare_inputs(args.exports.resolve(), args.output.resolve())
            observed.update({f"named-fixtures/order{row['degree']}.json": row["input_sha256"]
                             for row in manifest["rows"] if row["input_status"] == "available"})
            verify_export_receipt(receipt, observed)
            shutil.copyfile(args.export_log, args.output / "export.log")
            shutil.copyfile(args.export_receipt, args.output / "export-receipt.json")
            shutil.copyfile(Path(__file__).with_name("export_named.cpp"), args.output / "exporter-source.cpp")
            shutil.copyfile(Path(__file__).with_name("CMakeLists.txt"), args.output / "exporter-CMakeLists.txt")
            campaign.atomic_json(args.output / "export-provenance.json", dict(exporter_sha256=observed["out/build/nix-release/export_named"],
                                 log_sha256=observed["export-run.log"], source=campaign.SOURCE_REVISIONS["quest_qsvt"],
                                 adapter_sha256=campaign.sha256(args.output / "exporter-source.cpp"),
                                 cmake_sha256=campaign.sha256(args.output / "exporter-CMakeLists.txt")))
            manifest.update(export_provenance_sha256=campaign.sha256(args.output / "export-provenance.json"),
                            export_receipt_sha256=campaign.sha256(args.output / "export-receipt.json"))
            campaign.atomic_json(args.output / "manifest.json", manifest)
        print(json.dumps({"rows": len(manifest["rows"]), "available": sum(row["input_status"] == "available" for row in manifest["rows"])}))
    else:
        print(json.dumps(run(args.bundle.resolve(), args.workspace.resolve(), args.target.resolve(), args.output.resolve(),
                             args.implementation, args.reference_lane.resolve())))


if __name__ == "__main__":
    main()
