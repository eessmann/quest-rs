#![expect(
	clippy::panic_in_result_fn,
	reason = "Integration tests retain direct assertions while propagating construction errors"
)]
use quest_polynomial::{Laurent, Limits, Polynomial};
use quest_qsp::{Complex64, Policy, SynthesisAlgorithm, SynthesisBuilder};

#[test]
fn default_solver_is_inverse_nlft_and_explicit_rhw_remains_available()
-> Result<(), Box<dyn std::error::Error>> {
	assert_eq!(
		Policy::default().algorithm,
		SynthesisAlgorithm::InverseNlftDivideConquer
	);
	let target = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.2, 0.1)],
		Limits::default(),
	)?;
	for algorithm in [
		SynthesisAlgorithm::default(),
		SynthesisAlgorithm::RhwHalfCholesky,
	] {
		let candidate = SynthesisBuilder::new()
			.policy(Policy {
				algorithm,
				..Policy::default()
			})
			.unit_circle_response(&target)?
			.admit()?
			.complete()?
			.synthesize()?;
		assert_eq!(candidate.algorithm(), algorithm);
		assert!(
			(candidate.evaluate(Complex64::new(1.0, 0.0))?[0][0] - Complex64::new(0.2, 0.1)).norm()
				< 1e-11
		);
	}
	Ok(())
}

#[cfg(feature = "offline-synthesis")]
#[test]
fn offline_defaults_to_inverse_nlft() {
	assert_eq!(
		quest_qsp::offline::OfflinePolicy::default().algorithm,
		SynthesisAlgorithm::InverseNlftDivideConquer
	);
}

#[cfg(feature = "certification")]
#[test]
fn both_algorithms_and_conventions_retain_identity_through_production_certification()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_polynomial::Chebyshev;
	use quest_qsp::certification::{CertificationBuilder, CertificationPolicy};
	let generalized = Polynomial::new(
		Laurent::new(2),
		vec![Complex64::new(0.2, 0.1), Complex64::new(-0.07, 0.02)],
		Limits::default(),
	)?;
	for algorithm in [
		SynthesisAlgorithm::InverseNlftDivideConquer,
		SynthesisAlgorithm::RhwHalfCholesky,
	] {
		let policy = Policy {
			algorithm,
			..Policy::default()
		};
		let candidate = SynthesisBuilder::new()
			.policy(policy)
			.unit_circle_response(&generalized)?
			.admit()?
			.complete()?
			.synthesize()?;
		let certified = CertificationBuilder::new()
			.candidate(candidate)
			.policy(CertificationPolicy::default())?
			.certify()?;
		assert_eq!(certified.candidate().algorithm(), algorithm);
		for coefficients in [vec![0.1, 0.0, 0.2], vec![0.0, 0.3]] {
			let target = Polynomial::new(
				Chebyshev,
				coefficients
					.into_iter()
					.map(|x| Complex64::new(x, 0.0))
					.collect(),
				Limits::default(),
			)?;
			let candidate = SynthesisBuilder::new()
				.policy(policy)
				.real_parity_wx(&target)?
				.admit()?
				.complete()?
				.synthesize()?;
			let certified = CertificationBuilder::new()
				.candidate(candidate)
				.policy(CertificationPolicy::default())?
				.certify()?;
			assert_eq!(certified.candidate().algorithm(), algorithm);
		}
	}
	Ok(())
}

#[cfg(feature = "offline-synthesis")]
#[test]
fn both_algorithms_and_conventions_retain_identity_through_offline_certification()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_polynomial::Chebyshev;
	use quest_qsp::offline::{OfflineBuilder, OfflinePolicy};
	let generalized = Polynomial::new(
		Laurent::new(2),
		vec![Complex64::new(0.2, 0.1), Complex64::new(-0.07, 0.02)],
		Limits::default(),
	)?;
	for algorithm in [
		SynthesisAlgorithm::InverseNlftDivideConquer,
		SynthesisAlgorithm::RhwHalfCholesky,
	] {
		let policy = OfflinePolicy {
			algorithm,
			..OfflinePolicy::default()
		};
		let solution = OfflineBuilder::new()
			.unit_circle_response(&generalized)?
			.policy(policy)?
			.solve()?;
		assert_eq!(solution.certified().candidate().algorithm(), algorithm);
		for coefficients in [vec![0.1, 0.0, 0.2], vec![0.0, 0.3]] {
			let target = Polynomial::new(
				Chebyshev,
				coefficients
					.into_iter()
					.map(|x| Complex64::new(x, 0.0))
					.collect(),
				Limits::default(),
			)?;
			let solution = OfflineBuilder::new()
				.real_parity_wx(&target)?
				.policy(policy)?
				.solve()?;
			assert_eq!(solution.certified().candidate().algorithm(), algorithm);
		}
	}
	Ok(())
}
