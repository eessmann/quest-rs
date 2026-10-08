//! Shared finite binary64 decomposition; callers retain their own admission policy.
/// Checked binary64 decomposition. Fields are read-only outside `MathCore`.
///
/// ```compile_fail
/// let mut value = mathcore::dyadic::parts(1.0).unwrap();
/// value.mantissa = 0;
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parts {
	pub(crate) negative: bool,
	pub(crate) mantissa: u64,
	pub(crate) exponent: i32,
}
/// Decode a finite binary64 value without rounding; reject NaN and infinities.
#[must_use]
pub fn parts(value: f64) -> Option<Parts> {
	if !value.is_finite() {
		return None;
	}
	let bits = value.to_bits();
	let exponent = i32::try_from((bits >> 52) & 0x7ff).ok()?;
	let fraction = bits & ((1_u64 << 52) - 1);
	Some(Parts {
		negative: bits >> 63 != 0,
		mantissa: if exponent == 0 {
			fraction
		} else {
			fraction | (1_u64 << 52)
		},
		exponent: if exponent == 0 {
			-1074
		} else {
			exponent.checked_sub(1075)?
		},
	})
}
impl Parts {
	#[must_use]
	pub const fn negative(self) -> bool {
		self.negative
	}
	#[must_use]
	pub const fn mantissa(self) -> u64 {
		self.mantissa
	}
	#[must_use]
	pub const fn exponent(self) -> i32 {
		self.exponent
	}

	#[must_use]
	pub fn normalized(self) -> Self {
		if self.mantissa == 0 {
			return Self {
				exponent: 0,
				..self
			};
		}
		let zeros = self.mantissa.trailing_zeros();
		Self {
			mantissa: self.mantissa >> zeros,
			exponent: self
				.exponent
				.saturating_add(i32::try_from(zeros).unwrap_or(0)),
			..self
		}
	}
}
