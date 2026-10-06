use super::{CfdError, PhysicalSpace, Point, dot};
impl PhysicalSpace {
	/// Sample the reconstructed full field; interfaces use the first containing cell.
	/// # Errors
	/// Rejects malformed state and points outside the mesh.
	pub fn sample_velocity(&self, state: &[f64], point: Point) -> Result<Point, CfdError> {
		let coefficients = self.coefficients(state)?;
		if point.iter().any(|x| !x.is_finite()) || (self.dimension == 2 && point[2].abs() > 1e-10) {
			return Err(CfdError::InvalidInput("nonfinite physical sample"));
		}
		for (ci, cell) in self.cells.iter().enumerate() {
			let delta = std::array::from_fn::<_, 3, _>(|axis| point[axis] - cell.vertices[0][axis]);
			let bary = cell
				.gradients
				.iter()
				.enumerate()
				.map(|(i, g)| dot(g, &delta) + f64::from(i == 0))
				.collect::<Vec<_>>();
			if bary.iter().all(|x| *x >= -1e-10 && *x <= 1. + 1e-10) {
				let value = self.value(&coefficients, ci, &self.basis.values(&bary)?);
				if value.iter().any(|v| !v.is_finite()) {
					return Err(CfdError::InvalidInput("physical sample overflow"));
				}
				return Ok(value);
			}
		}
		Err(CfdError::InvalidInput("physical sample outside mesh"))
	}
	/// Volume integral of one half of squared physical vorticity.
	/// # Errors
	/// Rejects malformed states or numerical overflow.
	pub fn enstrophy(&self, state: &[f64]) -> Result<f64, CfdError> {
		let coefficients = self.coefficients(state)?;
		self.enstrophy_from_coefficients(&coefficients)
	}
	pub(super) fn enstrophy_from_coefficients(
		&self,
		coefficients: &[f64],
	) -> Result<f64, CfdError> {
		let mut result = 0.;
		for (ci, samples) in self.volume.iter().enumerate() {
			for sample in samples {
				let g = self.gradient(coefficients, ci, &sample.gradients);
				let curl = [g[2][1] - g[1][2], g[0][2] - g[2][0], g[1][0] - g[0][1]];
				result += 0.5 * sample.weight * dot(&curl, &curl);
			}
		}
		if !result.is_finite() {
			return Err(CfdError::InvalidInput("physical enstrophy overflow"));
		}
		Ok(result)
	}
	/// Quadrature error of the full recovered pressure in the zero-integral gauge.
	/// The callback must use the same physical pressure gauge.
	/// # Errors
	/// Rejects invalid states, failed recovery, nonfinite callbacks or overflow.
	pub fn pressure_error_l2(
		&self,
		state: &[f64],
		field: impl Fn(Point) -> f64,
	) -> Result<f64, CfdError> {
		let report = self.reconstruct_pressure(state)?;
		let mut error = 0.;
		for (ci, pressure) in report.pressure_coefficients.iter().enumerate() {
			for sample in self.high_samples(ci)? {
				let exact = field(sample.point);
				if !exact.is_finite() {
					return Err(CfdError::InvalidInput("nonfinite pressure reference"));
				}
				let value = if self.order == 1 {
					pressure[0]
				} else {
					dot(pressure, &sample.bary)
				};
				error += sample.weight * (value - exact).powi(2);
			}
		}
		if !error.is_finite() {
			return Err(CfdError::InvalidInput("physical pressure error overflow"));
		}
		Ok(error.sqrt())
	}
	/// Bounded classical RK4 evolution of every independent physical coordinate.
	/// # Errors
	/// Rejects invalid times/states and more than one billion modeled scalar work units.
	pub fn integrate_rk4(
		&self,
		initial: &[f64],
		duration: f64,
		steps: usize,
	) -> Result<Vec<f64>, CfdError> {
		self.integrate_rk4_with_work_limit(initial, duration, steps, 1_000_000_000)
	}
	/// Conservative scalar work for complete physical RK4, excluding prior assembly.
	/// # Errors
	/// Rejects checked work overflow; this does not admit a trajectory by itself.
	pub fn integration_work_bound(&self, steps: usize) -> Result<usize, CfdError> {
		let size = self.diagnostics.local_velocity_dimension;
		let samples = self.volume.iter().map(Vec::len).sum::<usize>()
			+ self.facets.iter().map(Vec::len).sum::<usize>();
		let per_step = size
			.checked_mul(size)
			.and_then(|x| x.checked_mul(64))
			.and_then(|x| {
				samples
					.checked_mul(self.local_velocity_per_cell())
					.and_then(|y| y.checked_mul(512))
					.and_then(|y| x.checked_add(y))
			})
			.ok_or(CfdError::InvalidInput("physical integration work overflow"))?;
		steps
			.checked_mul(per_step)
			.ok_or(CfdError::InvalidInput("physical integration work overflow"))
	}
	/// Same complete RK4 trajectory with an explicit global modeled-work allowance.
	/// The default API retains its one-billion-unit ceiling. The hard one-million
	/// step bound and physical-space construction bounds remain in force.
	/// # Errors
	/// Rejects invalid times/states, insufficient work allowance or arithmetic failure.
	pub fn integrate_rk4_with_work_limit(
		&self,
		initial: &[f64],
		duration: f64,
		steps: usize,
		max_work: usize,
	) -> Result<Vec<f64>, CfdError> {
		let work = self.integration_work_bound(steps)?;
		if !duration.is_finite()
			|| duration < 0.
			|| steps == 0
			|| steps > 1_000_000
			|| work > max_work
		{
			return Err(CfdError::InvalidInput(
				"physical integration time/work budget",
			));
		}
		self.coefficients(initial)?;
		let step_count = f64::from(
			u32::try_from(steps).map_err(|_| CfdError::InvalidInput("physical step count"))?,
		);
		let dt = duration / step_count;
		let mut state = initial.to_vec();
		for _ in 0..steps {
			let k1 = self.drift(&state)?;
			let intermediate = state
				.iter()
				.zip(&k1)
				.map(|(y, k)| y + 0.5 * dt * k)
				.collect::<Vec<_>>();
			let k2 = self.drift(&intermediate)?;
			let intermediate = state
				.iter()
				.zip(&k2)
				.map(|(y, k)| y + 0.5 * dt * k)
				.collect::<Vec<_>>();
			let k3 = self.drift(&intermediate)?;
			let intermediate = state
				.iter()
				.zip(&k3)
				.map(|(y, k)| y + dt * k)
				.collect::<Vec<_>>();
			let k4 = self.drift(&intermediate)?;
			for ((((y, a), b), c), d) in state.iter_mut().zip(k1).zip(k2).zip(k3).zip(k4) {
				*y += dt * (a + 2. * b + 2. * c + d) / 6.;
			}
		}
		self.coefficients(&state)?;
		Ok(state)
	}
}

