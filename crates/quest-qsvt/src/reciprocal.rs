//! Resource-bounded reciprocal synthesis with explicit physical spectral evidence.
//!
//! The supplied encoding is for A adjoint: odd SVT then maps b to A pseudoinverse b.
//! No dense matrix, SVD or normal equations are required by this interface.
use crate::{
	Complex64, Error, NumericalPolicy, ProjectedEncoding, Result, TransformBuilder,
	ValidatedTransform,
};
use quest_numerics::Interval;
use quest_polynomial::{Chebyshev, Limits, Polynomial};
use std::{
	ops::{Div, Mul},
	sync::Arc,
};

/// Provenance is retained; a caller premise is not upgraded to a certificate.
#[derive(Clone, Debug)]
pub enum SpectralEvidence {
	/// Independently derived bound (the derivation is identified, not re-proved here).
	Analytic { description: String },
	/// Bounded small-reference singular value calculation, including its size.
	DenseReference {
		dimension: usize,
		description: String,
	},
	/// Explicit external premise, requiring independent validation by the caller.
	CallerPremise { description: String },
}
/// Physical singular-value bounds on the admitted nonzero spectral subspace.
#[derive(Clone, Debug)]
pub struct SpectralBounds {
	lower: f64,
	upper: f64,
	evidence: Arc<SpectralEvidence>,
}
impl SpectralBounds {
	/// # Errors
	/// Rejects a nonpositive, nonfinite or reversed interval, and empty provenance.
	pub fn new(lower: f64, upper: f64, evidence: SpectralEvidence) -> Result<Self> {
		let description = match &evidence {
			SpectralEvidence::Analytic { description }
			| SpectralEvidence::DenseReference { description, .. }
			| SpectralEvidence::CallerPremise { description } => description,
		};
		if !lower.is_finite()
			|| !upper.is_finite()
			|| lower <= 0.0
			|| upper < lower
			|| description.trim().is_empty()
		{
			return Err(Error::Encoding(
				"finite positive spectral interval and evidence required",
			));
		}
		Ok(Self {
			lower,
			upper,
			evidence: Arc::new(evidence),
		})
	}
	#[must_use]
	pub const fn lower(&self) -> f64 {
		self.lower
	}
	#[must_use]
	pub const fn upper(&self) -> f64 {
		self.upper
	}
	#[must_use]
	pub fn evidence(&self) -> &SpectralEvidence {
		&self.evidence
	}
	/// Retained immutable provenance capacity and scalar wrapper metadata.
	/// # Errors
	/// Rejects byte-count overflow; allocator bookkeeping is excluded.
	pub fn retained_bytes(&self) -> Result<usize> {
		let description = match &*self.evidence {
			SpectralEvidence::Analytic { description }
			| SpectralEvidence::DenseReference { description, .. }
			| SpectralEvidence::CallerPremise { description } => description,
		};
		description
			.capacity()
			.checked_add(size_of::<SpectralEvidence>())
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.ok_or(Error::Budget("spectral provenance storage"))
	}
}
/// Chebyshev truncation of c [1-(1-x²)^b]/x with outward error bounds.
///
/// Positive binomial tails avoid alternating power-basis cancellation. Discarded
/// Chebyshev coefficients contribute their entire one-norm to the error bound.
#[derive(Clone, Debug)]
pub struct ReciprocalPolynomial {
	target: Polynomial<Chebyshev>,
	alpha: f64,
	delta: f64,
	scale: f64,
	error_bound: f64,
	rounding_bound: f64,
	global_magnitude_bound: f64,
	coefficient_capacity_bytes: usize,
	spectrum: SpectralBounds,
}
fn interval(value: f64) -> Result<Interval> {
	Ok(Interval::point(value)?)
}
impl ReciprocalPolynomial {
	/// Construct coefficients from positive binomial probabilities with outward arithmetic.
	///
	/// For |x|>=delta, the analytic remainder is c (1-delta²)^b/delta.
	/// The global magnitude is <= c sqrt(b)=1/2 before coefficient rounding.
	///
	/// # Errors
	/// Rejects incompatible normalization, impossible budgets or unestablished error.
	#[allow(
		clippy::arithmetic_side_effects,
		clippy::cast_precision_loss,
		clippy::cast_possible_truncation,
		clippy::cast_sign_loss,
		clippy::as_conversions,
		clippy::too_many_lines,
		reason = "Exponent cast is checked against a u32-representable admitted degree"
	)]
	pub fn geometric(
		spectrum: &SpectralBounds,
		alpha: f64,
		tolerance: f64,
		max_degree: usize,
		policy: NumericalPolicy,
	) -> Result<Self> {
		if !alpha.is_finite()
			|| alpha < spectrum.upper
			|| !tolerance.is_finite()
			|| tolerance <= 0.0
			|| tolerance >= 1.0
		{
			return Err(Error::Encoding(
				"invalid reciprocal normalization or tolerance",
			));
		}
		let delta = interval(spectrum.lower)?
			.checked_div(interval(alpha)?)?
			.lower();
		if delta <= 0.0 {
			return Err(Error::Encoding("normalized spectral lower bound underflow"));
		}
		let estimate = if delta >= 1.0 {
			1.0
		} else {
			(-((tolerance * 0.5) * delta).ln() / (delta * delta))
				.ceil()
				.max(1.0)
		};
		let max_b = u32::try_from(policy.max_bytes / 64).unwrap_or(u32::MAX);
		if !estimate.is_finite() || estimate > f64::from(max_b) {
			return Err(Error::Budget("reciprocal approximation degree"));
		}
		let b = estimate as usize;
		let count = b
			.checked_mul(2)
			.ok_or(Error::Budget("reciprocal coefficients"))?
			.min(
				max_degree
					.checked_add(1)
					.ok_or(Error::Budget("reciprocal degree"))?,
			);
		let bytes = count
			.checked_mul(size_of::<Complex64>())
			.and_then(|n| n.checked_add(b.checked_add(1)?.checked_mul(size_of::<Interval>())?))
			.and_then(|n| n.checked_add(spectrum.retained_bytes().ok()?))
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.ok_or(Error::Budget("reciprocal storage"))?;
		if bytes > policy.max_bytes {
			return Err(Error::Budget("reciprocal storage"));
		}
		let bf = b as f64;
		let scale = interval(0.5)?.checked_div(interval(bf)?.sqrt()?)?.lower();
		let mut probabilities = Vec::new();
		probabilities
			.try_reserve_exact(b + 1)
			.map_err(|_| Error::Budget("reciprocal probabilities"))?;
		let mut p = interval(1.0)?;
		for i in 1..=b {
			p = p.checked_mul(
				interval(1.0)?
					.checked_sub(interval(1.0)?.checked_div(interval(2.0 * (i as f64))?)?)?,
			)?;
		}
		probabilities.push(p);
		for i in 1..=b {
			p = p.checked_mul(
				interval((b - i + 1) as f64)?.checked_div(interval((b + i) as f64)?)?,
			)?;
			probabilities.push(p);
		}
		let mut coefficients = Vec::new();
		coefficients
			.try_reserve_exact(count)
			.map_err(|_| Error::Budget("reciprocal coefficients"))?;
		coefficients.resize(count, Complex64::new(0.0, 0.0));
		let mut tail = interval(0.0)?;
		let mut rounding = interval(0.0)?;
		let mut truncation = interval(0.0)?;
		for j in (0..b).rev() {
			tail = tail.checked_add(
				*probabilities
					.get(
						j.checked_add(1)
							.ok_or(Error::Budget("reciprocal probability index"))?,
					)
					.ok_or(Error::Encoding("reciprocal probability index"))?,
			)?;
			let coefficient = tail.checked_mul(interval(4.0 * scale)?)?;
			if 2 * j + 1 >= count
				|| (j > 0 && truncation.checked_add(coefficient)?.upper() <= tolerance * 0.25)
			{
				truncation = truncation.checked_add(coefficient)?;
				continue;
			}
			let midpoint = coefficient.lower().midpoint(coefficient.upper());
			*coefficients
				.get_mut(
					j.checked_mul(2)
						.and_then(|i| i.checked_add(1))
						.ok_or(Error::Budget("reciprocal coefficient index"))?,
				)
				.ok_or(Error::Encoding("reciprocal coefficient index"))? =
				Complex64::new(if j % 2 == 0 { midpoint } else { -midpoint }, 0.0);
			let radius = (midpoint - coefficient.lower())
				.abs()
				.max((coefficient.upper() - midpoint).abs())
				.next_up();
			rounding = rounding.checked_add(interval(radius)?)?;
		}
		let decay = interval(1.0)?.checked_sub(interval(delta)?.square()?)?;
		let remainder = if decay.upper() <= 0.0 {
			interval(0.0)?
		} else {
			Interval::new(decay.upper(), decay.upper())?
				.ln()?
				.checked_mul(interval(bf)?)?
				.exp()?
				.checked_mul(interval(scale)?)?
				.checked_div(interval(delta)?)?
		};
		let global_magnitude_bound = interval(scale)?
			.checked_mul(interval(bf)?.sqrt()?)?
			.checked_add(rounding)?
			.checked_add(truncation)?
			.upper();
		if global_magnitude_bound >= 1.0 {
			return Err(Error::Encoding(
				"reciprocal global contractivity bound not established",
			));
		}
		let error_bound = remainder
			.checked_add(rounding)?
			.checked_add(truncation)?
			.upper();
		if error_bound > tolerance {
			return Err(Error::Residual {
				operation: "reciprocal approximation",
				residual: error_bound,
				tolerance,
			});
		}
		let retained = coefficients
			.iter()
			.rposition(|c| c.re != 0.0)
			.map_or(0, |i| i + 1);
		coefficients.truncate(retained);
		let limits = Limits {
			shapes: quest_numerics::ShapeLimits {
				max_coefficients: count,
				..(Limits::default()).shapes
			},
			resources: quest_numerics::ResourceLimits {
				max_peak_bytes: policy.max_bytes,
				..(Limits::default()).resources
			},
		};
		let coefficient_capacity_bytes = coefficients
			.capacity()
			.checked_mul(size_of::<Complex64>())
			.ok_or(Error::Budget("reciprocal coefficient storage"))?;
		Ok(Self {
			coefficient_capacity_bytes,
			target: Polynomial::new(Chebyshev, coefficients, limits)?,
			alpha,
			delta,
			scale,
			error_bound,
			rounding_bound: rounding.upper(),
			global_magnitude_bound,
			spectrum: spectrum.clone(),
		})
	}
	/// Retained coefficient capacity, spectral provenance and wrapper/Arc metadata.
	/// # Errors
	/// Rejects resource-accounting overflow; allocator bookkeeping is excluded.
	pub fn retained_bytes(&self) -> Result<usize> {
		self.coefficient_capacity_bytes
			.checked_add(self.spectrum.retained_bytes()?)
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(size_of::<Vec<Complex64>>()))
			.and_then(|n| n.checked_add(2_usize.checked_mul(size_of::<usize>())?))
			.ok_or(Error::Budget("reciprocal retained storage"))
	}
	#[must_use]
	pub const fn target(&self) -> &Polynomial<Chebyshev> {
		&self.target
	}
	#[must_use]
	pub const fn scale(&self) -> f64 {
		self.scale
	}
	#[must_use]
	pub const fn alpha(&self) -> f64 {
		self.alpha
	}
	#[must_use]
	pub const fn normalized_lower_bound(&self) -> f64 {
		self.delta
	}
	#[must_use]
	pub const fn error_bound(&self) -> f64 {
		self.error_bound
	}
	#[must_use]
	pub const fn coefficient_rounding_bound(&self) -> f64 {
		self.rounding_bound
	}
	/// Outward global magnitude bound on [-1,1], including coefficient rounding and truncation.
	#[must_use]
	pub const fn global_magnitude_bound(&self) -> f64 {
		self.global_magnitude_bound
	}
	#[must_use]
	pub const fn spectrum(&self) -> &SpectralBounds {
		&self.spectrum
	}
	/// # Errors
	/// Rejects a nonfinite signal or failed polynomial evaluation.
	pub fn evaluate(&self, x: f64) -> Result<f64> {
		Ok(self.target.evaluate_real(x)?)
	}
	/// Rescale unnormalized successful amplitudes to physical solution units.
	/// # Errors
	/// Rejects nonfinite/zero RHS norm or over/underflow.
	pub fn physical_rescaling(&self, rhs_norm: f64) -> Result<f64> {
		let scale = rhs_norm.div(self.alpha.mul(self.scale));
		if !rhs_norm.is_finite() || rhs_norm <= 0.0 || !scale.is_finite() || scale <= 0.0 {
			return Err(Error::Encoding("invalid physical reciprocal rescaling"));
		}
		Ok(scale)
	}
	/// Bound the physical relative residual under explicit response and execution error premises.
	///
	/// Both supplied bounds are uniform successful-amplitude operator errors on
	/// the normalized input. The execution bound must include encoding error and
	/// arithmetic error; this API does not certify either premise. The recorded
	/// spectral evidence and polynomial error are preserved unchanged.
	///
	/// # Errors
	/// Rejects negative/nonfinite bounds or an unrepresentable enclosing result.
	pub fn relative_residual_bound(
		&self,
		projector_uniform_error: f64,
		execution_amplitude_error: f64,
	) -> Result<f64> {
		if !projector_uniform_error.is_finite()
			|| projector_uniform_error < 0.0
			|| !execution_amplitude_error.is_finite()
			|| execution_amplitude_error < 0.0
		{
			return Err(Error::Encoding(
				"invalid reciprocal response/execution error premise",
			));
		}
		let error = interval(self.error_bound)?
			.checked_add(interval(projector_uniform_error)?)?
			.checked_add(interval(execution_amplitude_error)?)?;
		Ok(interval(self.spectrum.upper)?
			.checked_mul(error)?
			.checked_div(interval(self.alpha)?.checked_mul(interval(self.scale)?)?)?
			.upper())
	}
	/// Admit the conditional relative residual bound against a positive finite tolerance.
	///
	/// # Errors
	/// Retains invalid premises and insufficient error-budget outcomes as errors.
	pub fn admit_relative_residual(
		&self,
		projector_uniform_error: f64,
		execution_amplitude_error: f64,
		tolerance: f64,
	) -> Result<f64> {
		if !tolerance.is_finite() || tolerance <= 0.0 {
			return Err(Error::Encoding("invalid relative residual tolerance"));
		}
		let bound =
			self.relative_residual_bound(projector_uniform_error, execution_amplitude_error)?;
		if bound > tolerance {
			return Err(Error::Residual {
				operation: "conditional reciprocal relative residual",
				residual: bound,
				tolerance,
			});
		}
		Ok(bound)
	}
	/// Synthesize against the same frozen coefficient target using existing NLFT infrastructure.
	/// # Errors
	/// Retains all contractivity, completion, response and resource failures.
	pub fn synthesize(
		&self,
		policy: quest_qsp::Policy,
	) -> Result<quest_qsp::FrozenCandidate<quest_qsp::RealParityWx>> {
		Ok(quest_qsp::SynthesisBuilder::new()
			.policy(policy)
			.real_parity_wx(&self.target)?
			.admit()?
			.complete()?
			.synthesize()?)
	}
	/// Build the bounded portable transform for an encoding of A adjoint.
	///
	/// `ProjectedEncoding` carries no semantic matrix identity. The orientation
	/// assertion is explicit in this method name and must be checked by the caller.
	/// Synthesis diagnostics are retained by the returned candidate; this method
	/// does not upgrade them to an independent certificate.
	/// # Errors
	/// Rejects inconsistent alpha or QSP/oracle construction failures.
	pub fn transform_of_adjoint(
		&self,
		encoding: ProjectedEncoding,
		policy: quest_qsp::Policy,
	) -> Result<(
		ValidatedTransform,
		quest_qsp::FrozenCandidate<quest_qsp::RealParityWx>,
	)> {
		if encoding.normalization().get().to_bits() != self.alpha.to_bits() {
			return Err(Error::Encoding(
				"reciprocal and oracle normalization disagree",
			));
		}
		let frozen = self.synthesize(policy)?;
		let transform = TransformBuilder::new()
			.encoding(encoding)
			.standard(frozen.phase_sequence())
			.build()?;
		Ok((transform, frozen))
	}
}
