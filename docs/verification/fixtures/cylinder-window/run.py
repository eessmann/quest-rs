#!/usr/bin/env python3
"""Linux bounded cylinder window execution with source/binary identities and failure evidence."""
import argparse
import hashlib
import json
import math
import os
import pathlib
import resource
import signal
import subprocess
import tempfile
import time

STDOUT_LIMIT = 64 * 1024**2
STDERR_LIMIT = 4 * 1024**2
STDERR_TAIL = 64 * 1024
JSON_DEPTH_LIMIT = 16

SOURCES = ("crates/quest-cfd/examples/cylinder_window.rs", "crates/quest-cfd/src/cylinder.rs",
           "crates/quest-cfd/src/observations.rs", "crates/quest-cfd/src/simplex.rs",
           "crates/quest-cfd/src/bdm.rs", "crates/quest-cfd/src/cases.rs",
           "crates/quest-cfd/src/lib.rs", "crates/quest-cfd/Cargo.toml", "Cargo.toml",
           "crates/quest-cfd/cases/shedding2d.json", "Cargo.lock")

DEFAULTS = dict(reynolds=100, angular_sectors=4, radial_layers=1, dt=0.0001,
                steps=80000, stride=100, observation_start=4.0, observation_end=8.0,
                short_window=False, max_steps=1000000, max_work=1000000000000,
                max_bytes=67108864, max_trace_bytes=65536, max_residual=1e-6,
                minimum_cycles=2, minimum_samples_per_cycle=8,
                maximum_period_variation=0.1, lift_amplitude_floor=1e-10)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def shape(value, fields):
    require(isinstance(value, dict) and set(value) == set(fields.split()),
            "missing, unknown or invalid object fields")


def integer(value, minimum=0, maximum=2**64-1):
    require(type(value) is int and minimum <= value <= maximum, "invalid integer/count")
    return value


def number(value, minimum=-math.inf, maximum=math.inf):
    require(type(value) in (int, float) and math.isfinite(value) and minimum <= value <= maximum,
            "nonfinite/out-of-range number")
    return value


def near(actual, expected):
    number(actual)
    require(math.isclose(actual, expected, rel_tol=1e-9, abs_tol=1e-12),
            "inconsistent numerical receipt")


class ParameterParser(argparse.ArgumentParser):
    def error(self, message):
        raise ValueError(message)


def requested_parameters(arguments):
    parser = ParameterParser(add_help=False, allow_abbrev=False)
    for key, default in DEFAULTS.items():
        flag = "--" + key.replace("_", "-")
        if type(default) is bool:
            parser.add_argument(flag, action="store_true")
        else:
            parser.add_argument(flag, type=type(default), default=default)
    return vars(parser.parse_args(arguments))


