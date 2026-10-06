//! Bounded dense reference for the stored history; never a quantum fallback.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Dimensions and cubic work admitted before bounded reference elimination"
)]
use crate::{CfdError, history::HistorySystem};
use quest_numerics::{Complex64, SparseLimits};
/// Independent classical-reference admission, separate from quantum execution.
#[derive(Clone, Copy, Debug)]
pub struct ReferenceBudget {
	pub max_dimension: usize,
	pub max_bytes: usize,
	pub max_work: usize,
	pub relative_residual: f64,
}
impl Default for ReferenceBudget {
	fn default() -> Self {
		Self {
			max_dimension: 512,
			max_bytes: 128 * 1024 * 1024,
			max_work: 1_073_741_824,
			relative_residual: 1e-10,
		}
	}
}
/// Executed dense reference with its measured sparse residual and modeled cost.
#[derive(Clone, Debug, serde::Serialize)]
pub struct HistoryReference {
	pub status: String,
	#[serde(skip)]
	pub solution: Vec<Complex64>,
	pub relative_residual: f64,
	pub modeled_peak_bytes: usize,
	pub modeled_work: usize,
}
fn zeroes(n: usize) -> Result<Vec<Complex64>, CfdError> {
	let mut out = Vec::new();
	out.try_reserve_exact(n)
		.map_err(|_| CfdError::InvalidInput("classical history allocation"))?;
	out.resize(n, Complex64::new(0., 0.));
	Ok(out)
}
/// Partial-pivot Gaussian elimination of the directly stored complex history.
///
/// This explicitly bounded independent validation path forms a dense matrix.
/// It supplies no singular-value or forward-error certificate, and is never
/// called as a fallback by the quantum solver. Original sparse entries are used
/// for an independent measured residual after elimination.
/// # Errors
/// Rejects dimension, simultaneous storage/work, singular pivots, nonfinite
/// arithmetic and measured residual failure before returning a solution.
#[allow(
	clippy::too_many_lines,
	reason = "One bounded reference transaction retains admission and residual checks"
)]
pub fn solve_reference(
	history: &HistorySystem,
	budget: ReferenceBudget,
) -> Result<HistoryReference, CfdError> {
	let n = history.operator().rows();
	if n == 0
		|| n > budget.max_dimension
		|| n > 2048
		|| !budget.relative_residual.is_finite()
		|| budget.relative_residual <= 0.
		|| budget.relative_residual >= 1.
	{
		return Err(CfdError::InvalidInput(
			"classical history dimension/tolerance admission",
		));
	}
	let square = n.checked_mul(n).ok_or(CfdError::InvalidInput(
		"classical history dimension overflow",
	))?;
	let work = square
		.checked_mul(n)
		.and_then(|x| x.checked_mul(8))
		.and_then(|x| x.checked_add(history.operator().nnz().checked_mul(16)?))
		.ok_or(CfdError::InvalidInput("classical history work overflow"))?;
	let peak = square
		.checked_add(
			n.checked_mul(4)
				.ok_or(CfdError::InvalidInput("classical history storage overflow"))?,
		)
		.and_then(|x| x.checked_mul(size_of::<Complex64>()))
		.and_then(|x| x.checked_add(history.retained_bytes().ok()?))
		.and_then(|x| x.checked_add(4 * size_of::<Vec<Complex64>>()))
		.ok_or(CfdError::InvalidInput("classical history storage overflow"))?;
	if work > budget.max_work || peak > budget.max_bytes {
		return Err(CfdError::InvalidInput(
			"classical history work/storage budget",
		));
	}
	let mut a = zeroes(square)?;
	let mut b = zeroes(n)?;
	b.copy_from_slice(history.rhs());
	for (row, col, value) in history.operator().entries() {
		a[row * n + col] = value;
	}
	for k in 0..n {
		let pivot = (k..n)
			.max_by(|&i, &j| a[i * n + k].norm().total_cmp(&a[j * n + k].norm()))
			.ok_or(CfdError::Assembly("empty pivot"))?;
		if a[pivot * n + k].norm() == 0. {
			return Err(CfdError::Assembly("singular classical history"));
		}
		if pivot != k {
			for j in k..n {
				a.swap(k * n + j, pivot * n + j);
			}
			b.swap(k, pivot);
		}
		for i in k + 1..n {
			let factor = a[i * n + k] / a[k * n + k];
			a[i * n + k] = Complex64::new(0., 0.);
			for j in k + 1..n {
				let value = a[k * n + j];
				a[i * n + j] -= factor * value;
			}
			let value = b[k];
			b[i] -= factor * value;
		}
	}
	let mut solution = zeroes(n)?;
	for i in (0..n).rev() {
		let mut value = b[i];
		for j in i + 1..n {
			value -= a[i * n + j] * solution[j];
		}
		solution[i] = value / a[i * n + i];
		if !solution[i].re.is_finite() || !solution[i].im.is_finite() {
			return Err(CfdError::Assembly("nonfinite classical history solution"));
		}
	}
	drop(a);
	drop(b);
	let applied = history.operator().matvec(
		&solution,
		SparseLimits {
			max_dimension: budget.max_dimension,
			max_entries: history.operator().nnz(),
			max_bytes: budget
				.max_bytes
				.checked_sub(
					history
						.retained_bytes()?
						.checked_sub(history.operator().retained_bytes()?)
						.ok_or(CfdError::InvalidInput("classical history accounting"))?,
				)
				.ok_or(CfdError::InvalidInput("classical history accounting"))?,
			max_work: budget.max_work,
		},
	)?;
	let rhs_norm = history.rhs().iter().fold(0_f64, |s, z| s.hypot(z.norm()));
	let residual = applied
		.iter()
		.zip(history.rhs())
		.fold(0_f64, |s, (a, b)| s.hypot((*a - *b).norm()));
	let relative = if rhs_norm == 0. {
		residual
	} else {
		residual / rhs_norm
	};
	if !relative.is_finite() || relative > budget.relative_residual {
		return Err(CfdError::Assembly(
			"classical history residual exceeds tolerance",
		));
	}
	Ok(HistoryReference {status:"bounded dense classical history reference; no quantum execution or spectral certificate".into(),solution,relative_residual:relative,modeled_peak_bytes:peak,modeled_work:work})
}
