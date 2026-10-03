use super::{
	Context, OfflineError as Error, OfflineResult as Result, fft,
	number::{Number, add, div, mul, neg, sqrt, sub, validate},
};
use crate::precision::{
	Binary, abs, checked, exact_from_f64, integer, nearest_atan, nearest_ln, nearest_sin_cos, zero,
};
use crate::{SynthesisAlgorithm, kernel::CompletionData};
pub(super) type Matrix = [Number; 4];
fn at(values: &[Number], index: usize, precision: u32) -> Number {
	values
		.get(index)
		.cloned()
		.unwrap_or_else(|| Number::zero(precision))
}
fn reverse(values: &[Number]) -> Vec<Number> {
	values.iter().rev().map(Number::conj).collect()
}
pub(super) fn controls(gamma: &[Number], context: &mut Context) -> Result<Vec<Matrix>> {
	context.charge(
		gamma
			.len()
			.checked_mul(64)
			.ok_or(Error::Budget("offline control work"))?,
	)?;
	gamma
		.iter()
		.map(|value| control(value, context.precision))
		.collect()
}
fn control(value: &Number, p: u32) -> Result<Matrix> {
	let one = integer(p, 1);
	let zero = integer(p, 0);
	value.validate()?;
	let mut scale = one.clone();
	for component in [&value.re, &value.im] {
		let magnitude = abs(component);
		if magnitude > scale {
			scale = magnitude;
		}
	}
	let re = div(&value.re, &scale)?;
	let im = div(&value.im, &scale)?;
	let scaled_one = div(&one, &scale)?;
	let norm = sqrt(&add(
		&add(&mul(&re, &re)?, &mul(&im, &im)?)?,
		&mul(&scaled_one, &scaled_one)?,
	)?)?;
	validate(&norm)?;
	let diagonal = Number {
		re: div(&scaled_one, &norm)?,
		im: zero,
	};
	let off = Number {
		re: div(&re, &norm)?,
		im: div(&im, &norm)?,
	};
	Ok([diagonal.clone(), off.clone(), off.conj().neg(), diagonal])
}