def validate_completed(r, arguments, cap, root):
    """Admit this known example's complete semantic receipt, without attesting its build.

    Counts are bounded before trace traversal. Progress from stderr is intentionally
    separate: it precedes appending an observation and may lag the final JSON.
    """
    shape(r, "schema status parameters completed convergence_certified quantum_execution elapsed_seconds process progress result error")
    require(r["schema"] == "quest-cfd-cylinder-window-v1" and r["status"] == "completed"
            and r["completed"] is True and r["convergence_certified"] is False
            and r["quantum_execution"] is False and r["error"] is None,
            "wrong schema/status or unsupported certificate")
    number(r["elapsed_seconds"], 0)
    p = r["parameters"]
    shape(p, " ".join(DEFAULTS))
    require(p == requested_parameters(arguments), "requested/native parameter mismatch")
    for key, default in DEFAULTS.items():
        if type(default) is int:
            integer(p[key], 1)
        elif type(default) is float:
            number(p[key], 0)
        else:
            require(type(p[key]) is bool, "invalid boolean parameter")
    dt, steps, stride = p["dt"], p["steps"], p["stride"]
    require(dt > 0 and steps <= min(p["max_steps"], 1000000) and dt * steps <= 8
            and 4 <= p["angular_sectors"] <= 64 and 1 <= p["radial_layers"] <= 8
            and p["reynolds"] == 100 and p["max_residual"] > 0
            and p["minimum_cycles"] >= 2 and p["minimum_samples_per_cycle"] >= 4
            and p["maximum_period_variation"] < 1 and p["lift_amplitude_floor"] > 0,
            "invalid admitted parameters")
    start, end = p["observation_start"], p["observation_end"]
    require(0 <= start < end <= dt * steps + 1e-12 * abs(dt * steps), "invalid window")
    first, last = round(start / dt), round(end / dt)
    require(abs(first * dt - start) <= 1e-12 * max(dt, abs(start)) and
            abs(last * dt - end) <= 1e-12 * max(dt, abs(end)), "unaligned window")
    frozen = dt * steps == 8 and [start, end] == [4, 8]
    require(frozen or p["short_window"], "changed window lacks explicit override")
    count = (last - first) // stride + 1 + int((last - first) % stride != 0)
    integer(count, 2, 1000000)
    metrics = r["process"]
    shape(metrics, "address_space_cap_bytes rss_high_water_bytes address_space_high_water_bytes")
    require(integer(metrics["address_space_cap_bytes"], 1) == cap, "process cap mismatch")
    integer(metrics["rss_high_water_bytes"], 1, cap)
    integer(metrics["address_space_high_water_bytes"], metrics["rss_high_water_bytes"], cap)
    progress = r["progress"]
    shape(progress, "completed_steps time observations_completed")
    require(integer(progress["completed_steps"], 1, 1000000) == steps and
            integer(progress["observations_completed"], 2, 1000000) == count,
            "incomplete final progress")
    near(progress["time"], dt * steps)
    result = r["result"]
    shape(result, "admission manifest independent_dimension local_velocity_dimension constraint_rank cylinder_segments maximum_geometry_deviation pressure_convention force_convention maximum_momentum_residual maximum_continuity_residual maximum_boundary_residual trace statistics")
    manifest = json.loads((root / "crates/quest-cfd/cases/shedding2d.json").read_text())
    manifest.setdefault("measurement_minimum_cycles", None)
    require(result["manifest"] == manifest, "wrong frozen manifest")
    local = (p["angular_sectors"] + 4) * 12 * p["radial_layers"]
    require(local <= 768, "reference mesh capacity exceeded")
    actual_local = integer(result["local_velocity_dimension"], 1, local)
    require(integer(result["independent_dimension"], 1, actual_local) +
            integer(result["constraint_rank"], 0, actual_local) == actual_local,
            "inconsistent complete physical dimensions")
    integer(result["cylinder_segments"], 4, p["angular_sectors"] + 4)
    number(result["maximum_geometry_deviation"], 0)
    for key in ("pressure_convention", "force_convention"):
        require(isinstance(result[key], str) and 0 < len(result[key]) < 1024, "missing physical convention")
    for key in ("maximum_momentum_residual", "maximum_continuity_residual", "maximum_boundary_residual"):
        number(result[key], 0, p["max_residual"])
    a = result["admission"]
    shape(a, "frozen_window observation_steps sample_count trace_bytes aggregate_work construction_work integration_work observation_work analysis_work local_velocity_upper_bound peak_managed_upper_bound_bytes")
    require(type(a["frozen_window"]) is bool and a["frozen_window"] == frozen
            and a["observation_steps"] == [first, last], "wrong admitted window")
    require(integer(a["sample_count"], 2, 1000000) == count, "wrong admitted count")
    trace_bytes = integer(a["trace_bytes"], count * 32, p["max_trace_bytes"])
    n2, n3 = local**2, local**3
    drift = 128 * n2 + 8192 * local
    costs = dict(construction_work=256*n3,
                 integration_work=steps*(4*drift+32*n2),
                 observation_work=count*(64*n3+8*drift), analysis_work=count*128)
    for key, expected in costs.items():
        require(integer(a[key]) == expected, "wrong aggregate work component")
    require(integer(a["aggregate_work"], 1, p["max_work"]) == sum(costs.values())
            and integer(a["local_velocity_upper_bound"], 1) == local,
            "incorrect complete work admission")
    require(integer(a["peak_managed_upper_bound_bytes"], 1, p["max_bytes"]) ==
            256*n2+65536*local+4*1024**2+trace_bytes, "wrong managed peak admission")
    trace = result["trace"]
    require(isinstance(trace, list) and len(trace) == count, "incomplete force trace")
    for i, sample in enumerate(trace):
        shape(sample, "time drag lift pressure_difference")
        expected_step = min(first + i*stride, last)
        near(sample["time"], dt * expected_step)
        require(i == 0 or sample["time"] > trace[i-1]["time"], "unordered force trace")
        for key in ("drag", "lift", "pressure_difference"):
            number(sample[key], -1e150, 1e150)
    s = result["statistics"]
    shape(s, "window sample_count mean_drag mean_lift lift_rms lift_standard_deviation mean_pressure_difference frequency frequency_unavailable_reason policy periodicity_certified")
    require(s["periodicity_certified"] is False and s["window"] ==
            [trace[0]["time"], trace[-1]["time"]] and integer(s["sample_count"], 2) == count,
            "wrong statistics window/certificate")
    policy = dict(minimum_cycles=p["minimum_cycles"], minimum_samples_per_cycle=p["minimum_samples_per_cycle"],
                  maximum_relative_period_variation=p["maximum_period_variation"],
                  lift_amplitude_floor=p["lift_amplitude_floor"], max_samples=count, max_work=count*128)
    require(s["policy"] == policy, "statistics policy mismatch")
    duration = trace[-1]["time"] - trace[0]["time"]
    moments = dict(drag=0.0, lift=0.0, pressure_difference=0.0)
    lift2 = 0.0
    for left, right in zip(trace, trace[1:]):
        weight = (right["time"]-left["time"])/(2*duration)
        for key in moments:
            moments[key] += weight*(left[key]+right[key])
        lift2 += weight*(left["lift"]**2+right["lift"]**2)
    for key in moments:
        near(s["mean_"+key], moments[key])
    near(number(s["lift_rms"], 0), math.sqrt(lift2))
    mean = moments["lift"]
    variance, crossings, max_gap = 0.0, [], 0.0
    amplitude = max(abs(v["lift"]-mean) for v in trace)
    for left, right in zip(trace, trace[1:]):
        gap = right["time"]-left["time"]
        la, lb = left["lift"]-mean, right["lift"]-mean
        variance += gap/(2*duration)*(la*la+lb*lb)
        max_gap = max(max_gap, gap)
        if la <= 0 < lb:
            crossings.append(left["time"]-la/(lb-la)*gap)
    # Cancellation in a nearly constant lift is sensitive to FMA accumulation.
    require(abs(number(s["lift_standard_deviation"], 0)-math.sqrt(variance)) <=
            1e-9*max(1, abs(mean)), "incorrect lift variance")
    reason, expected_frequency = None, None
    if amplitude < p["lift_amplitude_floor"]:
        reason = "lift variation below explicit amplitude floor"
    elif len(crossings)-1 < p["minimum_cycles"]:
        reason = "insufficient complete upward-crossing periods"
    else:
        periods = [b-a for a, b in zip(crossings, crossings[1:])]
        period = (crossings[-1]-crossings[0])/len(periods)
        variation = max(abs(min(periods)-period), abs(max(periods)-period))/period
        resolution = min(periods)/max_gap
        if variation > p["maximum_period_variation"]:
            reason = "crossing periods fail consistency policy"
        elif resolution < p["minimum_samples_per_cycle"]:
            reason = "crossings are insufficiently time resolved"
        else:
            expected_frequency = dict(frequency=1/period, strouhal=0.1/period,
                                      completed_periods=len(periods),
                                      maximum_relative_period_variation=variation,
                                      samples_per_shortest_period=resolution)
    require(s["frequency_unavailable_reason"] == reason, "incorrect unavailable-frequency reason")
    if expected_frequency is None:
        require(s["frequency"] is None, "unsupported measured frequency")
    else:
        shape(s["frequency"], " ".join(expected_frequency))
        integer(s["frequency"]["completed_periods"], p["minimum_cycles"], count)
        for key, expected in expected_frequency.items():
            near(s["frequency"][key], expected)


