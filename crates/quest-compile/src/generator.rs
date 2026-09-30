//! Explicit candidate generation; every returned certificate remains independently checked.
use quest_math::{ApproxCertificate, Limits, Target};
#[derive(Debug, thiserror::Error)]
pub enum GenerationError {
    #[error("independent candidate certificate rejected: {0}")]
    CertificateRejected(#[from] quest_math::Error),
    #[error(transparent)]
    Native(#[from] quest_synthesis::SynthesisError),
    #[error(transparent)]
    Process(#[from] quest_optimizer_client::Error),
}
/// Request-bounded candidate backend for the explicit rotation compilation pass.
pub trait RotationGenerator {
    #[must_use]
    fn algorithm(&self) -> &'static str {
        "custom-certified-rotation-v1"
    }
    /// # Errors
    /// Preserves mathematical rejection, work/precision exhaustion, cancellation and process failures.
    fn synthesize_rotation(
        &self,
        target: &Target,
        epsilon: f64,
        seed: u64,
        limits: Limits,
    ) -> Result<ApproxCertificate, GenerationError>;
}
impl RotationGenerator for quest_optimizer_client::Client {
    fn algorithm(&self) -> &'static str {
        "process-certified-rotation-v1"
    }
    fn synthesize_rotation(
        &self,
        target: &Target,
        epsilon: f64,
        seed: u64,
        limits: Limits,
    ) -> Result<ApproxCertificate, GenerationError> {
        Ok(self.synthesize(target, epsilon, seed, limits)?)
    }
}
/// Portable in-process synthesis. Construction never starts a search.
#[derive(Debug, Clone, Default)]
pub struct NativeSynthesis {
    options: quest_synthesis::SynthesisOptions,
}
impl NativeSynthesis {
    #[must_use]
    pub const fn new(options: quest_synthesis::SynthesisOptions) -> Self {
        Self { options }
    }
}
impl RotationGenerator for NativeSynthesis {
    fn algorithm(&self) -> &'static str {
        "ross-selinger-rust-v1"
    }
    fn synthesize_rotation(
        &self,
        target: &Target,
        epsilon: f64,
        seed: u64,
        limits: Limits,
    ) -> Result<ApproxCertificate, GenerationError> {
        let mut options = self.options.clone();
        options.seed = seed;
        // Both backend policy and per-pass limits are ceilings; neither may relax the other.
        options.limits = quest_math::Limits {
            qubits: limits.qubits.min(options.limits.qubits),
            gates: limits.gates.min(options.limits.gates),
            coefficient_bits: limits.coefficient_bits.min(options.limits.coefficient_bits),
            precision_bits: limits.precision_bits.min(options.limits.precision_bits),
            bytes: limits.bytes.min(options.limits.bytes),
            taylor_terms: limits.taylor_terms.min(options.limits.taylor_terms),
        };
        Ok(
            quest_optimizer_client::synthesize_direct(target, epsilon.to_bits(), options)?
                .certificate()
                .clone(),
        )
    }
}
pub use quest_synthesis::{CancellationToken, SynthesisError, SynthesisOptions};

#[expect(
    clippy::redundant_pub_crate,
    reason = "Candidate verification is shared only by compiler passes"
)]
pub(crate) fn generate_checked(
    generator: &dyn RotationGenerator,
    target: &Target,
    epsilon: f64,
    seed: u64,
    limits: Limits,
) -> Result<ApproxCertificate, GenerationError> {
    let proposed = generator.synthesize_rotation(target, epsilon, seed, limits)?;
    Ok(quest_math::certify_rotation(
        proposed.candidate(),
        target,
        epsilon.to_bits(),
        limits,
    )?)
}
