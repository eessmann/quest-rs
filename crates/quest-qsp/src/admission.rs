//! Binary64 outward circle admission. This is independent of the point FFT
//! used by candidate synthesis: a radix-two interval FFT encloses samples and
//! an analytic derivative bound covers every point between those samples.
use crate::{Complex64, Error, Policy, Result};
use quest_numerics::Interval;

#[derive(Clone, Copy)]
struct ComplexInterval {
	re: Interval,
	im: Interval,
}
impl ComplexInterval {
	fn point(value: Complex64) -> Result<Self> {
		Ok(Self {
			re: Interval::point(value.re)?,
			im: Interval::point(value.im)?,
		})
	}
	fn add(self, rhs: Self) -> Result<Self> {
		Ok(Self {
			re: self.re.checked_add(rhs.re)?,
			im: self.im.checked_add(rhs.im)?,
		})
	}
	fn sub(self, rhs: Self) -> Result<Self> {
		Ok(Self {
			re: self.re.checked_sub(rhs.re)?,
			im: self.im.checked_sub(rhs.im)?,
		})
	}
	fn mul(self, rhs: Self) -> Result<Self> {
		Ok(Self {
			re: self
				.re
				.checked_mul(rhs.re)?
				.checked_sub(self.im.checked_mul(rhs.im)?)?,
			im: self
				.re
				.checked_mul(rhs.im)?
				.checked_add(self.im.checked_mul(rhs.re)?)?,
		})
	}
	fn magnitude(self) -> Result<Interval> {
		Ok(self.re.square()?.checked_add(self.im.square()?)?.sqrt()?)
	}
}

pub fn contractivity(target: &[Complex64], policy: Policy) -> Result<f64> {
	let mut norm = Interval::point(0.0)?;
	let mut derivative = norm;
	for (index, &coefficient) in target.iter().enumerate() {
		let magnitude = ComplexInterval::point(coefficient)?.magnitude()?;
		norm = norm.checked_add(magnitude)?;
		let index =
			f64::from(u32::try_from(index).map_err(|_| Error::Budget("contractivity degree"))?);
		derivative = derivative.checked_add(magnitude.checked_mul(Interval::point(index)?)?)?;
	}
	let threshold_interval =
		Interval::point(1.0)?.checked_sub(Interval::point(policy.contractivity_margin)?)?;
	let threshold = threshold_interval.lower();
	if norm.upper() < threshold {
		return Ok(norm.upper());
	}
	let mut count = target
		.len()
		.checked_mul(4)
		.and_then(usize::checked_next_power_of_two)
		.ok_or(Error::Budget("contractivity FFT"))?
		.max(32);
	let mut last_bound = norm.upper();
	let mut charged = 0_usize;
	while count <= policy.max_completion_grid && count <= policy.limits.max_len {
		let work = count
			.checked_mul(usize::try_from(count.ilog2()).map_err(|_| Error::Budget("FFT work"))?)
			.and_then(|n| n.checked_mul(256))
			.ok_or(Error::Budget("FFT work"))?;
		charged = charged.checked_add(work).ok_or(Error::Budget("FFT work"))?;
		if charged > policy.limits.max_work {
			return Err(Error::Budget("contractivity FFT work"));
		}
		let values = sample_enclosures(target, count, policy)?;
		let mut max_sample = 0.0_f64;
		for value in values {
			let magnitude = value.magnitude()?;
			if magnitude.lower() >= threshold_interval.upper() {
				return Err(Error::ContractivityViolation {
					lower: magnitude.lower(),
					threshold: threshold_interval.upper(),
				});
			}
			max_sample = max_sample.max(magnitude.upper());
		}
		let pi = Interval::new(
			std::f64::consts::PI.next_down(),
			std::f64::consts::PI.next_up(),
		)?;
		let length = Interval::point(f64::from(
			u32::try_from(count).map_err(|_| Error::Budget("FFT length"))?,
		))?;
		let gap = derivative.checked_mul(pi.checked_div(length)?)?;
		last_bound = Interval::point(max_sample)?.checked_add(gap)?.upper();
		if last_bound < threshold {
			return Ok(last_bound);
		}
		count = count
			.checked_mul(2)
			.ok_or(Error::Budget("contractivity refinement"))?;
	}
	Err(Error::Contractivity { upper: last_bound })
}

