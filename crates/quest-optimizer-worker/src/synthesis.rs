//! Optional process adapter; the Rust library is also callable directly.
use quest_math::{Sequence, Target};
use quest_synthesis::{SynthesisError, SynthesisOptions, approximate_rotation};

/// # Errors
/// Preserves the engine's explicit failure reason in the process response.
pub fn synthesize(
    target: &Target,
    epsilon_bits: u64,
    seed: u64,
) -> Result<(Sequence, usize), SynthesisError> {
    let candidate = approximate_rotation(
        target,
        epsilon_bits,
        SynthesisOptions {
            seed,
            ..SynthesisOptions::default()
        },
    )?;
    Ok((
        candidate.sequence().clone(),
        candidate.working_precision_bits(),
    ))
}

pub const fn error_code(error: &SynthesisError) -> &'static str {
    match error {
        SynthesisError::Budget { .. }
        | SynthesisError::Math(quest_math::Error::Budget { .. } | quest_math::Error::Resource(_)) => {
            "resource-exhausted"
        }
        SynthesisError::WorkExhausted { .. } => "work-exhausted",
        SynthesisError::Cancelled => "cancelled",
        SynthesisError::PrecisionUnresolved { .. } => "precision-unresolved",
        SynthesisError::AncillaRequired { .. } => "ancilla-required",
        SynthesisError::NotUnitary => "not-unitary",
        SynthesisError::CertificateRejected(_) => "certificate-rejected",
        SynthesisError::Invalid(_) => "invalid-request",
        SynthesisError::Math(_) => "arithmetic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quest_math::{AngleTarget, Axis, Limits, certify_rotation};

    #[test]
    fn affine_target_uses_the_native_candidate_engine() {
        let target = Target {
            axis: Axis::Y,
            angle: AngleTarget::AffinePi {
                radians_numerator: 1.into(),
                radians_denominator: 3.into(),
                pi_numerator: 1.into(),
                pi_denominator: 5.into(),
            },
        };
        let (candidate, precision) =
            synthesize(&target, 0.2_f64.to_bits(), 17).expect("native affine synthesis");
        assert_eq!(precision, 256);
        certify_rotation(&candidate, &target, 0.2_f64.to_bits(), Limits::default())
            .expect("independent full-phase certificate");
    }
}
