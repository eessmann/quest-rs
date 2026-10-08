//! Scope admission must reject before uniqueness or capture allocation.
#![allow(
	clippy::unwrap_used,
	reason = "Test setup and successful controls must fail the test on unexpected errors"
)]
use mathcore::{
	RBig,
	arithmetic::{ArithmeticError, Backend, ExactConstant},
	dynamic::{DynamicExpression, ExpressionLimits},
	exact::{Owner, Symbol},
	multivariate::{PolynomialLimits, SparsePolynomial},
	typed,
};
use std::cell::Cell;
#[derive(Clone, Copy, Default, Debug)]
struct AllocationStats {
	peak: usize,
	allocations: u64,
}

fn measured<T>(operation: impl FnOnce() -> T) -> (T, AllocationStats) {
	let mut result = None;
	// Finish the measurement before propagating a panic, so the external
	// thread-local counter is reusable even when the operation unwinds.
	let stats = allocation_counter::measure(|| {
		result = Some(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
			operation,
		)));
	});
	let result = result
		.unwrap()
		.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
	(
		result,
		AllocationStats {
			peak: usize::try_from(stats.bytes_max).unwrap(),
			allocations: stats.count_total,
		},
	)
}

#[test]
fn allocation_measurement_observes_count_and_peak() {
	let ((), stats) = measured(|| {
		let value = std::hint::black_box(vec![0_u8; 4096]);
		std::hint::black_box(&value);
	});
	assert!(stats.allocations > 0);
	assert!(stats.peak >= 4096);
}

fn symbols(count: u64) -> Vec<Symbol> {
	(0..count)
		.map(|index| Symbol::new(Owner::new(91), index))
		.collect()
}

#[test]
fn capture_admits_complete_scope_storage_before_allocating() {
	let scope = symbols(4096);
	let limits = ExpressionLimits {
		max_bytes: 1024,
		..ExpressionLimits::default()
	};
	let (result, stats) =
		measured(|| DynamicExpression::from_typed(&typed::variable::<0>(), &scope, limits));
	assert!(
		result.is_err(),
		"large unused scope was accepted: {stats:?}"
	);
	assert_eq!(
		stats.allocations, 0,
		"rejection allocated scope scratch/input nodes: {stats:?}"
	);
}

#[test]
fn capture_admits_aggregate_nodes_even_when_scope_scratch_fits() {
	let scope = symbols(2);
	let leaf =
		DynamicExpression::variable(*scope.first().unwrap(), ExpressionLimits::default()).unwrap();
	let fixed = leaf
		.retained_bytes()
		.checked_mul(3)
		.unwrap()
		.checked_add(size_of::<Vec<DynamicExpression>>())
		.unwrap();
	let limits = ExpressionLimits {
		max_bytes: fixed.checked_sub(1).unwrap(),
		..ExpressionLimits::default()
	};
	let (result, stats) =
		measured(|| DynamicExpression::from_typed(&typed::variable::<0>(), &scope, limits));
	assert!(result.is_err());
	assert_eq!(
		stats.allocations, 0,
		"capture grew inputs before admitting output: {stats:?}"
	);
	let limits = ExpressionLimits {
		max_bytes: fixed,
		..limits
	};
	let (result, stats) =
		measured(|| DynamicExpression::from_typed(&typed::variable::<0>(), &scope, limits));
	assert!(result.is_ok());
	assert!(stats.peak <= limits.max_bytes);
}

