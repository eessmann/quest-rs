use super::{PhysicalSpace, Point, dot, mass_apply};
use crate::CfdError;
impl PhysicalSpace {
	pub(super) fn assemble_viscosity(&self) -> (Vec<Vec<f64>>, Vec<f64>) {
		let scalar = self.basis.nodes.len();
		let local = self.dimension * scalar;
		let size = self.diagnostics.local_velocity_dimension;
		let mut matrix = vec![vec![0.; size]; size];
		let mut force = vec![0.; size];
		for (ci, samples) in self.volume.iter().enumerate() {
			for sample in samples {
				for component in 0..self.dimension {
					for i in 0..scalar {
						for j in 0..scalar {
							matrix[ci * local + component * scalar + i]
								[ci * local + component * scalar + j] +=
								sample.weight * dot(&sample.gradients[i], &sample.gradients[j]);
						}
					}
				}
			}
		}
		let degree_factor = if self.order == 1 { 40. } else { 90. };
		let dimension_f = if self.dimension == 2 { 2. } else { 3. };
		for (face, samples) in self.faces.iter().zip(&self.facets) {
			if face.outflow {
				continue;
			}
			let minimum_volume = face.right.map_or(self.cells[face.left].volume, |right| {
				self.cells[face.left].volume.min(self.cells[right].volume)
			});
			let penalty = degree_factor * face.measure / (dimension_f * minimum_volume);
			let average = if face.right.is_some() { 0.5 } else { 1. };
			for sample in samples {
				let mut traces = Vec::new();
				for (cell, values, gradients, sign) in [
					(
						Some(face.left),
						&sample.left_values,
						&sample.left_gradients,
						1.,
					),
					(
						face.right,
						&sample.right_values,
						&sample.right_gradients,
						-1.,
					),
				] {
					if let Some(ci) = cell {
						for node in 0..scalar {
							traces.push((
								ci,
								node,
								sign * values[node],
								average * dot(&gradients[node], &face.normal),
							));
						}
					}
				}
				for component in 0..self.dimension {
					for &(ci, i, jump_i, derivative_i) in &traces {
						let row = ci * local + component * scalar + i;
						for &(cj, j, jump_j, derivative_j) in &traces {
							let column = cj * local + component * scalar + j;
							matrix[row][column] += sample.weight
								* (-derivative_i * jump_j - derivative_j * jump_i
									+ penalty * jump_i * jump_j);
						}
						if face.right.is_none() {
							force[row] += sample.weight
								* sample.prescribed[component]
								* (-derivative_i + penalty * jump_i);
						}
					}
				}
			}
		}
		(matrix, force)
	}
	pub(super) fn value(&self, coefficients: &[f64], cell: usize, basis: &[f64]) -> Point {
		let scalar = self.basis.nodes.len();
		let local = self.dimension * scalar;
		std::array::from_fn(|component| {
			if component < self.dimension {
				(0..scalar)
					.map(|node| {
						coefficients[cell * local + component * scalar + node] * basis[node]
					})
					.sum()
			} else {
				0.
			}
		})
	}
	pub(super) fn gradient(
		&self,
		coefficients: &[f64],
		cell: usize,
		gradients: &[Point],
	) -> [Point; 3] {
		let scalar = self.basis.nodes.len();
		let local = self.dimension * scalar;
		std::array::from_fn(|component| {
			std::array::from_fn(|axis| {
				if component < self.dimension {
					(0..scalar)
						.map(|node| {
							coefficients[cell * local + component * scalar + node]
								* gradients[node][axis]
						})
						.sum()
				} else {
					0.
				}
			})
		})
	}
	pub(super) fn force(
		&self,
		coefficients: &[f64],
		advective: bool,
	) -> Result<Vec<f64>, CfdError> {
		let scalar = self.basis.nodes.len();
		let local = self.dimension * scalar;
		let mut force = self
			.sip
			.iter()
			.zip(&self.boundary_force)
			.map(|(row, &boundary)| self.viscosity * (boundary - dot(row, coefficients)))
			.collect::<Vec<_>>();
		for (ci, samples) in self.volume.iter().enumerate() {
			for sample in samples {
				let velocity = self.value(coefficients, ci, &sample.values);
				let gradient = self.gradient(coefficients, ci, &sample.gradients);
				for component in 0..self.dimension {
					for node in 0..scalar {
						force[ci * local + component * scalar + node] += sample.weight
							* if advective {
								-sample.values[node] * dot(&velocity, &gradient[component])
							} else {
								velocity[component] * dot(&velocity, &sample.gradients[node])
							};
					}
				}
			}
		}
		for (face, samples) in self.faces.iter().zip(&self.facets) {
			for sample in samples {
				let left = self.value(coefficients, face.left, &sample.left_values);
				let right = face.right.map_or(
					if face.outflow {
						left
					} else {
						sample.prescribed
					},
					|ci| self.value(coefficients, ci, &sample.right_values),
				);
				let normal_velocity = dot(&left, &face.normal);
				for component in 0..self.dimension {
					let average = f64::midpoint(left[component], right[component]);
					for node in 0..scalar {
						if advective {
							force[face.left * local + component * scalar + node] -= sample.weight
								* normal_velocity
								* (average - left[component])
								* sample.left_values[node];
							if let Some(ci) = face.right {
								force[ci * local + component * scalar + node] += sample.weight
									* normal_velocity
									* (average - right[component])
									* sample.right_values[node];
							}
						} else {
							force[face.left * local + component * scalar + node] -= sample.weight
								* normal_velocity
								* average
								* sample.left_values[node];
							if let Some(ci) = face.right {
								force[ci * local + component * scalar + node] += sample.weight
									* normal_velocity
									* average
									* sample.right_values[node];
							}
						}
					}
				}
			}
		}
		if force.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::Assembly("full higher-order force overflow"));
		}
		Ok(force)
	}
	/// Reconstruct every broken velocity coefficient from the complete homogeneous chart.
	/// # Errors
	/// Rejects malformed/nonfinite state or overflow.
	pub fn coefficients(&self, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		if state.len() != self.dimension() || state.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("invalid full physical state"));
		}
		let mut coefficients = vec![0.; self.diagnostics.local_velocity_dimension];
		for (a, basis) in state.iter().zip(&self.chart) {
			for (value, b) in coefficients.iter_mut().zip(basis) {
				*value += a * b;
			}
		}
		if coefficients.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("full physical coefficient overflow"));
		}
		Ok(coefficients)
	}
	/// Recover complete coordinates of a conforming divergence-free broken coefficient vector.
	/// # Errors
	/// Rejects malformed vectors or a constraint violation instead of silently projecting it.
	pub fn coordinates(&self, coefficients: &[f64]) -> Result<Vec<f64>, CfdError> {
		if coefficients.len() != self.diagnostics.local_velocity_dimension
			|| coefficients.iter().any(|v| !v.is_finite())
		{
			return Err(CfdError::InvalidInput("invalid full broken coefficients"));
		}
		let scale = coefficients.iter().map(|v| v.abs()).fold(1., f64::max);
		if self
			.constraints
			.iter()
			.any(|row| dot(row, coefficients).abs() > 1e-9 * scale)
		{
			return Err(CfdError::InvalidInput(
				"broken coefficients violate full physical constraints",
			));
		}
		let mass = mass_apply(&self.cells, self.dimension, &self.basis, coefficients);
		let state = self.chart.iter().map(|q| dot(q, &mass)).collect::<Vec<_>>();
		if state.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("full physical coordinate overflow"));
		}
		Ok(state)
	}
	/// Complete central-conservative DG convection plus SIP viscosity and tangential boundary data.
	/// # Errors
	/// Rejects malformed states, arithmetic overflow and failed force evaluation.
	pub fn drift(&self, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		let coefficients = self.coefficients(state)?;
		let force = self.force(&coefficients, false)?;
		let result = self
			.chart
			.iter()
			.map(|q| dot(q, &force))
			.collect::<Vec<_>>();
		if result.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("full physical drift overflow"));
		}
		Ok(result)
	}
	/// Independent advective-volume formulation with central trace corrections on the full kernel.
	/// # Errors
	/// Rejects malformed states or numerical overflow.
	pub fn advective_drift_reference(&self, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		let coefficients = self.coefficients(state)?;
		let force = self.force(&coefficients, true)?;
		let result = self
			.chart
			.iter()
			.map(|q| dot(q, &force))
			.collect::<Vec<_>>();
		if result.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("advective physical drift overflow"));
		}
		Ok(result)
	}
	/// Integrated kinetic energy of the complete physical velocity.
	/// # Errors
	/// Rejects malformed state or numerical overflow.
	pub fn energy(&self, state: &[f64]) -> Result<f64, CfdError> {
		let coefficients = self.coefficients(state)?;
		self.energy_from_coefficients(&coefficients)
	}
	pub(super) fn energy_from_coefficients(&self, coefficients: &[f64]) -> Result<f64, CfdError> {
		let mass = mass_apply(&self.cells, self.dimension, &self.basis, coefficients);
		let energy = 0.5 * dot(coefficients, &mass);
		if !energy.is_finite() {
			return Err(CfdError::InvalidInput("physical energy overflow"));
		}
		Ok(energy)
	}
	/// Complete constrained L2 projection of a physical velocity callback.
	/// # Errors
	/// Rejects nonfinite callback values or numerical overflow.
	pub fn project_velocity(&self, field: impl Fn(Point) -> Point) -> Result<Vec<f64>, CfdError> {
		let scalar = self.basis.nodes.len();
		let local = self.dimension * scalar;
		let mut load = vec![0.; self.diagnostics.local_velocity_dimension];
		for ci in 0..self.cells.len() {
			for sample in self.high_samples(ci)? {
				let value = field(sample.point);
				if value.iter().any(|v| !v.is_finite()) {
					return Err(CfdError::InvalidInput("nonfinite projection field"));
				}
				for component in 0..self.dimension {
					for node in 0..scalar {
						load[ci * local + component * scalar + node] +=
							sample.weight * value[component] * sample.values[node];
					}
				}
			}
		}
		let state = self.chart.iter().map(|q| dot(q, &load)).collect::<Vec<_>>();
		self.coefficients(&state)?;
		Ok(state)
	}
	/// Quadrature L2 error of the full reconstructed physical field.
	/// # Errors
	/// Rejects nonfinite analytic values and numerical overflow.
	pub fn velocity_error_l2(
		&self,
		state: &[f64],
		field: impl Fn(Point) -> Point,
	) -> Result<f64, CfdError> {
		let coefficients = self.coefficients(state)?;
		let mut error = 0.;
		for ci in 0..self.cells.len() {
			for sample in self.high_samples(ci)? {
				let exact = field(sample.point);
				if exact.iter().any(|v| !v.is_finite()) {
					return Err(CfdError::InvalidInput("nonfinite error reference"));
				}
				let numerical = self.value(&coefficients, ci, &sample.values);
				error += sample.weight
					* (0..self.dimension)
						.map(|component| (numerical[component] - exact[component]).powi(2))
						.sum::<f64>();
			}
		}
		if !error.is_finite() {
			return Err(CfdError::InvalidInput("physical error overflow"));
		}
		Ok(error.sqrt())
	}
	/// Maximum pointwise divergence over a degree-exact volume rule.
	/// # Errors
	/// Rejects malformed state and numerical overflow.
	pub fn divergence_residual(&self, state: &[f64]) -> Result<f64, CfdError> {
		let coefficients = self.coefficients(state)?;
		let mut residual = 0_f64;
		for (ci, samples) in self.volume.iter().enumerate() {
			for sample in samples {
				let gradient = self.gradient(&coefficients, ci, &sample.gradients);
				let value = (0..self.dimension)
					.map(|axis| gradient[axis][axis])
					.sum::<f64>()
					.abs();
				if !value.is_finite() {
					return Err(CfdError::InvalidInput("physical divergence overflow"));
				}
				residual = residual.max(value);
			}
		}
		Ok(residual)
	}
	/// Maximum full polynomial normal-trace jump or boundary penetration.
	/// # Errors
	/// Rejects malformed state and numerical overflow.
	pub fn normal_trace_residual(&self, state: &[f64]) -> Result<f64, CfdError> {
		let coefficients = self.coefficients(state)?;
		let mut residual = 0_f64;
		for (face, samples) in self.faces.iter().zip(&self.facets) {
			if face.outflow {
				continue;
			} // Natural normal velocity is a free physical trace.
			for sample in samples {
				let left = self.value(&coefficients, face.left, &sample.left_values);
				let right = face.right.map_or([0.; 3], |ci| {
					self.value(&coefficients, ci, &sample.right_values)
				});
				let value = (dot(&left, &face.normal) - dot(&right, &face.normal)).abs();
				if !value.is_finite() {
					return Err(CfdError::InvalidInput("physical normal trace overflow"));
				}
				residual = residual.max(value);
			}
		}
		Ok(residual)
	}
}
