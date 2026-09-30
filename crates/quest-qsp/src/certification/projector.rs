//! Independent reconstruction of rounded projector rotations, including readout.
use super::{
    Bound, CertificationAttempt, CertificationError, CertificationPolicy, CertificationResult,
    Certified, Context, FrozenCandidate, MpComplex, MpInterval, RealParityWx, bound, difference,
    exact_from_f64, matrix_bound, product,
};
use crate::{Complex64, ConvertedProjectorPhases};
use std::time::Instant;

/// Domain of the scalar singular-value response proved by this certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectorDomain {
    /// Every real signal in [-1, 1], including both endpoints.
    RealUnitInterval,
}
/// Norm used for the converted response certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectorNorm {
    /// Uniform absolute scalar error on the declared domain.
    UniformAbsolute,
}
/// Interpretation of the complete converted payload and extraction rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectorConvention {
    /// P(phi0) R(x) ... R(x) P(phid), followed by the stored Rz readout;
    /// response is Re(exp(-i readout/2) U00).
    ReflectionSignalRealReadout,
}
/// Actual rounded projector phases with independently established response and
/// full-matrix unitarity bounds. No certificate for earlier phase values is reused.
#[derive(Debug)]
pub struct CertifiedProjectorPhases {
    converted: ConvertedProjectorPhases,
    readout: f64,
    source: FrozenCandidate<RealParityWx>,
    response: Bound,
    unitarity: Bound,
    policy: CertificationPolicy,
    attempts: Vec<CertificationAttempt>,
}
impl CertifiedProjectorPhases {
    /// Actual finite projector rotations admitted by this certificate.
    #[must_use]
    pub fn values(&self) -> &[f64] {
        self.converted.values()
    }
    /// Actual binary64 Rz readout angle, included in response certification.
    #[must_use]
    pub const fn readout_phase(&self) -> f64 {
        self.readout
    }
    /// Original real Chebyshev coefficients tied to these immutable phases.
    #[must_use]
    pub fn source_coefficients(&self) -> &[Complex64] {
        &self.source.admitted.source
    }
    /// Frozen source candidate retained for provenance, without inheriting its bounds.
    #[must_use]
    pub const fn source_candidate(&self) -> &FrozenCandidate<RealParityWx> {
        &self.source
    }
    /// Uniform error for the actual extracted response relative to the source.
    #[must_use]
    pub const fn response_bound(&self) -> &Bound {
        &self.response
    }
    /// Full 2x2 product unitarity defect, bounded in Frobenius norm.
    #[must_use]
    pub const fn unitarity_bound(&self) -> &Bound {
        &self.unitarity
    }
    /// Certified signal domain.
    #[must_use]
    pub const fn domain(&self) -> ProjectorDomain {
        ProjectorDomain::RealUnitInterval
    }
    /// Certified scalar-response norm.
    #[must_use]
    pub const fn norm(&self) -> ProjectorNorm {
        ProjectorNorm::UniformAbsolute
    }
    /// Product and extraction convention tied to this certificate.
    #[must_use]
    pub const fn convention(&self) -> ProjectorConvention {
        ProjectorConvention::ReflectionSignalRealReadout
    }
    /// Request-owned numerical tolerances and resource limits.
    #[must_use]
    pub const fn policy(&self) -> CertificationPolicy {
        self.policy
    }
    /// Completed precision attempts for this conversion, separate from source certification.
    #[must_use]
    pub fn attempts(&self) -> &[CertificationAttempt] {
        &self.attempts
    }
}
fn conversion_policy(
    candidate: &FrozenCandidate<RealParityWx>,
    policy: CertificationPolicy,
) -> CertificationResult<(CertificationPolicy, usize)> {
    // Both binary64 conversion vectors coexist transiently; retained source
    // arrays also remain alive during the independent interval verification.
    let converted = candidate
        .phases
        .len()
        .checked_mul(size_of::<[f64; 2]>())
        .ok_or(CertificationError::Budget("projector conversion storage"))?;
    let retained = super::source_bytes(candidate)?
        .checked_add(converted)
        .ok_or(CertificationError::Budget("projector retained storage"))?;
    let working = CertificationPolicy {
        max_bytes: policy
            .max_bytes
            .checked_sub(retained)
            .ok_or(CertificationError::Budget("projector retained storage"))?,
        ..policy
    };
    Ok((working, retained))
}
impl Certified<RealParityWx> {
    /// Convert and independently certify the actual rounded projector payload.
    /// The verifier reconstructs the reflection-signal product and its readout
    /// directly, against the original Chebyshev coefficients on [-1,1].
    ///
    /// # Errors
    /// Reports malformed export, arithmetic/resource failure, a proved response
    /// violation, or an unestablished response/ unitarity bound at maximum precision.
    pub fn certify_projector_phases(
        &self,
        policy: CertificationPolicy,
    ) -> CertificationResult<CertifiedProjectorPhases> {
        policy.validate()?;
        let phase_count = self.candidate.phases.len();
        let degree = phase_count
            .checked_sub(1)
            .ok_or(CertificationError::Export("empty projector phases"))?;
        let reduced = if degree == 0 {
            0
        } else {
            degree.saturating_sub(1) % 4
        };
        let readout = -f64::from(
            u32::try_from(reduced).map_err(|_| CertificationError::Budget("projector readout"))?,
        ) * std::f64::consts::PI;
        let count = degree
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .ok_or(CertificationError::Budget("projector support"))?;
        if count > policy.max_coefficients
            || self.candidate.admitted.source.len() > policy.max_coefficients
        {
            return Err(CertificationError::Budget("projector support"));
        }
        let (working, retained) = conversion_policy(&self.candidate, policy)?;
        let mut precision = policy.initial_precision;
        // This context admits verification plus conversion storage before the
        // first phase clone or trigonometric conversion can take place.
        let mut context = Context::new(
            count.max(self.candidate.admitted.source.len()),
            precision,
            working,
        )?;
        context.charge(
            phase_count
                .checked_mul(32)
                .ok_or(CertificationError::Budget("projector conversion work"))?,
        )?;
        let converted = self
            .candidate
            .phase_sequence()
            .real_parity_wx()
            .projector_phases_with_diagnostics();
        let mut attempts = Vec::new();
        loop {
            let started = Instant::now();
            let (response, unitarity) = verify(
                &converted,
                readout,
                &self.candidate.admitted.source,
                &mut context,
            )?;
            let work = context.work;
            attempts.push(CertificationAttempt {
                precision,
                work_units: context.work,
                modeled_peak_bytes: context
                    .bytes
                    .checked_add(retained)
                    .ok_or(CertificationError::Budget("projector aggregate memory"))?,
                elapsed: started.elapsed(),
            });
            let response_tolerance = exact_from_f64(policy.response_tolerance, precision)?;
            let unitarity_tolerance = exact_from_f64(policy.unitarity_tolerance, precision)?;
            if response.lower > response_tolerance || unitarity.lower > unitarity_tolerance {
                return Err(CertificationError::ProjectorViolation {
                    response: Box::new(response),
                    unitarity: Box::new(unitarity),
                });
            }
            if response.upper <= response_tolerance && unitarity.upper <= unitarity_tolerance {
                return Ok(CertifiedProjectorPhases {
                    converted,
                    readout,
                    source: self.candidate.clone(),
                    response,
                    unitarity,
                    policy,
                    attempts,
                });
            }
            if precision == policy.max_precision {
                return Err(CertificationError::ProjectorNotEstablished {
                    response: Box::new(response),
                    unitarity: Box::new(unitarity),
                });
            }
            precision = precision
                .checked_mul(2)
                .unwrap_or(policy.max_precision)
                .min(policy.max_precision);
            context = Context::new(
                count.max(self.candidate.admitted.source.len()),
                precision,
                working,
            )?;
            context.work = work;
        }
    }
}

