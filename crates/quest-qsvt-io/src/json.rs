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
    GeneralizedAngles {
        psi: Vec<f64>,
        phi: Vec<f64>,
        controls: ControlSequence,
    },
    GeneralizedMatrices(ControlSequence),
    Polynomial(PolynomialInput),
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
impl PolynomialInput {
    /// Convert while retaining outward coefficient conversion error evidence.
    ///
    /// # Errors
    /// Rejects unsupported support, numerical failures or conversion budgets.
    pub fn to_chebyshev(&self) -> Result<Conversion<Chebyshev>> {
        Ok(match &self.polynomial {
            AdmittedPolynomial::Chebyshev(p) => Conversion {
                polynomial: p.clone(),
                coefficient_error_bound: 0.0,
            },
            AdmittedPolynomial::Monomial(p) => p.to_basis(Chebyshev)?,
            AdmittedPolynomial::Laurent(p) => p.to_basis(Chebyshev)?,
            AdmittedPolynomial::Hermite(p) => p.to_basis(Chebyshev)?,
            AdmittedPolynomial::Laguerre(p) => p.to_basis(Chebyshev)?,
            AdmittedPolynomial::Jacobi(p) => p.to_basis(Chebyshev)?,
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
    if source.len() > policy.max_bytes {
        return Err(Error::Budget("JSON source"));
    }
    let value: Value = serde_json::from_str(source)?;
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
    if value.get("controls").is_some() {
        if value.get("convention").and_then(Value::as_str) != Some("gqsp-matrix-upper-left-v1") {
            return Err(Error::Format("frozen matrices require a convention tag"));
        }
        let raw = array(&value, "controls")?;
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
                let raw = value
                    .as_array()
                    .ok_or(Error::Format("control matrix columns"))?;
                let [left, right] = raw.as_slice() else {
                    return Err(Error::Format("control matrix is not 2 by 2"));
                };
                Ok([complex(left)?, complex(right)?])
            };
            matrices.push([row(first)?, row(last)?]);
        }
        return Ok(QspInput::GeneralizedMatrices(
            ControlSequence::builder().matrices(matrices).build()?,
        ));
    }
    if value.get("psi").is_some() || value.get("phi").is_some() {
        let psi = array(&value, "psi")?;
        let phi = array(&value, "phi")?;
        bounded(psi, policy)?;
        bounded(phi, policy)?;
        let psi = reals(psi)?;
        let phi = reals(phi)?;
        let controls = ControlSequence::builder().angles(&psi, &phi)?.build()?;
        return Ok(QspInput::GeneralizedAngles { psi, phi, controls });
    }
    read_polynomial(value, policy).map(QspInput::Polynomial)
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
        QspInput::Symmetric(p) => json!({"convention":p.convention(),"angles":p.values()}),
        QspInput::Laurent(p) => json!({"convention":p.convention(),"angles":p.values()}),
        QspInput::GeneralizedAngles { psi, phi, .. } => json!({"psi":psi,"phi":phi}),
        QspInput::GeneralizedMatrices(p) => {
            let matrices: Vec<_> = p
                .matrices()
                .iter()
                .map(|matrix| matrix.map(|row| row.map(|v| [v.re, v.im])))
                .collect();
            json!({"convention":"gqsp-matrix-upper-left-v1","controls":matrices})
        }
        QspInput::Polynomial(p) => p.original.clone(),
    };
    Ok(serde_json::to_string_pretty(&value)?)
}
