//! Cell-local SPD mass whitening. Binary64 factors are numerical evidence, not certificates.
#![allow(
	clippy::arithmetic_side_effects,
	reason = "Checked square block shape bounds all integer indices; binary64 output finiteness is checked"
)]
use crate::{Error, Result};
/// A complete cell block generated only on its physical-row owner.
pub struct CellInput {
	pub cell_id: usize,
	pub dimension: usize,
	/// Row-major SPD mass, dimension squared entries.
	pub mass: Vec<f64>,
	/// Row-major local physical DOF by global constraint column.
	pub constraints_transpose: Vec<f64>,
}
/// Immutable lower Cholesky factor M=LL^T, retaining the consumed allocation.
pub struct CellWhitening {
	dimension: usize,
	lower: Vec<f64>,
}
impl CellWhitening {
	/// # Errors
	/// Rejects shape, actual capacity budget, nonfinite/asymmetric or non-SPD input.
	#[allow(
		clippy::indexing_slicing,
		clippy::float_cmp,
		reason = "All indices are within the validated square block; exact symmetry is an explicit admission contract"
	)]
	pub fn new(dimension: usize, mut mass: Vec<f64>, max_bytes: usize) -> Result<Self> {
		if dimension == 0 || dimension.checked_mul(dimension) != Some(mass.len()) {
			return Err(Error::Length("cell mass"));
		}
		let bytes = mass.capacity().checked_mul(8).ok_or(Error::Overflow)?;
		if bytes > max_bytes {
			return Err(Error::Budget {
				resource: "cell mass capacity",
				requested: bytes,
				limit: max_bytes,
			});
		}
		for i in 0..dimension {
			for j in 0..dimension {
				if !mass[i * dimension + j].is_finite() {
					return Err(Error::NonFinite {
						index: i * dimension + j,
					});
				}
				if mass[i * dimension + j] != mass[j * dimension + i] {
					return Err(Error::Domain("symmetric cell mass"));
				}
			}
		}
		for i in 0..dimension {
			for j in 0..=i {
				let mut a = mass[i * dimension + j];
				for k in 0..j {
					a = (-mass[i * dimension + k]).mul_add(mass[j * dimension + k], a);
				}
				if i == j {
					if a <= 0. || !a.is_finite() {
						return Err(Error::Domain("positive definite cell mass"));
					}
					mass[i * dimension + j] = a.sqrt();
				} else {
					mass[i * dimension + j] = a / mass[j * dimension + j];
				}
				if !mass[i * dimension + j].is_finite() {
					return Err(Error::Domain("finite Cholesky factor"));
				}
			}
		}
		Ok(Self {
			dimension,
			lower: mass,
		})
	}
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.dimension
	}
	#[must_use]
	pub const fn retained_bytes(&self) -> usize {
		self.lower.capacity().saturating_mul(8)
	}
	fn shape(&self, v: &[f64]) -> Result<()> {
		if v.len() != self.dimension {
			return Err(Error::Length("cell vector"));
		}
		if let Some(index) = v.iter().position(|x| !x.is_finite()) {
			return Err(Error::NonFinite { index });
		}
		Ok(())
	}
	/// z=L^T u.
	/// # Errors
	/// Rejects shape or nonfinite output.
	#[allow(clippy::indexing_slicing, reason = "Validated local cell shape")]
	pub fn velocity_to_coordinates(&self, v: &mut [f64]) -> Result<()> {
		self.shape(v)?;
		let d = self.dimension;
		for i in 0..d {
			let mut a = 0.;
			for j in i..d {
				a = self.lower[j * d + i].mul_add(v[j], a);
			}
			v[i] = a;
		}
		self.shape(v)
	}
	/// u=L^-T z.
	/// # Errors
	/// Rejects shape or nonfinite output.
	#[allow(clippy::indexing_slicing, reason = "Validated local cell shape")]
	pub fn coordinates_to_velocity(&self, v: &mut [f64]) -> Result<()> {
		self.shape(v)?;
		let d = self.dimension;
		for i in (0..d).rev() {
			let mut a = v[i];
			for j in i + 1..d {
				a = (-self.lower[j * d + i]).mul_add(v[j], a);
			}
			v[i] = a / self.lower[i * d + i];
		}
		self.shape(v)
	}
	/// h=L^-1 f, also used for each column of C^T.
	/// # Errors
	/// Rejects shape or nonfinite output.
	#[allow(clippy::indexing_slicing, reason = "Validated local cell shape")]
	pub fn force_to_coordinates(&self, v: &mut [f64]) -> Result<()> {
		self.shape(v)?;
		let d = self.dimension;
		for i in 0..d {
			let mut a = v[i];
			for j in 0..i {
				a = (-self.lower[i * d + j]).mul_add(v[j], a);
			}
			v[i] = a / self.lower[i * d + i];
		}
		self.shape(v)
	}
	/// f=L h.
	/// # Errors
	/// Rejects shape or nonfinite output.
	#[allow(clippy::indexing_slicing, reason = "Validated local cell shape")]
	pub fn coordinates_to_force(&self, v: &mut [f64]) -> Result<()> {
		self.shape(v)?;
		let d = self.dimension;
		for i in (0..d).rev() {
			let mut a = 0.;
			for j in 0..=i {
				a = self.lower[i * d + j].mul_add(v[j], a);
			}
			v[i] = a;
		}
		self.shape(v)
	}
}
