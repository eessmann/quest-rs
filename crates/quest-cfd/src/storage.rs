//! Mechanical storage and normalization primitives; callers retain domain limits.
use crate::CfdError;
use quest_numerics::Complex64;

pub const fn bytes<T>(elements: usize) -> Option<usize> {
	elements.checked_mul(size_of::<T>())
}

pub fn reserved<T>(
	elements: usize,
	invalid: impl FnOnce() -> CfdError,
) -> Result<Vec<T>, CfdError> {
	let mut values = Vec::new();
	values.try_reserve_exact(elements).map_err(|_| invalid())?;
	Ok(values)
}

/// Preserve the represented sum order used by the configuration owners. The
/// caller supplies the scientific meaning of unresolved/zero sampled support.
#[expect(
	clippy::arithmetic_side_effects,
	reason = "The norm is checked finite and nonzero before division; amplitudes are bounded by that norm"
)]
pub fn normalize(
	values: &mut [Complex64],
	invalid: impl FnOnce() -> CfdError,
) -> Result<(), CfdError> {
	let norm = values.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	if !norm.is_finite() || norm == 0.0 {
		return Err(invalid());
	}
	for value in values {
		*value /= norm;
	}
	Ok(())
}
