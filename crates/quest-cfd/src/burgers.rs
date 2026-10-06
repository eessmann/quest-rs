//! Entropy-conservative split nodal DG Burgers with symmetric interior penalty diffusion.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Admitted DG1/DG2 indices and independently tested quadrature formulas"
)]
use crate::{CfdError, polynomial::PolynomialOde};
use mathcore::{
	RBig,
	arithmetic::ExactConstant,
	exact::{Owner, Symbol},
	multivariate::{PolynomialLimits, SparsePolynomial, rational_constant},
};
use std::sync::Arc;

/// Frozen one-dimensional physical demonstration, separate from lift/time refinements.
#[derive(Clone, Debug, serde::Serialize)]
pub struct BurgersManifest {
	/// Physical interval.
	pub domain: [f64; 2],
	/// Homogeneous essential boundary values.
	pub boundary: String,
	/// Kinematic viscosity.
	pub viscosity: f64,
	/// Physical final time.
	pub horizon: f64,
	/// Default full DG cell count.
	pub cells: u32,
	/// Default physical DG degree.
	pub order: usize,
	/// Cole-Hopf heat-potential amplitude, not a reduced modal amplitude.
	pub amplitude: f64,
	/// Initial number of temporal DG elements for quantum-history studies.
	pub temporal_cells: usize,
	/// Initial temporal DG degree.
	pub temporal_order: usize,
}
/// The approved frozen Burgers contract.
#[must_use]
pub fn manifest() -> BurgersManifest {
	BurgersManifest {
		domain: [0., 1.],
		boundary: "zero Dirichlet, imposed through entropy-conservative boundary flux and SIP"
			.into(),
		viscosity: 0.1,
		horizon: 0.1,
		cells: 4,
		order: 1,
		amplitude: 0.01,
		temporal_cells: 2,
		temporal_order: 1,
	}
}

