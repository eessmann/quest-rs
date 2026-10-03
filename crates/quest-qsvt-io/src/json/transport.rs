//! JSON syntax transport. Domain admission lives in the parent module.
use crate::{Complex64, Error, Result};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{collections::BTreeMap, fmt};

/// Missing fields and explicit null are distinct: present values must decode as T.
pub(super) struct Field<T>(Option<T>);
impl<T> Default for Field<T> {
	fn default() -> Self {
		Self(None)
	}
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Field<T> {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
		T::deserialize(deserializer).map(|value| Self(Some(value)))
	}
}
impl<T> Field<T> {
	pub(super) const fn present(&self) -> bool {
		self.0.is_some()
	}
	pub(super) fn optional(self) -> Option<T> {
		self.0
	}
	pub(super) fn required(self, message: &'static str) -> Result<T> {
		self.0.ok_or(Error::Format(message))
	}
}

/// A coefficient/component may be a real scalar or exactly two real components.
/// A visitor gives a malformed pair no alternative decoding branch to fall through.
pub(super) struct Component(pub(super) Complex64);
impl<'de> Deserialize<'de> for Component {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
		struct Visitor;
		impl<'de> de::Visitor<'de> for Visitor {
			type Value = Component;
			fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
				formatter.write_str("a real number or a complex pair of two real numbers")
			}
			fn visit_f64<E: de::Error>(self, value: f64) -> std::result::Result<Component, E> {
				Ok(Component(Complex64::new(value, 0.0)))
			}
			fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<Component, E> {
				let value = serde_json::Number::from(value)
					.as_f64()
					.ok_or_else(|| E::custom("invalid real number"))?;
				self.visit_f64(value)
			}
			fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Component, E> {
				let value = serde_json::Number::from(value)
					.as_f64()
					.ok_or_else(|| E::custom("invalid real number"))?;
				self.visit_f64(value)
			}
			fn visit_seq<A: de::SeqAccess<'de>>(
				self,
				sequence: A,
			) -> std::result::Result<Component, A::Error> {
				let [re, im] =
					<[f64; 2]>::deserialize(de::value::SeqAccessDeserializer::new(sequence))?;
				Ok(Component(Complex64::new(re, im)))
			}
		}
		deserializer.deserialize_any(Visitor)
	}
}

#[derive(Clone, Copy, Default, Deserialize)]
pub(super) enum Basis {
	Monomial,
	#[default]
	Chebyshev,
	Laurent,
	Hermite,
	Laguerre,
	Jacobi,
}
pub(super) type Matrix<T> = [[T; 2]; 2];
pub(super) type WordMatrix = Matrix<[u64; 2]>;

/// All recognized fields are decoded once, with Serde's duplicate-field checks.
/// Unknown metadata uses ordinary JSON values so number and nesting admission
/// matches the original JSON decoder; polynomial output retains the full source.
#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct Envelope {
	pub(super) convention: Field<String>,
	pub(super) angles: Field<Vec<f64>>,
	pub(super) psi: Field<Vec<f64>>,
	pub(super) phi: Field<Vec<f64>>,
	pub(super) psi_words: Field<Vec<u64>>,
	pub(super) phi_words: Field<Vec<u64>>,
	pub(super) controls: Field<Vec<Matrix<Component>>>,
	pub(super) control_words: Field<Vec<WordMatrix>>,
	pub(super) coefficients: Field<Vec<Component>>,
	pub(super) basis: Field<Basis>,
	pub(super) minimum_order: Field<i32>,
	pub(super) parameters: Field<Vec<f64>>,
	pub(super) theta: Field<de::IgnoredAny>,
	pub(super) lambda: Field<de::IgnoredAny>,
	pub(super) payload: Field<de::IgnoredAny>,
	pub(super) sha256: Field<de::IgnoredAny>,
	#[serde(flatten)]
	pub(super) metadata: BTreeMap<String, serde_json::Value>,
}
pub(super) enum Kind {
	Compiled,
	Phases,
	Matrices,
	Angles,
	Polynomial,
}
impl Envelope {
	pub(super) const fn kind(&self, execution: bool) -> Result<Kind> {
		if self.theta.present() || self.lambda.present() {
			return Err(Error::Format(
				"obsolete theta/lambda; use paper-native psi/phi",
			));
		}
		let phases = self.angles.present();
		let matrices = self.controls.present() || self.control_words.present();
		let angles = self.psi.present() || self.phi.present();
		let words = self.psi_words.present() || self.phi_words.present();
		let polynomial = self.coefficients.present()
			|| self.basis.present()
			|| self.minimum_order.present()
			|| self.parameters.present();
		let compiled = self.payload.present() || self.sha256.present();
		if self.controls.present() && self.control_words.present() {
			return Err(Error::Format("competing frozen matrix representations"));
		}
		if angles && words {
			return Err(Error::Format("competing source angle representations"));
		}
		if (compiled
			&& (phases || matrices || angles || words || polynomial || self.convention.present()))
			|| (phases && (matrices || angles || words || polynomial))
			|| (polynomial && (matrices || angles || words))
		{
			return Err(Error::Format("competing QSP payload families"));
		}
		if compiled {
			return Ok(Kind::Compiled);
		}
		if phases {
			return Ok(Kind::Phases);
		}
		if words && !matrices {
			return Err(Error::Format("source angle words require frozen controls"));
		}
		if execution && (matrices || angles) && !self.control_words.present() {
			return Err(Error::Format(
				"execution controls require frozen matrix words",
			));
		}
		if execution && angles {
			return Err(Error::Format(
				"execution angle provenance requires exact words",
			));
		}
		if matrices {
			Ok(Kind::Matrices)
		} else if angles {
			Ok(Kind::Angles)
		} else {
			Ok(Kind::Polynomial)
		}
	}
}

#[derive(Serialize)]
pub(super) struct Phases<'a> {
	pub(super) convention: &'a str,
	pub(super) angles: &'a [f64],
}
#[derive(Serialize)]
pub(super) struct SourceAngles<'a> {
	pub(super) psi: &'a [f64],
	pub(super) phi: &'a [f64],
}
#[derive(Serialize)]
pub(super) struct Controls {
	pub(super) convention: &'static str,
	pub(super) controls: Vec<Matrix<[f64; 2]>>,
}
#[derive(Serialize)]
pub(super) struct ExecutionControls {
	pub(super) convention: &'static str,
	pub(super) control_words: Vec<WordMatrix>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(super) psi_words: Option<Vec<u64>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(super) phi_words: Option<Vec<u64>>,
}
