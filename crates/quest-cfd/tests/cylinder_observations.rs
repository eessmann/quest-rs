#![allow(
	clippy::panic_in_result_fn,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Independent bounded pressure probes and observation diagnostics"
)]
#[test]
fn cylinder_snapshot_has_declared_pressure_difference_and_gauge_invariance()
-> Result<(), Box<dyn std::error::Error>> {
	let case = quest_cfd::cylinder::reference("shedding2d", 100, 4, 1, 1)?;
	let state = case.initial_state.clone();
	let snapshot = case.snapshot_at(&state, 0.)?;
	assert!(snapshot.pressure_difference.is_finite());
	let points = [[0.15, 0.2, 0.], [0.25, 0.2, 0.]];
	let p = case
		.model
		.sample_pressures(&snapshot.pressure.cell_pressure, &points)?;
	assert!((snapshot.pressure_difference - (p[0] - p[1])).abs() < 1e-12);
	let shifted: Vec<_> = snapshot
		.pressure
		.cell_pressure
		.iter()
		.map(|p| p + 2.5)
		.collect();
	let shifted = case.model.sample_pressures(&shifted, &points)?;
	assert!(((shifted[0] - shifted[1]) - snapshot.pressure_difference).abs() < 1e-12);
	assert!(
		case.model
			.sample_pressures(&snapshot.pressure.cell_pressure, &[[3., 0., 0.]])
			.is_err()
	);
	assert!(case.snapshot_at(&state, -1.).is_err());
	assert!(snapshot.method.contains("supplied"));
	assert!(!snapshot.method.contains("RK4"));
	assert!(case.reference(1e-5, 1)?.method.contains("RK4"));
	Ok(())
}

#[test]
fn cylinder_observations_revalidate_public_metadata() -> Result<(), Box<dyn std::error::Error>> {
	let original = quest_cfd::cylinder::reference("shedding2d", 100, 4, 1, 1)?;
	for invalid in [0., f64::NAN, f64::INFINITY, f64::MIN_POSITIVE, f64::MAX] {
		let mut case = original.clone();
		case.manifest.reference_velocity = invalid;
		assert!(case.snapshot_at(&case.initial_state, 0.).is_err());
		assert!(case.reference(1e-5, 1).is_err());
	}
	let mut case = original.clone();
	case.manifest.time_window[1] = f64::NAN;
	assert!(case.snapshot_at(&case.initial_state, 0.).is_err());
	assert!(case.reference(1e-5, 1).is_err());
	let mut case = original;
	case.manifest.dimension = 3;
	assert!(case.snapshot_at(&case.initial_state, 0.).is_err());
	Ok(())
}

#[test]
fn pressure_probe_budgets_admit_before_point_work() -> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::simplex::PressureProbeLimits;
	let case = quest_cfd::cylinder::reference("shedding2d", 100, 4, 1, 1)?;
	let snapshot = case.snapshot_at(&case.initial_state, 0.)?;
	let pressure = &snapshot.pressure.cell_pressure;
	let points = [[0.15, 0.2, 0.], [0.25, 0.2, 0.]];
	let limits = PressureProbeLimits {
		max_points: 2,
		max_work: pressure.len() * (1 + 64 * points.len()),
		max_bytes: pressure.len() * 8 + points.len() * 32 + 256,
	};
	let expected = case.model.sample_pressures(pressure, &points)?;
	let actual = case
		.model
		.sample_pressures_with_limits(pressure, &points, limits)?;
	assert_eq!(expected, actual);
	for denied in [
		PressureProbeLimits {
			max_points: 1,
			..limits
		},
		PressureProbeLimits {
			max_work: limits.max_work - 1,
			..limits
		},
		PressureProbeLimits {
			max_bytes: limits.max_bytes - 1,
			..limits
		},
	] {
		assert!(
			case.model
				.sample_pressures_with_limits(pressure, &points, denied)
				.is_err()
		);
	}
	Ok(())
}
