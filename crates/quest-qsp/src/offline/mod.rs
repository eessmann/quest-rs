//! Explicit offline arbitrary-precision synthesis from original inputs, never a production fallback.
//!
//! Construction consumes original polynomial snapshots, computes at the selected
//! binary precision, exports binary64 controls once, and independently certifies that
//! export. A retry reconstructs all numerical state from the original coefficients.
//! Certification failure retains the original input and last frozen export.
//! The candidate production reconstruction diagnostic is unavailable (`+infinity`);
//! use the finite independent reconstruction bound in the certified report.
//!
//! Available only with `offline-synthesis`, which also enables `certification`.
//! [`OfflineBuilder`] consumes an original polynomial, an explicit
//! [`OfflinePolicy`] and then a `solve` request. Production synthesis never
//! invokes it. `UnitCircleResponse` input needs nonnegative Laurent support; `real_parity_wx`
//! input needs real Chebyshev coefficients with exactly one parity. Neither
//! path projects, chops or rescales the source.
//!
//! Computation starts at 128 bits by default and can double to 4096 bits.
//! Precision limits are multiples of the backend word size. Each attempt
//! computes in arbitrary precision, exports binary64 values and invokes the
//! separate verifier. [`OfflineSolution`] is available only after that export
//! passes independent certification. More computation precision cannot promise
//! success below the final binary64 export's attainable error.
//!
//! [`OfflineRemezBuilder`] is the separate function-approximation entry point.
//! Its total uniform-error tolerance differs from the binary64 polynomial
//! builder's minimax-gap tolerance. The numerical exchange gap is not a minimax
//! certificate; the final exported polynomial receives a full-domain error bound.
//!
//! ```
//! use quest_polynomial::{Basis, Laurent, Limits, Polynomial};
//! use quest_qsp::{Complex64, offline::{OfflineBuilder, OfflinePolicy}};
//! let target = Polynomial::new(Laurent::new(0),
//!     vec![Complex64::new(0.3, 0.4)], Limits::default())?;
//! let solved = OfflineBuilder::new().unit_circle_response(&target)?
//!     .policy(OfflinePolicy::default())?.solve()?;
//! assert!(solved.certified().report().response().upper_f64() <= 1e-11);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use crate::precision::{BinaryRounding, checked, exact_from_f64, to_f64};
mod fft;
mod kernels;
mod number;
mod remez;
use crate::certification::{
    CertificationBuilder, CertificationError, CertificationMode, CertificationPolicy, Certified,
    MpComplex, MpInterval,
};
use crate::{
    AdmittedTarget, Complex64, Control, FrozenCandidate, RealParityWx, UnitCircleResponse,
};
use astro_float::{BigFloat, Consts, RoundingMode};
use number::Number;
use quest_polynomial::{Basis, Chebyshev, Laurent, Polynomial};
pub use remez::{
    ApproximationFailure, FunctionDomain, MissingFunction, OfflineApproximation,
    OfflineRemezBuilder, OfflineRemezPolicy, OriginalFunction, ReadyRemez,
};
use std::{
    collections::BTreeMap,
    marker::PhantomData,
    sync::Arc,
    time::{Duration, Instant},
};
/// Result of explicit arbitrary-precision synthesis or approximation.
pub type OfflineResult<T> = std::result::Result<T, OfflineError>;
/// Typed offline failures, with reports when final precision or certification fails.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OfflineError {
    #[error(transparent)]
    Precision(#[from] crate::precision::PrecisionError),
    #[error("offline constant cache: {0:?}")]
    Constants(astro_float::Error),
    #[error("offline policy: {0}")]
    Policy(&'static str),
    #[error("offline source domain: {0}")]
    Domain(&'static str),
    #[error("offline resource budget: {0}")]
    Budget(&'static str),
    #[error("offline numerical computation: {0}")]
    Numerical(&'static str),
    #[error("offline precision budget exhausted during {stage}")]
    NotEstablished {
        stage: &'static str,
        report: Box<OfflineReport>,
    },
    #[error("the frozen binary64 offline export did not satisfy independent certification")]
    Certification {
        report: Box<OfflineReport>,
        source: Box<CertificationError>,
    },
    #[error("uniform approximation not established within configured precision and error budgets")]
    ApproximationNotEstablished { report: Box<ApproximationFailure> },
    #[error(transparent)]
    Polynomial(#[from] quest_polynomial::Error),
    #[error(transparent)]
    Numerics(#[from] quest_numerics::Error),
    #[error("offline rigorous numerical admission failed: {0}")]
    Admission(#[from] CertificationError),
}
/// Computation precision and resources, with a separate final verification policy.
///
/// Precision limits are whole backend words and must satisfy
/// `64 <= initial_precision <= max_precision <= 1_048_576`. Byte and work
/// limits model algorithmic resources; backend internal temporary allocation
/// and transcendental iterations are not a process-wide quota.
#[derive(Clone, Copy, Debug)]
pub struct OfflinePolicy {
    /// Factorization, independent of arithmetic precision; never silently changed.
    pub algorithm: crate::SynthesisAlgorithm,
    /// First computation attempt's precision in bits (default 128).
    pub initial_precision: u32,
    /// Last permitted computation precision in bits (default 4096).
    pub max_precision: u32,
    /// Maximum completion/admission FFT sample count.
    pub max_grid: usize,
    /// Maximum source and target coefficient count.
    pub max_coefficients: usize,
    /// Maximum modeled arbitrary-precision computation storage in bytes.
    pub max_bytes: usize,
    /// Precision-weighted computation work across retries; verifier work is separate.
    pub max_work: usize,
    /// Strict positive gap required from unit-circle magnitude one.
    pub contractivity_margin: f64,
    /// Independent verification configuration for each frozen binary64 export.
    pub certification: CertificationPolicy,
}
impl Default for OfflinePolicy {
    fn default() -> Self {
        Self {
            algorithm: crate::SynthesisAlgorithm::default(),
            initial_precision: 128,
            max_precision: 4096,
            max_grid: 1_048_576,
            max_coefficients: 1_048_576,
            max_bytes: usize::try_from(4_294_967_296_u64).unwrap_or(usize::MAX),
            max_work: usize::try_from(1_099_511_627_776_u64).unwrap_or(usize::MAX),
            contractivity_margin: 1e-12,
            certification: CertificationPolicy::default(),
        }
    }
}
impl OfflinePolicy {
    fn validate(self) -> OfflineResult<()> {
        let word_bits = u32::try_from(astro_float::WORD_BIT_SIZE)
            .map_err(|_| OfflineError::Policy("backend word size"))?;
        if !self.initial_precision.is_multiple_of(word_bits)
            || !self.max_precision.is_multiple_of(word_bits)
        {
            return Err(OfflineError::Policy(
                "precision must be a whole backend word",
            ));
        }
        if self.initial_precision < 64
            || self.max_precision < self.initial_precision
            || self.max_precision > 1_048_576
        {
            return Err(OfflineError::Policy(
                "64 <= initial precision <= maximum <= 1048576 required",
            ));
        }
        if !self.contractivity_margin.is_finite()
            || self.contractivity_margin <= 0.0
            || self.contractivity_margin >= 1.0
        {
            return Err(OfflineError::Policy(
                "strict finite contractivity margin required",
            ));
        }
        if self.max_grid == 0
            || self.max_coefficients == 0
            || self.max_bytes == 0
            || self.max_work == 0
        {
            return Err(OfflineError::Policy("nonzero budgets required"));
        }
        let certification = self.certification;
        if !certification.initial_precision.is_multiple_of(word_bits)
            || !certification.max_precision.is_multiple_of(word_bits)
            || certification.initial_precision < 64
            || certification.max_precision < certification.initial_precision
            || certification.max_precision > 1_048_576
            || certification.max_coefficients == 0
            || certification.max_bytes == 0
            || certification.max_work == 0
        {
            return Err(OfflineError::Policy(
                "invalid independent certification precision or budgets",
            ));
        }
        for tolerance in [
            certification.response_tolerance,
            certification.completion_tolerance,
            certification.conversion_tolerance,
            certification.reconstruction_tolerance,
            certification.unitarity_tolerance,
        ] {
            if !tolerance.is_finite() || tolerance <= 0.0 {
                return Err(OfflineError::Policy(
                    "independent certification tolerances must be finite and positive",
                ));
            }
        }
        Ok(())
    }
}
/// Offline builder state before source coefficients are supplied.
#[derive(Debug)]
pub struct MissingTarget;
/// Retained original coefficients before precision/resource policy admission.
#[derive(Debug)]
pub struct OriginalTarget<M> {
    source_offset: i32,
    source_length: usize,
    source: Arc<Vec<Complex64>>,
    degree: usize,
    mode: PhantomData<M>,
}
/// Original target and admitted policy, ready for an explicit `solve` request.
#[derive(Debug)]
pub struct ReadyOffline<M> {
    target: OriginalTarget<M>,
    policy: OfflinePolicy,
}
/// Explicit original-input arbitrary-precision computation and independently checked binary64 export.
#[derive(Debug)]
pub struct OfflineBuilder<S = MissingTarget> {
    state: S,
}
impl Default for OfflineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
impl OfflineBuilder {
    /// Start an explicit offline request without source data.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: MissingTarget,
        }
    }
    /// Copy original `unit_circle_response` coefficients, padding any positive offset.
    /// # Errors
    /// Rejects negative Laurent powers or support overflow.
    pub fn unit_circle_response(
        self,
        polynomial: &Polynomial<Laurent>,
    ) -> OfflineResult<OfflineBuilder<OriginalTarget<UnitCircleResponse>>> {
        let (offset, last) = polynomial
            .stored_support()
            .unwrap_or_else(|| (polynomial.basis().offset(), polynomial.basis().offset()));
        if offset < 0 {
            return Err(OfflineError::Domain(
                "offline unit_circle_response target needs nonnegative powers",
            ));
        }
        let count = usize::try_from(last)
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(OfflineError::Budget("source support"))?;
        if count > OfflinePolicy::default().max_coefficients {
            return Err(OfflineError::Budget("source support"));
        }
        let mut source = vec![Complex64::new(0.0, 0.0); count];
        let offset = usize::try_from(offset).map_err(|_| OfflineError::Budget("source support"))?;
        let end = offset
            .checked_add(polynomial.coefficients().len())
            .ok_or(OfflineError::Budget("source support"))?;
        source
            .get_mut(offset..end)
            .ok_or(OfflineError::Budget("source support"))?
            .copy_from_slice(polynomial.coefficients());
        Ok(OfflineBuilder {
            state: OriginalTarget {
                source_offset: polynomial.basis().offset(),
                source_length: polynomial.coefficients().len(),
                source: Arc::new(source),
                degree: count.saturating_sub(1),
                mode: PhantomData,
            },
        })
    }
    /// Copy original real Chebyshev coefficients while preserving exact parity.
    /// # Errors
    /// Rejects complex coefficients or mixed exact parity; no source coefficients are projected.
    pub fn real_parity_wx(
        self,
        polynomial: &Polynomial<Chebyshev>,
    ) -> OfflineResult<OfflineBuilder<OriginalTarget<RealParityWx>>> {
        let source = polynomial.coefficients();
        let degree = source
            .iter()
            .rposition(|c| *c != Complex64::new(0.0, 0.0))
            .unwrap_or(0);
        if source
            .iter()
            .enumerate()
            .any(|(index, c)| c.im != 0.0 || (index % 2 != degree % 2 && c.re != 0.0))
        {
            return Err(OfflineError::Domain(
                "real_parity_wx source requires exact real parity",
            ));
        }
        Ok(OfflineBuilder {
            state: OriginalTarget {
                source_offset: 0,
                source_length: source.len(),
                source: Arc::new(source.to_vec()),
                degree,
                mode: PhantomData,
            },
        })
    }
}
impl<M: CertificationMode> OfflineBuilder<OriginalTarget<M>> {
    /// Admit computation and independent-verification resource policies.
    /// # Errors
    /// Rejects invalid precision/resource controls or impossible source storage.
    pub fn policy(self, policy: OfflinePolicy) -> OfflineResult<OfflineBuilder<ReadyOffline<M>>> {
        policy.validate()?;
        let count = self
            .state
            .degree
            .checked_add(1)
            .ok_or(OfflineError::Budget("degree"))?;
        if count > policy.max_coefficients || self.state.source.len() > policy.max_coefficients {
            return Err(OfflineError::Budget("source coefficients"));
        }
        Context::new(count, policy.initial_precision, policy)?;
        Ok(OfflineBuilder {
            state: ReadyOffline {
                target: self.state,
                policy,
            },
        })
    }
}
impl<M: CertificationMode> OfflineBuilder<ReadyOffline<M>> {
    /// Compute from the original source and certify the frozen binary64 export.
    /// # Errors
    /// Reports numerical/budget exhaustion or independent rejection of the final export.
    pub fn solve(self) -> OfflineResult<OfflineSolution<M>> {
        solve(&self.state.target, self.state.policy)
    }
}
/// Successful offline computation with a certified immutable binary64 candidate.
#[derive(Debug)]
pub struct OfflineSolution<M> {
    certified: Certified<M>,
    report: OfflineReport,
}
impl<M> OfflineSolution<M> {
    /// Frozen binary64 export and its independent numerical certificate.
    #[must_use]
    pub const fn certified(&self) -> &Certified<M> {
        &self.certified
    }
    /// Original source, attempt timings and final export provenance.
    #[must_use]
    pub const fn report(&self) -> &OfflineReport {
        &self.report
    }
    /// Recover ownership of the certified candidate, discarding offline provenance.
    #[must_use]
    pub fn into_certified(self) -> Certified<M> {
        self.certified
    }
}
/// Immutable last exported bytes, retained even if certification fails.
#[derive(Debug, Clone)]
pub struct ExportSnapshot {
    target: Arc<Vec<Complex64>>,
    complement: Arc<Vec<Complex64>>,
    controls: Arc<Vec<Control>>,
    phases: Arc<Vec<f64>>,
}
impl ExportSnapshot {
    /// Last exported target in increasing nonnegative exponents.
    #[must_use]
    pub fn target(&self) -> &[Complex64] {
        &self.target
    }
    /// Last exported conjugated complement in increasing nonnegative exponents.
    #[must_use]
    pub fn complement(&self) -> &[Complex64] {
        &self.complement
    }
    /// Last exported matrices including terminal K, in product order.
    #[must_use]
    pub fn controls(&self) -> &[Control] {
        &self.controls
    }
    /// Last `real_parity_wx` Wx angles in radians; empty for `unit_circle_response` synthesis.
    #[must_use]
    pub fn phases(&self) -> &[f64] {
        &self.phases
    }
    fn from_candidate<M>(candidate: &FrozenCandidate<M>) -> Self {
        Self {
            target: Arc::clone(&candidate.admitted.target),
            complement: Arc::clone(&candidate.a_star),
            controls: Arc::clone(&candidate.controls),
            phases: Arc::clone(&candidate.phases),
        }
    }
}
/// One offline attempt, separating computation work from verification time.
#[derive(Debug, Clone)]
pub struct OfflineAttempt {
    precision: u32,
    grid: usize,
    work: usize,
    computation: Duration,
    certification: Duration,
    completion_residual: Option<BigFloat>,
}
impl OfflineAttempt {
    /// Actual computation precision in bits.
    #[must_use]
    pub const fn precision(&self) -> u32 {
        self.precision
    }
    /// Accepted completion grid, or zero if computation did not complete.
    #[must_use]
    pub const fn completion_grid(&self) -> usize {
        self.grid
    }
    /// Computation work charged during this attempt, excluding verification.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }
    /// Computation wall time before independent certification.
    #[must_use]
    pub const fn computation_elapsed(&self) -> Duration {
        self.computation
    }
    /// Separate wall time spent certifying this attempt's binary64 export.
    #[must_use]
    pub const fn certification_elapsed(&self) -> Duration {
        self.certification
    }
    /// Arbitrary-precision completion diagnostic, when computation completed.
    #[must_use]
    pub const fn completion_residual(&self) -> Option<&BigFloat> {
        self.completion_residual.as_ref()
    }
}
/// Retained source and attempts for an offline result or reported final failure.
#[derive(Debug, Clone)]
pub struct OfflineReport {
    source: Arc<Vec<Complex64>>,
    attempts: Vec<OfflineAttempt>,
    last_export: Option<ExportSnapshot>,
    contractivity_upper: Option<BigFloat>,
}
impl OfflineReport {
    /// Original Chebyshev source or zero-padded `unit_circle_response` power coefficients.
    #[must_use]
    pub fn source_coefficients(&self) -> &[Complex64] {
        &self.source
    }
    /// Recorded attempts in increasing precision order.
    #[must_use]
    pub fn attempts(&self) -> &[OfflineAttempt] {
        &self.attempts
    }
    /// Last frozen export, present even when its certification did not succeed.
    #[must_use]
    pub const fn last_export(&self) -> Option<&ExportSnapshot> {
        self.last_export.as_ref()
    }
    /// Directed contractivity upper bound from the last completed computation.
    #[must_use]
    pub const fn contractivity_upper(&self) -> Option<&BigFloat> {
        self.contractivity_upper.as_ref()
    }
}
struct Context {
    constants: Consts,
    precision: u32,
    policy: OfflinePolicy,
    work: usize,
    bytes: usize,
    count: usize,
    roots: BTreeMap<usize, Arc<Vec<Number>>>,
}
fn modeled_scalar_bytes(precision: u32) -> OfflineResult<usize> {
    usize::try_from(precision)
        .map_err(|_| OfflineError::Budget("precision bytes"))?
        .div_ceil(8)
        .checked_add(size_of::<BigFloat>())
        .ok_or(OfflineError::Budget("precision bytes"))
}
impl Context {
    fn new(count: usize, precision: u32, policy: OfflinePolicy) -> OfflineResult<Self> {
        let grid = count
            .checked_next_power_of_two()
            .ok_or(OfflineError::Budget("offline source support"))?;
        let bytes = Self::modeled_storage(count, precision, policy, grid)?;
        Ok(Self {
            constants: Consts::new().map_err(OfflineError::Constants)?,
            precision,
            policy,
            work: 0,
            bytes,
            count,
            roots: BTreeMap::new(),
        })
    }
    fn exp(&mut self, value: &BigFloat) -> OfflineResult<BigFloat> {
        Ok(checked(value.exp(
            number::precision_bits(self.precision),
            RoundingMode::ToEven,
            &mut self.constants,
        ))?)
    }
    fn ln(&mut self, value: &BigFloat) -> OfflineResult<BigFloat> {
        Ok(checked(value.ln(
            number::precision_bits(self.precision),
            RoundingMode::ToEven,
            &mut self.constants,
        ))?)
    }
    fn sin(&mut self, value: &BigFloat) -> OfflineResult<BigFloat> {
        Ok(checked(value.sin(
            number::precision_bits(self.precision),
            RoundingMode::ToEven,
            &mut self.constants,
        ))?)
    }
    fn cos(&mut self, value: &BigFloat) -> OfflineResult<BigFloat> {
        Ok(checked(value.cos(
            number::precision_bits(self.precision),
            RoundingMode::ToEven,
            &mut self.constants,
        ))?)
    }
    fn charge(&mut self, units: usize) -> OfflineResult<()> {
        let limbs = usize::try_from(self.precision)
            .map_err(|_| OfflineError::Budget("offline precision"))?
            .div_ceil(64);
        self.consume(
            units
                .checked_mul(limbs)
                .ok_or(OfflineError::Budget("offline work"))?,
        )
    }
    fn consume(&mut self, units: usize) -> OfflineResult<()> {
        self.work = self
            .work
            .checked_add(units)
            .ok_or(OfflineError::Budget("offline work"))?;
        if self.work > self.policy.max_work {
            return Err(OfflineError::Budget("offline work"));
        }
        Ok(())
    }
    fn admit_grid(&mut self, grid: usize) -> OfflineResult<()> {
        self.bytes = self.bytes.max(Self::modeled_storage(
            self.count,
            self.precision,
            self.policy,
            grid,
        )?);
        Ok(())
    }
    fn modeled_storage(
        count: usize,
        precision: u32,
        policy: OfflinePolicy,
        grid: usize,
    ) -> OfflineResult<usize> {
        if grid > policy.max_grid {
            return Err(OfflineError::Budget("offline grid"));
        }
        let scalar_bytes = modeled_scalar_bytes(precision)?;
        let levels = usize::try_from(count.max(1).ilog2())
            .map_err(|_| OfflineError::Budget("offline levels"))?
            .saturating_add(1);
        let storage = count
            .checked_mul(levels)
            .and_then(|n| n.checked_mul(32))
            .and_then(|n| grid.checked_mul(32).and_then(|g| n.checked_add(g)))
            .and_then(|n| n.checked_mul(scalar_bytes))
            .ok_or(OfflineError::Budget("offline memory"))?;
        if storage > policy.max_bytes {
            return Err(OfflineError::Budget("offline modeled memory"));
        }
        Ok(storage)
    }
}
fn original<M: CertificationMode>(
    source: &OriginalTarget<M>,
    context: &mut Context,
) -> OfflineResult<Vec<Number>> {
    context.charge(
        source
            .source
            .len()
            .checked_mul(8)
            .ok_or(OfflineError::Budget("source work"))?,
    )?;
    let p = context.precision;
    if !M::CANONICAL {
        return source
            .source
            .iter()
            .map(|value| Number::exact(*value, p))
            .collect();
    }
    let count = source
        .degree
        .checked_add(1)
        .ok_or(OfflineError::Budget("source degree"))?;
    let half = exact_from_f64(0.5, p)?;
    let mut result = vec![Number::zero(p); count];
    for (index, value) in source.source.iter().enumerate().take(count) {
        if index % 2 != source.degree % 2 {
            continue;
        }
        let value = Number::exact(*value, p)?.scale(&half);
        let high = source
            .degree
            .checked_add(index)
            .ok_or(OfflineError::Budget("source conversion"))?
            / 2;
        let low = source
            .degree
            .checked_sub(index)
            .ok_or(OfflineError::Budget("source conversion"))?
            / 2;
        for slot in [high, low] {
            let out = result
                .get_mut(slot)
                .ok_or(OfflineError::Numerical("conversion support"))?;
            *out = out.add(&value);
        }
    }
    Ok(result)
}
fn contractivity(target: &[Number], context: &mut Context) -> OfflineResult<BigFloat> {
    let p = context.precision;
    let exact: Vec<_> = target
        .iter()
        .map(|value| {
            Ok(MpComplex::new(
                MpInterval::bounds(value.re.clone(), value.re.clone(), p)?,
                MpInterval::bounds(value.im.clone(), value.im.clone(), p)?,
            ))
        })
        .collect::<std::result::Result<_, CertificationError>>()?;
    let mut norm = BigFloat::from_i64(0, number::precision_bits(p));
    let mut derivative = BigFloat::from_i64(0, number::precision_bits(p));
    for (index, value) in exact.iter().enumerate() {
        let magnitude = value.magnitude()?;
        norm = checked(norm.add(
            magnitude.upper(),
            number::precision_bits(p),
            RoundingMode::Up,
        ))?;
        let term = checked(magnitude.upper().mul(
            &BigFloat::from_u64(
                u64::try_from(index).map_err(|_| OfflineError::Budget("integer interchange"))?,
                number::precision_bits(p),
            ),
            number::precision_bits(p),
            RoundingMode::Up,
        ))?;
        derivative = checked(derivative.add(&term, number::precision_bits(p), RoundingMode::Up))?;
    }
    let threshold = checked(BigFloat::from_u64(1, number::precision_bits(p)).sub(
        &exact_from_f64(context.policy.contractivity_margin, p)?,
        number::precision_bits(p),
        RoundingMode::Down,
    ))?;
    if norm < threshold {
        return Ok(norm);
    }
    let pi = checked(
        context
            .constants
            .pi(number::precision_bits(p), RoundingMode::Up),
    )?;
    let mut grid = target
        .len()
        .checked_mul(4)
        .and_then(usize::checked_next_power_of_two)
        .ok_or(OfflineError::Budget("admission grid"))?
        .max(32);
    while grid <= context.policy.max_grid {
        let policy = CertificationPolicy {
            max_bytes: context.policy.max_bytes,
            max_work: context
                .policy
                .max_work
                .checked_sub(context.work)
                .ok_or(OfflineError::Budget("admission work"))?,
            ..context.policy.certification
        };
        let (samples, work, bytes) = crate::certification::circle_values(&exact, grid, p, policy)?;
        context.consume(work)?;
        context.bytes = context.bytes.max(bytes);
        let mut maximum = BigFloat::from_i64(0, number::precision_bits(p));
        for value in samples {
            let magnitude = value.magnitude()?;
            if magnitude.upper() > &maximum {
                maximum.clone_from(magnitude.upper());
            }
        }
        let numerator = checked(pi.mul(&derivative, number::precision_bits(p), RoundingMode::Up))?;
        let correction = checked(numerator.div(
            &BigFloat::from_u64(
                u64::try_from(grid).map_err(|_| OfflineError::Budget("integer interchange"))?,
                number::precision_bits(p),
            ),
            number::precision_bits(p),
            RoundingMode::Up,
        ))?;
        let upper = checked(maximum.add(&correction, number::precision_bits(p), RoundingMode::Up))?;
        if upper < threshold {
            return Ok(upper);
        }
        grid = grid
            .checked_mul(2)
            .ok_or(OfflineError::Budget("admission refinement"))?;
    }
    Err(OfflineError::Numerical(
        "strict contractivity not established on complete circle",
    ))
}
#[expect(
    clippy::too_many_arguments,
    reason = "The freeze boundary names each original arbitrary-precision stage result explicitly"
)]
fn freeze<M: CertificationMode>(
    original: &OriginalTarget<M>,
    target: &[Number],
    astar: &[Number],
    gamma: &[Number],
    norm: &BigFloat,
    residual: &BigFloat,
    grid: usize,
    context: &mut Context,
) -> OfflineResult<FrozenCandidate<M>> {
    let (phases, mut matrices) = if M::CANONICAL {
        kernels::real_parity_wx_phases(gamma, context)?
    } else {
        (Vec::new(), kernels::controls(gamma, context)?)
    };
    let last = matrices
        .last_mut()
        .ok_or(OfflineError::Numerical("empty offline export"))?;
    let [a, b, c, d] = last.clone();
    *last = [b, a.neg(), d, c.neg()];
    let controls: Vec<_> = matrices
        .iter()
        .map(|matrix| {
            let [a, b, c, d] = matrix;
            Ok([
                [a.binary64()?, b.binary64()?],
                [c.binary64()?, d.binary64()?],
            ])
        })
        .collect::<OfflineResult<_>>()?;
    let target: Vec<_> = target
        .iter()
        .map(Number::binary64)
        .collect::<OfflineResult<_>>()?;
    let astar: Vec<_> = astar
        .iter()
        .map(Number::binary64)
        .collect::<OfflineResult<_>>()?;
    let phases: Vec<_> = phases
        .iter()
        .map(|phase| to_f64(phase, BinaryRounding::Nearest))
        .collect::<std::result::Result<_, _>>()?;
    if controls
        .iter()
        .flatten()
        .flatten()
        .chain(&target)
        .chain(&astar)
        .any(|v| !v.re.is_finite() || !v.im.is_finite())
        || phases.iter().any(|v| !v.is_finite())
    {
        return Err(OfflineError::Numerical("nonfinite binary64 export"));
    }
    let policy = crate::Policy {
        algorithm: context.policy.algorithm,
        response_tolerance: context.policy.certification.response_tolerance,
        contractivity_margin: context.policy.contractivity_margin,
        max_completion_grid: context.policy.max_grid,
        limits: quest_numerics::Limits {
            max_len: context.policy.max_coefficients,
            max_bytes: context.policy.max_bytes,
            max_work: context.policy.max_work,
        },
        ..crate::Policy::default()
    };
    let admitted = AdmittedTarget {
        source_offset: original.source_offset,
        source_length: original.source_length,
        source: Arc::clone(&original.source),
        target: Arc::new(target),
        norm_upper: to_f64(norm, BinaryRounding::Up)?,
        policy,
        _mode: PhantomData,
    };
    Ok(FrozenCandidate {
        synthesis_precision: crate::SynthesisPrecision::Arbitrary {
            bits: context.precision,
        },
        admitted,
        controls: Arc::new(controls),
        a_star: Arc::new(astar),
        phases: Arc::new(phases),
        completion_residual: to_f64(residual, BinaryRounding::Up)?,
        reconstruction_residual: None,
        completion_grid: grid,
    })
}
fn solve<M: CertificationMode>(
    source: &OriginalTarget<M>,
    policy: OfflinePolicy,
) -> OfflineResult<OfflineSolution<M>> {
    let count = source
        .degree
        .checked_add(1)
        .ok_or(OfflineError::Budget("source degree"))?;
    let mut report = OfflineReport {
        source: Arc::clone(&source.source),
        attempts: Vec::new(),
        last_export: None,
        contractivity_upper: None,
    };
    let mut precision = policy.initial_precision;
    let mut total_work = 0_usize;
    loop {
        let started = Instant::now();
        let mut context = Context::new(count, precision, policy)?;
        context.policy.max_work = policy
            .max_work
            .checked_sub(total_work)
            .ok_or(OfflineError::Budget("offline retry work"))?;
        let computed = (|| -> OfflineResult<_> {
            let target = original(source, &mut context)?;
            let norm = contractivity(&target, &mut context)?;
            let (astar, ratio, residual, grid) = kernels::completion(&target, &mut context)?;
            let gamma = match policy.algorithm {
                crate::SynthesisAlgorithm::RhwHalfCholesky => {
                    kernels::half_cholesky(&ratio, &mut context)?
                }
                crate::SynthesisAlgorithm::InverseNlftDivideConquer => {
                    kernels::inverse(&astar, &target, &mut context)?
                }
            };
            let candidate = freeze(
                source,
                &target,
                &astar,
                &gamma,
                &norm,
                &residual,
                grid,
                &mut context,
            )?;
            Ok((candidate, residual, norm, grid))
        })();
        total_work = total_work
            .checked_add(context.work)
            .ok_or(OfflineError::Budget("offline cumulative work"))?;
        let mut attempt = OfflineAttempt {
            precision,
            grid: 0,
            work: context.work,
            computation: started.elapsed(),
            certification: Duration::ZERO,
            completion_residual: None,
        };
        match computed {
            Ok((candidate, residual, norm, grid)) => {
                report.contractivity_upper = Some(norm);
                report.last_export = Some(ExportSnapshot::from_candidate(&candidate));
                attempt.grid = grid;
                attempt.completion_residual = Some(residual);
                let certify_started = Instant::now();
                let certified = CertificationBuilder::new()
                    .candidate(candidate)
                    .policy(policy.certification)
                    .and_then(CertificationBuilder::certify);
                attempt.certification = certify_started.elapsed();
                report.attempts.push(attempt);
                match certified {
                    Ok(certified) => return Ok(OfflineSolution { certified, report }),
                    Err(source) => {
                        // Verification resource/configuration failures are terminal
                        // policy outcomes: producer retries do not raise those
                        // limits. Reserve the explicit precision-retry policy for
                        // numerical bounds/violations, where recomputing can change
                        // the exported candidate and its verification result.
                        if precision == policy.max_precision
                            || matches!(
                                source,
                                CertificationError::Budget(_) | CertificationError::Policy(_)
                            )
                        {
                            return Err(OfflineError::Certification {
                                report: Box::new(report),
                                source: Box::new(source),
                            });
                        }
                    }
                }
            }
            Err(OfflineError::Numerical(stage)) => {
                report.attempts.push(attempt);
                if precision == policy.max_precision {
                    return Err(OfflineError::NotEstablished {
                        stage,
                        report: Box::new(report),
                    });
                }
            }
            Err(error) => return Err(error),
        }
        precision = precision
            .checked_mul(2)
            .unwrap_or(policy.max_precision)
            .min(policy.max_precision);
    }
}
