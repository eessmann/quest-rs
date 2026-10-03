use crate::{Complex64, Error, IoPolicy, Result, finite};
use quest_polynomial::{
	Chebyshev, Conversion, Hermite, Jacobi, Laguerre, Laurent, Monomial, Polynomial,
};
use quest_qsp::{ControlSequence, PhaseSequence, WxLaurent, WxSymmetric};
use serde_json::Value;

mod transport;
use transport::{Basis, Envelope, Kind};

#[derive(Debug, Clone)]
pub enum QspInput {
	Symmetric(PhaseSequence<WxSymmetric>),
	Laurent(PhaseSequence<WxLaurent>),
	GeneralizedAngles(GeneralizedAngleInput),
	GeneralizedMatrices(ControlSequence),
	Polynomial(PolynomialInput),
	#[cfg(feature = "certification")]
	Compiled(crate::CompiledInput),
}
/// Immutable source angles and their admitted execution matrices.
/// The matrices are the execution authority; angles are source provenance.
///
/// ```compile_fail
/// use quest_qsvt_io::GeneralizedAngleInput;
/// let mut input = GeneralizedAngleInput::new(vec![0.2], vec![0.3]).unwrap();
/// input.psi.push(0.4);
/// ```
///
/// ```compile_fail
/// use quest_qsvt_io::GeneralizedAngleInput;
/// let mut input = GeneralizedAngleInput::new(vec![0.2], vec![0.3]).unwrap();
/// input.controls = input.controls().clone();
/// ```
#[derive(Debug, Clone)]
pub struct GeneralizedAngleInput {
	psi: Vec<f64>,
	phi: Vec<f64>,
	controls: ControlSequence,
}
impl GeneralizedAngleInput {
	/// Admit source angles once and freeze their matrix execution authority.
	///
	/// # Errors
	/// Rejects invalid angle shapes, nonfinite values or failed matrix admission.
	pub fn new(psi: Vec<f64>, phi: Vec<f64>) -> Result<Self> {
		let controls = ControlSequence::builder().angles(&psi, &phi)?.build()?;
		Ok(Self { psi, phi, controls })
	}
	fn with_frozen_controls(
		psi: Vec<f64>,
		phi: Vec<f64>,
		controls: ControlSequence,
	) -> Result<Self> {
		if psi.len() != controls.matrices().len()
			|| phi.len() != psi.len()
			|| psi.iter().chain(&phi).any(|v| !v.is_finite())
		{
			return Err(Error::Format(
				"source angles and frozen controls differ in shape or finiteness",
			));
		}
		Ok(Self { psi, phi, controls })
	}
	#[must_use]
	pub fn psi(&self) -> &[f64] {
		&self.psi
	}
	#[must_use]
	pub fn phi(&self) -> &[f64] {
		&self.phi
	}
	#[must_use]
	pub const fn controls(&self) -> &ControlSequence {
		&self.controls
	}
	#[must_use]
	pub fn into_controls(self) -> ControlSequence {
		self.controls
	}
}
#[derive(Debug, Clone)]
enum AdmittedPolynomial {
	Monomial(Polynomial<Monomial>),
	Chebyshev(Polynomial<Chebyshev>),
	Laurent(Polynomial<Laurent>),
	Hermite(Polynomial<Hermite>),
	Laguerre(Polynomial<Laguerre>),
	Jacobi(Polynomial<Jacobi>),
}
/// A shape/basis-admitted polynomial and its original interchange representation.
#[derive(Debug, Clone)]
pub struct PolynomialInput {
	polynomial: AdmittedPolynomial,
	original: Value,
}
/// Immutable conversion retaining the admitted original interchange source.
#[derive(Debug, Clone)]
pub struct PolynomialConversion {
	source: PolynomialInput,
	polynomial: Polynomial<Chebyshev>,
	bound: f64,
}
impl PolynomialConversion {
	#[must_use]
	pub const fn source(&self) -> &PolynomialInput {
		&self.source
	}
	#[must_use]
	pub const fn polynomial(&self) -> &Polynomial<Chebyshev> {
		&self.polynomial
	}
	#[must_use]
	pub const fn coefficient_error_bound(&self) -> f64 {
		self.bound
	}
}
impl PolynomialInput {
	/// Convert while retaining outward coefficient conversion error evidence.
	///
	/// # Errors
	/// Rejects unsupported support, numerical failures or conversion budgets.
	pub fn to_chebyshev(&self) -> Result<PolynomialConversion> {
		fn parts<S: quest_polynomial::Basis>(
			c: Conversion<Chebyshev, S>,
		) -> (Polynomial<Chebyshev>, f64) {
			let bound = c.coefficient_error_bound();
			(c.into_polynomial(), bound)
		}
		let (polynomial, bound) = match &self.polynomial {
			AdmittedPolynomial::Chebyshev(p) => (p.clone(), 0.0),
			AdmittedPolynomial::Monomial(p) => parts(p.to_basis(Chebyshev)?),
			AdmittedPolynomial::Laurent(p) => parts(p.to_basis(Chebyshev)?),
			AdmittedPolynomial::Hermite(p) => parts(p.to_basis(Chebyshev)?),
			AdmittedPolynomial::Laguerre(p) => parts(p.to_basis(Chebyshev)?),
			AdmittedPolynomial::Jacobi(p) => parts(p.to_basis(Chebyshev)?),
		};
		Ok(PolynomialConversion {
			source: self.clone(),
			polynomial,
			bound,
		})
	}
	/// Nonnegative Laurent payload when already supplied in that basis.
	#[must_use]
	pub const fn laurent(&self) -> Option<&Polynomial<Laurent>> {
		match &self.polynomial {
			AdmittedPolynomial::Laurent(p) => Some(p),
			_ => None,
		}
	}
}

