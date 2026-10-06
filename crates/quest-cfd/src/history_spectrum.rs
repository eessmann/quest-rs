//! Explicitly bounded dense inverse candidate with an independent interval proof.
//!
//! This small-reference path is opt-in. It never supplies a classical solution
//! to the quantum solver or changes its directly encoded history operator.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Checked square/cubic admission bounds all dense reference indices"
)]
use crate::{CfdError, history::HistorySystem};
use quest_numerics::{Complex64, Interval, SparseLimits, SparseMatrix};
use quest_qsvt::reciprocal::{SpectralBounds, SpectralEvidence};

/// Explicit limits for classical spectral evidence; independent of circuit costs.
#[derive(Clone, Copy, Debug)]
pub struct ReferenceSpectrumBudget {
	pub max_dimension: usize,
	pub max_bytes: usize,
	pub max_work: usize,
}
impl Default for ReferenceSpectrumBudget {
	fn default() -> Self {
		Self {
			max_dimension: 512,
			max_bytes: 128 * 1024 * 1024,
			max_work: 1_073_741_824,
		}
	}
}

/// Outward bounds apply to the exact stored binary64 matrix, not the continuum.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ReferenceSpectrumReport {
	pub status: &'static str,
	pub history_dimension: usize,
	pub temporal_order: usize,
	pub residual_norm_upper: f64,
	pub inverse_candidate_norm_upper: f64,
	pub spectral_lower: f64,
	pub spectral_upper: f64,
	/// Includes the complete borrowed history and simultaneous admitted workspaces.
	/// Excludes allocator bookkeeping and preceding physical/lift construction.
	pub modeled_peak_bytes: usize,
	/// Conservative scalar-operation envelope, not measured CPU instructions.
	pub modeled_work: usize,
	pub seconds: f64,
}

fn filled<T: Clone>(length: usize, value: T) -> Result<Vec<T>, CfdError> {
	let mut out = Vec::new();
	out.try_reserve_exact(length)
		.map_err(|_| CfdError::InvalidInput("reference spectral allocation"))?;
	out.resize(length, value);
	Ok(out)
}

/// Construct X approximately; its accuracy is not trusted by the verifier.
fn inverse_candidate(operator: &SparseMatrix) -> Result<Vec<Complex64>, CfdError> {
	let n = operator.rows();
	let mut a = filled(n * n, Complex64::from(0.))?;
	let mut x = filled(n * n, Complex64::from(0.))?;
	for (i, j, value) in operator.entries() {
		a[i * n + j] = value;
	}
	for i in 0..n {
		x[i * n + i] = Complex64::from(1.);
	}
	for k in 0..n {
		let pivot = (k..n)
			.max_by(|&i, &j| a[i * n + k].norm().total_cmp(&a[j * n + k].norm()))
			.ok_or(CfdError::Assembly("empty reference spectral pivot"))?;
		let diagonal = a[pivot * n + k];
		if diagonal.norm() == 0. || !diagonal.norm().is_finite() {
			return Err(CfdError::Assembly(
				"singular/nonfinite reference spectral pivot",
			));
		}
		if pivot != k {
			for j in 0..n {
				a.swap(k * n + j, pivot * n + j);
				x.swap(k * n + j, pivot * n + j);
			}
		}
		for j in 0..n {
			a[k * n + j] /= diagonal;
			x[k * n + j] /= diagonal;
		}
		for i in 0..n {
			if i == k {
				continue;
			}
			let factor = a[i * n + k];
			for j in 0..n {
				let aj = a[k * n + j];
				let xj = x[k * n + j];
				a[i * n + j] -= factor * aj;
				x[i * n + j] -= factor * xj;
			}
		}
	}
	if x.iter().any(|z| !z.re.is_finite() || !z.im.is_finite()) {
		return Err(CfdError::Assembly("nonfinite reference inverse candidate"));
	}
	Ok(x)
}