pub(super) fn completion(
	target: &[Number],
	context: &mut Context,
) -> Result<(Vec<Number>, CompletionData<Number>, Binary, usize)> {
	let p = context.precision;
	let one = integer(p, 1);
	let half = exact_from_f64(0.5, p)?;
	let two = integer(p, 2);
	let tolerance = div(
		&exact_from_f64(context.policy.certification.completion_tolerance, p)?,
		&integer(p, 16),
	)?;
	let mut grid = target
		.len()
		.checked_mul(4)
		.and_then(usize::checked_next_power_of_two)
		.ok_or(Error::Budget("offline completion grid"))?
		.max(32);
	while grid <= context.policy.max_grid {
		context.admit_grid(grid)?;
		let mut values = vec![Number::zero(p); grid];
		for (out, value) in values.iter_mut().zip(target) {
			out.clone_from(value);
		}
		fft::transform(&mut values, true, false, context)?;
		let ratio_samples = match context.policy.algorithm {
			SynthesisAlgorithm::InverseNlftDivideConquer => None,
			SynthesisAlgorithm::RhwHalfCholesky => Some(values.clone()),
		};
		for value in &mut values {
			let remainder = sub(
				&one,
				&add(&mul(&value.re, &value.re)?, &mul(&value.im, &value.im)?)?,
			)?;
			validate(&remainder)?;
			if remainder <= Binary::ZERO {
				return Err(Error::Numerical("offline Weiss logarithm domain"));
			}
			*value = Number::real(mul(
				&half,
				&checked(nearest_ln(p, &remainder, &mut context.cache)?)?,
			)?);
		}
		fft::transform(&mut values, false, true, context)?;
		for (index, value) in values.iter_mut().enumerate().skip(1) {
			if index <= grid / 2 {
				*value = Number::zero(p);
			} else {
				*value = value.scale(&two)?;
			}
		}
		fft::transform(&mut values, true, false, context)?;
		let ratio = if let Some(mut samples) = ratio_samples {
			for (sample, exponent) in samples.iter_mut().zip(&values) {
				*sample = sample.mul(&exponent.neg().exp(&mut context.cache)?)?;
				sample.validate()?;
			}
			fft::transform(&mut samples, false, true, context)?;
			CompletionData::Rhw(
				samples
					.get(..target.len())
					.ok_or(Error::Budget("offline ratio support"))?
					.to_vec(),
			)
		} else {
			CompletionData::InverseNlft
		};
		for value in &mut values {
			*value = value.exp(&mut context.cache)?;
			if !value.is_finite() {
				return Err(Error::Numerical("offline Weiss exponential"));
			}
		}
		fft::transform(&mut values, false, true, context)?;
		let mut astar = Vec::with_capacity(target.len());
		for index in 0..target.len() {
			let slot = if index == 0 {
				0
			} else {
				grid.checked_sub(index)
					.ok_or(Error::Budget("offline completion support"))?
			};
			astar.push(at(&values, slot, p).conj());
		}
		let first = astar
			.first_mut()
			.ok_or(Error::Numerical("empty complement"))?;
		first.im = integer(p, 0);
		let residual = completion_residual(&astar, target, context)?;
		if residual <= tolerance {
			return Ok((astar, ratio, residual, grid));
		}
		grid = grid
			.checked_mul(2)
			.ok_or(Error::Budget("offline grid refinement"))?;
	}
	Err(Error::Numerical("offline completion grid exhausted"))
}
fn completion_residual(
	astar: &[Number],
	target: &[Number],
	context: &mut Context,
) -> Result<Binary> {
	let a = fft::convolve(astar, &reverse(astar), context)?;
	let b = fft::convolve(target, &reverse(target), context)?;
	let center = target.len().saturating_sub(1);
	let mut total = zero(context.precision);
	for (index, (a, b)) in a.iter().zip(&b).enumerate() {
		let mut value = a.add(b)?;
		if index == center {
			value = value.sub(&Number::one(context.precision))?;
		}
		total = add(&total, &value.abs()?)?;
	}
	validate(&total)?;
	Ok(total)
}
fn product_windows(
	left: [&[Number]; 4],
	right: [&[Number]; 2],
	windows: [(usize, usize); 4],
	context: &mut Context,
) -> Result<[Vec<Number>; 4]> {
	let mut session = fft::SharedConvolutionSession::new(right);
	let mut output: [Vec<Number>; 4] = std::array::from_fn(|_| Vec::new());
	for (((left, (offset, count)), out), index) in left
		.into_iter()
		.zip(windows)
		.zip(&mut output)
		.zip([0, 1, 1, 0])
	{
		let values = session.product(left, index, context)?;
		out.reserve_exact(count);
		for index in 0..count {
			out.push(at(
				values,
				offset
					.checked_add(index)
					.ok_or(Error::Budget("offline midpoint support"))?,
				context.precision,
			));
		}
	}
	Ok(output)
}

struct InverseNode {
	xi: Vec<Number>,
	eta: Vec<Number>,
}
fn midpoint_from_windows(windows: [Vec<Number>; 4], p: u32) -> Result<(Vec<Number>, Vec<Number>)> {
	let [mut a, xb, mut b, xa] = windows;
	for (index, (a, b)) in a.iter_mut().zip(&mut b).enumerate() {
		*a = a.add(&at(&xb, index, p))?;
		*b = b.sub(&at(&xa, index, p))?;
	}
	Ok((a, b))
}
fn transfer_from_windows(windows: [Vec<Number>; 4], p: u32) -> Result<InverseNode> {
	let [ex, mut xi, mut eta, xx] = windows;
	for (index, (x, e)) in xi.iter_mut().zip(&mut eta).enumerate() {
		let first = index
			.checked_sub(1)
			.map_or_else(|| Number::zero(p), |i| at(&ex, i, p));
		let second = index
			.checked_sub(1)
			.map_or_else(|| Number::zero(p), |i| at(&xx, i, p));
		*x = first.add(x)?;
		*e = e.sub(&second)?;
	}
	Ok(InverseNode { xi, eta })
}

