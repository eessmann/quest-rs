//! Complete Fu–Shu doubled-field ultraweak DG for periodic `KdV` and linear Airy validation.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Checked DG2/DG3 sizes bound modal indices and explicit quadrature formulas"
)]
use crate::{CfdError, polynomial::PolynomialOde};
use mathcore::{
	RBig,
	arithmetic::ExactConstant,
	exact::{Owner, Symbol},
	multivariate::{PolynomialLimits, SparsePolynomial, rational_constant},
};
use std::sync::Arc;

/// Physical equation selected explicitly; the auxiliary Airy field is always retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum KdvMode {
	/// `u_t+6u u_x+u_xxx=0`, `phi_t-phi_xxx=0`.
	Nonlinear,
	/// `u_t+u_xxx=0`, `phi_t-phi_xxx=0`, solely for a separate analytic validation.
	LinearAiry,
}
/// Frozen physical demonstration, independent of time/lift refinement.
#[derive(Clone, Debug, serde::Serialize)]
pub struct KdvManifest {
	/// Periodic physical interval.
	pub domain: [f64; 2],
	/// Explicit PDE and boundary definition.
	pub definition: String,
	/// Complete physical DG cell count.
	pub cells: u32,
	/// Physical DG polynomial degree.
	pub order: usize,
	/// Initial u amplitude; phi is initially zero.
	pub amplitude: f64,
	/// Physical final time.
	pub horizon: f64,
	/// Initial temporal DG cells.
	pub temporal_cells: usize,
	/// Initial temporal DG degree.
	pub temporal_order: usize,
}
/// The approved complete `KdV` demonstration.
#[must_use]
pub fn manifest() -> KdvManifest {
	KdvManifest{domain:[0.,std::f64::consts::TAU],definition:"u_t+6u u_x+u_xxx=0; phi_t-phi_xxx=0; periodic; u0=0.05cos(x), phi0=0; all auxiliary DG coordinates evolve".into(),cells:4,order:2,amplitude:0.05,horizon:0.1,temporal_cells:2,temporal_order:1}
}
/// Aggregate construction admission, including raw terms, exact forms and prepared kernels.
#[derive(Clone, Copy, Debug)]
pub struct KdvLimits {
	/// Complete dimension including the auxiliary field.
	pub max_dimension: usize,
	/// Conservative concurrent assembly and lowered-kernel storage.
	pub max_bytes: usize,
	/// Conservative assembly/extraction logical work.
	pub max_work: usize,
	/// Shared exact-algebra limits, applied in addition to the aggregate limits.
	pub polynomial: PolynomialLimits,
}
impl Default for KdvLimits {
	fn default() -> Self {
		Self {
			max_dimension: 128,
			max_bytes: 512 * 1024 * 1024,
			max_work: 256 * 1024 * 1024,
			polynomial: PolynomialLimits {
				max_bytes: 256 * 1024 * 1024,
				max_work: 256 * 1024 * 1024,
				..PolynomialLimits::default()
			},
		}
	}
}
type Jet = [f64; 4];
#[derive(Clone, Copy, Debug)]
struct Quadrature {
	xi: f64,
	weight: f64,
	basis: [Jet; 4],
}
#[derive(Clone, Debug)]
struct Element {
	size: usize,
	h: f64,
	quadrature: [Quadrature; 8],
	edge: [[Jet; 4]; 2],
}
fn jet(mode: usize, xi: f64, h: f64) -> Jet {
	let mut p = match mode {
		0 => [1., 0., 0., 0.],
		1 => [xi, 1., 0., 0.],
		2 => [0.5 * (3. * xi * xi - 1.), 3. * xi, 3., 0.],
		_ => [
			0.5 * (5. * xi * xi * xi - 3. * xi),
			0.5 * (15. * xi * xi - 3.),
			15. * xi,
			15.,
		],
	};
	let degree = match mode {
		0 => 1.,
		1 => 3.,
		2 => 5.,
		_ => 7.,
	};
	let root = (degree / h).sqrt();
	let mut scale = root;
	for value in &mut p {
		*value *= scale;
		scale *= 2. / h;
	}
	p
}
impl Element {
	fn new(order: usize, cells: u32) -> Self {
		let h = std::f64::consts::TAU / f64::from(cells);
		let quadrature = [
			(-0.960_289_856_497_536_3, 0.101_228_536_290_376_3),
			(-0.796_666_477_413_626_7, 0.222_381_034_453_374_5),
			(-0.525_532_409_916_329, 0.313_706_645_877_887_3),
			(-0.183_434_642_495_649_8, 0.362_683_783_378_362),
			(0.183_434_642_495_649_8, 0.362_683_783_378_362),
			(0.525_532_409_916_329, 0.313_706_645_877_887_3),
			(0.796_666_477_413_626_7, 0.222_381_034_453_374_5),
			(0.960_289_856_497_536_3, 0.101_228_536_290_376_3),
		]
		.map(|(xi, weight)| Quadrature {
			xi,
			weight: 0.5 * h * weight,
			basis: std::array::from_fn(|mode| jet(mode, xi, h)),
		});
		Self {
			size: order + 1,
			h,
			quadrature,
			edge: [
				std::array::from_fn(|mode| jet(mode, -1., h)),
				std::array::from_fn(|mode| jet(mode, 1., h)),
			],
		}
	}
}
fn reserved<T>(capacity: usize) -> Result<Vec<T>, CfdError> {
	let mut result = Vec::new();
	result
		.try_reserve_exact(capacity)
		.map_err(|_| CfdError::InvalidInput("KdV allocation reservation failed"))?;
	Ok(result)
}
fn zeros(size: usize) -> Result<Vec<f64>, CfdError> {
	let mut result = reserved(size)?;
	result.resize(size, 0.);
	Ok(result)
}
fn admit(
	cells: u32,
	order: usize,
	mode: KdvMode,
	limits: KdvLimits,
) -> Result<(usize, usize), CfdError> {
	if cells < 2 || ![2, 3].contains(&order) {
		return Err(CfdError::InvalidInput(
			"KdV requires at least two cells and physical DG2 or DG3",
		));
	}
	let count =
		usize::try_from(cells).map_err(|_| CfdError::InvalidInput("KdV cell conversion"))?;
	let dimension = count
		.checked_mul(order + 1)
		.and_then(|n| n.checked_mul(2))
		.ok_or(CfdError::InvalidInput("KdV complete dimension overflow"))?;
	if dimension > limits.max_dimension
		|| dimension > 128
		|| dimension > limits.polynomial.max_variables
	{
		return Err(CfdError::InvalidInput(
			"complete KdV dimension exceeds budget",
		));
	}
	let size = order + 1;
	let per_row = 9 * size
		+ if mode == KdvMode::Nonlinear {
			7 * size * size
		} else {
			0
		};
	let terms = dimension
		.checked_mul(9 * size)
		.and_then(|n| {
			n.checked_add(if mode == KdvMode::Nonlinear {
				(dimension / 2).checked_mul(7 * size * size)?
			} else {
				0
			})
		})
		.ok_or(CfdError::InvalidInput("KdV term budget overflow"))?;
	let bytes = terms
		.checked_mul(
			dimension
				.checked_mul(256)
				.and_then(|n| n.checked_add(4096))
				.ok_or(CfdError::InvalidInput("KdV byte budget overflow"))?,
		)
		.and_then(|n| n.checked_add(dimension.checked_mul(4096)?))
		.ok_or(CfdError::InvalidInput("KdV byte budget overflow"))?;
	let work = terms
		.checked_mul(
			dimension
				.checked_add(32)
				.ok_or(CfdError::InvalidInput("KdV work overflow"))?,
		)
		.and_then(|n| n.checked_mul(32))
		.ok_or(CfdError::InvalidInput("KdV work overflow"))?;
	if bytes > limits.max_bytes || work > limits.max_work || terms > limits.polynomial.max_terms {
		return Err(CfdError::InvalidInput(
			"complete KdV assembly exceeds aggregate budget",
		));
	}
	Ok((dimension, per_row))
}
fn push(
	terms: &mut Vec<(Vec<u32>, RBig)>,
	dimension: usize,
	variables: &[usize],
	coefficient: f64,
	limits: PolynomialLimits,
) -> Result<(), CfdError> {
	if coefficient == 0. {
		return Ok(());
	}
	if !coefficient.is_finite() {
		return Err(CfdError::Assembly("KdV coefficient overflow"));
	}
	let mut powers = reserved(dimension)?;
	powers.resize(dimension, 0);
	for &i in variables {
		powers[i] += 1;
	}
	terms.push((
		powers,
		rational_constant(&ExactConstant::Binary64(coefficient), limits)?,
	));
	Ok(())
}
#[allow(
	clippy::too_many_lines,
	reason = "Direct weak-form volume and interface coefficients share one bounded assembly routine"
)]
fn forms(
	cells: usize,
	element: &Element,
	mode: KdvMode,
	dimension: usize,
	per_row: usize,
	limits: PolynomialLimits,
) -> Result<PolynomialOde, CfdError> {
	let size = element.size;
	let field_dimension = dimension / 2;
	let mut symbols = reserved(dimension)?;
	for i in 0..dimension {
		symbols.push(Symbol::new(
			Owner::new(0x4346_444b_4456_3031),
			u64::try_from(i).map_err(|_| CfdError::InvalidInput("KdV symbol index"))?,
		));
	}
	let mut components = reserved(dimension)?;
	for field in 0..2 {
		let sign = if field == 0 { 1. } else { -1. };
		for cell in 0..cells {
			for test in 0..size {
				let mut terms = reserved(per_row)?;
				for source in 0..size {
					let value = sign
						* element
							.quadrature
							.iter()
							.map(|point| {
								point.weight * point.basis[source][0] * point.basis[test][3]
							})
							.sum::<f64>();
					push(
						&mut terms,
						dimension,
						&[field * field_dimension + cell * size + source],
						value,
						limits,
					)?;
				}
				if field == 0 && mode == KdvMode::Nonlinear {
					for a in 0..size {
						for b in 0..size {
							let value = 3.
								* element
									.quadrature
									.iter()
									.map(|point| {
										point.weight
											* point.basis[a][0]
											* point.basis[b][0]
											* point.basis[test][1]
									})
									.sum::<f64>();
							push(
								&mut terms,
								dimension,
								&[cell * size + a, cell * size + b],
								value,
								limits,
							)?;
						}
					}
				}
				for (side, normal) in [(0, -1.), (1, 1.)] {
					let (left, right) = if side == 0 {
						((cell + cells - 1) % cells, cell)
					} else {
						(cell, (cell + 1) % cells)
					};
					let test_jet = element.edge[side][test];
					for source_field in 0..2 {
						for (neighbor, trace_side, jump_sign) in [(left, 1, -1.), (right, 0, 1.)] {
							let flux_weight = if source_field == field {
								0.5
							} else {
								0.5 * jump_sign
							};
							for (source, &source_jet) in
								element.edge[trace_side].iter().take(size).enumerate()
							{
								let bracket = source_jet[0] * test_jet[2]
									- source_jet[1] * test_jet[1]
									+ source_jet[2] * test_jet[0];
								push(
									&mut terms,
									dimension,
									&[source_field * field_dimension + neighbor * size + source],
									-sign * normal * flux_weight * bracket,
									limits,
								)?;
							}
						}
					}
					if field == 0 && mode == KdvMode::Nonlinear {
						for a in 0..size {
							for b in 0..size {
								for (first, second, value) in [
									(
										left * size + a,
										left * size + b,
										element.edge[1][a][0] * element.edge[1][b][0],
									),
									(
										left * size + a,
										right * size + b,
										element.edge[1][a][0] * element.edge[0][b][0],
									),
									(
										right * size + a,
										right * size + b,
										element.edge[0][a][0] * element.edge[0][b][0],
									),
								] {
									push(
										&mut terms,
										dimension,
										&[first, second],
										-normal * test_jet[0] * value,
										limits,
									)?;
								}
							}
						}
					}
				}
				let mut scope = reserved(symbols.len())?;
				scope.extend_from_slice(&symbols);
				components.push(SparsePolynomial::from_terms(scope, terms, limits)?);
			}
		}
	}
	PolynomialOde::from_polynomials(components, dimension, limits)
}
/// Both evolving DG fields in a complete mass-orthonormal Legendre chart, ordered [u,phi].
#[derive(Clone, Debug)]
pub struct KdvDg {
	cells: u32,
	order: usize,
	mode: KdvMode,
	element: Element,
	ode: Arc<PolynomialOde>,
}
impl KdvDg {
	/// Assemble nonlinear `KdV` on the frozen periodic interval, retaining both fields.
	/// # Errors
	/// Rejects unsupported degree, dimensions and aggregate construction budgets.
	pub fn new(cells: u32, order: usize) -> Result<Self, CfdError> {
		Self::with_limits(cells, order, KdvMode::Nonlinear, KdvLimits::default())
	}
	/// Assemble the separate linear Airy validation problem with the same doubled-field scheme.
	/// # Errors
	/// Rejects invalid or unadmitted construction.
	pub fn linear_airy(cells: u32, order: usize) -> Result<Self, CfdError> {
		Self::with_limits(cells, order, KdvMode::LinearAiry, KdvLimits::default())
	}
	/// Assemble with explicit complete-state resource limits.
	/// # Errors
	/// Rejects limits before term growth and propagates exact-algebra admission failures.
	pub fn with_limits(
		cells: u32,
		order: usize,
		mode: KdvMode,
		limits: KdvLimits,
	) -> Result<Self, CfdError> {
		let (dimension, per_row) = admit(cells, order, mode, limits)?;
		let element = Element::new(order, cells);
		let count = usize::try_from(cells).map_err(|_| CfdError::InvalidInput("KdV cells"))?;
		let ode = Arc::new(forms(
			count,
			&element,
			mode,
			dimension,
			per_row,
			limits.polynomial,
		)?);
		Ok(Self {
			cells,
			order,
			mode,
			element,
			ode,
		})
	}
	/// Complete dimension including the auxiliary field.
	#[must_use]
	pub fn dimension(&self) -> usize {
		self.ode.dimension()
	}
	/// Number of physical u coordinates (the auxiliary block has the same size).
	#[must_use]
	pub fn field_dimension(&self) -> usize {
		self.dimension() / 2
	}
	/// Physical DG degree.
	#[must_use]
	pub const fn order(&self) -> usize {
		self.order
	}
	/// Explicit nonlinear or analytic-validation equation.
	#[must_use]
	pub const fn mode(&self) -> KdvMode {
		self.mode
	}
	/// Full exact coefficient records and lowered numerical kernels.
	#[must_use]
	pub fn polynomial_ode(&self) -> Arc<PolynomialOde> {
		Arc::clone(&self.ode)
	}
	fn validate(&self, state: &[f64]) -> Result<(), CfdError> {
		if state.len() != self.dimension() || state.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("invalid full KdV state"));
		}
		Ok(())
	}
	/// Evaluate the directly assembled polynomial dynamics.
	/// # Errors
	/// Rejects malformed states and numerical overflow.
	pub fn drift(&self, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		self.ode.drift(0., state)
	}
	/// Independent physical quadrature and numerical-flux evaluation, without extracted coefficients.
	/// # Errors
	/// Rejects malformed states, allocation failure and numerical overflow.
	pub fn direct_drift(&self, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		self.validate(state)?;
		let count = usize::try_from(self.cells)
			.map_err(|_| CfdError::InvalidInput("KdV cell conversion"))?;
		let size = self.element.size;
		let field_size = self.field_dimension();
		let mut fluxes = reserved(count)?;
		for left in 0..count {
			let right = (left + 1) % count;
			let mut trace = [[[0.; 3]; 2]; 2];
			for field in 0..2 {
				for (side, cell, edge) in [(0, left, 1), (1, right, 0)] {
					for derivative in 0..3 {
						trace[field][side][derivative] = (0..size)
							.map(|i| {
								state[field * field_size + cell * size + i]
									* self.element.edge[edge][i][derivative]
							})
							.sum();
					}
				}
			}
			let mut flux = [[0.; 3]; 2];
			for field in 0..2 {
				for derivative in 0..3 {
					flux[field][derivative] =
						f64::midpoint(trace[field][0][derivative], trace[field][1][derivative])
							+ 0.5
								* (trace[1 - field][1][derivative]
									- trace[1 - field][0][derivative]);
				}
			}
			let left_value = trace[0][0][0];
			let right_value = trace[0][1][0];
			#[allow(
				clippy::suspicious_operation_groupings,
				reason = "Entropy flux is the symmetric quadratic sum for physical f(u)=3u^2"
			)]
			let convection =
				left_value * left_value + left_value * right_value + right_value * right_value;
			fluxes.push((flux, convection));
		}
		let mut output = zeros(self.dimension())?;
		for field in 0..2 {
			let sign = if field == 0 { 1. } else { -1. };
			for cell in 0..count {
				for test in 0..size {
					let row = field * field_size + cell * size + test;
					let mut value = 0.;
					for point in &self.element.quadrature {
						let field_value = (0..size)
							.map(|i| {
								state[field * field_size + cell * size + i] * point.basis[i][0]
							})
							.sum::<f64>();
						value += sign * point.weight * field_value * point.basis[test][3];
						if field == 0 && self.mode == KdvMode::Nonlinear {
							value += 3.
								* point.weight
								* field_value
								* field_value
								* point.basis[test][1];
						}
					}
					for (side, face, normal) in
						[(0, (cell + count - 1) % count, -1.), (1, cell, 1.)]
					{
						let (flux, convection) = fluxes[face];
						let test_jet = self.element.edge[side][test];
						value -= sign
							* normal
							* (flux[field][0] * test_jet[2] - flux[field][1] * test_jet[1]
								+ flux[field][2] * test_jet[0]);
						if field == 0 && self.mode == KdvMode::Nonlinear {
							value -= normal * convection * test_jet[0];
						}
					}
					output[row] = value;
				}
			}
		}
		if output.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::Assembly("KdV direct residual overflow"));
		}
		Ok(output)
	}
	/// L2 project both full fields using eight-point Gauss quadrature in every cell.
	/// # Errors
	/// Rejects nonfinite field values, allocation failure and coefficient overflow.
	pub fn project_fields(
		&self,
		physical: impl Fn(f64) -> f64,
		auxiliary: impl Fn(f64) -> f64,
	) -> Result<Vec<f64>, CfdError> {
		let mut output = zeros(self.dimension())?;
		let size = self.element.size;
		let field_size = self.field_dimension();
		for cell in 0..self.cells {
			let cell_index =
				usize::try_from(cell).map_err(|_| CfdError::InvalidInput("KdV projection cell"))?;
			for point in &self.element.quadrature {
				let x = self.element.h * (f64::from(cell) + f64::midpoint(point.xi, 1.));
				let values = [physical(x), auxiliary(x)];
				if values.iter().any(|v| !v.is_finite()) {
					return Err(CfdError::InvalidInput("nonfinite KdV projection field"));
				}
				for field in 0..2 {
					for mode in 0..size {
						output[field * field_size + cell_index * size + mode] +=
							point.weight * values[field] * point.basis[mode][0];
					}
				}
			}
		}
		self.validate(&output)?;
		Ok(output)
	}
	/// Project amplitude*cos(x) and an exactly zero auxiliary initial field.
	/// # Errors
	/// Rejects nonfinite amplitude and numerical overflow.
	pub fn initial_state(&self, amplitude: f64) -> Result<Vec<f64>, CfdError> {
		if !amplitude.is_finite() {
			return Err(CfdError::InvalidInput("nonfinite KdV amplitude"));
		}
		self.project_fields(|x| amplitude * x.cos(), |_| 0.)
	}
	/// Physical and auxiliary squared L2 norms (no factor one half).
	/// # Errors
	/// Rejects malformed states and norm overflow.
	pub fn field_energies(&self, state: &[f64]) -> Result<[f64; 2], CfdError> {
		self.validate(state)?;
		let split = self.field_dimension();
		let result = [
			state[..split].iter().map(|v| v * v).sum::<f64>(),
			state[split..].iter().map(|v| v * v).sum::<f64>(),
		];
		if result.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("KdV energy overflow"));
		}
		Ok(result)
	}
	/// Conserved semidiscrete combined squared L2 norm; either field alone can exchange energy.
	/// # Errors
	/// Rejects malformed states or total-energy overflow.
	pub fn energy(&self, state: &[f64]) -> Result<f64, CfdError> {
		let energies = self.field_energies(state)?;
		let total = energies[0] + energies[1];
		if !total.is_finite() {
			return Err(CfdError::InvalidInput("KdV total energy overflow"));
		}
		Ok(total)
	}
	/// Auxiliary-field L2 norm, reported rather than discarded.
	/// # Errors
	/// Rejects invalid states and norm overflow.
	pub fn auxiliary_norm(&self, state: &[f64]) -> Result<f64, CfdError> {
		Ok(self.field_energies(state)?[1].sqrt())
	}
	/// Integrals of both fields, using their retained constant cell modes.
	/// # Errors
	/// Rejects malformed states and sum overflow.
	pub fn field_masses(&self, state: &[f64]) -> Result<[f64; 2], CfdError> {
		self.validate(state)?;
		let mut masses = [0.; 2];
		for (field, mass) in masses.iter_mut().enumerate() {
			*mass = state[field * self.field_dimension()..(field + 1) * self.field_dimension()]
				.iter()
				.step_by(self.element.size)
				.sum::<f64>()
				* self.element.h.sqrt();
		}
		if masses.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("KdV mass overflow"));
		}
		Ok(masses)
	}
	/// Independent physical L2 error against the positive-direction Airy phase cos(x+t).
	/// # Errors
	/// Rejects use on nonlinear `KdV`, invalid time/amplitude or malformed states.
	pub fn airy_l2_error(&self, state: &[f64], time: f64, amplitude: f64) -> Result<f64, CfdError> {
		self.validate(state)?;
		if self.mode != KdvMode::LinearAiry
			|| !time.is_finite()
			|| time < 0.
			|| !amplitude.is_finite()
		{
			return Err(CfdError::InvalidInput(
				"Airy error requires linear Airy mode and finite nonnegative time",
			));
		}
		let mut error = 0.;
		for cell in 0..self.cells {
			let index =
				usize::try_from(cell).map_err(|_| CfdError::InvalidInput("Airy error cell"))?;
			for point in &self.element.quadrature {
				let x = self.element.h * (f64::from(cell) + f64::midpoint(point.xi, 1.));
				let value = (0..self.element.size)
					.map(|i| state[index * self.element.size + i] * point.basis[i][0])
					.sum::<f64>();
				error += point.weight * (value - amplitude * (x + time).cos()).powi(2);
			}
		}
		if !error.is_finite() {
			return Err(CfdError::InvalidInput("Airy error overflow"));
		}
		Ok(error.sqrt())
	}
}
/// Executed complete-state classical reference; this is not quantum execution.
#[derive(Clone, Debug, serde::Serialize)]
pub struct KdvReference {
	/// Complete evolving coordinate count, including phi.
	pub dimension: usize,
	/// Physical cell count.
	pub cells: u32,
	/// Physical polynomial degree.
	pub order: usize,
	/// Explicit equation selection.
	pub mode: KdvMode,
	/// Final physical time.
	pub horizon: f64,
	/// Actual number of classical RK4 steps.
	pub steps: u32,
	/// All final physical and auxiliary coordinates, ordered [u,phi].
	pub state: Vec<f64>,
	/// Analytic Airy physical error, unavailable for nonlinear `KdV`.
	pub airy_l2_error: Option<f64>,
	/// Auxiliary field norm, which is never silently set to zero.
	pub auxiliary_l2_norm: f64,
	/// Initial combined squared L2 norm.
	pub initial_energy: f64,
	/// Final combined squared L2 norm; RK4 does not conserve this exactly.
	pub final_energy: f64,
	/// Initial physical and auxiliary integrals.
	pub initial_masses: [f64; 2],
	/// Final physical and auxiliary integrals.
	pub final_masses: [f64; 2],
	/// Explicit execution and analytic-reference status.
	pub status: String,
}
impl KdvDg {
	/// Integrate the independently evaluated full DG flux residual with classical RK4.
	/// # Errors
	/// Rejects invalid steps/state, more than one million steps, more than one billion
	/// modeled scalar operations, allocation failures, and numerical overflow.
	pub fn integrate_rk4(
		&self,
		initial: &[f64],
		dt: f64,
		steps: u32,
	) -> Result<Vec<f64>, CfdError> {
		self.validate(initial)?;
		let work = usize::try_from(steps)
			.ok()
			.and_then(|n| n.checked_mul(self.dimension()))
			.and_then(|n| n.checked_mul(self.element.size * 40 + 32));
		if !dt.is_finite()
			|| dt <= 0.
			|| steps > 1_000_000
			|| work.is_none_or(|n| n > 1_000_000_000)
		{
			return Err(CfdError::InvalidInput(
				"KdV classical integration exceeds step/work admission",
			));
		}
		let mut state = reserved(self.dimension())?;
		state.extend_from_slice(initial);
		for _ in 0..steps {
			let k1 = self.direct_drift(&state)?;
			let shift = |slope: &[f64], factor: f64| -> Result<Vec<f64>, CfdError> {
				let mut next = reserved(state.len())?;
				next.extend(state.iter().zip(slope).map(|(a, b)| a + factor * dt * b));
				Ok(next)
			};
			let k2 = self.direct_drift(&shift(&k1, 0.5)?)?;
			let k3 = self.direct_drift(&shift(&k2, 0.5)?)?;
			let k4 = self.direct_drift(&shift(&k3, 1.)?)?;
			for i in 0..state.len() {
				state[i] += dt * (k1[i] + 2. * k2[i] + 2. * k3[i] + k4[i]) / 6.;
			}
		}
		self.validate(&state)?;
		Ok(state)
	}
	/// Execute the full reference with explicit energy/mean and auxiliary diagnostics.
	/// # Errors
	/// Rejects invalid parameters, unadmitted integration and numerical overflow.
	pub fn classical_reference(
		&self,
		amplitude: f64,
		horizon: f64,
		steps: u32,
	) -> Result<KdvReference, CfdError> {
		if !horizon.is_finite() || horizon <= 0. || steps == 0 {
			return Err(CfdError::InvalidInput(
				"invalid KdV reference horizon/steps",
			));
		}
		let initial = self.initial_state(amplitude)?;
		let initial_energy = self.energy(&initial)?;
		let initial_masses = self.field_masses(&initial)?;
		let state = self.integrate_rk4(&initial, horizon / f64::from(steps), steps)?;
		let final_energy = self.energy(&state)?;
		let final_masses = self.field_masses(&state)?;
		let auxiliary_l2_norm = self.auxiliary_norm(&state)?;
		let airy_l2_error = if self.mode == KdvMode::LinearAiry {
			Some(self.airy_l2_error(&state, horizon, amplitude)?)
		} else {
			None
		};
		Ok(KdvReference{dimension:self.dimension(),cells:self.cells,order:self.order,mode:self.mode,horizon,steps,state,airy_l2_error,auxiliary_l2_norm,initial_energy,final_energy,initial_masses,final_masses,status:"executed classical complete doubled-field DG reference; no quantum execution; analytic Airy comparison only in LinearAiry mode".into()})
	}
}
