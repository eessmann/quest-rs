//! Conservative continuous-time Carleman truncation certificates for recorded quadratic ODEs.
//!
//! Uses logarithmic-norm contraction, not eigenvalue real parts. This certificate
//! excludes coefficient-extraction, spatial, temporal, encoding, preparation,
//! inverse-polynomial and observable errors. It is not a quantum complexity claim.
#![allow(
	clippy::arithmetic_side_effects,
	reason = "Floating candidates are independently admitted using outward interval arithmetic"
)]
use crate::{
	CfdError,
	polynomial::{CoefficientEvidence, PolynomialOde},
};
use quest_numerics::Interval;

/// A checked sufficient bound for the exact solution of the recorded polynomial ODE.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TruncationCertificate {
	scale: f64,
	initial_norm_upper: f64,
	scaled_initial_norm_upper: f64,
	rc_upper: f64,
	dissipation_lower: f64,
	scaled_quadratic_upper: f64,
	scaled_forcing_upper: f64,
	order: usize,
	horizon: f64,
	physical_error_bound: f64,
	scope: &'static str,
}
impl TruncationCertificate {
	/// Physical scale for every retained normalized symmetric monomial.
	#[must_use]
	pub const fn scale(&self) -> f64 {
		self.scale
	}
	/// Absolute Euclidean error in the recovered degree-one physical coordinates.
	#[must_use]
	pub const fn physical_error_bound(&self) -> f64 {
		self.physical_error_bound
	}
	/// Conservative nonlinearity/forcing ratio using full-space logarithmic decay.
	#[must_use]
	pub const fn rc_upper(&self) -> f64 {
		self.rc_upper
	}
}
/// Inconclusive sufficient conditions do not imply divergence of the lift.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TruncationAdmission {
	pub certificate: Option<TruncationCertificate>,
	pub reason: &'static str,
}
const fn inconclusive(reason: &'static str) -> TruncationAdmission {
	TruncationAdmission {
		certificate: None,
		reason,
	}
}
fn power(mut base: Interval, mut exponent: usize) -> Result<Interval, CfdError> {
	let mut result = Interval::point(1.)?;
	while exponent != 0 {
		if exponent & 1 != 0 {
			result = result.checked_mul(base)?;
		}
		exponent >>= 1;
		if exponent != 0 {
			base = base.square()?;
		}
	}
	Ok(result)
}
/// Seek and independently verify a scale satisfying strict full-space contraction.
///
/// Requires autonomous F1/F2, nonzero initial state, an invariant initial-norm ball,
/// and the corrected forcing restriction after rescaling. The stronger logarithmic
/// norm condition handles nonnormal operators without assuming diagonalizability.
/// Every physical coordinate participates in the bounds, including conserved means.
///
/// # Errors
/// Rejects malformed inputs, degree outside `1..=1_000_000`, interval overflow and
/// the coefficient-evidence storage/work limits. Failed sufficient hypotheses
/// return an inconclusive report rather than an error or a divergence claim.
pub fn admit_truncation(
	ode: &PolynomialOde,
	initial: &[f64],
	horizon: f64,
	order: usize,
) -> Result<TruncationAdmission, CfdError> {
	if initial.len() != ode.dimension()
		|| initial.iter().any(|v| !v.is_finite())
		|| !horizon.is_finite()
		|| horizon <= 0.
		|| !(1..=1_000_000).contains(&order)
	{
		return Err(CfdError::InvalidInput(
			"invalid truncation certificate inputs",
		));
	}
	let mut squared = Interval::point(0.)?;
	for &value in initial {
		squared = squared.checked_add(Interval::point(value)?.square()?)?;
	}
	let norm = squared.sqrt()?;
	if norm.lower() <= 0. {
		return Ok(inconclusive(
			"nonzero initial norm is required; zero problems use the separately reported exact/forced route",
		));
	}
	let evidence = ode.coefficient_evidence(horizon)?;
	if !evidence.autonomous_linear_quadratic || evidence.logarithmic_norm_upper >= 0. {
		return Ok(inconclusive(
			"autonomous full-space logarithmic contraction was not established",
		));
	}
	let gamma = -evidence.logarithmic_norm_upper;
	let b = evidence.quadratic_norm_upper;
	let c = evidence.forcing_norm_upper;
	let rc = Interval::point(b)?
		.checked_mul(norm)?
		.checked_add(Interval::point(c)?.checked_div(norm)?)?
		.checked_div(Interval::point(gamma)?)?
		.upper();
	if rc >= 1. {
		return Ok(inconclusive(
			"the invariant initial-norm ball condition RC<1 was not established",
		));
	}
	// Candidate arithmetic is deliberately not certificate arithmetic. All strict
	// inequalities are rechecked with outward enclosures by certify_scale below.
	let first = 2. * norm.upper();
	if let Some(certificate) = certify_scale(&evidence, norm.upper(), first, horizon, order, rc)? {
		return Ok(TruncationAdmission {
			certificate: Some(certificate),
			reason: "continuous-time truncation bound admitted for the recorded polynomial ODE",
		});
	}
	if b == 0. || (c > 0. && evidence.quadratic_norm_lower <= 0.) {
		return Ok(inconclusive(
			"no corrected-forcing-compatible nonlinear scale was established",
		));
	}
	let discriminant = gamma.mul_add(gamma, -4. * b * c);
	if !discriminant.is_finite() || discriminant <= 0. {
		return Ok(inconclusive(
			"no representable strictly dissipative scaling interval was established",
		));
	}
	let denominator = gamma + discriminant.sqrt();
	let upper = denominator / (2. * b);
	let force = if c == 0. {
		0.
	} else {
		(c / evidence.quadratic_norm_lower).sqrt()
	};
	let lower = (2. * c / denominator).max(force).max(norm.upper());
	let candidate = lower.midpoint(upper);
	if lower < upper
		&& candidate.is_finite()
		&& let Some(certificate) =
			certify_scale(&evidence, norm.upper(), candidate, horizon, order, rc)?
	{
		return Ok(TruncationAdmission {
			certificate: Some(certificate),
			reason: "continuous-time truncation bound admitted for the recorded polynomial ODE",
		});
	}
	Ok(inconclusive(
		"no candidate passed strict interval scaling and corrected forcing checks",
	))
}
fn certify_scale(
	evidence: &CoefficientEvidence,
	norm: f64,
	scale: f64,
	horizon: f64,
	order: usize,
	rc: f64,
) -> Result<Option<TruncationCertificate>, CfdError> {
	if !scale.is_finite() || scale <= norm {
		return Ok(None);
	}
	let s = Interval::point(scale)?;
	let b = s.checked_mul(Interval::point(evidence.quadratic_norm_upper)?)?;
	let c = Interval::point(evidence.forcing_norm_upper)?.checked_div(s)?;
	let lower_b = s.checked_mul(Interval::point(evidence.quadratic_norm_lower)?)?;
	let gamma = -evidence.logarithmic_norm_upper;
	if c.upper() > lower_b.lower() || b.checked_add(c)?.upper() >= gamma {
		return Ok(None);
	}
	let rho = Interval::point(norm)?.checked_div(s)?;
	if rho.upper() >= 1. {
		return Ok(None);
	}
	let degree =
		u32::try_from(order).map_err(|_| CfdError::InvalidInput("certificate order overflow"))?;
	let bound = s
		.checked_mul(Interval::point(horizon)?)?
		.checked_mul(Interval::point(f64::from(degree))?)?
		.checked_mul(b)?
		.checked_mul(power(rho, order + 1)?)?
		.upper();
	Ok(Some(TruncationCertificate {
		scale,
		initial_norm_upper: norm,
		scaled_initial_norm_upper: rho.upper(),
		rc_upper: rc,
		dissipation_lower: gamma,
		scaled_quadratic_upper: b.upper(),
		scaled_forcing_upper: c.upper(),
		order,
		horizon,
		physical_error_bound: bound,
		scope: "continuous ordered/symmetric Carleman truncation of the recorded quadratic ODE only; all other error sources excluded",
	}))
}
