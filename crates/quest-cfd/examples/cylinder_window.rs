//! Bounded continuous classical DFG2D2 force-window reference, never convergence evidence.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::redundant_pub_crate,
	reason = "Checked counts, bounded mesh and admitted step indices guard displayed arithmetic"
)]
use clap::Parser;
use quest_cfd::{
	CfdError,
	cases::{CaseManifest, manifest},
	cylinder,
	observations::{ForceSample, SheddingPolicy, SheddingStatistics, analyze_shedding},
};

#[derive(Clone, Debug, Parser, serde::Serialize)]
pub(crate) struct Options {
	#[arg(long, default_value_t = 100)]
	reynolds: u32,
	#[arg(long, default_value_t = 4)]
	angular_sectors: u32,
	#[arg(long, default_value_t = 1)]
	radial_layers: u32,
	#[arg(long, default_value_t = 0.0001)]
	pub(crate) dt: f64,
	#[arg(long, default_value_t = 80_000)]
	pub(crate) steps: u32,
	#[arg(long, default_value_t = 100)]
	stride: u32,
	#[arg(long, default_value_t = 4.0)]
	observation_start: f64,
	#[arg(long, default_value_t = 8.0)]
	observation_end: f64,
	/// Explicit contained diagnostic-window override; never frozen benchmark acceptance.
	#[arg(long)]
	short_window: bool,
	#[arg(long, default_value_t = 1_000_000)]
	max_steps: u32,
	#[arg(long, default_value_t = 1_000_000_000_000_u64)]
	pub(crate) max_work: u64,
	#[arg(long, default_value_t = 67_108_864)]
	max_bytes: usize,
	#[arg(long, default_value_t = 65_536)]
	pub(crate) max_trace_bytes: usize,
	#[arg(long, default_value_t = 1e-6)]
	max_residual: f64,
	#[arg(long, default_value_t = 2)]
	minimum_cycles: u32,
	#[arg(long, default_value_t = 8)]
	minimum_samples_per_cycle: u32,
	#[arg(long, default_value_t = 0.1)]
	maximum_period_variation: f64,
	#[arg(long, default_value_t = 1e-10)]
	lift_amplitude_floor: f64,
}
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct Admission {
	pub(crate) frozen_window: bool,
	pub(crate) observation_steps: [u32; 2],
	pub(crate) sample_count: usize,
	pub(crate) trace_bytes: usize,
	pub(crate) aggregate_work: u64,
	construction_work: u64,
	integration_work: u64,
	observation_work: u64,
	analysis_work: u64,
	local_velocity_upper_bound: usize,
	peak_managed_upper_bound_bytes: usize,
}
const fn invalid(message: &'static str) -> CfdError {
	CfdError::InvalidInput(message)
}
fn product(a: u64, b: u64) -> Result<u64, CfdError> {
	a.checked_mul(b)
		.ok_or_else(|| invalid("window work overflow"))
}
fn sum(a: u64, b: u64) -> Result<u64, CfdError> {
	a.checked_add(b)
		.ok_or_else(|| invalid("window work overflow"))
}
fn step_at(time: f64, dt: f64, steps: u32) -> Result<u32, CfdError> {
	let (mut lo, mut hi) = (0, steps);
	while lo < hi {
		let mid = lo + (hi - lo) / 2;
		if dt * f64::from(mid) < time {
			lo = mid + 1;
		} else {
			hi = mid;
		}
	}
	for step in [lo, lo.saturating_sub(1)] {
		if dt.mul_add(f64::from(step), -time).abs() <= 1e-12 * dt.max(time.abs()) {
			return Ok(step);
		}
	}
	Err(invalid(
		"observation endpoints must align with integration steps",
	))
}
#[allow(
	clippy::float_cmp,
	clippy::too_many_lines,
	reason = "Exact frozen contract identity; linear complete preflight keeps work and storage admission together"
)]
pub(crate) fn admit(o: &Options) -> Result<Admission, CfdError> {
	let case = manifest("shedding2d")?;
	case.viscosity(o.reynolds)?;
	let end = o.dt * f64::from(o.steps);
	if !o.dt.is_finite()
		|| o.dt <= 0.
		|| o.steps == 0
		|| o.steps > o.max_steps
		|| o.steps > 1_000_000
		|| o.stride == 0
		|| !end.is_finite()
		|| end > case.time_window[1]
		|| !o.observation_start.is_finite()
		|| !o.observation_end.is_finite()
		|| o.observation_start < 0.
		|| o.observation_end > 1e-12_f64.mul_add(end.abs(), end)
		|| o.observation_end <= o.observation_start
		|| !(4..=64).contains(&o.angular_sectors)
		|| !(1..=8).contains(&o.radial_layers)
		|| !o.max_residual.is_finite()
		|| o.max_residual <= 0.
		|| o.minimum_cycles < case.measurement_minimum_cycles.unwrap_or(2).max(2)
		|| o.minimum_samples_per_cycle < 4
		|| !o.maximum_period_variation.is_finite()
		|| !(0. ..1.).contains(&o.maximum_period_variation)
		|| !o.lift_amplitude_floor.is_finite()
		|| o.lift_amplitude_floor <= 0.
	{
		return Err(invalid("invalid cylinder window/time/mesh/policy"));
	}
	let frozen_window = end == case.time_window[1]
		&& [o.observation_start, o.observation_end] == case.measurement_window;
	if !frozen_window && !o.short_window {
		return Err(invalid("changed window requires explicit --short-window"));
	}
	let observation_steps = [
		step_at(o.observation_start, o.dt, o.steps)?,
		step_at(o.observation_end, o.dt, o.steps)?,
	];
	let span = observation_steps[1] - observation_steps[0];
	let sample_count = usize::try_from(span / o.stride + 1 + u32::from(span % o.stride != 0))
		.map_err(|_| invalid("trace count overflow"))?;
	let trace_bytes = sample_count
		.checked_mul(size_of::<ForceSample>())
		.ok_or_else(|| invalid("trace storage overflow"))?;
	// Four corner rays may supplement the angular sectors. Each layer has two
	// triangles per ray interval, each retaining all six broken BDM1 coefficients.
	let local = (usize::try_from(o.angular_sectors).map_err(|_| invalid("mesh size"))? + 4)
		.checked_mul(12)
		.and_then(|n| n.checked_mul(usize::try_from(o.radial_layers).ok()?))
		.ok_or_else(|| invalid("mesh storage overflow"))?;
	if local > 768 || !(2..=1_000_000).contains(&sample_count) || trace_bytes > o.max_trace_bytes {
		return Err(invalid(
			"full reference mesh or force trace exceeds capacity",
		));
	}
	let n = u64::try_from(local).map_err(|_| invalid("mesh work overflow"))?;
	let square = product(n, n)?;
	let cube = product(square, n)?;
	// Conservative elementary-operation envelopes cover dense chart/SIP products,
	// bounded triangle/face quadrature, pressure Gram construction and elimination.
	let drift_work = sum(product(128, square)?, product(8192, n)?)?;
	let construction_work = product(256, cube)?;
	let integration_work = product(
		u64::from(o.steps),
		sum(product(4, drift_work)?, product(32, square)?)?,
	)?;
	let pressure_work = sum(product(64, cube)?, product(8, drift_work)?)?;
	let observation_work = product(
		u64::try_from(sample_count).map_err(|_| invalid("sample work"))?,
		pressure_work,
	)?;
	let analysis_work = product(
		u64::try_from(sample_count).map_err(|_| invalid("sample work"))?,
		128,
	)?;
	let aggregate_work = sum(
		sum(construction_work, integration_work)?,
		sum(observation_work, analysis_work)?,
	)?;
	// Assembly/chart plus one RK4 state and one full pressure solve, no trajectory.
	let base = local
		.checked_mul(local)
		.and_then(|v| v.checked_mul(256))
		.and_then(|v| v.checked_add(local.checked_mul(65_536)?))
		.and_then(|v| v.checked_add(4 * 1024 * 1024))
		.ok_or_else(|| invalid("reference peak overflow"))?;
	let peak_managed_upper_bound_bytes = base
		.checked_add(trace_bytes)
		.ok_or_else(|| invalid("trace peak overflow"))?;
	if aggregate_work > o.max_work || peak_managed_upper_bound_bytes > o.max_bytes {
		return Err(invalid(
			"full integration/pressure work or reference peak budget exceeded",
		));
	}
	Ok(Admission {
		frozen_window,
		observation_steps,
		sample_count,
		trace_bytes,
		aggregate_work,
		construction_work,
		integration_work,
		observation_work,
		analysis_work,
		local_velocity_upper_bound: local,
		peak_managed_upper_bound_bytes,
	})
}
#[derive(Default, Debug, serde::Serialize)]
pub(crate) struct Progress {
	pub(crate) completed_steps: u32,
	time: f64,
	observations_completed: usize,
}
#[derive(serde::Serialize)]
pub(crate) struct WindowResult {
	pub(crate) admission: Admission,
	manifest: CaseManifest,
	independent_dimension: usize,
	local_velocity_dimension: usize,
	constraint_rank: usize,
	cylinder_segments: usize,
	pub(crate) maximum_geometry_deviation: f64,
	pressure_convention: &'static str,
	force_convention: &'static str,
	maximum_momentum_residual: f64,
	maximum_continuity_residual: f64,
	maximum_boundary_residual: f64,
	pub(crate) trace: Vec<ForceSample>,
	pub(crate) statistics: SheddingStatistics,
}
#[allow(
	clippy::too_many_lines,
	reason = "Single state lifetime and observation failures remain beside their scalar trace writes"
)]
pub(crate) fn experiment(o: &Options, progress: &mut Progress) -> Result<WindowResult, CfdError> {
	let mut admission = admit(o)?;
	let mut trace = Vec::new();
	trace
		.try_reserve_exact(admission.sample_count)
		.map_err(|_| invalid("force trace allocation failed"))?;
	let actual_trace_bytes = trace
		.capacity()
		.checked_mul(size_of::<ForceSample>())
		.ok_or_else(|| invalid("actual trace capacity overflow"))?;
	let actual_peak = admission
		.peak_managed_upper_bound_bytes
		.checked_sub(admission.trace_bytes)
		.and_then(|n| n.checked_add(actual_trace_bytes))
		.ok_or_else(|| invalid("actual trace peak overflow"))?;
	if actual_trace_bytes > o.max_trace_bytes || actual_peak > o.max_bytes {
		return Err(invalid("actual force trace capacity exceeded"));
	}
	admission.trace_bytes = actual_trace_bytes;
	admission.peak_managed_upper_bound_bytes = actual_peak;
	let reference = cylinder::reference(
		"shedding2d",
		o.reynolds,
		o.angular_sectors,
		o.radial_layers,
		1,
	)?;
	let mut state = reference.initial_state.clone();
	let mut momentum = 0_f64;
	let mut continuity = 0_f64;
	let mut boundary = 0_f64;
	for step in 0..=o.steps {
		if step > 0 {
			state = reference.model.integrate_rk4(&state, o.dt, 1)?;
		}
		progress.completed_steps = step;
		progress.time = o.dt * f64::from(step);
		if step % o.stride == 0 || step == o.steps {
			eprintln!(
				"{}",
				serde_json::to_string(progress).map_err(|_| invalid("progress encoding"))?
			);
		}
		let [first, last] = admission.observation_steps;
		if step < first || step > last || ((step - first) % o.stride != 0 && step != last) {
			continue;
		}
		let s = reference.snapshot_at(&state, progress.time)?;
		let residuals = [
			s.pressure.momentum_residual,
			s.pressure.continuity_residual,
			s.pressure.pressure_mean_residual,
			s.boundary_residual,
		];
		if residuals
			.iter()
			.any(|v| !v.is_finite() || *v < 0. || *v > o.max_residual)
			|| [
				s.drag_coefficient,
				s.lift_coefficient,
				s.pressure_difference,
				s.mean_kinetic_energy,
			]
			.iter()
			.any(|v| !v.is_finite())
		{
			return Err(invalid(
				"force observation nonfinite or physical recovery residual too large",
			));
		}
		momentum = momentum.max(s.pressure.momentum_residual);
		continuity = continuity.max(s.pressure.continuity_residual);
		boundary = boundary.max(s.boundary_residual);
		trace.push(ForceSample {
			time: s.time,
			drag: s.drag_coefficient,
			lift: s.lift_coefficient,
			pressure_difference: s.pressure_difference,
		});
		progress.observations_completed = trace.len();
	}
	if trace.len() != admission.sample_count {
		return Err(invalid("force trace incomplete"));
	}
	let statistics = analyze_shedding(
		&trace,
		reference.manifest.reference_length,
		reference.manifest.reference_velocity,
		SheddingPolicy {
			minimum_cycles: o.minimum_cycles,
			minimum_samples_per_cycle: o.minimum_samples_per_cycle,
			maximum_relative_period_variation: o.maximum_period_variation,
			lift_amplitude_floor: o.lift_amplitude_floor,
			max_samples: admission.sample_count,
			max_work: usize::try_from(admission.analysis_work)
				.map_err(|_| invalid("analysis work conversion"))?,
		},
	)?;
	Ok(WindowResult {
		admission,
		independent_dimension: reference.model.dimension(),
		local_velocity_dimension: reference.model.diagnostics().local_velocity_dimension,
		constraint_rank: reference.model.diagnostics().constraint_rank,
		cylinder_segments: reference.cylinder_segments,
		maximum_geometry_deviation: reference.maximum_geometry_deviation,
		manifest: reference.manifest,
		pressure_convention: "p(front)-p(back); probes (0.15,0.2),(0.25,0.2); arithmetic mean of incident fluid P0 traces; natural-outflow pressure level",
		force_convention: "full pressure plus unsymmetrized viscous traction; Cd/Cl=2F/(rho*U_mean^2*D), rho=1,U_mean=1,D=0.1",
		maximum_momentum_residual: momentum,
		maximum_continuity_residual: continuity,
		maximum_boundary_residual: boundary,
		trace,
		statistics,
	})
}
#[cfg(not(test))]
#[derive(serde::Serialize)]
#[allow(
	clippy::struct_field_names,
	reason = "Machine-readable process counters name their byte units explicitly"
)]
struct ProcessMetrics {
	address_space_cap_bytes: Option<usize>,
	rss_high_water_bytes: usize,
	address_space_high_water_bytes: usize,
}
#[cfg(not(test))]
fn process_metrics() -> Result<ProcessMetrics, Box<dyn std::error::Error>> {
	let limits = std::fs::read_to_string("/proc/self/limits")?;
	let word = limits
		.lines()
		.find_map(|line| line.strip_prefix("Max address space"))
		.and_then(|line| line.split_whitespace().next())
		.ok_or("Linux process cap unavailable")?;
	let address_space_cap_bytes = if word == "unlimited" {
		None
	} else {
		Some(word.parse()?)
	};
	let status = std::fs::read_to_string("/proc/self/status")?;
	let bytes = |prefix: &str| -> Result<usize, Box<dyn std::error::Error>> {
		let word = status
			.lines()
			.find_map(|line| line.strip_prefix(prefix))
			.and_then(|line| line.split_whitespace().next())
			.ok_or("Linux process memory unavailable")?;
		Ok(word
			.parse::<usize>()?
			.checked_mul(1024)
			.ok_or("memory counter overflow")?)
	};
	Ok(ProcessMetrics {
		address_space_cap_bytes,
		rss_high_water_bytes: bytes("VmHWM:")?,
		address_space_high_water_bytes: bytes("VmPeak:")?,
	})
}
#[cfg(not(test))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
	#[derive(serde::Serialize)]
	struct Output<'a> {
		schema: &'static str,
		status: &'static str,
		parameters: &'a Options,
		completed: bool,
		convergence_certified: bool,
		quantum_execution: bool,
		elapsed_seconds: f64,
		process: ProcessMetrics,
		progress: Progress,
		result: Option<WindowResult>,
		error: Option<String>,
	}
	let options = Options::parse();
	let started = std::time::Instant::now();
	let mut progress = Progress::default();
	let outcome = experiment(&options, &mut progress);
	let completed = outcome.is_ok();
	let (result, error) = match outcome {
		Ok(result) => (Some(result), None),
		Err(error) => (None, Some(error.to_string())),
	};
	let output = Output {
		schema: "quest-cfd-cylinder-window-v1",
		status: if completed { "completed" } else { "rejected" },
		parameters: &options,
		completed,
		convergence_certified: false,
		quantum_execution: false,
		elapsed_seconds: started.elapsed().as_secs_f64(),
		process: process_metrics()?,
		progress,
		result,
		error,
	};
	serde_json::to_writer_pretty(std::io::stdout().lock(), &output)?;
	println!();
	if !completed {
		return Err("cylinder reference rejected; see structured receipt".into());
	}
	Ok(())
}
