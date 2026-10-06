//! Complete quadratic dynamics built from exact shared `MathCore` polynomials.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Bounded coefficient indices and finite checked numerical formulas"
)]
use crate::CfdError;
use mathcore::{
	RBig,
	exact::Symbol,
	multivariate::{PolynomialKernel, PolynomialLimits, SparsePolynomial},
};
use quest_numerics::{
	Interval,
	arithmetic::{F64Backend, Interval64Backend},
};
use std::collections::BTreeMap;

/// One canonical state monomial with a prepared polynomial-in-time coefficient.
#[derive(Clone, Debug)]
pub struct CoefficientTerm {
	/// Equation index in the complete physical chart.
	pub row: usize,
	/// State exponents; every physical coordinate is represented.
	pub powers: Vec<u32>,
	symbolic: SparsePolynomial,
	kernel: PolynomialKernel<f64>,
	enclosure: PolynomialKernel<Interval>,
}
impl CoefficientTerm {
	/// Evaluate the prepared coefficient, with optional time in the final symbol slot.
	/// # Errors
	/// Rejects nonfinite time or numerical overflow.
	pub fn value(&self, time: f64) -> Result<f64, CfdError> {
		let mut inputs = vec![0.; self.symbolic.symbols().len()];
		if inputs.len() > self.powers.len()
			&& let Some(last) = inputs.last_mut()
		{
			*last = time;
		}
		if !time.is_finite() {
			return Err(CfdError::InvalidInput("nonfinite polynomial time"));
		}
		Ok(self.kernel.evaluate(&mut F64Backend, &inputs)?)
	}
	/// Exact coefficient polynomial with zero state exponents.
	#[must_use]
	pub const fn symbolic(&self) -> &SparsePolynomial {
		&self.symbolic
	}
	pub(crate) fn enclosure(&self, horizon: f64) -> Result<Interval, CfdError> {
		let mut inputs = vec![Interval::point(0.)?; self.symbolic.symbols().len()];
		if inputs.len() > self.powers.len()
			&& let Some(last) = inputs.last_mut()
		{
			*last = Interval::new(0., horizon)?;
		}
		Ok(self.enclosure.evaluate(&mut Interval64Backend, &inputs)?)
	}
}

