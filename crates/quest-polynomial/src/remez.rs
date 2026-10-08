//! One owning exchange engine. Candidate arithmetic and the meaning of its
//! certificates are independent choices. Every numerical failure returns the
//! original target, exact request and accumulated evidence.
mod exchange;
pub mod linear;
mod proof;
use crate::{
	AdmittedFunction, Chebyshev, Complex64, DynamicShape, Error, Limits, Polynomial, Result, Shape,
	StaticShape, Structural,
};
pub use linear::{LinearSolver, MpHouseholder, PivotedQr};
use quest_numerics::arithmetic::{
	ArithmeticError, CertifyingBackend, EnclosureBackend, ExactConstant, F64Backend,
	Interval64Backend, PointBackend,
};
use quest_numerics::arithmetic::{Budget, BudgetedBackend};
use quest_numerics::roots::RootCover;
use std::{fmt, marker::PhantomData, sync::Arc};

/// Source values are retained, without binary64 pre-rounding.
#[derive(Clone, Debug)]
pub struct ExactDomain {
	pub lower: ExactConstant,
	pub upper: ExactConstant,
}
impl ExactDomain {
	#[must_use]
	pub const fn new(lower: ExactConstant, upper: ExactConstant) -> Self {
		Self { lower, upper }
	}
	#[must_use]
	pub const fn binary64(lower: f64, upper: f64) -> Self {
		Self::new(
			ExactConstant::Binary64(lower),
			ExactConstant::Binary64(upper),
		)
	}
}
#[derive(Clone, Debug)]
pub enum Accuracy {
	UniformError(ExactConstant),
	MinimaxGap(ExactConstant),
	Both {
		uniform: ExactConstant,
		gap: ExactConstant,
	},
}
#[derive(Clone, Debug)]
pub struct RemezOptions {
	pub accuracy: Accuracy,
	pub root_width: ExactConstant,
	pub max_iterations: usize,
	pub max_subdivisions: usize,
	pub limits: Limits,
	/// Round candidate coefficients to binary64 before any certificate is made.
	pub export_binary64: bool,
}
impl Default for RemezOptions {
	fn default() -> Self {
		Self {
			accuracy: Accuracy::MinimaxGap(ExactConstant::Binary64(1e-8)),
			root_width: ExactConstant::Binary64(1e-10),
			max_iterations: 30,
			max_subdivisions: 32768,
			limits: Limits::default(),
			export_binary64: false,
		}
	}
}
#[derive(Debug)]
pub struct AuditedEnclosures;
#[derive(Debug)]
pub struct AssumedEnclosures;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnclosureAssumption {
	AllOperationsEncloseTheirMathematicalResult,
}
/// Certificate type parameters retain both function and arithmetic admission.
/// Its constructor is private and only the enclosure proof may create it.
///
/// Conditional functions cannot obtain unconditional evidence:
/// ```compile_fail
/// use quest_polynomial::{AssumedFunction, ConsistencyAssumption, ExactDomain, RemezRequest, function};
/// let f = AssumedFunction::new(function!(|x| x), ConsistencyAssumption::SameFunctionAndValidEnclosures);
/// let result = RemezRequest::binary64(f, ExactDomain::binary64(-1.0, 1.0), 1).run().unwrap();
/// result.uniform_error().unconditional_bound();
/// ```
#[derive(Debug)]
pub struct UniformErrorCertificate<T, F, A> {
	bound: T,
	_evidence: PhantomData<(F, A)>,
}
impl<T, F, A> UniformErrorCertificate<T, F, A> {
	#[must_use]
	pub const fn bound(&self) -> &T {
		&self.bound
	}
}
impl<T> UniformErrorCertificate<T, Structural, AuditedEnclosures> {
	/// Available only for sealed expressions and audited enclosing arithmetic.
	#[must_use]
	pub const fn unconditional_bound(&self) -> &T {
		&self.bound
	}
}
#[derive(Debug)]
pub struct MinimaxGapCertificate<T, F, A> {
	lower: T,
	gap: T,
	_evidence: PhantomData<(F, A)>,
}
impl<T, F, A> MinimaxGapCertificate<T, F, A> {
	#[must_use]
	pub const fn lower_bound(&self) -> &T {
		&self.lower
	}
	#[must_use]
	pub const fn gap(&self) -> &T {
		&self.gap
	}
}
/// A request is constructed with all required inputs. Static coefficient counts
/// are part of this same type; no separate forwarding builder is involved.
///
/// Arithmetic admission is required independently of structural functions:
/// ```compile_fail
/// use quest_numerics::arithmetic::{EnclosureBackend, F64Backend};
/// use quest_polynomial::{DynamicShape, ExactDomain, PivotedQr, RemezRequest, function};
/// fn unreviewed<I: EnclosureBackend<Endpoint=f64, Error=quest_numerics::arithmetic::ArithmeticError>>(intervals: I) {
///     RemezRequest::new(function!(|x| x), ExactDomain::binary64(-1.0,1.0), DynamicShape(2), F64Backend, intervals, PivotedQr);
/// }
/// ```
///
/// Degree relationships are checked, including overflow:
/// ```compile_fail
/// #![feature(generic_const_exprs)]
/// #![allow(incomplete_features)]
/// use quest_polynomial::{ExactDomain, RemezRequest, function};
/// let request = RemezRequest::binary64(function!(|x| x), ExactDomain::binary64(-1.0,1.0), 1).degree::<{usize::MAX}>();
/// ```
pub struct RemezRequest<F, P, I, D = DynamicShape, L = PivotedQr, A = AuditedEnclosures> {
	target: F,
	domain: ExactDomain,
	shape: D,
	point: P,
	enclosure: I,
	solver: L,
	options: RemezOptions,
	enclosure_assumption: Option<EnclosureAssumption>,
	requested_degree: Option<usize>,
	precision_policy: Option<PrecisionAttempts>,
	_admission: PhantomData<A>,
}
impl<F, P, I, D, L> RemezRequest<F, P, I, D, L, AuditedEnclosures>
where
	F: AdmittedFunction,
	P: PointBackend<Error = ArithmeticError>,
	I: CertifyingBackend<Endpoint = P::Scalar, Error = ArithmeticError>,
	D: Shape,
{
	#[must_use]
	pub fn new(
		target: F,
		domain: ExactDomain,
		shape: D,
		point: P,
		enclosure: I,
		solver: L,
	) -> Self {
		Self {
			target,
			domain,
			shape,
			point,
			enclosure,
			solver,
			options: RemezOptions::default(),
			enclosure_assumption: None,
			requested_degree: None,
			precision_policy: None,
			_admission: PhantomData,
		}
	}
}
impl<F: AdmittedFunction> RemezRequest<F, F64Backend, Interval64Backend> {
	#[must_use]
	pub fn binary64(target: F, domain: ExactDomain, degree: usize) -> Self {
		// Overflow is represented as an invalid empty shape and rejected by run,
		// where the complete original request is retained.
		let mut request = Self::new(
			target,
			domain,
			DynamicShape(degree.checked_add(1).unwrap_or(0)),
			F64Backend,
			Interval64Backend,
			PivotedQr,
		);
		request.requested_degree = Some(degree);
		request
	}
}
impl<F, P, I, D, L> RemezRequest<F, P, I, D, L, AssumedEnclosures>
where
	F: AdmittedFunction,
	P: PointBackend<Error = ArithmeticError>,
	I: EnclosureBackend<Endpoint = P::Scalar, Error = ArithmeticError>,
	D: Shape,
{
	#[must_use]
	pub fn assuming_enclosures(
		target: F,
		domain: ExactDomain,
		shape: D,
		point: P,
		enclosure: I,
		solver: L,
		assumption: EnclosureAssumption,
	) -> Self {
		Self {
			target,
			domain,
			shape,
			point,
			enclosure,
			solver,
			options: RemezOptions::default(),
			enclosure_assumption: Some(assumption),
			requested_degree: None,
			precision_policy: None,
			_admission: PhantomData,
		}
	}
}
impl<F, P, I, D, L, A> RemezRequest<F, P, I, D, L, A> {
	#[must_use]
	pub fn options(mut self, options: RemezOptions) -> Self {
		self.options = options;
		self
	}
	#[must_use]
	pub fn accuracy(mut self, accuracy: Accuracy) -> Self {
		self.options.accuracy = accuracy;
		self
	}
	#[must_use]
	pub const fn export_binary64(mut self) -> Self {
		self.options.export_binary64 = true;
		self
	}
	#[must_use]
	pub const fn target(&self) -> &F {
		&self.target
	}
	#[must_use]
	pub const fn requested_degree(&self) -> Option<usize> {
		self.requested_degree
	}
	#[must_use]
	pub const fn precision_policy(&self) -> Option<&PrecisionAttempts> {
		self.precision_policy.as_ref()
	}
	#[must_use]
	pub const fn domain(&self) -> &ExactDomain {
		&self.domain
	}
	#[must_use]
	pub const fn configuration(&self) -> &RemezOptions {
		&self.options
	}
	#[must_use]
	pub const fn shape(&self) -> &D {
		&self.shape
	}
	#[must_use]
	pub const fn enclosure_assumption(&self) -> Option<EnclosureAssumption> {
		self.enclosure_assumption
	}
	/// Compile-time degree plus one is checked by the pinned nightly compiler.
	#[must_use]
	pub fn degree<const N: usize>(self) -> RemezRequest<F, P, I, StaticShape<{ N + 1 }>, L, A>
	where
		[(); N + 1]:,
		[(); N + 2]:,
	{
		RemezRequest {
			target: self.target,
			domain: self.domain,
			shape: StaticShape,
			point: self.point,
			enclosure: self.enclosure,
			solver: self.solver,
			options: self.options,
			enclosure_assumption: self.enclosure_assumption,
			requested_degree: Some(N),
			precision_policy: self.precision_policy,
			_admission: PhantomData,
		}
	}
}
#[derive(Debug)]
pub struct Attempt<C, T, D: Shape> {
	pub precision_bits: usize,
	pub iterations: usize,
	pub work: usize,
	pub candidate: Option<Polynomial<Chebyshev, C, D>>,
	pub coverage: Option<RootCover<T>>,
	pub binary64_export: Option<Arc<[f64]>>,
}
pub struct RemezResult<F: AdmittedFunction, P: PointBackend, I: EnclosureBackend, D: Shape, L, A> {
	request: RemezRequest<F, P, I, D, L, A>,
	polynomial: Polynomial<Chebyshev, P::Scalar, D>,
	binary64_export: Option<Arc<[f64]>>,
	uniform: UniformErrorCertificate<I::Scalar, F::Evidence, A>,
	minimax: MinimaxGapCertificate<I::Scalar, F::Evidence, A>,
	attempts: Vec<Attempt<P::Scalar, I::Scalar, D>>,
}
impl<F: AdmittedFunction, P: PointBackend, I: EnclosureBackend, D: Shape, L, A>
	RemezResult<F, P, I, D, L, A>
{
	#[must_use]
	pub const fn target(&self) -> &F {
		&self.request.target
	}
	#[must_use]
	pub const fn request(&self) -> &RemezRequest<F, P, I, D, L, A> {
		&self.request
	}
	#[must_use]
	pub const fn polynomial(&self) -> &Polynomial<Chebyshev, P::Scalar, D> {
		&self.polynomial
	}
	#[must_use]
	pub const fn uniform_error(&self) -> &UniformErrorCertificate<I::Scalar, F::Evidence, A> {
		&self.uniform
	}
	#[must_use]
	pub const fn minimax_gap(&self) -> &MinimaxGapCertificate<I::Scalar, F::Evidence, A> {
		&self.minimax
	}
	#[must_use]
	pub fn attempts(&self) -> &[Attempt<P::Scalar, I::Scalar, D>] {
		&self.attempts
	}
	#[must_use]
	pub fn into_polynomial(self) -> Polynomial<Chebyshev, P::Scalar, D> {
		self.polynomial
	}
	/// Explicit binary64 interchange for QSP and other numerical consumers.
	/// # Errors
	/// Requires that this exact export was selected before certification.
	pub fn binary64_polynomial(&self) -> Result<Polynomial<Chebyshev>> {
		let payload = self
			.binary64_export
			.as_ref()
			.ok_or(Error::NotEstablished("binary64 export was not certified"))?;
		let values = payload
			.iter()
			.map(|value| Complex64::new(*value, 0.0))
			.collect();
		Polynomial::new(Chebyshev, values, self.request.options.limits)
	}
}
pub struct RemezFailure<F, P: PointBackend, I: EnclosureBackend, D: Shape, L, A> {
	request: Box<RemezRequest<F, P, I, D, L, A>>,
	error: Error,
	attempts: Vec<Attempt<P::Scalar, I::Scalar, D>>,
}
/// A solver outcome owns the exact request on either branch. This alias names
/// that shared ownership contract rather than introducing an adapter.
pub type RemezOutcome<F, P, I, D, L, A> =
	std::result::Result<RemezResult<F, P, I, D, L, A>, RemezFailure<F, P, I, D, L, A>>;
