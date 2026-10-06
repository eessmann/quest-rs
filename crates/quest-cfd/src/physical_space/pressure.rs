use super::{CfdError, PhysicalSpace, dot, mass_apply};
/// Closed-domain mixed-system recovery with the volume-weighted pressure gauge fixed.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PhysicalPressureRecovery {
	/// Separate admitted affine-mesh pressure-query envelope, absent on old box receipts.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub mesh_resources: Option<super::mesh::PhysicalMeshQueryResources>,
	/// Cell-major P0 constant or P1 vertex-barycentric pressure coefficients.
	pub pressure_coefficients: Vec<Vec<f64>>,
	/// Multipliers for the full polynomial normal trace constraints.
	pub normal_multipliers: Vec<f64>,
	/// Maximum residual of the complete broken momentum equation.
	pub momentum_residual: f64,
	/// Maximum polynomial divergence and normal trace residual.
	pub continuity_residual: f64,
	/// Absolute integral of pressure over the domain.
	pub gauge_residual: f64,
}
/// Whether pressure has a closed gauge or its level is fixed by boundary data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum PressureNormalization {
	ZeroVolumeMean,
	PrescribedMechanicalTraction,
}
/// General original-momentum pressure with explicit normalization semantics.
#[derive(Clone, Debug, serde::Serialize)]
pub struct GeneralPressureRecovery {
	pub mesh_resources: Option<super::mesh::PhysicalMeshQueryResources>,
	pub pressure_coefficients: Vec<Vec<f64>>,
	pub normal_multipliers: Vec<f64>,
	pub momentum_residual: f64,
	pub continuity_residual: f64,
	pub normalization: PressureNormalization,
	pub pressure_integral: f64,
	pub normalization_residual: Option<f64>,
}
impl GeneralPressureRecovery {
	pub(super) fn into_closed(self) -> Result<PhysicalPressureRecovery, CfdError> {
		let gauge_residual = self.normalization_residual.ok_or(CfdError::InvalidInput(
			"natural traction pressure requires the general pressure report",
		))?;
		Ok(PhysicalPressureRecovery {
			mesh_resources: self.mesh_resources,
			pressure_coefficients: self.pressure_coefficients,
			normal_multipliers: self.normal_multipliers,
			momentum_residual: self.momentum_residual,
			continuity_residual: self.continuity_residual,
			gauge_residual,
		})
	}
}
impl PhysicalSpace {
	/// Recover full cell pressure and facet multipliers, with integral pressure zero.
	/// # Errors
	/// Rejects malformed states, ambiguous mixed rank, or failed momentum recovery.
	pub fn reconstruct_pressure(
		&self,
		state: &[f64],
	) -> Result<PhysicalPressureRecovery, CfdError> {
		if self.faces.iter().any(|f| f.outflow) {
			return Err(CfdError::InvalidInput(
				"natural traction pressure requires the general pressure report",
			));
		}
		self.reconstruct_pressure_general(state)?.into_closed()
	}
	/// Recover all pressure coefficients; natural traction fixes their absolute level.
	/// # Errors
	/// Rejects shape, limits, rank ambiguity or failed original momentum recovery.
	pub fn reconstruct_pressure_general(
		&self,
		state: &[f64],
	) -> Result<GeneralPressureRecovery, CfdError> {
		self.admit_mesh_pressure()?;
		let coefficients = self.coefficients(state)?;
		let force = self.force(&coefficients, false)?;
		let acceleration = self.coefficients(&self.drift(state)?)?;
		self.recover_pressure_general(
			&coefficients,
			&acceleration,
			&force,
			self.divergence_residual(state)?
				.max(self.normal_trace_residual(state)?),
		)
	}
	#[allow(
		clippy::too_many_lines,
		reason = "One original-momentum solve keeps closed gauge and prescribed-traction branches together"
	)]
	pub(super) fn recover_pressure_general(
		&self,
		coefficients: &[f64],
		acceleration: &[f64],
		force: &[f64],
		continuity_residual: f64,
	) -> Result<GeneralPressureRecovery, CfdError> {
		let mesh_resources = self.admit_mesh_pressure()?;
		let closed = !self.faces.iter().any(|f| f.outflow);
		let size = self.diagnostics.local_velocity_dimension;
		if [coefficients, acceleration, force]
			.iter()
			.any(|v| v.len() != size || v.iter().any(|x| !x.is_finite()))
			|| !continuity_residual.is_finite()
		{
			return Err(CfdError::InvalidInput(
				"invalid original physical momentum data",
			));
		}
		let mass = mass_apply(&self.cells, self.dimension, &self.basis, acceleration);
		if mass.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::Assembly("pressure acceleration overflow"));
		}
		let residual = force
			.iter()
			.zip(mass)
			.map(|(f, a)| f - a)
			.collect::<Vec<_>>();
		let modes = self.pressure_modes_per_cell();
		let divisor = if self.order == 1 {
			1.
		} else if self.dimension == 2 {
			3.
		} else {
			4.
		};
		let weights = self
			.cells
			.iter()
			.flat_map(|cell| vec![cell.volume / divisor; modes])
			.collect::<Vec<_>>();
		let last = self.constraints.len() - 1;
		let last_weight = weights[weights.len() - 1];
		let mut rows = if closed {
			self.constraints[..last].to_vec()
		} else {
			self.constraints.clone()
		};
		if closed {
			for (i, row) in rows.iter_mut().enumerate().skip(self.normal_count) {
				let ratio = weights[i - self.normal_count] / last_weight;
				for (value, last_value) in row.iter_mut().zip(&self.constraints[last]) {
					*value -= ratio * last_value;
				}
			}
		}
		if rows.len() != self.diagnostics.constraint_rank {
			return Err(CfdError::Assembly(
				"physical pressure boundary rank mismatch",
			));
		}
		let factor = super::constraint_qr::RowQr::new(&rows)?;
		if let Some(receipt) = &mesh_resources {
			use super::mesh::{add, mul};
			let rows_bytes = rows
				.iter()
				.try_fold(mul(rows.capacity(), size_of::<Vec<f64>>())?, |sum, row| {
					add(sum, mul(row.capacity(), 8)?)
				})?;
			let retained = add(
				factor.retained_bytes()?,
				add(
					rows_bytes,
					mul(add(residual.capacity(), weights.capacity())?, 8)?,
				)?,
			)?;
			// Original coefficient/force/acceleration owners, multipliers, recovery,
			// and returned pressure remain live across this factor's lifetime.
			let later = add(
				mul(8, add(mul(6, size)?, mul(4, self.constraints.len())?)?)?,
				mul(self.constraints.len(), size_of::<Vec<f64>>())?,
			)?;
			if add(retained, later)? > receipt.scratch_bytes {
				return Err(CfdError::InvalidInput(
					"pressure QR actual capacity exceeds admitted scratch",
				));
			}
		}
		let mut multipliers = factor.solve_transposed(&residual)?;
		if closed {
			let final_pressure = -dot(
				&multipliers[self.normal_count..],
				&weights[..weights.len() - 1],
			) / last_weight;
			multipliers.push(final_pressure);
		}
		let mut recovered = vec![0.; residual.len()];
		for (row, multiplier) in self.constraints.iter().zip(&multipliers) {
			for (value, c) in recovered.iter_mut().zip(row) {
				*value += c * multiplier;
			}
		}
		let momentum_residual = recovered
			.iter()
			.zip(&residual)
			.map(|(a, b)| (a - b).abs())
			.fold(0., f64::max);
		let scale = residual.iter().map(|v| v.abs()).fold(1., f64::max);
		if multipliers.iter().chain(&recovered).any(|v| !v.is_finite())
			|| !momentum_residual.is_finite()
			|| momentum_residual > 1e-8 * scale
		{
			return Err(CfdError::Assembly(
				"physical pressure momentum certificate failed",
			));
		}
		let pressure = &multipliers[self.normal_count..];
		let pressure_integral = -dot(pressure, &weights);
		if !pressure_integral.is_finite() {
			return Err(CfdError::Assembly("physical pressure integral overflow"));
		}
		Ok(GeneralPressureRecovery {
			mesh_resources,
			pressure_coefficients: pressure
				.chunks_exact(modes)
				.map(|cell| cell.iter().map(|value| -value).collect())
				.collect(),
			normal_multipliers: multipliers[..self.normal_count].to_vec(),
			momentum_residual,
			continuity_residual,
			normalization: if closed {
				PressureNormalization::ZeroVolumeMean
			} else {
				PressureNormalization::PrescribedMechanicalTraction
			},
			pressure_integral,
			normalization_residual: closed.then_some(pressure_integral.abs()),
		})
	}
}