pub(super) fn inverse(
	astar: &[Number],
	target: &[Number],
	context: &mut Context,
) -> Result<Vec<Number>> {
	let mut gamma = vec![Number::zero(context.precision); target.len()];
	inverse_node(astar, target, &mut gamma, context, false)?;
	Ok(gamma)
}
fn inverse_leaf(
	astar: &[Number],
	target: &[Number],
	gamma: &mut [Number],
	context: &mut Context,
	transfer: bool,
) -> Result<Option<InverseNode>> {
	let p = context.precision;
	let pivot = at(astar, 0, p);
	if pivot.is_zero() {
		return Err(Error::Numerical("offline singular inverse pivot"));
	}
	let reflection = at(target, 0, p).div(&pivot)?;
	*gamma
		.first_mut()
		.ok_or(Error::Numerical("offline leaf support"))? = reflection;
	if !transfer {
		gamma
			.first()
			.ok_or(Error::Numerical("offline leaf support"))?
			.validate()?;
		return Ok(None);
	}
	context.charge(64)?;
	let [eta, xi, _, _] = control(
		gamma
			.first()
			.ok_or(Error::Numerical("offline leaf support"))?,
		p,
	)?;
	Ok(Some(InverseNode {
		xi: vec![xi],
		eta: vec![eta],
	}))
}
fn inverse_node(
	astar: &[Number],
	target: &[Number],
	gamma: &mut [Number],
	context: &mut Context,
	transfer: bool,
) -> Result<Option<InverseNode>> {
	let p = context.precision;
	let count = target.len();
	if count == 0 || astar.len() != count || gamma.len() != count {
		return Err(Error::Numerical("offline inverse support"));
	}
	if count == 1 {
		return inverse_leaf(astar, target, gamma, context, transfer);
	}
	let lower_len = count / 2;
	let upper_len = count
		.checked_sub(lower_len)
		.ok_or(Error::Budget("offline inverse split"))?;
	let (upper_gamma, lower_gamma) = gamma.split_at_mut(upper_len);
	let upper = inverse_node(
		astar
			.get(..upper_len)
			.ok_or(Error::Numerical("offline inverse prefix"))?,
		target
			.get(..upper_len)
			.ok_or(Error::Numerical("offline inverse prefix"))?,
		upper_gamma,
		context,
		true,
	)?
	.ok_or(Error::Numerical("missing offline upper transfer"))?;
	let eta_conj = reverse(&upper.eta);
	let xi_conj = reverse(&upper.xi);
	let offset = upper_len.saturating_sub(1);
	let (midpoint_a, midpoint_b) = midpoint_from_windows(
		product_windows(
			[&eta_conj, &xi_conj, &upper.eta, &upper.xi],
			[astar, target],
			[
				(offset, lower_len),
				(offset, lower_len),
				(upper_len, lower_len),
				(upper_len, lower_len),
			],
			context,
		)?,
		p,
	)?;
	// The second inverse is invoked only after the completed first-half update.
	let lower = inverse_node(&midpoint_a, &midpoint_b, lower_gamma, context, true)?
		.ok_or(Error::Numerical("missing offline lower transfer"))?;
	drop((midpoint_a, midpoint_b));
	if !transfer {
		return Ok(None);
	}
	let shifted_count = count
		.checked_sub(1)
		.ok_or(Error::Budget("offline reconstruction support"))?;
	let node = transfer_from_windows(
		product_windows(
			[&eta_conj, &upper.xi, &upper.eta, &xi_conj],
			[&lower.xi, &lower.eta],
			[
				(0, shifted_count),
				(0, count),
				(0, count),
				(0, shifted_count),
			],
			context,
		)?,
		p,
	)?;
	Ok(Some(node))
}

