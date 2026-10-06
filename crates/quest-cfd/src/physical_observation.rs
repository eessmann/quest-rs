//! Bounded complete-coordinate physical observables lowered through `MathCore`.
//!
//! Numerical polarization captures the known affine/quadratic reference formulas.
//! Independent validation states test capture accuracy; they are not a uniform
//! physical-error certificate. No physical coordinate or configuration mode is removed.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Checked resource envelopes bound complete-coordinate polynomial extraction and fixed-degree evaluation"
)]
use crate::{
	CfdError,
	configuration::ConfigurationGrid,
	kvn_recipe::KvnRecipeLimits,
	physical_space::{BoxBoundarySide, PhysicalSpace, PolynomialBoundary},
	probability_observation::DiagonalRange,
	simplex::SimplexBdm,
};
use mathcore::{
	arithmetic::ExactConstant,
	exact::{Owner, Symbol},
	multivariate::{PolynomialKernel, PolynomialLimits, SparsePolynomial, rational_constant},
};
use quest_numerics::{
	Interval,
	arithmetic::{F64Backend, Interval64Backend},
};
use std::cell::RefCell;

/// Reference semantics are inherited without force normalization or volume division.
#[derive(Clone, Debug)]
pub enum PhysicalObservableKind {
	/// Integral of one half the squared curl, including the complete affine lifting.
	Enstrophy,
	/// First-containing-cell DG trace at the specified physical point.
	VelocityComponent { point: [f64; 3], component: usize },
	/// Reconstructed P0/P1 pressure difference; interface probes average incident traces.
	PressureDifference { first: [f64; 3], second: [f64; 3] },
	/// Mechanical integral of `p n - nu grad(u) n`; simplex labels or canonical box-side names.
	BoundaryForceComponent { label: String, component: usize },
}
impl PhysicalObservableKind {
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		add(
			size_of::<Self>(),
			match self {
				Self::BoundaryForceComponent { label, .. } => label.capacity(),
				_ => 0,
			},
		)
	}
	const fn affine(&self) -> bool {
		matches!(self, Self::VelocityComponent { .. })
	}
	const fn pressure(&self) -> bool {
		matches!(
			self,
			Self::PressureDifference { .. } | Self::BoundaryForceComponent { .. }
		)
	}
	fn validate(&self, d: usize) -> Result<(), CfdError> {
		let point = |p: &[f64; 3]| p.iter().all(|v| v.is_finite()) && (d == 3 || p[2] == 0.);
		let valid = match self {
			Self::Enstrophy => true,
			Self::VelocityComponent {
				point: p,
				component,
			} => *component < d && point(p),
			Self::PressureDifference { first, second } => point(first) && point(second),
			Self::BoundaryForceComponent { label, component } => {
				*component < d && !label.is_empty()
			}
		};
		if valid { Ok(()) } else { Err(invalid()) }
	}
}
/// Preparation envelopes include capture, shared algebra lowering and numerical
/// validation, overlapping the complete already-built physical model's payload.
#[derive(Clone, Copy, Debug)]
pub struct PhysicalObservableLimits {
	pub max_coordinates: usize,
	pub max_reference_evaluations: usize,
	pub max_bytes: usize,
	pub max_prepare_bytes: usize,
	pub max_prepare_work: usize,
	pub max_query_work: usize,
	pub validation_tolerance: f64,
}
impl Default for PhysicalObservableLimits {
	fn default() -> Self {
		Self {
			max_coordinates: 256,
			max_reference_evaluations: 200_000,
			max_bytes: 134_217_728,
			max_prepare_bytes: 536_870_912,
			max_prepare_work: 64_000_000_000,
			max_query_work: 100_000_000,
			validation_tolerance: 1e-9,
		}
	}
}
/// Conservative additional storage/work, excluding the borrowed physical model.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct PhysicalObservableResources {
	pub retained_bytes: usize,
	pub prepare_peak_bytes: usize,
	pub prepare_work: usize,
	pub reference_evaluations: usize,
	pub query_work: usize,
	pub query_scratch_bytes: usize,
	pub terms: usize,
	/// Largest sampled `|capture-reference|/(1+|reference|)` on unused test states.
	pub validation_scaled_defect: f64,
	/// Actual borrowed source payload additionally admitted during preparation.
	pub borrowed_source_bytes: usize,
}

