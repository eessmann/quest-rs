#![allow(
	clippy::unwrap_used,
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	reason = "Small independent analytic and rejection references"
)]
use mathcore::{
	RBig,
	exact::{Owner, Symbol},
	multivariate::{PolynomialLimits, SparsePolynomial},
};
use quest_cfd::{
	carleman::{CarlemanLimits, SymmetricCarleman},
	carleman_certificate::admit_truncation,
	polynomial::PolynomialOde,
};
use std::sync::Arc;
fn ode(terms: Vec<Vec<(Vec<u32>, RBig)>>, symbols: usize) -> Arc<PolynomialOde> {
	let limits = PolynomialLimits::default();
	let scope = (0..symbols)
		.map(|i| Symbol::new(Owner::new(97851), u64::try_from(i).unwrap()))
		.collect::<Vec<_>>();
	let m = terms.len();
	Arc::new(
		PolynomialOde::from_polynomials(
			terms
				.into_iter()
				.map(|terms| SparsePolynomial::from_terms(scope.clone(), terms, limits).unwrap())
				.collect(),
			m,
			limits,
		)
		.unwrap(),
	)
}
#[test]
fn admitted_bound_contains_analytic_quadratic_truncation_error() {
	let model = ode(
		vec![vec![(vec![1], RBig::from(-2)), (vec![2], RBig::ONE)]],
		1,
	);
	let initial: f64 = 0.2;
	let time: f64 = 0.3;
	let exact = 2. * initial / (2. - initial).mul_add((2. * time).exp(), initial);
	for order in 1..=5 {
		let report = admit_truncation(&model, &[initial], time, order).unwrap();
		let certificate = report.certificate.unwrap();
		let hierarchy = SymmetricCarleman::new(
			Arc::clone(&model),
			order,
			certificate.scale(),
			CarlemanLimits::default(),
		)
		.unwrap();
		let approx = hierarchy
			.recover(&hierarchy.integrate_rk4(&[initial], 0.0001, 3000).unwrap())
			.unwrap()[0];
		assert!((approx - exact).abs() < certificate.physical_error_bound());
		assert!(certificate.scale() > initial);
		assert!(certificate.rc_upper() < 1.);
	}
}
#[test]
fn corrected_forcing_and_scaling_are_checked_after_rescaling() {
	let model = ode(
		vec![vec![
			(vec![1], RBig::from(-4)),
			(vec![2], RBig::ONE),
			(vec![0], RBig::ONE / RBig::from(100)),
		]],
		1,
	);
	let report = admit_truncation(&model, &[0.02], 0.2, 3).unwrap();
	let certificate = report.certificate.unwrap();
	// 2||a0||=.04 violates c/s<=s*b. The admitted scale must change.
	assert!(certificate.scale() > 0.1);
	let hierarchy = SymmetricCarleman::new(
		Arc::clone(&model),
		3,
		certificate.scale(),
		CarlemanLimits::default(),
	)
	.unwrap();
	let approx = hierarchy
		.recover(&hierarchy.integrate_rk4(&[0.02], 0.0001, 2000).unwrap())
		.unwrap()[0];
	let physical = model.integrate_rk4(&[0.02], 0.0001, 2000).unwrap()[0];
	assert!((approx - physical).abs() < certificate.physical_error_bound());
}
#[test]
fn nonnormal_means_zero_forcing_and_time_dependence_are_not_false_certificates() {
	let nonnormal = ode(
		vec![
			vec![(vec![1, 0], RBig::from(-1)), (vec![0, 1], RBig::from(100))],
			vec![(vec![0, 1], RBig::from(-1))],
		],
		2,
	);
	assert!(
		admit_truncation(&nonnormal, &[0.1, 0.], 0.1, 2)
			.unwrap()
			.certificate
			.is_none()
	);
	let means = ode(vec![vec![], vec![(vec![0, 1], RBig::from(-1))]], 2);
	assert!(
		admit_truncation(&means, &[0.1, 0.2], 0.1, 2)
			.unwrap()
			.certificate
			.is_none()
	);
	let forced = ode(
		vec![vec![
			(vec![1], RBig::from(-4)),
			(vec![2], RBig::ONE),
			(vec![0], RBig::ONE),
		]],
		1,
	);
	assert!(
		admit_truncation(&forced, &[0.], 0.1, 2)
			.unwrap()
			.certificate
			.is_none()
	);
	assert!(
		admit_truncation(&forced, &[0.01], 0.1, 2)
			.unwrap()
			.certificate
			.is_none()
	);
	let timed = ode(
		vec![vec![(vec![1, 1], RBig::from(-4)), (vec![2, 0], RBig::ONE)]],
		2,
	);
	assert!(
		admit_truncation(&timed, &[0.1], 0.1, 2)
			.unwrap()
			.certificate
			.is_none()
	);
	assert!(admit_truncation(&means, &[0.1], 0.1, 2).is_err());
	assert!(admit_truncation(&means, &[0.1, 0.2], 0.1, 0).is_err());
}
