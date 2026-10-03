#![cfg(feature = "synthesis")]
use googletest::prelude::*;
use quest_compile::{NativeSynthesis, RotationGenerator};
use quest_math::{AngleTarget, Axis, Target};

#[gtest]
fn native_compiler_records_the_algorithm_that_produced_the_candidate() -> Result<()> {
	let target = Target {
		axis: Axis::Z,
		angle: AngleTarget::RationalPi {
			numerator: 0.into(),
			denominator: 1.into(),
		},
	};
	let approximation = quest_synthesis::approximate_rotation(
		&target,
		0.01_f64.to_bits(),
		quest_synthesis::SynthesisOptions::default(),
	)?;
	expect_eq!(
		NativeSynthesis::default().algorithm(),
		approximation.algorithm()
	);
	Ok(())
}

#[gtest]
fn custom_generator_certificate_is_rechecked_against_the_requested_target() -> Result<()> {
	use quest_compile::{
		Angle, Gate, GenerationError, QuantumRegionBuilder, RotationSynthesisPasses, WorkerError,
	};
	struct WrongTarget;
	impl RotationGenerator for WrongTarget {
		fn synthesize_rotation(
			&self,
			_: &Target,
			epsilon: f64,
			_: u64,
			limits: quest_math::Limits,
		) -> std::result::Result<quest_math::ApproxCertificate, GenerationError> {
			let other = Target {
				axis: Axis::Z,
				angle: AngleTarget::RationalPi {
					numerator: 0.into(),
					denominator: 1.into(),
				},
			};
			Ok(quest_math::certify_rotation(
				&quest_math::Sequence {
					qubits: 1,
					operations: vec![],
				},
				&other,
				epsilon.to_bits(),
				limits,
			)?)
		}
	}
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	builder.gate(Gate::Rz(Angle::pi(1, 1)?), &[builder.qubit(0)?], &[])?;
	expect_true!(matches!(
		builder
			.finish()?
			.synthesize_rotations(&WrongTarget, 0.1, 0, quest_math::Limits::default()),
		Err(WorkerError::Generator(
			GenerationError::CertificateRejected(_)
		))
	));
	Ok(())
}