fn bounded<T>(values: &[T], policy: IoPolicy) -> Result<()> {
	if values.len() > policy.max_coefficients {
		return Err(Error::Budget("coefficient count"));
	}
	policy.check(values.len(), 8)
}

/// Read the canonical C++ JSON conventions or the lossless frozen-matrix extension.
///
/// # Errors
/// Rejects invalid types, conventions, finite/shape admission and storage limits.
pub fn read_qsp_json(source: &str, policy: IoPolicy) -> Result<QspInput> {
	read_qsp_json_impl(source, policy, false, None)
}

/// Read source/sequence JSON or independently recertify compiled JSON at the requested tolerance.
/// # Errors
/// Rejects invalid tolerances, payloads, budgets or independent certification failure.
pub fn read_qsp_json_with_tolerance(
	source: &str,
	policy: IoPolicy,
	tolerance: f64,
) -> Result<QspInput> {
	if !tolerance.is_finite() || tolerance <= 0.0 {
		return Err(Error::Format(
			"positive finite verification tolerance required",
		));
	}
	read_qsp_json_impl(source, policy, false, Some(tolerance))
}

/// Read an execution payload. Imported generalized angles must carry admitted
/// matrix words so workers never reconstruct them with local trigonometry.
///
/// # Errors
/// Rejects source-only generalized angles and invalid interchange payloads.
pub fn read_qsp_execution_json(source: &str, policy: IoPolicy) -> Result<QspInput> {
	read_qsp_json_impl(source, policy, true, None)
}