pub(super) fn real_parity_wx_phases(
	gamma: &[Number],
	context: &mut Context,
) -> Result<(Vec<Binary>, Vec<Matrix>)> {
	context.charge(
		gamma
			.len()
			.checked_mul(32)
			.ok_or(Error::Budget("offline phase work"))?,
	)?;
	let p = context.precision;
	let two = integer(p, 2);
	let zero = integer(p, 0);
	let mut phases: Vec<_> = gamma
		.iter()
		.map(|value| checked(nearest_atan(p, &value.re, &mut context.cache)?))
		.collect::<std::result::Result<_, _>>()?;
	for index in 0..phases.len() / 2 {
		let mirror = phases
			.len()
			.checked_sub(index)
			.and_then(|v| v.checked_sub(1))
			.ok_or(Error::Budget("offline phase support"))?;
		let middle = div(
			&add(
				phases.get(index).ok_or(Error::Numerical("offline phase"))?,
				phases
					.get(mirror)
					.ok_or(Error::Numerical("offline phase"))?,
			)?,
			&two,
		)?;
		phases
			.get_mut(index)
			.ok_or(Error::Numerical("offline phase"))?
			.clone_from(&middle);
		*phases
			.get_mut(mirror)
			.ok_or(Error::Numerical("offline phase"))? = middle;
	}
	let matrices = phases
		.iter()
		.map(|phase| {
			let (sin, cos) = nearest_sin_cos(p, phase, &mut context.cache)?;
			Ok([
				Number {
					re: cos.clone(),
					im: zero.clone(),
				},
				Number {
					re: sin.clone(),
					im: zero.clone(),
				},
				Number {
					re: neg(&sin),
					im: zero.clone(),
				},
				Number {
					re: cos,
					im: zero.clone(),
				},
			])
		})
		.collect::<Result<_>>()?;
	Ok((phases, matrices))
}

// Same complex displacement identity, in explicitly requested offline precision.
#[expect(
	clippy::many_single_char_names,
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	reason = "Complex rank-two recurrence uses equal-length vectors with 0 <= k < j < n"
)]
pub(super) fn half_cholesky(c: &[Number], context: &mut Context) -> Result<Vec<Number>> {
	let n = c.len();
	if n == 0 {
		return Err(Error::Numerical("empty offline Weiss ratio"));
	}
	context.charge(
		n.checked_mul(n)
			.and_then(|v| v.checked_mul(64))
			.ok_or(Error::Budget("offline Half-Cholesky work"))?,
	)?;
	let p = context.precision;
	let mut first = vec![Number::zero(p); n];
	first[0] = Number::one(p);
	let mut second = reverse(c);
	let mut solution = second.clone();
	for k in 0..n {
		let x = first[k].clone();
		let y = second[k].clone();
		let scale = sqrt(&add(
			&mul(&x.abs()?, &x.abs()?)?,
			&mul(&y.abs()?, &y.abs()?)?,
		)?)?;
		validate(&scale)?;
		if scale == Binary::ZERO {
			return Err(Error::Numerical("offline Half-Cholesky pivot"));
		}
		let inverse = div(&Number::one(p).re, &scale)?;
		let alpha = x.scale(&inverse)?;
		let beta = y.scale(&inverse)?;
		let rhs = solution[k].clone();
		let mut previous = Number::real(scale);
		for j in k + 1..n {
			let u = first[j]
				.mul(&alpha.conj())?
				.add(&second[j].mul(&beta.conj())?)?;
			let v = second[j].mul(&alpha)?.sub(&first[j].mul(&beta)?)?;
			u.validate()?;
			v.validate()?;
			solution[j] = solution[j].sub(&u.scale(&inverse)?.mul(&rhs)?)?;
			solution[j].validate()?;
			first[j] = previous;
			second[j] = v;
			previous = u;
		}
	}
	Ok(reverse(&solution))
}

#[cfg(test)]
#[expect(
	clippy::arithmetic_side_effects,
	clippy::panic_in_result_fn,
	reason = "Bounded deterministic inverse fixtures and independent pre-optimization oracle"
)]
mod tests {
	use super::Result;
	use super::*;
	use crate::offline::{ConstCache, OfflinePolicy};
	use crate::precision::{nearest_cos, nearest_sin};
	use googletest::prelude::*;

