//! Shared native multiprecision primitives, independent of admission and certificate policy.
//!
//! A result is retagged to the common binary representation without a second
//! rounding step. Callers retain responsibility for precision, input admission,
//! resource accounting, and interpretation of native arithmetic failures.
use super::Binary;
use dashu_float::{
	ConstCache, Context, FpError,
	round::{ErrorBounds, Round, mode::HalfEven},
};

/// Native add at the caller's precision and rounding direction.
/// # Errors
/// Returns the native domain, exponent-range, or nonfinite-input failure.
pub fn add<R: Round>(context: Context<R>, a: &Binary, b: &Binary) -> Result<Binary, FpError> {
	Ok(context
		.add(a.repr(), b.repr())?
		.value()
		.with_rounding::<HalfEven>())
}
/// Native sub at the caller's precision and rounding direction.
/// # Errors
/// Returns the native domain, exponent-range, or nonfinite-input failure.
pub fn sub<R: Round>(context: Context<R>, a: &Binary, b: &Binary) -> Result<Binary, FpError> {
	Ok(context
		.sub(a.repr(), b.repr())?
		.value()
		.with_rounding::<HalfEven>())
}
/// Native mul at the caller's precision and rounding direction.
/// # Errors
/// Returns the native domain, exponent-range, or nonfinite-input failure.
pub fn mul<R: Round>(context: Context<R>, a: &Binary, b: &Binary) -> Result<Binary, FpError> {
	Ok(context
		.mul(a.repr(), b.repr())?
		.value()
		.with_rounding::<HalfEven>())
}
/// Native div at the caller's precision and rounding direction.
/// # Errors
/// Returns the native domain, exponent-range, or nonfinite-input failure.
pub fn div<R: Round>(context: Context<R>, a: &Binary, b: &Binary) -> Result<Binary, FpError> {
	Ok(context
		.div(a.repr(), b.repr())?
		.value()
		.with_rounding::<HalfEven>())
}
/// Native sqrt at the caller's precision and rounding direction.
/// # Errors
/// Returns the native domain, exponent-range, nonfinite-input, or rounding failure.
pub fn sqrt<R: Round + ErrorBounds>(context: Context<R>, a: &Binary) -> Result<Binary, FpError> {
	Ok(context.sqrt(a.repr())?.value().with_rounding::<HalfEven>())
}
/// Native exp at the caller's precision and rounding direction.
/// # Errors
/// Returns the native domain, exponent-range, nonfinite-input, or rounding failure.
pub fn exp<R: Round + ErrorBounds>(
	context: Context<R>,
	a: &Binary,
	cache: &mut ConstCache,
) -> Result<Binary, FpError> {
	Ok(context
		.exp(a.repr(), Some(cache))?
		.value()
		.with_rounding::<HalfEven>())
}
/// Native ln at the caller's precision and rounding direction.
/// # Errors
/// Returns the native domain, exponent-range, nonfinite-input, or rounding failure.
pub fn ln<R: Round + ErrorBounds>(
	context: Context<R>,
	a: &Binary,
	cache: &mut ConstCache,
) -> Result<Binary, FpError> {
	Ok(context
		.ln(a.repr(), Some(cache))?
		.value()
		.with_rounding::<HalfEven>())
}
/// Native sin at the caller's precision and rounding direction.
/// # Errors
/// Returns the native domain, exponent-range, nonfinite-input, or rounding failure.
pub fn sin<R: Round + ErrorBounds>(
	context: Context<R>,
	a: &Binary,
	cache: &mut ConstCache,
) -> Result<Binary, FpError> {
	Ok(context
		.sin(a.repr(), Some(cache))?
		.value()
		.with_rounding::<HalfEven>())
}
/// Native cos at the caller's precision and rounding direction.
/// # Errors
/// Returns the native domain, exponent-range, nonfinite-input, or rounding failure.
pub fn cos<R: Round + ErrorBounds>(
	context: Context<R>,
	a: &Binary,
	cache: &mut ConstCache,
) -> Result<Binary, FpError> {
	Ok(context
		.cos(a.repr(), Some(cache))?
		.value()
		.with_rounding::<HalfEven>())
}
/// Native atan at the caller's precision and rounding direction.
/// # Errors
/// Returns the native domain, exponent-range, nonfinite-input, or rounding failure.
pub fn atan<R: Round + ErrorBounds>(
	context: Context<R>,
	a: &Binary,
	cache: &mut ConstCache,
) -> Result<Binary, FpError> {
	Ok(context
		.atan(a.repr(), Some(cache))?
		.value()
		.with_rounding::<HalfEven>())
}
/// Paired correctly rounded sine and cosine without rerounding either result.
/// # Errors
/// Returns the native domain, exponent-range, nonfinite-input, or rounding failure.
pub fn sin_cos<R: Round + ErrorBounds>(
	context: Context<R>,
	a: &Binary,
	cache: &mut ConstCache,
) -> Result<(Binary, Binary), FpError> {
	let (sin, cos) = context.sin_cos(a.repr(), Some(cache));
	Ok((
		sin?.value().with_rounding::<HalfEven>(),
		cos?.value().with_rounding::<HalfEven>(),
	))
}
/// Native pi at the caller's precision and rounding direction.
pub fn pi<R: Round + ErrorBounds>(context: Context<R>, cache: &mut ConstCache) -> Binary {
	context
		.pi::<2>(Some(cache))
		.value()
		.with_rounding::<HalfEven>()
}