fn sample_enclosures(
	target: &[Complex64],
	count: usize,
	policy: Policy,
) -> Result<Vec<ComplexInterval>> {
	let bytes = count
		.checked_mul(size_of::<ComplexInterval>())
		.and_then(|n| n.checked_mul(2))
		.ok_or(Error::Budget("interval FFT storage"))?;
	if bytes > policy.limits.max_bytes {
		return Err(Error::Budget("interval FFT storage"));
	}
	let zero = ComplexInterval::point(Complex64::new(0.0, 0.0))?;
	let mut values = Vec::new();
	values
		.try_reserve_exact(count)
		.map_err(|_| Error::Budget("interval FFT allocation"))?;
	values.resize(count, zero);
	for (slot, &value) in values.iter_mut().zip(target) {
		*slot = ComplexInterval::point(value)?;
	}
	let mut roots = Vec::new();
	roots
		.try_reserve_exact(count / 2)
		.map_err(|_| Error::Budget("interval FFT roots"))?;
	let pi = Interval::new(
		std::f64::consts::PI.next_down(),
		std::f64::consts::PI.next_up(),
	)?;
	let denominator = Interval::point(f64::from(
		u32::try_from(count).map_err(|_| Error::Budget("interval FFT length"))?,
	))?;
	for index in 0..count / 2 {
		let twice = index
			.checked_mul(2)
			.and_then(|n| u32::try_from(n).ok())
			.ok_or(Error::Budget("root index"))?;
		let angle = pi
			.checked_mul(Interval::point(f64::from(twice))?)?
			.checked_div(denominator)?;
		roots.push(ComplexInterval {
			re: angle.cos()?,
			im: angle.sin()?,
		});
	}
	let shift = usize::BITS
		.checked_sub(count.ilog2())
		.ok_or(Error::Budget("bit permutation"))?;
	for index in 0..count {
		let reversed = index
			.reverse_bits()
			.checked_shr(shift)
			.ok_or(Error::Budget("bit permutation"))?;
		if index < reversed {
			values.swap(index, reversed);
		}
	}
	let mut width = 2;
	loop {
		let step = count
			.checked_div(width)
			.ok_or(Error::Budget("FFT stage width"))?;
		for block in values.chunks_exact_mut(width) {
			let (left, right) = block.split_at_mut(width / 2);
			for (index, (first, second)) in left.iter_mut().zip(right).enumerate() {
				let root = roots
					.get(
						index
							.checked_mul(step)
							.ok_or(Error::Budget("root offset"))?,
					)
					.ok_or(Error::Budget("root support"))?;
				let rotated = second.mul(*root)?;
				let original = *first;
				*first = original.add(rotated)?;
				*second = original.sub(rotated)?;
			}
		}
		if width == count {
			break;
		}
		width = width
			.checked_mul(2)
			.ok_or(Error::Budget("interval FFT stage"))?;
	}
	Ok(values)
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;
	#[gtest]
	fn interval_fft_encloses_independent_four_point_values() -> googletest::Result<()> {
		let target = [Complex64::new(0.1, 0.2), Complex64::new(-0.3, 0.1)];
		let values = sample_enclosures(&target, 4, Policy::default())?;
		// Exact dyadic 0.1 - 0.3 equals the successor of binary64 -0.2.
		let expected = [
			((-0.2_f64).next_up(), 0.3),
			(0.0, -0.1),
			(0.4, 0.1),
			(0.2, 0.5),
		];
		for (value, (re, im)) in values.iter().zip(expected) {
			expect_true!(value.re.contains(re));
			expect_true!(value.im.contains(im));
		}
		Ok(())
	}
	#[gtest]
	fn circle_bound_admits_a_contracting_target_whose_coefficient_sum_exceeds_one()
	-> googletest::Result<()> {
		let target = [
			Complex64::new(0.4, 0.0),
			Complex64::new(0.4, 0.0),
			Complex64::new(-0.4, 0.0),
		];
		let upper = contractivity(&target, Policy::default())?;
		expect_that!(upper, lt(1.0));
		expect_that!(upper, ge(0.4 * 5.0_f64.sqrt()));
		Ok(())
	}
}
