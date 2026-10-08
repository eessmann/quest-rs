use super::{Context, OfflineError as Error, OfflineResult as Result, number::Number};
use crate::precision::{checked, integer, nearest_pi, nearest_sin_cos};
use std::sync::Arc;
fn roots(length: usize, context: &mut Context) -> Result<Arc<Vec<Number>>> {
	if let Some(values) = context.roots.get(&length) {
		return Ok(Arc::clone(values));
	}
	context.charge(
		length
			.checked_mul(32)
			.ok_or(Error::Budget("offline twiddles"))?,
	)?;
	let mut values = Vec::with_capacity(length / 2);
	// The one-point transform has no twiddles and previously allocated no constant cache.
	if length == 1 {
		let values = Arc::new(values);
		context.roots.insert(length, Arc::clone(&values));
		return Ok(values);
	}
	let p = context.precision;
	let pi = checked(nearest_pi(p, &mut context.cache)?)?;
	for index in 0..length / 2 {
		let numerator = index
			.checked_mul(2)
			.ok_or(Error::Budget("offline FFT angle"))?;
		let rational = super::number::div(
			&integer(
				p,
				u64::try_from(numerator)
					.map_err(|_| super::OfflineError::Budget("integer interchange"))?,
			),
			&integer(
				p,
				u64::try_from(length)
					.map_err(|_| super::OfflineError::Budget("integer interchange"))?,
			),
		)?;
		let angle = checked(super::number::mul(&rational, &pi)?)?;
		let (sin, cos) = nearest_sin_cos(p, &angle, &mut context.cache)?;
		values.push(Number {
			re: cos,
			im: super::number::neg(&sin),
		});
	}
	let values = Arc::new(values);
	context.roots.insert(length, Arc::clone(&values));
	Ok(values)
}
pub(super) fn transform(
	values: &mut [Number],
	inverse: bool,
	normalize: bool,
	context: &mut Context,
) -> Result<()> {
	let length = values.len();
	if !length.is_power_of_two() {
		return Err(Error::Numerical("non-power-of-two offline FFT"));
	}
	let stages =
		usize::try_from(length.ilog2().max(1)).map_err(|_| Error::Budget("offline FFT stages"))?;
	context.charge(
		length
			.checked_mul(stages)
			.and_then(|n| n.checked_mul(32))
			.ok_or(Error::Budget("offline FFT work"))?,
	)?;
	let roots = roots(length, context)?;
	let mut reversed = 0_usize;
	for index in 1..length {
		let mut bit = length >> 1;
		while reversed & bit != 0 {
			reversed ^= bit;
			bit >>= 1;
		}
		reversed ^= bit;
		if index < reversed {
			values.swap(index, reversed);
		}
	}
	let mut stage = 2_usize;
	while stage <= length {
		let stride = length
			.checked_div(stage)
			.ok_or(Error::Numerical("offline FFT stage"))?;
		for block in values.chunks_exact_mut(stage) {
			let (first, second) = block.split_at_mut(stage / 2);
			for (index, (a, b)) in first.iter_mut().zip(second).enumerate() {
				let root = roots
					.get(
						index
							.checked_mul(stride)
							.ok_or(Error::Budget("offline twiddle index"))?,
					)
					.ok_or(Error::Numerical("offline FFT twiddle"))?;
				let value = b.mul(&if inverse { root.conj() } else { root.clone() })?;
				let original = a.clone();
				*a = original.add(&value)?;
				*b = original.sub(&value)?;
			}
		}
		if stage == length {
			break;
		}
		stage = stage
			.checked_mul(2)
			.ok_or(Error::Budget("offline FFT stage"))?;
	}
	if normalize {
		let inverse_length = super::number::div(
			&integer(context.precision, 1),
			&integer(
				context.precision,
				u64::try_from(length)
					.map_err(|_| super::OfflineError::Budget("integer interchange"))?,
			),
		)?;
		for value in values.iter_mut() {
			*value = value.scale(&inverse_length)?;
		}
	}
	for value in values {
		value.validate()?;
	}
	Ok(())
}
pub(super) fn convolve(
	left: &[Number],
	right: &[Number],
	context: &mut Context,
) -> Result<Vec<Number>> {
	let count = left
		.len()
		.checked_add(right.len())
		.and_then(|n| n.checked_sub(1))
		.ok_or(Error::Budget("offline convolution"))?;
	if left.iter().all(Number::is_zero) || right.iter().all(Number::is_zero) {
		return Ok(vec![Number::zero(context.precision); count]);
	}
	if count <= 16 {
		context.charge(
			left.len()
				.checked_mul(right.len())
				.and_then(|n| n.checked_mul(16))
				.ok_or(Error::Budget("offline direct convolution"))?,
		)?;
		let mut result = vec![Number::zero(context.precision); count];
		for (i, a) in left.iter().enumerate() {
			for (j, b) in right.iter().enumerate() {
				let out = result
					.get_mut(
						i.checked_add(j)
							.ok_or(Error::Budget("offline convolution index"))?,
					)
					.ok_or(Error::Numerical("offline support"))?;
				*out = out.add(&a.mul(b)?)?;
			}
		}
		for value in &result {
			value.validate()?;
		}
		return Ok(result);
	}
	let length = count
		.checked_next_power_of_two()
		.ok_or(Error::Budget("offline FFT support"))?;
	context.admit_grid(length)?;
	let mut a = vec![Number::zero(context.precision); length];
	let mut b = a.clone();
	for (out, value) in a.iter_mut().zip(left) {
		out.clone_from(value);
	}
	for (out, value) in b.iter_mut().zip(right) {
		out.clone_from(value);
	}
	transform(&mut a, false, false, context)?;
	transform(&mut b, false, false, context)?;
	for (a, b) in a.iter_mut().zip(b) {
		*a = a.mul(&b)?;
	}
	transform(&mut a, true, true, context)?;
	a.truncate(count);
	Ok(a)
}

