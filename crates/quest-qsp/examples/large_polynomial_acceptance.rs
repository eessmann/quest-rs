//! Exact-source regression and workspace-capacity lane, separate from paper timing.
use quest_numerics::{
	ExecutionPolicy, FftBackend, OperationLimits, OperationResources, ResourceReport,
};
use quest_qsp::{Complex64, ForwardNlftWorkspace, InverseNlftWorkspace};
use serde_json::{Value, json};
use std::{error::Error, fs, ops::Sub, path::Path};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn bits(input: &Value, key: &str) -> Result<Vec<Complex64>> {
	input
		.get(key)
		.and_then(Value::as_array)
		.ok_or("missing exact input")?
		.iter()
		.map(|v| {
			Ok(Complex64::new(
				f64::from_bits(
					v.get(0)
						.and_then(Value::as_u64)
						.ok_or("invalid real bits")?,
				),
				f64::from_bits(
					v.get(1)
						.and_then(Value::as_u64)
						.ok_or("invalid imaginary bits")?,
				),
			))
		})
		.collect()
}
fn array(file: &hdf5_metno::File, key: &str) -> Result<Vec<Complex64>> {
	Ok(file
		.dataset(key)?
		.read_raw::<f64>()?
		.as_chunks::<2>()
		.0
		.iter()
		.map(|&[real, imaginary]| Complex64::new(real, imaginary))
		.collect())
}
fn residual(a: &[Complex64], b: &[Complex64]) -> Result<f64> {
	if a.len() != b.len() {
		return Err("support mismatch".into());
	}
	a.iter().zip(b).try_fold(0.0_f64, |maximum, (a, b)| {
		let error = a.sub(*b).norm();
		if error.is_finite() {
			Ok(maximum.max(error))
		} else {
			Err("nonfinite residual".into())
		}
	})
}
fn report(r: ResourceReport) -> Value {
	json!({"live_bytes":r.live_bytes,"peak_bytes":r.peak_bytes,
    "live_buffer_bytes":r.live_buffer_bytes,"live_planner_bytes_estimate":r.live_planner_bytes_estimate,
    "work_units":r.work_units,"reservations":r.reservations,"requested_peak_bytes":r.requested_peak_bytes,
    "requested_work_units":r.requested_work_units,"last_rejection":r.last_rejection.map(|e|e.to_string())})
}
struct Input {
	a_star: Vec<Complex64>,
	b: Vec<Complex64>,
	gamma: Option<Vec<Complex64>>,
	tolerance: f64,
	degree: u64,
}
fn load(mode: &str, path: &Path) -> Result<Input> {
	let (a_star, b, gamma, tolerance, degree) = if mode == "inverse" {
		let input: Value = serde_json::from_slice(&fs::read(path)?)?;
		let d = input
			.get("degree")
			.and_then(Value::as_u64)
			.ok_or("missing degree")?;
		let tolerance = input
			.get("requested_tolerance_bits")
			.and_then(Value::as_u64)
			.map_or(if d <= 10_000 { 1e-12 } else { 1e-10 }, f64::from_bits);
		(
			bits(&input, "conjugate_complement_bits")?,
			bits(&input, "canonical_coefficients_bits")?,
			input
				.get("expected_reflections_bits")
				.map(|_| bits(&input, "expected_reflections_bits"))
				.transpose()?,
			tolerance,
			d,
		)
	} else if mode == "forward" {
		let file = hdf5_metno::File::open(path)?;
		let a = array(&file, "a")?;
		let b = array(&file, "b")?;
		let gamma = array(&file, "gamma")?;
		let d = u64::try_from(gamma.len().checked_sub(1).ok_or("empty gamma")?)?;
		(
			a.into_iter().rev().map(|v| v.conj()).collect(),
			b,
			Some(gamma),
			1e-10,
			d,
		)
	} else {
		return Err("mode must be inverse or forward".into());
	};
	Ok(Input {
		a_star,
		b,
		gamma,
		tolerance,
		degree,
	})
}
fn run(mode: &str, path: &Path) -> Result<Value> {
	let Input {
		a_star,
		b,
		gamma,
		tolerance,
		degree,
	} = load(mode, path)?;
	let resources = OperationResources::from_limits(OperationLimits::million_degree());
	let mut inverse = InverseNlftWorkspace::new(
		FftBackend::Simd,
		resources.clone(),
		ExecutionPolicy::Sequential,
	);
	let mut forward = ForwardNlftWorkspace::new(
		FftBackend::Simd,
		resources.clone(),
		ExecutionPolicy::Sequential,
	);
	let recovered = if mode == "inverse" {
		Some(inverse.inverse(&a_star, &b)?)
	} else {
		None
	};
	let reflection_error = if let (Some(actual), Some(expected)) = (&recovered, &gamma) {
		Some(residual(actual, expected)?)
	} else {
		None
	};
	let selected = if let Some(ref values) = recovered {
		values.as_slice()
	} else {
		gamma.as_deref().ok_or("missing gamma")?
	};
	let source_inverse_pass = mode != "inverse"
		|| (selected.len() == b.len()
			&& selected
				.iter()
				.all(|value| value.re.is_finite() && value.im.is_finite())
			&& reflection_error.is_none_or(|error| error <= tolerance));
	let pair = forward.forward(selected)?;
	let backward_error =
		residual(&pair.target, &b)?.max(residual(&pair.conjugate_complement, &a_star)?);
	let reconstructed_a_constant = *pair
		.conjugate_complement
		.first()
		.ok_or("empty complement")?;
	let live = resources.report();
	let plans_before = forward.cached_plan_count();
	let scratch_before = forward.cached_buffer_bytes()?;
	drop(pair);
	let reuse = if mode == "forward" {
		let repeated = forward.forward(selected)?;
		let error =
			residual(&repeated.target, &b)?.max(residual(&repeated.conjugate_complement, &a_star)?);
		let result = json!({"status":if error<=tolerance{"ok"}else{"accuracy_failure"},
            "coefficient_linf":error,"plans_before":plans_before,"plans_after":forward.cached_plan_count(),
            "scratch_bytes_before":scratch_before,"scratch_bytes_after":forward.cached_buffer_bytes()?,
            "resources_after_reuse":report(resources.report())});
		drop(repeated);
		Some(result)
	} else {
		None
	};
	drop(recovered);
	let cached = resources.report();
	drop(inverse);
	drop(forward);
	let released = resources.report();
	if released.live_bytes != 0 {
		return Err("workspace/result reservations leaked".into());
	}
	let source_a_constant = a_star.first().ok_or("empty source complement")?;
	// Upstream's large-fixture tolerance is for reflection recovery, not a/b.
	// Retain the earlier Rust harness's extra check as a distinct observation.
	let accepted = source_inverse_pass && backward_error <= 1e-10;
	Ok(
		json!({"status":if accepted{"ok"}else{"accuracy_failure"},"operation":mode,"degree":degree,
        "profile":"million-degree-inverse-forward-v1","backend":"rustfft-simd-sequential",
        "source_inverse_check":if mode=="inverse" {Some(if source_inverse_pass{"ok"}else{"accuracy_failure"})}else{None},
        "source_inverse_contract":if gamma.is_some(){"finite, size-preserving, max complex reflection error"}else{"finite, size-preserving"},
        "main_backward_tolerance":1e-10,"additional_reconstruction_status":if backward_error<=tolerance{"ok"}else{"accuracy_failure"},
        "tolerance":tolerance,"source_a_constant":[source_a_constant.re,source_a_constant.im],"reconstructed_a_constant":[reconstructed_a_constant.re,reconstructed_a_constant.im],"coefficient_backward_linf":backward_error,"source_reflection_linf":reflection_error,
        "live_result_and_workspaces":report(live),"cached_after_result_release":report(cached),"after_workspace_release":report(released),
        "reuse":reuse,"cached_forward_plans":plans_before,"cached_forward_scratch_bytes":scratch_before,"memory_kind":"modeled live allocations plus retained opaque planner estimate; not process RSS"}),
	)
}
fn main() -> Result<()> {
	let args: Vec<_> = std::env::args().collect();
	let [_, mode, input, output] = args.as_slice() else {
		return Err("usage: mode input output-json".into());
	};
	let value = match run(mode, Path::new(input)) {
		Ok(value) => value,
		Err(error) => json!({"status":"rejected","detail":error.to_string()}),
	};
	fs::write(output, serde_json::to_vec_pretty(&value)?)?;
	if value.get("status").and_then(Value::as_str) != Some("ok") {
		return Err(value.to_string().into());
	}
	Ok(())
}
