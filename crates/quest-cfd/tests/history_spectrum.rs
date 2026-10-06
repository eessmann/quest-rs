#![allow(
	clippy::panic_in_result_fn,
	clippy::arithmetic_side_effects,
	reason = "Independent small spectral bounds and rejecting resource fixtures"
)]
use quest_cfd::{
	history::HistorySystem,
	history_spectrum::{ReferenceSpectrumBudget, reference_spectrum},
};
use quest_numerics::{Complex64, SparseFormat, SparseLimits, SparseMatrix};

#[test]
fn interval_reference_certifies_complex_nonnormal_dg1_and_dg2()
-> Result<(), Box<dyn std::error::Error>> {
	let limits = SparseLimits::default();
	let g = SparseMatrix::from_triplets(
		2,
		2,
		SparseFormat::Csr,
		vec![
			(0, 0, Complex64::from(-1.)),
			(0, 1, Complex64::new(0., 30.)),
			(1, 1, Complex64::from(-2.)),
		],
		limits,
	)?;
	for order in [1, 2] {
		let h = HistorySystem::assemble(
			&g,
			&[Complex64::new(1., -0.3), Complex64::new(0.2, 0.8)],
			0.1,
			2,
			order,
			limits,
		)?;
		let (bound, report) = reference_spectrum(&h, ReferenceSpectrumBudget::default())?;
		assert!(bound.lower() > 0.01);
		assert!(bound.upper() > bound.lower());
		assert!(report.residual_norm_upper < 1e-11);
		assert_eq!(report.temporal_order, order);
		assert_eq!(report.history_dimension, 4 * (order + 1));
		assert!(format!("{:?}", bound.evidence()).contains("DenseReference"));
		for policy in [
			ReferenceSpectrumBudget {
				max_dimension: 1,
				..ReferenceSpectrumBudget::default()
			},
			ReferenceSpectrumBudget {
				max_bytes: 1,
				..ReferenceSpectrumBudget::default()
			},
			ReferenceSpectrumBudget {
				max_work: 1,
				..ReferenceSpectrumBudget::default()
			},
		] {
			assert!(reference_spectrum(&h, policy).is_err());
		}
	}
	Ok(())
}

#[test]
fn known_constant_singular_value_and_complete_burgers_window()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::{
		burgers::BurgersDg,
		carleman::{CarlemanLimits, SymmetricCarleman},
	};
	let zero =
		SparseMatrix::from_triplets(1, 1, SparseFormat::Csr, vec![], SparseLimits::default())?;
	let h = HistorySystem::assemble(
		&zero,
		&[Complex64::from(1.)],
		0.1,
		1,
		1,
		SparseLimits::default(),
	)?;
	let (b, _) = reference_spectrum(&h, ReferenceSpectrumBudget::default())?;
	assert!(b.lower() <= std::f64::consts::FRAC_1_SQRT_2);
	assert!(b.upper() >= std::f64::consts::FRAC_1_SQRT_2);
	assert!(b.lower() > 0.49);
	let model = BurgersDg::new(4, 1, 0.1)?;
	let initial = model.initial_state(0.01)?;
	let ode = model.polynomial_ode();
	let scale = ode
		.experimental_scaling(&initial, 0.1)?
		.scale
		.ok_or("scale")?;
	let lift = SymmetricCarleman::new(ode, 2, scale, CarlemanLimits::default())?;
	let z: Vec<_> = lift
		.lift(&initial)?
		.into_iter()
		.map(Complex64::from)
		.collect();
	let history = HistorySystem::assemble_dynamics(&lift, &z, 0.1, 2, 1, SparseLimits::default())?;
	assert!(history.spectral_bounds(SparseLimits::default()).is_err());
	let (bounds, report) = reference_spectrum(&history, ReferenceSpectrumBudget::default())?;
	assert_eq!(report.history_dimension, 176);
	assert!(bounds.lower() > 0.1);
	eprintln!(
		"full Burgers bound={}, upper={}, residual={}, work={}, bytes={}",
		bounds.lower(),
		bounds.upper(),
		report.residual_norm_upper,
		report.modeled_work,
		report.modeled_peak_bytes
	);
	// Independent direct-history reference for the executed native fixture;
	// this result never enters the inverse circuit or its spectral checker.
	let direct = quest_cfd::classical_history::solve_reference(
		&history,
		quest_cfd::classical_history::ReferenceBudget::default(),
	)?;
	let endpoint: Vec<_> = direct
		.solution
		.iter()
		.skip(direct.solution.len() - lift.dimension())
		.map(|z| z.re)
		.collect();
	let physical = lift.recover(&endpoint)?;
	eprintln!(
		"BURGER_HISTORY_REFERENCE_JSON={}",
		serde_json::json!({
			"status":"independent bounded classical direct-history solve",
			"quantum_execution":false,"physical_coordinates":physical,
			"history_relative_residual":direct.relative_residual,
			"history_dimension":176,"physical_dimension":8,"carleman_order":2,
			"horizon":0.1,"temporal_order":1,"time_slabs":2,
			"physical_scale":lift.scale(),"spectral_certificate":report,
		})
	);
	Ok(())
}

#[test]
fn opt_in_spectral_evidence_executes_the_same_constant_inverse()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::solve::{SolveBackend, SolveBudget, solve_history_with_reference_spectrum};
	let zero =
		SparseMatrix::from_triplets(1, 1, SparseFormat::Csr, vec![], SparseLimits::default())?;
	let h = HistorySystem::assemble(
		&zero,
		&[Complex64::new(0.3, 0.8)],
		0.1,
		1,
		1,
		SparseLimits::default(),
	)?;
	let (result, report) = solve_history_with_reference_spectrum(
		&h,
		SolveBudget::default(),
		SolveBackend::ScalarReference,
		ReferenceSpectrumBudget::default(),
	)?;
	assert!(report.residual_norm_upper < 1e-12);
	assert!(result.relative_residual < 0.01);
	assert!(result.projector_response_bound.is_some());
	for z in result.solution {
		assert!((z - Complex64::new(0.3, 0.8)).norm() < 0.01);
	}
	Ok(())
}

#[test]
fn singular_stored_history_rejects_instead_of_certifying_a_pseudoinverse()
-> Result<(), Box<dyn std::error::Error>> {
	// DG1 B=[[.5-.5g,.5],[-.5,.5-.5g]], g=1+i, has determinant zero.
	let g = SparseMatrix::from_triplets(
		1,
		1,
		SparseFormat::Csr,
		vec![(0, 0, Complex64::new(1., 1.))],
		SparseLimits::default(),
	)?;
	let history = HistorySystem::assemble(
		&g,
		&[Complex64::from(1.)],
		1.,
		1,
		1,
		SparseLimits::default(),
	)?;
	assert!(reference_spectrum(&history, ReferenceSpectrumBudget::default()).is_err());
	Ok(())
}
