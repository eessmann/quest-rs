//! Fixed protocol arithmetic and independent analytic readout.
#![allow(
	clippy::arithmetic_side_effects,
	reason = "All dimensions and scalar terms are fixed bounded protocol values"
)]
use quest::Complex64;
use quest_numerics::{Interval, sparse_stream::SparseEntry};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const N: usize = 32;
pub const RANK_BYTES: usize = 64 * 1024 * 1024;
pub const NODE_BYTES: usize = 512 * 1024 * 1024;
pub const LOAD_BYTES: usize = 1024 * 1024;
pub const BRIDGE_BYTES: usize = 4 * 1024 * 1024;
pub const COMPILE_BYTES: usize = 48 * 1024 * 1024;
pub const READOUT_BYTES: usize = 1024 * 1024;
pub const READOUT_WORK: usize = 4_000_000;
pub const TOLERANCE: f64 = 1e-4;
pub const MAX_DEGREE: usize = 81;
pub const WEIGHTS: [f64; 3] = [0.25, 0.25, -0.125];
pub fn coefficient(term: usize, slot: usize) -> Result<Complex64> {
	match (term, slot) {
		(0 | 1, 0) => Ok(Complex64::new(1., 0.)),
		(2, 0) => Ok(Complex64::new(-1., 0.)),
		(0 | 2, 1) => Ok(Complex64::new(0., 1.)),
		(1, 1) => Ok(Complex64::new(0., -1.)),
		_ => Err("fixed term/slot".into()),
	}
}
pub fn entries(
	n: usize,
	term: usize,
	rank: usize,
	parts: usize,
) -> Result<impl Iterator<Item = quest_numerics::Result<SparseEntry>>> {
	if !matches!(n, 4 | 32) || term >= 3 || !matches!(parts, 1 | 2 | 4 | 8) || rank >= parts {
		return Err("fixed source ownership".into());
	}
	let values = [coefficient(term, 0)?, coefficient(term, 1)?];
	Ok((rank..n).step_by(parts).flat_map(move |column| {
		values.into_iter().enumerate().map(move |(slot, value)| {
			Ok(SparseEntry {
				row: column ^ slot,
				column,
				ordinal: u64::try_from(2 * column + slot)
					.map_err(|_| quest_numerics::Error::Overflow)?,
				value,
			})
		})
	}))
}
pub fn targets(n: usize) -> Result<Vec<usize>> {
	match n {
		32 => Ok(vec![0, 9, 8, 7, 6, 5, 1]),
		4 => Ok(vec![0, 7, 6, 1]),
		_ => Err("fixed system width".into()),
	}
}
#[cfg(test)]
pub fn system_index(j: usize, targets: &[usize]) -> Result<usize> {
	let width = targets.len().checked_sub(2).ok_or("matching width")?;
	if width > 5 || j >= (1 << width) {
		return Err("system index".into());
	}
	let mut result = 0;
	for (bit, &target) in targets.iter().skip(1).take(width).enumerate() {
		if target >= 10 {
			return Err("physical target".into());
		}
		result |= ((j >> bit) & 1) << target;
	}
	Ok(result)
}
pub fn decode_system(physical: usize, targets: &[usize]) -> Result<usize> {
	let width = targets.len().checked_sub(2).ok_or("matching width")?;
	if width > 5 {
		return Err("system width".into());
	}
	let mut j = 0;
	for (bit, &target) in targets.iter().skip(1).take(width).enumerate() {
		if target >= 10 {
			return Err("physical target".into());
		}
		j |= ((physical >> target) & 1) << bit;
	}
	Ok(j)
}
pub fn spectrum() -> Result<(f64, f64)> {
	let bound = Interval::point(26.)?
		.sqrt()?
		.checked_div(Interval::point(8.)?)?;
	Ok((bound.lower(), bound.upper()))
}
pub fn chebyshev(coefficients: &[f64], x: f64) -> Result<f64> {
	if coefficients.is_empty()
		|| coefficients.len() > 82
		|| !x.is_finite()
		|| x.abs() > 1.
		|| coefficients.iter().any(|c| !c.is_finite())
	{
		return Err("finite frozen polynomial".into());
	}
	let mut sum = *coefficients.first().ok_or("constant coefficient")?;
	let mut previous = 1.;
	let mut current = x;
	for (index, &c) in coefficients.iter().enumerate().skip(1) {
		if index > 1 {
			let next = (2. * x).mul_add(current, -previous);
			previous = current;
			current = next;
		}
		sum = c.mul_add(current, sum);
	}
	if !sum.is_finite() {
		return Err("polynomial overflow".into());
	}
	Ok(sum)
}
pub fn inverse_entry(j: usize, adjoint: bool) -> Complex64 {
	match j {
		0 => Complex64::new(20. / 13., 0.),
		1 => Complex64::new(0., if adjoint { 4. / 13. } else { -4. / 13. }),
		_ => Complex64::new(0., 0.),
	}
}
pub fn residual_entry(
	x: Complex64,
	partner: Complex64,
	j: usize,
	adjoint: bool,
) -> Result<Complex64> {
	if j >= 32
		|| !x.re.is_finite()
		|| !x.im.is_finite()
		|| !partner.re.is_finite()
		|| !partner.im.is_finite()
	{
		return Err("finite full-coordinate residual".into());
	}
	Ok(
		0.625 * x + Complex64::new(0., if adjoint { -0.125 } else { 0.125 }) * partner
			- Complex64::new(f64::from(j == 0), 0.),
	)
}