#[test]
fn capture_rejects_heap_constant_payload_before_cloning() {
	let mut text = String::with_capacity(4096);
	text.push('1');
	let source = typed::exact(ExactConstant::Decimal(text));
	let limits = ExpressionLimits {
		max_bytes: 1024,
		..ExpressionLimits::default()
	};
	let (result, stats) = measured(|| DynamicExpression::from_typed(&source, &[], limits));
	assert!(result.is_err());
	assert_eq!(
		stats.allocations, 0,
		"oversized source constant was cloned: {stats:?}"
	);
}
#[test]
fn capture_admits_scope_work_before_allocating() {
	let scope = symbols(4096);
	let limits = ExpressionLimits {
		max_work: 1,
		..ExpressionLimits::default()
	};
	let (result, stats) =
		measured(|| DynamicExpression::from_typed(&typed::variable::<0>(), &scope, limits));
	assert!(
		result.is_err(),
		"scope validation/construction work was free"
	);
	assert_eq!(
		stats.allocations, 0,
		"rejection allocated capture inputs: {stats:?}"
	);
}
#[test]
fn lower_admits_scope_scratch_before_allocating_or_backend_callbacks() {
	let scope = symbols(4096);
	let limits = ExpressionLimits {
		max_bytes: 1024,
		..ExpressionLimits::default()
	};
	let expression = DynamicExpression::variable(*scope.first().unwrap(), limits).unwrap();
	let mut backend = CountingBackend::default();
	let (result, stats) = measured(|| expression.lower(&mut backend, &scope));
	assert!(result.is_err(), "oversized lowering scope was accepted");
	assert_eq!(
		stats.allocations, 0,
		"rejection allocated uniqueness scratch: {stats:?}"
	);
	assert_eq!(
		backend.callbacks, 0,
		"rejected preflight called the numerical backend"
	);
}
#[test]
fn sparse_import_rejects_retained_capacity_before_uniqueness_or_terms() {
	let scope = symbols(4096);
	let calls = Cell::new(0_usize);
	let terms = std::iter::from_fn(|| {
		calls.set(calls.get().saturating_add(1));
		None::<(Vec<u32>, RBig)>
	});
	let limits = PolynomialLimits {
		max_bytes: 1,
		..PolynomialLimits::default()
	};
	let (result, stats) = measured(|| SparsePolynomial::from_terms(scope, terms, limits));
	assert!(result.is_err());
	assert_eq!(
		stats.allocations, 0,
		"late rejection allocated uniqueness scratch: {stats:?}"
	);
	assert_eq!(calls.get(), 0, "rejected import consumed terms");
}
#[test]
fn admitted_constant_capture_preserves_entire_input_validation_contract() {
	let scope = symbols(2);
	let limits = ExpressionLimits {
		max_bytes: 4096,
		max_work: 100,
		..ExpressionLimits::default()
	};
	let (result, stats) =
		measured(|| DynamicExpression::from_typed(&typed::constant(-0.0), &scope, limits));
	let expression = result.unwrap();
	assert!(
		stats.peak <= limits.max_bytes,
		"admitted capture exceeded limit: {stats:?}"
	);
	let mut backend = CountingBackend::default();
	let kernel = expression.lower(&mut backend, &scope).unwrap();
	assert_eq!(
		kernel
			.evaluate(&mut backend, &[1.0, 2.0])
			.unwrap()
			.to_bits(),
		(-0.0_f64).to_bits()
	);
	assert!(kernel.evaluate(&mut backend, &[1.0]).is_err());
	assert!(kernel.evaluate(&mut backend, &[1.0, f64::NAN]).is_err());
}
#[test]
fn polynomial_convenience_constructors_preflight_scope_work_before_exponents() {
	for variable in [false, true] {
		let scope = symbols(4096);
		let limits = PolynomialLimits {
			max_work: 1,
			..PolynomialLimits::default()
		};
		let (result, stats) = measured(|| {
			if variable {
				SparsePolynomial::variable(scope, 0, limits)
			} else {
				SparsePolynomial::constant(scope, RBig::ONE, limits)
			}
		});
		assert!(result.is_err());
		assert_eq!(
			stats.allocations, 0,
			"constructor allocated exponents before scope work admission: {stats:?}"
		);
	}
}
#[test]
fn expression_polynomial_rejects_scope_before_cloning_it() {
	let scope = symbols(4096);
	let expression =
		DynamicExpression::variable(*scope.first().unwrap(), ExpressionLimits::default()).unwrap();
	for limits in [
		PolynomialLimits {
			max_bytes: 1024,
			..PolynomialLimits::default()
		},
		PolynomialLimits {
			max_work: 1,
			..PolynomialLimits::default()
		},
	] {
		let (result, stats) = measured(|| expression.polynomial(&scope, limits));
		assert!(result.is_err());
		assert_eq!(
			stats.allocations, 0,
			"polynomial extraction cloned rejected scope: {stats:?}"
		);
	}
}
#[test]
fn capture_scope_work_boundary_is_charged_and_original_binding_order_survives() {
	let mut scope = symbols(2);
	scope.reverse();
	// 2n + 4n ceil(log2(n)) for scope validation, n input nodes, one source node.
	let limits = ExpressionLimits {
		max_work: 15,
		..ExpressionLimits::default()
	};
	let rejected = ExpressionLimits {
		max_work: 14,
		..limits
	};
	let (result, stats) =
		measured(|| DynamicExpression::from_typed(&typed::variable::<0>(), &scope, rejected));
	assert!(result.is_err());
	assert_eq!(stats.allocations, 0);
	let expression =
		DynamicExpression::from_typed(&typed::variable::<0>(), &scope, limits).unwrap();
	assert_eq!(expression.logical_work(), 15);
	// Lowering also charges original capture plus scope/search work, so retain
	// the binding-order control under its own adequate receiving policy.
	let expression =
		DynamicExpression::variable(*scope.first().unwrap(), ExpressionLimits::default()).unwrap();
	let mut backend = CountingBackend::default();
	let kernel = expression.lower(&mut backend, &scope).unwrap();
	assert_eq!(kernel.evaluate(&mut backend, &[7.0, 11.0]).unwrap(), 7.0);
}
#[test]
fn lowering_work_rejection_precedes_scope_allocation_and_backend_callbacks() {
	let scope = symbols(2);
	let expression = DynamicExpression::variable(
		*scope.first().unwrap(),
		ExpressionLimits {
			max_work: 1,
			..ExpressionLimits::default()
		},
	)
	.unwrap();
	let mut backend = CountingBackend::default();
	let (result, stats) = measured(|| expression.lower(&mut backend, &scope));
	assert!(result.is_err());
	assert_eq!(stats.allocations, 0);
	assert_eq!(backend.callbacks, 0);
}
#[test]
fn duplicate_scopes_are_rejected_with_admitted_bounded_scratch() {
	let mut scope = symbols(2);
	let first = *scope.first().unwrap();
	*scope.last_mut().unwrap() = first;
	let limits = ExpressionLimits {
		max_bytes: 4096,
		max_work: 100,
		..ExpressionLimits::default()
	};
	let (result, stats) =
		measured(|| DynamicExpression::from_typed(&typed::variable::<0>(), &scope, limits));
	assert!(matches!(
		result,
		Err(ArithmeticError::Domain("duplicate symbol scope"))
	));
	assert!(stats.peak <= limits.max_bytes);
}
#[derive(Default)]
struct CountingBackend {
	callbacks: usize,
}
impl Backend for CountingBackend {
	fn profile(&self) -> mathcore::arithmetic::ArithmeticProfile {
		mathcore::arithmetic::ArithmeticProfile::new("scope-test-f64", 53, "nearest-even")
	}
	type Scalar = f64;
	type Error = ArithmeticError;
	fn validate(&self, value: &f64) -> Result<(), Self::Error> {
		mathcore::scalar::finite(*value).map(|_| ())
	}
	fn visit(&mut self) -> Result<(), Self::Error> {
		self.callbacks = self.callbacks.saturating_add(1);
		Ok(())
	}
	fn constant(&mut self, value: &ExactConstant) -> Result<f64, Self::Error> {
		self.callbacks = self.callbacks.saturating_add(1);
		match value {
			ExactConstant::Binary64(v) => mathcore::scalar::finite(*v),
			_ => Err(ArithmeticError::Domain("test constant")),
		}
	}
	fn add(&mut self, a: f64, b: f64) -> Result<f64, Self::Error> {
		mathcore::scalar::binary(a, mathcore::scalar::BinaryOperation::Add, b)
	}
	fn sub(&mut self, a: f64, b: f64) -> Result<f64, Self::Error> {
		mathcore::scalar::binary(a, mathcore::scalar::BinaryOperation::Subtract, b)
	}
	fn mul(&mut self, a: f64, b: f64) -> Result<f64, Self::Error> {
		mathcore::scalar::binary(a, mathcore::scalar::BinaryOperation::Multiply, b)
	}
	fn div(&mut self, a: f64, b: f64) -> Result<f64, Self::Error> {
		mathcore::scalar::binary(a, mathcore::scalar::BinaryOperation::Divide, b)
	}
	fn neg(&mut self, a: f64) -> Result<f64, Self::Error> {
		mathcore::scalar::unary(a, mathcore::scalar::UnaryOperation::Negate)
	}
	fn exp(&mut self, a: f64) -> Result<f64, Self::Error> {
		mathcore::scalar::unary(a, mathcore::scalar::UnaryOperation::Exp)
	}
	fn ln(&mut self, a: f64) -> Result<f64, Self::Error> {
		mathcore::scalar::unary(a, mathcore::scalar::UnaryOperation::Ln)
	}
	fn sin(&mut self, a: f64) -> Result<f64, Self::Error> {
		mathcore::scalar::unary(a, mathcore::scalar::UnaryOperation::Sin)
	}
	fn cos(&mut self, a: f64) -> Result<f64, Self::Error> {
		mathcore::scalar::unary(a, mathcore::scalar::UnaryOperation::Cos)
	}
	fn sqrt(&mut self, a: f64) -> Result<f64, Self::Error> {
		mathcore::scalar::unary(a, mathcore::scalar::UnaryOperation::Sqrt)
	}
}