impl PhysicalSpace {
	pub(super) fn high_samples(&self, cell: usize) -> Result<Vec<super::VolumeSample>, CfdError> {
		let geometry = &self.cells[cell];
		super::basis::high_quadrature(self.dimension)
			.into_iter()
			.map(|(bary, weight)| {
				let point = std::array::from_fn(|axis| {
					geometry
						.vertices
						.iter()
						.zip(&bary)
						.map(|(v, l)| v[axis] * l)
						.sum()
				});
				Ok(super::VolumeSample {
					values: self.basis.values(&bary)?,
					gradients: Vec::new(),
					bary,
					point,
					weight: weight * geometry.volume,
				})
			})
			.collect()
	}
	/// Physical broken-gradient dissipation, viscosity times integral of squared gradient.
	/// This observable excludes SIP face terms.
	/// # Errors
	/// Rejects malformed states and numerical overflow.
	pub fn gradient_dissipation(&self, state: &[f64]) -> Result<f64, CfdError> {
		let coefficients = self.coefficients(state)?;
		let mut result = 0.;
		for (ci, samples) in self.volume.iter().enumerate() {
			for sample in samples {
				let gradient = self.gradient(&coefficients, ci, &sample.gradients);
				result += self.viscosity
					* sample.weight
					* gradient.iter().map(|row| dot(row, row)).sum::<f64>();
			}
		}
		if !result.is_finite() {
			return Err(CfdError::InvalidInput(
				"physical gradient dissipation overflow",
			));
		}
		Ok(result)
	}
}
