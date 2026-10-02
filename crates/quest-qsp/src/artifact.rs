//! Versioned exact-bit compiled artifacts. Historical receipts are untrusted data;
//! only [`load_certified`] constructs fresh independent accuracy evidence.
use crate::certification::{
    CertificationBuilder, CertificationError, CertificationPolicy, Certified, ConvolutionMethod,
};
use crate::{
    AdmittedTarget, Complex64, Control, FftBackend, FrozenCandidate, Policy, RealParityWx,
    SynthesisAlgorithm, SynthesisPrecision, UnitCircleResponse,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{marker::PhantomData, sync::Arc};

/// Structural, serialization, resource or independent verification rejection.
#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    /// Unknown version, malformed finite payload, or mismatching digest.
    #[error("invalid compiled QSP artifact: {0}")]
    Invalid(&'static str),
    /// Caller-owned resource cap exceeded before decoding or allocation.
    #[error("compiled QSP artifact resource limit: {0}")]
    Budget(&'static str),
    /// Invalid JSON or unknown fields.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Independent contractivity or source admission failure.
    #[error(transparent)]
    Admission(#[from] crate::Error),
    /// Independent recertification did not establish the requested bounds.
    #[error(transparent)]
    Certification(#[from] CertificationError),
}
/// Artifact operations never silently synthesize replacement values.
pub type ArtifactResult<T> = std::result::Result<T, ArtifactError>;
/// Bounds for input bytes, modeled decoding storage and retained coefficient arrays.
#[derive(Debug, Clone, Copy)]
pub struct ArtifactLimits {
    /// Maximum encoded file length.
    pub max_bytes: usize,
    /// Maximum modeled simultaneous JSON/payload/candidate storage.
    pub max_decoded_bytes: usize,
    /// Maximum length of each source, target, complement or export array.
    pub max_coefficients: usize,
}
impl Default for ArtifactLimits {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_decoded_bytes: 512 * 1024 * 1024,
            max_coefficients: 1_048_576,
        }
    }
}
/// Independent loader limits; file-supplied policy can never increase these.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadPolicy {
    /// Structural and allocation admission before JSON decoding.
    pub storage: ArtifactLimits,
    /// Budget and margin used to re-establish target contractivity.
    pub admission: Policy,
}
/// Structurally validated export with freshly admitted target, without accuracy evidence.
#[derive(Debug)]
pub enum LoadedCompiled {
    /// Original exact-bit imaginary-U00 Wx phase export.
    RealParityWx(FrozenCandidate<RealParityWx>),
    /// Original exact-bit unit-circle controls, retaining terminal K.
    UnitCircleResponse(FrozenCandidate<UnitCircleResponse>),
}
/// Fresh independent certification of the exact loaded export.
#[derive(Debug, Clone)]
pub enum LoadedCertified {
    /// The exact phases can subsequently undergo certified projector conversion.
    RealParityWx(Certified<RealParityWx>),
    /// Full complex exported controls were independently reconstructed.
    UnitCircleResponse(Certified<UnitCircleResponse>),
}
mod private {
    pub trait Sealed {}
}
impl private::Sealed for RealParityWx {}
impl private::Sealed for UnitCircleResponse {}
/// Sealed supported artifact response conventions.
pub trait ArtifactMode: private::Sealed {
    /// Pinned convention, domain, readout and terminal-factor identity.
    #[doc(hidden)]
    const CONVENTION: &'static str;
}
impl ArtifactMode for RealParityWx {
    const CONVENTION: &'static str = "wx-imaginary-u00-real-interval-v1";
}
impl ArtifactMode for UnitCircleResponse {
    const CONVENTION: &'static str = "unit-circle-u00-final-k-v1";
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    payload: Payload,
    sha256: [u8; 32],
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    schema_version: u32,
    algorithm_version: u32,
    algorithm: String,
    precision_kind: String,
    precision_bits: Option<u32>,
    convention: String,
    source_basis: String,
    support_offset: i32,
    original_source_offset: i32,
    original_source_length: usize,
    outer_gauge: String,
    policy: StoredPolicy,
    source: Vec<[u64; 2]>,
    target: Vec<[u64; 2]>,
    complement: Vec<[u64; 2]>,
    controls: Vec<[[[u64; 2]; 2]; 2]>,
    phases: Vec<u64>,
    completion_grid: usize,
    completion_residual: u64,
    reconstruction_residual: Option<u64>,
    contractivity_upper: u64,
    historical_receipt: Option<Receipt>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPolicy {
    response_tolerance: u64,
    contractivity_margin: u64,
    max_completion_grid: usize,
    backend: String,
    max_len: usize,
    max_bytes: usize,
    max_work: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    status: String,
    initial_precision: u32,
    max_precision: u32,
    method: String,
    tolerances: [u64; 5],
    limits: [usize; 3],
    // Binary64 outward summaries, not serialized arbitrary-precision proof objects.
    bounds: [[u64; 2]; 9],
    attempts: Vec<Attempt>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attempt {
    precision: u32,
    work: usize,
    modeled_peak_bytes: usize,
    elapsed_seconds: u64,
    elapsed_nanoseconds: u32,
}
const fn bits(z: Complex64) -> [u64; 2] {
    [z.re.to_bits(), z.im.to_bits()]
}
const fn scalar(value: u64) -> ArtifactResult<f64> {
    let value = f64::from_bits(value);
    if !value.is_finite() {
        return Err(ArtifactError::Invalid("nonfinite IEEE payload"));
    }
    Ok(value)
}
fn complex(value: [u64; 2]) -> ArtifactResult<Complex64> {
    Ok(Complex64::new(scalar(value[0])?, scalar(value[1])?))
}
fn policy(value: Policy) -> StoredPolicy {
    StoredPolicy {
        response_tolerance: value.response_tolerance.to_bits(),
        contractivity_margin: value.contractivity_margin.to_bits(),
        max_completion_grid: value.max_completion_grid,
        backend: match value.backend {
            FftBackend::Scalar => "scalar",
            FftBackend::Simd => "simd",
        }
        .into(),
        max_len: value.limits.max_len,
        max_bytes: value.limits.max_bytes,
        max_work: value.limits.max_work,
    }
}
fn decode_policy(value: &StoredPolicy, algorithm: SynthesisAlgorithm) -> ArtifactResult<Policy> {
    let result = Policy {
        algorithm,
        response_tolerance: scalar(value.response_tolerance)?,
        contractivity_margin: scalar(value.contractivity_margin)?,
        max_completion_grid: value.max_completion_grid,
        backend: match value.backend.as_str() {
            "scalar" => FftBackend::Scalar,
            "simd" => FftBackend::Simd,
            _ => return Err(ArtifactError::Invalid("FFT backend")),
        },
        limits: quest_numerics::Limits {
            max_len: value.max_len,
            max_bytes: value.max_bytes,
            max_work: value.max_work,
        },
    };
    result.validate()?;
    if value.max_len == 0 || value.max_bytes == 0 || value.max_work == 0 {
        return Err(ArtifactError::Invalid("zero producer limits"));
    }
    Ok(result)
}
const fn algorithm(value: SynthesisAlgorithm) -> &'static str {
    match value {
        SynthesisAlgorithm::RhwHalfCholesky => "rhw-half-cholesky",
        SynthesisAlgorithm::InverseNlftDivideConquer => "inverse-nlft-divide-conquer",
    }
}
fn count_storage(lengths: &[usize], limits: ArtifactLimits) -> ArtifactResult<()> {
    let mut total = 0usize;
    for &n in lengths {
        if n > limits.max_coefficients {
            return Err(ArtifactError::Budget("coefficient count"));
        }
        total = total
            .checked_add(n)
            .ok_or(ArtifactError::Budget("array length overflow"))?;
    }
    if total
        .checked_mul(1024)
        .is_none_or(|n| n > limits.max_decoded_bytes)
    {
        return Err(ArtifactError::Budget("decoded storage"));
    }
    Ok(())
}
fn encode<M: ArtifactMode>(
    candidate: &FrozenCandidate<M>,
    limits: ArtifactLimits,
    receipt: Option<Receipt>,
) -> ArtifactResult<Vec<u8>> {
    count_storage(
        &[
            candidate.target().len(),
            candidate.admitted.source.len(),
            candidate.controls.len(),
            candidate.phases.len(),
            candidate.a_star.len(),
        ],
        limits,
    )?;
    let payload = Payload {
        schema_version: 1,
        algorithm_version: 1,
        algorithm: algorithm(candidate.algorithm()).into(),
        precision_kind: match candidate.synthesis_precision() {
            SynthesisPrecision::Binary64 => "binary64",
            SynthesisPrecision::Arbitrary { .. } => "arbitrary",
        }
        .into(),
        precision_bits: match candidate.synthesis_precision() {
            SynthesisPrecision::Binary64 => None,
            SynthesisPrecision::Arbitrary { bits } => Some(bits),
        },
        convention: M::CONVENTION.into(),
        source_basis: if M::CONVENTION == RealParityWx::CONVENTION {
            "chebyshev"
        } else {
            "laurent-padded-nonnegative"
        }
        .into(),
        support_offset: 0,
        original_source_offset: candidate.admitted.source_offset,
        original_source_length: candidate.admitted.source_length,
        outer_gauge: "positive-real-constant".into(),
        policy: policy(candidate.admitted.policy),
        source: candidate
            .admitted
            .source
            .iter()
            .copied()
            .map(bits)
            .collect(),
        target: candidate.target().iter().copied().map(bits).collect(),
        complement: candidate.a_star.iter().copied().map(bits).collect(),
        controls: candidate
            .controls
            .iter()
            .map(|m| m.map(|r| r.map(bits)))
            .collect(),
        phases: candidate.phases.iter().map(|v| v.to_bits()).collect(),
        completion_grid: candidate.completion_grid,
        completion_residual: candidate.completion_residual.to_bits(),
        reconstruction_residual: candidate.reconstruction_residual.map(f64::to_bits),
        contractivity_upper: candidate.admitted.norm_upper.to_bits(),
        historical_receipt: receipt,
    };
    let canonical = serde_json::to_vec(&payload)?;
    if canonical.len() > limits.max_bytes {
        return Err(ArtifactError::Budget("encoded payload"));
    }
    let sha256 = Sha256::digest(&canonical).into();
    let output = serde_json::to_vec(&Envelope { payload, sha256 })?;
    if output.len() > limits.max_bytes {
        return Err(ArtifactError::Budget("encoded envelope"));
    }
    Ok(output)
}
/// Export complete frozen data as versioned JSON with exact IEEE bits and SHA256.
/// The digest detects accidental changes; it does not authenticate provenance.
/// # Errors
/// Rejects caller resource limits or serialization failure.
pub fn export_compiled<M: ArtifactMode>(
    candidate: &FrozenCandidate<M>,
    limits: ArtifactLimits,
) -> ArtifactResult<Vec<u8>> {
    encode(candidate, limits, None)
}
/// Export frozen data plus a digest-bound historical verification receipt.
/// Saved outward summaries never construct trusted evidence on loading.
/// # Errors
/// Rejects caller resource limits or serialization failure.
pub fn export_certified<M: ArtifactMode>(
    certified: &Certified<M>,
    limits: ArtifactLimits,
) -> ArtifactResult<Vec<u8>> {
    let report = certified.report();
    let p = report.policy();
    let values = [
        report.response(),
        report.completion(),
        report.conversion(),
        report.reconstruction(),
        report.unitarity(),
        &report.entries()[0],
        &report.entries()[1],
        &report.entries()[2],
        &report.entries()[3],
    ];
    let receipt = Receipt {
        status: "historical-outward-summary-requires-recertification".into(),
        initial_precision: p.initial_precision,
        max_precision: p.max_precision,
        method: match p.method {
            ConvolutionMethod::Direct => "direct",
            ConvolutionMethod::IntervalFft => "interval-fft",
        }
        .into(),
        tolerances: [
            p.response_tolerance,
            p.completion_tolerance,
            p.conversion_tolerance,
            p.reconstruction_tolerance,
            p.unitarity_tolerance,
        ]
        .map(f64::to_bits),
        limits: [p.max_coefficients, p.max_bytes, p.max_work],
        bounds: values.map(|bound| [bound.lower_f64().to_bits(), bound.upper_f64().to_bits()]),
        attempts: report
            .attempts()
            .iter()
            .map(|a| Attempt {
                precision: a.precision(),
                work: a.work_units(),
                modeled_peak_bytes: a.modeled_peak_bytes(),
                elapsed_seconds: a.elapsed().as_secs(),
                elapsed_nanoseconds: a.elapsed().subsec_nanos(),
            })
            .collect(),
    };
    encode(certified.candidate(), limits, Some(receipt))
}
#[expect(
    clippy::too_many_lines,
    reason = "All structural mode checks precede the single immutable candidate construction"
)]
fn decode<M: ArtifactMode>(
    payload: Payload,
    load: LoadPolicy,
    algorithm: SynthesisAlgorithm,
) -> ArtifactResult<FrozenCandidate<M>> {
    let producer = decode_policy(&payload.policy, algorithm)?;
    if !matches!(
        (payload.precision_kind.as_str(), payload.precision_bits),
        ("binary64", None) | ("arbitrary", Some(_))
    ) {
        return Err(ArtifactError::Invalid("precision identity"));
    }
    let precision = match payload.precision_bits {
        None => SynthesisPrecision::Binary64,
        Some(bits) if (64..=1_048_576).contains(&bits) => SynthesisPrecision::Arbitrary { bits },
        Some(_) => return Err(ArtifactError::Invalid("precision")),
    };
    if payload.completion_grid == 0
        || !payload.completion_grid.is_power_of_two()
        || payload.completion_grid > producer.max_completion_grid
    {
        return Err(ArtifactError::Invalid("completion grid"));
    }
    let start = usize::try_from(payload.original_source_offset)
        .map_err(|_| ArtifactError::Invalid("original source offset"))?;
    let end = start
        .checked_add(payload.original_source_length)
        .ok_or(ArtifactError::Invalid("original source span"))?;
    if end > payload.source.len()
        || (M::CONVENTION == RealParityWx::CONVENTION
            && (start != 0 || end != payload.source.len()))
        || payload
            .source
            .iter()
            .take(start)
            .any(|&v| v != bits(Complex64::new(0.0, 0.0)))
        || payload
            .source
            .iter()
            .skip(end)
            .any(|&v| v != bits(Complex64::new(0.0, 0.0)))
    {
        return Err(ArtifactError::Invalid("original source storage"));
    }
    let target = payload
        .target
        .into_iter()
        .map(complex)
        .collect::<ArtifactResult<Vec<_>>>()?;
    let source = payload
        .source
        .into_iter()
        .map(complex)
        .collect::<ArtifactResult<Vec<_>>>()?;
    let a_star = payload
        .complement
        .into_iter()
        .map(complex)
        .collect::<ArtifactResult<Vec<_>>>()?;
    let phases = payload
        .phases
        .into_iter()
        .map(scalar)
        .collect::<ArtifactResult<Vec<_>>>()?;
    let controls = payload
        .controls
        .into_iter()
        .map(|m| {
            Ok([
                [complex(m[0][0])?, complex(m[0][1])?],
                [complex(m[1][0])?, complex(m[1][1])?],
            ])
        })
        .collect::<ArtifactResult<Vec<Control>>>()?;
    if target.is_empty() || a_star.len() != target.len() || controls.len() != target.len() {
        return Err(ArtifactError::Invalid("export shape"));
    }
    if target.len() > producer.limits.max_len || source.len() > producer.limits.max_len {
        return Err(ArtifactError::Invalid("producer coefficient limit"));
    }
    if M::CONVENTION == RealParityWx::CONVENTION {
        if payload.source_basis != "chebyshev"
            || phases.len() != target.len()
            || phases
                .iter()
                .zip(phases.iter().rev())
                .any(|(a, b)| a.to_bits() != b.to_bits())
        {
            return Err(ArtifactError::Invalid("Wx phase convention"));
        }
        let degree = source
            .iter()
            .rposition(|z| *z != Complex64::new(0.0, 0.0))
            .unwrap_or(0);
        if source
            .iter()
            .enumerate()
            .any(|(i, z)| z.im != 0.0 || (i % 2 != degree % 2 && z.re != 0.0))
        {
            return Err(ArtifactError::Invalid("Wx source parity or reality"));
        }
    } else if payload.source_basis != "laurent-padded-nonnegative"
        || !phases.is_empty()
        || source.len() != target.len()
        || source.iter().zip(&target).any(|(&a, &b)| a != b)
    {
        return Err(ArtifactError::Invalid("unit-circle source convention"));
    }
    let residual = scalar(payload.completion_residual)?;
    let reconstruction = payload.reconstruction_residual.map(scalar).transpose()?;
    let historical_norm = scalar(payload.contractivity_upper)?;
    if residual < 0.0
        || reconstruction.is_some_and(|r| r < 0.0)
        || !(0.0..1.0).contains(&historical_norm)
    {
        return Err(ArtifactError::Invalid("diagnostic range"));
    }
    // Re-establish target admissibility with loader-owned resources. Recorded
    // contractivity bounds are not treated as proof from an untrusted file.
    let mut admission = load.admission;
    admission.contractivity_margin = admission
        .contractivity_margin
        .max(producer.contractivity_margin);
    admission.validate()?;
    if target.len() > admission.limits.max_len {
        return Err(ArtifactError::Budget("admission length"));
    }
    let norm_upper = crate::admission::contractivity(&target, admission)?;
    Ok(FrozenCandidate {
        synthesis_precision: precision,
        admitted: AdmittedTarget {
            source_offset: payload.original_source_offset,
            source_length: payload.original_source_length,
            target: Arc::new(target),
            source: Arc::new(source),
            norm_upper,
            policy: producer,
            _mode: PhantomData,
        },
        controls: Arc::new(controls),
        a_star: Arc::new(a_star),
        phases: Arc::new(phases),
        completion_residual: residual,
        reconstruction_residual: reconstruction,
        completion_grid: payload.completion_grid,
    })
}
fn validate_receipt(receipt: &Receipt) -> ArtifactResult<()> {
    if receipt.attempts.is_empty()
        || receipt.attempts.len() > 32
        || receipt.status != "historical-outward-summary-requires-recertification"
    {
        return Err(ArtifactError::Invalid("historical receipt"));
    }
    let tolerances = receipt.tolerances.map(scalar);
    let [
        response_tolerance,
        completion_tolerance,
        conversion_tolerance,
        reconstruction_tolerance,
        unitarity_tolerance,
    ] = tolerances;
    CertificationPolicy {
        initial_precision: receipt.initial_precision,
        max_precision: receipt.max_precision,
        response_tolerance: response_tolerance?,
        completion_tolerance: completion_tolerance?,
        conversion_tolerance: conversion_tolerance?,
        reconstruction_tolerance: reconstruction_tolerance?,
        unitarity_tolerance: unitarity_tolerance?,
        method: match receipt.method.as_str() {
            "direct" => ConvolutionMethod::Direct,
            "interval-fft" => ConvolutionMethod::IntervalFft,
            _ => return Err(ArtifactError::Invalid("historical method")),
        },
        max_coefficients: receipt.limits[0],
        max_bytes: receipt.limits[1],
        max_work: receipt.limits[2],
    }
    .validate()?;
    let mut previous = 0;
    for attempt in &receipt.attempts {
        if attempt.precision < receipt.initial_precision
            || attempt.precision > receipt.max_precision
            || attempt.precision <= previous
            || attempt.elapsed_nanoseconds >= 1_000_000_000
        {
            return Err(ArtifactError::Invalid("historical attempt"));
        }
        previous = attempt.precision;
    }
    for bound in receipt.bounds {
        let lo = scalar(bound[0])?;
        let hi = scalar(bound[1])?;
        if lo < 0.0 || lo > hi {
            return Err(ArtifactError::Invalid("historical bound"));
        }
    }
    for tolerance in receipt.tolerances {
        if scalar(tolerance)? <= 0.0 {
            return Err(ArtifactError::Invalid("historical tolerance"));
        }
    }

    Ok(())
}
/// Bounded structural loading, digest checking and fresh contractivity admission.
/// No accuracy certificate or algorithm provenance is inferred from stored metadata.
/// # Errors
/// Rejects malformed/nonfinite payloads, unsupported versions and caller budgets.
pub fn load_compiled(bytes: &[u8], policy: LoadPolicy) -> ArtifactResult<LoadedCompiled> {
    if bytes.len() > policy.storage.max_bytes {
        return Err(ArtifactError::Budget("encoded bytes"));
    }
    // At least one input byte per JSON scalar; 32-fold covers u64 vectors,
    // duplicate-field rejection state, canonical serialization and frozen copies.
    if bytes
        .len()
        .checked_mul(32)
        .is_none_or(|n| n > policy.storage.max_decoded_bytes)
    {
        return Err(ArtifactError::Budget("JSON decoding storage"));
    }
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    let p = envelope.payload;
    count_storage(
        &[
            p.source.len(),
            p.target.len(),
            p.complement.len(),
            p.controls.len(),
            p.phases.len(),
        ],
        policy.storage,
    )?;
    if p.schema_version != 1
        || p.algorithm_version != 1
        || p.support_offset != 0
        || p.outer_gauge != "positive-real-constant"
    {
        return Err(ArtifactError::Invalid(
            "schema, algorithm version, support or gauge",
        ));
    }
    if let Some(receipt) = &p.historical_receipt {
        validate_receipt(receipt)?;
    }
    let hash: [u8; 32] = Sha256::digest(serde_json::to_vec(&p)?).into();
    if hash != envelope.sha256 {
        return Err(ArtifactError::Invalid("payload SHA256 mismatch"));
    }
    let algorithm = match p.algorithm.as_str() {
        "rhw-half-cholesky" => SynthesisAlgorithm::RhwHalfCholesky,
        "inverse-nlft-divide-conquer" => SynthesisAlgorithm::InverseNlftDivideConquer,
        _ => return Err(ArtifactError::Invalid("algorithm")),
    };
    match p.convention.as_str() {
        RealParityWx::CONVENTION => Ok(LoadedCompiled::RealParityWx(decode(p, policy, algorithm)?)),
        UnitCircleResponse::CONVENTION => Ok(LoadedCompiled::UnitCircleResponse(decode(
            p, policy, algorithm,
        )?)),
        _ => Err(ArtifactError::Invalid("response convention")),
    }
}
/// Load then independently reconstruct and certify the exact saved payload.
///
/// Saved receipts are never reused as acceptance evidence. Projector conversion
/// remains an explicit second certification on the returned Wx certificate.
/// # Errors
/// Includes every structural error and independent certification failure.
pub fn load_certified(
    bytes: &[u8],
    load: LoadPolicy,
    certification: CertificationPolicy,
) -> ArtifactResult<LoadedCertified> {
    match load_compiled(bytes, load)? {
        LoadedCompiled::RealParityWx(candidate) => Ok(LoadedCertified::RealParityWx(
            CertificationBuilder::new()
                .candidate(candidate)
                .policy(certification)?
                .certify()?,
        )),
        LoadedCompiled::UnitCircleResponse(candidate) => Ok(LoadedCertified::UnitCircleResponse(
            CertificationBuilder::new()
                .candidate(candidate)
                .policy(certification)?
                .certify()?,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quest_polynomial::{Chebyshev, Limits, Polynomial};
    fn receipt() -> Vec<u8> {
        let polynomial =
            Polynomial::new(Chebyshev, vec![Complex64::new(0.3, 0.0)], Limits::default()).unwrap();
        let candidate = crate::SynthesisBuilder::new()
            .real_parity_wx(&polynomial)
            .unwrap()
            .admit()
            .unwrap()
            .complete()
            .unwrap()
            .synthesize()
            .unwrap();
        let certified = CertificationBuilder::new()
            .candidate(candidate)
            .policy(CertificationPolicy::default())
            .unwrap()
            .certify()
            .unwrap();
        export_certified(&certified, ArtifactLimits::default()).unwrap()
    }
    fn resign(mut envelope: Envelope) -> Vec<u8> {
        envelope.sha256 = Sha256::digest(serde_json::to_vec(&envelope.payload).unwrap()).into();
        serde_json::to_vec(&envelope).unwrap()
    }
    #[test]
    fn forged_receipt_never_replaces_independent_payload_verification() {
        let mut envelope: Envelope = serde_json::from_slice(&receipt()).unwrap();
        envelope.payload.phases[0] = 0.8f64.to_bits();
        envelope.payload.historical_receipt.as_mut().unwrap().bounds = [[0, 0]; 9];
        let bytes = resign(envelope);
        assert!(load_compiled(&bytes, LoadPolicy::default()).is_ok());
        assert!(matches!(
            load_certified(
                &bytes,
                LoadPolicy::default(),
                CertificationPolicy::default()
            ),
            Err(ArtifactError::Certification(_))
        ));
    }
    #[test]
    fn rejects_unknown_versions_nonfinite_values_and_forged_policy() {
        for mutation in [0, 1, 2, 3, 4] {
            let mut envelope: Envelope = serde_json::from_slice(&receipt()).unwrap();
            match mutation {
                0 => envelope.payload.schema_version = 2,
                1 => envelope.payload.algorithm_version = 2,
                2 => envelope.payload.controls[0][0][0][0] = f64::NAN.to_bits(),
                3 => envelope.payload.policy.contractivity_margin = 0.0f64.to_bits(),
                _ => envelope.payload.precision_kind = "unrecognized".into(),
            }
            assert!(load_compiled(&resign(envelope), LoadPolicy::default()).is_err());
        }
    }
    #[test]
    fn zero_storage_and_actual_projector_recertification() {
        let bytes = receipt();
        assert!(
            load_compiled(
                &bytes,
                LoadPolicy {
                    storage: ArtifactLimits {
                        max_decoded_bytes: 0,
                        ..ArtifactLimits::default()
                    },
                    ..LoadPolicy::default()
                }
            )
            .is_err()
        );
        let LoadedCertified::RealParityWx(loaded) = load_certified(
            &bytes,
            LoadPolicy::default(),
            CertificationPolicy::default(),
        )
        .unwrap() else {
            panic!("mode")
        };
        let converted = loaded
            .certify_projector_phases(CertificationPolicy::default())
            .unwrap();
        assert!(converted.response_bound().upper_f64() <= 1e-11);
    }
}