/// All physical equations `F0(t)+F1(t)a+F2(t)(a tensor a)`, with no coordinate reduction.
#[derive(Clone, Debug)]
pub struct PolynomialOde {
	dimension: usize,
	components: Vec<SparsePolynomial>,
	kernels: Vec<PolynomialKernel<f64>>,
	derivatives: Vec<(usize, usize, PolynomialKernel<f64>)>,
	coefficients: Vec<CoefficientTerm>,
	limits: PolynomialLimits,
	retained_bytes: usize,
}
impl PolynomialOde {
	/// Import exact polynomials; symbols are all state coordinates followed optionally by time.
	/// # Errors
	/// Rejects incompatible scopes, nonquadratic state dependence and aggregate budgets.
	#[allow(
		clippy::too_many_lines,
		reason = "Admission, exact grouping and lowering form one auditable constructor"
	)]
	pub fn from_polynomials(
		components: Vec<SparsePolynomial>,
		dimension: usize,
		limits: PolynomialLimits,
	) -> Result<Self, CfdError> {
		if dimension == 0 || components.len() != dimension {
			return Err(CfdError::InvalidInput(
				"complete polynomial ODE requires one equation per coordinate",
			));
		}
		let symbols = components[0].symbols();
		if !(symbols.len() == dimension
			|| symbols.len()
				== dimension
					.checked_add(1)
					.ok_or(CfdError::InvalidInput("polynomial dimension overflow"))?)
			|| components.iter().any(|p| p.symbols() != symbols)
		{
			return Err(CfdError::InvalidInput(
				"incompatible polynomial state/time symbols",
			));
		}
		let mut input_bytes = components
			.capacity()
			.checked_mul(size_of::<SparsePolynomial>())
			.ok_or(CfdError::InvalidInput(
				"polynomial equation capacity overflow",
			))?;
		let mut work = 0usize;
		let mut term_count = 0usize;
		for p in &components {
			input_bytes = input_bytes
				.checked_add(p.retained_bytes()?)
				.ok_or(CfdError::InvalidInput("polynomial storage overflow"))?;
			work = work
				.checked_add(p.logical_work())
				.ok_or(CfdError::InvalidInput("polynomial work overflow"))?;
			term_count = term_count
				.checked_add(p.terms().count())
				.ok_or(CfdError::InvalidInput("polynomial term count overflow"))?;
			for (powers, _) in p.terms() {
				if powers[..dimension]
					.iter()
					.try_fold(0_u32, |a, b| a.checked_add(*b))
					.is_none_or(|d| d > 2)
				{
					return Err(CfdError::Unsupported(
						"physical polynomial extraction requires state degree at most two".into(),
					));
				}
			}
		}
		// Before grouping or lowering: exact coefficient copies, two numerical backends,
		// scope vectors per coefficient/derivative, and transient construction containers.
		let retained_bytes = input_bytes
			.checked_mul(16)
			.and_then(|n| {
				n.checked_add(
					term_count.checked_mul(symbols.len().checked_mul(128)?.checked_add(1024)?)?,
				)
			})
			.and_then(|n| n.checked_add(dimension.checked_mul(512)?))
			.ok_or(CfdError::InvalidInput(
				"polynomial aggregate storage overflow",
			))?;
		let work = work
			.checked_mul(
				dimension
					.checked_add(4)
					.ok_or(CfdError::InvalidInput("polynomial work overflow"))?,
			)
			.and_then(|n| {
				n.checked_add(
					term_count
						.checked_mul(symbols.len().checked_add(8)?)?
						.checked_mul(8)?,
				)
			})
			.ok_or(CfdError::InvalidInput("polynomial aggregate work overflow"))?;
		if symbols.len() > limits.max_variables
			|| term_count > limits.max_terms
			|| retained_bytes > limits.max_bytes
			|| work > limits.max_work
		{
			return Err(CfdError::InvalidInput(
				"complete polynomial ODE exceeds aggregate budget",
			));
		}
		let mut coefficients = Vec::new();
		let mut kernels = Vec::new();
		let mut derivatives = Vec::new();
		for (row, p) in components.iter().enumerate() {
			let mut grouped: BTreeMap<Vec<u32>, Vec<(Vec<u32>, RBig)>> = BTreeMap::new();
			for (powers, value) in p.terms() {
				let degree = powers[..dimension]
					.iter()
					.try_fold(0_u32, |a, b| a.checked_add(*b))
					.ok_or(CfdError::InvalidInput("polynomial degree overflow"))?;
				if degree > 2 {
					return Err(CfdError::Unsupported(
						"physical polynomial extraction requires state degree at most two".into(),
					));
				}
				let mut time_powers = vec![0; symbols.len()];
				if symbols.len() > dimension {
					time_powers[dimension] = powers[dimension];
				}
				grouped
					.entry(powers[..dimension].to_vec())
					.or_default()
					.push((time_powers, value.clone()));
			}
			for (powers, terms) in grouped {
				let symbolic = SparsePolynomial::from_terms(symbols.to_vec(), terms, limits)?;
				let kernel = symbolic.lower(&mut F64Backend)?;
				let enclosure = symbolic.lower(&mut Interval64Backend)?;
				coefficients.push(CoefficientTerm {
					row,
					powers,
					symbolic,
					kernel,
					enclosure,
				});
			}
			for (column, &symbol) in symbols.iter().take(dimension).enumerate() {
				if p.terms().any(|(powers, _)| powers[column] > 0) {
					let derivative = p.differentiate(symbol)?;
					derivatives.push((row, column, derivative.lower(&mut F64Backend)?));
				}
			}
			kernels.push(p.lower(&mut F64Backend)?);
			if retained_bytes > limits.max_bytes
				|| work > limits.max_work
				|| coefficients.len() > limits.max_terms
			{
				return Err(CfdError::InvalidInput(
					"complete polynomial ODE exceeds aggregate budget",
				));
			}
		}
		Ok(Self {
			dimension,
			components,
			kernels,
			derivatives,
			coefficients,
			limits,
			retained_bytes,
		})
	}
	/// Complete physical dimension.
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.dimension
	}
	/// Exact equations supplied to the shared compiler.
	#[must_use]
	pub fn components(&self) -> &[SparsePolynomial] {
		&self.components
	}
	/// State symbols followed optionally by time.
	#[must_use]
	pub fn symbols(&self) -> &[Symbol] {
		self.components[0].symbols()
	}
	/// Canonical coefficients without a dense cubic tensor.
	#[must_use]
	pub fn coefficient_terms(&self) -> &[CoefficientTerm] {
		&self.coefficients
	}
	/// Construction limits carried to subsequent symbolic consumers.
	#[must_use]
	pub const fn limits(&self) -> PolynomialLimits {
		self.limits
	}
	/// Conservatively modeled retained exact forms and prepared kernels.
	#[must_use]
	pub const fn retained_bytes(&self) -> usize {
		self.retained_bytes
	}
	/// Whether the external source is symbolically zero for every time, not merely sampled zero.
	#[must_use]
	pub fn source_identically_zero(&self) -> bool {
		self.coefficients
			.iter()
			.all(|t| t.powers.iter().any(|&p| p != 0))
	}
	fn inputs(&self, time: f64, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		if !time.is_finite()
			|| state.len() != self.dimension
			|| state.iter().any(|v| !v.is_finite())
		{
			return Err(CfdError::InvalidInput(
				"invalid complete polynomial state or time",
			));
		}
		let mut inputs = state.to_vec();
		if self.symbols().len() > self.dimension {
			inputs.push(time);
		}
		Ok(inputs)
	}
	/// Evaluate lowered kernels; no symbolic work occurs in the numerical loop.
	/// # Errors
	/// Rejects malformed states, nonfinite time or arithmetic overflow.
	pub fn drift(&self, time: f64, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		let inputs = self.inputs(time, state)?;
		self.kernels
			.iter()
			.map(|k| Ok(k.evaluate(&mut F64Backend, &inputs)?))
			.collect()
	}
	/// Evaluate the shared symbolic derivatives as a bounded dense Jacobian.
	/// # Errors
	/// Rejects malformed inputs and allocations exceeding the retained construction budget.
	pub fn jacobian(&self, time: f64, state: &[f64]) -> Result<Vec<Vec<f64>>, CfdError> {
		let bytes = self
			.dimension
			.checked_mul(self.dimension)
			.and_then(|n| n.checked_mul(8))
			.and_then(|n| n.checked_add(self.retained_bytes))
			.ok_or(CfdError::InvalidInput("Jacobian size overflow"))?;
		if bytes > self.limits.max_bytes {
			return Err(CfdError::InvalidInput("Jacobian exceeds storage budget"));
		}
		let inputs = self.inputs(time, state)?;
		let mut result = vec![vec![0.; self.dimension]; self.dimension];
		for (i, j, kernel) in &self.derivatives {
			result[*i][*j] = kernel.evaluate(&mut F64Backend, &inputs)?;
		}
		Ok(result)
	}
	/// Classical RK4 of this same complete ODE, including time-dependent coefficients.
	/// # Errors
	/// Rejects invalid steps, malformed data and arithmetic overflow.
	pub fn integrate_rk4(
		&self,
		initial: &[f64],
		dt: f64,
		steps: u32,
	) -> Result<Vec<f64>, CfdError> {
		if !dt.is_finite() || dt <= 0. || steps > 1_000_000 {
			return Err(CfdError::InvalidInput("invalid polynomial reference step"));
		}
		self.inputs(0., initial)?;
		let attempt = crate::reference_rk4::integrate(
			initial.to_vec(),
			dt,
			steps,
			usize::MAX,
			|time, state| self.drift(time, state),
			|_, time, state| self.inputs(time, state).map(|_| ()),
		);
		attempt.outcome?;
		Ok(attempt.state)
	}
}