	fn fixture(count: usize, precision: u32) -> Result<(Vec<Number>, Vec<Number>)> {
		let a = (0..count)
			.map(|index| {
				Number::exact(
					if index == 0 {
						crate::Complex64::new(0.95, 0.0)
					} else {
						crate::Complex64::new(
							0.001 * f64::from(u32::try_from(index).unwrap()),
							-0.0005,
						)
					},
					precision,
				)
			})
			.collect::<Result<_>>()?;
		let b = (0..count)
			.map(|index| {
				Number::exact(
					crate::Complex64::new(
						0.01 / f64::from(u32::try_from(index + 1).unwrap()),
						0.002,
					),
					precision,
				)
			})
			.collect::<Result<_>>()?;
		Ok((a, b))
	}
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
	fn full_transfer_oracle(
		astar: &[Number],
		target: &[Number],
		gamma: &mut [Number],
		context: &mut Context,
	) -> Result<InverseNode> {
		let p = context.precision;
		let count = target.len();
		if count == 0 || astar.len() != count || gamma.len() != count {
			return Err(Error::Numerical("offline inverse support"));
		}
		if count == 1 {
			let pivot = at(astar, 0, p);
			if pivot.is_zero() {
				return Err(Error::Numerical("offline singular inverse pivot"));
			}
			let reflection = at(target, 0, p).div(&pivot)?;
			let matrix = controls(std::slice::from_ref(&reflection), context)?
				.pop()
				.ok_or(Error::Numerical("offline leaf control"))?;
			*gamma
				.first_mut()
				.ok_or(Error::Numerical("offline leaf support"))? = reflection;
			let [eta, xi, _, _] = matrix;
			return Ok(InverseNode {
				xi: vec![xi],
				eta: vec![eta],
			});
		}
		let lower_len = count / 2;
		let upper_len = count
			.checked_sub(lower_len)
			.ok_or(Error::Budget("offline inverse split"))?;
		let (upper_gamma, lower_gamma) = gamma.split_at_mut(upper_len);
		let upper = full_transfer_oracle(
			astar
				.get(..upper_len)
				.ok_or(Error::Numerical("offline inverse prefix"))?,
			target
				.get(..upper_len)
				.ok_or(Error::Numerical("offline inverse prefix"))?,
			upper_gamma,
			context,
		)?;
		let eta_conj = reverse(&upper.eta);
		let xi_conj = reverse(&upper.xi);
		let ea = fft::convolve(&eta_conj, astar, context)?;
		let xb = fft::convolve(&xi_conj, target, context)?;
		let eb = fft::convolve(&upper.eta, target, context)?;
		let xa = fft::convolve(&upper.xi, astar, context)?;
		let offset = upper_len.saturating_sub(1);
		let mut midpoint_a = Vec::with_capacity(lower_len);
		let mut midpoint_b = Vec::with_capacity(lower_len);
		for index in 0..lower_len {
			let a_index = offset
				.checked_add(index)
				.ok_or(Error::Budget("offline midpoint support"))?;
			let b_index = upper_len
				.checked_add(index)
				.ok_or(Error::Budget("offline midpoint support"))?;
			midpoint_a.push(at(&ea, a_index, p).add(&at(&xb, a_index, p))?);
			midpoint_b.push(at(&eb, b_index, p).sub(&at(&xa, b_index, p))?);
		}
		// The second inverse is invoked only after the completed first-half update.
		let lower = full_transfer_oracle(&midpoint_a, &midpoint_b, lower_gamma, context)?;
		let ex = fft::convolve(&eta_conj, &lower.xi, context)?;
		let xe = fft::convolve(&upper.xi, &lower.eta, context)?;
		let ee = fft::convolve(&upper.eta, &lower.eta, context)?;
		let xx = fft::convolve(&xi_conj, &lower.xi, context)?;
		let mut xi = Vec::with_capacity(count);
		let mut eta = Vec::with_capacity(count);
		for index in 0..count {
			let first = index
				.checked_sub(1)
				.map_or_else(|| Number::zero(p), |i| at(&ex, i, p));
			let second = index
				.checked_sub(1)
				.map_or_else(|| Number::zero(p), |i| at(&xx, i, p));
			xi.push(first.add(&at(&xe, index, p))?);
			eta.push(at(&ee, index, p).sub(&second)?);
		}
		Ok(InverseNode { xi, eta })
	}
	fn shared_savings(count: usize, transfer: bool) -> usize {
		if count == 1 {
			return 0;
		}
		let upper = count.div_ceil(2);
		let transform = |support: usize| {
			if support <= 16 {
				0
			} else {
				let length = support.next_power_of_two();
				32 * length * usize::try_from(length.ilog2().max(1)).unwrap()
			}
		};
		shared_savings(upper, true)
			+ shared_savings(count / 2, true)
			+ 2 * transform(count + upper - 1)
			+ if transfer {
				2 * transform(count - 1)
			} else {
				0
			}
	}
	#[test]
	fn reflections_only_root_matches_full_transfer_values_and_exact_work_boundaries() -> Result<()>
	{
		for p in [65, 128, 256] {
			for count in [1, 2, 3, 5, 8, 17, 32, 65] {
				let (a, b) = fixture(count, p)?;
				let mut context = Context::new(count, p, OfflinePolicy::default())?;
				let actual = inverse(&a, &b, &mut context)?;
				let mut expected = vec![Number::zero(p); count];
				let mut full = Context::new(count, p, OfflinePolicy::default())?;
				let original_transfer = full_transfer_oracle(&a, &b, &mut expected, &mut full)?;
				let mut full_gamma = vec![Number::zero(p); count];
				let mut shared_context = Context::new(count, p, OfflinePolicy::default())?;
				let shared_transfer =
					inverse_node(&a, &b, &mut full_gamma, &mut shared_context, true)?
						.ok_or(Error::Numerical("fixture transfer"))?;
				assert_numbers(&shared_transfer.xi, &original_transfer.xi);
				assert_numbers(&shared_transfer.eta, &original_transfer.eta);
				assert_numbers(&full_gamma, &expected);
				assert_eq!(
					shared_context.work
						+ shared_savings(count, true) * usize::try_from(p).unwrap().div_ceil(64),
					full.work
				);
				assert_numbers(&actual, &expected);
				let mut top = Context::new(count, p, OfflinePolicy::default())?;
				let mut top_gamma = vec![Number::zero(p); count];
				let mut node_context = Context::new(count, p, OfflinePolicy::default())?;
				if count == 1 {
					top.charge(64)?;
				} else {
					let upper_len = count.div_ceil(2);
					let (upper_gamma, lower_gamma) = top_gamma.split_at_mut(upper_len);
					let upper = full_transfer_oracle(
						&a[..upper_len],
						&b[..upper_len],
						upper_gamma,
						&mut node_context,
					)?;
					let eta_conj = reverse(&upper.eta);
					let xi_conj = reverse(&upper.xi);
					let ea = fft::convolve(&eta_conj, &a, &mut node_context)?;
					let xb = fft::convolve(&xi_conj, &b, &mut node_context)?;
					let eb = fft::convolve(&upper.eta, &b, &mut node_context)?;
					let xa = fft::convolve(&upper.xi, &a, &mut node_context)?;
					let ma = (0..count / 2)
						.map(|i| at(&ea, upper_len - 1 + i, p).add(&at(&xb, upper_len - 1 + i, p)))
						.collect::<Result<Vec<_>>>()?;
					let mb = (0..count / 2)
						.map(|i| at(&eb, upper_len + i, p).sub(&at(&xa, upper_len + i, p)))
						.collect::<Result<Vec<_>>>()?;
					let lower = full_transfer_oracle(&ma, &mb, lower_gamma, &mut node_context)?;
					top.roots.clone_from(&context.roots);
					fft::convolve(&eta_conj, &lower.xi, &mut top)?;
					fft::convolve(&upper.xi, &lower.eta, &mut top)?;
					fft::convolve(&upper.eta, &lower.eta, &mut top)?;
					fft::convolve(&xi_conj, &lower.xi, &mut top)?;
				}
				assert_eq!(
					context.work
						+ top.work
						+ shared_savings(count, false) * usize::try_from(p).unwrap().div_ceil(64),
					full.work,
					"p={p}, count={count}"
				);
				let mut policy = OfflinePolicy {
					max_work: context.work,
					..OfflinePolicy::default()
				};
				let mut limited = Context::new(count, p, policy)?;
				assert_numbers(&inverse(&a, &b, &mut limited)?, &actual);
				if context.work > 0 {
					policy.max_work = context.work - 1;
					assert!(matches!(
						inverse(&a, &b, &mut Context::new(count, p, policy)?),
						Err(Error::Budget("offline work"))
					));
				}
				let mut storage = OfflinePolicy {
					max_bytes: context.bytes,
					..OfflinePolicy::default()
				};
				inverse(&a, &b, &mut Context::new(count, p, storage)?)?;
				storage.max_bytes -= 1;
				assert!(
					Context::new(count, p, storage)
						.and_then(|mut context| inverse(&a, &b, &mut context))
						.is_err()
				);
			}
			let mut singleton = Context::new(1, p, OfflinePolicy::default())?;
			assert!(matches!(
				inverse(&[Number::zero(p)], &[Number::one(p)], &mut singleton),
				Err(Error::Numerical("offline singular inverse pivot"))
			));
		}
		Ok(())
	}

