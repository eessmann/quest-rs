//! Shared finite binary64 decomposition; callers retain their own admission policy.
#[derive(Clone, Copy)]
pub struct Parts {
	pub negative: bool,
	pub mantissa: u64,
	pub exponent: i32,
}
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