/// Numerical source category; this metadata is not a proof of chart/source identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum PhysicalObservableSource {
	SimplexBdm1,
	BoxSpace,
	PolynomialBoundary,
}
/// Gauge convention of the physical recovery used during capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum PhysicalObservableGauge {
	VolumeWeightedZeroMean,
	InheritedSimplexReference,
}
/// Snapshot semantics. Every chart coordinate remains declared, including zero terms.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct PhysicalObservableProvenance {
	pub source: PhysicalObservableSource,
	pub physical_dimension: usize,
	pub velocity_order: usize,
	pub pressure_degree: usize,
	/// Explicit fixed physical time for a polynomial boundary; autonomous snapshots use None.
	pub snapshot_time: Option<f64>,
	pub gauge: PhysicalObservableGauge,
	/// All incident cell traces are averaged arithmetically; no pressure smoothing.
	pub pressure_trace: &'static str,
	/// Interior mechanical traction; no SIP/convective flux or force normalization.
	pub mechanical_traction: &'static str,
}
const PRESSURE_TRACE: &str = "incident-cell arithmetic mean";
const MECHANICAL_TRACTION: &str = "p n - nu grad(u) n; outward fluid normal; fluid-on-boundary";
#[derive(Clone, Copy)]
struct CaptureSource {
	provenance: PhysicalObservableProvenance,
	bytes: usize,
	query_work: usize,
	query_bytes: usize,
}
impl CaptureSource {
	const fn simplex(dimension: usize) -> Self {
		Self {
			provenance: PhysicalObservableProvenance {
				source: PhysicalObservableSource::SimplexBdm1,
				physical_dimension: dimension,
				velocity_order: 1,
				pressure_degree: 0,
				snapshot_time: None,
				gauge: PhysicalObservableGauge::InheritedSimplexReference,
				pressure_trace: PRESSURE_TRACE,
				mechanical_traction: MECHANICAL_TRACTION,
			},
			bytes: 0,
			query_work: 0,
			query_bytes: 0,
		}
	}
}
/// Complete affine/quadratic numerical reference, with exact dyadic captured
/// coefficients and shared floating/interval prepared kernels.
///
/// The physical model
/// is not retained. Query storage and work scale with every declared coordinate.
pub struct PreparedPhysicalObservable {
	kind: PhysicalObservableKind,
	provenance: PhysicalObservableProvenance,
	kernel: PolynomialKernel<f64>,
	enclosure: PolynomialKernel<Interval>,
	resources: PhysicalObservableResources,
}
const fn invalid() -> CfdError {
	CfdError::InvalidInput("physical observable domain/resource admission")
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(invalid)
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(invalid)
}
fn zeros<T: Clone>(n: usize, value: T) -> Result<Vec<T>, CfdError> {
	let mut v = Vec::new();
	v.try_reserve_exact(n).map_err(|_| invalid())?;
	v.resize(n, value);
	Ok(v)
}
impl PreparedPhysicalObservable {
	/// Capture a supported p1/p2 full box-space observable, without model reduction.
	/// # Errors
	/// Rejects invalid pressure/traction probes or canonical box-side labels,
	/// reference failures, allocation/budget exhaustion and failed capture validation.
	pub fn box_space(
		model: &PhysicalSpace,
		kind: PhysicalObservableKind,
		limits: PhysicalObservableLimits,
	) -> Result<Self, CfdError> {
		kind.validate(model.physical_dimension())?;
		Self::validate_box_side(model, &kind)?;
		Self::capture(
			model.dimension(),
			model.diagnostics().local_velocity_dimension,
			kind,
			limits,
			CaptureSource {
				provenance: PhysicalObservableProvenance {
					source: PhysicalObservableSource::BoxSpace,
					physical_dimension: model.physical_dimension(),
					velocity_order: model.order(),
					pressure_degree: model.order() - 1,
					snapshot_time: None,
					gauge: PhysicalObservableGauge::VolumeWeightedZeroMean,
					pressure_trace: PRESSURE_TRACE,
					mechanical_traction: MECHANICAL_TRACTION,
				},
				bytes: model.retained_bytes()?,
				query_work: 0,
				query_bytes: 0,
			},
			|kind, state| match kind {
				PhysicalObservableKind::Enstrophy => model.enstrophy(state),
				PhysicalObservableKind::VelocityComponent { point, component } => {
					Ok(model.sample_velocity(state, *point)?[*component])
				}
				PhysicalObservableKind::PressureDifference { first, second } => {
					let pressure = model.reconstruct_pressure(state)?;
					let values = model
						.sample_pressures(&pressure.pressure_coefficients, &[*first, *second])?;
					Ok(values[0] - values[1])
				}
				PhysicalObservableKind::BoundaryForceComponent { label, component } => {
					let pressure = model.reconstruct_pressure(state)?;
					Ok(model.boundary_force(
						state,
						&pressure.pressure_coefficients,
						BoxBoundarySide::from_label(label)?,
					)?[*component])
				}
			},
		)
	}
	/// Capture the existing full BDM1 simplex reference, including supported pressure
	/// recovery and boundary force. Its gauge, P0 trace and unsymmetrized viscous force
	/// convention are preserved. This is a bounded numerical reference snapshot.
	/// # Errors
	/// Rejects invalid/missing probes or labels, resource exhaustion, reference errors
	/// and sampled capture discrepancies above the declared tolerance.
	pub fn simplex(
		model: &SimplexBdm,
		kind: PhysicalObservableKind,
		limits: PhysicalObservableLimits,
	) -> Result<Self, CfdError> {
		kind.validate(model.physical_dimension())?;
		let mut source = CaptureSource::simplex(model.physical_dimension());
		source.bytes = model.retained_bytes()?;
		Self::capture(
			model.dimension(),
			model.diagnostics().local_velocity_dimension,
			kind,
			limits,
			source,
			|kind, state| match kind {
				PhysicalObservableKind::Enstrophy => model.enstrophy(state),
				PhysicalObservableKind::VelocityComponent { point, component } => {
					Ok(model.sample_velocity(state, *point)?[*component])
				}
				PhysicalObservableKind::PressureDifference { first, second } => {
					let pressure = model.reconstruct_pressure(state)?;
					let values =
						model.sample_pressures(&pressure.cell_pressure, &[*first, *second])?;
					Ok(values[0] - values[1])
				}
				PhysicalObservableKind::BoundaryForceComponent { label, component } => {
					let pressure = model.reconstruct_pressure(state)?;
					Ok(model.boundary_force(state, &pressure.cell_pressure, label)?[*component])
				}
			},
		)
	}
	/// Capture complete pressure or mechanical traction at an explicit physical time.
	/// Original pressure recovery retains `ell_dot`; traction uses the full lifted velocity.
	/// Time is a fixed parameter, not an added/reduced physical coordinate. Recorded
	/// dyadics and validation probes do not certify uniform physical capture error.
	/// # Errors
	/// Rejects other kinds, nonfinite time, invalid data, source/query/capture budgets,
	/// original momentum/gauge failures and discrepancies above the declared tolerance.
	pub fn polynomial_boundary(
		problem: &PolynomialBoundary<'_>,
		time: f64,
		kind: PhysicalObservableKind,
		limits: PhysicalObservableLimits,
	) -> Result<Self, CfdError> {
		let space = problem.physical_space();
		kind.validate(space.physical_dimension())?;
		Self::validate_box_side(space, &kind)?;
		if !kind.pressure() || !time.is_finite() {
			return Err(invalid());
		}
		let r = problem.resources();
		Self::capture(
			problem.dimension(),
			space.diagnostics().local_velocity_dimension,
			kind,
			limits,
			CaptureSource {
				provenance: PhysicalObservableProvenance {
					source: PhysicalObservableSource::PolynomialBoundary,
					physical_dimension: space.physical_dimension(),
					velocity_order: space.order(),
					pressure_degree: space.order() - 1,
					snapshot_time: Some(time),
					gauge: PhysicalObservableGauge::VolumeWeightedZeroMean,
					pressure_trace: PRESSURE_TRACE,
					mechanical_traction: MECHANICAL_TRACTION,
				},
				bytes: add(r.borrowed_space_bytes, r.retained_bytes)?,
				query_work: add(r.pressure_work, r.drift_work)?,
				query_bytes: r.peak_bytes,
			},
			|kind, state| {
				let pressure = problem.reconstruct_pressure(time, state)?;
				match kind {
					PhysicalObservableKind::PressureDifference { first, second } => {
						let p = space.sample_pressures(
							&pressure.pressure_coefficients,
							&[*first, *second],
						)?;
						Ok(p[0] - p[1])
					}
					PhysicalObservableKind::BoundaryForceComponent { label, component } => {
						Ok(problem.boundary_force(
							time,
							state,
							&pressure.pressure_coefficients,
							BoxBoundarySide::from_label(label)?,
						)?[*component])
					}
					_ => Err(invalid()),
				}
			},
		)
	}
	fn validate_box_side(
		model: &PhysicalSpace,
		kind: &PhysicalObservableKind,
	) -> Result<(), CfdError> {
		if let PhysicalObservableKind::BoundaryForceComponent { label, .. } = kind
			&& (BoxBoundarySide::from_label(label)?.axis() >= model.physical_dimension()
				|| model.boundary() == crate::simplex::BoxBoundary::Periodic)
		{
			return Err(invalid());
		}
		Ok(())
	}
	#[allow(
		clippy::too_many_lines,
		reason = "Capture admission, polarization, lowering and independent validation share one auditable lifetime"
	)]
	fn capture(
		m: usize,
		broken: usize,
		kind: PhysicalObservableKind,
		limits: PhysicalObservableLimits,
		source: CaptureSource,
		mut evaluate: impl FnMut(&PhysicalObservableKind, &[f64]) -> Result<f64, CfdError>,
	) -> Result<Self, CfdError> {
		if m == 0
			|| m > limits.max_coordinates
			|| u32::try_from(m).is_err()
			|| !limits.validation_tolerance.is_finite()
			|| limits.validation_tolerance <= 0.
		{
			return Err(invalid());
		}
		let quadratic = !kind.affine();
		let pairs = mul(m, m.saturating_sub(1))? / 2;
		let terms = add(add(1, m)?, if quadratic { add(m, pairs)? } else { 0 })?;
		let calls = add(
			add(8, mul(2, m)?)?,
			if quadratic { mul(4, pairs)? } else { 0 },
		)?;
		let reference_work = if kind.pressure() {
			add(mul(256, mul(mul(broken, broken)?, broken)?)?, 8192)?
		} else {
			add(mul(256, mul(broken, broken)?)?, 8192)?
		}
		.max(source.query_work);
		let reference_bytes = if kind.pressure() {
			add(mul(256, mul(broken, broken)?)?, 8192)?
		} else {
			add(mul(256, broken)?, 8192)?
		}
		.max(source.query_bytes);
		// Binary64 dyadics need at most 1075 coefficient bits; 4096 bytes per term
		// plus 128 per exponent cover overlapping exact trees and both lowered kernels.
		let algebra_bytes = mul(terms, add(mul(128, m)?, 4096)?)?;
		let prepare_peak_bytes = add(
			add(add(algebra_bytes, reference_bytes)?, mul(256, m)?)?,
			add(kind.retained_bytes()?, source.bytes)?,
		)?;
		let algebra_work = mul(terms, add(mul(1024, m)?, 8192)?)?;
		let prepare_work = add(mul(calls, reference_work)?, algebra_work)?;
		let query_work = add(mul(terms, add(mul(16, m)?, 128)?)?, mul(16, m)?)?;
		let query_scratch_bytes = add(mul(32, m)?, 512)?;
		let retained_upper_bound = add(
			add(size_of::<Self>(), kind.retained_bytes()?)?,
			algebra_bytes,
		)?;
		if calls > limits.max_reference_evaluations
			|| prepare_peak_bytes > limits.max_prepare_bytes
			|| prepare_work > limits.max_prepare_work
			|| query_work > limits.max_query_work
			|| add(retained_upper_bound, query_scratch_bytes)? > limits.max_bytes
		{
			return Err(invalid());
		}
		let pl = PolynomialLimits {
			max_variables: m,
			max_terms: terms,
			max_degree: 2,
			max_coefficient_bits: 2048,
			max_bytes: limits.max_prepare_bytes,
			max_work: limits.max_prepare_work,
		};
		let mut state = zeros(m, 0.)?;
		if mul(state.capacity(), 8)? > mul(16, m)? {
			return Err(invalid());
		}
		let mut sample = |state: &[f64]| -> Result<f64, CfdError> {
			let x = evaluate(&kind, state)?;
			if x.is_finite() { Ok(x) } else { Err(invalid()) }
		};
		let zero = sample(&state)?;
		let mut entries = Vec::new();
		entries.try_reserve_exact(terms).map_err(|_| invalid())?;
		if mul(entries.capacity(), size_of::<(Vec<u32>, mathcore::RBig)>())? > algebra_bytes / 4 {
			return Err(invalid());
		}
		let mut exponent_bytes = 0usize;
		let mut push = |i: Option<usize>, j: Option<usize>, value: f64| -> Result<(), CfdError> {
			if !value.is_finite() {
				return Err(invalid());
			}
			if value != 0. {
				let mut p = zeros(m, 0_u32)?;
				exponent_bytes = add(exponent_bytes, mul(p.capacity(), size_of::<u32>())?)?;
				if exponent_bytes > algebra_bytes / 4 {
					return Err(invalid());
				}
				if let Some(i) = i {
					p[i] += 1;
				}
				if let Some(j) = j {
					p[j] += 1;
				}
				entries.push((p, rational_constant(&ExactConstant::Binary64(value), pl)?));
			}
			Ok(())
		};
		push(None, None, zero)?;
		for i in 0..m {
			state[i] = 1.;
			let positive = sample(&state)?;
			state[i] = -1.;
			let negative = sample(&state)?;
			state[i] = 0.;
			push(Some(i), None, 0.5 * positive - 0.5 * negative)?;
			if quadratic {
				push(Some(i), Some(i), 0.5 * positive + 0.5 * negative - zero)?;
			}
		}
		if quadratic {
			for i in 0..m {
				for j in 0..i {
					state[i] = 1.;
					state[j] = 1.;
					let pp = sample(&state)?;
					state[j] = -1.;
					let pm = sample(&state)?;
					state[i] = -1.;
					let mm = sample(&state)?;
					state[j] = 1.;
					let mp = sample(&state)?;
					state[i] = 0.;
					state[j] = 0.;
					push(
						Some(i),
						Some(j),
						0.25 * pp - 0.25 * pm + 0.25 * mm - 0.25 * mp,
					)?;
				}
			}
		}
		let mut symbols = Vec::new();
		symbols.try_reserve_exact(m).map_err(|_| invalid())?;
		if mul(symbols.capacity(), size_of::<Symbol>())? > mul(128, m)? {
			return Err(invalid());
		}
		for i in 0..m {
			symbols.push(Symbol::new(
				Owner::new(0x4346_444f_4253_4552),
				u64::try_from(i).map_err(|_| invalid())?,
			));
		}
		let polynomial = SparsePolynomial::from_terms(symbols, entries, pl)?;
		let kernel = polynomial.lower(&mut F64Backend)?;
		let enclosure = polynomial.lower(&mut Interval64Backend)?;
		let actual_algebra = add(
			add(polynomial.retained_bytes()?, kernel.retained_bytes()?)?,
			enclosure.retained_bytes()?,
		)?;
		if actual_algebra > algebra_bytes {
			return Err(invalid());
		}
		let retained_bytes = add(
			add(
				add(size_of::<Self>(), kind.retained_bytes()?)?,
				kernel.retained_bytes()?,
			)?,
			enclosure.retained_bytes()?,
		)?;
		if add(retained_bytes, query_scratch_bytes)? > limits.max_bytes {
			return Err(invalid());
		}
		let mut defect = 0_f64;
		for trial in 0..7 {
			for (i, x) in state.iter_mut().enumerate() {
				*x = 0.21
					* f64::from(u32::try_from((i + 1) * (trial + 3)).map_err(|_| invalid())?).sin();
			}
			let expected = sample(&state)?;
			let actual = kernel.evaluate(&mut F64Backend, &state)?;
			defect = defect.max((actual - expected).abs() / (1. + expected.abs()));
		}
		if !defect.is_finite() || defect > limits.validation_tolerance {
			return Err(CfdError::Assembly("physical observable capture validation"));
		}
		Ok(Self {
			kind,
			provenance: source.provenance,
			kernel,
			enclosure,
			resources: PhysicalObservableResources {
				retained_bytes,
				prepare_peak_bytes,
				prepare_work,
				reference_evaluations: calls,
				query_work,
				query_scratch_bytes,
				terms: polynomial.terms().len(),
				validation_scaled_defect: defect,
				borrowed_source_bytes: source.bytes,
			},
		})
	}
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.kernel.variables()
	}
	#[must_use]
	pub const fn resources(&self) -> PhysicalObservableResources {
		self.resources
	}
	#[must_use]
	pub const fn kind(&self) -> &PhysicalObservableKind {
		&self.kind
	}
	#[must_use]
	pub const fn provenance(&self) -> PhysicalObservableProvenance {
		self.provenance
	}
	/// Evaluate the captured full-coordinate reference; no projection/truncation occurs.
	/// # Errors
	/// Rejects incorrect full shape, nonfinite values and arithmetic overflow.
	pub fn value(&self, state: &[f64]) -> Result<f64, CfdError> {
		Ok(self.kernel.evaluate(&mut F64Backend, state)?)
	}
	/// Prepare a generated diagonal on a complete configuration grid. The range is an
	/// outward enclosure of this stored numerical polynomial over the entire box;
	/// capture/discretization/physical-target errors remain separate and uncertified.
	/// # Errors
	/// Rejects coordinate reduction, invalid finite range and retained/query budgets.
	pub fn configuration<'a>(
		&'a self,
		grid: &'a ConfigurationGrid,
		limits: KvnRecipeLimits,
	) -> Result<GridPhysicalObservable<'a>, CfdError> {
		if grid.axes() != self.dimension() || grid.dimension() > limits.max_dimension {
			return Err(invalid());
		}
		let retained = add(
			add(
				add(self.resources.retained_bytes, grid.retained_bytes()?)?,
				size_of::<GridPhysicalObservable<'a>>(),
			)?,
			mul(8, self.dimension())?,
		)?;
		let query_bytes = add(mul(32, self.dimension())?, 512)?;
		let query_work = add(self.resources.query_work, mul(32, self.dimension())?)?;
		if add(retained, query_bytes)? > limits.max_bytes || query_work > limits.max_query_work {
			return Err(invalid());
		}
		let (lo, hi) = grid.bounds();
		let input = zeros(self.dimension(), Interval::new(lo, hi)?)?;
		if mul(input.capacity(), size_of::<Interval>())? > query_bytes {
			return Err(invalid());
		}
		let range = self.enclosure.evaluate(&mut Interval64Backend, &input)?;
		drop(input);
		let point = zeros(self.dimension(), 0.)?;
		let retained = add(
			retained,
			mul(point.capacity().saturating_sub(self.dimension()), 8)?,
		)?;
		if add(retained, query_bytes)? > limits.max_bytes {
			return Err(invalid());
		}
		Ok(GridPhysicalObservable {
			prepared: self,
			grid,
			point: RefCell::new(point),
			range: DiagonalRange::new(range.lower(), range.upper())?,
			retained,
			query_bytes,
			query_work,
		})
	}
}
/// No complete configuration table: decode one tensor index and query shared kernels.
///
/// The callback is deterministic, single-threaded and compatible with probability
/// reduction. Retained bytes include the borrowed full kernel/grid and scratch capacity.
pub struct GridPhysicalObservable<'a> {
	prepared: &'a PreparedPhysicalObservable,
	grid: &'a ConfigurationGrid,
	point: RefCell<Vec<f64>>,
	range: DiagonalRange,
	retained: usize,
	query_bytes: usize,
	query_work: usize,
}
impl GridPhysicalObservable<'_> {
	#[must_use]
	pub const fn configuration_dimension(&self) -> usize {
		self.grid.dimension()
	}
	#[must_use]
	pub const fn physical_preparation_resources(&self) -> PhysicalObservableResources {
		self.prepared.resources()
	}
	#[must_use]
	pub const fn range(&self) -> DiagonalRange {
		self.range
	}
	#[must_use]
	pub const fn retained_bytes(&self) -> usize {
		self.retained
	}
	#[must_use]
	pub const fn query_bytes(&self) -> usize {
		self.query_bytes
	}
	#[must_use]
	pub const fn query_work(&self) -> usize {
		self.query_work
	}
	/// Single range-enclosure preparation work ceiling, additional to the physical snapshot.
	#[must_use]
	pub const fn preparation_work(&self) -> usize {
		self.query_work
	}
	/// Query one real diagonal value; tensor weights belong to the state amplitudes.
	/// # Errors
	/// Rejects out-of-range indices, reentrant use, and nonfinite arithmetic.
	pub fn value(&self, mut index: usize) -> Result<f64, CfdError> {
		if index >= self.grid.dimension() {
			return Err(invalid());
		}
		let mut point = self.point.try_borrow_mut().map_err(|_| invalid())?;
		for x in point.iter_mut() {
			*x = self
				.grid
				.axis_node(index % self.grid.axis_dimension())
				.ok_or_else(invalid)?;
			index /= self.grid.axis_dimension();
		}
		self.prepared.value(&point)
	}
}
#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn every_capture_ceiling_and_live_source_is_admitted_before_reference_calls() {
		for limits in [
			PhysicalObservableLimits {
				max_bytes: 0,
				..Default::default()
			},
			PhysicalObservableLimits {
				max_prepare_bytes: 0,
				..Default::default()
			},
			PhysicalObservableLimits {
				max_prepare_work: 0,
				..Default::default()
			},
			PhysicalObservableLimits {
				max_query_work: 0,
				..Default::default()
			},
			PhysicalObservableLimits {
				max_reference_evaluations: 0,
				..Default::default()
			},
		] {
			let mut calls = 0;
			assert!(
				PreparedPhysicalObservable::capture(
					1,
					6,
					PhysicalObservableKind::PressureDifference {
						first: [0.; 3],
						second: [0.; 3]
					},
					limits,
					CaptureSource::simplex(2),
					|_, _| {
						calls += 1;
						Ok(0.)
					}
				)
				.is_err()
			);
			assert_eq!(calls, 0);
		}
		let mut source = CaptureSource::simplex(2);
		source.bytes = usize::MAX;
		let mut calls = 0;
		assert!(
			PreparedPhysicalObservable::capture(
				1,
				6,
				PhysicalObservableKind::Enstrophy,
				PhysicalObservableLimits::default(),
				source,
				|_, _| {
					calls += 1;
					Ok(0.)
				}
			)
			.is_err()
		);
		assert_eq!(calls, 0);
	}
	#[test]
	fn oversized_owned_label_is_rejected_before_reference_calls() {
		let mut label = String::with_capacity(2_000_000);
		label.push_str("wall");
		let mut calls = 0;
		let result = PreparedPhysicalObservable::capture(
			1,
			6,
			PhysicalObservableKind::BoundaryForceComponent {
				label,
				component: 0,
			},
			PhysicalObservableLimits {
				max_prepare_bytes: 1_000_000,
				..Default::default()
			},
			CaptureSource::simplex(2),
			|_, _| {
				calls += 1;
				Ok(0.)
			},
		);
		assert!(result.is_err());
		assert_eq!(calls, 0);
	}
}