	#[gtest]
	fn offline_memory_admits_only_the_selected_completion_payload() -> googletest::Result<()> {
		let nlft_policy = OfflinePolicy::default();
		let budget = Context::modeled_storage(2, 128, nlft_policy, 32)?;
		let mut nlft = Context::new(
			2,
			128,
			OfflinePolicy {
				max_bytes: budget,
				..nlft_policy
			},
		)?;
		expect_true!(nlft.admit_grid(32).is_ok());
		let mut rhw = Context::new(
			2,
			128,
			OfflinePolicy {
				algorithm: SynthesisAlgorithm::RhwHalfCholesky,
				max_bytes: budget,
				..nlft_policy
			},
		)?;
		expect_true!(matches!(
			rhw.admit_grid(32),
			Err(Error::Budget("offline modeled memory"))
		));
		Ok(())
	}

	#[gtest]
	#[allow(
		clippy::arithmetic_side_effects,
		reason = "Small bounded fixtures keep the independent work formula visible"
	)]
	fn nlft_completion_skips_the_ratio_transform_at_each_precision() -> googletest::Result<()> {
		for precision in [128, 256] {
			let target = vec![Number::exact(crate::Complex64::new(0.3, 0.2), precision)?];
			let mut nlft = Context::new(1, precision, OfflinePolicy::default())?;
			let mut rhw = Context::new(
				1,
				precision,
				OfflinePolicy {
					algorithm: crate::SynthesisAlgorithm::RhwHalfCholesky,
					..OfflinePolicy::default()
				},
			)?;
			completion(&target, &mut nlft)?;
			completion(&target, &mut rhw)?;
			// One grid-32 FFT is 32 * 5 * 32 units, weighted by limbs.
			expect_eq!(
				rhw.work - nlft.work,
				5_120 * usize::try_from(precision)?.div_ceil(64)
			);
		}
		Ok(())
	}

	#[gtest]
	#[allow(
		clippy::arithmetic_side_effects,
		reason = "Small bounded fixtures keep the independent work formula visible"
	)]
	fn completion_payload_preserves_common_values_and_cumulative_refinement_work()
	-> googletest::Result<()> {
		for precision in [128, 256] {
			let target = vec![Number::exact(crate::Complex64::new(0.49, 0.0), precision)?; 2];
			let mut nlft = Context::new(2, precision, OfflinePolicy::default())?;
			let (expected, payload, residual, grid) = completion(&target, &mut nlft)?;
			expect_true!(matches!(payload, CompletionData::InverseNlft));
			expect_that!(grid, gt(32));
			for (algorithm, transforms) in [
				(SynthesisAlgorithm::InverseNlftDivideConquer, 4),
				(SynthesisAlgorithm::RhwHalfCholesky, 5),
			] {
				let mut units = 0;
				let mut attempt: usize = 32;
				while attempt <= grid {
					// Each grid creates twiddles once, performs the selected
					// FFTs, and uses two 2-by-2 direct residual products.
					units += attempt * usize::try_from(attempt.ilog2())? * 32 * transforms
						+ attempt * 32
						+ 128;
					attempt *= 2;
				}
				let budget = units * usize::try_from(precision)?.div_ceil(64);
				let policy = OfflinePolicy {
					algorithm,
					max_work: budget,
					..OfflinePolicy::default()
				};
				let mut context = Context::new(2, precision, policy)?;
				let (actual, payload, actual_residual, actual_grid) =
					completion(&target, &mut context)?;
				expect_eq!(actual_grid, grid);
				expect_eq!(&actual_residual, &residual);
				for (actual, expected) in actual.iter().zip(&expected) {
					expect_eq!(&actual.re, &expected.re);
					expect_eq!(&actual.im, &expected.im);
					expect_eq!(
						actual.im.repr().is_neg_zero(),
						expected.im.repr().is_neg_zero()
					);
				}
				expect_eq!(context.work, budget);
				match algorithm {
					SynthesisAlgorithm::InverseNlftDivideConquer => {
						expect_true!(matches!(payload, CompletionData::InverseNlft));
					}
					SynthesisAlgorithm::RhwHalfCholesky => {
						let CompletionData::Rhw(ratio) = payload else {
							return fail!("missing RHW payload");
						};
						expect_eq!(ratio.len(), target.len());
					}
				}
				let mut insufficient = Context::new(
					2,
					precision,
					OfflinePolicy {
						max_work: budget - 1,
						..policy
					},
				)?;
				expect_true!(matches!(
					completion(&target, &mut insufficient),
					Err(Error::Budget("offline work"))
				));
			}
		}
		Ok(())
	}

	#[test]
	fn compact_windows_become_outputs_without_new_vector_storage() -> Result<()> {
		for p in [65, 128, 256] {
			let input = vec![Number::exact(crate::Complex64::new(0.25, -0.0), p)?; 2];
			let mut context = Context::new(2, p, OfflinePolicy::default())?;
			let windows = product_windows(
				[&input, &input, &input, &input],
				[&input, &input],
				[(0, 2); 4],
				&mut context,
			)?;
			let midpoint_storage = [
				(windows[0].as_ptr(), windows[0].capacity()),
				(windows[2].as_ptr(), windows[2].capacity()),
			];
			let (a, b) = midpoint_from_windows(windows, p)?;
			assert_eq!((a.as_ptr(), a.capacity()), midpoint_storage[0]);
			assert_eq!((b.as_ptr(), b.capacity()), midpoint_storage[1]);
			let windows = product_windows(
				[&input, &input, &input, &input],
				[&input, &input],
				[(0, 1), (0, 2), (0, 2), (0, 1)],
				&mut context,
			)?;
			let transfer_storage = [
				(windows[1].as_ptr(), windows[1].capacity()),
				(windows[2].as_ptr(), windows[2].capacity()),
			];
			let node = transfer_from_windows(windows, p)?;
			assert_eq!((node.xi.as_ptr(), node.xi.capacity()), transfer_storage[0]);
			assert_eq!(
				(node.eta.as_ptr(), node.eta.capacity()),
				transfer_storage[1]
			);
		}
		Ok(())
	}
	#[gtest]
	fn phase_controls_match_separate_correctly_rounded_primitives() -> googletest::Result<()> {
		for p in [65, 128, 256] {
			for values in [vec![0.0], vec![-0.0], vec![0.25, -0.5, 1.0, -0.25]] {
				let gamma = values
					.into_iter()
					.map(|value| exact_from_f64(value, p).map(Number::real))
					.collect::<std::result::Result<Vec<_>, _>>()?;
				let mut context = Context::new(gamma.len(), p, OfflinePolicy::default())?;
				let (phases, matrices) = real_parity_wx_phases(&gamma, &mut context)?;
				expect_eq!(
					context.work,
					gamma
						.len()
						.checked_mul(32)
						.and_then(|units| units.checked_mul(usize::try_from(p).ok()?.div_ceil(64)))
						.ok_or(Error::Budget("fixture work"))?
				);
				expect_eq!(phases.len(), gamma.len());
				let mut scalar_cache = ConstCache::default();
				for (phase, matrix) in phases.iter().zip(matrices) {
					let sin = nearest_sin(p, phase, &mut scalar_cache)?;
					let cos = nearest_cos(p, phase, &mut scalar_cache)?;
					let expected = [cos.clone(), sin.clone(), neg(&sin), cos];
					for (actual, expected) in matrix.iter().zip(expected) {
						expect_eq!(&actual.re, &expected);
						expect_eq!(
							actual.re.repr().is_neg_zero(),
							expected.repr().is_neg_zero()
						);
						expect_eq!(&actual.im, &integer(p, 0));
					}
				}
			}
		}
		Ok(())
	}
}
