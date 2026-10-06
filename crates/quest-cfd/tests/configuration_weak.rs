#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	clippy::panic_in_result_fn,
	reason = "Independent complete-coordinate references and resource boundary assertions"
)]
use quest_cfd::{
	configuration::ConfigurationGrid,
	configuration_weak::{PeriodicWeakSource, WeakLimits, WeakRequest},
};
use quest_numerics::Complex64;

#[test]
fn full_physical_reference_and_exact_nonzero_underflow_are_retained()
-> Result<(), Box<dyn std::error::Error>> {
	let prepared = PeriodicWeakSource::prepare(0.01, 0, WeakLimits::default());
	let source = prepared.outcome?;
	let grid = ConfigurationGrid::uniform(5, -1., 1., 1, 2, 243)?;
	let mut state = vec![Complex64::new(0., 0.); 243];
	state[121] = Complex64::new(1., 0.);
	state[242] = Complex64::new(f64::from_bits(1), 0.);
	let attempt = source.diagnose(&grid, &state, WeakRequest::default(), WeakLimits::default());
	assert_eq!(attempt.resources.supported_rows, 2);
	assert_eq!(attempt.visited_rows, 2);
	let report = attempt.outcome?;
	assert!(!report.zero_exterior_trace);
	assert_eq!(report.rates.len(), 6);
	assert!(report.cartesian_energy_discrepancy < 1e-11);
	assert!(!report.convergence_certified);
	Ok(())
}

#[test]
fn complete_complex_ensemble_matches_original_five_coordinate_expectations()
-> Result<(), Box<dyn std::error::Error>> {
	let source = PeriodicWeakSource::prepare(0.01, 0, WeakLimits::default()).outcome?;
	let grid = ConfigurationGrid::uniform(5, -0.4, 0.4, 1, 2, 243)?;
	let mut state = vec![Complex64::new(0., 0.); 243];
	state[16] = Complex64::new(0.3, 0.4);
	state[185] = Complex64::new(-0.2, 0.1);
	state[97] = Complex64::new(0.1, 0.2);
	let probability: f64 = state.iter().map(Complex64::norm_sqr).sum();
	let result = source
		.diagnose(
			&grid,
			&state,
			WeakRequest {
				nonlinear_witness: true,
				..WeakRequest::default()
			},
			WeakLimits::default(),
		)
		.outcome?;
	let mut reference = [0.; 5];
	for i in [16, 97, 185] {
		let a = grid.point(i).ok_or("point")?;
		let f = source.model().drift(&a)?;
		for j in 0..5 {
			reference[j] += state[i].norm_sqr() / probability * f[j];
		}
	}
	for (j, &expected) in reference.iter().enumerate() {
		assert!((result.rates[j].physical_rate - expected).abs() < 1e-11);
	}
	let generator = grid.generator(source.model(), quest_numerics::SparseLimits::default())?;
	let action = generator.matvec(&state, quest_numerics::SparseLimits::default())?;
	let mut direct = [0.; 6];
	for i in 0..243 {
		let a = grid.point(i).ok_or("point")?;
		let weight = 2. * (state[i].conj() * action[i]).re / probability;
		for j in 0..5 {
			direct[j] += weight * a[j];
		}
		direct[5] += weight * source.model().energy(&a)?;
	}
	assert!(direct.iter().any(|x| x.abs() > 1e-6));
	for (j, &expected) in direct.iter().enumerate() {
		assert!((result.rates[j].raw_skew_rate - expected).abs() < 1e-11);
	}
	assert!(result.nonlinear_mean_square.ok_or("witness")? > 1e-8);
	assert!(result.cartesian_energy_discrepancy < 1e-11);
	assert_eq!(result.coordinate_standard_deviations.len(), 5);
	Ok(())
}