impl<F, P: PointBackend, I: EnclosureBackend, D: Shape, L, A> RemezFailure<F, P, I, D, L, A> {
	#[must_use]
	pub const fn request(&self) -> &RemezRequest<F, P, I, D, L, A> {
		&self.request
	}
	#[must_use]
	pub const fn error(&self) -> &Error {
		&self.error
	}
	#[must_use]
	pub fn attempts(&self) -> &[Attempt<P::Scalar, I::Scalar, D>] {
		&self.attempts
	}
	#[must_use]
	pub fn into_request(self) -> RemezRequest<F, P, I, D, L, A> {
		*self.request
	}
}
impl<F, P: PointBackend, I: EnclosureBackend, D: Shape, L, A> fmt::Debug
	for RemezFailure<F, P, I, D, L, A>
{
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("RemezFailure")
			.field("error", &self.error)
			.field("attempts", &self.attempts.len())
			.finish_non_exhaustive()
	}
}
impl<F, P: PointBackend, I: EnclosureBackend, D: Shape, L, A> fmt::Display
	for RemezFailure<F, P, I, D, L, A>
{
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.error.fmt(f)
	}
}
impl<F, P: PointBackend, I: EnclosureBackend, D: Shape, L, A> std::error::Error
	for RemezFailure<F, P, I, D, L, A>
{
}
impl<F, P, I, D, L, A> RemezRequest<F, P, I, D, L, A>
where
	F: AdmittedFunction,
	P: PointBackend<Error = ArithmeticError>,
	I: EnclosureBackend<Endpoint = P::Scalar, Error = ArithmeticError>,
	D: Shape,
	L: for<'b> LinearSolver<BudgetedBackend<'b, P>>,
{
	/// # Errors
	/// Returns the owning request, last candidate, root coverage and accounting.
	pub fn run(mut self) -> RemezOutcome<F, P, I, D, L, A> {
		let work = Budget::new(self.options.limits.resources.max_work_units);
		let mut attempt = Attempt {
			precision_bits: self.point.precision_bits(),
			iterations: 0,
			work: 0,
			candidate: None,
			coverage: None,
			binary64_export: None,
		};
		let result = {
			let mut point = BudgetedBackend::new(&mut self.point, &work);
			let mut interval = BudgetedBackend::new(&mut self.enclosure, &work);
			exchange::run(
				&self.target,
				&self.domain,
				&self.shape,
				&mut point,
				&mut interval,
				&self.solver,
				&self.options,
				&mut attempt,
			)
		};
		attempt.work = work.used();
		match result {
			Ok(exchange::Established {
				polynomial,
				uniform: upper,
				lower,
				gap,
			}) => Ok(RemezResult {
				request: self,
				polynomial,
				binary64_export: attempt.binary64_export.clone(),
				uniform: UniformErrorCertificate {
					bound: upper,
					_evidence: PhantomData,
				},
				minimax: MinimaxGapCertificate {
					lower,
					gap,
					_evidence: PhantomData,
				},
				attempts: vec![attempt],
			}),
			Err(error) => Err(RemezFailure {
				request: Box::new(self),
				error,
				attempts: vec![attempt],
			}),
		}
	}
}