fn modulus(real: Interval, imag: Interval) -> Result<Interval, CfdError> {
	Ok(real.square()?.checked_add(imag.square()?)?.sqrt()?)
}

/// Independent posterior check: no elimination factors or pivot errors enter.
/// rho>=||I-XH||_2 and chi>=||X||_2 imply sigma_min(H)>=(1-rho)/chi.
fn check_inverse(operator: &SparseMatrix, x: &[Complex64]) -> Result<(f64, f64, f64), CfdError> {
	let n = operator.rows();
	if x.len() != n * n || x.iter().any(|z| !z.re.is_finite() || !z.im.is_finite()) {
		return Err(CfdError::InvalidInput("reference inverse shape/value"));
	}
	let point = Interval::point;
	let zero = point(0.)?;
	let mut residual_columns = filled(n, zero)?;
	let mut inverse_columns = filled(n, zero)?;
	let mut row = filled(n, (zero, zero))?;
	let mut residual_max_row = 0.0_f64;
	let mut inverse_max_row = 0.0_f64;
	for i in 0..n {
		row.fill((zero, zero));
		row[i].0 = point(-1.)?;
		let mut inverse_sum = zero;
		for k in 0..n {
			let real = point(x[i * n + k].re)?;
			let imag = point(x[i * n + k].im)?;
			let abs = modulus(real, imag)?;
			inverse_sum = inverse_sum.checked_add(abs)?;
			inverse_columns[k] = inverse_columns[k].checked_add(abs)?;
			for (j, value) in operator.row(k) {
				let br = point(value.re)?;
				let bi = point(value.im)?;
				row[j].0 = row[j]
					.0
					.checked_add(real.checked_mul(br)?.checked_sub(imag.checked_mul(bi)?)?)?;
				row[j].1 = row[j]
					.1
					.checked_add(real.checked_mul(bi)?.checked_add(imag.checked_mul(br)?)?)?;
			}
		}
		let mut residual_sum = zero;
		for j in 0..n {
			let abs = modulus(row[j].0, row[j].1)?;
			residual_sum = residual_sum.checked_add(abs)?;
			residual_columns[j] = residual_columns[j].checked_add(abs)?;
		}
		residual_max_row = residual_max_row.max(residual_sum.upper());
		inverse_max_row = inverse_max_row.max(inverse_sum.upper());
	}
	let residual_max_column = residual_columns
		.iter()
		.map(|v| v.upper())
		.fold(0.0, f64::max);
	let inverse_max_column = inverse_columns
		.iter()
		.map(|v| v.upper())
		.fold(0.0, f64::max);
	let rho = point(residual_max_row)?
		.checked_mul(point(residual_max_column)?)?
		.sqrt()?
		.upper();
	let chi = point(inverse_max_row)?
		.checked_mul(point(inverse_max_column)?)?
		.sqrt()?
		.upper();
	if rho >= 1. || chi <= 0. {
		return Err(CfdError::Assembly(
			"reference spectral residual is inconclusive",
		));
	}
	let lower = point(1.)?
		.checked_sub(point(rho)?)?
		.checked_div(point(chi)?)?
		.lower();
	if lower <= 0. {
		return Err(CfdError::Assembly(
			"reference spectral lower bound underflow",
		));
	}
	Ok((lower, rho, chi))
}

