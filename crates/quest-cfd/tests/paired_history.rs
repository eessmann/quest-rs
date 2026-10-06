//! Independent contracts for the bounded common-ensemble history comparison.
#![allow(
	clippy::panic_in_result_fn,
	reason = "Independent bounded numerical assertions intentionally fail a test"
)]
use quest_cfd::paired_history::{
	PairedHistoryLimits, PairedHistoryRequest, PairedLift, run_paired_history_row,
};

#[test]
fn complete_ensemble_moments_are_not_a_center_trajectory() -> Result<(), Box<dyn std::error::Error>>
{
	let row = run_paired_history_row(
		PairedHistoryRequest::History {
			lift: PairedLift::Carleman { order: 2 },
			time_cells: 1,
			time_order: 1,
		},
		PairedHistoryLimits::default(),
	)?;
	assert_eq!(row.physical_dimension, 5);
	assert_eq!(row.history_dimension, Some(40));
	assert_eq!(row.initial.sample_count, 243);
	assert!(row.initial.covariance_trace > 0.01);
	assert!(
		row.initial
			.moment_reconstruction_error
			.is_some_and(|error| error < 1e-12)
	);
	assert_eq!(row.observations.len(), 2);
	assert!(row.history_relative_residual.is_some_and(|r| r < 1e-10));
	assert!(!row.quantum_execution);
	assert!(!row.convergence_certified);
	Ok(())
}

#[test]
fn fixed_reference_budgets_and_unsupported_rows_reject() {
	let request = PairedHistoryRequest::History {
		lift: PairedLift::Carleman { order: 2 },
		time_cells: 1,
		time_order: 1,
	};
	for limits in [
		PairedHistoryLimits {
			max_bytes: 1,
			..Default::default()
		},
		PairedHistoryLimits {
			max_work: 0,
			..Default::default()
		},
	] {
		assert!(run_paired_history_row(request, limits).is_err());
	}
	for (order, cells, time_order) in [(1, 1, 1), (5, 1, 1), (2, 3, 1), (2, 2, 2)] {
		assert!(
			run_paired_history_row(
				PairedHistoryRequest::History {
					lift: PairedLift::Carleman { order },
					time_cells: cells,
					time_order,
				},
				PairedHistoryLimits::default(),
			)
			.is_err()
		);
	}
}

#[test]
fn fixed_fixture_rejects_silently_ignored_physics_and_scale() {
	for input in [
		r#"{"History":{"lift":"Kvn","time_cells":1,"time_order":1,"viscosity":2.0}}"#,
		r#"{"History":{"lift":"Kvn","time_cells":1,"time_order":1,"mesh":4}}"#,
		r#"{"History":{"lift":{"Carleman":{"order":2,"scale":99.0}},"time_cells":1,"time_order":1}}"#,
		r#"{"EnsembleReference":{"steps":256,"initial_center":[0,0,0,0,0]}}"#,
	] {
		assert!(serde_json::from_str::<PairedHistoryRequest>(input).is_err());
	}
}

#[test]
fn full_kvn_row_charges_diagnostic_and_distinct_assembly_work()
-> Result<(), Box<dyn std::error::Error>> {
	let request = PairedHistoryRequest::History {
		lift: PairedLift::Kvn,
		time_cells: 1,
		time_order: 1,
	};
	let row = run_paired_history_row(request, PairedHistoryLimits::default())?;
	assert_eq!(row.physical_dimension, 5);
	assert_eq!(row.history_dimension, Some(486));
	assert!(row.nonlinear_initial_action.is_some_and(|x| x > 1e-12));
	assert!(row.source_query_work > 3_000_000);
	assert_eq!(row.history_assembly_work_allowance, 1_000_000_000);
	assert!(row.history_relative_residual.is_some_and(|r| r < 1e-10));
	assert!(
		run_paired_history_row(
			request,
			PairedHistoryLimits {
				max_source_work: row
					.source_query_work
					.checked_sub(1)
					.ok_or("positive work")?,
				..Default::default()
			}
		)
		.is_err()
	);
	Ok(())
}