#[expect(
    clippy::many_single_char_names,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "2x2 Laurent recurrence: four equal-length arrays; checked size grows by two; indices bounded by admitted degree"
)]
fn verify(
    converted: &ConvertedProjectorPhases,
    readout: f64,
    source: &[Complex64],
    context: &mut Context,
) -> CertificationResult<(Bound, Bound)> {
    let p = context.precision;
    context.charge(
        source
            .len()
            .checked_mul(8)
            .ok_or(CertificationError::Budget("projector source work"))?,
    )?;
    let mut actual: [Vec<MpComplex>; 4] = [
        vec![MpComplex::one(p)],
        vec![MpComplex::zero(p)],
        vec![MpComplex::zero(p)],
        vec![MpComplex::one(p)],
    ];
    // w R(x) has constant and quadratic coefficients. This independent
    // recurrence never invokes production controls or a source QSP certificate.
    let half = MpComplex::exact(Complex64::new(0.5, 0.0), p)?;
    let imaginary_half = MpComplex::exact(Complex64::new(0.0, 0.5), p)?;
    for (index, &phase) in converted.values().iter().enumerate() {
        if index > 0 {
            let size = actual[0]
                .len()
                .checked_add(2)
                .ok_or(CertificationError::Budget("projector product"))?;
            context.charge(
                size.checked_mul(256)
                    .ok_or(CertificationError::Budget("projector product"))?,
            )?;
            let mut next: [Vec<MpComplex>; 4] =
                std::array::from_fn(|_| vec![MpComplex::zero(p); size]);
            for j in 0..actual[0].len() {
                for (offset, s) in [(0, imaginary_half.clone()), (2, imaginary_half.neg())] {
                    for row in 0..2 {
                        let a = &actual[2 * row][j];
                        let b = &actual[2 * row + 1][j];
                        next[2 * row][j + offset] =
                            next[2 * row][j + offset].add(&a.mul(&half)?.add(&b.mul(&s)?)?)?;
                        next[2 * row + 1][j + offset] =
                            next[2 * row + 1][j + offset].add(&a.mul(&s)?.sub(&b.mul(&half)?)?)?;
                    }
                }
            }
            actual = next;
        }
        context.charge(
            actual[0]
                .len()
                .checked_mul(128)
                .ok_or(CertificationError::Budget("projector phase"))?,
        )?;
        let (sin, cos) = MpInterval::sin_cos_exact(phase, p, &mut context.constants)?;
        let rotation = MpComplex::new(cos, sin);
        for (entry, coefficients) in actual.iter_mut().enumerate() {
            let phase = if entry % 2 == 0 {
                rotation.clone()
            } else {
                rotation.conj()
            };
            for value in coefficients {
                *value = value.mul(&phase)?;
            }
        }
    }
    let [a, b, c, d] = &actual;
    let entries = [
        bound(&product::gram_entry(a, c, a, c, true, context)?, p)?,
        bound(&product::gram_entry(a, c, b, d, false, context)?, p)?,
        bound(&product::gram_entry(b, d, a, c, false, context)?, p)?,
        bound(&product::gram_entry(b, d, b, d, true, context)?, p)?,
    ];
    let unitarity = matrix_bound(&entries, p)?;
    let (sin, cos) = MpInterval::sin_cos_exact(-readout / 2.0, p, &mut context.constants)?;
    let rotation = MpComplex::new(cos, sin);
    let rotated: Vec<_> = a
        .iter()
        .map(|v| v.mul(&rotation))
        .collect::<CertificationResult<_>>()?;
    let response: Vec<_> = rotated
        .iter()
        .zip(rotated.iter().rev())
        .map(|(a, b)| a.add(&b.conj())?.divide_usize(2))
        .collect::<CertificationResult<_>>()?;
    let degree = converted.values().len() - 1;
    let mut expected = vec![MpComplex::zero(p); 2 * degree + 1];
    for (j, &value) in source.iter().enumerate() {
        if value.im != 0.0 || (j > degree && value.re != 0.0) {
            return Err(CertificationError::Export("projector source domain"));
        }
        if j > degree {
            continue;
        }
        let half = MpComplex::exact(value, p)?.divide_usize(2)?;
        for index in [degree - j, degree + j] {
            expected[index] = expected[index].add(&half)?;
        }
    }
    Ok((bound(&difference(&response, &expected, p)?, p)?, unitarity))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aggregate_conversion_storage_is_admitted_before_conversion() {
        use quest_polynomial::{Chebyshev, Limits, Polynomial};
        let p =
            Polynomial::new(Chebyshev, vec![Complex64::new(0.3, 0.0)], Limits::default()).unwrap();
        let candidate = crate::SynthesisBuilder::new()
            .real_parity_wx(&p)
            .unwrap()
            .admit()
            .unwrap()
            .complete()
            .unwrap()
            .synthesize()
            .unwrap();
        let source = super::super::CertificationBuilder::new()
            .candidate(candidate)
            .policy(CertificationPolicy::default())
            .unwrap()
            .certify()
            .unwrap();
        let verification_bytes = Context::new(1, 256, CertificationPolicy::default())
            .unwrap()
            .bytes;
        assert!(matches!(
            source.certify_projector_phases(CertificationPolicy {
                max_bytes: verification_bytes,
                ..CertificationPolicy::default()
            }),
            Err(CertificationError::Budget(_))
        ));
        assert!(matches!(
            source.certify_projector_phases(CertificationPolicy {
                max_coefficients: 0,
                ..CertificationPolicy::default()
            }),
            Err(CertificationError::Policy(_))
        ));
    }
    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "Regression assertions in test only"
    )]
    fn independent_projector_reconstruction_rejects_a_changed_phase() -> CertificationResult<()> {
        let phases = crate::PhaseSequence::<crate::WxImaginaryU00>::builder(vec![0.1])
            .build()
            .map_err(|_| CertificationError::Export("test import"))?;
        let actual = phases.projector_phases_with_diagnostics();
        let mut context = Context::new(1, 256, CertificationPolicy::default())?;
        let (response, unitarity) =
            verify(&actual, 0.0, &[Complex64::new(0.0, 0.0)], &mut context)?;
        assert!(response.lower_f64() > 0.09);
        assert!(unitarity.upper_f64() < 1e-11);
        Ok(())
    }
}