/// Outward interval bounds on actual coefficients over the complete time interval.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CoefficientEvidence {
	/// Spectral norm is bounded by the matrix Frobenius norm.
	pub linear_norm_upper: f64,
	/// Frobenius bound for the symmetric ordered-pair representation of F2.
	pub quadratic_norm_upper: f64,
	/// Maximum absolute ordered-pair entry, a lower bound on the F2 spectral norm.
	pub quadratic_norm_lower: f64,
	/// Supremum source Euclidean norm over the entire interval.
	pub forcing_norm_upper: f64,
	/// Gershgorin upper bound on `lambda_max((F1+F1^T)/2)`, not an eigenvalue decay assumption.
	pub logarithmic_norm_upper: f64,
	/// Whether F1 and F2 are autonomous, as required by the cited Liu setup.
	pub autonomous_linear_quadratic: bool,
	/// This checks coefficients; no full convergence theorem is automatically certified.
	pub theorem_status: String,
}
impl PolynomialOde {
	/// Compute interval norm and logarithmic-norm evidence without a dense F2 tensor.
	/// # Errors
	/// Rejects invalid horizons, interval overflow and unbudgeted dense F1 storage.
	pub fn coefficient_evidence(&self, horizon: f64) -> Result<CoefficientEvidence, CfdError> {
		if !horizon.is_finite() || horizon < 0. {
			return Err(CfdError::InvalidInput(
				"invalid coefficient evidence horizon",
			));
		}
		let bytes = self
			.dimension
			.checked_mul(self.dimension)
			.and_then(|n| n.checked_mul(size_of::<Interval>()))
			.and_then(|n| n.checked_add(self.retained_bytes))
			.ok_or(CfdError::InvalidInput("coefficient evidence size overflow"))?;
		if bytes > self.limits.max_bytes {
			return Err(CfdError::InvalidInput(
				"coefficient evidence exceeds storage budget",
			));
		}
		let zero = Interval::point(0.)?;
		let mut linear = vec![vec![zero; self.dimension]; self.dimension];
		let mut force = vec![zero; self.dimension];
		let mut quadratic = zero;
		let mut qlower = 0_f64;
		let mut autonomous = true;
		for term in &self.coefficients {
			let coefficient = term.enclosure(horizon)?;
			let degree: u32 = term.powers.iter().sum();
			if degree > 0
				&& self.symbols().len() > self.dimension
				&& term.symbolic.terms().any(|(p, _)| p[self.dimension] != 0)
			{
				autonomous = false;
			}
			if degree == 0 {
				force[term.row] = coefficient;
			} else if degree == 1 {
				let column = term
					.powers
					.iter()
					.position(|&p| p == 1)
					.ok_or(CfdError::Assembly("linear monomial"))?;
				linear[term.row][column] = coefficient;
			} else {
				let mixed = term.powers.iter().filter(|&&p| p > 0).count() == 2;
				let factor = if mixed { 0.5 } else { 1. };
				quadratic = quadratic.checked_add(
					coefficient
						.square()?
						.checked_mul(Interval::point(factor)?)?,
				)?;
				let lower = if coefficient.contains(0.) {
					0.
				} else {
					coefficient.lower().abs().min(coefficient.upper().abs())
				};
				qlower = qlower.max(
					Interval::point(lower)?
						.checked_mul(Interval::point(factor)?)?
						.lower(),
				);
			}
		}
		let mut linear_square = zero;
		let mut force_square = zero;
		let mut lognorm = f64::NEG_INFINITY;
		for i in 0..self.dimension {
			force_square = force_square.checked_add(force[i].square()?)?;
			let mut bound = linear[i][i];
			for j in 0..self.dimension {
				linear_square = linear_square.checked_add(linear[i][j].square()?)?;
				if i != j {
					let entry = linear[i][j]
						.checked_add(linear[j][i])?
						.checked_mul(Interval::point(0.5)?)?;
					let magnitude = entry.lower().abs().max(entry.upper().abs());
					bound = bound.checked_add(Interval::point(magnitude)?)?;
				}
			}
			lognorm = lognorm.max(bound.upper());
		}
		Ok(CoefficientEvidence {
			linear_norm_upper: linear_square.sqrt()?.upper(),
			quadratic_norm_upper: quadratic.sqrt()?.upper(),
			quadratic_norm_lower: qlower,
			forcing_norm_upper: force_square.sqrt()?.upper(),
			logarithmic_norm_upper: lognorm,
			autonomous_linear_quadratic: autonomous,
			theorem_status:
				"coefficient bounds only; complete Carleman convergence hypotheses not certified"
					.into(),
		})
	}
}

