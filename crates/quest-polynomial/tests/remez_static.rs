#![allow(
	clippy::panic_in_result_fn,
	reason = "Mathematical regression assertions intentionally fail the test"
)]
#![feature(generic_const_exprs)]
#![allow(incomplete_features)]
use quest_numerics::arithmetic::{
	Backend, ExactConstant, F64Backend, MpBackend, MpIntervalBackend, Precision,
};
use quest_polynomial::{
	Accuracy, ExactDomain, GenericFunction, MpHouseholder, PrecisionAttempts, PrecisionPair,
	RemezOptions, RemezRequest, StaticShape, function,
};

#[test]
fn binary64_minimax_gap_is_distinct_from_uniform_error() -> Result<(), Box<dyn std::error::Error>> {
	let report =
		RemezRequest::binary64(function!(|x| x.exp()), ExactDomain::binary64(-1.0, 1.0), 3)
			.degree::<3>()
			.run()?;
	assert!(report.uniform_error().unconditional_bound().upper() < 0.006);
	assert!(report.minimax_gap().gap().upper() <= 1e-8);
	assert!(report.minimax_gap().lower_bound().lower() > 0.0055);
	assert!((report.target().evaluate(&mut F64Backend, 0.5)? - 0.5_f64.exp()).abs() < 1e-15);
	Ok(())
}
fn mp_options() -> RemezOptions {
	RemezOptions {
		accuracy: Accuracy::UniformError(ExactConstant::Decimal("1e-50".into())),
		root_width: ExactConstant::Decimal("1e-55".into()),
		..RemezOptions::default()
	}
}
#[test]
fn mp_certifies_below_binary64_floor_without_rounding_the_target()
-> Result<(), Box<dyn std::error::Error>> {
	let exact = quest_polynomial::typed::exact(ExactConstant::Rational(1, 3));
	let target = function!(|x| exact);
	let point = MpBackend::new(Precision::default())?;
	let enclosure = MpIntervalBackend::new(Precision::default())?;
	let report = RemezRequest::new(
		target,
		ExactDomain::binary64(-1.0, 1.0),
		StaticShape::<1>,
		point,
		enclosure,
		MpHouseholder,
	)
	.options(mp_options())
	.run()?;
	let mut check = MpBackend::new(Precision::default())?;
	let tolerance = check.constant(&ExactConstant::Decimal("1e-50".into()))?;
	assert!(report.uniform_error().unconditional_bound().upper() <= &tolerance);
	assert_eq!(report.polynomial().coefficients().len(), 1);
	Ok(())
}
#[test]
fn higher_proof_precision_cannot_hide_binary64_export_error()
-> Result<(), Box<dyn std::error::Error>> {
	let exact = quest_polynomial::typed::exact(ExactConstant::Rational(1, 3));
	let target = function!(|x| exact);
	let point = MpBackend::new(Precision::default())?;
	let enclosure = MpIntervalBackend::new(Precision {
		bits: 512,
		..Precision::default()
	})?;
	let result = RemezRequest::new(
		target,
		ExactDomain::binary64(-1.0, 1.0),
		StaticShape::<1>,
		point,
		enclosure,
		MpHouseholder,
	)
	.options(mp_options())
	.export_binary64()
	.run();
	let Err(failure) = result else {
		return Err("uncertifiable binary64 export was accepted".into());
	};
	assert!(
		failure
			.attempts()
			.last()
			.is_some_and(|attempt| attempt.candidate.is_some())
	);
	assert!(failure.request().configuration().export_binary64);
	Ok(())
}
#[test]
fn explicit_precision_attempts_keep_the_original_owned_function()
-> Result<(), Box<dyn std::error::Error>> {
	let exact = quest_polynomial::typed::exact(ExactConstant::Rational(1, 3));
	let target = function!(|x| exact);
	let point = MpBackend::new(Precision::default())?;
	let enclosure = MpIntervalBackend::new(Precision::default())?;
	let report = RemezRequest::new(
		target,
		ExactDomain::binary64(-1.0, 1.0),
		StaticShape::<1>,
		point,
		enclosure,
		MpHouseholder,
	)
	.options(mp_options())
	.run_with_precisions(PrecisionAttempts {
		attempts: vec![
			PrecisionPair {
				candidate_bits: 64,
				proof_bits: 64,
			},
			PrecisionPair {
				candidate_bits: 256,
				proof_bits: 256,
			},
		],
		max_total_work: 1_000_000,
	})?;
	assert_eq!(report.attempts().len(), 2);
	assert_eq!(
		report
			.attempts()
			.first()
			.map(|attempt| attempt.precision_bits),
		Some(64)
	);
	assert_eq!(
		report
			.attempts()
			.get(1)
			.map(|attempt| attempt.precision_bits),
		Some(256)
	);
	assert!(report.request().precision_policy().is_some());
	Ok(())
}

