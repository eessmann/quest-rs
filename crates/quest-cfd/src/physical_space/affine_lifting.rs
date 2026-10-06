//! Admitted minimum-mass particular fields using the shared scaled row QR.
use super::{
	CfdError, PhysicalGeometryKind, PhysicalSpace, Point, constraint_qr::RowQr, dot, mass_apply,
	reserved,
};
const fn invalid() -> CfdError {
	CfdError::InvalidInput("canonical mixed lifting shape/work/storage admission")
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(invalid)
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(invalid)
}
/// Complete batch admission; work never resets for successive time modes.
#[derive(Clone, Copy, Debug)]
pub struct CanonicalLiftingLimits {
	pub max_modes: usize,
	pub max_bytes: usize,
	pub max_work: usize,
	/// Other caller-owned storage live during this query, in addition to mesh-declared external storage.
	pub external_retained_bytes: usize,
}
impl Default for CanonicalLiftingLimits {
	fn default() -> Self {
		Self {
			max_modes: 9,
			max_bytes: 256 * 1024 * 1024,
			max_work: 1_000_000_000,
			external_retained_bytes: 0,
		}
	}
}
/// Managed whole-live payload allowance, not an allocator or RSS ceiling.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct CanonicalLiftingResources {
	pub borrowed_space_bytes: usize,
	pub external_retained_bytes: usize,
	pub input_capacity_bytes: usize,
	pub output_capacity_bytes: usize,
	pub scratch_bytes: usize,
	pub peak_bytes: usize,
	pub work: usize,
}
/// Every original broken coefficient of the minimum-mass particular fields.
#[derive(Debug)]
pub struct CanonicalLiftings {
	pub coefficients: Vec<Vec<f64>>,
	pub resources: CanonicalLiftingResources,
	/// Largest original `C ell - d` residual over the full batch.
	pub constraint_residual: f64,
	/// Largest `Q^T M ell` coefficient over the full batch.
	pub mass_orthogonality_residual: f64,
}
impl PhysicalSpace {
	/// Solve all Dirichlet normal data in one admitted, full-coordinate batch.
	///
	/// Input is time-mode, `dirichlet_facets()` order, complete facet node order.
	/// Tangential traces remain weak data. This particular lifting does not constrain
	/// natural velocity traces. Explicit supplied liftings remain supported separately.
	/// The QR acts on `C B`, where `B^T M B=I`; no normal equations are formed.
	/// # Errors
	/// Rejects non-mixed owners, malformed/nonfinite traces, limits, ambiguous QR,
	/// or failed original constraint/minimum-mass numerical residual checks.
	#[allow(
		clippy::ptr_arg,
		clippy::too_many_lines,
		clippy::many_single_char_names,
		reason = "One batch ledger and QR solve uses conventional N/R/K dimensions and counts actual nested caller capacities"
	)]
	pub fn canonical_liftings(
		&self,
		traces: &Vec<Vec<Vec<Point>>>,
		limits: CanonicalLiftingLimits,
	) -> Result<CanonicalLiftings, CfdError> {
		let k = traces.len();
		let n = self.diagnostics.local_velocity_dimension;
		let r = self.constraints.len();
		let s = self.basis.nodes.len();
		let fd = self
			.face_layout
			.iter()
			.filter(|f| f.dirichlet.is_some())
			.count();
		let t = if self.order == 1 {
			self.dimension
		} else {
			self.dimension * (self.dimension + 1) / 2
		};
		if self.geometry_kind() != PhysicalGeometryKind::MixedAffineMesh
			|| k == 0
			|| k > limits.max_modes
			|| k > 65
			|| r != self.diagnostics.constraint_rank
		{
			return Err(invalid());
		}
		let rn = mul(r, n)?;
		let rr = mul(r, r)?;
		let nn = mul(n, n)?;
		let work = add(
			add(mul(128, mul(rr, n)?)?, mul(64, mul(rn, s)?)?)?,
			mul(
				mul(128, k)?,
				add(add(rr, rn)?, add(nn, mul(mul(fd, t)?, self.dimension)?)?)?,
			)?,
		)?;
		let borrowed_space_bytes = self.retained_bytes()?;
		let external = add(
			limits.external_retained_bytes,
			self.mesh_metadata
				.as_ref()
				.map_or(0, |m| m.limits.external_retained_bytes),
		)?;
		let mut input = mul(traces.capacity(), size_of::<Vec<Vec<Point>>>())?;
		if work > limits.max_work
			|| add(add(borrowed_space_bytes, external)?, input)? > limits.max_bytes
		{
			return Err(invalid());
		}
		for mode in traces {
			if mode.len() != fd {
				return Err(invalid());
			}
			input = add(input, mul(mode.capacity(), size_of::<Vec<Point>>())?)?;
			for face in mode {
				if face.len() != t {
					return Err(invalid());
				}
				input = add(input, mul(face.capacity(), size_of::<Point>())?)?;
			}
		}
		let words = add(add(mul(3, rn)?, rr)?, add(mul(32, n)?, mul(8, r)?)?)?;
		let scratch = add(
			mul(8, words)?,
			add(
				mul(add(mul(4, r)?, mul(4, n)?)?, size_of::<Vec<f64>>())?,
				4096,
			)?,
		)?;
		let output = add(
			size_of::<CanonicalLiftings>(),
			add(mul(k, size_of::<Vec<f64>>())?, mul(8, mul(k, n)?)?)?,
		)?;
		let peak = add(
			add(add(add(borrowed_space_bytes, external)?, input)?, scratch)?,
			output,
		)?;
		if peak > limits.max_bytes {
			return Err(invalid());
		}
		if traces
			.iter()
			.flatten()
			.flatten()
			.any(|p| p.iter().any(|v| !v.is_finite()) || (self.dimension == 2 && p[2] != 0.))
		{
			return Err(invalid());
		}
		let mut rows = reserved(r)?;
		for row in &self.constraints {
			let mut white = vec![0.; n];
			for (ci, cell) in self.cells.iter().enumerate() {
				for axis in 0..self.dimension {
					let start = (ci * self.dimension + axis) * s;
					white[start..start + s].copy_from_slice(
						&self.basis.whiten_row(&row[start..start + s], cell.volume),
					);
				}
			}
			rows.push(white);
		}
		let factor = RowQr::new(&rows)?;
		let factor_bytes = add(
			factor.retained_bytes()?,
			rows.iter().try_fold(
				mul(rows.capacity(), size_of::<Vec<f64>>())?,
				|total, row| add(total, mul(row.capacity(), 8)?),
			)?,
		)?;
		// All repeatedly allocated RHS/solve/whitening/mass buffers remain covered
		// before the first batch solve; check each actual returned capacity below.
		let mode_reserve = add(mul(8, add(mul(8, n)?, mul(4, r)?)?)?, 1024)?;
		if add(factor_bytes, mode_reserve)? > scratch {
			return Err(invalid());
		}
		let mut coefficients = reserved(k)?;
		let mut constraint_residual = 0_f64;
		let mut mass_orthogonality_residual = 0_f64;
		for mode in traces {
			let mut rhs = vec![0.; r];
			for (fi, layout) in self.face_layout.iter().enumerate() {
				if let (Some(index), Some(start)) = (layout.dirichlet, layout.normal_start) {
					for (node, value) in mode[index].iter().enumerate() {
						rhs[start + node] = dot(value, &self.faces[fi].normal);
					}
				}
			}
			let white = factor.solve_particular(&rhs, n)?;
			let mut field = vec![0.; n];
			for (ci, cell) in self.cells.iter().enumerate() {
				for axis in 0..self.dimension {
					let start = (ci * self.dimension + axis) * s;
					field[start..start + s].copy_from_slice(
						&self.basis.unwhiten(&white[start..start + s], cell.volume),
					);
				}
			}
			let mass = mass_apply(&self.cells, self.dimension, &self.basis, &field);
			let live = [&rhs, &white, &field, &mass]
				.iter()
				.try_fold(factor_bytes, |total, v| add(total, mul(v.capacity(), 8)?))?;
			if add(live, mul(8, add(r, mul(2, s)?)?)?)? > scratch {
				return Err(invalid());
			}
			let c_error = self
				.constraints
				.iter()
				.zip(&rhs)
				.map(|(row, b)| (dot(row, &field) - b).abs())
				.try_fold(0_f64, |a, b| {
					if b.is_finite() {
						Ok(a.max(b))
					} else {
						Err(invalid())
					}
				})?;
			let q_error =
				self.chart
					.iter()
					.map(|q| dot(q, &mass).abs())
					.try_fold(0_f64, |a, b| {
						if b.is_finite() {
							Ok(a.max(b))
						} else {
							Err(invalid())
						}
					})?;
			let scale = rhs.iter().map(|x| x.abs()).fold(1_f64, f64::max);
			let energy = dot(&field, &mass).abs().sqrt().max(1.);
			if field.iter().chain(&mass).any(|v| !v.is_finite())
				|| !c_error.is_finite()
				|| !q_error.is_finite()
				|| !energy.is_finite()
				|| c_error > 1e-8 * scale
				|| q_error > 1e-8 * energy
			{
				return Err(CfdError::Assembly(
					"canonical lifting numerical residual failed",
				));
			}
			constraint_residual = constraint_residual.max(c_error);
			mass_orthogonality_residual = mass_orthogonality_residual.max(q_error);
			coefficients.push(field);
		}
		let actual = coefficients.iter().try_fold(
			add(
				size_of::<CanonicalLiftings>(),
				mul(coefficients.capacity(), size_of::<Vec<f64>>())?,
			)?,
			|sum, c| add(sum, mul(c.capacity(), 8)?),
		)?;
		if actual > output {
			return Err(invalid());
		}
		Ok(CanonicalLiftings {
			coefficients,
			resources: CanonicalLiftingResources {
				borrowed_space_bytes,
				external_retained_bytes: external,
				input_capacity_bytes: input,
				output_capacity_bytes: actual,
				scratch_bytes: scratch,
				peak_bytes: peak,
				work,
			},
			constraint_residual,
			mass_orthogonality_residual,
		})
	}
}
