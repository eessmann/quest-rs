//! Bounded classical force-window diagnostics; no periodicity or sampling certificate.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Finite ordered windows, checked counts and explicit nonfinite-result rejection"
)]
use crate::CfdError;
/// A classical force observation at one physical time.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct ForceSample {
	pub time: f64,
	pub drag: f64,
	pub lift: f64,
	pub pressure_difference: f64,
}
/// Admission and explicit sufficient sampling policies for reporting a crossing rate.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct SheddingPolicy {
	pub minimum_cycles: u32,
	pub minimum_samples_per_cycle: u32,
	pub maximum_relative_period_variation: f64,
	pub lift_amplitude_floor: f64,
	pub max_samples: usize,
	pub max_work: usize,
}
impl Default for SheddingPolicy {
	fn default() -> Self {
		Self {
			minimum_cycles: 2,
			minimum_samples_per_cycle: 8,
			maximum_relative_period_variation: 0.1,
			lift_amplitude_floor: 1e-10,
			max_samples: 1_000_000,
			max_work: 128_000_000,
		}
	}
}
/// Frequency of upward crossings of the window-mean lift, with linear interpolation.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct CrossingFrequency {
	pub frequency: f64,
	pub strouhal: f64,
	pub completed_periods: u32,
	pub maximum_relative_period_variation: f64,
	pub samples_per_shortest_period: f64,
}
/// Time-weighted trapezoidal force statistics on the supplied window.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SheddingStatistics {
	pub window: [f64; 2],
	pub sample_count: usize,
	pub mean_drag: f64,
	pub mean_lift: f64,
	pub lift_rms: f64,
	pub lift_standard_deviation: f64,
	pub mean_pressure_difference: f64,
	pub frequency: Option<CrossingFrequency>,
	pub frequency_unavailable_reason: Option<&'static str>,
	pub policy: SheddingPolicy,
	pub periodicity_certified: bool,
}
/// Analyze an admitted physical observation window without inventing a Strouhal value.
///
/// A returned rate measures upward mean crossings. It is not proof of stationarity,
/// a fundamental spectral frequency, absence of aliasing, or a converged benchmark.
/// The caller must separately enforce its manifest's observation window and units.
/// # Errors
/// Rejects invalid policy/units, unordered or nonfinite samples, work/storage limits
/// and nonrepresentable moment arithmetic. Retains only O(1) additional storage.
#[allow(
	clippy::too_many_lines,
	reason = "Two bounded passes preserve time-weighted moments and an explicit reason for every unavailable frequency"
)]
pub fn analyze_shedding(
	samples: &[ForceSample],
	reference_length: f64,
	reference_velocity: f64,
	policy: SheddingPolicy,
) -> Result<SheddingStatistics, CfdError> {
	if samples.len() < 2
		|| samples.len() > policy.max_samples
		|| samples.len() > 1_000_000
		|| samples
			.len()
			.checked_mul(128)
			.is_none_or(|w| w > policy.max_work)
		|| policy.minimum_cycles < 2
		|| policy.minimum_samples_per_cycle < 4
		|| !policy.maximum_relative_period_variation.is_finite()
		|| !(0. ..1.).contains(&policy.maximum_relative_period_variation)
		|| !policy.lift_amplitude_floor.is_finite()
		|| policy.lift_amplitude_floor <= 0.
		|| !reference_length.is_finite()
		|| reference_length <= 0.
		|| !reference_velocity.is_finite()
		|| reference_velocity <= 0.
	{
		return Err(CfdError::InvalidInput(
			"invalid force-window policy, units or work budget",
		));
	}
	if samples.iter().any(|s| {
		[s.time, s.drag, s.lift, s.pressure_difference]
			.iter()
			.any(|v| !v.is_finite())
	}) {
		return Err(CfdError::InvalidInput("nonfinite force observation"));
	}
	let window = [samples[0].time, samples[samples.len() - 1].time];
	let duration = window[1] - window[0];
	if !duration.is_finite() || duration <= 0. {
		return Err(CfdError::InvalidInput("invalid observation duration"));
	}
	let (mut drag, mut lift, mut lift2, mut pressure, mut max_gap) = (0., 0., 0., 0., 0_f64);
	for pair in samples.windows(2) {
		let (a, b) = (&pair[0], &pair[1]);
		let dt = b.time - a.time;
		if dt <= 0. || !dt.is_finite() {
			return Err(CfdError::InvalidInput("force times must strictly increase"));
		}
		let w = 0.5 * (dt / duration);
		drag = w.mul_add(a.drag + b.drag, drag);
		lift = w.mul_add(a.lift + b.lift, lift);
		lift2 = w.mul_add(b.lift.mul_add(b.lift, a.lift * a.lift), lift2);
		pressure = w.mul_add(a.pressure_difference + b.pressure_difference, pressure);
		max_gap = max_gap.max(dt);
	}
	if [drag, lift, lift2, pressure].iter().any(|v| !v.is_finite()) {
		return Err(CfdError::InvalidInput("force moment overflow"));
	}
	let (mut variance, mut max_amplitude) = (0., 0_f64);
	let (mut first, mut last) = (None, None);
	let (mut periods, mut min_period, mut max_period) = (0_u32, f64::INFINITY, 0_f64);
	for pair in samples.windows(2) {
		let (a, b) = (&pair[0], &pair[1]);
		let (la, lb) = (a.lift - lift, b.lift - lift);
		let dt = b.time - a.time;
		variance = (0.5 * (dt / duration)).mul_add(lb.mul_add(lb, la * la), variance);
		max_amplitude = max_amplitude.max(la.abs()).max(lb.abs());
		if la <= 0. && lb > 0. {
			let crossing = ((-la) / (lb - la)).mul_add(dt, a.time);
			if !crossing.is_finite() {
				return Err(CfdError::InvalidInput("crossing time overflow"));
			}
			if let Some(previous) = last {
				let period = crossing - previous;
				min_period = min_period.min(period);
				max_period = max_period.max(period);
				periods += 1;
			} else {
				first = Some(crossing);
			}
			last = Some(crossing);
		}
	}
	if !variance.is_finite() || !max_amplitude.is_finite() {
		return Err(CfdError::InvalidInput("force variance overflow"));
	}
	let mut frequency = None;
	let reason = if max_amplitude < policy.lift_amplitude_floor {
		Some("lift variation below explicit amplitude floor")
	} else if periods < policy.minimum_cycles {
		Some("insufficient complete upward-crossing periods")
	} else {
		let period = (last.unwrap_or(window[1]) - first.unwrap_or(window[0])) / f64::from(periods);
		let variation = ((min_period - period).abs().max((max_period - period).abs())) / period;
		let samples_per_period = min_period / max_gap;
		if !period.is_finite() || period <= 0. || !variation.is_finite() {
			return Err(CfdError::InvalidInput("crossing period overflow"));
		}
		if variation > policy.maximum_relative_period_variation {
			Some("crossing periods fail consistency policy")
		} else if samples_per_period < f64::from(policy.minimum_samples_per_cycle) {
			Some("crossings are insufficiently time resolved")
		} else {
			let f = 1. / period;
			let strouhal = f * reference_length / reference_velocity;
			if !f.is_finite() || !strouhal.is_finite() {
				return Err(CfdError::InvalidInput("crossing frequency overflow"));
			}
			frequency = Some(CrossingFrequency {
				frequency: f,
				strouhal,
				completed_periods: periods,
				maximum_relative_period_variation: variation,
				samples_per_shortest_period: samples_per_period,
			});
			None
		}
	};
	Ok(SheddingStatistics {
		window,
		sample_count: samples.len(),
		mean_drag: drag,
		mean_lift: lift,
		lift_rms: lift2.sqrt(),
		lift_standard_deviation: variance.sqrt(),
		mean_pressure_difference: pressure,
		frequency,
		frequency_unavailable_reason: reason,
		policy,
		periodicity_certified: false,
	})
}
