//! Typed captures keep exact mathematical angles separate from QASM floating values.
use crate::{Angle, LanguageError};
use quest_language::classical::{FloatWidth, ScalarValue};
use std::collections::BTreeMap;
mod sealed {
	pub trait Sealed {}
}
/// Accepted Rust capture types. Ordinary source floats never acquire exact proofs.
pub trait AngleCapture: sealed::Sealed {
	/// # Errors
	/// Rejects nonfinite values, missing exact bindings, and conversion limits.
	fn into_capture(self) -> Result<CapturedAngle, LanguageError>;
}
#[derive(Debug, Clone)]
pub struct CapturedAngle {
	scalar: ScalarValue,
	exact: Option<Angle>,
}
impl CapturedAngle {
	#[must_use]
	pub const fn scalar(&self) -> ScalarValue {
		self.scalar
	}
	#[must_use]
	pub const fn exact(&self) -> Option<&Angle> {
		self.exact.as_ref()
	}
}
impl sealed::Sealed for f64 {}
impl AngleCapture for f64 {
	fn into_capture(self) -> Result<CapturedAngle, LanguageError> {
		Ok(CapturedAngle {
			scalar: ScalarValue::floating(FloatWidth::F64, self)?,
			exact: None,
		})
	}
}
impl sealed::Sealed for Angle {}
impl AngleCapture for Angle {
	fn into_capture(self) -> Result<CapturedAngle, LanguageError> {
		let value = self.evaluate(&BTreeMap::new())?;
		Ok(CapturedAngle {
			scalar: ScalarValue::floating(FloatWidth::F64, value)?,
			exact: Some(self),
		})
	}
}
/// Evaluate this Rust value once and preserve its explicit exact type.
/// # Errors
/// Rejects invalid captures and source conversion obligations.
pub fn capture_angle(value: impl AngleCapture) -> Result<CapturedAngle, LanguageError> {
	value.into_capture()
}
