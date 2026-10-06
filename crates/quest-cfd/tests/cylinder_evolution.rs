//! Complete fixed-window dynamics, independently declared unchanged resource ceilings.
#![allow(
	clippy::panic_in_result_fn,
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	reason = "Fixed 54-coordinate independent fixture assertions preserve explicit numerical checks"
)]
use quest_cfd::cylinder_high_order::{
	CylinderEvolutionRequest, CylinderInitialCondition, CylinderPhysicalLimits,
	CylinderPhysicalSource,
};
#[test]
fn complete_evolution_preflight_and_final_pressure_use_one_ledger()
-> Result<(), Box<dyn std::error::Error>> {
	let source = CylinderPhysicalSource::new(
		4,
		1,
		100,
		CylinderPhysicalLimits {
			max_work: 2_000_000_000,
			..Default::default()
		},
	)?;
	let mut prepared = source.prepare()?;
	let initial = prepared.initial_state().to_vec();
	let attempt = prepared.evolve_with_receipt(
		&initial,
		CylinderEvolutionRequest {
			steps: 2,
			initial_condition: CylinderInitialCondition::PreparedMinimumMassCompatible,
		},
	);
	let report = attempt.outcome?;
	assert_eq!(attempt.last_state.len(), 54);
	assert_eq!(attempt.progress.completed_steps, 2);
	assert_eq!(attempt.progress.drift_calls_attempted, 8);
	assert_eq!(report.quadratic_probe_calls, 3);
	assert!(report.state_change_norm > 1e-8);
	assert!(report.snapshot.pressure.momentum_residual < 1e-8);
	assert_eq!(attempt.resources.cumulative_work, 1_733_765_504);
	assert!(
		prepared
			.evolve_with_receipt(
				&initial,
				CylinderEvolutionRequest {
					steps: 2,
					initial_condition: CylinderInitialCondition::PreparedMinimumMassCompatible
				}
			)
			.outcome
			.is_err()
	);
	Ok(())
}

#[test]
fn rejected_requests_and_failed_initial_diagnostic_keep_truthful_progress()
-> Result<(), Box<dyn std::error::Error>> {
	let source = CylinderPhysicalSource::new(
		4,
		1,
		100,
		CylinderPhysicalLimits {
			max_work: 2_000_000_000,
			..Default::default()
		},
	)?;
	let mut prepared = source.prepare()?;
	let original_work = prepared.resources().cumulative_work;
	let mut state = prepared.initial_state().to_vec();
	state[53] = 0.001;
	let request = CylinderEvolutionRequest {
		steps: 2,
		initial_condition: CylinderInitialCondition::PreparedMinimumMassCompatible,
	};
	let rejected = prepared.evolve_with_receipt(&state, request);
	assert!(rejected.outcome.is_err());
	assert_eq!(rejected.last_state, Vec::<f64>::new());
	assert_eq!(rejected.progress.drift_calls_attempted, 0);
	assert_eq!(rejected.resources.cumulative_work, original_work);
	assert!(
		serde_json::from_str::<CylinderEvolutionRequest>(
			r#"{"steps":2,"initial_condition":"SuppliedComplete","horizon":30}"#
		)
		.is_err()
	);
	let supplied = prepared.evolve_with_receipt(
		&state,
		CylinderEvolutionRequest {
			initial_condition: CylinderInitialCondition::SuppliedComplete,
			..request
		},
	);
	supplied.outcome?;
	assert_eq!(supplied.last_state.len(), 54);
	let mut prepared = source.prepare()?;
	let bad = vec![1e308; 54];
	let failed = prepared.evolve_with_receipt(
		&bad,
		CylinderEvolutionRequest {
			initial_condition: CylinderInitialCondition::SuppliedComplete,
			..request
		},
	);
	assert!(failed.outcome.is_err());
	assert_eq!(failed.last_state, bad);
	assert_eq!(failed.progress.completed_steps, 0);
	assert_eq!(
		failed.progress.failure_phase,
		Some("initial physical energy")
	);
	assert_eq!(failed.energy_calls_attempted, 1);
	assert_eq!(failed.progress.drift_calls_attempted, 0);
	assert!(failed.resources.cumulative_work > original_work);
	Ok(())
}
#[test]
fn declared_extra_owner_and_integration_reserve_reject_before_callbacks()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::physical_space::PhysicalMeshLimits;
	let limits = CylinderPhysicalLimits {
		max_work: 2_000_000_000,
		..Default::default()
	};
	let source = CylinderPhysicalSource::new(4, 1, 100, limits)?;
	let tight = CylinderPhysicalSource::new(
		4,
		1,
		100,
		CylinderPhysicalLimits {
			max_bytes: source.resources().peak_bytes,
			..limits
		},
	)?;
	let mut prepared = tight.prepare()?;
	let initial = prepared.initial_state().to_vec();
	let request = CylinderEvolutionRequest {
		steps: 8,
		initial_condition: CylinderInitialCondition::PreparedMinimumMassCompatible,
	};
	let attempt = prepared.evolve_with_receipt(&initial, request);
	assert!(attempt.outcome.is_err());
	assert_eq!(attempt.progress.drift_calls_attempted, 0);
	assert_eq!(attempt.last_state, Vec::<f64>::new());
	let owner = CylinderPhysicalSource::new(
		4,
		1,
		100,
		CylinderPhysicalLimits {
			mesh: PhysicalMeshLimits {
				external_retained_bytes: 64 * 1024 * 1024,
				..Default::default()
			},
			..limits
		},
	)?;
	let mut prepared = owner.prepare()?;
	let initial = prepared.initial_state().to_vec();
	let attempt = prepared.evolve_with_receipt(&initial, request);
	assert!(attempt.outcome.is_err());
	assert_eq!(attempt.progress.drift_calls_attempted, 0);
	assert_eq!(attempt.last_state, Vec::<f64>::new());
	Ok(())
}
