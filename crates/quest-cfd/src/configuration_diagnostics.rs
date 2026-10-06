//! Bounded classical resolution diagnostics, separate from a convergence certificate.
//!
//! Periodic closure in configuration coordinates has no absorbing boundary. Outer
//! cell probability measures occupation; it is neither outward flux nor a leakage bound.
#![allow(
	clippy::arithmetic_side_effects,
	reason = "Finite admitted grid and moment arithmetic is checked before publishing diagnostics"
)]
use crate::{CfdError, configuration::ConfigurationGrid};
use quest_numerics::Complex64;

/// Initial support sampling under an explicitly selected minimum-node policy.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RegularizationResolution {
	pub distinct_samples_in_support: Vec<usize>,
	pub minimum_required_samples: usize,
	pub maximum_distinct_node_spacing: f64,
	pub width_per_spacing: f64,
	pub support_intersects_boundary: bool,
	pub convergence_certified: bool,
}
#[allow(
	clippy::float_cmp,
	reason = "Co-located DG endpoint samples use the identical uniform-grid coordinate calculation; signed zeros are also the same location"
)]
fn distinct_nodes(grid: &ConfigurationGrid) -> Result<Vec<f64>, CfdError> {
	let mut nodes = Vec::new();
	nodes
		.try_reserve_exact(grid.axis_dimension())
		.map_err(|_| CfdError::InvalidInput("configuration diagnostic allocation"))?;
	for i in 0..grid.axis_dimension() {
		let point = grid
			.point(i)
			.ok_or(CfdError::InvalidInput("configuration diagnostic index"))?;
		let value = *point
			.first()
			.ok_or(CfdError::InvalidInput("empty configuration point"))?;
		if nodes.last().is_none_or(|last| *last != value) {
			nodes.push(value);
		}
	}
	Ok(nodes)
}
fn spacing(nodes: &[f64]) -> Result<f64, CfdError> {
	let value = nodes
		.windows(2)
		.map(|p| p.last().copied().unwrap_or(0.) - p.first().copied().unwrap_or(0.))
		.fold(0., f64::max);
	if value.is_finite() && value > 0. {
		Ok(value)
	} else {
		Err(CfdError::InvalidInput(
			"configuration spacing is not representable",
		))
	}
}
/// Check distinct physical sample locations inside every factor of the compact bump.
///
/// Duplicate DG facet nodes do not provide extra spatial resolution. Passing this
/// necessary sampling policy does not bound quadrature or regularization errors.
/// # Errors
/// Rejects invalid parameters and any axis with fewer support nodes than requested.
pub fn regularization_resolution(
	grid: &ConfigurationGrid,
	center: &[f64],
	width: f64,
	minimum_samples: usize,
) -> Result<RegularizationResolution, CfdError> {
	if center.len() != grid.axes()
		|| center.iter().any(|v| !v.is_finite())
		|| !width.is_finite()
		|| width <= 0.
		|| minimum_samples == 0
	{
		return Err(CfdError::InvalidInput(
			"invalid regularization sampling policy",
		));
	}
	let nodes = distinct_nodes(grid)?;
	let spacing = spacing(&nodes)?;
	let counts: Vec<_> = center
		.iter()
		.map(|c| {
			nodes
				.iter()
				.filter(|x| ((*x - c) / width).abs() < 1.)
				.count()
		})
		.collect();
	if counts.iter().any(|&n| n < minimum_samples) {
		return Err(CfdError::InvalidInput(
			"regularization fails distinct-support sampling policy",
		));
	}
	let (lower, upper) = grid.bounds();
	let ratio = width / spacing;
	if !ratio.is_finite() {
		return Err(CfdError::InvalidInput(
			"regularization resolution ratio overflow",
		));
	}
	Ok(RegularizationResolution {
		distinct_samples_in_support: counts,
		minimum_required_samples: minimum_samples,
		maximum_distinct_node_spacing: spacing,
		width_per_spacing: ratio,
		support_intersects_boundary: center.iter().any(|c| {
			(lower - c).abs() < width || (upper - c).abs() < width || *c < lower || *c > upper
		}),
		convergence_certified: false,
	})
}

/// Moment and occupation diagnostics for the complete mass-weighted state.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ConcentrationDiagnostic {
	pub probability: f64,
	pub boundary_occupation_fraction: f64,
	pub standard_deviations_per_spacing: Vec<f64>,
	pub maximum_nodal_probability: f64,
	pub effective_nodal_coefficients: f64,
	pub minimum_required_standard_deviations_per_spacing: Option<f64>,
	pub convergence_certified: bool,
}
/// Diagnose concentration and optionally reject widths below an explicit grid policy.
///
/// Standard deviations use ordinary domain coordinates; a distribution wrapping
/// around the periodic configuration boundary can have misleadingly large variance.
/// Always retain boundary occupation and independently refine domain and resolution.
/// Effective nodal count includes distinct DG coefficients at coincident facets.
/// # Errors
/// Rejects malformed states, nonfinite moments, or a failed positive concentration policy.
pub fn concentration(
	grid: &ConfigurationGrid,
	state: &[Complex64],
	minimum_std_per_spacing: Option<f64>,
) -> Result<ConcentrationDiagnostic, CfdError> {
	if minimum_std_per_spacing.is_some_and(|v| !v.is_finite() || v <= 0.) {
		return Err(CfdError::InvalidInput("invalid concentration policy"));
	}
	let moments = grid.observables(state, |_| Ok(0.))?;
	let spacing = spacing(&distinct_nodes(grid)?)?;
	let widths: Vec<_> = moments
		.coordinate_variances
		.iter()
		.map(|v| v.sqrt() / spacing)
		.collect();
	let mut maximum = 0_f64;
	let mut squared = 0.;
	for z in state {
		let p = z.norm_sqr() / moments.probability;
		maximum = maximum.max(p);
		squared = p.mul_add(p, squared);
	}
	let effective = 1. / squared;
	let occupation = moments.boundary_probability / moments.probability;
	if !effective.is_finite() || !occupation.is_finite() || widths.iter().any(|w| !w.is_finite()) {
		return Err(CfdError::InvalidInput("concentration diagnostic overflow"));
	}
	if minimum_std_per_spacing.is_some_and(|minimum| widths.iter().any(|w| *w < minimum)) {
		return Err(CfdError::InvalidInput(
			"state fails concentration sampling policy",
		));
	}
	Ok(ConcentrationDiagnostic {
		probability: moments.probability,
		boundary_occupation_fraction: occupation,
		standard_deviations_per_spacing: widths,
		maximum_nodal_probability: maximum,
		effective_nodal_coefficients: effective,
		minimum_required_standard_deviations_per_spacing: minimum_std_per_spacing,
		convergence_certified: false,
	})
}
