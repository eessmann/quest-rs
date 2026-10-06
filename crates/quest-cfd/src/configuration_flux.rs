//! Instantaneous exterior LGL trace flux in every configuration coordinate.
//!
//! These rates have units inverse time. Periodic configuration evolution does not
//! lose this probability: the diagnostic is neither an integrated escape bound
//! nor a convergence certificate. Outer-cell occupation is reported separately.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Admitted radix indices and finite-checked LGL trace arithmetic"
)]
use crate::{
	CfdError,
	configuration::ConfigurationGrid,
	kvn_recipe::{KvnHistoryRecipe, KvnRecipeLimits},
	polynomial::PolynomialOde,
};
use quest_numerics::Complex64;

/// Explicit diagnostic ceilings.
#[derive(Clone, Copy, Debug)]
pub struct ConfigurationFluxLimits {
	/// Complete borrowed-input, result and conservative query peak.
	pub max_bytes: usize,
	/// Aggregate modeled validation, traversal and prepared-kernel query work.
	pub max_work: usize,
	/// Exact number of exterior tensor nodes, including zero-amplitude nodes.
	pub max_drift_calls: usize,
}
impl Default for ConfigurationFluxLimits {
	fn default() -> Self {
		Self {
			max_bytes: 268_435_456,
			max_work: 1_000_000_000,
			max_drift_calls: 1_000_000,
		}
	}
}
/// One exterior face.
#[derive(Clone, Copy, Debug, Default)]
pub struct FaceFlux {
	/// Normalized density integrated with the tangential face quadrature.
	/// Units inverse coordinate length; this is not a probability mass.
	pub normalized_trace: f64,
	/// Positive outward rate (inverse time).
	pub outward_rate: f64,
	/// Magnitude of the negative outward rate (inverse time).
	pub inward_rate: f64,
	pub net_rate: f64,
}
/// Lower and upper exterior faces of one retained coordinate.
#[derive(Clone, Copy, Debug, Default)]
pub struct AxisFlux {
	pub lower: FaceFlux,
	pub upper: FaceFlux,
	pub outward_rate: f64,
	pub inward_rate: f64,
	pub net_rate: f64,
}
/// Diagnostic resource receipt.
#[derive(Clone, Copy, Debug, Default)]
pub struct ConfigurationFluxResources {
	pub drift_calls: usize,
	pub face_samples: usize,
	pub constructor_validation_work: usize,
	pub traversal_work: usize,
	/// A whole conservative `KvN` row allowance per actual boundary drift call.
	pub query_work: usize,
	pub total_work: usize,
	/// Actual grid capacity plus inherited conservative complete ODE allowance
	/// and borrowed `KvN` recipe descriptor.
	pub retained_input_bytes: usize,
	/// Accessible slice payload only; inaccessible backing allocation is excluded.
	pub accessible_state_bytes: usize,
	/// Result descriptor and actual axis-vector capacity.
	pub result_bytes: usize,
	/// Actual point capacity plus inherited conservative `KvN` query scratch.
	pub scratch_bytes: usize,
	pub peak_bytes: usize,
}
/// Scope of the returned numerical result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigurationFluxStatus {
	NumericalInstantaneous,
}
/// Instantaneous numerical diagnostic.
#[derive(Debug)]
pub struct ConfigurationFluxDiagnostic {
	pub status: ConfigurationFluxStatus,
	pub time: f64,
	/// Unnormalized mass-weighted state norm squared.
	pub probability: f64,
	/// Normalized occupation of outer cells, not exterior trace probability.
	pub outer_cell_occupation: f64,
	/// Complete coordinate order, axis zero fastest in the tensor radix.
	pub axes: Vec<AxisFlux>,
	pub outward_rate: f64,
	pub inward_rate: f64,
	pub net_rate: f64,
	pub resources: ConfigurationFluxResources,
}
const fn overflow() -> CfdError {
	CfdError::InvalidInput("configuration flux resource overflow")
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(overflow)
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(overflow)
}
const fn finite(value: f64) -> Result<f64, CfdError> {
	if value.is_finite() {
		Ok(value)
	} else {
		Err(CfdError::InvalidInput(
			"nonfinite configuration flux arithmetic",
		))
	}
}
fn peak(retained: usize, state: usize, result: usize, scratch: usize) -> Result<usize, CfdError> {
	add(add(retained, state)?, add(result, scratch)?)
}
fn norm(state: &[Complex64]) -> Result<f64, CfdError> {
	let mut probability = 0.;
	for amplitude in state {
		finite(amplitude.re)?;
		finite(amplitude.im)?;
		probability = finite(probability + finite(amplitude.norm_sqr())?)?;
	}
	if probability <= 0. {
		return Err(CfdError::InvalidInput("configuration flux zero norm"));
	}
	Ok(probability)
}
fn admit(
	grid: &ConfigurationGrid,
	ode: &PolynomialOde,
	state_length: usize,
	limits: ConfigurationFluxLimits,
) -> Result<(ConfigurationFluxResources, usize), CfdError> {
	let m = grid.axes();
	let n = grid.axis_dimension();
	let dimension = grid.dimension();
	let exponent = u32::try_from(m).map_err(|_| overflow())?;
	let interior = n
		.checked_sub(2)
		.and_then(|v| v.checked_pow(exponent))
		.ok_or_else(overflow)?;
	let drift_calls = dimension.checked_sub(interior).ok_or_else(overflow)?;
	let face_samples = mul(mul(2, m)?, dimension / n)?;
	if drift_calls > limits.max_drift_calls {
		return Err(CfdError::InvalidInput(
			"configuration flux drift-call budget",
		));
	}
	// Immutable uniform P1/P2 grids have at most five entries per axis row.
	// The cached complete ODE allowance covers all exponent/coefficient storage.
	// Charge their validation scans before retained_bytes()/KvN constructor scans.
	let constructor_validation_work = add(
		add(mul(128, ode.retained_bytes())?, mul(1024, n)?)?,
		add(mul(128, m)?, 4096)?,
	)?;
	let traversal_work = add(
		mul(dimension, mul(256, add(m, 1)?)?)?,
		mul(64, face_samples)?,
	)?;
	let pre_query_work = add(constructor_validation_work, traversal_work)?;
	if pre_query_work > limits.max_work {
		return Err(CfdError::InvalidInput(
			"configuration flux validation/traversal work budget",
		));
	}
	let recipe = KvnHistoryRecipe::new(
		grid,
		ode,
		KvnRecipeLimits {
			max_dimension: dimension,
			max_bytes: limits.max_bytes,
			max_query_work: limits.max_work,
		},
	)?;
	let kr = recipe.resources();
	let query_work = mul(drift_calls, kr.row_query_work)?;
	let total_work = add(pre_query_work, query_work)?;
	if total_work > limits.max_work {
		return Err(CfdError::InvalidInput(
			"configuration flux aggregate work budget",
		));
	}
	let accessible_state_bytes = mul(state_length, size_of::<Complex64>())?;
	let planned_result = add(
		size_of::<ConfigurationFluxDiagnostic>(),
		mul(m, size_of::<AxisFlux>())?,
	)?;
	let planned_scratch = add(kr.row_query_bytes, mul(m, size_of::<f64>())?)?;
	if peak(
		kr.retained_bytes,
		accessible_state_bytes,
		planned_result,
		planned_scratch,
	)? > limits.max_bytes
	{
		return Err(CfdError::InvalidInput(
			"configuration flux peak storage budget",
		));
	}
	Ok((
		ConfigurationFluxResources {
			drift_calls,
			face_samples,
			constructor_validation_work,
			traversal_work,
			query_work,
			total_work,
			retained_input_bytes: kr.retained_bytes,
			accessible_state_bytes,
			result_bytes: planned_result,
			scratch_bytes: planned_scratch,
			peak_bytes: peak(
				kr.retained_bytes,
				accessible_state_bytes,
				planned_result,
				planned_scratch,
			)?,
		},
		kr.row_query_bytes,
	))
}
fn accumulate_faces(
	grid: &ConfigurationGrid,
	index: usize,
	normalized_mass: f64,
	drift: &[f64],
	axes: &mut [AxisFlux],
) -> Result<usize, CfdError> {
	let n = grid.axis_dimension();
	let mut samples = 0;
	let mut radix = index;
	for (axis, receipt) in axes.iter_mut().enumerate() {
		let digit = radix % n;
		radix /= n;
		let (face, speed) = if digit == 0 {
			(&mut receipt.lower, -drift[axis])
		} else if digit == n - 1 {
			(&mut receipt.upper, drift[axis])
		} else {
			continue;
		};
		let weight = grid
			.axis_weight(digit)
			.ok_or(CfdError::Assembly("configuration flux face weight"))?;
		let trace = finite(normalized_mass / weight)?;
		let rate = finite(trace * speed)?;
		face.normalized_trace = finite(face.normalized_trace + trace)?;
		face.outward_rate = finite(face.outward_rate + rate.max(0.))?;
		face.inward_rate = finite(face.inward_rate + (-rate).max(0.))?;
		samples = add(samples, 1)?;
	}
	Ok(samples)
}
/// Diagnose exterior traces without changing evolution or dropping coordinates.
///
/// For a tensor node on a face normal to coordinate `j`, the normalized face
/// quadrature contribution is `(|z_i|² / ||z||²) / w_j`. Only axis indices zero
/// and last are exterior. Internal duplicate DG facets are excluded. Corners
/// contribute to each incident face, with a single full drift evaluation.
///
/// Admission precedes allocation and numerical drift. Prepared-input validation
/// is charged before its scans, then the existing `KvnHistoryRecipe` resource
/// engine is reused conservatively: each boundary drift is charged an entire
/// row-query allowance, although no generator row is evaluated. The state slice
/// charges all accessible entries; its owner's inaccessible spare capacity is
/// outside this receipt. Preparation of the already prepared ODE/grid is separate.
/// # Errors
/// Rejects wrong complete shapes, nonfinite data/time/arithmetic, zero norm,
/// allocation failure, checked-resource overflow and any exceeded fixed ceiling.
pub fn boundary_flux(
	grid: &ConfigurationGrid,
	ode: &PolynomialOde,
	time: f64,
	state: &[Complex64],
	limits: ConfigurationFluxLimits,
) -> Result<ConfigurationFluxDiagnostic, CfdError> {
	let m = grid.axes();
	let n = grid.axis_dimension();
	let dimension = grid.dimension();
	if !time.is_finite() || ode.dimension() != m || state.len() != dimension || n < 2 {
		return Err(CfdError::InvalidInput(
			"configuration flux complete shape/time",
		));
	}
	let (mut resources, query_scratch) = admit(grid, ode, state.len(), limits)?;
	let mut axes = Vec::new();
	axes.try_reserve_exact(m)
		.map_err(|_| CfdError::InvalidInput("configuration flux result allocation"))?;
	axes.resize(m, AxisFlux::default());
	let mut point = Vec::new();
	point
		.try_reserve_exact(m)
		.map_err(|_| CfdError::InvalidInput("configuration flux point allocation"))?;
	point.resize(m, 0.);
	resources.result_bytes = add(
		size_of::<ConfigurationFluxDiagnostic>(),
		mul(axes.capacity(), size_of::<AxisFlux>())?,
	)?;
	resources.scratch_bytes = add(query_scratch, mul(point.capacity(), size_of::<f64>())?)?;
	resources.peak_bytes = peak(
		resources.retained_input_bytes,
		resources.accessible_state_bytes,
		resources.result_bytes,
		resources.scratch_bytes,
	)?;
	if resources.peak_bytes > limits.max_bytes {
		return Err(CfdError::InvalidInput(
			"configuration flux actual-capacity storage budget",
		));
	}
	let probability = norm(state)?;
	let outer_cell_occupation = finite(grid.boundary_mass(state)? / probability)?;
	let mut calls = 0usize;
	let mut samples = 0usize;
	for (index, amplitude) in state.iter().enumerate() {
		let mut radix = index;
		let mut boundary = false;
		for coordinate in &mut point {
			let digit = radix % n;
			radix /= n;
			boundary |= digit == 0 || digit == n - 1;
			*coordinate = grid
				.axis_node(digit)
				.ok_or(CfdError::Assembly("configuration flux axis point"))?;
		}
		if !boundary {
			continue;
		}
		let drift = ode.drift(time, &point)?;
		calls = add(calls, 1)?;
		if drift.len() != m || drift.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput(
				"configuration flux full drift result",
			));
		}
		// Returned drift is one part of the inherited query scratch envelope.
		if add(mul(drift.capacity(), size_of::<f64>())?, 128)? > query_scratch {
			return Err(CfdError::InvalidInput(
				"configuration flux actual drift capacity",
			));
		}
		let normalized_mass = finite(amplitude.norm_sqr() / probability)?;
		samples = add(
			samples,
			accumulate_faces(grid, index, normalized_mass, &drift, &mut axes)?,
		)?;
	}
	if calls != resources.drift_calls || samples != resources.face_samples {
		return Err(CfdError::Assembly("configuration flux traversal counts"));
	}
	let mut outward_rate = 0.;
	let mut inward_rate = 0.;
	for axis in &mut axes {
		axis.lower.net_rate = finite(axis.lower.outward_rate - axis.lower.inward_rate)?;
		axis.upper.net_rate = finite(axis.upper.outward_rate - axis.upper.inward_rate)?;
		axis.outward_rate = finite(axis.lower.outward_rate + axis.upper.outward_rate)?;
		axis.inward_rate = finite(axis.lower.inward_rate + axis.upper.inward_rate)?;
		axis.net_rate = finite(axis.outward_rate - axis.inward_rate)?;
		outward_rate = finite(outward_rate + axis.outward_rate)?;
		inward_rate = finite(inward_rate + axis.inward_rate)?;
	}
	Ok(ConfigurationFluxDiagnostic {
		status: ConfigurationFluxStatus::NumericalInstantaneous,
		time,
		probability,
		outer_cell_occupation,
		axes,
		outward_rate,
		inward_rate,
		net_rate: finite(outward_rate - inward_rate)?,
		resources,
	})
}