/// Explicit, finite candidate/proof precision attempts. There is no automatic
/// algorithm change or tolerance relaxation when an attempt fails.
#[derive(Clone, Copy, Debug)]
pub struct PrecisionPair {
	pub candidate_bits: usize,
	pub proof_bits: usize,
}
#[derive(Clone, Debug)]
pub struct PrecisionAttempts {
	pub attempts: Vec<PrecisionPair>,
	pub max_total_work: usize,
}
impl<F, D, A>
	RemezRequest<
		F,
		quest_numerics::arithmetic::MpBackend,
		quest_numerics::arithmetic::MpIntervalBackend,
		D,
		MpHouseholder,
		A,
	>
where
	F: AdmittedFunction,
	D: Shape,
{
	/// # Errors
	/// Retains the same owned target, exact inputs and all attempted candidates.
	/// A schedule is limited to 32 entries, and total work is bounded separately
	/// from the per-attempt request limit.
	pub fn run_with_precisions(
		mut self,
		policy: PrecisionAttempts,
	) -> RemezOutcome<
		F,
		quest_numerics::arithmetic::MpBackend,
		quest_numerics::arithmetic::MpIntervalBackend,
		D,
		MpHouseholder,
		A,
	> {
		use quest_numerics::arithmetic::{MpBackend, MpIntervalBackend};
		let mut history = Vec::new();
		if policy.attempts.is_empty() || policy.attempts.len() > 32 {
			self.precision_policy = Some(policy);
			return Err(RemezFailure {
				request: Box::new(self),
				error: Error::Domain,
				attempts: history,
			});
		}
		self.precision_policy = Some(policy.clone());
		let original_limit = self.options.limits.resources.max_work_units;
		let original_bytes = self.options.limits.resources.max_peak_bytes;
		if let Err(error) = reserve_history(&mut history, policy.attempts.len(), original_bytes) {
			return Err(RemezFailure {
				request: Box::new(self),
				error,
				attempts: history,
			});
		}
		let mut used = 0usize;
		let mut last_error = Error::NotEstablished("precision schedule exhausted");
		for pair in policy.attempts {
			let mut candidate = self.point.precision();
			candidate.bits = pair.candidate_bits;
			let mut proof = self.enclosure.precision();
			proof.bits = pair.proof_bits;
			let backends = MpBackend::new(candidate).and_then(|point| {
				MpIntervalBackend::new(proof).map(|enclosure| (point, enclosure))
			});
			let (point, enclosure) = match backends {
				Ok(values) => values,
				Err(error) => {
					return Err(RemezFailure {
						request: Box::new(self),
						error: error.into(),
						attempts: history,
					});
				}
			};
			let retained = match retained_bytes(&history, &point, &enclosure) {
				Ok(bytes) => bytes,
				Err(error) => {
					return Err(RemezFailure {
						request: Box::new(self),
						error,
						attempts: history,
					});
				}
			};
			let Some(remaining_bytes) = original_bytes.checked_sub(retained) else {
				return Err(RemezFailure {
					request: Box::new(self),
					error: Error::Budget("precision history storage"),
					attempts: history,
				});
			};
			let remaining = policy.max_total_work.saturating_sub(used);
			if remaining == 0 {
				return Err(RemezFailure {
					request: Box::new(self),
					error: Error::Budget("total precision-attempt work"),
					attempts: history,
				});
			}
			self.point = point;
			self.enclosure = enclosure;
			self.options.limits.resources.max_work_units = original_limit.min(remaining);
			self.options.limits.resources.max_peak_bytes = remaining_bytes;
			match self.run() {
				Ok(mut report) => {
					history.append(&mut report.attempts);
					report.attempts = history;
					report.request.options.limits.resources.max_work_units = original_limit;
					report.request.options.limits.resources.max_peak_bytes = original_bytes;
					return Ok(report);
				}
				Err(mut failure) => {
					for attempt in &failure.attempts {
						used = used.saturating_add(attempt.work);
					}
					history.append(&mut failure.attempts);
					last_error = failure.error;
					self = *failure.request;
					self.options.limits.resources.max_work_units = original_limit;
					self.options.limits.resources.max_peak_bytes = original_bytes;
				}
			}
		}
		self.options.limits.resources.max_work_units = original_limit;
		self.options.limits.resources.max_peak_bytes = original_bytes;
		Err(RemezFailure {
			request: Box::new(self),
			error: last_error,
			attempts: history,
		})
	}
}