#[test]
fn mp_nonconstant_minimax_gap_uses_mp_critical_point_enclosures()
-> Result<(), Box<dyn std::error::Error>> {
	let point = MpBackend::new(Precision::default())?;
	let enclosure = MpIntervalBackend::new(Precision::default())?;
	let options = RemezOptions {
		accuracy: Accuracy::Both {
			uniform: ExactConstant::Decimal("0.006".into()),
			gap: ExactConstant::Decimal("1e-35".into()),
		},
		root_width: ExactConstant::Decimal("1e-45".into()),
		..RemezOptions::default()
	};
	let report = RemezRequest::new(
		function!(|x| x.exp()),
		ExactDomain::binary64(-1.0, 1.0),
		StaticShape::<4>,
		point,
		enclosure,
		MpHouseholder,
	)
	.options(options)
	.run()?;
	let mut check = MpBackend::new(Precision::default())?;
	let tolerance = check.constant(&ExactConstant::Decimal("1e-35".into()))?;
	assert!(report.minimax_gap().gap().upper() <= &tolerance);
	Ok(())
}

struct FailsSecondSolve(std::cell::Cell<bool>);
impl<
	P: quest_numerics::arithmetic::PointBackend<
			Scalar = f64,
			Error = quest_numerics::arithmetic::ArithmeticError,
		>,
> quest_polynomial::LinearSolver<P> for FailsSecondSolve
{
	fn solve(
		&self,
		b: &mut P,
		m: &[f64],
		rhs: &[f64],
		n: usize,
		limits: quest_polynomial::Limits,
	) -> quest_polynomial::Result<quest_polynomial::linear::LinearSolution<f64>> {
		if self.0.replace(true) {
			return Err(quest_polynomial::Error::Budget(
				"injected next candidate failure",
			));
		}
		quest_polynomial::PivotedQr.solve(b, m, rhs, n, limits)
	}
}
#[test]
fn next_candidate_failure_retains_the_previous_candidate_and_cover()
-> Result<(), Box<dyn std::error::Error>> {
	let result = RemezRequest::new(
		function!(|x| x.exp()),
		ExactDomain::binary64(-1.0, 1.0),
		StaticShape::<4>,
		F64Backend,
		quest_numerics::arithmetic::Interval64Backend,
		FailsSecondSolve(std::cell::Cell::new(false)),
	)
	.accuracy(Accuracy::MinimaxGap(ExactConstant::Binary64(1e-30)))
	.run();
	let Err(failure) = result else {
		return Err("injected failure did not fail".into());
	};
	assert!(matches!(
		failure.error(),
		quest_polynomial::Error::Budget("injected next candidate failure")
	));
	let attempt = failure
		.attempts()
		.last()
		.ok_or("missing attempted evidence")?;
	assert!(attempt.candidate.is_some());
	assert!(
		attempt
			.coverage
			.as_ref()
			.is_some_and(quest_numerics::roots::RootCover::complete)
	);
	Ok(())
}
