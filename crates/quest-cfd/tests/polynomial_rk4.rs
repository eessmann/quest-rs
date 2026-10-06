//! Public-wrapper compatibility after extracting the shared time-aware reference loop.
#![allow(
	clippy::panic_in_result_fn,
	reason = "Independent closed-form RK4 polynomial and input-contract assertions"
)]
use mathcore::{
	RBig,
	exact::{Owner, Symbol},
	multivariate::{PolynomialLimits, SparsePolynomial},
};
use quest_cfd::polynomial::PolynomialOde;
#[test]
fn time_forcing_and_zero_step_validation_are_preserved() -> Result<(), Box<dyn std::error::Error>> {
	let limits = PolynomialLimits::default();
	let field = SparsePolynomial::from_terms(
		vec![
			Symbol::new(Owner::new(11001), 0),
			Symbol::new(Owner::new(11001), 1),
		],
		[(vec![0, 1], RBig::ONE)],
		limits,
	)?;
	let ode = PolynomialOde::from_polynomials(vec![field], 1, limits)?;
	let zero = ode.integrate_rk4(&[-0.], 1e308, 0)?;
	assert_eq!(zero.first().ok_or("state")?.to_bits(), (-0_f64).to_bits());
	let result = ode.integrate_rk4(&[1.], 0.1, 2)?;
	assert!((result.first().ok_or("state")? - 1.02).abs() < 1e-14);
	assert!(ode.integrate_rk4(&[], 0.1, 0).is_err());
	assert!(ode.integrate_rk4(&[1.], 0., 0).is_err());
	assert!(ode.integrate_rk4(&[1.], f64::INFINITY, 0).is_err());
	assert!(ode.integrate_rk4(&[1.], 0.1, 1_000_001).is_err());
	Ok(())
}
