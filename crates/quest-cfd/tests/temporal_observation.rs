#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	clippy::panic_in_result_fn,
	reason = "Independent low-degree interpolation identities use bounded fixture indices and explicit test assertions"
)]
use quest_cfd::{
	CfdError,
	probability_observation::TemporalNodeSide,
	stream_history::{HistoryRowDynamics, HistoryStreamLimits, TemporalHistoryRecipe},
	temporal_observation::{TemporalInterpolation, TemporalInterpolationLimits},
};
use quest_numerics::Complex64;
struct TwoCoordinates;
impl HistoryRowDynamics for TwoCoordinates {
	fn dimension(&self) -> usize {
		2
	}
	fn max_row_entries(&self) -> usize {
		0
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(0)
	}
	fn row_query_bytes(&self) -> usize {
		0
	}
	fn row_query_work(&self) -> usize {
		1
	}
	fn visit_row(
		&self,
		_: f64,
		_: usize,
		_: &mut dyn FnMut(usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		Ok(())
	}
	fn source_entry(&self, _: f64, _: usize) -> Result<Complex64, CfdError> {
		Ok(Complex64::new(0., 0.))
	}
}
#[test]
fn interpolation_preserves_complex_interference_and_quadratic_time_dependence()
-> Result<(), CfdError> {
	let d = TwoCoordinates;
	let h = TemporalHistoryRecipe::new(&d, 2., 2, 1, HistoryStreamLimits::default())?;
	let t = TemporalInterpolation::new(&h, 0, 0.5, TemporalInterpolationLimits::default())?;
	let zero = t.amplitude(0, |index| {
		Ok(Complex64::new(if index == 0 { 1. } else { -1. }, 0.))
	})?;
	assert!(
		zero.norm() < 1e-15,
		"opposite amplitudes cancel; averaging probabilities is wrong"
	);
	let complex = t.amplitude(0, |index| {
		Ok(if index == 0 {
			Complex64::new(1., 0.)
		} else {
			Complex64::new(0., 1.)
		})
	})?;
	assert!((complex.norm_sqr() - 0.5).abs() < 1e-15);
	let h = TemporalHistoryRecipe::new(&d, 2., 2, 2, HistoryStreamLimits::default())?;
	let t = TemporalInterpolation::new(&h, 1, 0.25, TemporalInterpolationLimits::default())?;
	let mut queried = Vec::new();
	let a = t.amplitude(1, |index| {
		queried.push(index);
		let time = match index {
			7 => 1.,
			9 => 1.5,
			11 => 2.,
			_ => return Err(CfdError::InvalidInput("unexpected time coefficient")),
		};
		Ok(Complex64::new(time * time, time))
	})?;
	assert_eq!(queried, vec![7, 9, 11]);
	assert!((a - Complex64::new(1.5625, 1.25)).norm() < 1e-14);
	assert!((t.physical_time() - 1.25).abs() < 1e-15);
	assert!(t.norm_upper_bound() >= t.weights().iter().map(|v| v * v).sum::<f64>().sqrt());
	Ok(())
}
#[test]
fn shared_boundary_keeps_both_dg_traces_and_admits_before_queries() -> Result<(), CfdError> {
	let d = TwoCoordinates;
	let h = TemporalHistoryRecipe::new(&d, 2., 2, 1, HistoryStreamLimits::default())?;
	let left = TemporalInterpolation::new(&h, 0, 1., TemporalInterpolationLimits::default())?;
	let right = TemporalInterpolation::new(&h, 1, 0., TemporalInterpolationLimits::default())?;
	assert!(matches!(left.side(), TemporalNodeSide::SlabRight));
	assert!(matches!(right.side(), TemporalNodeSide::SlabLeft));
	assert!(
		(left
			.amplitude(0, |i| Ok(Complex64::new(
				if i == 2 { 11. } else { 22. },
				0.
			)))?
			.re
			- 11.)
			.abs()
			< 1e-15
	);
	assert!((right.amplitude(0, |_| Ok(Complex64::new(22., 0.)))?.re - 22.).abs() < 1e-15);
	let mut queries = 0;
	assert!(
		left.amplitude(2, |_| {
			queries += 1;
			Ok(Complex64::new(0., 0.))
		})
		.is_err()
	);
	assert_eq!(queries, 0);
	for fraction in [f64::NAN, -0.01, 1.01] {
		assert!(
			TemporalInterpolation::new(&h, 0, fraction, TemporalInterpolationLimits::default())
				.is_err()
		);
	}
	assert!(
		TemporalInterpolation::new(&h, 2, 0.5, TemporalInterpolationLimits::default()).is_err()
	);
	assert!(
		TemporalInterpolation::new(
			&h,
			0,
			0.5,
			TemporalInterpolationLimits {
				max_query_work: 0,
				..Default::default()
			}
		)
		.is_err()
	);
	assert!(
		left.amplitude(0, |_| Ok(Complex64::new(f64::NAN, 0.)))
			.is_err()
	);
	Ok(())
}
