use crate::{Complex64, Error, IoPolicy, Result, finite};
use quest_polynomial::{
	Chebyshev, Conversion, Hermite, Jacobi, Laguerre, Laurent, Monomial, Polynomial,
};
use quest_qsp::{ControlSequence, PhaseSequence, WxLaurent, WxSymmetric};
use serde_json::{Value, json};

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

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
	value
		.get(key)
		.and_then(Value::as_array)
		.map(Vec::as_slice)
		.ok_or(Error::Format("missing array field"))
}
fn real(value: &Value) -> Result<f64> {
	let number = value
		.as_f64()
		.ok_or(Error::Format("expected real number"))?;
	if number.is_finite() {
		Ok(number)
	} else {
		Err(Error::NonFinite)
	}
}
fn reals(values: &[Value]) -> Result<Vec<f64>> {
	values.iter().map(real).collect()
}
fn complex(value: &Value) -> Result<Complex64> {
	if let Some(values) = value.as_array() {
		let [re, im] = values.as_slice() else {
			return Err(Error::Format("complex pair requires two components"));
		};
		finite(Complex64::new(real(re)?, real(im)?))
	} else {
		Ok(Complex64::new(real(value)?, 0.0))
	}
}
fn complex_words(value: &Value) -> Result<Complex64> {
	let pair = value.as_array().ok_or(Error::Format("matrix word pair"))?;
	let [re, im] = pair.as_slice() else {
		return Err(Error::Format("matrix word pair"));
	};
	Ok(Complex64::new(
		f64::from_bits(re.as_u64().ok_or(Error::Format("matrix real word"))?),
		f64::from_bits(im.as_u64().ok_or(Error::Format("matrix imaginary word"))?),
	))
}
fn read_controls(
	value: &Value,
	policy: IoPolicy,
	key: &str,
	convention: &str,
	component: impl Fn(&Value) -> Result<Complex64>,
) -> Result<ControlSequence> {
	if value.get("convention").and_then(Value::as_str) != Some(convention) {
		return Err(Error::Format("frozen controls require a convention tag"));
	}
	let raw = array(value, key)?;
	bounded(raw, policy)?;
	let mut matrices = Vec::new();
	matrices
		.try_reserve_exact(raw.len())
		.map_err(|_| Error::Budget("control matrices"))?;
	for matrix in raw {
		let rows = matrix
			.as_array()
			.ok_or(Error::Format("control matrix rows"))?;
		let [first, last] = rows.as_slice() else {
			return Err(Error::Format("control matrix is not 2 by 2"));
		};
		let row = |value: &Value| -> Result<[Complex64; 2]> {
			let cells = value
				.as_array()
				.ok_or(Error::Format("control matrix columns"))?;
			let [left, right] = cells.as_slice() else {
				return Err(Error::Format("control matrix is not 2 by 2"));
			};
			Ok([component(left)?, component(right)?])
		};
		matrices.push([row(first)?, row(last)?]);
	}
	Ok(ControlSequence::builder().matrices(matrices).build()?)
}
fn bounded(values: &[Value], policy: IoPolicy) -> Result<()> {
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
	let value: Value = serde_json::from_str(source)?;
	if value.get("payload").is_some() || value.get("sha256").is_some() {
		drop(value);
		return compiled_at_tolerance(source, policy, verification_tolerance);
	}
	if value.get("control_words").is_some() && value.get("controls").is_some() {
		return Err(Error::Format("competing frozen matrix representations"));
	}
	if (value.get("psi_words").is_some() || value.get("phi_words").is_some())
		&& (value.get("psi").is_some() || value.get("phi").is_some())
	{
		return Err(Error::Format("competing source angle representations"));
	}
	if execution
		&& (value.get("psi").is_some()
			|| value.get("phi").is_some()
			|| value.get("controls").is_some())
		&& value.get("control_words").is_none()
	{
		return Err(Error::Format(
			"execution controls require frozen matrix words",
		));
	}
	if execution
		&& (value.get("psi").is_some() || value.get("phi").is_some())
		&& value.get("psi_words").is_none()
	{
		return Err(Error::Format(
			"execution angle provenance requires exact words",
		));
	}
	if value.get("theta").is_some() || value.get("lambda").is_some() {
		return Err(Error::Format(
			"obsolete theta/lambda; use paper-native psi/phi",
		));
	}
	if value.get("angles").is_some() {
		let angles = array(&value, "angles")?;
		bounded(angles, policy)?;
		return match value.get("convention").and_then(Value::as_str) {
			Some("pyqsp-wx-symmetric") => Ok(QspInput::Symmetric(
				PhaseSequence::builder(reals(angles)?).build()?,
			)),
			Some("pyqsp-wx-laurent") => Ok(QspInput::Laurent(
				PhaseSequence::builder(reals(angles)?).build()?,
			)),
			_ => Err(Error::Format("missing or unsupported phase convention")),
		};
	}
	if value.get("control_words").is_some() {
		let controls = read_controls(
			&value,
			policy,
			"control_words",
			"gqsp-matrix-words-v1",
			complex_words,
		)?;
		return with_optional_angle_provenance(&value, policy, controls);
	}
	if value.get("controls").is_some() {
		let controls = read_controls(
			&value,
			policy,
			"controls",
			"gqsp-matrix-upper-left-v1",
			complex,
		)?;
		return with_optional_angle_provenance(&value, policy, controls);
	}
	if value.get("psi").is_some() || value.get("phi").is_some() {
		let psi = array(&value, "psi")?;
		let phi = array(&value, "phi")?;
		bounded(psi, policy)?;
		bounded(phi, policy)?;
		return Ok(QspInput::GeneralizedAngles(GeneralizedAngleInput::new(
			reals(psi)?,
			reals(phi)?,
		)?));
	}
	read_polynomial(value, policy).map(QspInput::Polynomial)
}
fn with_optional_angle_provenance(
	value: &Value,
	policy: IoPolicy,
	controls: ControlSequence,
) -> Result<QspInput> {
	if value.get("psi_words").is_some() || value.get("phi_words").is_some() {
		let psi = array(value, "psi_words")?;
		let phi = array(value, "phi_words")?;
		bounded(psi, policy)?;
		bounded(phi, policy)?;
		let words = |values: &[Value]| -> Result<Vec<f64>> {
			values
				.iter()
				.map(|value| {
					value
						.as_u64()
						.map(f64::from_bits)
						.ok_or(Error::Format("source angle word"))
				})
				.collect()
		};
		return Ok(QspInput::GeneralizedAngles(
			GeneralizedAngleInput::with_frozen_controls(words(psi)?, words(phi)?, controls)?,
		));
	}
	if value.get("psi").is_some() || value.get("phi").is_some() {
		let psi = array(value, "psi")?;
		let phi = array(value, "phi")?;
		bounded(psi, policy)?;
		bounded(phi, policy)?;
		return Ok(QspInput::GeneralizedAngles(
			GeneralizedAngleInput::with_frozen_controls(reals(psi)?, reals(phi)?, controls)?,
		));
	}
	Ok(QspInput::GeneralizedMatrices(controls))
}
fn read_polynomial(value: Value, policy: IoPolicy) -> Result<PolynomialInput> {
	let raw = array(&value, "coefficients")?;
	bounded(raw, policy)?;
	let mut coefficients = raw.iter().map(complex).collect::<Result<Vec<_>>>()?;
	let basis = value
		.get("basis")
		.and_then(Value::as_str)
		.unwrap_or("Chebyshev");
	let offset = value.get("minimum_order").map_or(Ok(0), |v| {
		v.as_i64()
			.and_then(|n| i32::try_from(n).ok())
			.ok_or(Error::Format("minimum_order requires signed32"))
	})?;
	if basis != "Laurent" && offset != 0 {
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
	let parameters = value.get("parameters").map_or(Ok(Vec::new()), |v| {
		v.as_array()
			.ok_or(Error::Format("parameters must be an array"))
			.and_then(|v| reals(v))
	})?;
	let limits = policy.polynomial_limits();
	let polynomial = match (basis, parameters.as_slice()) {
		("Monomial", []) => {
			AdmittedPolynomial::Monomial(Polynomial::new(Monomial, coefficients, limits)?)
		}
		("Chebyshev", []) => {
			AdmittedPolynomial::Chebyshev(Polynomial::new(Chebyshev, coefficients, limits)?)
		}
		("Laurent", []) => AdmittedPolynomial::Laurent(Polynomial::new(
			Laurent::new(offset),
			coefficients,
			limits,
		)?),
		("Hermite", []) => AdmittedPolynomial::Hermite(Polynomial::new(
			Hermite::physicists(),
			coefficients,
			limits,
		)?),
		("Laguerre", []) => AdmittedPolynomial::Laguerre(Polynomial::new(
			Laguerre::new(0.0)?,
			coefficients,
			limits,
		)?),
		("Laguerre", [alpha]) => AdmittedPolynomial::Laguerre(Polynomial::new(
			Laguerre::new(*alpha)?,
			coefficients,
			limits,
		)?),
		("Jacobi", []) => AdmittedPolynomial::Jacobi(Polynomial::new(
			Jacobi::new(0.0, 0.0)?,
			coefficients,
			limits,
		)?),
		("Jacobi", [alpha, beta]) => AdmittedPolynomial::Jacobi(Polynomial::new(
			Jacobi::new(*alpha, *beta)?,
			coefficients,
			limits,
		)?),
		_ => return Err(Error::Format("unsupported polynomial basis/parameters")),
	};
	Ok(PolynomialInput {
		polynomial,
		original: value,
	})
}

/// Export phases with original tags or frozen matrices without refactorization.
/// The matrix extension preserves actual binary64 values through JSON round trips.
///
/// # Errors
/// Returns JSON serialization errors.
pub fn write_qsp_json(input: &QspInput) -> Result<String> {
	let value = match input {
		#[cfg(feature = "certification")]
		QspInput::Compiled(p) => return Ok(p.json().to_owned()),
		QspInput::Symmetric(p) => json!({"convention":p.convention(),"angles":p.values()}),
		QspInput::Laurent(p) => json!({"convention":p.convention(),"angles":p.values()}),
		QspInput::GeneralizedAngles(angles) => json!({"psi":angles.psi,"phi":angles.phi}),
		QspInput::GeneralizedMatrices(p) => frozen_controls_json(p),
		QspInput::Polynomial(p) => p.original.clone(),
	};
	Ok(serde_json::to_string_pretty(&value)?)
}

fn frozen_controls_json(controls: &ControlSequence) -> Value {
	let matrices: Vec<_> = controls
		.matrices()
		.iter()
		.map(|matrix| matrix.map(|row| row.map(|v| [v.re, v.im])))
		.collect();
	json!({"convention":"gqsp-matrix-upper-left-v1","controls":matrices})
}

fn frozen_control_words_json(controls: &ControlSequence) -> Value {
	let words: Vec<_> = controls
		.matrices()
		.iter()
		.map(|matrix| matrix.map(|row| row.map(|v| [v.re.to_bits(), v.im.to_bits()])))
		.collect();
	json!({"convention":"gqsp-matrix-words-v1","control_words":words})
}

/// Serialize execution authority, preserving exact admitted matrix words and
/// original angle provenance when the source was an angle import.
///
/// # Errors
/// Returns JSON serialization errors.
pub fn write_qsp_execution_json(input: &QspInput) -> Result<String> {
	if let QspInput::GeneralizedAngles(angles) = input {
		let mut value = frozen_control_words_json(angles.controls());
		let object = value
			.as_object_mut()
			.ok_or(Error::Format("frozen control object"))?;
		object.insert(
			"psi_words".into(),
			json!(angles.psi().iter().map(|x| x.to_bits()).collect::<Vec<_>>()),
		);
		object.insert(
			"phi_words".into(),
			json!(angles.phi().iter().map(|x| x.to_bits()).collect::<Vec<_>>()),
		);
		return Ok(serde_json::to_string_pretty(&value)?);
	}
	if let QspInput::GeneralizedMatrices(controls) = input {
		return Ok(serde_json::to_string_pretty(&frozen_control_words_json(
			controls,
		))?);
	}
	write_qsp_json(input)
}