/// Complete mass-weighted nodal DG state, with no removed physical coordinates.
#[derive(Clone, Debug)]
pub struct BurgersDg {
	cells: u32,
	order: usize,
	viscosity: f64,
	nodes: Vec<f64>,
	weights: Vec<f64>,
	derivative: Vec<Vec<f64>>,
	mass_roots: Vec<f64>,
	stiffness: Vec<Vec<f64>>,
	ode: Arc<PolynomialOde>,
}
type ElementData = (Vec<f64>, Vec<f64>, Vec<Vec<f64>>);
fn element(order: usize) -> Result<ElementData, CfdError> {
	match order {
		1 => Ok((
			vec![-1., 1.],
			vec![1., 1.],
			vec![vec![-0.5, 0.5], vec![-0.5, 0.5]],
		)),
		2 => Ok((
			vec![-1., 0., 1.],
			vec![1. / 3., 4. / 3., 1. / 3.],
			vec![
				vec![-1.5, 2., -0.5],
				vec![-0.5, 0., 0.5],
				vec![0.5, -2., 1.5],
			],
		)),
		_ => Err(CfdError::Unsupported(
			"Burgers reference supports physical DG1 and DG2".into(),
		)),
	}
}
fn stiffness(cells: usize, weights: &[f64], derivative: &[Vec<f64>], h: f64) -> Vec<Vec<f64>> {
	let q = weights.len();
	let n = cells * q;
	let mut matrix = vec![vec![0.; n]; n];
	for cell in 0..cells {
		for i in 0..q {
			for j in 0..q {
				matrix[cell * q + i][cell * q + j] = (0..q)
					.map(|k| 2. / h * weights[k] * derivative[k][i] * derivative[k][j])
					.sum();
			}
		}
	}
	let qf = if q == 2 { 2. } else { 3. };
	let penalty = 4. * qf * qf / h;
	for face in 0..=cells {
		let mut traces = Vec::new();
		if face > 0 {
			for (i, &gradient) in derivative[q - 1].iter().enumerate() {
				traces.push(((face - 1) * q + i, f64::from(i == q - 1), gradient * 2. / h));
			}
		}
		if face < cells {
			for (i, &gradient) in derivative[0].iter().enumerate() {
				traces.push((face * q + i, -f64::from(i == 0), gradient * 2. / h));
			}
		}
		let boundary = face == 0 || face == cells;
		let average = if boundary { 1. } else { 0.5 };
		for &(i, ji, di) in &traces {
			for &(j, jj, dj) in &traces {
				matrix[i][j] += -average * di * jj - average * dj * ji + penalty * ji * jj;
			}
		}
	}
	matrix
}
fn term(
	terms: &mut Vec<(Vec<u32>, RBig)>,
	n: usize,
	indices: &[usize],
	value: f64,
	limits: PolynomialLimits,
) -> Result<(), CfdError> {
	if value == 0. {
		return Ok(());
	}
	let mut powers = vec![0; n];
	for &i in indices {
		powers[i] += 1;
	}
	terms.push((
		powers,
		rational_constant(&ExactConstant::Binary64(value), limits)?,
	));
	Ok(())
}
#[allow(
	clippy::many_single_char_names,
	reason = "Conventional local DG indices and cell length in polynomial assembly"
)]
fn forms(
	cells: usize,
	derivative: &[Vec<f64>],
	roots: &[f64],
	stiffness: &[Vec<f64>],
	h: f64,
	nu: f64,
) -> Result<PolynomialOde, CfdError> {
	let q = derivative.len();
	let n = roots.len();
	let limits = PolynomialLimits::default();
	let symbols: Result<Vec<_>, CfdError> = (0..n)
		.map(|i| {
			Ok(Symbol::new(
				Owner::new(0x4346_4442_5552_4745),
				u64::try_from(i).map_err(|_| CfdError::InvalidInput("Burgers symbol index"))?,
			))
		})
		.collect();
	let symbols = symbols?;
	let mut equations = Vec::new();
	for i in 0..n {
		let cell = i / q;
		let local = i % q;
		let mut terms = Vec::new();
		for j in 0..n {
			term(
				&mut terms,
				n,
				&[j],
				-nu * stiffness[i][j] / (roots[i] * roots[j]),
				limits,
			)?;
		}
		for (b, &gradient) in derivative[local].iter().enumerate() {
			let j = cell * q + b;
			let c = -4. / h * gradient * roots[i] / 6.;
			term(&mut terms, n, &[i, i], c / (roots[i] * roots[i]), limits)?;
			term(&mut terms, n, &[i, j], c / (roots[i] * roots[j]), limits)?;
			term(&mut terms, n, &[j, j], c / (roots[j] * roots[j]), limits)?;
		}
		for (side, normal) in [(0, -1.), (q - 1, 1.)] {
			if local == side {
				let neighbor = if side == 0 {
					cell.checked_sub(1).map(|c| c * q + q - 1)
				} else if cell + 1 < cells {
					Some((cell + 1) * q)
				} else {
					None
				};
				let c = -normal / (6. * roots[i]);
				term(
					&mut terms,
					n,
					&[i, i],
					c / (roots[i] * roots[i]) + normal / (2. * roots[i].powi(3)),
					limits,
				)?;
				if let Some(j) = neighbor {
					term(&mut terms, n, &[i, j], c / (roots[i] * roots[j]), limits)?;
					term(&mut terms, n, &[j, j], c / (roots[j] * roots[j]), limits)?;
				}
			}
		}
		equations.push(SparsePolynomial::from_terms(
			symbols.clone(),
			terms,
			limits,
		)?);
	}
	PolynomialOde::from_polynomials(equations, n, limits)
}
impl BurgersDg {
	/// Assemble complete DG1/DG2 dynamics on a uniform unit interval.
	/// # Errors
	/// Rejects invalid viscosity, unsupported degree and more than 128 physical coefficients.
	pub fn new(cells: u32, order: usize, viscosity: f64) -> Result<Self, CfdError> {
		if cells == 0 || !viscosity.is_finite() || viscosity < 0. {
			return Err(CfdError::InvalidInput("invalid Burgers cells or viscosity"));
		}
		let (nodes, weights, derivative) = element(order)?;
		let count =
			usize::try_from(cells).map_err(|_| CfdError::InvalidInput("Burgers cell count"))?;
		let n = count
			.checked_mul(nodes.len())
			.filter(|&n| n <= 128)
			.ok_or_else(|| {
				CfdError::Unsupported(
					"Burgers bounded classical assembly requires at most 128 full coefficients"
						.into(),
				)
			})?;
		let h = 1. / f64::from(cells);
		let mass_roots = (0..n)
			.map(|i| (0.5 * h * weights[i % nodes.len()]).sqrt())
			.collect::<Vec<_>>();
		let stiffness = stiffness(count, &weights, &derivative, h);
		let ode = Arc::new(forms(
			count,
			&derivative,
			&mass_roots,
			&stiffness,
			h,
			viscosity,
		)?);
		Ok(Self {
			cells,
			order,
			viscosity,
			nodes,
			weights,
			derivative,
			mass_roots,
			stiffness,
			ode,
		})
	}
	/// Full physical coefficient count, including boundary traces.
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.mass_roots.len()
	}
	/// Physical polynomial degree.
	#[must_use]
	pub const fn order(&self) -> usize {
		self.order
	}
	/// Shared exact equations and numerical kernels used by all lifts.
	#[must_use]
	pub fn polynomial_ode(&self) -> Arc<PolynomialOde> {
		Arc::clone(&self.ode)
	}
	/// Evaluate the lowered complete DG polynomial.
	/// # Errors
	/// Rejects invalid states and numerical overflow.
	pub fn drift(&self, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		self.ode.drift(0., state)
	}
	/// Independently evaluate split flux and SIP residuals without coefficient extraction.
	/// # Errors
	/// Rejects invalid states and numerical overflow.
	pub fn direct_drift(&self, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		if state.len() != self.dimension() || state.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("invalid Burgers mass coordinates"));
		}
		let u: Vec<_> = state
			.iter()
			.zip(&self.mass_roots)
			.map(|(a, m)| a / m)
			.collect();
		let q = self.nodes.len();
		let h = 1. / f64::from(self.cells);
		let mut result = vec![0.; self.dimension()];
		#[allow(
			clippy::suspicious_operation_groupings,
			reason = "The entropy-conservative Burgers flux is the symmetric quadratic sum"
		)]
		let flux = |a: f64, b: f64| (a * a + a * b + b * b) / 6.;
		for i in 0..self.dimension() {
			let cell = i / q;
			let local = i % q;
			let mut derivative = 0.;
			for j in 0..q {
				derivative -= 4. / h * self.derivative[local][j] * flux(u[i], u[cell * q + j]);
			}
			for (side, normal) in [(0, -1.), (q - 1, 1.)] {
				if local == side {
					let neighbor = if side == 0 {
						cell.checked_sub(1).map_or(0., |c| u[c * q + q - 1])
					} else {
						u.get((cell + 1) * q).copied().unwrap_or(0.)
					};
					derivative -= normal / (0.5 * h * self.weights[local])
						* (flux(u[i], neighbor) - 0.5 * u[i] * u[i]);
				}
			}
			result[i] = self.mass_roots[i] * derivative
				- self.viscosity / self.mass_roots[i]
					* self.stiffness[i]
						.iter()
						.zip(&u)
						.map(|(a, b)| a * b)
						.sum::<f64>();
		}
		if result.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("Burgers residual overflow"));
		}
		Ok(result)
	}
	/// Smooth exact Cole-Hopf solution with heat potential 1+epsilon exp(-nu pi²t) cos(pi x).
	/// # Errors
	/// Rejects nonpositive viscosity, invalid amplitude/time or points outside [0,1].
	pub fn cole_hopf(&self, time: f64, x: f64, amplitude: f64) -> Result<f64, CfdError> {
		if !time.is_finite()
			|| time < 0.
			|| !x.is_finite()
			|| !(0.0..=1.0).contains(&x)
			|| !amplitude.is_finite()
			|| amplitude.abs() >= 1.
			|| self.viscosity <= 0.
		{
			return Err(CfdError::InvalidInput("invalid Cole-Hopf reference"));
		}
		let pi = std::f64::consts::PI;
		let a = amplitude * (-self.viscosity * pi * pi * time).exp();
		Ok(2. * self.viscosity * pi * a * (pi * x).sin() / (1. + a * (pi * x).cos()))
	}
	/// Interpolate the full initial Cole-Hopf field at every DG node and apply mass weighting.
	/// # Errors
	/// Rejects invalid Cole-Hopf parameters.
	pub fn initial_state(&self, amplitude: f64) -> Result<Vec<f64>, CfdError> {
		let q = self.nodes.len();
		let mut state = Vec::new();
		for cell in 0..self.cells {
			for a in 0..q {
				let x =
					(f64::from(cell) + f64::midpoint(self.nodes[a], 1.)) / f64::from(self.cells);
				state.push(self.cole_hopf(0., x, amplitude)? * self.mass_roots[state.len()]);
			}
		}
		Ok(state)
	}
	/// Quadrature kinetic energy in the complete mass coordinates.
	/// # Errors
	/// Rejects malformed/nonfinite states or energy overflow.
	pub fn energy(&self, state: &[f64]) -> Result<f64, CfdError> {
		if state.len() != self.dimension() || state.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("invalid Burgers energy state"));
		}
		let energy = 0.5 * state.iter().map(|v| v * v).sum::<f64>();
		if !energy.is_finite() {
			return Err(CfdError::InvalidInput("Burgers energy overflow"));
		}
		Ok(energy)
	}
	/// Independent Gauss integration of physical velocity error against Cole-Hopf.
	/// # Errors
	/// Rejects invalid states or reference parameters.
	pub fn l2_error(&self, state: &[f64], time: f64, amplitude: f64) -> Result<f64, CfdError> {
		self.energy(state)?;
		let q = self.nodes.len();
		let h = 1. / f64::from(self.cells);
		let mut error = 0.;
		for cell in 0..self.cells {
			let c = usize::try_from(cell).map_err(|_| CfdError::InvalidInput("cell index"))?;
			for (xi, w) in [
				(-0.861_136_311_594_052_6, 0.347_854_845_137_453_85),
				(-0.339_981_043_584_856_3, 0.652_145_154_862_546_1),
				(0.339_981_043_584_856_3, 0.652_145_154_862_546_1),
				(0.861_136_311_594_052_6, 0.347_854_845_137_453_85),
			] {
				let value = (0..q)
					.map(|i| {
						let basis = (0..q)
							.filter(|&j| j != i)
							.map(|j| (xi - self.nodes[j]) / (self.nodes[i] - self.nodes[j]))
							.product::<f64>();
						basis * state[c * q + i] / self.mass_roots[c * q + i]
					})
					.sum::<f64>();
				let x = h * (f64::from(cell) + f64::midpoint(xi, 1.));
				error += 0.5 * h * w * (value - self.cole_hopf(time, x, amplitude)?).powi(2);
			}
		}
		Ok(error.sqrt())
	}
}

