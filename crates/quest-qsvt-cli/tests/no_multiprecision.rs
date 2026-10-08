#![cfg(all(feature = "native", feature = "certification"))]
use googletest::prelude::*;
use num_complex::Complex64 as C;
use quest::{Environment, QubitCount};
use quest_polynomial::{Chebyshev, Limits, Polynomial};
use quest_qsp::{PhaseSequence, Policy, SynthesisBuilder, WxSymmetric};
use quest_qsvt::{DenseEncodingBuilder, NumericalPolicy, TransformBuilder};

#[gtest]
fn production_failure_and_native_execution_are_independent_of_cold_verification() -> Result<()> {
	if std::env::var_os("QUEST_PRECISION_PROBE").is_none() {
		let status = std::process::Command::new(std::env::current_exe()?)
			.args([
				"--exact",
				"production_failure_and_native_execution_are_independent_of_cold_verification",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_PRECISION_PROBE", "1")
			.status()?;
		expect_true!(status.success());
		return Ok(());
	}
	let target = Polynomial::new(
		Chebyshev,
		vec![C::new(0.0, 0.0), C::new(0.2, 0.0)],
		Limits::default(),
	)?;
	let candidate = SynthesisBuilder::new()
		.real_parity_wx(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	let difficult = Polynomial::new(
		Chebyshev,
		vec![C::new(0.013_671_875, 0.0)],
		Limits::default(),
	)?;
	let failed = SynthesisBuilder::new()
		.policy(Policy {
			accuracy: quest_qsp::AccuracyPolicy {
				response_tolerance: 1e-18,
				..(Policy::default()).accuracy
			},
			..Policy::default()
		})
		.real_parity_wx(&difficult)?
		.admit()?
		.complete()?
		.synthesize();
	expect_true!(matches!(
		failed,
		Err(quest_qsp::Error::NotEstablished { .. })
	));
	let snapshot = {
		let environment = Environment::builder().build()?;
		let matrix = faer::Mat::from_fn(1, 1, |_, _| C::new(0.3, 0.4));
		let encoding = DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
			.normalization(1.0)?
			.build()?;
		let phases = PhaseSequence::<WxSymmetric>::builder(candidate.phases().to_vec()).build()?;
		let transform = TransformBuilder::new()
			.encoding(encoding)
			.standard(phases)
			.build()?;
		let width = transform.operands().num_qubits();
		let mut prepared = environment.qsvt().transform(transform).prepare()?;
		let mut register = environment.state_vector(QubitCount::new(width)?)?;
		let bytes = environment.allocated_bytes();
		for _ in 0..8 {
			register.init_zero()?;
			let _ = prepared.run(&mut register)?.release();
			expect_eq!(environment.allocated_bytes(), bytes);
		}
		register.snapshot()?
	};
	expect_gt!(snapshot.nrows(), 0);
	// The independent verifier remains usable after environment teardown.
	let certified = quest_qsp::certification::CertificationBuilder::new()
		.candidate(candidate)
		.policy(quest_qsp::certification::CertificationPolicy::default())?
		.certify()?;
	expect_le!(certified.report().response().upper_f64(), 1e-11);
	Ok(())
}