/// Explicit normalization choice and sufficient-condition diagnostics.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ScalingEvidence {
	/// None means the exact zero trajectory, which has no normalized quantum state.
	pub scale: Option<f64>,
	/// Initial Euclidean norm in the complete physical coordinates.
	pub initial_norm: f64,
	/// A conservative RC bound using logarithmic-norm dissipation, if defined.
	pub rc_upper: Option<f64>,
	/// Whether scaled forcing upper <= scaled quadratic lower, the additional Liu premise.
	pub corrected_forcing_hypothesis: bool,
	/// Exact-zero, experimental scaling or insufficient-dissipation evidence; never divergence.
	pub status: String,
}
impl PolynomialOde {
	/// Choose s=2||a0||, or a positive forcing-based scale for zero initial state.
	/// # Errors
	/// Rejects invalid inputs, unrepresentable scales and coefficient-bound failures.
	pub fn experimental_scaling(
		&self,
		initial: &[f64],
		horizon: f64,
	) -> Result<ScalingEvidence, CfdError> {
		self.inputs(0., initial)?;
		if !horizon.is_finite() || horizon <= 0. {
			return Err(CfdError::InvalidInput(
				"positive horizon required for scaling",
			));
		}
		let norm = initial.iter().fold(0_f64, |a, &b| a.hypot(b));
		if norm == 0. && self.source_identically_zero() {
			return Ok(ScalingEvidence {
				scale: None,
				initial_norm: 0.,
				rc_upper: None,
				corrected_forcing_hypothesis: false,
				status: "exact zero trajectory; normalized quantum solve is undefined".into(),
			});
		}
		let evidence = self.coefficient_evidence(horizon)?;
		let scale = if norm > 0. {
			2. * norm
		} else {
			2. * horizon * evidence.forcing_norm_upper
		};
		if !scale.is_finite() || scale <= 0. {
			return Err(CfdError::InvalidInput(
				"forcing/initial scaling is unrepresentable",
			));
		}
		let mut norm_square = Interval::point(0.)?;
		for &v in initial {
			norm_square = norm_square.checked_add(Interval::point(v)?.square()?)?;
		}
		let norm_bound = norm_square.sqrt()?;
		let rc_upper = if norm_bound.lower() > 0. && evidence.logarithmic_norm_upper < 0. {
			Some(
				norm_bound
					.checked_mul(Interval::point(evidence.quadratic_norm_upper)?)?
					.checked_add(
						Interval::point(evidence.forcing_norm_upper)?.checked_div(norm_bound)?,
					)?
					.checked_div(Interval::point(-evidence.logarithmic_norm_upper)?)?
					.upper(),
			)
		} else {
			None
		};
		let corrected_forcing_hypothesis = forcing_hypothesis(
			evidence.forcing_norm_upper,
			scale,
			evidence.quadratic_norm_lower,
		)?;
		Ok(ScalingEvidence {
			scale: Some(scale),
			initial_norm: norm,
			rc_upper,
			corrected_forcing_hypothesis,
			status: if norm == 0. {
				"experimental forcing-based scale; RC undefined for zero initial state".into()
			} else {
				"experimental full-coordinate scale; insufficient theorem evidence is not divergence".into()
			},
		})
	}
}

