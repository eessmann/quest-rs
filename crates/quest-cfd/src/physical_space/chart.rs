use super::{basis::Basis, dot};
use crate::{CfdError, simplex::Cell};

pub(super) fn mass_apply(
	cells: &[Cell],
	dimension: usize,
	basis: &Basis,
	state: &[f64],
) -> Vec<f64> {
	let scalar = basis.nodes.len();
	let local = dimension * scalar;
	let mut output = vec![0.; state.len()];
	for (ci, cell) in cells.iter().enumerate() {
		for component in 0..dimension {
			let offset = ci * local + component * scalar;
			output[offset..offset + scalar]
				.copy_from_slice(&basis.apply_mass(&state[offset..offset + scalar], cell.volume));
		}
	}
	output
}
/// Rank-revealing twice-orthogonalized row elimination in whitened coordinates.
/// This is a bounded dense reference, deliberately independent of the distributed QR path.
pub(super) fn complete_chart(
	rows: &[Vec<f64>],
	cells: &[Cell],
	dimension: usize,
	basis: &Basis,
) -> Result<Vec<Vec<f64>>, CfdError> {
	let scalar = basis.nodes.len();
	let local = dimension * scalar;
	let size = cells.len() * local;
	let mut residual = Vec::new();
	for row in rows {
		let mut white = vec![0.; size];
		for (ci, cell) in cells.iter().enumerate() {
			for component in 0..dimension {
				let offset = ci * local + component * scalar;
				white[offset..offset + scalar]
					.copy_from_slice(&basis.whiten_row(&row[offset..offset + scalar], cell.volume));
			}
		}
		let norm = dot(&white, &white).sqrt();
		if !norm.is_finite() || norm == 0. {
			return Err(CfdError::Assembly("zero/invalid complete constraint row"));
		}
		for v in &mut white {
			*v /= norm;
		}
		residual.push(white);
	}
	let mut row_basis: Vec<Vec<f64>> = Vec::new();
	while row_basis.len() < residual.len() {
		let rank = row_basis.len();
		let pivot = (rank..residual.len())
			.max_by(|&i, &j| {
				dot(&residual[i], &residual[i]).total_cmp(&dot(&residual[j], &residual[j]))
			})
			.ok_or(CfdError::Assembly("empty rank pivot"))?;
		let norm = dot(&residual[pivot], &residual[pivot]).sqrt();
		if norm <= 1e-11 {
			break;
		}
		if norm < 1e-8 {
			return Err(CfdError::Assembly("ambiguous complete constraint rank"));
		}
		residual.swap(rank, pivot);
		let mut direction = residual[rank].clone();
		for value in &mut direction {
			*value /= norm;
		}
		for row in residual.iter_mut().skip(rank + 1) {
			for _ in 0..2 {
				let projection = dot(row, &direction);
				for (value, basis_value) in row.iter_mut().zip(&direction) {
					*value -= projection * basis_value;
				}
			}
		}
		row_basis.push(direction);
	}
	let expected = size.checked_sub(row_basis.len()).ok_or(CfdError::Assembly(
		"constraint rank exceeds velocity dimension",
	))?;
	if expected == 0 {
		return Err(CfdError::Assembly("full space has no velocity kernel"));
	}
	let mut white_chart: Vec<Vec<f64>> = Vec::new();
	for axis in 0..size {
		let mut direction = vec![0.; size];
		direction[axis] = 1.;
		for _ in 0..2 {
			for row in row_basis.iter().chain(&white_chart) {
				let projection = dot(&direction, row);
				for (v, r) in direction.iter_mut().zip(row) {
					*v -= projection * r;
				}
			}
		}
		let norm = dot(&direction, &direction).sqrt();
		if norm < 1e-11 {
			continue;
		}
		if norm < 1e-8 {
			return Err(CfdError::Assembly("ambiguous complete nullspace rank"));
		}
		for v in &mut direction {
			*v /= norm;
		}
		white_chart.push(direction);
	}
	if white_chart.len() != expected {
		return Err(CfdError::Assembly("incomplete physical kernel"));
	}
	let mut chart = Vec::new();
	for direction in white_chart {
		let mut physical = vec![0.; size];
		for (ci, cell) in cells.iter().enumerate() {
			for component in 0..dimension {
				let offset = ci * local + component * scalar;
				physical[offset..offset + scalar].copy_from_slice(
					&basis.unwhiten(&direction[offset..offset + scalar], cell.volume),
				);
			}
		}
		chart.push(physical);
	}
	Ok(chart)
}