#[test]
fn admissions_preserve_failed_phase_and_constructor_overlap()
-> Result<(), Box<dyn std::error::Error>> {
	let rejected = PeriodicWeakSource::prepare(
		0.01,
		0,
		WeakLimits {
			max_bytes: 1,
			..WeakLimits::default()
		},
	);
	assert!(rejected.outcome.is_err());
	assert!(rejected.resources.peak_bytes > 1);
	let source =
		PeriodicWeakSource::prepare(0.01, 64 * 1024 * 1024, WeakLimits::default()).outcome?;
	let grid = ConfigurationGrid::uniform(5, -1., 1., 1, 2, 243)?;
	let mut state = vec![Complex64::new(0., 0.); 243];
	state[121] = Complex64::new(1., 0.);
	let first = source.diagnose(&grid, &state, WeakRequest::default(), WeakLimits::default());
	first.outcome?;
	assert!(first.resources.peak_bytes >= 64 * 1024 * 1024 + 8 * 1024 * 1024);
	let attempt = source.diagnose(
		&grid,
		&state,
		WeakRequest::default(),
		WeakLimits {
			max_source_work: first.resources.source_work - 1,
			..WeakLimits::default()
		},
	);
	assert!(attempt.outcome.is_err());
	assert_eq!(attempt.visited_rows, 0);
	assert_eq!(attempt.resources.supported_rows, 1);
	assert_eq!(attempt.phase, "contraction admission");
	let attempt = source.diagnose(
		&grid,
		&state,
		WeakRequest {
			additional_external_bytes: usize::MAX,
			..WeakRequest::default()
		},
		WeakLimits::default(),
	);
	assert!(attempt.outcome.is_err());
	assert_eq!(attempt.visited_rows, 0);
	state[0].re = f64::NAN;
	assert!(
		source
			.diagnose(&grid, &state, WeakRequest::default(), WeakLimits::default())
			.outcome
			.is_err()
	);
	Ok(())
}

#[test]
fn failed_extraction_reports_attempted_callbacks_not_reserved_maximum() {
	let attempt = PeriodicWeakSource::prepare(f64::MAX, 0, WeakLimits::default());
	assert!(attempt.outcome.is_err());
	assert!(attempt.resources.physical_calls_attempted > 0);
	assert!(attempt.resources.physical_calls_attempted < attempt.resources.physical_calls);
}

#[test]
fn sampling_policy_and_dg2_query_rejection_keep_the_full_tensor()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::configuration_diagnostics::regularization_resolution;
	let center = [0.15, -0.1, 0.07, 0.11, -0.04];
	let small = ConfigurationGrid::uniform(5, -1., 1., 1, 2, 243)?;
	assert!(regularization_resolution(&small, &center, 0.5, 2).is_err());
	for (order, cells, counts) in [
		(1, 3, [2, 2, 2, 2, 2]),
		(1, 4, [2, 2, 2, 2, 2]),
		(1, 5, [3, 2, 2, 3, 2]),
		(2, 3, [3, 3, 3, 3, 3]),
	] {
		let grid = ConfigurationGrid::uniform(5, -1., 1., cells, order, 100_000)?;
		assert_eq!(
			regularization_resolution(&grid, &center, 0.5, 2)?.distinct_samples_in_support,
			counts
		);
	}
	let source = PeriodicWeakSource::prepare(0.01, 0, WeakLimits::default()).outcome?;
	let grid = ConfigurationGrid::uniform(5, -1., 1., 3, 2, 59_049)?;
	let state = grid.initial_bump(&center, 0.5)?;
	let attempt = source.diagnose(&grid, &state, WeakRequest::default(), WeakLimits::default());
	assert!(attempt.outcome.is_err());
	assert_eq!(attempt.phase, "contraction admission");
	assert_eq!(attempt.validated_rows, 59_049);
	assert_eq!(attempt.visited_rows, 0);
	assert_eq!(attempt.resources.supported_rows, 3_125);
	eprintln!("DG2 rejected full tensor receipt: {:?}", attempt.resources);
	assert!(attempt.resources.source_work > WeakLimits::default().max_source_work);
	Ok(())
}

