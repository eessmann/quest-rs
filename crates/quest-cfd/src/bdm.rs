//! Bounded dense assembly on exactly two triangles; indices are fixed by BDM1.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	clippy::manual_midpoint
)]

use crate::CfdError;

type Coefficients = [f64; 12];
type Matrix = [[f64; 12]; 12];

/// Numerical certificates for the complete constraint chart.
#[derive(Clone, Debug, serde::Serialize)]
pub struct AssemblyDiagnostics {
	/// Two cells, two velocity components, three affine monomials.
	pub local_velocity_dimension: usize,
	/// Six normal trace constraints and one independent divergence constraint.
	pub constraint_rank: usize,
	/// All independent velocity degrees of freedom, without model reduction.
	pub independent_dimension: usize,
	/// Maximum entry of C Q.
	pub constraint_residual: f64,
	/// Maximum entry of Q^T M Q - I.
	pub mass_orthogonality_residual: f64,
}

/// Reconstructed P0 pressure and normal-trace constraint forces.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PressureRecovery {
	/// Cell pressures with the area-weighted zero-mean gauge.
	pub cell_pressure: [f64; 2],
	/// Multipliers of the six pointwise normal trace constraints.
	pub normal_multipliers: [f64; 6],
	/// Infinity norm of the full twelve-component momentum residual.
	pub momentum_residual: f64,
	/// Maximum absolute cell-integrated divergence.
	pub continuity_residual: f64,
}

/// Full BDM1/P0 DG on the periodic unit square split along x=y.
///
/// Coefficients are ordered cell, component, then monomial (1,x,y).
/// The chart columns span the complete five-dimensional constraint kernel.
#[derive(Clone, Debug)]
pub struct PeriodicBdm1 {
	viscosity: f64,
	mass: Matrix,
	sip: Matrix,
	constraints: Vec<Coefficients>,
	chart: Vec<Coefficients>,
	diagnostics: AssemblyDiagnostics,
}

#[derive(Clone, Copy)]
struct Face {
	left: [[f64; 2]; 2],
	right: [[f64; 2]; 2],
	normal: [f64; 2],
	length: f64,
}

fn faces() -> [Face; 3] {
	let s = std::f64::consts::FRAC_1_SQRT_2;
	[
		Face {
			left: [[0., 0.], [1., 1.]],
			right: [[0., 0.], [1., 1.]],
			normal: [-s, s],
			length: std::f64::consts::SQRT_2,
		},
		Face {
			left: [[1., 0.], [1., 1.]],
			right: [[0., 0.], [0., 1.]],
			normal: [1., 0.],
			length: 1.,
		},
		Face {
			left: [[0., 0.], [1., 0.]],
			right: [[0., 1.], [1., 1.]],
			normal: [0., -1.],
			length: 1.,
		},
	]
}

fn interpolate(edge: [[f64; 2]; 2], t: f64) -> [f64; 2] {
	[
		edge[0][0] * (1. - t) + edge[1][0] * t,
		edge[0][1] * (1. - t) + edge[1][1] * t,
	]
}

const fn basis(index: usize, cell: usize, point: [f64; 2]) -> [f64; 2] {
	let mut value = [0.; 2];
	if index / 6 == cell {
		value[(index % 6) / 3] = [1., point[0], point[1]][index % 3];
	}
	value
}

const fn gradient(index: usize, cell: usize) -> [[f64; 2]; 2] {
	let mut value = [[0.; 2]; 2];
	if index / 6 == cell && !index.is_multiple_of(3) {
		value[(index % 6) / 3][index % 3 - 1] = 1.;
	}
	value
}

