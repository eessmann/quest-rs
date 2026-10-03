//! Checked scalar storage and the simulator profile's classical arithmetic.
mod operations;
mod typing;
use crate::syntax::{BinaryOperator, UnaryOperator};
use num_traits::ToPrimitive;

/// An admitted fixed width, including the 64-bit boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8"))]
pub struct Width(u8);
impl TryFrom<u8> for Width {
	type Error = ValueError;
	fn try_from(value: u8) -> Result<Self, Self::Error> {
		Self::new(value)
	}
}
impl Width {
	/// # Errors
	/// Rejects widths outside 1 through 64.
	pub const fn new(width: u8) -> Result<Self, ValueError> {
		if width == 0 || width > 64 {
			Err(ValueError::Width(width))
		} else {
			Ok(Self(width))
		}
	}
	#[must_use]
	pub const fn value(self) -> u8 {
		self.0
	}
	#[must_use]
	pub fn mask(self) -> u64 {
		u64::MAX
			.checked_shr(64u32.saturating_sub(u32::from(self.0)))
			.unwrap_or(0)
	}
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FloatWidth {
	F32,
	F64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ScalarType {
	Bool,
	Bit(Width),
	Int(Width),
	Uint(Width),
	Angle(Width),
	Float(FloatWidth),
}
impl ScalarType {
	#[must_use]
	pub const fn width(self) -> Option<Width> {
		match self {
			Self::Bit(w) | Self::Int(w) | Self::Uint(w) | Self::Angle(w) => Some(w),
			Self::Bool | Self::Float(_) => None,
		}
	}
	#[must_use]
	pub const fn storage_bytes(self) -> usize {
		match self {
			Self::Bool => 1,
			Self::Float(FloatWidth::F32) => 4,
			_ => 8,
		}
	}
}
/// Scalar payloads cannot contain an invalid bit width or nonfinite float.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "SerializedScalar"))]
pub struct ScalarValue {
	ty: ScalarType,
	data: Data,
}
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum Data {
	Bool(bool),
	Bits(u64),
	Float(f64),
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValueError {
	#[error("unsupported scalar width {0}; expected 1..=64")]
	Width(u8),
	#[error("scalar operation requires compatible types")]
	Type,
	#[error("scalar arithmetic overflow")]
	Overflow,
	#[error("division by zero")]
	DivisionByZero,
	#[error("nonfinite floating value")]
	NonFinite,
	#[error("invalid numeric literal {0}")]
	Literal(String),
	#[error("index {index} is outside a collection of length {length}")]
	Index { index: i128, length: usize },
	#[error("unsupported classical function {0}")]
	Function(String),
}
impl ScalarValue {
	#[must_use]
	pub const fn ty(&self) -> ScalarType {
		self.ty
	}
	#[must_use]
	pub const fn boolean(value: bool) -> Self {
		Self {
			ty: ScalarType::Bool,
			data: Data::Bool(value),
		}
	}
	/// # Errors
	/// Rejects an empty, oversized, or malformed bit string.
	pub fn bitstring(value: &str) -> Result<Self, ValueError> {
		let clean = value.replace('_', "");
		let width = Width::new(u8::try_from(clean.len()).map_err(|_| ValueError::Overflow)?)?;
		let bits = u64::from_str_radix(&clean, 2).map_err(|_| ValueError::Literal(value.into()))?;
		Ok(Self {
			ty: ScalarType::Bit(width),
			data: Data::Bits(bits),
		})
	}
	/// # Errors
	/// Rejects values outside the signed width's two's-complement range.
	pub fn signed(width: Width, value: i128) -> Result<Self, ValueError> {
		let magnitude = 1i128
			.checked_shl(u32::from(width.value()).saturating_sub(1))
			.ok_or(ValueError::Overflow)?;
		let minimum = magnitude.checked_neg().ok_or(ValueError::Overflow)?;
		if value < minimum || value >= magnitude {
			return Err(ValueError::Overflow);
		}
		let bits = if value < 0 {
			u64::try_from(
				value
					.checked_add(
						1i128
							.checked_shl(u32::from(width.value()))
							.ok_or(ValueError::Overflow)?,
					)
					.ok_or(ValueError::Overflow)?,
			)
			.map_err(|_| ValueError::Overflow)?
		} else {
			u64::try_from(value).map_err(|_| ValueError::Overflow)?
		};
		Ok(Self {
			ty: ScalarType::Int(width),
			data: Data::Bits(bits),
		})
	}
	/// # Errors
	/// Rejects values outside the requested unsigned width.
	pub fn unsigned(width: Width, value: u64) -> Result<Self, ValueError> {
		if value > width.mask() {
			return Err(ValueError::Overflow);
		}
		Ok(Self {
			ty: ScalarType::Uint(width),
			data: Data::Bits(value),
		})
	}
	/// # Errors
	/// Rejects bits outside the admitted angle width.
	pub fn angle_bits(width: Width, value: u64) -> Result<Self, ValueError> {
		if value > width.mask() {
			return Err(ValueError::Overflow);
		}
		Ok(Self {
			ty: ScalarType::Angle(width),
			data: Data::Bits(value),
		})
	}
	/// # Errors
	/// Rejects NaN, infinity, and float32 conversion overflow.
	pub fn floating(width: FloatWidth, value: f64) -> Result<Self, ValueError> {
		let value = if width == FloatWidth::F32 {
			f64::from(value.to_f32().ok_or(ValueError::Overflow)?)
		} else {
			value
		};
		if !value.is_finite() {
			return Err(ValueError::NonFinite);
		}
		Ok(Self {
			ty: ScalarType::Float(width),
			data: Data::Float(value),
		})
	}
	/// Parse a numeric token; no exact angle semantics are inferred.
	///
	/// # Errors
	/// Rejects malformed literals and values outside the 64-bit profile.
	pub fn parse_number(token: &str) -> Result<Self, ValueError> {
		let clean = token.replace('_', "");
		if clean.contains(['.', 'e', 'E']) && !clean.starts_with("0x") && !clean.starts_with("0X") {
			return Self::floating(
				FloatWidth::F64,
				clean
					.parse::<f64>()
					.map_err(|_| ValueError::Literal(token.into()))?,
			);
		}
		let radix = [
			("0x", 16),
			("0X", 16),
			("0b", 2),
			("0B", 2),
			("0o", 8),
			("0O", 8),
		]
		.iter()
		.find_map(|(prefix, radix)| clean.strip_prefix(prefix).map(|digits| (*radix, digits)));
		let value = radix
			.map_or_else(
				|| clean.parse::<i128>(),
				|(radix, digits)| i128::from_str_radix(digits, radix),
			)
			.map_err(|_| ValueError::Literal(token.into()))?;
		Self::signed(Width::new(64)?, value)
	}
	/// # Errors
	/// Requires a bit, integer or angle payload.
	pub const fn raw_bits(&self) -> Result<u64, ValueError> {
		if let Data::Bits(bits) = self.data {
			Ok(bits)
		} else {
			Err(ValueError::Type)
		}
	}
	/// # Errors
	/// Requires a Boolean, rather than silently applying a truthiness cast.
	pub const fn to_bool(&self) -> Result<bool, ValueError> {
		if let Data::Bool(value) = self.data {
			Ok(value)
		} else {
			Err(ValueError::Type)
		}
	}
	/// # Errors
	/// Requires an integral value. Angles are not integers.
	pub fn to_i128(&self) -> Result<i128, ValueError> {
		match (self.ty, self.data) {
			(ScalarType::Int(width), Data::Bits(bits)) => {
				let sign = 1u64
					.checked_shl(u32::from(width.value()).saturating_sub(1))
					.ok_or(ValueError::Overflow)?;
				if bits & sign == 0 {
					Ok(i128::from(bits))
				} else {
					i128::from(bits)
						.checked_sub(
							1i128
								.checked_shl(u32::from(width.value()))
								.ok_or(ValueError::Overflow)?,
						)
						.ok_or(ValueError::Overflow)
				}
			}
			(ScalarType::Uint(_) | ScalarType::Bit(_), Data::Bits(bits)) => Ok(i128::from(bits)),
			(ScalarType::Bool, Data::Bool(value)) => Ok(i128::from(value)),
			_ => Err(ValueError::Type),
		}
	}
	/// # Errors
	/// Requires a finite real scalar; integer conversion follows IEEE rounding.
	pub fn to_f64(&self) -> Result<f64, ValueError> {
		match (self.ty, self.data) {
			(ScalarType::Float(_), Data::Float(value)) => Ok(value),
			(ScalarType::Angle(width), Data::Bits(bits)) => Ok(bits
				.to_f64()
				.ok_or(ValueError::Overflow)?
				* std::f64::consts::TAU
				/ 2f64.powi(i32::from(width.value()))),
			_ => self.to_i128()?.to_f64().ok_or(ValueError::Overflow),
		}
	}
	/// Checked indexing, including the profile's negative array indices.
	///
	/// # Errors
	/// Rejects a non-integral value or an index outside the collection.
	pub fn to_index(&self, length: usize) -> Result<usize, ValueError> {
		let index = self.to_i128()?;
		let size = i128::try_from(length).map_err(|_| ValueError::Overflow)?;
		let adjusted = if index < 0 {
			index.checked_add(size).ok_or(ValueError::Overflow)?
		} else {
			index
		};
		if adjusted < 0 || adjusted >= size {
			return Err(ValueError::Index { index, length });
		}
		usize::try_from(adjusted).map_err(|_| ValueError::Overflow)
	}
	/// Explicit casts preserve bit identities and check undefined conversions.
	///
	/// # Errors
	/// Rejects unsupported casts, nonfinite values and out-of-range float casts.
	pub fn cast(&self, target: ScalarType) -> Result<Self, ValueError> {
		if !self.ty.can_explicitly_cast_to(target) {
			return Err(ValueError::Type);
		}
		if target == self.ty {
			return Ok(*self);
		}
		match target {
			ScalarType::Bool => Ok(Self::boolean(match self.data {
				Data::Bool(value) => value,
				Data::Bits(bits) => bits != 0,
				Data::Float(value) => value != 0.0,
			})),
			ScalarType::Float(width) => Self::floating(width, self.to_f64()?),
			ScalarType::Angle(width) => self.cast_angle(width),
			ScalarType::Bit(width) => {
				let bits = match self.ty {
					ScalarType::Bool if width.value() == 1 => u64::from(self.to_bool()?),
					ScalarType::Bit(w)
					| ScalarType::Int(w)
					| ScalarType::Uint(w)
					| ScalarType::Angle(w)
						if w == width =>
					{
						self.raw_bits()?
					}
					_ => return Err(ValueError::Type),
				};
				Ok(Self {
					ty: target,
					data: Data::Bits(bits),
				})
			}
			ScalarType::Int(width) | ScalarType::Uint(width) => self.cast_integer(target, width),
		}
	}
	fn cast_integer(&self, target: ScalarType, width: Width) -> Result<Self, ValueError> {
		let bits = match (self.ty, self.data) {
			(ScalarType::Angle(_), _) => return Err(ValueError::Type),
			(ScalarType::Bit(source), _) if source != width => return Err(ValueError::Type),
			(_, Data::Float(value)) => {
				let integer = value.to_i128().ok_or(ValueError::Overflow)?;
				return if matches!(target, ScalarType::Int(_)) {
					Self::signed(width, integer)
				} else {
					Self::unsigned(
						width,
						u64::try_from(integer).map_err(|_| ValueError::Overflow)?,
					)
				};
			}
			(_, Data::Bool(value)) => u64::from(value),
			(ScalarType::Int(_), Data::Bits(_)) => {
				let integer = self.to_i128()?;
				let modulus = 1i128
					.checked_shl(u32::from(width.value()))
					.ok_or(ValueError::Overflow)?;
				u64::try_from(integer.rem_euclid(modulus)).map_err(|_| ValueError::Overflow)?
			}
			(_, Data::Bits(bits)) => bits & width.mask(),
		};
		Ok(Self {
			ty: target,
			data: Data::Bits(bits),
		})
	}
	fn cast_angle(&self, width: Width) -> Result<Self, ValueError> {
		let bits = match (self.ty, self.data) {
			(ScalarType::Angle(source), Data::Bits(bits)) => {
				if source.value() > width.value() {
					bits.checked_shr(u32::from(source.value().saturating_sub(width.value())))
						.ok_or(ValueError::Overflow)?
				} else {
					bits.checked_shl(u32::from(width.value().saturating_sub(source.value())))
						.ok_or(ValueError::Overflow)?
				}
			}
			(ScalarType::Bit(source), Data::Bits(bits)) if source == width => bits,
			(ScalarType::Float(float_width), Data::Float(value)) => {
				let tau = if float_width == FloatWidth::F32 {
					f64::from(std::f32::consts::TAU)
				} else {
					std::f64::consts::TAU
				};
				let scale = 2f64.powi(i32::from(width.value()));
				let fraction = value.rem_euclid(tau) / tau;
				let fraction = if float_width == FloatWidth::F32 {
					f64::from(fraction.to_f32().ok_or(ValueError::Overflow)?)
				} else {
					fraction
				};
				let rounded = (fraction * scale).round_ties_even();
				if rounded >= scale {
					0
				} else {
					rounded.to_u64().ok_or(ValueError::Overflow)?
				}
			}
			_ => return Err(ValueError::Type),
		};
		Self::angle_bits(width, bits)
	}
	/// # Errors
	/// Rejects an unsupported operator/type pairing or arithmetic failure.
	pub fn unary(&self, operator: UnaryOperator) -> Result<Self, ValueError> {
		self.ty.unary_result(operator)?;
		operations::unary(*self, operator)
	}
	/// # Errors
	/// Rejects incompatible types, zero divisors and signed overflow.
	pub fn binary(&self, operator: BinaryOperator, rhs: &Self) -> Result<Self, ValueError> {
		self.ty.binary_result(operator, rhs.ty)?;
		operations::binary(*self, operator, *rhs)
	}
	/// # Errors
	/// Rejects an unknown function, wrong arity, domain errors and nonfinite output.
	pub fn function(name: &str, arguments: &[Self]) -> Result<Self, ValueError> {
		operations::function(name, arguments)
	}
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
struct SerializedScalar {
	ty: ScalarType,
	data: Data,
}
#[cfg(feature = "serde")]
impl TryFrom<SerializedScalar> for ScalarValue {
	type Error = ValueError;
	fn try_from(raw: SerializedScalar) -> Result<Self, Self::Error> {
		match (raw.ty, raw.data) {
			(ScalarType::Bool, Data::Bool(value)) => Ok(Self::boolean(value)),
			(ScalarType::Float(width), Data::Float(value)) => {
				let checked = Self::floating(width, value)?;
				if checked.data != raw.data {
					return Err(ValueError::Type);
				}
				Ok(checked)
			}
			(
				ScalarType::Bit(width)
				| ScalarType::Int(width)
				| ScalarType::Uint(width)
				| ScalarType::Angle(width),
				Data::Bits(value),
			) if value <= width.mask() => Ok(Self {
				ty: raw.ty,
				data: raw.data,
			}),
			_ => Err(ValueError::Type),
		}
	}
}