#[test]
fn zero_shape_time_and_every_limit_fail_without_row_execution()
-> Result<(), Box<dyn std::error::Error>> {
	let source = PeriodicWeakSource::prepare(0.01, 0, WeakLimits::default()).outcome?;
	let grid = ConfigurationGrid::uniform(5, -1., 1., 1, 2, 243)?;
	let mut state = vec![Complex64::new(0., 0.); 243];
	let zero = source.diagnose(&grid, &state, WeakRequest::default(), WeakLimits::default());
	assert!(zero.outcome.is_err());
	assert_eq!(zero.phase, "state validation");
	assert_eq!(zero.visited_rows, 0);
	assert!(
		source
			.diagnose(
				&grid,
				&state[..242],
				WeakRequest::default(),
				WeakLimits::default()
			)
			.outcome
			.is_err()
	);
	state[100] = Complex64::new(0.3, 0.4);
	assert!(
		source
			.diagnose(
				&grid,
				&state,
				WeakRequest {
					time: f64::INFINITY,
					..WeakRequest::default()
				},
				WeakLimits::default()
			)
			.outcome
			.is_err()
	);
	let exact = source.diagnose(&grid, &state, WeakRequest::default(), WeakLimits::default());
	exact.outcome?;
	for limits in [
		WeakLimits {
			max_bytes: exact.resources.peak_bytes - 1,
			..WeakLimits::default()
		},
		WeakLimits {
			max_physical_work: exact.resources.physical_work - 1,
			..WeakLimits::default()
		},
		WeakLimits {
			max_physical_calls: exact.resources.physical_calls - 1,
			..WeakLimits::default()
		},
	] {
		let rejected = source.diagnose(&grid, &state, WeakRequest::default(), limits);
		assert!(rejected.outcome.is_err());
		assert_eq!(rejected.visited_rows, 0);
	}
	Ok(())
}

#[test]
fn caller_preparation_is_explicit_and_charged_before_any_state_scan()
-> Result<(), Box<dyn std::error::Error>> {
	let source = PeriodicWeakSource::prepare(0.01, 0, WeakLimits::default()).outcome?;
	let grid = ConfigurationGrid::uniform(5, -1., 1., 1, 2, 243)?;
	let mut state = vec![Complex64::new(0., 0.); 243];
	state[121] = Complex64::new(1., 0.);
	let request = WeakRequest {
		caller_declared_input_preparation_work: Some(1_000_000_000),
		..WeakRequest::default()
	};
	let rejected = source.diagnose(&grid, &state, request, WeakLimits::default());
	assert!(rejected.outcome.is_err());
	assert_eq!(rejected.validated_rows, 0);
	assert_eq!(
		rejected.resources.caller_declared_input_preparation_work,
		Some(1_000_000_000)
	);
	let unknown = source.diagnose(&grid, &state, WeakRequest::default(), WeakLimits::default());
	unknown.outcome?;
	assert_eq!(
		unknown.resources.caller_declared_input_preparation_work,
		None
	);
	Ok(())
}

#[test]
fn declared_inaccessible_state_capacity_is_not_silently_free()
-> Result<(), Box<dyn std::error::Error>> {
	let source = PeriodicWeakSource::prepare(0.01, 0, WeakLimits::default()).outcome?;
	let grid = ConfigurationGrid::uniform(5, -1., 1., 1, 2, 243)?;
	let mut state = Vec::with_capacity(2_000_000);
	state.resize(243, Complex64::new(0., 0.));
	state[121] = Complex64::new(1., 0.);
	let hidden = (state.capacity() - state.len()) * size_of::<Complex64>();
	let limits = WeakLimits {
		max_bytes: 16 * 1024 * 1024,
		..WeakLimits::default()
	};
	let attempt = source.diagnose(
		&grid,
		&state,
		WeakRequest {
			additional_external_bytes: hidden,
			..WeakRequest::default()
		},
		limits,
	);
	assert!(attempt.outcome.is_err());
	assert_eq!(attempt.visited_rows, 0);
	assert_eq!(attempt.resources.external_bytes, hidden);
	assert!(attempt.resources.peak_bytes > limits.max_bytes);
	Ok(())
}
