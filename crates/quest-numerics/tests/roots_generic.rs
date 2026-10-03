#![allow(
	clippy::many_single_char_names,
	reason = "Conventional symbols in analytic root fixtures"
)]
#![allow(
	clippy::unwrap_used,
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	clippy::float_cmp,
	clippy::panic,
	clippy::panic_in_result_fn,
	reason = "Bounded analytic fixtures intentionally use exact assertions and fail immediately"
)]
use quest_numerics::Interval;
use quest_numerics::{arithmetic::*, roots::*};
fn square_minus_one(
	b: &mut Interval64Backend,
	x: &Interval,
) -> Result<First<Interval>, ArithmeticError> {
	let square = b.mul(*x, *x)?;
	let one = b.point(1.0)?;
	let two = b.point(2.0)?;
	Ok(First {
		value: b.sub(square, one)?,
		first: b.mul(two, *x)?,
	})
}
#[test]
fn split_and_budget_preserve_both_roots() {
	let mut b = Interval64Backend;
	let x = Interval::new(-2.0, 2.0).unwrap();
	let r = cover(
		&mut b,
		x,
		square_minus_one,
		CoverLimits {
			max_iterations: 1,
			..Default::default()
		},
		&1e-10,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(!r.unresolved.is_empty());
	for v in [-1.0, 1.0] {
		assert!(r.unresolved.iter().any(|x| x.contains(v)));
	}
}
#[test]
fn derivative_exclusion_is_only_at_most_one() {
	let mut b = Interval64Backend;
	let x = Interval::new(1.0, 2.0).unwrap();
	let c = b.point(1.5).unwrap();
	let r = newton(
		&mut b,
		x,
		c,
		|b, x| {
			let offset = b.point(3.5)?;
			Ok(First {
				value: b.add(*x, offset)?,
				first: b.point(1.0)?,
			})
		},
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(r.images.is_empty());
	assert!(!r.evidence.exists);
}
#[test]
fn vector_linear_system_is_unique_and_singular_is_not() {
	let mut b = Interval64Backend;
	let x = [Interval::new(-2.0, 2.0).unwrap(); 2];
	let c = [Interval::point(0.0).unwrap(); 2];
	let identity = quest_numerics::shapes::Matrix::from_rows([
		[Interval::point(1.0).unwrap(), Interval::point(0.0).unwrap()],
		[Interval::point(0.0).unwrap(), Interval::point(1.0).unwrap()],
	])
	.unwrap();
	let evaluate = |b: &mut Interval64Backend, v: &[Interval; 2]| {
		Ok(VectorEvaluation {
			value: [
				b.sub(v[0], Interval::point(1.0).unwrap())?,
				b.add(v[1], Interval::point(1.0).unwrap())?,
			],
			jacobian: identity.clone(),
		})
	};
	let r = vector_hansen_sengupta(
		&mut b,
		x,
		c,
		evaluate,
		&identity,
		CoverLimits::default(),
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(r.evidence.unique());
	assert_eq!(r.images.len(), 1);
	let zero =
		quest_numerics::shapes::Matrix::from_rows([[Interval::point(0.0).unwrap(); 2]; 2]).unwrap();
	let r = vector_hansen_sengupta(
		&mut b,
		x,
		c,
		evaluate,
		&zero,
		CoverLimits::default(),
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(!r.evidence.unique());
	assert_eq!(r.images.len(), 1);
}
#[test]
fn backend_failure_retains_partial_cover() {
	let mut b = Interval64Backend;
	let x = Interval::new(-1.0, 1.0).unwrap();
	let r = cover(
		&mut b,
		x,
		|_, _| Err(ArithmeticError::Budget("test arithmetic")),
		CoverLimits::default(),
		&1e-10,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(r.failure.is_some());
	assert_eq!(r.unresolved.len(), 1);
	assert!(r.unresolved[0].contains(-1.0) && r.unresolved[0].contains(1.0));
}
#[test]
fn mp_root_cover_certifies_below_binary64_range() {
	let precision = Precision {
		bits: 512,
		..Default::default()
	};
	let mut b = MpIntervalBackend::new(precision).unwrap();
	let mut p = MpBackend::new(precision).unwrap();
	let tolerance = p
		.constant(&ExactConstant::Decimal("1e-1100".into()))
		.unwrap();
	let lo = b.point(0.0).unwrap();
	let hi = b
		.constant(&ExactConstant::Decimal("1e-999".into()))
		.unwrap();
	let input = b.hull(&lo, &hi).unwrap();
	let r = cover(
		&mut b,
		input,
		|b, x| {
			let target = b.constant(&ExactConstant::Decimal("1e-1000".into()))?;
			Ok(First {
				value: b.sub(x.clone(), target)?,
				first: b.point(1.0)?,
			})
		},
		CoverLimits::default(),
		&tolerance,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(r.failure.is_none());
	assert!(r.complete());
	assert_eq!(r.covered.len(), 1);
}
#[test]
fn endpoint_and_repeated_roots_are_distinguished() {
	let mut b = Interval64Backend;
	let input = Interval::new(0.0, 1.0).unwrap();
	let linear = cover(
		&mut b,
		input,
		|b, x| {
			Ok(First {
				value: *x,
				first: b.point(1.0)?,
			})
		},
		CoverLimits::default(),
		&1e-9,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(linear.complete());
	assert_eq!(linear.covered.len(), 1);
	let repeated = cover(
		&mut b,
		Interval::new(-1.0, 1.0).unwrap(),
		|b, x| {
			let two = b.point(2.0)?;
			Ok(First {
				value: b.mul(*x, *x)?,
				first: b.mul(two, *x)?,
			})
		},
		CoverLimits {
			max_iterations: 20,
			..Default::default()
		},
		&1e-9,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(!repeated.complete());
	assert!(repeated.unresolved.iter().any(|x| x.contains(0.0)));
}
#[test]
fn scalar_methods_and_split_branches_have_separate_evidence() {
	let mut b = Interval64Backend;
	let x = Interval::new(-2.0, 2.0).unwrap();
	let c = Interval::point(0.0).unwrap();
	let r = newton(
		&mut b,
		x,
		c,
		square_minus_one,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert_eq!(r.images.len(), 2);
	assert!(!r.evidence.unique());
	let f = |b: &mut Interval64Backend, x: &Interval| {
		let half = b.point(0.5)?;
		Ok(First {
			value: b.sub(*x, half)?,
			first: b.point(1.0)?,
		})
	};
	let p = Interval::point(1.0).unwrap();
	let r = krawczyk(
		&mut b,
		x,
		c,
		f,
		p,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(r.evidence.unique());
	let r = hansen_sengupta(
		&mut b,
		x,
		c,
		f,
		p,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(r.evidence.unique());
	let zero = Interval::point(0.0).unwrap();
	let r = krawczyk(
		&mut b,
		x,
		c,
		f,
		zero,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(!r.evidence.unique());
	assert_eq!(r.images[0].lower(), -2.0);
}
#[test]
fn vector_splits_survive_branch_and_storage_budgets() {
	use quest_numerics::shapes::Matrix;
	let mut b = Interval64Backend;
	let x = [Interval::new(-2.0, 2.0).unwrap()];
	let c = [Interval::point(0.0).unwrap()];
	let p = Matrix::from_rows([[Interval::point(1.0).unwrap()]]).unwrap();
	let f = |b: &mut Interval64Backend, x: &[Interval; 1]| {
		let e = square_minus_one(b, &x[0])?;
		Ok(VectorEvaluation {
			value: [e.value],
			jacobian: Matrix::from_rows([[e.first]])?,
		})
	};
	let r = vector_hansen_sengupta(
		&mut b,
		x,
		c,
		f,
		&p,
		CoverLimits::default(),
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert_eq!(r.images.len(), 2);
	for root in [-1.0, 1.0] {
		assert!(r.images.iter().any(|v| v[0].contains(root)));
	}
	for limits in [
		CoverLimits {
			max_boxes: 1,
			..Default::default()
		},
		CoverLimits {
			max_bytes: 0,
			..Default::default()
		},
	] {
		let r = vector_hansen_sengupta(
			&mut b,
			x,
			c,
			f,
			&p,
			limits,
			quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
		)
		.unwrap();
		assert_eq!(r.images.len(), 1);
		assert!(!r.evidence.unique());
		assert!(r.images[0][0].contains(-1.0) && r.images[0][0].contains(1.0));
	}
}
#[test]
fn coupled_vector_system_contracts_with_rounded_inverse() {
	use quest_numerics::shapes::Matrix;
	let mut b = Interval64Backend;
	let x = [Interval::new(-2.0, 2.0).unwrap(); 2];
	let c = [Interval::point(0.0).unwrap(); 2];
	let p = Matrix::from_rows([
		[
			Interval::point(2.0 / 3.0).unwrap(),
			Interval::point(-1.0 / 3.0).unwrap(),
		],
		[
			Interval::point(-1.0 / 3.0).unwrap(),
			Interval::point(2.0 / 3.0).unwrap(),
		],
	])
	.unwrap();
	let f = |b: &mut Interval64Backend, x: &[Interval; 2]| {
		let one = b.point(1.0)?;
		let two = b.point(2.0)?;
		let left = b.mul(two, x[0])?;
		let left = b.add(left, x[1])?;
		let right = b.mul(two, x[1])?;
		let right = b.add(right, x[0])?;
		Ok(VectorEvaluation {
			value: [b.sub(left, one)?, b.add(right, one)?],
			jacobian: Matrix::from_rows([[two, one], [one, two]])?,
		})
	};
	let r = vector_hansen_sengupta(
		&mut b,
		x,
		c,
		f,
		&p,
		CoverLimits::default(),
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(r.evidence.unique());
	assert!(r.images[0][0].contains(1.0));
	assert!(r.images[0][1].contains(-1.0));
}
#[test]
fn constant_zero_is_covered_as_continuum_and_constant_nonzero_is_excluded() {
	let mut b = Interval64Backend;
	let x = Interval::new(-10.0, 10.0).unwrap();
	let r = cover(
		&mut b,
		x,
		|b, _| {
			Ok(First {
				value: b.point(0.0)?,
				first: b.point(0.0)?,
			})
		},
		CoverLimits::default(),
		&1e-30,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(r.complete());
	assert_eq!(r.covered.len(), 1);
	assert!(r.covered[0].continuum);
	assert!(r.covered[0].evidence.exists);
	assert!(!r.covered[0].evidence.at_most_one);
	let r = cover(
		&mut b,
		x,
		|b, _| {
			Ok(First {
				value: b.point(1.0)?,
				first: b.point(0.0)?,
			})
		},
		CoverLimits::default(),
		&1e-30,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(r.complete());
	assert_eq!(r.excluded.len(), 1);
}
#[test]
fn repeated_root_coverage_completes_without_claiming_uniqueness() {
	let mut b = Interval64Backend;
	let input = Interval::new(-1.0, 1.0).unwrap();
	let result = cover(
		&mut b,
		input,
		|b, x| {
			let two = b.point(2.0)?;
			Ok(First {
				value: b.mul(*x, *x)?,
				first: b.mul(two, *x)?,
			})
		},
		CoverLimits::default(),
		&1e-4,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(result.complete());
	assert!(result.covered.iter().any(|v| v.interval.contains(0.0)));
	assert!(result.covered.iter().all(|v| !v.evidence.unique()));
}
#[test]
fn mp_limb_storage_is_admitted_before_callback_execution() {
	use std::cell::Cell;
	let mut point = MpBackend::new(Precision {
		bits: 16384,
		..Default::default()
	})
	.unwrap();
	let value = Binary::from_parts(
		(dashu_int::IBig::ONE << 16383) + dashu_int::IBig::ONE,
		-16383,
	);
	let mut b = MpIntervalBackend::new(Precision::default()).unwrap();
	let input = b.singleton(&value).unwrap();
	let tolerance = point.point(1e-12).unwrap();
	let called = Cell::new(false);
	let result = cover(
		&mut b,
		input,
		|_, _| {
			called.set(true);
			Err(ArithmeticError::Domain("should not execute"))
		},
		CoverLimits {
			max_bytes: 1024,
			..Default::default()
		},
		&tolerance,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(!called.get());
	assert_eq!(result.unresolved.len(), 1);
}

#[test]
fn scalar_split_admission_preserves_parent_before_committing() {
	for tolerance in [1e-10, 10.0] {
		let input = Interval::new(-2.0, 2.0).unwrap();
		let report = cover(
			&mut Interval64Backend,
			input,
			square_minus_one,
			CoverLimits {
				max_boxes: 1,
				..CoverLimits::default()
			},
			&tolerance,
			quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
		)
		.unwrap();
		assert!(!report.complete());
		assert!(report.covered.is_empty());
		assert_eq!(report.unresolved.len(), 1);
		assert!(report.unresolved[0].contains(-2.0));
		assert!(report.unresolved[0].contains(2.0));
	}
}

#[test]
fn vector_storage_admits_simultaneous_matrices_before_callback() {
	use quest_numerics::shapes::Matrix;
	const N: usize = 32;
	let zero = Interval::point(0.0).unwrap();
	let matrix = Matrix::<_, N, N>::from_vec(vec![zero; N * N]).unwrap();
	let input = [Interval::new(-2.0, 2.0).unwrap(); N];
	let mut calls = 0;
	let report = vector_hansen_sengupta(
		&mut Interval64Backend,
		input,
		[zero; N],
		|_, _| {
			calls += 1;
			Ok(VectorEvaluation {
				value: [zero; N],
				jacobian: matrix.clone(),
			})
		},
		&matrix,
		CoverLimits {
			max_boxes: 1,
			max_bytes: 2 * N * N * std::mem::size_of::<Interval>(),
			..CoverLimits::default()
		},
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert_eq!(calls, 0);
	assert_eq!(report.images.len(), 1);
	assert!(!report.evidence.exists);
}

#[test]
fn vector_returned_mp_limb_storage_is_admitted() {
	use quest_numerics::shapes::Matrix;
	let mut backend = MpIntervalBackend::new(Precision {
		bits: 64,
		..Precision::default()
	})
	.unwrap();
	let mut wide = MpIntervalBackend::new(Precision {
		bits: 65536,
		..Precision::default()
	})
	.unwrap();
	let lo = backend.point(-2.0).unwrap();
	let hi = backend.point(2.0).unwrap();
	let domain = backend.hull(&lo, &hi).unwrap();
	let zero = backend.point(0.0).unwrap();
	let one = backend.point(1.0).unwrap();
	let wide_value = Binary::from_parts(
		(dashu_int::IBig::ONE << 65535) + dashu_int::IBig::ONE,
		-65535,
	);
	let wide_one = wide.singleton(&wide_value).unwrap();
	let preconditioner =
		Matrix::from_rows([[one.clone(), zero.clone()], [zero.clone(), one]]).unwrap();
	let jacobian =
		Matrix::from_rows([[wide_one.clone(), zero.clone()], [zero.clone(), wide_one]]).unwrap();
	let bytes = backend.working_scalar_bytes() * 128;
	let report = vector_hansen_sengupta(
		&mut backend,
		[domain.clone(), domain.clone()],
		[zero.clone(), zero],
		|_, x| {
			Ok(VectorEvaluation {
				value: x.clone(),
				jacobian: jacobian.clone(),
			})
		},
		&preconditioner,
		CoverLimits {
			max_boxes: 1,
			max_bytes: bytes,
			..CoverLimits::default()
		},
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert_eq!(report.images.len(), 1);
	assert!(!report.evidence.exists);
	assert!(backend.same(&report.images[0][0], &domain).unwrap());
}

struct FailingMetadata {
	inner: Interval64Backend,
	fail: bool,
}
macro_rules! forward_mut {
    ($($name:ident($($arg:ident:$kind:ty),*) -> $output:ty;)*) => {$ (
        fn $name(&mut self,$($arg:$kind),*) -> Result<$output,ArithmeticError> { self.inner.$name($($arg),*) }
    )*};
}
macro_rules! forward_ref {
    ($($name:ident($($arg:ident:$kind:ty),*) -> $output:ty;)*) => {$ (
        fn $name(&self,$($arg:$kind),*) -> Result<$output,ArithmeticError> { self.inner.$name($($arg),*) }
    )*};
}
impl Backend for FailingMetadata {
	type Scalar = Interval;
	type Error = ArithmeticError;
	fn storage_bytes(&self, _: &Interval) -> Result<usize, ArithmeticError> {
		if self.fail {
			Err(ArithmeticError::Budget("test metadata"))
		} else {
			Ok(std::mem::size_of::<Interval>())
		}
	}
	forward_mut! {
		constant(x:&ExactConstant)->Interval;
		add(a:Interval,b:Interval)->Interval;
		sub(a:Interval,b:Interval)->Interval;
		mul(a:Interval,b:Interval)->Interval;
		div(a:Interval,b:Interval)->Interval;
		neg(a:Interval)->Interval;
		exp(a:Interval)->Interval;
		ln(a:Interval)->Interval;
		sqrt(a:Interval)->Interval;
		sin(a:Interval)->Interval;
		cos(a:Interval)->Interval;
	}
}
impl EnclosureBackend for FailingMetadata {
	type Endpoint = f64;
	forward_mut! {
		singleton(x:&f64)->Interval;
		hull(a:&Interval,b:&Interval)->Interval;
		intersection(a:&Interval,b:&Interval)->Option<Interval>;
		midpoint(a:&Interval)->Interval;
		bisect(a:&Interval)->Option<(Interval,Interval)>;
		width_le(a:&Interval,t:&f64)->bool;
		pi()->Interval;
	}
	forward_ref! {
		lower_endpoint(a:&Interval)->f64;
		upper_endpoint(a:&Interval)->f64;
		contains_zero(a:&Interval)->bool;
		is_zero(a:&Interval)->bool;
		strict_subset(a:&Interval,b:&Interval)->bool;
		same(a:&Interval,b:&Interval)->bool;
		magnitude_lt_one(a:&Interval)->bool;
		nonnegative(a:&Interval)->bool;
	}
}
#[test]
fn metadata_failure_retains_current_parent() {
	let input = Interval::new(-1.0, 1.0).unwrap();
	let mut backend = FailingMetadata {
		inner: Interval64Backend,
		fail: false,
	};
	let report = cover(
		&mut backend,
		input,
		|b, _| {
			b.fail = true;
			Ok(First {
				value: input,
				first: Interval::point(0.0)?,
			})
		},
		CoverLimits::default(),
		&1e-10,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(report.failure.is_some());
	assert!(!report.complete());
	assert_eq!(report.unresolved.len(), 1);
	assert!(report.unresolved[0].contains(-1.0) && report.unresolved[0].contains(1.0));
}
#[test]
fn initial_backend_budget_failure_retains_domain() {
	let input = Interval::new(-1.0, 1.0).unwrap();
	let mut inner = Interval64Backend;
	let budget = Budget::new(0);
	let mut backend = BudgetedBackend::new(&mut inner, &budget);
	let report = cover(
		&mut backend,
		input,
		|_, _| Err(ArithmeticError::Domain("callback must not run")),
		CoverLimits::default(),
		&1e-10,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)
	.unwrap();
	assert!(report.failure.is_some());
	assert_eq!(report.unresolved.len(), 1);
}
