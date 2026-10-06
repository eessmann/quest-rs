#![feature(const_trait_impl)]
#![allow(
	clippy::arithmetic_side_effects,
	reason = "Operator syntax intentionally constructs fallible ordered expression nodes"
)]
use googletest::prelude::*;
use mathcore::{
	RBig,
	exact::{Context, Limits, Owner},
	multivariate::{PolynomialLimits, SparsePolynomial},
};

#[gtest]
fn exact_affine_and_sparse_polynomial_have_one_scoped_symbol_identity() -> Result<()> {
	let owner = Owner::new(17);
	let context = Context::with_limits(owner, Limits::default())?;
	let symbol = mathcore::exact::Symbol::new(owner, 0);
	let x = SparsePolynomial::variable(vec![symbol], 0, PolynomialLimits::default())?;
	let x2 = x.multiply(&x)?;
	let derivative = x2.differentiate(symbol)?;
	verify_eq!(derivative.terms().count(), 1)?;
	let (_, coefficient) = derivative
		.terms()
		.next()
		.ok_or_else(|| std::io::Error::other("missing derivative"))?;
	verify_eq!(coefficient, &RBig::from(2))?;
	let affine = context.symbol(symbol)?;
	verify_eq!(
		*affine
			.terms()
			.next()
			.ok_or_else(|| std::io::Error::other("missing affine"))?
			.0,
		symbol
	)?;
	Ok(())
}

#[gtest]
fn polynomial_expansion_budget_is_charged_before_exact_cancellation() -> Result<()> {
	let owner = Owner::new(18);
	let symbol = mathcore::exact::Symbol::new(owner, 0);
	let limits = PolynomialLimits {
		max_work: 3,
		..PolynomialLimits::default()
	};
	let x = SparsePolynomial::variable(vec![symbol], 0, limits)?;
	verify_that!(x.multiply(&x), err(anything()))?;
	Ok(())
}

#[gtest]
fn typed_dynamic_bridge_and_derivative_retain_domain_obligations() -> Result<()> {
	use mathcore::{
		dynamic::{DynamicExpression, ExpressionLimits},
		scalar::BinaryOperation,
		typed::variable,
	};
	let symbol = mathcore::exact::Symbol::new(Owner::new(19), 0);
	let x = variable::<0>();
	let expression =
		DynamicExpression::from_typed(&(x / x), &[symbol], ExpressionLimits::default())?;
	let derivative = expression.differentiate(symbol)?;
	let mut backend = TestBackend::default();
	let kernel = derivative.lower(&mut backend, &[symbol])?;
	verify_that!(kernel.evaluate(&mut backend, &[0.0]), err(anything()))?;
	verify_eq!(kernel.evaluate(&mut backend, &[2.0])?, 0.0)?;
	let zero = DynamicExpression::constant(
		mathcore::arithmetic::ExactConstant::Binary64(-0.0),
		ExpressionLimits::default(),
	)?;
	let value = zero.lower(&mut backend, &[])?.evaluate(&mut backend, &[])?;
	verify_eq!(value.to_bits(), (-0.0_f64).to_bits())?;
	let _ = BinaryOperation::Divide;
	Ok(())
}