/// Executed full-coordinate classical reference, explicitly separate from a quantum solve.
#[derive(Clone, Debug, serde::Serialize)]
pub struct BurgersReference {
	/// Number of full physical mass coordinates.
	pub physical_dimension: usize,
	/// Physical DG cells.
	pub cells: u32,
	/// Physical DG degree.
	pub order: usize,
	/// Viscosity used by the assembled ODE.
	pub viscosity: f64,
	/// Final physical time.
	pub horizon: f64,
	/// Number of actual RK4 steps.
	pub steps: u32,
	/// All final physical mass coordinates.
	pub state: Vec<f64>,
	/// Initial quadrature energy.
	pub initial_energy: f64,
	/// Final quadrature energy.
	pub final_energy: f64,
	/// Independently integrated physical velocity error.
	pub cole_hopf_l2_error: f64,
	/// Honest execution status.
	pub status: String,
}
impl BurgersDg {
	/// Execute a bounded full DG reference and compare against the analytic Cole-Hopf field.
	/// # Errors
	/// Rejects invalid parameters, more than one million steps and unstable numerical overflow.
	pub fn classical_reference(
		&self,
		amplitude: f64,
		horizon: f64,
		steps: u32,
	) -> Result<BurgersReference, CfdError> {
		if !horizon.is_finite() || horizon <= 0. || steps == 0 || steps > 1_000_000 {
			return Err(CfdError::InvalidInput(
				"invalid Burgers reference horizon/step budget",
			));
		}
		let initial = self.initial_state(amplitude)?;
		let initial_energy = self.energy(&initial)?;
		let state = self
			.ode
			.integrate_rk4(&initial, horizon / f64::from(steps), steps)?;
		let final_energy = self.energy(&state)?;
		let cole_hopf_l2_error = self.l2_error(&state, horizon, amplitude)?;
		Ok(BurgersReference {
			physical_dimension: self.dimension(),
			cells: self.cells,
			order: self.order,
			viscosity: self.viscosity,
			horizon,
			steps,
			state,
			initial_energy,
			final_energy,
			cole_hopf_l2_error,
			status: "executed classical full-coordinate DG reference; no quantum execution".into(),
		})
	}
}
