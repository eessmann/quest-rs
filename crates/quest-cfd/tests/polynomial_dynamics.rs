#![allow(
	clippy::unwrap_used,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Small exact coefficient and independent polarization references use indexed arithmetic; construction failures fail the test"
)]
use mathcore::{
	RBig,
	exact::{Owner, Symbol},
	multivariate::{PolynomialLimits, SparsePolynomial},
};
use quest_cfd::polynomial::PolynomialOde;
#[test]
fn shared_exact_forms_extract_complete_time_dependent_quadratic_drift() {
	let symbols = vec![
		Symbol::new(Owner::new(701), 0),
		Symbol::new(Owner::new(701), 1),
		Symbol::new(Owner::new(701), 2),
	];
	let limits = PolynomialLimits::default();
	let f0 = SparsePolynomial::from_terms(
		symbols.clone(),
		[
			(vec![0, 0, 1], RBig::ONE),
			(vec![1, 0, 0], RBig::from(-2)),
			(vec![1, 1, 0], RBig::from(3)),
		],
		limits,
	)
	.unwrap();
	let f1 = SparsePolynomial::from_terms(
		symbols,
		[
			(vec![0, 1, 0], RBig::from(-1)),
			(vec![2, 0, 0], RBig::from(2)),
		],
		limits,
	)
	.unwrap();
	let ode = PolynomialOde::from_polynomials(vec![f0, f1], 2, limits).unwrap();
	assert_eq!(ode.drift(0.5, &[2., -1.]).unwrap(), vec![-9.5, 9.]);
}
fn make(terms: Vec<Vec<(Vec<u32>, RBig)>>, symbols: usize) -> PolynomialOde {
	let limits = PolynomialLimits::default();
	let scope = (0..symbols)
		.map(|i| Symbol::new(Owner::new(702), u64::try_from(i).unwrap()))
		.collect::<Vec<_>>();
	let m = terms.len();
	let components = terms
		.into_iter()
		.map(|t| SparsePolynomial::from_terms(scope.clone(), t, limits).unwrap())
		.collect();
	PolynomialOde::from_polynomials(components, m, limits).unwrap()
}
#[test]
fn derivatives_polarization_and_quadratic_norm_use_actual_coefficients() {
	let ode = make(
		vec![
			vec![(vec![1, 1], RBig::from(6))],
			vec![(vec![2, 0], RBig::from(2))],
		],
		2,
	);
	assert_eq!(
		ode.jacobian(0., &[2., 3.]).unwrap(),
		vec![vec![18., 12.], vec![8., 0.]]
	);
	let zero = ode.drift(0., &[0., 0.]).unwrap();
	let ex = ode.drift(0., &[1., 0.]).unwrap();
	let ey = ode.drift(0., &[0., 1.]).unwrap();
	let both = ode.drift(0., &[1., 1.]).unwrap();
	assert_eq!(both[0] - ex[0] - ey[0] + zero[0], 6.);
	let evidence = ode.coefficient_evidence(0.1).unwrap();
	assert!(evidence.quadratic_norm_upper >= 22_f64.sqrt());
	assert!(evidence.quadratic_norm_upper < 22_f64.sqrt() + 1e-12);
	assert_eq!(evidence.quadratic_norm_lower, 3.);
}
#[test]
fn zero_forcing_nonnormal_and_mean_modes_are_not_false_certificates() {
	let zero = make(vec![vec![]], 1);
	assert!(
		zero.experimental_scaling(&[0.], 0.1)
			.unwrap()
			.scale
			.is_none()
	);
	let forced = make(vec![vec![(vec![0, 1], RBig::ONE)]], 2);
	let scaling = forced.experimental_scaling(&[0.], 0.1).unwrap();
	assert!(scaling.scale.unwrap() > 0.);
	assert!(scaling.rc_upper.is_none());
	assert!(!forced.source_identically_zero());
	let nonnormal = make(
		vec![
			vec![(vec![1, 0], RBig::from(-1)), (vec![0, 1], RBig::from(100))],
			vec![(vec![0, 1], RBig::from(-1))],
		],
		2,
	);
	assert!(
		nonnormal
			.coefficient_evidence(0.1)
			.unwrap()
			.logarithmic_norm_upper
			> 0.
	);
	assert!(
		nonnormal
			.experimental_scaling(&[0.1, 0.], 0.1)
			.unwrap()
			.rc_upper
			.is_none()
	);
	let mean = make(vec![vec![], vec![(vec![0, 1], RBig::from(-1))]], 2);
	assert_eq!(mean.dimension(), 2);
	assert!(
		mean.coefficient_evidence(0.1)
			.unwrap()
			.logarithmic_norm_upper
			>= 0.
	);
}
#[test]
fn rejects_nonquadratic_state_and_aggregate_storage_before_lowering() {
	let limits = PolynomialLimits::default();
	let x = Symbol::new(Owner::new(703), 0);
	let cubic = SparsePolynomial::from_terms(vec![x], [(vec![3], RBig::ONE)], limits).unwrap();
	assert!(PolynomialOde::from_polynomials(vec![cubic], 1, limits).is_err());
	let linear = SparsePolynomial::from_terms(vec![x], [(vec![1], RBig::ONE)], limits).unwrap();
	let tiny = PolynomialLimits {
		max_bytes: 100,
		..limits
	};
	assert!(PolynomialOde::from_polynomials(vec![linear], 1, tiny).is_err());
}
#[test]
fn polynomial_lifting_includes_its_time_derivative() {
	let limits = PolynomialLimits::default();
	let symbols = vec![
		Symbol::new(Owner::new(710), 0),
		Symbol::new(Owner::new(710), 1),
	];
	let physical = SparsePolynomial::from_terms(
		symbols.clone(),
		[(vec![1, 0], RBig::from(-1)), (vec![2, 0], RBig::ONE)],
		limits,
	)
	.unwrap();
	let ode = PolynomialOde::from_polynomials(vec![physical], 1, limits).unwrap();
	let lifting = SparsePolynomial::from_terms(symbols, [(vec![0, 1], RBig::ONE)], limits).unwrap();
	let shifted = ode.with_lifting(vec![lifting]).unwrap();
	assert_eq!(shifted.drift(0., &[0.]).unwrap(), vec![-1.]);
	assert!((shifted.drift(0.3, &[0.2]).unwrap()[0] + 1.25).abs() < 1e-14);
	assert!((shifted.jacobian(0.3, &[0.2]).unwrap()[0][0]).abs() < 1e-14);
}
#[test]
fn periodic_bdm_snapshot_retains_every_coordinate_and_reports_rounding() {
	let model = quest_cfd::PeriodicBdm1::assemble(0.01).unwrap();
	let snapshot = PolynomialOde::from_periodic_bdm1(&model, PolynomialLimits::default()).unwrap();
	assert_eq!(snapshot.dynamics.dimension(), 5);
	assert_eq!(snapshot.evidence.physical_dimension, 5);
	assert!(snapshot.evidence.independent_probe_scaled_error < 1e-12);
	assert!(snapshot.evidence.coefficient_roundoff_scale_estimate > 0.);
	let state = [0.013, -0.071, 0.027, 0.052, -0.011];
	let direct = model.drift(&state).unwrap();
	let extracted = snapshot.dynamics.drift(0., &state).unwrap();
	for (a, b) in direct.iter().zip(extracted) {
		assert!((a - b).abs() < 1e-12);
	}
}
#[test]
fn simplex_cavity_snapshot_keeps_stationary_boundary_forcing() {
	use quest_cfd::simplex::{BoxBoundary, SimplexBdm};
	let model =
		SimplexBdm::box_mesh(2, 2, 1., 0.01, BoxBoundary::Cavity { lid_speed: 1. }).unwrap();
	let snapshot = PolynomialOde::from_simplex_bdm1(&model, PolynomialLimits::default()).unwrap();
	assert_eq!(snapshot.dynamics.dimension(), model.dimension());
	assert!(!snapshot.dynamics.source_identically_zero());
	assert!(snapshot.evidence.independent_probe_scaled_error < 1e-11);
}
#[test]
fn owned_equation_capacity_is_admitted_before_lowering() {
	let limits = PolynomialLimits {
		max_bytes: 65_536,
		..PolynomialLimits::default()
	};
	let p = SparsePolynomial::constant(vec![Symbol::new(Owner::new(719), 0)], RBig::ZERO, limits)
		.unwrap();
	let mut equations = Vec::with_capacity(16_384);
	equations.push(p);
	assert!(PolynomialOde::from_polynomials(equations, 1, limits).is_err());
}

#[test]
fn mixed_quadratic_lower_bound_rounds_down_at_subnormal_halfway() {
	let smallest = f64::from_bits(1);
	let coefficient = RBig::from(3) / RBig::from(dashu_int::UBig::ONE << 1074);
	let model = make(vec![vec![(vec![1, 1], coefficient)], vec![]], 2);
	let evidence = model.coefficient_evidence(0.1).unwrap();
	// Each ordered mixed-pair entry is 1.5 least subnormals. A certified lower
	// bound must not round upward to the representable value 2 least subnormals.
	assert_eq!(evidence.quadratic_norm_lower, smallest);
}
