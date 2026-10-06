#![allow(
	clippy::unwrap_used,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Small independent ordered and symmetric hierarchy references use direct indexed arithmetic; construction failures fail the test"
)]
use mathcore::{
	RBig,
	exact::{Owner, Symbol},
	multivariate::{PolynomialLimits, SparsePolynomial},
};
use quest_cfd::{
	carleman::{CarlemanLimits, SymmetricCarleman},
	polynomial::PolynomialOde,
};
use std::sync::Arc;
fn scalar() -> Arc<PolynomialOde> {
	let limits = PolynomialLimits::default();
	let x = Symbol::new(Owner::new(7701), 0);
	let f = SparsePolynomial::from_terms(
		vec![x],
		[(vec![1], RBig::from(-2)), (vec![2], RBig::ONE)],
		limits,
	)
	.unwrap();
	Arc::new(PolynomialOde::from_polynomials(vec![f], 1, limits).unwrap())
}
#[test]
fn degree_one_is_complete_physical_state() {
	let c = SymmetricCarleman::new(scalar(), 3, 0.4, CarlemanLimits::default()).unwrap();
	assert_eq!(c.dimension(), 3);
	let y = c.lift(&[0.2]).unwrap();
	assert_eq!(y, vec![0.5, 0.25, 0.125]);
	assert_eq!(c.recover(&y).unwrap(), vec![0.2]);
}
#[test]
fn normalized_symmetric_generator_intertwines_ordered_tensor_with_forcing_and_time() {
	use quest_cfd::carleman::OrderedCarlemanReference;
	let limits = PolynomialLimits::default();
	let symbols = (0..3)
		.map(|i| Symbol::new(Owner::new(7702), i))
		.collect::<Vec<_>>();
	let f = SparsePolynomial::from_terms(
		symbols.clone(),
		[
			(vec![0, 0, 1], RBig::ONE),
			(vec![1, 0, 0], RBig::from(-2)),
			(vec![1, 1, 0], RBig::from(3)),
		],
		limits,
	)
	.unwrap();
	let g = SparsePolynomial::from_terms(
		symbols,
		[
			(vec![0, 1, 0], RBig::from(-1)),
			(vec![2, 0, 0], RBig::from(2)),
		],
		limits,
	)
	.unwrap();
	let ode = Arc::new(PolynomialOde::from_polynomials(vec![f, g], 2, limits).unwrap());
	let s = SymmetricCarleman::new(Arc::clone(&ode), 3, 0.7, CarlemanLimits::default()).unwrap();
	let o = OrderedCarlemanReference::new(ode, 3, 0.7, CarlemanLimits::default()).unwrap();
	assert_eq!(s.dimension(), 9);
	assert_eq!(o.dimension(), 14);
	// An arbitrary hierarchy vector, not just the rank-one lifted manifold.
	let state = (0..9).map(|i| f64::from(i + 1) / 13.).collect::<Vec<_>>();
	let embedded = o.embed_symmetric(&s, &state).unwrap();
	assert!(
		(state.iter().map(|x| x * x).sum::<f64>() - embedded.iter().map(|x| x * x).sum::<f64>())
			.abs()
			< 1e-13
	);
	let left = o.drift(0.37, &embedded).unwrap();
	let right = o
		.embed_symmetric(&s, &s.drift(0.37, &state).unwrap())
		.unwrap();
	for (a, b) in left.iter().zip(right) {
		assert!((a - b).abs() < 1e-12, "{a} != {b}");
	}
}
#[test]
fn scalar_truncation_converges_and_defect_is_reported() {
	let ode = scalar();
	let reference = ode.integrate_rk4(&[0.2], 0.001, 100).unwrap()[0];
	let mut previous = f64::INFINITY;
	for order in 1..=3 {
		let c = SymmetricCarleman::new(Arc::clone(&ode), order, 0.4, CarlemanLimits::default())
			.unwrap();
		let y = c.integrate_rk4(&[0.2], 0.001, 100).unwrap();
		let error = (c.recover(&y).unwrap()[0] - reference).abs();
		assert!(error < previous / 20.);
		previous = error;
		assert!(c.reconstruction_defect(0., &[0.2]).unwrap() > 0.);
	}
}
#[test]
fn arbitrary_width_dimension_and_early_budget_rejection() {
	use dashu_int::{UBig, ops::BitTest};
	use quest_cfd::carleman::symmetric_dimension;
	assert_eq!(
		symmetric_dimension(&UBig::from(8u8), 3).unwrap(),
		UBig::from(164u16)
	);
	assert!(
		symmetric_dimension(&(UBig::ONE << 200), 3)
			.unwrap()
			.bit_len()
			> 590
	);
	let limits = CarlemanLimits {
		max_dimension: 2,
		..CarlemanLimits::default()
	};
	assert!(SymmetricCarleman::new(scalar(), 3, 0.4, limits).is_err());
}
#[test]
fn complete_burgers_hierarchy_converges_without_removing_modes() {
	use quest_cfd::burgers::BurgersDg;
	let model = BurgersDg::new(4, 1, 0.1).unwrap();
	let ode = model.polynomial_ode();
	let initial = model.initial_state(0.01).unwrap();
	let scale = ode
		.experimental_scaling(&initial, 0.1)
		.unwrap()
		.scale
		.unwrap();
	let reference = ode.integrate_rk4(&initial, 0.00005, 2000).unwrap();
	let mut previous = f64::INFINITY;
	for (order, dimension) in [(1, 8), (2, 44), (3, 164)] {
		let c = SymmetricCarleman::new(Arc::clone(&ode), order, scale, CarlemanLimits::default())
			.unwrap();
		assert_eq!(c.dimension(), dimension);
		assert_eq!(c.physical_dimension(), 8);
		let y = c.integrate_rk4(&initial, 0.00005, 2000).unwrap();
		let recovered = c.recover(&y).unwrap();
		let error = recovered
			.iter()
			.zip(&reference)
			.map(|(a, b)| (a - b).powi(2))
			.sum::<f64>()
			.sqrt();
		eprintln!(
			"Burgers lift r={order} full dimension={dimension}, reconstruction error={error}"
		);
		assert!(error < previous / 10.);
		previous = error;
	}
}
#[test]
fn external_source_matches_affine_lift_and_time_validation() {
	use quest_cfd::history::HistoryDynamics;
	use quest_numerics::Complex64;
	let limits = PolynomialLimits::default();
	let symbols = vec![
		Symbol::new(Owner::new(7703), 0),
		Symbol::new(Owner::new(7703), 1),
	];
	let f = SparsePolynomial::from_terms(symbols, [(vec![0, 1], RBig::ONE)], limits).unwrap();
	let ode = Arc::new(PolynomialOde::from_polynomials(vec![f], 1, limits).unwrap());
	let hierarchy = SymmetricCarleman::new(ode, 3, 2., CarlemanLimits::default()).unwrap();
	let mut source = vec![Complex64::new(9., 3.); 3];
	hierarchy.source(0.4, &mut source).unwrap();
	assert_eq!(
		source,
		vec![
			Complex64::new(0.2, 0.),
			Complex64::new(0., 0.),
			Complex64::new(0., 0.)
		]
	);
	let state = hierarchy.lift(&[0.6]).unwrap();
	let drift = hierarchy.drift(0.4, &state).unwrap();
	assert!((drift[0] - 0.2).abs() < 1e-14);
	assert!((drift[1] - 0.12).abs() < 1e-14);
	assert!((drift[2] - 0.054).abs() < 1e-14);
	assert!(hierarchy.reconstruction_defect(0.4, &[0.6]).unwrap() < 1e-14);
	assert!(hierarchy.source(f64::NAN, &mut source).is_err());
	assert!(
		hierarchy
			.visit_generator(f64::NAN, &mut |_, _, _| Ok(()))
			.is_err()
	);
}
#[test]
fn symbolic_degree_and_zero_work_limits_are_admitted_before_growth() {
	let limits = PolynomialLimits {
		max_degree: 2,
		..PolynomialLimits::default()
	};
	let x = Symbol::new(Owner::new(7704), 0);
	let f = SparsePolynomial::from_terms(vec![x], [(vec![2], RBig::ONE)], limits).unwrap();
	let ode = Arc::new(PolynomialOde::from_polynomials(vec![f], 1, limits).unwrap());
	// The discarded degree-three row contribution need not be constructed to build r=2.
	assert!(SymmetricCarleman::new(Arc::clone(&ode), 2, 1., CarlemanLimits::default()).is_ok());
	assert!(SymmetricCarleman::new(Arc::clone(&ode), 3, 1., CarlemanLimits::default()).is_err());
	assert!(
		SymmetricCarleman::new(
			ode,
			2,
			1.,
			CarlemanLimits {
				max_work: 0,
				..CarlemanLimits::default()
			}
		)
		.is_err()
	);
}
#[test]
fn physical_defect_detects_inconsistent_moments_separately_from_top_order_truncation() {
	let c = SymmetricCarleman::new(scalar(), 2, 2., CarlemanLimits::default()).unwrap();
	// a=1, but degree-two moment is zero. Physical drift=-1; represented drift=-2.
	assert!((c.physical_reconstruction_defect(0., &[0.5, 0.]).unwrap() - 1.).abs() < 1e-14);
	let y = c.lift(&[1.]).unwrap();
	assert!(c.physical_reconstruction_defect(0., &y).unwrap() < 1e-14);
	assert!(c.reconstruction_defect(0., &[1.]).unwrap() > 0.);
}

#[test]
fn complete_kdv_order_two_uses_sparse_recipe_storage_under_default_budget() {
	let model = quest_cfd::kdv::KdvDg::new(4, 2).unwrap();
	let initial = model.initial_state(0.05).unwrap();
	let norm = initial.iter().fold(0_f64, |n, v| n.hypot(*v));
	let hierarchy = SymmetricCarleman::new(
		model.polynomial_ode(),
		2,
		2. * norm,
		CarlemanLimits::default(),
	)
	.unwrap();
	assert_eq!(hierarchy.physical_dimension(), 24);
	assert_eq!(hierarchy.dimension(), 324);
	let lifted = hierarchy.integrate_rk4(&initial, 0.0001, 10).unwrap();
	let physical = model.integrate_rk4(&initial, 0.0001, 10).unwrap();
	let error = hierarchy
		.recover(&lifted)
		.unwrap()
		.iter()
		.zip(physical)
		.fold(0_f64, |norm, (a, b)| norm.hypot(a - b));
	assert!(error < 1e-8, "{error}");
}
