//! Independent, outward arbitrary-precision verification of an immutable binary64 QSP export.
//!
//! No production synthesis routine is called by this module. Precision retries
//! reconstruct the same frozen controls/phases and never repair their values.
//!
//! Available with the `certification` feature. Pass a [`crate::FrozenCandidate`]
//! through [`CertificationBuilder::candidate`], admit a [`CertificationPolicy`],
//! then call [`CertificationBuilder::certify`]. [`Certified`] owns that same
//! candidate and the independent [`CertificationReport`].
//!
//! `UnitCircleResponse` matrices are interpreted as exact binary64 dyadics. `RealParityWx`
//! controls are reconstructed from the exact binary64 phase payload using
//! directed arbitrary-precision sine and cosine. All four matrix-polynomial
//! entries are reconstructed independently of production FFT/NLFT routines.
//! [`Bound`] retains the arbitrary-precision endpoints used for acceptance;
//! its binary64 summaries round outward. The sufficient upper bound can be
//! loose: [`CertificationError::NotEstablished`] does not imply a violation,
//! while [`CertificationError::Violation`] requires a coefficient lower witness.
//!
//! Default verification starts at 256 bits and doubles up to 1024 bits. All five
//! default tolerances are `1e-11`. Precision and work are bounded, but modeled
//! memory and work do not impose allocator quotas or preempt internal backend
//! transcendental iterations. This module verifies the QSP export's numerical
//! properties; it does not certify native execution or an application's oracle.

pub(crate) mod interval;
mod product;
mod projector;
use crate::precision::{
	Binary, BinaryRounding, checked, exact_from_f64, to_f64, up_add, up_mul, up_sqrt, zero,
};
use crate::{FrozenCandidate, RealParityWx, UnitCircleResponse};
use dashu_float::ConstCache;
pub use interval::{MpComplex, MpInterval};
#[cfg(feature = "offline-synthesis")]
pub(crate) use product::circle_values;
pub use projector::{
	CertifiedProjectorPhases, ProjectorConvention, ProjectorDomain, ProjectorNorm,
};
use std::{
	collections::BTreeMap,
	sync::Arc,
	time::{Duration, Instant},
};