/// Evidence for a numerical coefficient snapshot of a known quadratic DG assembly.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SnapshotEvidence {
	/// All original physical chart coordinates, including means.
	pub physical_dimension: usize,
	/// Actual calls to the existing assembled DG residual.
	pub residual_evaluations: usize,
	/// Maximum absolute error on independent full-state probes.
	pub independent_probe_max_error: f64,
	/// Maximum error divided by max(1, direct residual magnitude).
	pub independent_probe_scaled_error: f64,
	/// Heuristic floating-point subtraction scale, explicitly not an interval proof.
	pub coefficient_roundoff_scale_estimate: f64,
	/// Distinguishes exact recorded dyadics from the numerical extraction of the original model.
	pub status: String,
}
/// Complete polynomial snapshot with separate numerical extraction evidence.
#[derive(Clone, Debug)]
pub struct PolynomialSnapshot {
	/// Every physical equation, compiled from recorded binary64 coefficient dyadics.
	pub dynamics: PolynomialOde,
	/// Actual independent comparison evidence.
	pub evidence: SnapshotEvidence,
}
impl PolynomialOde {
	/// Snapshot the complete known quadratic two-triangle BDM1 assembly.
	/// # Errors
	/// Rejects budgets and failed independent numerical comparisons.
	pub fn from_periodic_bdm1(
		model: &crate::PeriodicBdm1,
		limits: PolynomialLimits,
	) -> Result<PolynomialSnapshot, CfdError> {
		snapshot_quadratic(model.dimension(), |x| model.drift(x), limits)
	}
	/// Snapshot the complete known quadratic simplex BDM1 assembly, including stationary lifting.
	/// # Errors
	/// Rejects budgets and failed independent numerical comparisons. Time-varying boundary
	/// lifting is not supplied by this autonomous model and is not inferred by this adapter.
	pub fn from_simplex_bdm1(
		model: &crate::simplex::SimplexBdm,
		limits: PolynomialLimits,
	) -> Result<PolynomialSnapshot, CfdError> {
		snapshot_quadratic(model.dimension(), |x| model.drift(x), limits)
	}
	/// Snapshot every coordinate of a bounded BDM1/P0 or BDM2/P1 physical assembly.
	/// # Errors
	/// Rejects extraction beyond 64 independent coordinates, budget exhaustion and
	/// failed independent comparisons. Recorded dyadics do not certify assembly roundoff.
	pub fn from_physical_space(
		model: &crate::physical_space::PhysicalSpace,
		limits: PolynomialLimits,
	) -> Result<PolynomialSnapshot, CfdError> {
		snapshot_quadratic(model.dimension(), |x| model.drift(x), limits)
	}
	/// Snapshot complete BDM1 dynamics with prescribed trace scale `g(t)=offset+rate*t`.
	///
	/// The known residual is quadratic jointly in chart coordinates and this affine
	/// time parameter. Polarization therefore captures the entire polynomial, including
	/// `-Q^T M l g'`. The last symbol becomes external time after extraction; it is
	/// never an additional physical coordinate in either lifted representation.
	/// # Errors
	/// Rejects nonfinite boundary data, complete extraction budgets and failed probes.
	pub fn from_simplex_bdm1_affine_boundary(
		model: &crate::simplex::SimplexBdm,
		offset: f64,
		rate: f64,
		limits: PolynomialLimits,
	) -> Result<PolynomialSnapshot, CfdError> {
		if !offset.is_finite() || !rate.is_finite() {
			return Err(CfdError::InvalidInput("nonfinite affine boundary data"));
		}
		let dimension = model.dimension();
		let mut snapshot = snapshot_quadratic(
			dimension
				.checked_add(1)
				.ok_or(CfdError::InvalidInput("boundary scope overflow"))?,
			|x| {
				let mut result = model.drift_with_boundary_scale(
					&x[..dimension],
					offset + rate * x[dimension],
					rate,
				)?;
				result.push(0.);
				Ok(result)
			},
			limits,
		)?;
		let Self { mut components, .. } = snapshot.dynamics;
		components.pop();
		snapshot.dynamics = Self::from_polynomials(components, dimension, limits)?;
		snapshot.evidence.physical_dimension = dimension;
		snapshot.evidence.status = "numerical centered-polarization snapshot of complete BDM1 with affine prescribed trace and lifting derivative; external time is not a physical coordinate; recorded coefficients are exact dyadics, probe evidence is not an exact assembly proof".into();
		Ok(snapshot)
	}
	/// Apply an explicitly supplied polynomial lifting u=a+l(t), including -dl/dt.
	/// # Errors
	/// Rejects scopes, state-dependent liftings, missing time symbols and symbolic budgets.
	pub fn with_lifting(&self, lifting: Vec<SparsePolynomial>) -> Result<Self, CfdError> {
		if lifting.len() != self.dimension
			|| self.symbols().len() != self.dimension + 1
			|| lifting.iter().any(|p| {
				p.symbols() != self.symbols()
					|| p.terms()
						.any(|(powers, _)| powers[..self.dimension].iter().any(|&e| e != 0))
			}) {
			return Err(CfdError::InvalidInput(
				"polynomial lifting requires one time-only polynomial per physical coordinate",
			));
		}
		let mut bytes = self.retained_bytes;
		for p in &lifting {
			bytes = bytes
				.checked_add(p.retained_bytes()?)
				.ok_or(CfdError::InvalidInput("lifting storage overflow"))?;
		}
		let expanded_terms = self
			.components
			.iter()
			.try_fold(0usize, |sum, p| sum.checked_add(p.terms().count()))
			.and_then(|n| {
				n.checked_mul(
					lifting
						.iter()
						.map(|p| p.terms().count())
						.max()
						.unwrap_or(0)
						.checked_add(1)?
						.checked_pow(2)?,
				)
			})
			.ok_or(CfdError::InvalidInput("lifting expansion budget overflow"))?;
		let expansion_bytes = expanded_terms
			.checked_mul(
				self.symbols()
					.len()
					.checked_mul(128)
					.and_then(|n| {
						n.checked_add(
							self.limits
								.max_coefficient_bits
								.checked_add(7)?
								.checked_div(8)?
								.checked_mul(64)?,
						)
					})
					.and_then(|n| n.checked_add(1024))
					.ok_or(CfdError::InvalidInput("lifting storage overflow"))?,
			)
			.and_then(|n| n.checked_add(bytes))
			.ok_or(CfdError::InvalidInput("lifting storage overflow"))?;
		if expanded_terms > self.limits.max_terms || expansion_bytes > self.limits.max_bytes {
			return Err(CfdError::InvalidInput(
				"polynomial lifting exceeds expansion budget",
			));
		}
		let substitutions = self
			.symbols()
			.iter()
			.take(self.dimension)
			.copied()
			.zip(&lifting)
			.enumerate()
			.map(|(index, (symbol, shift))| {
				Ok((
					symbol,
					SparsePolynomial::variable(self.symbols().to_vec(), index, self.limits)?
						.add(shift)?,
				))
			})
			.collect::<Result<BTreeMap<_, _>, CfdError>>()?;
		let time = self.symbols()[self.dimension];
		let components = self
			.components
			.iter()
			.zip(lifting)
			.map(|(p, l)| {
				Ok(p.substitute(&substitutions)?
					.subtract(&l.differentiate(time)?)?)
			})
			.collect::<Result<Vec<_>, CfdError>>()?;
		Self::from_polynomials(components, self.dimension, self.limits)
	}
}
// This crate-private adapter is used only for the known DG implementations and
// their complete polynomial-time boundary adapter.
// Finite sampling is deliberately not presented as a polynomiality proof for arbitrary closures.
#[allow(
	clippy::too_many_lines,
	reason = "One bounded numerical extraction records all probes and their evidence"
)]
pub(crate) fn snapshot_quadratic(
	dimension: usize,
	evaluate: impl Fn(&[f64]) -> Result<Vec<f64>, CfdError>,
	limits: PolynomialLimits,
) -> Result<PolynomialSnapshot, CfdError> {
	use mathcore::{arithmetic::ExactConstant, exact::Owner, multivariate::rational_constant};
	if dimension == 0 || dimension > 64 || dimension > limits.max_variables {
		return Err(CfdError::InvalidInput(
			"bounded BDM polynomial snapshot requires 1..64 complete coordinates",
		));
	}
	let per_equation = dimension
		.checked_add(1)
		.and_then(|n| n.checked_add(dimension.checked_mul(dimension.checked_add(1)?)? / 2))
		.ok_or(CfdError::InvalidInput("snapshot term count overflow"))?;
	let term_count = per_equation
		.checked_mul(dimension)
		.ok_or(CfdError::InvalidInput("snapshot term count overflow"))?;
	let bytes = term_count
		.checked_mul(
			dimension
				.checked_mul(128)
				.and_then(|n| n.checked_add(2048))
				.ok_or(CfdError::InvalidInput("snapshot storage overflow"))?,
		)
		.ok_or(CfdError::InvalidInput("snapshot storage overflow"))?;
	let work = term_count
		.checked_mul(
			dimension
				.checked_add(8)
				.ok_or(CfdError::InvalidInput("snapshot work overflow"))?,
		)
		.and_then(|n| n.checked_mul(16))
		.ok_or(CfdError::InvalidInput("snapshot work overflow"))?;
	if term_count > limits.max_terms || bytes > limits.max_bytes || work > limits.max_work {
		return Err(CfdError::InvalidInput(
			"complete BDM polynomial snapshot exceeds budget",
		));
	}
	let symbols = (0..dimension)
		.map(|i| {
			Ok(Symbol::new(
				Owner::new(0x4346_4442_444d_534e),
				u64::try_from(i).map_err(|_| CfdError::InvalidInput("snapshot symbol index"))?,
			))
		})
		.collect::<Result<Vec<_>, CfdError>>()?;
	let mut calls = 0usize;
	let mut max_sample = 0_f64;
	let mut sample = |state: &[f64]| {
		let value = evaluate(state)?;
		if value.len() != dimension || value.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::Assembly("invalid BDM snapshot sample"));
		}
		calls += 1;
		for v in &value {
			max_sample = max_sample.max(v.abs());
		}
		Ok(value)
	};
	let zero = sample(&vec![0.; dimension])?;
	let mut terms = vec![Vec::new(); dimension];
	let mut add = |row: usize, powers: Vec<u32>, value: f64| -> Result<(), CfdError> {
		if value != 0. {
			terms[row].push((
				powers,
				rational_constant(&ExactConstant::Binary64(value), limits)?,
			));
		}
		Ok(())
	};
	for (row, &value) in zero.iter().enumerate() {
		add(row, vec![0; dimension], value)?;
	}
	for axis in 0..dimension {
		let mut state = vec![0.; dimension];
		state[axis] = 1.;
		let positive = sample(&state)?;
		state[axis] = -1.;
		let negative = sample(&state)?;
		for row in 0..dimension {
			let mut p = vec![0; dimension];
			p[axis] = 1;
			add(row, p, 0.5 * (positive[row] - negative[row]))?;
			let mut p = vec![0; dimension];
			p[axis] = 2;
			add(
				row,
				p,
				0.5 * positive[row] + 0.5 * negative[row] - zero[row],
			)?;
		}
		for second in axis + 1..dimension {
			let mut values = Vec::new();
			for (a, b) in [(1., 1.), (1., -1.), (-1., 1.), (-1., -1.)] {
				state[axis] = a;
				state[second] = b;
				values.push(sample(&state)?);
			}
			state[second] = 0.;
			for row in 0..dimension {
				let mut p = vec![0; dimension];
				p[axis] = 1;
				p[second] = 1;
				add(
					row,
					p,
					0.25 * (values[0][row] - values[1][row] - values[2][row] + values[3][row]),
				)?;
			}
		}
	}
	let components = terms
		.into_iter()
		.map(|t| SparsePolynomial::from_terms(symbols.clone(), t, limits))
		.collect::<Result<Vec<_>, _>>()?;
	let dynamics = PolynomialOde::from_polynomials(components, dimension, limits)?;
	let mut max_error = 0_f64;
	let mut scaled_error = 0_f64;
	for probe in 1..=7u32 {
		let state = (0..dimension)
			.map(|i| {
				let index =
					u32::try_from(i).map_err(|_| CfdError::InvalidInput("snapshot probe index"))?;
				Ok((f64::from((index + 1) * probe) * 0.731).sin() * (f64::from(probe) * 0.17))
			})
			.collect::<Result<Vec<_>, CfdError>>()?;
		let direct = sample(&state)?;
		let reconstructed = dynamics.drift(0., &state)?;
		for (a, b) in direct.iter().zip(reconstructed) {
			let error = (a - b).abs();
			max_error = max_error.max(error);
			scaled_error = scaled_error.max(error / a.abs().max(1.));
		}
	}
	if scaled_error > 2e-10 {
		return Err(CfdError::Assembly(
			"BDM quadratic snapshot failed independent probes",
		));
	}
	Ok(PolynomialSnapshot{dynamics,evidence:SnapshotEvidence{physical_dimension:dimension,residual_evaluations:calls,independent_probe_max_error:max_error,independent_probe_scaled_error:scaled_error,coefficient_roundoff_scale_estimate:128.*f64::EPSILON*max_sample,status:"numerical centered-polarization snapshot of known quadratic BDM assembly; recorded coefficients are exact dyadics, but probe and roundoff evidence is not an exact assembly proof".into()}})
}

