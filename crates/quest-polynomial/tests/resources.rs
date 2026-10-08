#![allow(
	clippy::panic_in_result_fn,
	reason = "Regression tests assert independently observable resource and numerical behavior"
)]
use quest_polynomial::{Complex64, Monomial, OperationResources, Polynomial};

#[test]
fn shared_polynomial_storage_is_charged_once_and_evaluation_work_is_cumulative()
-> Result<(), Box<dyn std::error::Error>> {
	let resources = OperationResources::default();
	let polynomial = Polynomial::new_with_resources(
		Monomial,
		vec![Complex64::new(1.0, 0.0), Complex64::new(2.0, 0.0)],
		resources.clone(),
	)?;
	let shared = polynomial.clone();
	assert_eq!(resources.report().live_bytes, 32);
	let before = resources.report().work_units;
	assert_eq!(
		polynomial.evaluate(Complex64::new(3.0, 0.0))?,
		Complex64::new(7.0, 0.0)
	);
	let after = resources.report().work_units;
	assert!(after > before);
	shared.evaluate(Complex64::new(2.0, 0.0))?;
	assert!(resources.report().work_units > after);
	drop(polynomial);
	assert_eq!(resources.report().live_bytes, 32);
	drop(shared);
	assert_eq!(resources.report().live_bytes, 0);
	Ok(())
}

#[test]
fn quadratic_derivative_exhaustion_precedes_its_workspace_allocation()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_polynomial::{Jacobi, OperationLimits};
	let mut limits = OperationLimits::default();
	limits.resources.max_work_units = 100;
	let resources = OperationResources::from_limits(limits);
	let polynomial = Polynomial::new_with_resources(
		Jacobi::new(0.0, 0.0)?,
		vec![Complex64::new(1.0, 0.0); 11],
		resources.clone(),
	)?;
	let before = resources.report();
	assert!(polynomial.derivative().is_err());
	let after = resources.report();
	assert_eq!(after.reservations, before.reservations);
	assert_eq!(after.work_units, 11);
	assert_eq!(after.requested_work_units, 111);
	Ok(())
}

#[test]
fn repeated_norm_covers_share_work_and_release_their_workspaces()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_polynomial::{Interval, NormDomain, NormOptions, OperationLimits};
	let mut limits = OperationLimits::default();
	limits.resources.max_work_units = 300;
	let resources = OperationResources::from_limits(limits);
	let polynomial = Polynomial::new_with_resources(
		Monomial,
		vec![Complex64::new(0.1, 0.0), Complex64::new(0.2, 0.0)],
		resources.clone(),
	)?;
	let options = NormOptions {
		max_cells: 2,
		..NormOptions::default()
	};
	polynomial.certify_norm(NormDomain::RealInterval(Interval::new(-1.0, 1.0)?), options)?;
	assert_eq!(resources.report().live_bytes, 32);
	assert!(
		polynomial
			.certify_norm(NormDomain::RealInterval(Interval::new(-1.0, 1.0)?), options)
			.is_err()
	);
	Ok(())
}

#[test]
fn rejected_polynomial_shape_is_reported_before_ownership_or_work_admission() {
	use quest_polynomial::OperationLimits;
	let mut limits = OperationLimits::default();
	limits.shapes.max_coefficients = 1;
	let resources = OperationResources::from_limits(limits);
	assert!(
		Polynomial::new_with_resources(
			Monomial,
			vec![Complex64::new(1.0, 0.0); 2],
			resources.clone()
		)
		.is_err()
	);
	assert_eq!(
		resources.report().last_rejection,
		Some(quest_numerics::ResourceError::Limit {
			resource: "coefficients",
			requested: 2,
			limit: 1
		})
	);
	assert_eq!(resources.report().work_units, 0);
	assert_eq!(resources.report().peak_bytes, 0);
}