/// Read a frozen execution wire with an explicitly selected verification tolerance.
/// # Errors
/// Rejects malformed execution authority, invalid tolerances or failed recertification.
pub fn read_qsp_execution_json_with_tolerance(
	source: &str,
	policy: IoPolicy,
	tolerance: f64,
) -> Result<QspInput> {
	if !tolerance.is_finite() || tolerance <= 0.0 {
		return Err(Error::Format(
			"positive finite verification tolerance required",
		));
	}
	read_qsp_json_impl(source, policy, true, Some(tolerance))
}
#[cfg(not(feature = "certification"))]
const fn compiled_at_tolerance(
	_source: &str,
	_policy: IoPolicy,
	_tolerance: Option<f64>,
) -> Result<QspInput> {
	Err(Error::Format(
		"compiled QSP artifacts require the certification feature",
	))
}
#[cfg(feature = "certification")]
fn compiled_at_tolerance(
	source: &str,
	policy: IoPolicy,
	tolerance: Option<f64>,
) -> Result<QspInput> {
	let tolerance = tolerance.unwrap_or(1e-11);
	let verification = quest_qsp::certification::CertificationPolicy {
		response_tolerance: tolerance,
		completion_tolerance: tolerance,
		conversion_tolerance: tolerance,
		reconstruction_tolerance: tolerance,
		unitarity_tolerance: tolerance,
		..quest_qsp::certification::CertificationPolicy::default()
	};
	crate::read_compiled_qsp_json(source, policy, verification).map(QspInput::Compiled)
}
fn read_qsp_json_impl(
	source: &str,
	policy: IoPolicy,
	execution: bool,
	verification_tolerance: Option<f64>,
) -> Result<QspInput> {
	if source.len() > policy.max_bytes {
		return Err(Error::Budget("JSON source"));
	}
	if source
		.len()
		.checked_mul(32)
		.is_none_or(|bytes| bytes > policy.max_bytes)
	{
		return Err(Error::Budget("JSON decoded storage"));
	}
	let mut envelope: Envelope = serde_json::from_str(source)?;
	let kind = envelope.kind(execution)?;
	// Dynamic metadata was syntax-admitted above. Only a polynomial retains it,
	// by decoding the original representation after domain admission below.
	envelope.metadata.clear();
	match kind {
		Kind::Compiled => compiled_at_tolerance(source, policy, verification_tolerance),
		Kind::Phases => {
			let angles = envelope.angles.required("missing phase angles")?;
			bounded(&angles, policy)?;
			match envelope.convention.optional().as_deref() {
				Some("pyqsp-wx-symmetric") => {
					Ok(QspInput::Symmetric(PhaseSequence::builder(angles).build()?))
				}
				Some("pyqsp-wx-laurent") => {
					Ok(QspInput::Laurent(PhaseSequence::builder(angles).build()?))
				}
				_ => Err(Error::Format("missing or unsupported phase convention")),
			}
		}
		Kind::Matrices => read_controls(envelope, policy),
		Kind::Angles => {
			let psi = envelope.psi.required("missing psi source angles")?;
			let phi = envelope.phi.required("missing phi source angles")?;
			bounded(&psi, policy)?;
			bounded(&phi, policy)?;
			Ok(QspInput::GeneralizedAngles(GeneralizedAngleInput::new(
				psi, phi,
			)?))
		}
		Kind::Polynomial => read_polynomial(envelope, source, policy).map(QspInput::Polynomial),
	}
}
fn read_controls(envelope: Envelope, policy: IoPolicy) -> Result<QspInput> {
	let matrices = if let Some(raw) = envelope.control_words.optional() {
		if envelope.convention.optional().as_deref() != Some("gqsp-matrix-words-v1") {
			return Err(Error::Format("frozen controls require a convention tag"));
		}
		bounded(&raw, policy)?;
		raw.into_iter()
			.map(|matrix| {
				matrix.map(|row| {
					row.map(|[re, im]| Complex64::new(f64::from_bits(re), f64::from_bits(im)))
				})
			})
			.collect()
	} else {
		if envelope.convention.optional().as_deref() != Some("gqsp-matrix-upper-left-v1") {
			return Err(Error::Format("frozen controls require a convention tag"));
		}
		let raw = envelope.controls.required("missing frozen controls")?;
		bounded(&raw, policy)?;
		raw.into_iter()
			.map(|matrix| matrix.map(|row| row.map(|component| component.0)))
			.collect()
	};
	let controls = ControlSequence::builder().matrices(matrices).build()?;
	let provenance = if envelope.psi_words.present() || envelope.phi_words.present() {
		let psi = envelope.psi_words.required("missing psi source words")?;
		let phi = envelope.phi_words.required("missing phi source words")?;
		bounded(&psi, policy)?;
		bounded(&phi, policy)?;
		Some((
			psi.into_iter().map(f64::from_bits).collect(),
			phi.into_iter().map(f64::from_bits).collect(),
		))
	} else if envelope.psi.present() || envelope.phi.present() {
		let psi = envelope.psi.required("missing psi source angles")?;
		let phi = envelope.phi.required("missing phi source angles")?;
		bounded(&psi, policy)?;
		bounded(&phi, policy)?;
		Some((psi, phi))
	} else {
		None
	};
	match provenance {
		Some((psi, phi)) => Ok(QspInput::GeneralizedAngles(
			GeneralizedAngleInput::with_frozen_controls(psi, phi, controls)?,
		)),
		None => Ok(QspInput::GeneralizedMatrices(controls)),
	}
}
fn read_polynomial(envelope: Envelope, source: &str, policy: IoPolicy) -> Result<PolynomialInput> {
	let raw = envelope
		.coefficients
		.required("missing polynomial coefficients")?;
	bounded(&raw, policy)?;
	let mut coefficients = raw
		.into_iter()
		.map(|component| finite(component.0))
		.collect::<Result<Vec<_>>>()?;
	let basis = envelope.basis.optional().unwrap_or_default();
	let offset = envelope.minimum_order.optional().unwrap_or(0);
	if !matches!(basis, Basis::Laurent) && offset != 0 {
		let prefix = usize::try_from(offset)
			.map_err(|_| Error::Format("negative support requires Laurent basis"))?;
		let count = prefix
			.checked_add(coefficients.len())
			.ok_or(Error::Budget("polynomial support"))?;
		if count > policy.max_coefficients {
			return Err(Error::Budget("polynomial support"));
		}
		policy.check(count, 2)?;
		let mut padded = Vec::new();
		padded
			.try_reserve_exact(count)
			.map_err(|_| Error::Budget("polynomial padding"))?;
		padded.resize(prefix, Complex64::new(0.0, 0.0));
		padded.extend(coefficients);
		coefficients = padded;
	}
	let parameters = envelope.parameters.optional().unwrap_or_default();
	if parameters.iter().any(|value| !value.is_finite()) {
		return Err(Error::NonFinite);
	}
	let limits = policy.polynomial_limits();
	let polynomial = match (basis, parameters.as_slice()) {
		(Basis::Monomial, []) => {
			AdmittedPolynomial::Monomial(Polynomial::new(Monomial, coefficients, limits)?)
		}
		(Basis::Chebyshev, []) => {
			AdmittedPolynomial::Chebyshev(Polynomial::new(Chebyshev, coefficients, limits)?)
		}
		(Basis::Laurent, []) => AdmittedPolynomial::Laurent(Polynomial::new(
			Laurent::new(offset),
			coefficients,
			limits,
		)?),
		(Basis::Hermite, []) => AdmittedPolynomial::Hermite(Polynomial::new(
			Hermite::physicists(),
			coefficients,
			limits,
		)?),
		(Basis::Laguerre, []) => AdmittedPolynomial::Laguerre(Polynomial::new(
			Laguerre::new(0.0)?,
			coefficients,
			limits,
		)?),
		(Basis::Laguerre, [alpha]) => AdmittedPolynomial::Laguerre(Polynomial::new(
			Laguerre::new(*alpha)?,
			coefficients,
			limits,
		)?),
		(Basis::Jacobi, []) => AdmittedPolynomial::Jacobi(Polynomial::new(
			Jacobi::new(0.0, 0.0)?,
			coefficients,
			limits,
		)?),
		(Basis::Jacobi, [alpha, beta]) => AdmittedPolynomial::Jacobi(Polynomial::new(
			Jacobi::new(*alpha, *beta)?,
			coefficients,
			limits,
		)?),
		_ => return Err(Error::Format("unsupported polynomial basis/parameters")),
	};
	Ok(PolynomialInput {
		polynomial,
		original: serde_json::from_str(source)?,
	})
}

