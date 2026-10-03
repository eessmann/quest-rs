#![cfg(feature = "rayon")]
use googletest::prelude::*;
use quest_numerics::ExecutionPolicy;
use quest_polynomial::{Chebyshev, Laurent, Limits, Polynomial};
use quest_qsp::{Complex64, SynthesisBuilder};
fn control_bits<M>(candidate: &quest_qsp::FrozenCandidate<M>) -> Vec<u64> {
	candidate
		.controls()
		.iter()
		.flatten()
		.flatten()
		.flat_map(|value| [value.re.to_bits(), value.im.to_bits()])
		.collect()
}
#[gtest]
fn caller_pools_preserve_generalized_export_bits_across_worker_counts() -> Result<()> {
	let target = Polynomial::new(
		Laurent::new(0),
		(0_i32..257)
			.map(|k| {
				Complex64::new(
					f64::from(k.rem_euclid(3).saturating_sub(1)) / 1024.0,
					f64::from(k.rem_euclid(5).saturating_sub(2)) / 2048.0,
				)
			})
			.collect(),
		Limits::default(),
	)?;
	let admitted = SynthesisBuilder::new()
		.unit_circle_response(&target)?
		.admit()?;
	let serial = admitted.clone().complete()?.synthesize()?;
	let expected = control_bits(&serial);
	for workers in [1, 2, 4] {
		let pool = rayon::ThreadPoolBuilder::new()
			.num_threads(workers)
			.build()?;
		let execution = ExecutionPolicy::Rayon(&pool);
		let candidate = admitted
			.clone()
			.complete_with(execution)?
			.synthesize_with(execution)?;
		expect_that!(control_bits(&candidate), eq(&expected));
		expect_that!(
			candidate.conjugate_complement(),
			eq(serial.conjugate_complement())
		);
	}
	Ok(())
}
#[gtest]
fn canonical_pool_scope_ends_before_frozen_phase_payload_is_used() -> Result<()> {
	let target = Polynomial::new(
		Chebyshev,
		vec![Complex64::new(0.0, 0.0), Complex64::new(0.6, 0.0)],
		Limits::default(),
	)?;
	let admitted = SynthesisBuilder::new().real_parity_wx(&target)?.admit()?;
	let serial = admitted.clone().complete()?.synthesize()?;
	let parallel = {
		let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build()?;
		admitted
			.complete_with(ExecutionPolicy::Rayon(&pool))?
			.synthesize_with(ExecutionPolicy::Rayon(&pool))?
	};
	expect_that!(
		parallel
			.phases()
			.iter()
			.map(|value| value.to_bits())
			.collect::<Vec<_>>(),
		eq(&serial
			.phases()
			.iter()
			.map(|value| value.to_bits())
			.collect::<Vec<_>>())
	);
	expect_that!(
		parallel.response(0.3)?.to_bits(),
		eq(serial.response(0.3)?.to_bits())
	);
	Ok(())
}
#[gtest]
fn caller_pools_do_not_retry_failed_work_budgets_with_another_backend() -> Result<()> {
	let target = Polynomial::new(Chebyshev, vec![Complex64::new(0.1, 0.0)], Limits::default())?;
	let mut policy = quest_qsp::Policy::default();
	policy.limits.max_work = 1280;
	let admitted = SynthesisBuilder::new()
		.policy(policy)
		.real_parity_wx(&target)?
		.admit()?;
	expect_true!(matches!(
		admitted.clone().complete(),
		Err(quest_qsp::Error::Budget("Weiss work"))
	));
	for workers in [1, 2, 4] {
		let pool = rayon::ThreadPoolBuilder::new()
			.num_threads(workers)
			.build()?;
		expect_true!(matches!(
			admitted
				.clone()
				.complete_with(ExecutionPolicy::Rayon(&pool)),
			Err(quest_qsp::Error::Budget("Weiss work"))
		));
	}
	Ok(())
}
#[gtest]
#[ignore = "explicit release acceptance using the degree-8105 catalog source"]
fn degree_8105_catalog_preserves_canonical_and_generalized_bits() -> Result<()> {
	let bytes = include_bytes!("data/inverse-degree-8105.bin");
	let (chunks, tail) = bytes.as_chunks::<8>();
	expect_true!(tail.is_empty());
	let target = Polynomial::new(
		Chebyshev,
		chunks
			.iter()
			.map(|bytes| Complex64::new(f64::from_le_bytes(*bytes), 0.0))
			.collect(),
		Limits::default(),
	)?;
	let real_parity_wx = SynthesisBuilder::new().real_parity_wx(&target)?.admit()?;
	let powers = Polynomial::new(
		Laurent::new(0),
		real_parity_wx.coefficients().to_vec(),
		Limits::default(),
	)?;
	let unit_circle_response = SynthesisBuilder::new()
		.unit_circle_response(&powers)?
		.admit()?;
	let start = std::time::Instant::now();
	let canonical_serial = real_parity_wx.clone().complete()?.synthesize()?;
	let generalized_serial = unit_circle_response.clone().complete()?.synthesize()?;
	eprintln!(
		"degree8105 sequential real_parity_wx+unit_circle_response seconds={}",
		start.elapsed().as_secs_f64()
	);
	for workers in [1, 2, 4] {
		let pool = rayon::ThreadPoolBuilder::new()
			.num_threads(workers)
			.build()?;
		let execution = ExecutionPolicy::Rayon(&pool);
		let start = std::time::Instant::now();
		let a = real_parity_wx
			.clone()
			.complete_with(execution)?
			.synthesize_with(execution)?;
		let b = unit_circle_response
			.clone()
			.complete_with(execution)?
			.synthesize_with(execution)?;
		expect_that!(control_bits(&a), eq(&control_bits(&canonical_serial)));
		expect_that!(control_bits(&b), eq(&control_bits(&generalized_serial)));
		expect_that!(
			a.phases().iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
			eq(&canonical_serial
				.phases()
				.iter()
				.map(|v| v.to_bits())
				.collect::<Vec<_>>())
		);
		expect_that!(
			a.conjugate_complement(),
			eq(canonical_serial.conjugate_complement())
		);
		expect_that!(
			b.conjugate_complement(),
			eq(generalized_serial.conjugate_complement())
		);
		eprintln!(
			"degree8105 workers={workers} real_parity_wx+unit_circle_response seconds={}",
			start.elapsed().as_secs_f64()
		);
	}
	#[cfg(feature = "certification")]
	{
		use quest_qsp::certification::{CertificationBuilder, CertificationPolicy};
		CertificationBuilder::new()
			.candidate(canonical_serial)
			.policy(CertificationPolicy::default())?
			.certify()?;
		CertificationBuilder::new()
			.candidate(generalized_serial)
			.policy(CertificationPolicy::default())?
			.certify()?;
	}
	Ok(())
}