/// Group-scoped equivalent of the binary64 shared convolution session.
/// Buffers never survive the group or a recursive descent. The existing
/// 32*grid scalar allowance covers three complex arrays, retained twiddles,
/// and arithmetic scratch alongside the separately modeled recursive payload.
pub(super) struct SharedConvolutionSession<'inputs> {
	inputs: [&'inputs [Number]; 2],
	left: Vec<Number>,
	right: [Vec<Number>; 2],
	ready: [Option<usize>; 2],
}
impl<'inputs> SharedConvolutionSession<'inputs> {
	pub(super) const fn new(inputs: [&'inputs [Number]; 2]) -> Self {
		Self {
			inputs,
			left: Vec::new(),
			right: [Vec::new(), Vec::new()],
			ready: [None; 2],
		}
	}
	pub(super) fn product(
		&mut self,
		left: &[Number],
		index: usize,
		context: &mut Context,
	) -> Result<&[Number]> {
		match self.product_inner(left, index, context) {
			Ok(count) => self
				.left
				.get(..count)
				.ok_or(Error::Numerical("offline support")),
			Err(error) => {
				self.ready.fill(None);
				Err(error)
			}
		}
	}
	fn product_inner(
		&mut self,
		left: &[Number],
		index: usize,
		context: &mut Context,
	) -> Result<usize> {
		let right = *self
			.inputs
			.get(index)
			.ok_or(Error::Numerical("offline right index"))?;
		let count = left
			.len()
			.checked_add(right.len())
			.and_then(|n| n.checked_sub(1))
			.ok_or(Error::Budget("offline convolution"))?;
		let p = context.precision;
		// Preserve zero shortcuts ahead of direct/FFT admission and work.
		if left.iter().all(Number::is_zero) || right.iter().all(Number::is_zero) {
			self.left.resize_with(count, || Number::zero(p));
			self.left.fill(Number::zero(p));
			return Ok(count);
		}
		if count <= 16 {
			context.charge(
				left.len()
					.checked_mul(right.len())
					.and_then(|n| n.checked_mul(16))
					.ok_or(Error::Budget("offline direct convolution"))?,
			)?;
			self.left.resize_with(count, || Number::zero(p));
			self.left.fill(Number::zero(p));
			for (i, a) in left.iter().enumerate() {
				for (j, b) in right.iter().enumerate() {
					let out = self
						.left
						.get_mut(
							i.checked_add(j)
								.ok_or(Error::Budget("offline convolution index"))?,
						)
						.ok_or(Error::Numerical("offline support"))?;
					*out = out.add(&a.mul(b)?)?;
				}
			}
			for value in &self.left {
				value.validate()?;
			}
			return Ok(count);
		}
		let length = count
			.checked_next_power_of_two()
			.ok_or(Error::Budget("offline FFT support"))?;
		context.admit_grid(length)?;
		self.left.resize_with(length, || Number::zero(p));
		self.left.fill(Number::zero(p));
		for (out, value) in self.left.iter_mut().zip(left) {
			out.clone_from(value);
		}
		transform(&mut self.left, false, false, context)?;
		let ready = self
			.ready
			.get_mut(index)
			.ok_or(Error::Numerical("offline right index"))?;
		let spectrum = self
			.right
			.get_mut(index)
			.ok_or(Error::Numerical("offline right index"))?;
		if *ready != Some(length) {
			spectrum.resize_with(length, || Number::zero(p));
			spectrum.fill(Number::zero(p));
			for (out, value) in spectrum.iter_mut().zip(right) {
				out.clone_from(value);
			}
			transform(spectrum, false, false, context)?;
			*ready = Some(length);
		}
		for (a, b) in self.left.iter_mut().zip(spectrum.iter()) {
			*a = a.mul(b)?;
		}
		transform(&mut self.left, true, true, context)?;
		Ok(count)
	}
}

#[cfg(test)]
mod tests {
	use super::super::OfflinePolicy;
	use super::*;
	use crate::precision::{nearest_cos, nearest_sin};
	use googletest::prelude::*;
	fn assert_numbers(actual: &[Number], expected: &[Number]) {
		assert_eq!(actual.len(), expected.len());
		for (actual, expected) in actual.iter().zip(expected) {
			for (actual, expected) in [(&actual.re, &expected.re), (&actual.im, &expected.im)] {
				assert_eq!(actual, expected);
				assert_eq!(actual.precision(), expected.precision());
				assert_eq!(actual.repr().is_neg_zero(), expected.repr().is_neg_zero());
			}
		}
	}
	#[test]
	#[expect(
		clippy::panic_in_result_fn,
		reason = "Bounded exact reference and resource fixtures"
	)]
	fn shared_products_match_ordinary_across_direct_fft_zero_and_budget_boundaries()
	-> super::Result<()> {
		for p in [65, 128, 256] {
			// Product supports 15/16/17/23 exercise the exact direct/FFT switch.
			for (left_count, right_count) in [(8, 8), (8, 9), (9, 9), (13, 11)] {
				let number = Number::exact(crate::Complex64::new(0.125, -0.0), p)?;
				let imaginary = Number::exact(crate::Complex64::new(0.0625, 0.03125), p)?;
				let left0 = vec![number.clone(); left_count];
				let left1 = vec![imaginary.clone(); left_count];
				let right0 = vec![number; right_count];
				let right1 = vec![imaginary; right_count];
				let mut shared =
					Context::new(left_count + right_count, p, OfflinePolicy::default())?;
				let mut ordinary =
					Context::new(left_count + right_count, p, OfflinePolicy::default())?;
				let mut session = SharedConvolutionSession::new([&right0, &right1]);
				for (left, index, right) in [
					(&left0, 0, &right0),
					(&left1, 1, &right1),
					(&left0, 1, &right1),
					(&left1, 0, &right0),
				] {
					let actual = session.product(left, index, &mut shared)?;
					let expected = convolve(left, right, &mut ordinary)?;
					assert_numbers(actual, &expected);
				}
				let count = left_count + right_count - 1;
				let saved = if count <= 16 {
					0
				} else {
					let length = count.next_power_of_two();
					2 * 32
						* length
						* usize::try_from(length.ilog2().max(1)).unwrap()
						* usize::try_from(p).unwrap().div_ceil(64)
				};
				assert_eq!(shared.work + saved, ordinary.work);
				let policy = OfflinePolicy {
					max_work: shared.work,
					max_bytes: shared.bytes,
					..OfflinePolicy::default()
				};
				let mut exact = Context::new(left_count + right_count, p, policy)?;
				let mut limited = SharedConvolutionSession::new([&right0, &right1]);
				for (left, index) in [(&left0, 0), (&left1, 1), (&left0, 1), (&left1, 0)] {
					limited.product(left, index, &mut exact)?;
				}
				assert_eq!(exact.work, shared.work);
				let short_policy = OfflinePolicy {
					max_work: shared.work - 1,
					..policy
				};
				let mut short = Context::new(left_count + right_count, p, short_policy)?;
				let mut limited = SharedConvolutionSession::new([&right0, &right1]);
				for (left, index) in [(&left0, 0), (&left1, 1), (&left0, 1)] {
					limited.product(left, index, &mut short)?;
				}
				assert!(matches!(
					limited.product(&left1, 0, &mut short),
					Err(Error::Resource(_))
				));
				assert_eq!(limited.ready, [None, None]);
				let short_storage = OfflinePolicy {
					max_bytes: shared.bytes - 1,
					..policy
				};
				assert!(
					Context::new(left_count + right_count, p, short_storage)
						.and_then(
							|mut context| SharedConvolutionSession::new([&right0, &right1])
								.product(&left0, 0, &mut context)
								.map(|_| ())
						)
						.is_err()
				);
				// A new session cannot reuse the previous RHS spectra. Twiddles
				// remain cached in Context, exactly as ordinary convolution.
				let before = shared.work;
				let mut fresh = SharedConvolutionSession::new([&right0, &right1]);
				assert_numbers(
					fresh.product(&left0, 0, &mut shared)?,
					&convolve(&left0, &right0, &mut ordinary)?,
				);
				let cost = if count <= 16 {
					left_count * right_count * 16
				} else {
					let length = count.next_power_of_two();
					3 * 32 * length * usize::try_from(length.ilog2().max(1)).unwrap()
				} * usize::try_from(p).unwrap().div_ceil(64);
				assert_eq!(shared.work - before, cost);
				let zeros = vec![Number::zero(p); right_count];
				let mut zero_context =
					Context::new(left_count + right_count, p, OfflinePolicy::default())?;
				let mut zero_session = SharedConvolutionSession::new([&zeros, &right1]);
				assert_numbers(
					zero_session.product(&left0, 0, &mut zero_context)?,
					&convolve(&left0, &zeros, &mut ordinary)?,
				);
				assert_eq!(zero_context.work, 0);
				assert_eq!(zero_session.ready, [None, None]);
			}
		}
		Ok(())
	}
	#[test]
	#[expect(
		clippy::panic_in_result_fn,
		reason = "Exact shared-session error recovery assertions"
	)]
	fn shared_fft_failure_invalidates_both_spectra_and_preserves_lazy_errors() -> super::Result<()>
	{
		let right = vec![Number::one(128); 9];
		let left = right.clone();
		let mut context = Context::new(18, 128, OfflinePolicy::default())?;
		let mut session = SharedConvolutionSession::new([&right, &right]);
		session.product(&left, 0, &mut context)?;
		session.product(&left, 1, &mut context)?;
		assert_eq!(session.ready, [Some(32), Some(32)]);
		let before = context.work;
		assert!(matches!(
			session.product(&left, 2, &mut context),
			Err(Error::Numerical("offline right index"))
		));
		assert_eq!(context.work, before);
		assert_eq!(session.ready, [None, None]);
		let mut invalid = left.clone();
		invalid.first_mut().ok_or(Error::Numerical("fixture"))?.re =
			crate::precision::Binary::INFINITY;
		assert!(session.product(&invalid, 0, &mut context).is_err());
		assert_eq!(session.ready, [None, None]);
		assert_numbers(
			session.product(&left, 0, &mut context)?,
			&convolve(
				&left,
				&right,
				&mut Context::new(18, 128, OfflinePolicy::default())?,
			)?,
		);
		Ok(())
	}

	#[gtest]
	fn roots_preserve_exact_values_cache_reuse_and_length_one_resources() -> googletest::Result<()>
	{
		for length in [1, 2, 8, 32] {
			let mut context = Context::new(length, 65, OfflinePolicy::default())?;
			let actual = roots(length, &mut context)?;
			let work = context.work;
			let reused = roots(length, &mut context)?;
			expect_true!(Arc::ptr_eq(&actual, &reused));
			expect_eq!(context.work, work);
			if length == 1 {
				expect_true!(actual.is_empty());
				expect_eq!(context.cache.total_words(), 0);
			}
			for (index, root) in actual.iter().enumerate() {
				let numerator = index.checked_mul(2).ok_or(Error::Budget("fixture angle"))?;
				let ratio = super::super::number::div(
					&integer(65, u64::try_from(numerator)?),
					&integer(65, u64::try_from(length)?),
				)?;
				let mut scalar_cache = crate::offline::ConstCache::default();
				let pi = nearest_pi(65, &mut scalar_cache)?;
				let angle = super::super::number::mul(&ratio, &pi)?;
				let re = nearest_cos(65, &angle, &mut scalar_cache)?;
				let im = super::super::number::neg(&nearest_sin(65, &angle, &mut scalar_cache)?);
				expect_that!(&root.re, eq(&re));
				expect_that!(&root.im, eq(&im));
				expect_eq!(root.im.repr().is_neg_zero(), im.repr().is_neg_zero());
			}
		}
		Ok(())
	}
}