/// Export phases with original tags or frozen matrices without refactorization.
/// The matrix extension preserves actual binary64 values through JSON round trips.
///
/// # Errors
/// Returns JSON serialization errors.
pub fn write_qsp_json(input: &QspInput) -> Result<String> {
	let encoded = match input {
		#[cfg(feature = "certification")]
		QspInput::Compiled(p) => return Ok(p.json().to_owned()),
		QspInput::Symmetric(p) => serde_json::to_string_pretty(&transport::Phases {
			convention: p.convention(),
			angles: p.values(),
		})?,
		QspInput::Laurent(p) => serde_json::to_string_pretty(&transport::Phases {
			convention: p.convention(),
			angles: p.values(),
		})?,
		QspInput::GeneralizedAngles(angles) => {
			serde_json::to_string_pretty(&transport::SourceAngles {
				psi: angles.psi(),
				phi: angles.phi(),
			})?
		}
		QspInput::GeneralizedMatrices(p) => serde_json::to_string_pretty(&transport::Controls {
			convention: "gqsp-matrix-upper-left-v1",
			controls: p
				.matrices()
				.iter()
				.map(|matrix| matrix.map(|row| row.map(|value| [value.re, value.im])))
				.collect(),
		})?,
		QspInput::Polynomial(p) => serde_json::to_string_pretty(&p.original)?,
	};
	Ok(encoded)
}

/// Serialize execution authority, preserving exact admitted matrix words and
/// original angle provenance when the source was an angle import.
///
/// # Errors
/// Returns JSON serialization errors.
pub fn write_qsp_execution_json(input: &QspInput) -> Result<String> {
	let (controls, provenance) = match input {
		QspInput::GeneralizedAngles(angles) => (angles.controls(), Some(angles)),
		QspInput::GeneralizedMatrices(controls) => (controls, None),
		_ => return write_qsp_json(input),
	};
	let payload = transport::ExecutionControls {
		convention: "gqsp-matrix-words-v1",
		control_words: controls
			.matrices()
			.iter()
			.map(|matrix| {
				matrix.map(|row| row.map(|value| [value.re.to_bits(), value.im.to_bits()]))
			})
			.collect(),
		psi_words: provenance
			.map(|angles| angles.psi().iter().map(|value| value.to_bits()).collect()),
		phi_words: provenance
			.map(|angles| angles.phi().iter().map(|value| value.to_bits()).collect()),
	};
	Ok(serde_json::to_string_pretty(&payload)?)
}