fn velocity(c: &Coefficients, cell: usize, point: [f64; 2]) -> [f64; 2] {
	let mut v = [0.; 2];
	for (i, &a) in c.iter().enumerate() {
		let b = basis(i, cell, point);
		for k in 0..2 {
			v[k] += a * b[k];
		}
	}
	v
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
	a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn mv(a: &Matrix, x: &Coefficients) -> Coefficients {
	std::array::from_fn(|i| dot(&a[i], x))
}
fn inner(a: &Coefficients, m: &Matrix, b: &Coefficients) -> f64 {
	dot(a, &mv(m, b))
}

fn volume_points(cell: usize) -> [[f64; 2]; 3] {
	if cell == 0 {
		[[1. / 3., 1. / 6.], [5. / 6., 1. / 6.], [5. / 6., 2. / 3.]]
	} else {
		[[1. / 6., 1. / 3.], [2. / 3., 5. / 6.], [1. / 6., 5. / 6.]]
	}
}

fn mass_matrix() -> Matrix {
	let mut m = [[0.; 12]; 12];
	for cell in 0..2 {
		for p in volume_points(cell) {
			for (i, row) in m.iter_mut().enumerate() {
				for (j, x) in row.iter_mut().enumerate() {
					*x += dot(&basis(i, cell, p), &basis(j, cell, p)) / 6.;
				}
			}
		}
	}
	m
}

fn constraint_rows() -> Vec<Coefficients> {
	let mut rows = Vec::new();
	for face in faces() {
		for end in 0..2 {
			rows.push(std::array::from_fn(|i| {
				dot(&basis(i, 0, face.left[end]), &face.normal)
					- dot(&basis(i, 1, face.right[end]), &face.normal)
			}));
		}
	}
	// D0-D1 fixes the independent divergence; periodic normal continuity gives D0+D1=0.
	rows.push(std::array::from_fn(|i| {
		let a = gradient(i, 0);
		let b = gradient(i, 1);
		0.5 * (a[0][0] + a[1][1] - b[0][0] - b[1][1])
	}));
	rows
}

fn nullspace(rows: &[Coefficients], mass: &Matrix) -> Result<Vec<Coefficients>, CfdError> {
	let mut rref = rows.to_vec();
	let mut pivots = Vec::new();
	for col in 0..12 {
		let rank = pivots.len();
		let pivot = (rank..rref.len()).find(|&row| rref[row][col].abs() > 1e-12);
		let Some(row) = pivot else {
			continue;
		};
		rref.swap(rank, row);
		let scale = rref[rank][col];
		for value in &mut rref[rank] {
			*value /= scale;
		}
		let pivot_row = rref[rank];
		for (i, values) in rref.iter_mut().enumerate() {
			if i != rank {
				let factor = values[col];
				for j in 0..12 {
					values[j] -= factor * pivot_row[j];
				}
			}
		}
		pivots.push(col);
	}
	if pivots.len() != 7 {
		return Err(CfdError::Assembly("constraint rank must be seven"));
	}
	let mut chart: Vec<Coefficients> = Vec::new();
	for free in (0..12).filter(|i| !pivots.contains(i)) {
		let mut q = [0.; 12];
		q[free] = 1.;
		for (row, &pivot) in pivots.iter().enumerate() {
			q[pivot] = -rref[row][free];
		}
		// Reorthogonalized mass Gram-Schmidt preserves the entire kernel.
		for _ in 0..2 {
			for previous in &chart {
				let projection = inner(previous, mass, &q);
				for i in 0..12 {
					q[i] -= projection * previous[i];
				}
			}
		}
		let norm = inner(&q, mass, &q).sqrt();
		if !norm.is_finite() || norm < 1e-12 {
			return Err(CfdError::Assembly("singular mass chart"));
		}
		for value in &mut q {
			*value /= norm;
		}
		chart.push(q);
	}
	Ok(chart)
}

fn sip_matrix() -> Matrix {
	let mut a = [[0.; 12]; 12];
	for (i, row) in a.iter_mut().enumerate() {
		for (j, value) in row.iter_mut().enumerate() {
			for cell in 0..2 {
				let gi = gradient(i, cell);
				let gj = gradient(j, cell);
				*value += 0.5 * (dot(&gi[0], &gj[0]) + dot(&gi[1], &gj[1]));
			}
			for face in faces() {
				for t in [0.5 - 0.5 / 3_f64.sqrt(), 0.5 + 0.5 / 3_f64.sqrt()] {
					let pl = interpolate(face.left, t);
					let pr = interpolate(face.right, t);
					let il = basis(i, 0, pl);
					let ir = basis(i, 1, pr);
					let jl = basis(j, 0, pl);
					let jr = basis(j, 1, pr);
					let ji = [il[0] - ir[0], il[1] - ir[1]];
					let jj = [jl[0] - jr[0], jl[1] - jr[1]];
					let gil = gradient(i, 0);
					let gir = gradient(i, 1);
					let gjl = gradient(j, 0);
					let gjr = gradient(j, 1);
					let dni = std::array::from_fn::<_, 2, _>(|k| {
						0.5 * (dot(&gil[k], &face.normal) + dot(&gir[k], &face.normal))
					});
					let dnj = std::array::from_fn::<_, 2, _>(|k| {
						0.5 * (dot(&gjl[k], &face.normal) + dot(&gjr[k], &face.normal))
					});
					// h=2*cell_area/face_length=1/face_length; penalty eta=20.
					*value += face.length
						* 0.5
						* (-dot(&dni, &jj) - dot(&dnj, &ji) + 20. * face.length * dot(&ji, &jj));
				}
			}
		}
	}
	a
}

fn retained_payload_bytes(constraints: usize, chart: usize) -> Result<usize, CfdError> {
	constraints
		.checked_add(chart)
		.and_then(|n| n.checked_mul(size_of::<Coefficients>()))
		.and_then(|n| n.checked_add(size_of::<PeriodicBdm1>()))
		.ok_or(CfdError::InvalidInput(
			"periodic BDM retained capacity overflow",
		))
}
impl PeriodicBdm1 {
	/// Actual retained fixed matrices and complete constraint/chart vector capacities.
	/// Excludes allocator metadata and temporary assembly/evaluation storage.
	/// # Errors
	/// Rejects checked capacity-byte overflow.
	pub fn retained_bytes(&self) -> Result<usize, CfdError> {
		retained_payload_bytes(self.constraints.capacity(), self.chart.capacity())
	}
	/// Assemble the complete periodic BDM1/P0 system with central convection and SIP viscosity.
	///
	/// # Errors
	/// Rejects negative/nonfinite viscosity or a failed full-rank/chart certificate.
	pub fn assemble(viscosity: f64) -> Result<Self, CfdError> {
		if !viscosity.is_finite() || viscosity < 0. {
			return Err(CfdError::InvalidInput(
				"viscosity must be finite and nonnegative",
			));
		}
		let mass = mass_matrix();
		let constraints = constraint_rows();
		let chart = nullspace(&constraints, &mass)?;
		let constraint_residual = constraints
			.iter()
			.flat_map(|c| chart.iter().map(move |q| dot(c, q).abs()))
			.fold(0., f64::max);
		let mut mass_orthogonality_residual = 0_f64;
		for (i, a) in chart.iter().enumerate() {
			for (j, b) in chart.iter().enumerate() {
				mass_orthogonality_residual =
					mass_orthogonality_residual.max((inner(a, &mass, b) - f64::from(i == j)).abs());
			}
		}
		if constraint_residual > 1e-11 || mass_orthogonality_residual > 1e-11 {
			return Err(CfdError::Assembly("chart residual exceeds tolerance"));
		}
		let diagnostics = AssemblyDiagnostics {
			local_velocity_dimension: 12,
			constraint_rank: constraints.len(),
			independent_dimension: chart.len(),
			constraint_residual,
			mass_orthogonality_residual,
		};
		Ok(Self {
			viscosity,
			mass,
			sip: sip_matrix(),
			constraints,
			chart,
			diagnostics,
		})
	}

	/// The complete independent dimension, never a reduced-model dimension.
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.chart.len()
	}

	/// Rank, constraint and mass certificates.
	#[must_use]
	pub const fn diagnostics(&self) -> &AssemblyDiagnostics {
		&self.diagnostics
	}

	/// The complete mass-orthonormal chart, one column per independent coordinate.
	#[must_use]
	pub fn chart(&self) -> &[[f64; 12]] {
		&self.chart
	}

	/// Convert independent mass coordinates to all twelve local BDM1 coefficients.
	///
	/// # Errors
	/// Rejects incorrect dimensions, nonfinite inputs or overflowing output.
	pub fn coefficients(&self, state: &[f64]) -> Result<Coefficients, CfdError> {
		if state.len() != self.dimension() || state.iter().any(|x| !x.is_finite()) {
			return Err(CfdError::InvalidInput(
				"state requires five finite mass coordinates",
			));
		}
		let c = std::array::from_fn(|i| {
			self.chart
				.iter()
				.zip(state)
				.map(|(q, s)| q[i] * s)
				.sum::<f64>()
		});
		if c.iter().any(|x| !x.is_finite()) {
			return Err(CfdError::InvalidInput("coefficient overflow"));
		}
		Ok(c)
	}

	/// Invert the full chart for an admissible local velocity.
	///
	/// # Errors
	/// Rejects nonfinite or non-divergence-conforming coefficients.
	pub fn coordinates(&self, c: &Coefficients) -> Result<Vec<f64>, CfdError> {
		if c.iter().any(|x| !x.is_finite()) {
			return Err(CfdError::InvalidInput("nonfinite coefficient"));
		}
		let scale = c.iter().map(|x| x.abs()).fold(1., f64::max);
		if self
			.constraints
			.iter()
			.any(|row| dot(row, c).abs() > 1e-10 * scale)
		{
			return Err(CfdError::InvalidInput(
				"velocity violates normal continuity or incompressibility",
			));
		}
		let state: Vec<_> = self.chart.iter().map(|q| inner(q, &self.mass, c)).collect();
		if state.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("coordinate overflow"));
		}
		Ok(state)
	}

	fn convection(c: &Coefficients) -> Coefficients {
		let mut result = [0.; 12];
		for (i, value) in result.iter_mut().enumerate() {
			for cell in 0..2 {
				let grad = gradient(i, cell);
				for p in volume_points(cell) {
					let u = velocity(c, cell, p);
					*value -= (u[0] * dot(&u, &grad[0]) + u[1] * dot(&u, &grad[1])) / 6.;
				}
			}
			for face in faces() {
				for t in [0.5 - 0.5 / 3_f64.sqrt(), 0.5 + 0.5 / 3_f64.sqrt()] {
					let pl = interpolate(face.left, t);
					let pr = interpolate(face.right, t);
					let ul = velocity(c, 0, pl);
					let ur = velocity(c, 1, pr);
					let vi = basis(i, 0, pl);
					let vj = basis(i, 1, pr);
					let avg = [(ul[0] + ur[0]) * 0.5, (ul[1] + ur[1]) * 0.5];
					let jump = [vi[0] - vj[0], vi[1] - vj[1]];
					*value += face.length * 0.5 * dot(&ul, &face.normal) * dot(&avg, &jump);
				}
			}
		}
		result
	}

	fn force(&self, c: &Coefficients) -> Coefficients {
		let convection = Self::convection(c);
		let diffusion = mv(&self.sip, c);
		std::array::from_fn(|i| -convection[i] - self.viscosity * diffusion[i])
	}

	/// Evaluate the complete quadratic incompressible DG ODE in mass coordinates.
	///
	/// # Errors
	/// Rejects malformed states or numerical overflow.
	pub fn drift(&self, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		let c = self.coefficients(state)?;
		let force = self.force(&c);
		let drift: Vec<_> = self.chart.iter().map(|q| dot(q, &force)).collect();
		if drift.iter().any(|x| !x.is_finite()) {
			return Err(CfdError::InvalidInput("drift overflow"));
		}
		Ok(drift)
	}

	/// DG kinetic energy, including both constant mean-flow modes.
	///
	/// # Errors
	/// Rejects malformed states.
	pub fn energy(&self, state: &[f64]) -> Result<f64, CfdError> {
		self.coefficients(state)?;
		let energy = 0.5 * dot(state, state);
		if !energy.is_finite() {
			return Err(CfdError::InvalidInput("energy overflow"));
		}
		Ok(energy)
	}

	/// Recover the pressure and hybrid normal forces from the full momentum equations.
	///
	/// # Errors
	/// Rejects malformed states or singular pressure recovery.
	pub fn reconstruct_pressure(&self, state: &[f64]) -> Result<PressureRecovery, CfdError> {
		let c = self.coefficients(state)?;
		let acceleration = self.coefficients(&self.drift(state)?)?;
		let f = self.force(&c);
		let ma = mv(&self.mass, &acceleration);
		let residual: Coefficients = std::array::from_fn(|i| f[i] - ma[i]);
		let gram: Vec<Vec<f64>> = self
			.constraints
			.iter()
			.map(|a| self.constraints.iter().map(|b| dot(a, b)).collect())
			.collect();
		let rhs = self.constraints.iter().map(|a| dot(a, &residual)).collect();
		let multipliers = solve(gram, rhs)?;
		let momentum_residual = (0..12)
			.map(|i| {
				(residual[i]
					- self
						.constraints
						.iter()
						.zip(&multipliers)
						.map(|(row, l)| row[i] * l)
						.sum::<f64>())
				.abs()
			})
			.fold(0., f64::max);
		let continuity_residual = [0, 1]
			.iter()
			.map(|&cell| (0.5 * (c[cell * 6 + 1] + c[cell * 6 + 5])).abs())
			.fold(0., f64::max);
		Ok(PressureRecovery {
			cell_pressure: [-multipliers[6], multipliers[6]],
			normal_multipliers: std::array::from_fn(|i| multipliers[i]),
			momentum_residual,
			continuity_residual,
		})
	}

	/// Classical same-model RK4 reference; this is not quantum execution.
	///
	/// # Errors
	/// Rejects nonpositive/nonfinite time steps, malformed states or overflow.
	pub fn integrate_rk4(
		&self,
		initial: &[f64],
		dt: f64,
		steps: usize,
	) -> Result<Vec<f64>, CfdError> {
		if !dt.is_finite() || dt <= 0. {
			return Err(CfdError::InvalidInput(
				"time step must be positive and finite",
			));
		}
		self.coefficients(initial)?;
		let mut state = initial.to_vec();
		for _ in 0..steps {
			let k1 = self.drift(&state)?;
			let shifted = |k: &[f64], factor: f64| {
				state
					.iter()
					.zip(k)
					.map(|(s, v)| s + factor * dt * v)
					.collect::<Vec<_>>()
			};
			let k2 = self.drift(&shifted(&k1, 0.5))?;
			let k3 = self.drift(&shifted(&k2, 0.5))?;
			let k4 = self.drift(&shifted(&k3, 1.))?;
			for i in 0..state.len() {
				state[i] += dt * (k1[i] + 2. * k2[i] + 2. * k3[i] + k4[i]) / 6.;
			}
			self.coefficients(&state)?;
		}
		Ok(state)
	}
}

