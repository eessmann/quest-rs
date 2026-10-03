#![expect(
	clippy::panic_in_result_fn,
	reason = "Integration tests retain direct assertions while propagating construction errors"
)]
use quest_polynomial::{Laurent, Limits, Polynomial};
use quest_qsp::{Complex64, Policy, SynthesisAlgorithm, SynthesisBuilder};

#[test]
fn explicit_rhw_retains_algorithm_and_complex_response() -> Result<(), Box<dyn std::error::Error>> {
	assert_eq!(
		Policy::default().algorithm,
		SynthesisAlgorithm::InverseNlftDivideConquer
	);
	for coefficients in [
		vec![Complex64::new(0.3, 0.2)],
		vec![Complex64::new(0.2, 0.1), Complex64::new(-0.1, 0.07)],
		vec![
			Complex64::new(0.12, -0.17),
			Complex64::new(0.07, 0.09),
			Complex64::new(-0.11, 0.03),
		],
		(0_i32..17)
			.map(|k| {
				Complex64::new(
					f64::from(k.rem_euclid(3) - 1) / 128.0,
					f64::from(k.rem_euclid(5) - 2) / 256.0,
				)
			})
			.collect(),
	] {
		let target = Polynomial::new(Laurent::new(0), coefficients, Limits::default())?;
		let rhw = SynthesisBuilder::new()
			.policy(Policy {
				algorithm: SynthesisAlgorithm::RhwHalfCholesky,
				..Policy::default()
			})
			.unit_circle_response(&target)?
			.admit()?
			.complete()?
			.synthesize()?;
		let inverse = SynthesisBuilder::new()
			.policy(Policy {
				algorithm: SynthesisAlgorithm::InverseNlftDivideConquer,
				..Policy::default()
			})
			.unit_circle_response(&target)?
			.admit()?
			.complete()?
			.synthesize()?;
		assert_eq!(rhw.algorithm(), SynthesisAlgorithm::RhwHalfCholesky);
		assert_eq!(
			rhw.synthesis_precision(),
			quest_qsp::SynthesisPrecision::Binary64
		);
		for k in 0..31 {
			let z = Complex64::from_polar(1.0, f64::from(k) * 0.2);
			let a = rhw.evaluate(z)?;
			let b = inverse.evaluate(z)?;
			for (a, b) in a.iter().flatten().zip(b.iter().flatten()) {
				assert!((*a - *b).norm() < 1e-10, "{a} != {b}");
			}
		}
	}
	Ok(())
}

#[cfg(feature = "offline-synthesis")]
#[test]
fn offline_rhw_retains_algorithm() -> Result<(), Box<dyn std::error::Error>> {
	use quest_qsp::offline::{OfflineBuilder, OfflinePolicy};
	let target = Polynomial::new(
		Laurent::new(2),
		vec![Complex64::new(0.23, 0.11), Complex64::new(-0.07, 0.03)],
		Limits::default(),
	)?;
	for algorithm in [
		SynthesisAlgorithm::RhwHalfCholesky,
		SynthesisAlgorithm::InverseNlftDivideConquer,
	] {
		let solution = OfflineBuilder::new()
			.unit_circle_response(&target)?
			.policy(OfflinePolicy {
				algorithm,
				..OfflinePolicy::default()
			})?
			.solve()?;
		assert_eq!(solution.certified().candidate().algorithm(), algorithm);
		assert!(
			matches!(solution.certified().candidate().synthesis_precision(),quest_qsp::SynthesisPrecision::Arbitrary {bits} if bits>=128)
		);
	}
	Ok(())
}

#[test]
fn contractivity_failure_distinguishes_witness_from_exhausted_bound()
-> Result<(), Box<dyn std::error::Error>> {
	let rejected = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(1.1, 0.0)],
		Limits::default(),
	)?;
	assert!(matches!(
		SynthesisBuilder::new()
			.policy(Policy {
				algorithm: SynthesisAlgorithm::RhwHalfCholesky,
				..Policy::default()
			})
			.unit_circle_response(&rejected)?
			.admit(),
		Err(quest_qsp::Error::ContractivityViolation { .. })
	));
	let unresolved = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.8, 0.0), Complex64::new(0.0, 0.3)],
		Limits::default(),
	)?;
	assert!(matches!(
		SynthesisBuilder::new()
			.policy(Policy {
				max_completion_grid: 1,
				..Policy::default()
			})
			.unit_circle_response(&unresolved)?
			.admit(),
		Err(quest_qsp::Error::Contractivity { .. })
	));
	Ok(())
}