/// Certify positive singular-value bounds for an explicitly small stored history.
///
/// X is constructed classically and discarded before quantum preparation. The
/// interval residual proves the bound even for nonnormal complex histories.
/// No H-adjoint H, SVD or physical solution is formed. This is not a scalable
/// spectral algorithm and is never used automatically by `solve_history`.
/// # Errors
/// Rejects dimension/work/storage limits, singular/nonfinite candidates and
/// inconclusive interval proofs. Reference allocations precede no quantum state.
pub fn reference_spectrum(
	history: &HistorySystem,
	budget: ReferenceSpectrumBudget,
) -> Result<(SpectralBounds, ReferenceSpectrumReport), CfdError> {
	let started = std::time::Instant::now();
	let n = history.operator().rows();
	if n == 0 || n > budget.max_dimension || n > 2048 {
		return Err(CfdError::InvalidInput(
			"reference spectral dimension admission",
		));
	}
	let square = n.checked_mul(n).ok_or(CfdError::InvalidInput(
		"reference spectral dimension overflow",
	))?;
	let work = square
		.checked_mul(n)
		.and_then(|v| v.checked_mul(64))
		.and_then(|v| v.checked_add(n.checked_mul(history.operator().nnz())?.checked_mul(32)?))
		.and_then(|v| v.checked_add(square.checked_mul(32)?))
		.ok_or(CfdError::InvalidInput("reference spectral work overflow"))?;
	let peak = square
		.checked_mul(2 * size_of::<Complex64>())
		.and_then(|v| v.checked_add(n.checked_mul(8 * size_of::<Interval>())?))
		.and_then(|v| v.checked_add(history.retained_bytes().ok()?))
		.and_then(|v| v.checked_add(4096))
		.ok_or(CfdError::InvalidInput(
			"reference spectral storage overflow",
		))?;
	if work > budget.max_work || peak > budget.max_bytes {
		return Err(CfdError::InvalidInput(
			"reference spectral work/storage admission",
		));
	}
	let x = inverse_candidate(history.operator())?;
	let (lower, rho, chi) = check_inverse(history.operator(), &x)?;
	drop(x);
	let upper = history
		.operator()
		.norms(SparseLimits {
			max_bytes: budget
				.max_bytes
				.checked_sub(
					history
						.retained_bytes()?
						.checked_sub(history.operator().retained_bytes()?)
						.ok_or(CfdError::InvalidInput("reference spectral accounting"))?,
				)
				.ok_or(CfdError::InvalidInput("reference spectral accounting"))?,
			max_dimension: budget.max_dimension,
			max_work: budget.max_work,
			max_entries: history.operator().nnz(),
		})?
		.spectral_upper_bound;
	let spectrum = SpectralBounds::new(lower,upper,SpectralEvidence::DenseReference {
		dimension:n,description:"Explicit bounded binary64 inverse candidate; directed complex interval residual I-XH, induced 1/infinity norm bounds and Neumann inverse theorem; no normality assumption or normal equations".into(),
	})?;
	let report = ReferenceSpectrumReport {
		status: "bounded classical spectral certificate for the stored history; no quantum execution",
		history_dimension: n,
		temporal_order: history.temporal_order(),
		residual_norm_upper: rho,
		inverse_candidate_norm_upper: chi,
		spectral_lower: lower,
		spectral_upper: upper,
		modeled_peak_bytes: peak,
		modeled_work: work,
		seconds: started.elapsed().as_secs_f64(),
	};
	Ok((spectrum, report))
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	#[allow(
		clippy::panic_in_result_fn,
		reason = "Independent rejecting certificate assertions in a bounded unit test"
	)]
	fn posterior_checker_rejects_wrong_inverse_and_nonfinite_values() -> Result<(), CfdError> {
		let a = SparseMatrix::from_triplets(
			1,
			1,
			quest_numerics::SparseFormat::Csr,
			vec![(0, 0, Complex64::from(2.))],
			SparseLimits::default(),
		)?;
		assert!(check_inverse(&a, &[Complex64::from(0.)]).is_err());
		assert!(check_inverse(&a, &[Complex64::from(1.)]).is_err());
		assert!(check_inverse(&a, &[Complex64::from(f64::NAN)]).is_err());
		assert!(check_inverse(&a, &[]).is_err());
		let (lower, rho, _) = check_inverse(&a, &[Complex64::from(0.5)])?;
		assert!(lower <= 2. && lower > 1.99 && rho < 1e-12);
		Ok(())
	}
}