#[derive(Clone, Copy)]
struct TestBackend {
	working: usize,
	reject_constants: bool,
}
impl Default for TestBackend {
	fn default() -> Self {
		Self {
			working: size_of::<f64>(),
			reject_constants: false,
		}
	}
}
impl mathcore::arithmetic::Backend for TestBackend {
	type Scalar = f64;
	type Error = mathcore::arithmetic::ArithmeticError;
	fn working_scalar_bytes(&self) -> usize {
		self.working
	}
	fn constant(
		&mut self,
		value: &mathcore::arithmetic::ExactConstant,
	) -> std::result::Result<f64, Self::Error> {
		use mathcore::arithmetic::ExactConstant;
		if self.reject_constants {
			return Err(Self::Error::Domain("constant should not be lowered"));
		}
		match value {
			ExactConstant::Binary64(v) => mathcore::scalar::finite(*v),
			ExactConstant::Integer(v) => Ok(v
				.to_string()
				.parse()
				.map_err(|_| Self::Error::Interchange("test integer"))?),
			ExactConstant::Ratio {
				numerator,
				denominator,
			} => {
				let a = numerator
					.parse()
					.map_err(|_| Self::Error::Interchange("test ratio"))?;
				let b = denominator
					.parse()
					.map_err(|_| Self::Error::Interchange("test ratio"))?;
				mathcore::scalar::binary(a, mathcore::scalar::BinaryOperation::Divide, b)
			}
			_ => Err(Self::Error::Domain("test constant")),
		}
	}
	fn add(&mut self, a: f64, b: f64) -> std::result::Result<f64, Self::Error> {
		mathcore::scalar::binary(a, mathcore::scalar::BinaryOperation::Add, b)
	}
	fn sub(&mut self, a: f64, b: f64) -> std::result::Result<f64, Self::Error> {
		mathcore::scalar::binary(a, mathcore::scalar::BinaryOperation::Subtract, b)
	}
	fn mul(&mut self, a: f64, b: f64) -> std::result::Result<f64, Self::Error> {
		mathcore::scalar::binary(a, mathcore::scalar::BinaryOperation::Multiply, b)
	}
	fn div(&mut self, a: f64, b: f64) -> std::result::Result<f64, Self::Error> {
		mathcore::scalar::binary(a, mathcore::scalar::BinaryOperation::Divide, b)
	}
	fn neg(&mut self, a: f64) -> std::result::Result<f64, Self::Error> {
		mathcore::scalar::unary(a, mathcore::scalar::UnaryOperation::Negate)
	}
	fn exp(&mut self, a: f64) -> std::result::Result<f64, Self::Error> {
		mathcore::scalar::unary(a, mathcore::scalar::UnaryOperation::Exp)
	}
	fn ln(&mut self, a: f64) -> std::result::Result<f64, Self::Error> {
		mathcore::scalar::unary(a, mathcore::scalar::UnaryOperation::Ln)
	}
	fn sin(&mut self, a: f64) -> std::result::Result<f64, Self::Error> {
		mathcore::scalar::unary(a, mathcore::scalar::UnaryOperation::Sin)
	}
	fn cos(&mut self, a: f64) -> std::result::Result<f64, Self::Error> {
		mathcore::scalar::unary(a, mathcore::scalar::UnaryOperation::Cos)
	}
	fn sqrt(&mut self, a: f64) -> std::result::Result<f64, Self::Error> {
		mathcore::scalar::unary(a, mathcore::scalar::UnaryOperation::Sqrt)
	}
}

