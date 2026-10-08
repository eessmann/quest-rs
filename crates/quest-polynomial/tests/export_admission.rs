#![allow(
	clippy::panic_in_result_fn,
	reason = "Mathematical regression assertions intentionally fail the test"
)]
//! Candidate arithmetic may be custom. Frozen exports still require the audited
//! enclosing backend to agree with every actual external coefficient.
#![allow(clippy::float_cmp, clippy::arithmetic_side_effects)]
use quest_numerics::arithmetic::{
	ArithmeticError, Backend, ExactConstant, F64Backend, Interval64Backend, PointBackend,
};
use quest_polynomial::{Accuracy, DynamicShape, ExactDomain, PivotedQr, RemezRequest, function};
use std::{cell::Cell, cmp::Ordering, rc::Rc};
struct MutableConversion {
	calls: Rc<Cell<usize>>,
	forge: bool,
	zero_support: bool,
}
macro_rules! binary{($($name:ident),*)=>{$(fn $name(&mut self,a:f64,b:f64)->Result<f64,ArithmeticError>{F64Backend.$name(a,b)})*};}
macro_rules! unary{($($name:ident),*)=>{$(fn $name(&mut self,a:f64)->Result<f64,ArithmeticError>{F64Backend.$name(a)})*};}
impl Backend for MutableConversion {
	fn profile(&self) -> mathcore::arithmetic::ArithmeticProfile {
		mathcore::arithmetic::ArithmeticProfile::new("MutableConversion", 53, "fixed")
	}
	type Scalar = f64;
	type Error = ArithmeticError;
	fn constant(&mut self, c: &ExactConstant) -> Result<f64, ArithmeticError> {
		if self.forge && self.calls.get() > 0 && matches!(c, ExactConstant::Binary64(0.5)) {
			return Ok(0.25);
		}
		F64Backend.constant(c)
	}
	binary!(add, sub, mul, div);
	unary!(neg, exp, ln, sin, cos, sqrt);
}
impl PointBackend for MutableConversion {
	fn compare(&self, a: &f64, b: &f64) -> Result<Ordering, ArithmeticError> {
		if self.zero_support && *b == 0.0 {
			return Ok(Ordering::Equal);
		}
		F64Backend.compare(a, b)
	}
	fn to_f64(&self, x: &f64) -> Result<f64, ArithmeticError> {
		let n = self.calls.get();
		self.calls.set(n + 1);
		Ok(if self.forge {
			0.5
		} else if n == 0 {
			*x
		} else {
			*x + 1.0
		})
	}
	fn pi(&mut self) -> Result<f64, ArithmeticError> {
		F64Backend.pi()
	}
	fn epsilon(&mut self) -> Result<f64, ArithmeticError> {
		F64Backend.epsilon()
	}
	fn precision_bits(&self) -> usize {
		53
	}
}
#[test]
fn certified_payload_is_frozen_before_a_stateful_conversion_can_change()
-> Result<(), Box<dyn std::error::Error>> {
	let calls = Rc::new(Cell::new(0));
	let candidate = MutableConversion {
		calls: Rc::clone(&calls),
		forge: false,
		zero_support: false,
	};
	let report = RemezRequest::new(
		function!(|x| 0.25),
		ExactDomain::binary64(-1.0, 1.0),
		DynamicShape(1),
		candidate,
		Interval64Backend,
		PivotedQr,
	)
	.export_binary64()
	.accuracy(Accuracy::UniformError(ExactConstant::Binary64(1e-12)))
	.run()?;
	let frozen = report.binary64_polynomial()?.coefficients()[0].re;
	assert!((frozen - 0.25).abs() < 1e-12);
	assert_eq!(report.binary64_polynomial()?.coefficients()[0].re, frozen);
	assert_eq!(calls.get(), 1);
	Ok(())
}
#[test]
fn forged_reimport_cannot_certify_a_different_payload() {
	let candidate = MutableConversion {
		calls: Rc::new(Cell::new(0)),
		forge: true,
		zero_support: false,
	};
	let result = RemezRequest::new(
		function!(|x| 0.25),
		ExactDomain::binary64(-1.0, 1.0),
		DynamicShape(1),
		candidate,
		Interval64Backend,
		PivotedQr,
	)
	.export_binary64()
	.accuracy(Accuracy::UniformError(ExactConstant::Binary64(1e-12)))
	.run();
	assert!(result.is_err());
}

struct ForgedConstant;
impl<P: PointBackend<Scalar = f64, Error = ArithmeticError>> quest_polynomial::LinearSolver<P>
	for ForgedConstant
{
	fn solve(
		&self,
		_: &mut P,
		_: &[f64],
		_: &[f64],
		n: usize,
		_: quest_polynomial::Limits,
	) -> quest_polynomial::Result<quest_polynomial::linear::LinearSolution<f64>> {
		if n != 2 {
			return Err(quest_polynomial::Error::Domain);
		}
		Ok(quest_polynomial::linear::LinearSolution {
			values: vec![1.0, 0.0],
			rank: 2,
			residual: 0.0,
			relative_threshold: 1e-12,
		})
	}
}
#[test]
fn forged_support_cannot_hide_a_stored_coefficient_from_certification() {
	let candidate = MutableConversion {
		calls: Rc::new(Cell::new(0)),
		forge: false,
		zero_support: true,
	};
	let result = RemezRequest::new(
		function!(|x| 0.0),
		ExactDomain::binary64(-1.0, 1.0),
		DynamicShape(1),
		candidate,
		Interval64Backend,
		ForgedConstant,
	)
	.export_binary64()
	.accuracy(Accuracy::UniformError(ExactConstant::Binary64(1e-12)))
	.run();
	assert!(
		result.is_err(),
		"candidate-derived support metadata granted a false certificate"
	);
}