#[cfg(feature = "certification")]
#[test]
fn seeded_offset_and_near_boundary_rhw_exports_are_independently_certified()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_qsp::certification::{CertificationBuilder, CertificationPolicy};
	let mut state = 0x4152_4857_u64;
	for (offset, n) in [(0, 1), (0, 2), (3, 5), (0, 17), (2, 33)] {
		let mut coefficients = Vec::new();
		for _ in 0..n {
			state = state
				.wrapping_mul(6_364_136_223_846_793_005)
				.wrapping_add(1);
			let re = f64::from(u32::try_from(state >> 32)?) / f64::from(u32::MAX) - 0.5;
			state = state
				.wrapping_mul(6_364_136_223_846_793_005)
				.wrapping_add(1);
			let im = f64::from(u32::try_from(state >> 32)?) / f64::from(u32::MAX) - 0.5;
			coefficients.push(Complex64::new(re, im));
		}
		let scale = 0.8 / coefficients.iter().map(|v| v.norm()).sum::<f64>();
		for c in &mut coefficients {
			*c *= scale;
		}
		let target = Polynomial::new(Laurent::new(offset), coefficients, Limits::default())?;
		let completed = SynthesisBuilder::new()
			.policy(Policy {
				algorithm: SynthesisAlgorithm::RhwHalfCholesky,
				..Policy::default()
			})
			.unit_circle_response(&target)?
			.admit()?
			.complete()?;
		let ratio = completed.weiss_ratio().ok_or("missing RHW ratio")?;
		assert_eq!(ratio.grid(), completed.completion_grid());
		assert!(ratio.contractivity_upper_bound() < 1.0);
		let frozen = completed.synthesize()?;
		let certified = CertificationBuilder::new()
			.candidate(frozen)
			.policy(CertificationPolicy::default())?
			.certify()?;
		assert!(certified.report().reconstruction().upper_f64() < 1e-11);
	}
	let target = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.999_999, 0.0)],
		Limits::default(),
	)?;
	let frozen = SynthesisBuilder::new()
		.policy(Policy {
			algorithm: SynthesisAlgorithm::RhwHalfCholesky,
			..Policy::default()
		})
		.unit_circle_response(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	CertificationBuilder::new()
		.candidate(frozen)
		.policy(CertificationPolicy::default())?
		.certify()?;
	Ok(())
}

#[test]
fn both_real_parities_support_explicit_rhw_and_inverse_nlft()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_polynomial::Chebyshev;
	for coefficients in [
		vec![0.1, 0.0, -0.2, 0.0, 0.07],
		vec![0.0, 0.15, 0.0, -0.03, 0.0, 0.11],
	] {
		let target = Polynomial::new(
			Chebyshev,
			coefficients
				.into_iter()
				.map(|v| Complex64::new(v, 0.0))
				.collect(),
			Limits::default(),
		)?;
		let rhw = SynthesisBuilder::new()
			.policy(Policy {
				algorithm: SynthesisAlgorithm::RhwHalfCholesky,
				..Policy::default()
			})
			.real_parity_wx(&target)?
			.admit()?
			.complete()?
			.synthesize()?;
		let nlft = SynthesisBuilder::new()
			.policy(Policy {
				algorithm: SynthesisAlgorithm::InverseNlftDivideConquer,
				..Policy::default()
			})
			.real_parity_wx(&target)?
			.admit()?
			.complete()?
			.synthesize()?;
		for k in 0..31 {
			let x = f64::from(k) / 15.0 - 1.0;
			assert!((rhw.response(x)? - nlft.response(x)?).abs() < 1e-11);
			let z = Complex64::from_polar(1.0, f64::from(k) * 0.2);
			for (a, b) in rhw
				.evaluate(z)?
				.iter()
				.flatten()
				.zip(nlft.evaluate(z)?.iter().flatten())
			{
				assert!((*a - *b).norm() < 1e-11);
			}
		}
	}
	Ok(())
}
