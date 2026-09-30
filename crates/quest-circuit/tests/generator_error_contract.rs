#![cfg(any(feature = "synthesis", feature = "workers"))]
use googletest::prelude::*;
use quest_circuit::{
    Angle, Gate, GenerationError, QuantumRegionBuilder, RotationGenerator, RotationSynthesisPasses,
    SynthesisError, WorkerError, optimizer,
};
use quest_math::{ApproxCertificate, Limits, Target};

#[derive(Clone, Copy)]
enum Failure {
    Decline,
    Timeout,
    Envelope,
    Cancelled,
    Certificate,
}
impl RotationGenerator for Failure {
    fn synthesize_rotation(
        &self,
        _: &Target,
        _: f64,
        _: u64,
        _: Limits,
    ) -> std::result::Result<ApproxCertificate, GenerationError> {
        Err(match self {
            Self::Decline => GenerationError::Process(optimizer::Error::Candidate {
                code: "declined".into(),
                message: "use MITM".into(),
            }),
            Self::Timeout => GenerationError::Process(optimizer::Error::Timeout),
            Self::Envelope => GenerationError::Process(optimizer::Error::Envelope),
            Self::Cancelled => GenerationError::Native(SynthesisError::Cancelled),
            Self::Certificate => {
                GenerationError::CertificateRejected(quest_math::Error::NotCertified)
            }
        })
    }
}
#[gtest]
fn rotation_generation_preserves_process_classification_and_native_failures() -> Result<()> {
    let mut b = QuantumRegionBuilder::new(1, 0)?;
    b.gate(Gate::Rx(Angle::radians(0.1)?), &[b.qubit(0)?], &[])?;
    let region = b.finish()?;
    for failure in [
        Failure::Decline,
        Failure::Timeout,
        Failure::Envelope,
        Failure::Cancelled,
        Failure::Certificate,
    ] {
        let result = region
            .clone()
            .synthesize_rotations(&failure, 0.2, 0, Limits::default());
        let correct = match (failure, result) {
            (
                Failure::Decline,
                Err(WorkerError::Worker(optimizer::Error::Candidate { code, message })),
            ) => code == "declined" && message == "use MITM",
            (Failure::Timeout, Err(WorkerError::Worker(optimizer::Error::Timeout)))
            | (Failure::Envelope, Err(WorkerError::Worker(optimizer::Error::Envelope)))
            | (
                Failure::Cancelled,
                Err(WorkerError::Generator(GenerationError::Native(SynthesisError::Cancelled))),
            )
            | (
                Failure::Certificate,
                Err(WorkerError::Generator(GenerationError::CertificateRejected(
                    quest_math::Error::NotCertified,
                ))),
            ) => true,
            _ => false,
        };
        expect_true!(correct);
    }
    Ok(())
}