fn reserve_history<T>(history: &mut Vec<T>, count: usize, max_bytes: usize) -> Result<()> {
	let bytes = count
		.checked_mul(size_of::<T>())
		.ok_or(Error::Budget("precision history storage"))?;
	if bytes > max_bytes {
		return Err(Error::Budget("precision history storage"));
	}
	history
		.try_reserve_exact(count)
		.map_err(|_| Error::Budget("precision history storage"))
}

fn retained_bytes<P, I, D>(
	attempts: &Vec<Attempt<P::Scalar, I::Scalar, D>>,
	point: &P,
	enclosure: &I,
) -> Result<usize>
where
	P: PointBackend<Error = ArithmeticError>,
	I: EnclosureBackend<Error = ArithmeticError>,
	D: Shape,
{
	fn add(total: &mut usize, bytes: usize) -> Result<()> {
		*total = total
			.checked_add(bytes)
			.ok_or(Error::Budget("retained Remez evidence"))?;
		Ok(())
	}
	let mut total = attempts
		.capacity()
		.checked_mul(size_of::<Attempt<P::Scalar, I::Scalar, D>>())
		.ok_or(Error::Budget("retained Remez evidence"))?;
	for attempt in attempts {
		if let Some(candidate) = &attempt.candidate {
			add(&mut total, candidate.retained_heap_bytes(point)?)?;
		}
		if let Some(export) = &attempt.binary64_export {
			add(&mut total, 2_usize.saturating_mul(size_of::<usize>()))?;
			add(
				&mut total,
				export
					.len()
					.checked_mul(size_of::<f64>())
					.ok_or(Error::Budget("export evidence"))?,
			)?;
		}
		if let Some(cover) = &attempt.coverage {
			add(
				&mut total,
				cover
					.covered
					.capacity()
					.checked_mul(size_of::<quest_numerics::roots::RootBox<I::Scalar>>())
					.ok_or(Error::Budget("cover evidence"))?,
			)?;
			let capacity = cover
				.excluded
				.capacity()
				.checked_add(cover.unresolved.capacity())
				.ok_or(Error::Budget("cover evidence"))?;
			add(
				&mut total,
				capacity
					.checked_mul(size_of::<I::Scalar>())
					.ok_or(Error::Budget("cover evidence"))?,
			)?;
			for value in cover
				.covered
				.iter()
				.map(|root| &root.interval)
				.chain(&cover.excluded)
				.chain(&cover.unresolved)
			{
				add(
					&mut total,
					enclosure
						.storage_bytes(value)?
						.saturating_sub(size_of::<I::Scalar>()),
				)?;
			}
		}
	}
	Ok(total)
}
