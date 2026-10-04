#![allow(
	clippy::unwrap_used,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops
)]
use quest_cfd::resources::{ResourceRequest, estimate};

#[test]
fn full_lift_counts_beyond_machine_words_without_saturation() {
	let request = ResourceRequest {
		local_velocity_dimension: 107,
		constraint_rank: 7,
		configuration_coefficients_per_axis: 2,
		time_elements: 3,
		temporal_coefficients: 2,
		auxiliary_qubits: 4,
		statevector_budget_bytes: "18446744073709551615".into(),
	};
	let resources = estimate(&request).unwrap();
	assert_eq!(resources.independent_dimension, 100);
	assert_eq!(
		resources.configuration_dimension,
		"1267650600228229401496703205376"
	);
	assert_eq!(
		resources.history_dimension,
		"7605903601369376408980219232256"
	);
	assert_eq!(resources.data_qubits, 103);
	assert_eq!(resources.total_qubits, 107);
	assert_eq!(
		resources.statevector_bytes,
		"2596148429267413814265248164610048"
	);
	assert!(!resources.fits_statevector_budget);
}

#[test]
fn every_case_uses_its_entire_supplied_constraint_chart() {
	for id in quest_cfd::cases::family_names() {
		let manifest = quest_cfd::cases::manifest(id).unwrap();
		let local = if manifest.dimension == 2 { 12 } else { 24 };
		let request = ResourceRequest {
			local_velocity_dimension: local,
			constraint_rank: 7,
			configuration_coefficients_per_axis: 3,
			time_elements: 2,
			temporal_coefficients: 2,
			auxiliary_qubits: 2,
			statevector_budget_bytes: "0".into(),
		};
		let report = estimate(&request).unwrap();
		assert_eq!(report.independent_dimension, local - 7);
		assert!(!report.fits_statevector_budget);
		for reynolds in manifest.reynolds.iter().copied() {
			assert!(manifest.viscosity(reynolds).unwrap() > 0.);
		}
	}
}

#[test]
fn invalid_rank_and_zero_axes_are_rejected_before_allocation() {
	let mut request = ResourceRequest {
		local_velocity_dimension: 12,
		constraint_rank: 13,
		configuration_coefficients_per_axis: 2,
		time_elements: 1,
		temporal_coefficients: 1,
		auxiliary_qubits: 0,
		statevector_budget_bytes: "1024".into(),
	};
	assert!(estimate(&request).is_err());
	request.constraint_rank = 7;
	request.configuration_coefficients_per_axis = 0;
	assert!(estimate(&request).is_err());
	request.configuration_coefficients_per_axis = 2;
	request.local_velocity_dimension = usize::MAX;
	assert!(estimate(&request).is_err());
}

#[test]
fn symbolic_full_counts_survive_exponents_too_large_for_metadata_or_machine_indexing() {
	let request = ResourceRequest {
		local_velocity_dimension: 72_000_000,
		constraint_rank: 41_999_999,
		configuration_coefficients_per_axis: 4,
		time_elements: 100,
		temporal_coefficients: 2,
		auxiliary_qubits: 16,
		statevector_budget_bytes: "1073741824".into(),
	};
	let result = quest_cfd::resources::estimate_symbolic(&request).unwrap();
	assert_eq!(result.independent_dimension, 30_000_001);
	assert_eq!(result.configuration_dimension, "4^30000001");
	assert_eq!(result.data_qubits_lower, "60000010");
	assert_eq!(result.data_qubits_upper, result.data_qubits_lower);
	assert!(result.exceeds_statevector_budget);
}
