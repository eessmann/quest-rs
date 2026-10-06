//! Shared scaled row QR for admitted original-pressure and particular-lifting solves.
use super::{CfdError, dot, reserved};
pub(super) struct RowQr {
	basis: Vec<Vec<f64>>,
	triangular: Vec<Vec<f64>>,
	scales: Vec<f64>,
}
impl RowQr {
	pub fn new(rows: &[Vec<f64>]) -> Result<Self, CfdError> {
		let mut basis: Vec<Vec<f64>> = reserved(rows.len())?;
		let mut triangular = reserved(rows.len())?;
		let mut scales = reserved(rows.len())?;
		for row in rows {
			let scale = dot(row, row).sqrt();
			if !scale.is_finite() || scale <= 0. {
				return Err(CfdError::Assembly("invalid physical pressure row"));
			}
			let mut vector = row.iter().map(|v| v / scale).collect::<Vec<_>>();
			let mut entries = vec![0.; basis.len() + 1];
			for _ in 0..2 {
				for (j, q) in basis.iter().enumerate() {
					let value = dot(&vector, q);
					entries[j] += value;
					for (x, b) in vector.iter_mut().zip(q) {
						*x -= value * b;
					}
				}
			}
			let length = dot(&vector, &vector).sqrt();
			if !length.is_finite() || length < 1e-10 {
				return Err(CfdError::Assembly("ambiguous physical pressure rank"));
			}
			entries[basis.len()] = length;
			for value in &mut vector {
				*value /= length;
			}
			basis.push(vector);
			triangular.push(entries);
			scales.push(scale);
		}
		Ok(Self {
			basis,
			triangular,
			scales,
		})
	}
	/// Actual retained Vec capacities; allocator metadata is outside the managed model.
	pub fn retained_bytes(&self) -> Result<usize, CfdError> {
		let invalid = || CfdError::InvalidInput("constraint QR capacity overflow");
		let mut bytes = size_of::<Self>()
			.checked_add(self.scales.capacity().checked_mul(8).ok_or_else(invalid)?)
			.ok_or_else(invalid)?;
		for table in [&self.basis, &self.triangular] {
			bytes = bytes
				.checked_add(
					table
						.capacity()
						.checked_mul(size_of::<Vec<f64>>())
						.ok_or_else(invalid)?,
				)
				.ok_or_else(invalid)?;
			for row in table {
				bytes = bytes
					.checked_add(row.capacity().checked_mul(8).ok_or_else(invalid)?)
					.ok_or_else(invalid)?;
			}
		}
		Ok(bytes)
	}

	/// Solve original rows transposed times multipliers equals the supplied force.
	/// With row-normalized `A=T Z^T`, solve `T^T scaled_lambda=Z^T force`.
	pub fn solve_transposed(&self, force: &[f64]) -> Result<Vec<f64>, CfdError> {
		let mut values = self.basis.iter().map(|q| dot(q, force)).collect::<Vec<_>>();
		for i in (0..values.len()).rev() {
			for j in i + 1..values.len() {
				values[i] -= self.triangular[j][i] * values[j];
			}
			values[i] /= self.triangular[i][i];
		}
		for (value, scale) in values.iter_mut().zip(&self.scales) {
			*value /= scale;
		}
		if values.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::Assembly("physical constraint solve overflow"));
		}
		Ok(values)
	}
	/// Minimum Euclidean norm solution of rows times x=rhs.
	/// Solve `T y=rhs/row_scale`, then `x=Z y` (forward, not transpose).
	pub fn solve_particular(&self, rhs: &[f64], width: usize) -> Result<Vec<f64>, CfdError> {
		if rhs.len() != self.scales.len() {
			return Err(CfdError::InvalidInput("particular lifting RHS shape"));
		}
		let mut values = rhs
			.iter()
			.zip(&self.scales)
			.map(|(b, s)| b / s)
			.collect::<Vec<_>>();
		for i in 0..values.len() {
			for j in 0..i {
				values[i] -= self.triangular[i][j] * values[j];
			}
			values[i] /= self.triangular[i][i];
		}
		let mut result = vec![0.; width];
		for (value, q) in values.iter().zip(&self.basis) {
			for (x, b) in result.iter_mut().zip(q) {
				*x += value * b;
			}
		}
		if result.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::Assembly("particular lifting overflow"));
		}
		Ok(result)
	}
}
