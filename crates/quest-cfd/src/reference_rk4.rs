//! Internal fixed-step RK4, shared by typed, independently admitted reference owners.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Bounded checked shapes preserve the existing RK4 operation order; candidates are validated before acceptance"
)]
use crate::CfdError;
/// Progress of complete accepted states, separate from callback completion.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Rk4Progress {
	pub completed_steps: u32,
	pub attempted_step: Option<u32>,
	pub accepted_time: f64,
	pub drift_calls_attempted: u64,
	pub observer_calls_attempted: u64,
	pub failure_phase: Option<&'static str>,
}
pub struct Rk4Attempt {
	pub outcome: Result<(), CfdError>,
	pub state: Vec<f64>,
	pub progress: Rk4Progress,
}
const fn invalid() -> CfdError {
	CfdError::InvalidInput("complete RK4 shape, finite arithmetic or scratch admission")
}
fn audit(vectors: &[&Vec<f64>], bytes: usize) -> Result<(), CfdError> {
	let total = vectors.iter().try_fold(0usize, |sum, v| {
		v.capacity()
			.checked_mul(8)
			.and_then(|n| sum.checked_add(n))
			.ok_or_else(invalid)
	})?;
	if total > bytes {
		Err(invalid())
	} else {
		Ok(())
	}
}
fn clone_values(values: &[f64]) -> Result<Vec<f64>, CfdError> {
	let mut out = Vec::new();
	out.try_reserve_exact(values.len()).map_err(|_| invalid())?;
	out.extend_from_slice(values);
	Ok(out)
}
fn valid(values: &[f64], dimension: usize) -> Result<(), CfdError> {
	if values.len() != dimension || values.iter().any(|x| !x.is_finite()) {
		Err(invalid())
	} else {
		Ok(())
	}
}
/// Callers own source/query accounting; this private helper audits all retained vector capacities.
#[allow(
	clippy::too_many_lines,
	reason = "One fixed-step transaction keeps every acceptance/failure boundary explicit"
)]
pub fn integrate(
	mut state: Vec<f64>,
	dt: f64,
	steps: u32,
	scratch_bytes: usize,
	mut drift: impl FnMut(f64, &[f64]) -> Result<Vec<f64>, CfdError>,
	mut observer: impl FnMut(u32, f64, &[f64]) -> Result<(), CfdError>,
) -> Rk4Attempt {
	let mut progress = Rk4Progress::default();
	let outcome = (|| {
		progress.failure_phase = Some("preflight");
		if !dt.is_finite()
			|| dt <= 0.
			|| steps > 1_000_000
			|| !(f64::from(steps) * dt).is_finite()
			|| !(f64::from(steps.saturating_sub(1)) * dt + dt).is_finite()
		{
			return Err(invalid());
		}
		let dimension = state.len();
		if dimension.checked_mul(64).is_none_or(|n| n > scratch_bytes) {
			return Err(invalid());
		}
		valid(&state, dimension)?;
		audit(&[&state], scratch_bytes)?;
		for step in 0..steps {
			progress.attempted_step = Some(step + 1);
			let time = f64::from(step) * dt;
			let mut call = |time: f64, values: &[f64]| {
				progress.drift_calls_attempted += 1;
				let out = drift(time, values)?;
				valid(&out, dimension)?;
				Ok::<_, CfdError>(out)
			};
			progress.failure_phase = Some("drift stage 1");
			let k1 = call(time, &state)?;
			audit(&[&state, &k1], scratch_bytes)?;
			let shift = |k: &[f64], factor: f64| -> Result<Vec<f64>, CfdError> {
				let mut out = clone_values(&state)?;
				for (x, v) in out.iter_mut().zip(k) {
					*x += factor * dt * v;
				}
				valid(&out, dimension)?;
				Ok(out)
			};
			progress.failure_phase = Some("drift stage 2");
			let intermediate = shift(&k1, 0.5)?;
			audit(&[&state, &k1, &intermediate], scratch_bytes)?;
			let k2 = call(time + 0.5 * dt, &intermediate)?;
			audit(&[&state, &k1, &k2, &intermediate], scratch_bytes)?;
			drop(intermediate);
			progress.failure_phase = Some("drift stage 3");
			let intermediate = shift(&k2, 0.5)?;
			audit(&[&state, &k1, &k2, &intermediate], scratch_bytes)?;
			let k3 = call(time + 0.5 * dt, &intermediate)?;
			audit(&[&state, &k1, &k2, &k3, &intermediate], scratch_bytes)?;
			drop(intermediate);
			progress.failure_phase = Some("drift stage 4");
			let intermediate = shift(&k3, 1.)?;
			audit(&[&state, &k1, &k2, &k3, &intermediate], scratch_bytes)?;
			let k4 = call(time + dt, &intermediate)?;
			audit(&[&state, &k1, &k2, &k3, &k4, &intermediate], scratch_bytes)?;
			drop(intermediate);
			progress.failure_phase = Some("candidate combination");
			let mut candidate = clone_values(&state)?;
			audit(&[&state, &k1, &k2, &k3, &k4, &candidate], scratch_bytes)?;
			for i in 0..dimension {
				candidate[i] += dt * (k1[i] + 2. * k2[i] + 2. * k3[i] + k4[i]) / 6.;
			}
			valid(&candidate, dimension)?;
			let accepted_time = time + dt;
			if !accepted_time.is_finite() {
				return Err(invalid());
			}
			state = candidate;
			drop(k1);
			drop(k2);
			drop(k3);
			drop(k4);
			progress.completed_steps = step + 1;
			progress.accepted_time = accepted_time;
			progress.failure_phase = Some("accepted-step observer");
			progress.observer_calls_attempted += 1;
			observer(step + 1, accepted_time, &state)?;
		}
		progress.attempted_step = None;
		progress.failure_phase = None;
		Ok(())
	})();
	Rk4Attempt {
		outcome,
		state,
		progress,
	}
}
#[cfg(test)]
mod tests {
	#![allow(
		clippy::panic_in_result_fn,
		reason = "Independent analytic and injected-failure assertions fail the bounded test directly"
	)]
	use super::*;
	#[test]
	fn analytic_nonautonomous_and_nonlinear_orders() -> Result<(), crate::CfdError> {
		for nonlinear in [false, true] {
			let mut errors = Vec::new();
			for steps in [4, 8, 16] {
				let dt = 0.2 / f64::from(steps);
				let attempt = integrate(
					vec![1.],
					dt,
					steps,
					4096,
					|time, x| Ok(vec![if nonlinear { x[0] * x[0] } else { time * x[0] }]),
					|_, _, _| Ok(()),
				);
				attempt.outcome?;
				let exact = if nonlinear { 1.25 } else { 0.02_f64.exp() };
				errors.push((attempt.state[0] - exact).abs());
			}
			assert!(errors[0] / errors[1] > 12.);
			assert!(errors[1] / errors[2] > 12.);
		}
		Ok(())
	}
	#[test]
	fn failures_keep_last_accepted_state_and_count_attempts() {
		let mut calls = 0;
		let attempt = integrate(
			vec![2.],
			0.1,
			3,
			4096,
			|_, _| {
				calls += 1;
				if calls == 6 {
					Err(crate::CfdError::InvalidInput("injected drift"))
				} else {
					Ok(vec![1.])
				}
			},
			|_, _, _| Ok(()),
		);
		assert!(attempt.outcome.is_err());
		assert_eq!(attempt.progress.completed_steps, 1);
		assert_eq!(attempt.progress.drift_calls_attempted, 6);
		assert!((attempt.state[0] - 2.1).abs() < 1e-14);
		let observed = integrate(
			vec![2.],
			0.1,
			3,
			4096,
			|_, _| Ok(vec![1.]),
			|_, _, _| Err(crate::CfdError::InvalidInput("injected observer")),
		);
		assert!(observed.outcome.is_err());
		assert_eq!(observed.progress.completed_steps, 1);
		assert_eq!(
			observed.progress.failure_phase,
			Some("accepted-step observer")
		);
		assert!((observed.state[0] - 2.1).abs() < 1e-14);
		let tight = integrate(
			vec![2.],
			0.1,
			3,
			1,
			|_, _| panic!("unadmitted callback"),
			|_, _, _| Ok(()),
		);
		assert!(tight.outcome.is_err());
		assert_eq!(tight.progress.drift_calls_attempted, 0);
	}
	#[test]
	fn combination_overflow_and_oversized_callback_keep_the_prior_state() {
		let a = integrate(
			vec![2.],
			0.01,
			1,
			4096,
			|_, _| Ok(vec![1e308]),
			|_, _, _| Ok(()),
		);
		assert!(a.outcome.is_err());
		assert_eq!(a.progress.completed_steps, 0);
		assert_eq!(a.state, vec![2.]);
		assert_eq!(a.progress.failure_phase, Some("candidate combination"));
		let a = integrate(
			vec![2.],
			0.01,
			1,
			64,
			|_, _| {
				let mut x = Vec::with_capacity(1024);
				x.push(1.);
				Ok(x)
			},
			|_, _, _| Ok(()),
		);
		assert!(a.outcome.is_err());
		assert_eq!(a.progress.drift_calls_attempted, 1);
		assert_eq!(a.state, vec![2.]);
	}
	#[test]
	fn polynomial_lifting_derivative_is_part_of_the_integrated_dynamics()
	-> Result<(), crate::CfdError> {
		use crate::physical_space::{
			BoundaryLimits, BoundaryTimeCoefficient, PhysicalSpace, PolynomialBoundary,
		};
		let space =
			PhysicalSpace::box_mesh(2, 1, 1., 0.02, crate::simplex::BoxBoundary::Periodic, 1)?;
		let mut modes = vec![BoundaryTimeCoefficient::zero(&space)?; 2];
		for block in modes[1].lifting.chunks_mut(6) {
			block[..3].fill(1.);
		}
		let flow = PolynomialBoundary::new(&space, modes, BoundaryLimits::default())?;
		let attempt = integrate(
			vec![0.; space.dimension()],
			0.025,
			4,
			65536,
			|time, state| flow.drift(time, state),
			|_, _, _| Ok(()),
		);
		attempt.outcome?;
		assert!(attempt.state.iter().any(|a| a.abs() > 0.01));
		assert!(
			flow.coefficients_at(0.1, &attempt.state)?
				.iter()
				.all(|v| v.abs() < 1e-11)
		);
		Ok(())
	}
	#[test]
	fn absolute_stages_and_zero_steps_preserve_existing_order() -> Result<(), crate::CfdError> {
		let mut times = Vec::new();
		let a = integrate(
			vec![1.],
			0.1,
			2,
			4096,
			|time, _| {
				times.push(time.to_bits());
				Ok(vec![time])
			},
			|_, _, _| Ok(()),
		);
		a.outcome?;
		let expected = [0_f64, 0.05, 0.05, 0.1, 0.1, 0.1 + 0.05, 0.1 + 0.05, 0.2];
		assert_eq!(times, expected.map(f64::to_bits));
		assert!((a.state[0] - 1.02).abs() < 1e-14);
		let z = integrate(
			vec![-0.],
			0.1,
			0,
			4096,
			|_, _| panic!("zero-step drift"),
			|_, _, _| panic!("zero-step observer"),
		);
		z.outcome?;
		assert_eq!(z.state[0].to_bits(), (-0_f64).to_bits());
		Ok(())
	}
	#[test]
	fn complete_interval_overflow_rejects_before_a_callback() {
		let mut calls = 0;
		let result = integrate(
			vec![1.],
			1e308,
			2,
			4096,
			|_, _| {
				calls += 1;
				Ok(vec![0.])
			},
			|_, _, _| Ok(()),
		);
		assert!(result.outcome.is_err());
		assert_eq!(calls, 0);
		assert_eq!(result.progress.completed_steps, 0);
	}
}