#[gtest]
fn zero_terms_cannot_hide_large_imported_capacity() -> Result<()> {
	let symbol = mathcore::exact::Symbol::new(Owner::new(21), 0);
	let mut powers = Vec::with_capacity(1024);
	powers.push(0);
	let limits = PolynomialLimits {
		max_bytes: 1024,
		..PolynomialLimits::default()
	};
	verify_that!(
		SparsePolynomial::from_terms(vec![symbol], [(powers, RBig::ZERO)], limits),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn exact_constant_kinds_and_polynomial_limits_remain_explicit() -> Result<()> {
	use mathcore::{
		arithmetic::ExactConstant,
		dynamic::{DynamicExpression, ExpressionLimits},
		multivariate::rational_constant,
	};
	let limits = PolynomialLimits::default();
	verify_eq!(
		rational_constant(&ExactConstant::Decimal("0.1".into()), limits)?,
		rational_constant(&ExactConstant::Rational(1, 10), limits)?
	)?;
	verify_ne!(
		rational_constant(&ExactConstant::Binary64(0.1), limits)?,
		rational_constant(&ExactConstant::Decimal("0.1".into()), limits)?
	)?;
	let pi = DynamicExpression::constant(ExactConstant::Pi, ExpressionLimits::default())?;
	verify_that!(pi.polynomial(&[], limits), err(anything()))?;
	let symbol = mathcore::exact::Symbol::new(Owner::new(22), 0);
	for limited in [
		PolynomialLimits {
			max_degree: 0,
			..limits
		},
		PolynomialLimits {
			max_terms: 0,
			..limits
		},
		PolynomialLimits {
			max_coefficient_bits: 0,
			..limits
		},
		PolynomialLimits {
			max_bytes: 1,
			..limits
		},
	] {
		verify_that!(
			SparsePolynomial::variable(vec![symbol], 0, limited),
			err(anything())
		)?;
	}
	let dynamic_limits = ExpressionLimits {
		max_nodes: 2,
		..ExpressionLimits::default()
	};
	let x = DynamicExpression::variable(symbol, dynamic_limits)?;
	verify_that!(
		x.binary(mathcore::scalar::BinaryOperation::Multiply, &x),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn dynamic_composition_cannot_bypass_stricter_coefficient_admission() -> Result<()> {
	use mathcore::{
		arithmetic::ExactConstant,
		dynamic::{DynamicExpression, ExpressionLimits},
		scalar::BinaryOperation,
	};
	let small = ExpressionLimits {
		polynomial: PolynomialLimits {
			max_coefficient_bits: 4,
			..PolynomialLimits::default()
		},
		..ExpressionLimits::default()
	};
	let symbol = mathcore::exact::Symbol::new(Owner::new(23), 0);
	let x = DynamicExpression::variable(symbol, small)?;
	let large =
		DynamicExpression::constant(ExactConstant::Integer(1024), ExpressionLimits::default())?;
	verify_that!(x.binary(BinaryOperation::Add, &large), err(anything()))?;
	Ok(())
}

#[gtest]
fn exact_decimal_grammar_matches_numerical_backend_admission() -> Result<()> {
	use mathcore::{arithmetic::ExactConstant, multivariate::rational_constant};
	for text in ["1_0", "1e+", ".", "+", "1e1e1"] {
		verify_that!(
			rational_constant(
				&ExactConstant::Decimal(text.into()),
				PolynomialLimits::default()
			),
			err(anything())
		)?;
	}
	Ok(())
}

#[gtest]
fn dynamic_kernel_storage_is_admitted_before_lowering_constants() -> Result<()> {
	use mathcore::{
		arithmetic::{ArithmeticError, ExactConstant},
		dynamic::{DynamicExpression, ExpressionLimits},
	};
	let limits = ExpressionLimits {
		max_bytes: 1024,
		..ExpressionLimits::default()
	};
	let expression = DynamicExpression::constant(ExactConstant::Integer(1), limits)?;
	let mut backend = TestBackend {
		working: 4096,
		reject_constants: true,
	};
	verify_eq!(
		expression.lower(&mut backend, &[]).err(),
		Some(ArithmeticError::Budget("expression kernel storage"))
	)?;
	Ok(())
}

#[gtest]
fn duplicate_monomials_keep_original_key_capacity_accounted() -> Result<()> {
	let symbol = mathcore::exact::Symbol::new(Owner::new(25), 0);
	let make = || {
		let mut powers = Vec::with_capacity(128);
		powers.push(1);
		powers
	};
	let single = SparsePolynomial::from_terms(
		vec![symbol],
		[(make(), RBig::ONE)],
		PolynomialLimits::default(),
	)?;
	let duplicate = SparsePolynomial::from_terms(
		vec![symbol],
		[(make(), RBig::ONE), (vec![1], RBig::ONE)],
		PolynomialLimits::default(),
	)?;
	verify_eq!(single.retained_bytes()?, duplicate.retained_bytes()?)?;
	Ok(())
}

#[gtest]
fn prepared_polynomial_accounts_retained_capacity_and_unused_coordinates() -> Result<()> {
	let symbols = (0..8)
		.map(|i| mathcore::exact::Symbol::new(Owner::new(92), i))
		.collect::<Vec<_>>();
	let mut powers = Vec::with_capacity(64);
	powers.extend([1, 0, 0, 0, 0, 0, 0, 0]);
	let p = SparsePolynomial::from_terms(
		symbols,
		vec![(powers, RBig::from(3))],
		PolynomialLimits::default(),
	)?;
	let mut backend = TestBackend::default();
	let kernel = p.lower(&mut backend)?;
	verify_eq!(kernel.variables(), 8)?;
	verify_that!(
		kernel.retained_bytes()?,
		ge(size_of::<mathcore::multivariate::PolynomialKernel<f64>>()
			+ size_of::<(Vec<u32>, f64)>()
			+ 64 * size_of::<u32>())
	)?;
	verify_that!(kernel.evaluate(&mut backend, &[2.; 7]), err(anything()))?;
	verify_eq!(kernel.evaluate(&mut backend, &[2.; 8])?, 6.)?;
	Ok(())
}