pub fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Result<Vec<f64>, CfdError> {
	let n = b.len();
	for k in 0..n {
		let pivot = (k..n)
			.max_by(|&i, &j| a[i][k].abs().total_cmp(&a[j][k].abs()))
			.ok_or(CfdError::Assembly("empty pressure system"))?;
		if a[pivot][k].abs() < 1e-12 {
			return Err(CfdError::Assembly("singular pressure system"));
		}
		a.swap(k, pivot);
		b.swap(k, pivot);
		let scale = a[k][k];
		for value in a[k].iter_mut().skip(k) {
			*value /= scale;
		}
		b[k] /= scale;
		for i in 0..n {
			if i != k {
				let factor = a[i][k];
				for j in k..n {
					a[i][j] -= factor * a[k][j];
				}
				b[i] -= factor * b[k];
			}
		}
	}
	Ok(b)
}

#[cfg(test)]
#[allow(
	clippy::panic_in_result_fn,
	reason = "Actual-capacity regression uses assertions on a fallible test fixture"
)]
mod retained_payload_tests {
	use super::*;
	#[test]
	fn complete_fixed_source_accounts_actual_capacities() -> Result<(), CfdError> {
		let mut model = PeriodicBdm1::assemble(0.01)?;
		let previous = model.retained_bytes()?;
		let capacity = model.chart.capacity();
		model
			.chart
			.try_reserve_exact(100)
			.map_err(|_| CfdError::InvalidInput("test reserve"))?;
		assert_eq!(
			model.retained_bytes()? - previous,
			(model.chart.capacity() - capacity) * size_of::<Coefficients>()
		);
		assert!(retained_payload_bytes(usize::MAX, 1).is_err());
		Ok(())
	}
}
