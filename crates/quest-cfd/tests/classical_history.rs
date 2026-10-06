#![allow(
	clippy::panic_in_result_fn,
	clippy::arithmetic_side_effects,
	reason = "Independent analytic reference and bounded admission tests"
)]
use quest_cfd::{
	classical_history::{ReferenceBudget, solve_reference},
	history::HistorySystem,
};
use quest_numerics::{Complex64, SparseFormat, SparseLimits, SparseMatrix};
use std::sync::Arc;
#[test]
fn complex_constant_history_is_recovered_and_budget_checked()
-> Result<(), Box<dyn std::error::Error>> {
	let zero =
		SparseMatrix::from_triplets(2, 2, SparseFormat::Csr, vec![], SparseLimits::default())?;
	let initial = [Complex64::new(1., -0.5), Complex64::new(-0.2, 2.)];
	for order in [1, 2] {
		let history =
			HistorySystem::assemble(&zero, &initial, 0.1, 3, order, SparseLimits::default())?;
		let result = solve_reference(&history, ReferenceBudget::default())?;
		assert!(result.relative_residual < 1e-13);
		for (actual, expected) in result.solution.iter().zip(initial.iter().cycle()) {
			assert!((*actual - *expected).norm() < 1e-13);
		}
		assert!(
			solve_reference(
				&history,
				ReferenceBudget {
					max_bytes: 1,
					..ReferenceBudget::default()
				}
			)
			.is_err()
		);
		assert!(
			solve_reference(
				&history,
				ReferenceBudget {
					max_work: 1,
					..ReferenceBudget::default()
				}
			)
			.is_err()
		);
	}
	Ok(())
}

#[test]
fn burgers_history_refines_time_at_fixed_full_physical_and_lift_dimensions()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::{
		burgers::BurgersDg,
		carleman::{CarlemanLimits, SymmetricCarleman},
	};
	let model = BurgersDg::new(4, 1, 0.1)?;
	let initial = model.initial_state(0.01)?;
	let ode = model.polynomial_ode();
	let scale = ode
		.experimental_scaling(&initial, 0.1)?
		.scale
		.ok_or("nonzero initial state")?;
	let hierarchy = SymmetricCarleman::new(Arc::clone(&ode), 2, scale, CarlemanLimits::default())?;
	assert_eq!(hierarchy.dimension(), 44);
	assert_eq!(hierarchy.physical_dimension(), 8);
	let reference = hierarchy.integrate_rk4(&initial, 0.000_025, 4000)?;
	let lifted: Vec<_> = hierarchy
		.lift(&initial)?
		.into_iter()
		.map(Complex64::from)
		.collect();
	for order in [1, 2] {
		let mut previous = f64::INFINITY;
		for cells in [1, 2, 4] {
			let history = HistorySystem::assemble_dynamics(
				&hierarchy,
				&lifted,
				0.1,
				cells,
				order,
				SparseLimits::default(),
			)?;
			let result = solve_reference(
				&history,
				ReferenceBudget {
					max_dimension: 1024,
					max_work: 2_000_000_000,
					..ReferenceBudget::default()
				},
			)?;
			let final_slice = result
				.solution
				.get(result.solution.len() - hierarchy.dimension()..)
				.ok_or("final history block")?;
			let error = final_slice
				.iter()
				.zip(&reference)
				.fold(0_f64, |norm, (a, b)| norm.hypot((a - b).norm()));
			eprintln!(
				"Burgers DG{order} time cells={cells}, dimension={}, final full-hierarchy error={error:e}, residual={:e}",
				history.operator().rows(),
				result.relative_residual
			);
			assert!(
				error < previous / 2.,
				"temporal refinement did not improve {previous:e} -> {error:e}"
			);
			previous = error;
		}
	}
	Ok(())
}