fn forcing_hypothesis(
	forcing_upper: f64,
	scale: f64,
	quadratic_lower: f64,
) -> Result<bool, CfdError> {
	let left = Interval::point(forcing_upper)?.checked_div(Interval::point(scale)?)?;
	let right = Interval::point(scale)?.checked_mul(Interval::point(quadratic_lower)?)?;
	Ok(left.upper() <= right.lower())
}
#[cfg(test)]
#[allow(
	clippy::panic_in_result_fn,
	reason = "Independent exact dyadic boundary regression"
)]
mod forcing_boundary_test {
	#[test]
	fn rounded_equality_does_not_prove_the_forcing_premise() -> Result<(), super::CfdError> {
		// Exact dyadics give lhs > rhs, although both nearest operations round equal.
		use mathcore::{
			arithmetic::ExactConstant,
			multivariate::{PolynomialLimits, rational_constant},
		};
		let exact = |x| rational_constant(&ExactConstant::Binary64(x), PolynomialLimits::default());
		let f = exact(2.923_015_864_176_111_5)?;
		let s = exact(1.650_934_473_039_853_9)?;
		let q = exact(1.072_436_286_667_542_8)?;
		assert!(f / &s > s * q);
		assert!(!super::forcing_hypothesis(
			2.923_015_864_176_111_5,
			1.650_934_473_039_853_9,
			1.072_436_286_667_542_8
		)?);
		Ok(())
	}
}