/// Result of independent frozen-export verification.
pub type CertificationResult<T> = std::result::Result<T, CertificationError>;
/// Verification failures with distinct insufficient-bound and violation outcomes.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CertificationError {
	/// Direct reconstruction proved a violation for the actual projector export.
	#[error("actual projector response or unitarity violates its requested tolerance")]
	ProjectorViolation {
		/// Response enclosure for the actual converted payload.
		response: Box<Bound>,
		/// Full product unitarity enclosure.
		unitarity: Box<Bound>,
	},
	/// Available precision did not establish converted-export bounds.
	#[error("actual projector response or unitarity bound was not established")]
	ProjectorNotEstablished {
		/// Last response enclosure.
		response: Box<Bound>,
		/// Last full product unitarity enclosure.
		unitarity: Box<Bound>,
	},
	/// Invalid tolerance, precision or resource configuration.
	#[error("invalid certification policy: {0}")]
	Policy(&'static str),
	/// Modeled coefficient, byte or work resources were exhausted.
	#[error("certification resource budget: {0}")]
	Budget(&'static str),
	/// Directed arithmetic could not produce a valid enclosure.
	#[error("multiprecision verification arithmetic: {0}")]
	Arithmetic(&'static str),
	/// Checked binary64/arbitrary-precision interchange failed.
	#[error(transparent)]
	Precision(#[from] crate::precision::PrecisionError),
	/// Frozen source, support or convention data is inconsistent.
	#[error("inconsistent frozen export: {0}")]
	Export(&'static str),
	/// Sufficient bounds remained above a tolerance at the precision limit.
	#[error("requested bounds were not established within the precision budget")]
	NotEstablished {
		/// Last bounds and completed precision attempts.
		report: Box<CertificationReport>,
	},
	/// A rigorous lower witness exceeded the requested tolerance.
	#[error("a Fourier coefficient proves the {metric} tolerance is violated")]
	Violation {
		/// Name of the response, completion, conversion, reconstruction or unitarity metric.
		metric: &'static str,
		/// Bounds containing the witness and completed precision attempts.
		report: Box<CertificationReport>,
	},
}
/// Independent direct reference or balanced outward interval FFT reconstruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConvolutionMethod {
	/// Reference reconstruction using direct interval convolution.
	Direct,
	/// Balanced product reconstruction using outward interval FFT convolution.
	#[default]
	IntervalFft,
}
/// Precision, accuracy and modeled resources for independent verification.
///
/// Precision is selected in individual bits, with
/// `64 <= initial_precision <= max_precision <= 1_048_576`. All tolerances
/// must be positive and finite. Configuration is checked by
/// [`CertificationBuilder::policy`].
#[derive(Debug, Clone, Copy)]
pub struct CertificationPolicy {
	/// First attempt's working precision in bits (default 256).
	pub initial_precision: u32,
	/// Final permitted precision in bits (default 1024).
	pub max_precision: u32,
	/// Upper-left response error relative to the original source.
	pub response_tolerance: f64,
	/// Defect in the target/complement squared-magnitude identity.
	pub completion_tolerance: f64,
	/// Error of the frozen target relative to the original source conversion.
	pub conversion_tolerance: f64,
	/// Full-matrix reconstruction error relative to the target/complement quartet.
	pub reconstruction_tolerance: f64,
	/// Full-matrix unitary defect over the unit circle.
	pub unitarity_tolerance: f64,
	/// Independent direct or interval-FFT reconstruction algorithm.
	pub method: ConvolutionMethod,
	/// Maximum permitted coefficient count for retained source and target arrays.
	pub max_coefficients: usize,
	/// Conservative modeled coefficient/FFT storage, including allocated limbs.
	/// This is not a hard cap on the backend's adaptive transcendental scratch.
	pub max_bytes: usize,
	/// Precision-weighted algebraic work across attempts. Backend internal
	/// correct-rounding iterations are not individually counted or preempted.
	pub max_work: usize,
}
impl Default for CertificationPolicy {
	fn default() -> Self {
		Self {
			initial_precision: 256,
			max_precision: 1024,
			response_tolerance: 1e-11,
			completion_tolerance: 1e-11,
			conversion_tolerance: 1e-11,
			reconstruction_tolerance: 1e-11,
			unitarity_tolerance: 1e-11,
			method: ConvolutionMethod::IntervalFft,
			max_coefficients: 1_048_576,
			max_bytes: 2_147_483_648,
			max_work: usize::try_from(1_099_511_627_776_u64).unwrap_or(usize::MAX),
		}
	}
}
impl CertificationPolicy {
	pub(crate) fn validate(self) -> CertificationResult<()> {
		if self.initial_precision < 64
			|| self.max_precision < self.initial_precision
			|| self.max_precision > 1_048_576
		{
			return Err(CertificationError::Policy(
				"precision must satisfy 64 <= initial <= maximum <= 1048576",
			));
		}
		for tolerance in [
			self.response_tolerance,
			self.completion_tolerance,
			self.conversion_tolerance,
			self.reconstruction_tolerance,
			self.unitarity_tolerance,
		] {
			if !tolerance.is_finite() || tolerance <= 0.0 {
				return Err(CertificationError::Policy(
					"positive finite tolerances required",
				));
			}
		}
		if self.max_coefficients == 0 || self.max_bytes == 0 || self.max_work == 0 {
			return Err(CertificationError::Policy(
				"nonzero resource budgets required",
			));
		}
		Ok(())
	}
}
mod sealed {
	pub trait Sealed {}
}
/// Export conventions accepted by the independent verifier.
pub trait CertificationMode: sealed::Sealed {
	#[doc(hidden)]
	const CANONICAL: bool;
}
impl sealed::Sealed for UnitCircleResponse {}
impl sealed::Sealed for RealParityWx {}
impl CertificationMode for UnitCircleResponse {
	const CANONICAL: bool = false;
}
impl CertificationMode for RealParityWx {
	const CANONICAL: bool = true;
}
/// Builder state before ownership of a frozen candidate is supplied.
#[derive(Debug)]
pub struct MissingCandidate;
/// Builder state owning a candidate whose verification policy is not yet admitted.
#[derive(Debug)]
pub struct WithCandidate<M> {
	candidate: FrozenCandidate<M>,
}
/// Builder state with admitted source sizes and verification configuration.
#[derive(Debug)]
pub struct ReadyCertification<M> {
	candidate: FrozenCandidate<M>,
	policy: CertificationPolicy,
}
/// Own a frozen candidate, admit a verification policy, then certify.
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use quest_polynomial::{Laurent, Limits, Polynomial};
/// use quest_qsp::{Complex64, SynthesisBuilder};
/// use quest_qsp::certification::{CertificationBuilder, CertificationPolicy};
/// let target = Polynomial::new(Laurent::new(0), vec![Complex64::new(0.3, 0.2)], Limits::default())?;
/// let frozen = SynthesisBuilder::new().unit_circle_response(&target)?.admit()?.complete()?.synthesize()?;
/// let certified = CertificationBuilder::new().candidate(frozen)
///     .policy(CertificationPolicy::default())?.certify()?;
/// assert!(certified.report().response().upper_f64() <= 1e-11);
/// # Ok(())
/// # }
/// ```
///
/// ```compile_fail
/// use quest_qsp::certification::CertificationBuilder;
/// CertificationBuilder::new().certify();
/// ```
#[derive(Debug)]
pub struct CertificationBuilder<S = MissingCandidate> {
	state: S,
}
impl Default for CertificationBuilder {
	fn default() -> Self {
		Self::new()
	}
}
impl CertificationBuilder {
	/// Start verification configuration without a candidate.
	#[must_use]
	pub const fn new() -> Self {
		Self {
			state: MissingCandidate,
		}
	}
	/// Take ownership of the exact frozen export that will be verified.
	#[must_use]
	pub const fn candidate<M: CertificationMode>(
		self,
		candidate: FrozenCandidate<M>,
	) -> CertificationBuilder<WithCandidate<M>> {
		CertificationBuilder {
			state: WithCandidate { candidate },
		}
	}
}
impl<M: CertificationMode> CertificationBuilder<WithCandidate<M>> {
	/// Validate precision, tolerances, support and modeled initial storage.
	/// # Errors
	/// Rejects invalid precision/tolerances or impossible source/IR sizes.
	pub fn policy(
		self,
		policy: CertificationPolicy,
	) -> CertificationResult<CertificationBuilder<ReadyCertification<M>>> {
		policy.validate()?;
		check_export(&self.state.candidate, policy)?;
		Ok(CertificationBuilder {
			state: ReadyCertification {
				candidate: self.state.candidate,
				policy,
			},
		})
	}
}
impl<M: CertificationMode> CertificationBuilder<ReadyCertification<M>> {
	/// Independently verify the frozen export, retrying only verifier precision.
	/// # Errors
	/// Distinguishes insufficient sufficient bounds from proved coefficient
	/// violations; also reports resource, export and arithmetic failures.
	pub fn certify(self) -> CertificationResult<Certified<M>> {
		certify(self.state.candidate, self.state.policy)
	}
}
/// Frozen candidate with independently established, mode-specific numerical bounds.
#[derive(Debug, Clone)]
pub struct Certified<M> {
	candidate: FrozenCandidate<M>,
	report: CertificationReport,
}
impl<M> Certified<M> {
	/// Original candidate, with controls and phases unchanged by verification.
	#[must_use]
	pub const fn candidate(&self) -> &FrozenCandidate<M> {
		&self.candidate
	}
	/// Detailed immutable numerical evidence and attempt accounting.
	#[must_use]
	pub const fn report(&self) -> &CertificationReport {
		&self.report
	}
	/// Recover ownership of the candidate, discarding the attached report.
	#[must_use]
	pub fn into_candidate(self) -> FrozenCandidate<M> {
		self.candidate
	}
}
/// Lower and upper bounds for a unit-circle supremum norm.
///
/// The lower bound is the largest enclosed Fourier-coefficient magnitude; the
/// upper bound is a coefficient l1 sufficient bound. For matrices, the
/// Frobenius bound also bounds the spectral/operator norm.
#[derive(Debug, Clone)]
pub struct Bound {
	lower: Binary,
	upper: Binary,
	lower_f64: f64,
	upper_f64: f64,
}
impl Bound {
	/// Rigorous arbitrary-precision lower witness for the supremum norm.
	#[must_use]
	pub const fn lower(&self) -> &Binary {
		&self.lower
	}
	/// Rigorous arbitrary-precision sufficient upper bound.
	#[must_use]
	pub const fn upper(&self) -> &Binary {
		&self.upper
	}
	/// Lower witness summarized in binary64 with downward rounding.
	#[must_use]
	pub const fn lower_f64(&self) -> f64 {
		self.lower_f64
	}
	/// Sufficient upper bound summarized in binary64 with upward rounding.
	#[must_use]
	pub const fn upper_f64(&self) -> f64 {
		self.upper_f64
	}
	fn new(lower: Binary, upper: Binary) -> CertificationResult<Self> {
		let lower = checked(lower)?;
		let upper = checked(upper)?;
		if lower > upper {
			return Err(CertificationError::Arithmetic("invalid norm bound"));
		}
		let lower_f64 = to_f64(&lower, BinaryRounding::Down)?;
		let upper_f64 = to_f64(&upper, BinaryRounding::Up)?;
		Ok(Self {
			lower,
			upper,
			lower_f64,
			upper_f64,
		})
	}
}
/// Accounting for one completed attempt at a fixed verifier precision.
#[derive(Debug, Clone)]
pub struct CertificationAttempt {
	precision: u32,
	work_units: usize,
	modeled_peak_bytes: usize,
	elapsed: Duration,
}
impl CertificationAttempt {
	/// Actual working precision in bits.
	#[must_use]
	pub const fn precision(&self) -> u32 {
		self.precision
	}
	/// Precision-weighted algebraic work charged during this attempt.
	#[must_use]
	pub const fn work_units(&self) -> usize {
		self.work_units
	}
	/// Conservative modeled peak bytes, not an allocator-enforced process cap.
	#[must_use]
	pub const fn modeled_peak_bytes(&self) -> usize {
		self.modeled_peak_bytes
	}
	/// Wall-clock time for this verification attempt.
	#[must_use]
	pub const fn elapsed(&self) -> Duration {
		self.elapsed
	}
}
/// Complete evidence for the exact original frozen binary64 export.
#[derive(Debug, Clone)]
pub struct CertificationReport {
	response: Bound,
	completion: Bound,
	conversion: Bound,
	reconstruction: Bound,
	unitarity: Bound,
	entries: [Bound; 4],
	coefficients: Vec<[[MpComplex; 2]; 2]>,
	attempts: Vec<CertificationAttempt>,
	policy: CertificationPolicy,
}
impl CertificationReport {
	/// Upper-left response error relative to the original source conversion.
	#[must_use]
	pub const fn response(&self) -> &Bound {
		&self.response
	}
	/// Unit-circle defect of `|target|^2 + |complement|^2 = 1`.
	#[must_use]
	pub const fn completion(&self) -> &Bound {
		&self.completion
	}
	/// Frozen target error relative to the independently converted original source.
	#[must_use]
	pub const fn conversion(&self) -> &Bound {
		&self.conversion
	}
	/// Full-matrix error relative to the target/complement quartet.
	#[must_use]
	pub const fn reconstruction(&self) -> &Bound {
		&self.reconstruction
	}
	/// Full-matrix defect `U(z)^* U(z) - I` over the unit circle.
	#[must_use]
	pub const fn unitarity(&self) -> &Bound {
		&self.unitarity
	}
	/// Per-entry reconstruction bounds in row-major matrix order.
	#[must_use]
	pub const fn entries(&self) -> &[Bound; 4] {
		&self.entries
	}
	/// All four actual exported-product coefficient intervals, exponents 0..d.
	#[must_use]
	pub fn coefficients(&self) -> &[[[MpComplex; 2]; 2]] {
		&self.coefficients
	}
	/// Completed verification attempts, in increasing precision order.
	#[must_use]
	pub fn attempts(&self) -> &[CertificationAttempt] {
		&self.attempts
	}
	/// Configuration against which this report was accepted or rejected.
	#[must_use]
	pub const fn policy(&self) -> CertificationPolicy {
		self.policy
	}
}
struct Context {
	precision: u32,
	policy: CertificationPolicy,
	work: usize,
	bytes: usize,
	cache: ConstCache,
	roots: BTreeMap<usize, Arc<Vec<MpComplex>>>,
}
impl Context {
	fn new(count: usize, precision: u32, policy: CertificationPolicy) -> CertificationResult<Self> {
		let support = count
			.checked_mul(2)
			.and_then(|x| x.checked_sub(1))
			.ok_or(CertificationError::Budget("support overflow"))?;
		let fft = support
			.checked_next_power_of_two()
			.ok_or(CertificationError::Budget("FFT support"))?;
		let levels = usize::try_from(fft.ilog2())
			.map_err(|_| CertificationError::Budget("tree levels"))?
			.saturating_add(1);
		let float_bytes = usize::try_from(
			precision
				.checked_add(1)
				.ok_or(CertificationError::Budget("guard-bit storage"))?,
		)
		.map_err(|_| CertificationError::Budget("precision"))?
		.div_ceil(64)
		.checked_mul(8)
		.and_then(|bytes| bytes.checked_add(64))
		.ok_or(CertificationError::Budget("precision storage"))?;
		// Conservative logical live coefficients, tree temporaries and FFT
		// rectangles; includes limb storage, not a process allocator cap.
		let slots = count
			.checked_mul(levels)
			.and_then(|n| n.checked_mul(64))
			.and_then(|n| fft.checked_mul(64).and_then(|v| n.checked_add(v)))
			.ok_or(CertificationError::Budget("verification storage"))?;
		let bytes = slots
			.checked_mul(float_bytes)
			.ok_or(CertificationError::Budget("verification storage"))?;
		if bytes > policy.max_bytes {
			return Err(CertificationError::Budget("modeled verification memory"));
		}
		Ok(Self {
			precision,
			policy,
			work: 0,
			bytes,
			cache: ConstCache::default(),
			roots: BTreeMap::new(),
		})
	}
	// Called once at the end of each owned-cache attempt; include actual retained words.
	fn admit_cache(&mut self) -> CertificationResult<()> {
		let cache_bytes = self
			.cache
			.total_words()
			.checked_mul(size_of::<dashu_int::Word>())
			.and_then(|n| n.checked_add(size_of::<ConstCache>()))
			.ok_or(CertificationError::Budget("constant cache storage"))?;
		self.bytes = self
			.bytes
			.checked_add(cache_bytes)
			.ok_or(CertificationError::Budget("constant cache storage"))?;
		if self.bytes > self.policy.max_bytes {
			return Err(CertificationError::Budget("constant cache storage"));
		}
		Ok(())
	}
	fn charge(&mut self, units: usize) -> CertificationResult<()> {
		let limbs = usize::try_from(self.precision)
			.map_err(|_| CertificationError::Budget("precision work"))?
			.div_ceil(64);
		let weighted = units
			.checked_mul(limbs)
			.ok_or(CertificationError::Budget("work overflow"))?;
		self.work = self
			.work
			.checked_add(weighted)
			.ok_or(CertificationError::Budget("work overflow"))?;
		if self.work > self.policy.max_work {
			return Err(CertificationError::Budget("verification work"));
		}
		Ok(())
	}
}
fn source_bytes<M>(candidate: &FrozenCandidate<M>) -> CertificationResult<usize> {
	let count = candidate
		.target()
		.len()
		.checked_add(candidate.conjugate_complement().len())
		.and_then(|n| n.checked_add(candidate.admitted.source.len()))
		.ok_or(CertificationError::Budget("source storage"))?;
	count
		.checked_mul(size_of::<crate::Complex64>())
		.and_then(|n| {
			candidate
				.controls()
				.len()
				.checked_mul(size_of::<crate::Control>())
				.and_then(|v| n.checked_add(v))
		})
		.and_then(|n| {
			candidate
				.phases
				.len()
				.checked_mul(size_of::<f64>())
				.and_then(|v| n.checked_add(v))
		})
		.ok_or(CertificationError::Budget("source storage"))
}
fn check_export<M: CertificationMode>(
	candidate: &FrozenCandidate<M>,
	policy: CertificationPolicy,
) -> CertificationResult<()> {
	let count = candidate.target().len();
	if count == 0
		|| count > policy.max_coefficients
		|| candidate.admitted.source.len() > policy.max_coefficients
	{
		return Err(CertificationError::Budget("source support"));
	}
	if candidate.conjugate_complement().len() != count || candidate.controls().len() != count {
		return Err(CertificationError::Export("inconsistent degree/support"));
	}
	if M::CANONICAL && candidate.phases.len() != count {
		return Err(CertificationError::Export("real_parity_wx phase count"));
	}
	if M::CANONICAL
		&& !candidate
			.phases
			.iter()
			.zip(candidate.phases.iter().rev())
			.all(|(a, b)| a.to_bits() == b.to_bits())
	{
		return Err(CertificationError::Export(
			"real_parity_wx phases must be exactly symmetric",
		));
	}
	for coefficient in candidate
		.target()
		.iter()
		.chain(candidate.conjugate_complement())
		.chain(candidate.admitted.source.iter())
	{
		if !coefficient.re.is_finite() || !coefficient.im.is_finite() {
			return Err(CertificationError::Export("nonfinite source"));
		}
	}
	let context = Context::new(count, policy.initial_precision, policy)?;
	if context
		.bytes
		.checked_add(source_bytes(candidate)?)
		.ok_or(CertificationError::Budget("source/IR memory"))?
		> policy.max_bytes
	{
		return Err(CertificationError::Budget("source/IR memory"));
	}
	Ok(())
}
fn certify<M: CertificationMode>(
	candidate: FrozenCandidate<M>,
	policy: CertificationPolicy,
) -> CertificationResult<Certified<M>> {
	let mut precision = policy.initial_precision;
	let mut attempts = Vec::new();
	let mut total_work = 0_usize;
	loop {
		let started = Instant::now();
		let mut context = Context::new(candidate.target().len(), precision, policy)?;
		context.bytes = context
			.bytes
			.checked_add(source_bytes(&candidate)?)
			.ok_or(CertificationError::Budget("source/IR memory"))?;
		if context.bytes > policy.max_bytes {
			return Err(CertificationError::Budget("source/IR memory"));
		}
		context.policy.max_work = policy
			.max_work
			.checked_sub(total_work)
			.ok_or(CertificationError::Budget("retry work"))?;
		let mut report = verify(&candidate, &mut context)?;
		context.admit_cache()?;
		total_work = total_work
			.checked_add(context.work)
			.ok_or(CertificationError::Budget("retry work"))?;
		attempts.push(CertificationAttempt {
			precision,
			work_units: context.work,
			modeled_peak_bytes: context.bytes,
			elapsed: started.elapsed(),
		});
		report.attempts = attempts;
		report.policy = policy;
		let axes = [
			("response", &report.response, policy.response_tolerance),
			(
				"completion",
				&report.completion,
				policy.completion_tolerance,
			),
			(
				"conversion",
				&report.conversion,
				policy.conversion_tolerance,
			),
			(
				"reconstruction",
				&report.reconstruction,
				policy.reconstruction_tolerance,
			),
			("unitarity", &report.unitarity, policy.unitarity_tolerance),
		];
		let mut established = true;
		let mut violation = None;
		for (metric, bound, tolerance) in axes {
			let exact_tolerance = exact_from_f64(tolerance, precision)?;
			if bound.lower > exact_tolerance {
				violation = Some(metric);
				break;
			}
			if bound.upper > exact_tolerance {
				established = false;
			}
		}
		if let Some(metric) = violation {
			return Err(CertificationError::Violation {
				metric,
				report: Box::new(report),
			});
		}
		if established {
			return Ok(Certified { candidate, report });
		}
		if precision == policy.max_precision {
			return Err(CertificationError::NotEstablished {
				report: Box::new(report),
			});
		}
		attempts = std::mem::take(&mut report.attempts);
		precision = precision
			.checked_mul(2)
			.unwrap_or(policy.max_precision)
			.min(policy.max_precision);
	}
}
fn coefficients(
	values: &[crate::Complex64],
	precision: u32,
) -> CertificationResult<Vec<MpComplex>> {
	values
		.iter()
		.map(|value| MpComplex::exact(*value, precision))
		.collect()
}
fn expected_source<M: CertificationMode>(
	candidate: &FrozenCandidate<M>,
	precision: u32,
) -> CertificationResult<Vec<MpComplex>> {
	if !M::CANONICAL {
		return coefficients(&candidate.admitted.source, precision);
	}
	let count = candidate.target().len();
	let degree = count.saturating_sub(1);
	let mut result = vec![MpComplex::zero(precision); count];
	for (index, coefficient) in candidate.admitted.source.iter().enumerate() {
		if coefficient.im != 0.0
			|| ((index % 2 != degree % 2 || index > degree) && coefficient.re != 0.0)
		{
			return Err(CertificationError::Export(
				"real_parity_wx source parity/domain",
			));
		}
		if index > degree || index % 2 != degree % 2 {
			continue;
		}
		let half = MpComplex::exact(*coefficient, precision)?.divide_usize(2)?;
		let high = degree
			.checked_add(index)
			.ok_or(CertificationError::Budget("conversion support"))?
			/ 2;
		let low = degree
			.checked_sub(index)
			.ok_or(CertificationError::Budget("conversion support"))?
			/ 2;
		for index in [high, low] {
			let out = result
				.get_mut(index)
				.ok_or(CertificationError::Export("conversion support"))?;
			*out = out.add(&half)?;
		}
	}
	Ok(result)
}
fn difference(
	left: &[MpComplex],
	right: &[MpComplex],
	precision: u32,
) -> CertificationResult<Vec<MpComplex>> {
	let zero = MpComplex::zero(precision);
	let count = left.len().max(right.len());
	(0..count)
		.map(|i| {
			left.get(i)
				.unwrap_or(&zero)
				.sub(right.get(i).unwrap_or(&zero))
		})
		.collect()
}
fn bound(values: &[MpComplex], precision: u32) -> CertificationResult<Bound> {
	let p = precision;
	let mut lower = checked(zero(p))?;
	let mut upper = lower.clone();
	for value in values {
		let magnitude = value.magnitude()?;
		if magnitude.lower() > &lower {
			lower.clone_from(magnitude.lower());
		}
		upper = checked(up_add(p, &upper, magnitude.upper())?)?;
	}
	Bound::new(lower, upper)
}
fn matrix_bound(entries: &[Bound; 4], precision: u32) -> CertificationResult<Bound> {
	let p = precision;
	let mut lower = checked(zero(p))?;
	let mut squared = lower.clone();
	for entry in entries {
		if entry.lower > lower {
			lower.clone_from(&entry.lower);
		}
		let term = checked(up_mul(p, &entry.upper, &entry.upper)?)?;
		squared = checked(up_add(p, &squared, &term)?)?;
	}
	Bound::new(lower, up_sqrt(p, &squared)?)
}

fn verify<M: CertificationMode>(
	candidate: &FrozenCandidate<M>,
	context: &mut Context,
) -> CertificationResult<CertificationReport> {
	let precision = context.precision;
	context.charge(source_bytes(candidate)?)?;
	let target = coefficients(candidate.target(), precision)?;
	let astar = coefficients(candidate.conjugate_complement(), precision)?;
	let source = expected_source(candidate, precision)?;
	let conversion = bound(&difference(&target, &source, precision)?, precision)?;
	let controls = product::controls(candidate, context)?;
	let actual = product::reconstruct(&controls, context)?;
	let expected = [
		target.clone(),
		astar.iter().rev().map(|value| value.conj().neg()).collect(),
		astar.clone(),
		target.iter().rev().map(MpComplex::conj).collect(),
	];
	let [a, b, c, d] = &actual;
	let [ea, eb, ec, ed] = &expected;
	let entries = [
		bound(&difference(a, ea, precision)?, precision)?,
		bound(&difference(b, eb, precision)?, precision)?,
		bound(&difference(c, ec, precision)?, precision)?,
		bound(&difference(d, ed, precision)?, precision)?,
	];
	let reconstruction = matrix_bound(&entries, precision)?;
	let response = bound(&difference(a, &source, precision)?, precision)?;
	let completion = bound(
		&product::gram_entry(&target, &astar, &target, &astar, true, context)?,
		precision,
	)?;
	let unitary_entries = [
		bound(&product::gram_entry(a, c, a, c, true, context)?, precision)?,
		bound(&product::gram_entry(a, c, b, d, false, context)?, precision)?,
		bound(&product::gram_entry(b, d, a, c, false, context)?, precision)?,
		bound(&product::gram_entry(b, d, b, d, true, context)?, precision)?,
	];
	let unitarity = matrix_bound(&unitary_entries, precision)?;
	let [a, b, c, d] = actual;
	let coefficients = a
		.into_iter()
		.zip(b)
		.zip(c)
		.zip(d)
		.map(|((first, c), d)| [first.into(), [c, d]])
		.collect();
	Ok(CertificationReport {
		response,
		completion,
		conversion,
		reconstruction,
		unitarity,
		entries,
		coefficients,
		attempts: Vec::new(),
		policy: context.policy,
	})
}
#[cfg(test)]
mod tests;