def identities(root):
    out = {}
    for name in SOURCES:
        with (root / name).open("rb") as source:
            out[name] = hashlib.file_digest(source, "sha256").hexdigest()
    return out


def strict_json(text):
    """Reject duplicates, exponent overflow and excessive depth before json decoding."""
    depth, quoted, escaped = 0, False, False
    for character in text:
        if quoted:
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == '"':
                quoted = False
        elif character == '"':
            quoted = True
        elif character in "[{":
            depth += 1
            require(depth <= JSON_DEPTH_LIMIT, "JSON nesting exceeds receipt limit")
        elif character in "]}":
            depth -= 1

    def pairs(items):
        output = {}
        for key, value in items:
            require(key not in output, "duplicate JSON object key")
            output[key] = value
        return output

    def finite_float(word):
        return number(float(word))

    def invalid_constant(word):
        raise ValueError("nonfinite JSON constant: " + word)

    return json.loads(text, object_pairs_hook=pairs, parse_float=finite_float,
                      parse_constant=invalid_constant)


def bounded_execution(binary, parameters, cap, timeout):
    """Spool bounded process output, keeping only a bounded stderr tail in memory."""
    def limits():
        resource.setrlimit(resource.RLIMIT_AS, (cap, cap))
        # Backstop even if an output producer outruns the live size observer.
        resource.setrlimit(resource.RLIMIT_FSIZE, (STDOUT_LIMIT, STDOUT_LIMIT))

    start = time.monotonic()
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        with subprocess.Popen([str(binary), *parameters], stdout=out, stderr=err,
                              start_new_session=True, preexec_fn=limits) as process:
            status = None
            while process.poll() is None:
                if os.fstat(out.fileno()).st_size > STDOUT_LIMIT or os.fstat(err.fileno()).st_size > STDERR_LIMIT:
                    status = "output_limit"
                    break
                remaining = timeout - (time.monotonic()-start)
                if remaining <= 0:
                    status = "timeout"
                    break
                try:
                    process.wait(timeout=min(0.05, remaining))
                except subprocess.TimeoutExpired:
                    pass
            if status:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait()
            else:
                status = "completed" if process.returncode == 0 else "rejected"
            sizes = [os.fstat(stream.fileno()).st_size for stream in (out, err)]
            if sizes[0] >= STDOUT_LIMIT or sizes[1] > STDERR_LIMIT or process.returncode == -signal.SIGXFSZ:
                if status != "timeout":
                    status = "output_limit"
            out.seek(0)
            try:
                stdout = out.read(STDOUT_LIMIT).decode("utf-8", errors="strict") if sizes[0] < STDOUT_LIMIT else ""
            except UnicodeError:
                stdout = ""
                if status == "completed":
                    status = "invalid_receipt"
            err.seek(max(0, sizes[1]-STDERR_TAIL))
            stderr = err.read(STDERR_TAIL).decode("utf-8", errors="replace")
            return dict(status=status, exit_code=process.returncode,
                        elapsed_seconds=time.monotonic()-start), stdout, stderr, sizes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--process-as-mib", type=int, default=512)
    parser.add_argument("parameters", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if not 1 <= args.timeout <= 120 or args.process_as_mib <= 0:
        parser.error("positive process cap and timeout 1..120 required")
    if args.output.exists():
        parser.error("output receipt must be new")
    root = pathlib.Path(__file__).resolve().parents[4]
    binary = args.binary.resolve(strict=True)
    with binary.open("rb") as stream:
        binary_identity = hashlib.file_digest(stream, "sha256").hexdigest()
    before = identities(root)
    parameters = args.parameters[1:] if args.parameters[:1] == ["--"] else args.parameters
    cap = args.process_as_mib * 1024**2
    receipt = dict(schema="quest-cfd-capped-cylinder-window-v1", binary_sha256=binary_identity,
                   source_sha256=before, build_profile="caller-selected prebuilt binary; recorded build command required",
                   parameters=parameters, timeout_seconds=args.timeout, process_address_space_cap_bytes=cap,
                   cap_kind="Linux RLIMIT_AS hard/soft; not RSS/node enforcement", completed=False,
                   benchmark_converged=False, source_identity_claim="source and prebuilt binary hashed independently")
    execution, stdout, stderr, sizes = bounded_execution(binary, parameters, cap, args.timeout)
    receipt.update(execution)
    receipt["output_limits"] = dict(stdout_bytes=STDOUT_LIMIT, stderr_bytes=STDERR_LIMIT,
                                   stderr_retained_tail_bytes=STDERR_TAIL, json_depth=JSON_DEPTH_LIMIT)
    receipt["output_bytes"] = dict(stdout=sizes[0], stderr=sizes[1])
    receipt["stderr_tail_truncated"] = sizes[1] > STDERR_TAIL
    last_progress = None
    for line in stderr.splitlines():
        try:
            value = strict_json(line)
            if isinstance(value, dict) and "completed_steps" in value:
                last_progress = value
        except (ValueError, RecursionError):
            pass
    receipt["last_reported_progress"] = last_progress
    # Scalar progress only: retain its last observation, not an unbounded log/trajectory.
    try:
        result = strict_json(stdout)
        receipt["native_receipt"] = result
        if receipt["status"] == "completed":
            validate_completed(result, parameters, cap, root)
            receipt["completed"] = True
    except (ValueError, KeyError, TypeError, OverflowError, RecursionError) as error:
        receipt["receipt_error"] = str(error)
        if receipt["status"] == "completed":
            receipt["status"] = "invalid_receipt"
    receipt["sources_changed_during_run"] = identities(root) != before
    if receipt["sources_changed_during_run"]:
        receipt["completed"] = False
        if receipt["status"] == "completed":
            receipt["status"] = "source_changed"
    # Normal errors contain no path; preserve only non-progress diagnostics and sanitize paths.
    diagnostics = "\n".join(line for line in stderr.splitlines() if not line.startswith("{"))
    receipt["diagnostics"] = diagnostics.replace(str(root), "<workspace>").replace(str(binary), "<example-binary>")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as stream:
        json.dump(receipt, stream, indent=2, allow_nan=False)
        stream.write("\n")
    print(json.dumps({key: receipt[key] for key in ("status", "completed", "elapsed_seconds", "last_reported_progress")}))
    return 0 if receipt["completed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
