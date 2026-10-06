//! Owned complete DG time-power coefficients; no global mesh or reduced drift tensor.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Checked degree<=8, d<=3, p<=2 and owned-cell ranges bound fixed local arithmetic"
)]
use super::{BoxConstraintRecipe, reserved};
use crate::CfdError;
use mathcore::{
	RBig,
	exact::{Owner, Symbol},
	multivariate::{PolynomialKernel, PolynomialLimits, SparsePolynomial},
};
use quest_numerics::arithmetic::F64Backend;
use std::ops::Range;
const PREPARE_SCRATCH: usize = 1_048_576;
const PREPARE_WORK: usize = 100_000_000;
const CELL_WORK: usize = 100_000;
const QUERY_BYTES: usize = 16_384;
const fn invalid() -> CfdError {
	CfdError::InvalidInput("complete box time data shape, domain, compatibility or budget")
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(invalid)
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(invalid)
}
/// One time-power coefficient for one complete cell.
///
/// Active velocity entries use
/// component/scalar-node order. Each exterior trace embeds its full facet nodes
/// in the same ordering; every off-facet/inactive/interior slot is exactly zero.
#[derive(Clone, Copy, Debug)]
pub struct BoxTimeCoefficient {
	pub lifting: [f64; 30],
	pub body_force: [f64; 30],
	pub prescribed: [[f64; 30]; 4],
}
impl BoxTimeCoefficient {
	#[must_use]
	pub const fn zero() -> Self {
		Self {
			lifting: [0.; 30],
			body_force: [0.; 30],
			prescribed: [[0.; 30]; 4],
		}
	}
}
/// Query values in the original broken physical basis; body force is acceleration.
#[derive(Clone, Copy, Debug)]
pub struct BoxTimeEvaluation {
	pub lifting: [f64; 30],
	pub lifting_derivative: [f64; 30],
	pub body_force: [f64; 30],
	pub prescribed: [[f64; 30]; 4],
}
/// Local source preparation/query limits. Collective validation is separately admitted.
#[derive(Clone, Copy, Debug)]
pub struct BoxTimeDataLimits {
	pub max_bytes: usize,
	pub max_prepare_work: usize,
	pub max_cell_work: usize,
	pub max_degree: usize,
	pub compatibility_tolerance: f64,
}
impl Default for BoxTimeDataLimits {
	fn default() -> Self {
		Self {
			max_bytes: 64 * 1024 * 1024,
			max_prepare_work: 1_000_000_000,
			max_cell_work: CELL_WORK,
			max_degree: 8,
			compatibility_tolerance: 1e-9,
		}
	}
}
/// Owned numerical capacities plus fixed kernel work/scratch bounds; excludes geometry.
#[derive(Clone, Copy, Debug)]
pub struct BoxTimeDataResources {
	pub retained_bytes: usize,
	pub prepare_peak_bytes: usize,
	pub prepare_work: usize,
	pub cell_query_work: usize,
	pub query_scratch_bytes: usize,
}
/// Locally shaped immutable shards.
///
/// This type alone does not assert global
/// compatibility; collective preparation checks every coefficient and full constraint.
/// Coefficients are time-power-major, then owned cell. No omitted physical modes.
pub struct BoxTimeDataShard<'source> {
	source: &'source BoxConstraintRecipe,
	range: Range<usize>,
	degree: usize,
	interval: [f64; 2],
	coefficients: Vec<BoxTimeCoefficient>,
	powers: Vec<PolynomialKernel<f64>>,
	derivatives: Vec<PolynomialKernel<f64>>,
	limits: BoxTimeDataLimits,
	resources: BoxTimeDataResources,
}
pub(super) fn validate_fields(
	source: &BoxConstraintRecipe,
	cell: usize,
	ell: &[f64; 30],
	body: &[f64; 30],
	trace: &[[f64; 30]; 4],
) -> Result<(), CfdError> {
	let n = source.local_velocity_dimension();
	let d = source.dimension();
	let scalar = n / d;
	if cell >= source.cell_count()
		|| ell
			.iter()
			.chain(body)
			.chain(trace.iter().flatten())
			.any(|x| !x.is_finite())
		|| ell[n..].iter().chain(&body[n..]).any(|&x| x != 0.)
	{
		return Err(invalid());
	}
	for (face, row) in trace.iter().enumerate() {
		let exterior = face <= d && source.facet_is_exterior(cell, face)?;
		for (i, &v) in row.iter().enumerate() {
			let mut active = false;
			if exterior && i < n {
				for mode in 0..source.facet_mode_count() {
					active |= source.facet_velocity_node(face, mode)? == i % scalar;
				}
			}
			if !active && v != 0. {
				return Err(invalid());
			}
		}
	}
	Ok(())
}
impl<'source> BoxTimeDataShard<'source> {
	/// Admit actual input capacities before preparing shared `MathCore` time kernels.
	/// # Errors
	/// Rejects invalid complete shape, inactive entries, finite interval or budgets.
	pub fn new(
		source: &'source BoxConstraintRecipe,
		range: Range<usize>,
		degree: usize,
		interval: [f64; 2],
		coefficients: Vec<BoxTimeCoefficient>,
		limits: BoxTimeDataLimits,
	) -> Result<Self, CfdError> {
		if range.start > range.end
			|| range.end > source.cell_count()
			|| degree > 8
			|| degree > limits.max_degree
			|| interval.iter().any(|v| !v.is_finite())
			|| interval[0] > interval[1]
			|| !limits.compatibility_tolerance.is_finite()
			|| limits.compatibility_tolerance <= 0.
			|| coefficients.len() != mul(degree + 1, range.len())?
		{
			return Err(invalid());
		}
		let input = add(
			size_of::<Self>(),
			mul(coefficients.capacity(), size_of::<BoxTimeCoefficient>())?,
		)?;
		let planned = add(input, PREPARE_SCRATCH)?;
		let work = add(PREPARE_WORK, mul(coefficients.len(), CELL_WORK)?)?;
		if planned > limits.max_bytes
			|| work > limits.max_prepare_work
			|| CELL_WORK > limits.max_cell_work
		{
			return Err(invalid());
		}
		for (index, c) in coefficients.iter().enumerate() {
			validate_fields(
				source,
				range.start + index % range.len(),
				&c.lifting,
				&c.body_force,
				&c.prescribed,
			)?;
		}
		let symbol = Symbol::new(Owner::new(0x424f_5854_494d_4501), 0);
		let policy = PolynomialLimits {
			max_terms: 2,
			max_degree: 8,
			max_bytes: 65536,
			max_work: 65536,
			..Default::default()
		};
		let mut powers = reserved(degree + 1)?;
		let mut derivatives = reserved(degree + 1)?;
		for k in 0..=degree {
			let exp = u32::try_from(k).map_err(|_| invalid())?;
			let p = SparsePolynomial::from_terms(
				vec![symbol],
				vec![(vec![exp], RBig::from(1))],
				policy,
			)?;
			powers.push(p.lower(&mut F64Backend)?);
			derivatives.push(p.differentiate(symbol)?.lower(&mut F64Backend)?);
		}
		let mut retained = add(
			input,
			mul(
				powers.capacity() + derivatives.capacity(),
				size_of::<PolynomialKernel<f64>>(),
			)?,
		)?;
		for kernel in powers.iter().chain(&derivatives) {
			retained = add(retained, kernel.retained_bytes()?)?;
		}
		let prepare_peak = add(retained, PREPARE_SCRATCH)?;
		if prepare_peak > limits.max_bytes {
			return Err(invalid());
		}
		let out = Self {
			source,
			range,
			degree,
			interval,
			coefficients,
			powers,
			derivatives,
			limits,
			resources: BoxTimeDataResources {
				retained_bytes: retained,
				prepare_peak_bytes: prepare_peak,
				prepare_work: work,
				cell_query_work: CELL_WORK,
				query_scratch_bytes: QUERY_BYTES,
			},
		};
		// Endpoint magnitude bounds dominate powers/derivatives on the admitted interval.
		out.modes(interval[0])?;
		out.modes(interval[1])?;
		Ok(out)
	}
	#[must_use]
	pub const fn source(&self) -> &BoxConstraintRecipe {
		self.source
	}
	#[must_use]
	pub fn cell_range(&self) -> Range<usize> {
		self.range.clone()
	}
	#[must_use]
	pub const fn degree(&self) -> usize {
		self.degree
	}
	#[must_use]
	pub const fn time_interval(&self) -> [f64; 2] {
		self.interval
	}
	#[must_use]
	pub const fn resources(&self) -> BoxTimeDataResources {
		self.resources
	}
	#[must_use]
	pub const fn compatibility_tolerance(&self) -> f64 {
		self.limits.compatibility_tolerance
	}
	/// Complete owned coefficient without allocation.
	#[must_use]
	pub fn coefficient(&self, k: usize, cell: usize) -> Option<&BoxTimeCoefficient> {
		if k > self.degree || !self.range.contains(&cell) {
			None
		} else {
			self.coefficients
				.get(k * self.range.len() + cell - self.range.start)
		}
	}
	fn modes(&self, time: f64) -> Result<([f64; 9], [f64; 9]), CfdError> {
		if !time.is_finite() || time < self.interval[0] || time > self.interval[1] {
			return Err(invalid());
		}
		let mut v = [0.; 9];
		let mut dv = [0.; 9];
		for k in 0..=self.degree {
			v[k] = self.powers[k].evaluate(&mut F64Backend, &[time])?;
			dv[k] = self.derivatives[k].evaluate(&mut F64Backend, &[time])?;
		}
		if v.iter().chain(&dv).any(|x| !x.is_finite()) {
			return Err(invalid());
		}
		Ok((v, dv))
	}
	/// Evaluate every retained local mode and its analytic temporal derivative.
	/// # Errors
	/// Rejects unowned cells, times outside the declared interval or arithmetic overflow.
	pub fn evaluate_cell(&self, time: f64, cell: usize) -> Result<BoxTimeEvaluation, CfdError> {
		let (v, dv) = self.modes(time)?;
		let mut out = BoxTimeEvaluation {
			lifting: [0.; 30],
			lifting_derivative: [0.; 30],
			body_force: [0.; 30],
			prescribed: [[0.; 30]; 4],
		};
		for k in 0..=self.degree {
			let c = self.coefficient(k, cell).ok_or_else(invalid)?;
			for j in 0..30 {
				out.lifting[j] += v[k] * c.lifting[j];
				out.lifting_derivative[j] += dv[k] * c.lifting[j];
				out.body_force[j] += v[k] * c.body_force[j];
				for f in 0..4 {
					out.prescribed[f][j] += v[k] * c.prescribed[f][j];
				}
			}
		}
		if out
			.lifting
			.iter()
			.chain(&out.lifting_derivative)
			.chain(&out.body_force)
			.chain(out.prescribed.iter().flatten())
			.any(|x| !x.is_finite())
		{
			return Err(invalid());
		}
		Ok(out)
	}
	/// Check every divergence and normal coefficient for one owned cell and power.
	/// `fetch` must supply that same power's immutable neighbor lifting. Its own cost
	/// is additional. Distributed callers use the fixed collective halo schedule.
	/// # Errors
	/// Rejects invalid ownership/data, failed fetch, overflow or incompatible modes.
	pub fn validate_cell(
		&self,
		k: usize,
		cell: usize,
		fetch: &mut impl FnMut(usize) -> Result<[f64; 30], CfdError>,
	) -> Result<f64, CfdError> {
		let c = self.coefficient(k, cell).ok_or_else(invalid)?;
		let source = self.source;
		let n = source.local_velocity_dimension();
		let d = source.dimension();
		let modes = source.facet_mode_count();
		let mut defect = 0_f64;
		let mut scale = 1_f64;
		for &v in c.lifting.iter().chain(c.prescribed.iter().flatten()) {
			scale = scale.max(v.abs());
		}
		let mut neighbors = [[0.; 30]; 4];
		for (face, words) in neighbors.iter_mut().enumerate().take(d + 1) {
			if let Some((other, _)) = source.partner(cell, face) {
				*words = fetch(other)?;
				if words.iter().any(|v| !v.is_finite()) || words[n..].iter().any(|&x| x != 0.) {
					return Err(invalid());
				}
				for &v in words.iter() {
					scale = scale.max(v.abs());
				}
			}
			for mode in 0..modes {
				let column = (cell * (d + 1) + face) * modes + mode;
				let mut residual = 0.;
				for (i, &v) in c.lifting.iter().enumerate().take(n) {
					residual += source.constraint_value(cell, i, column)? * v;
				}
				if let Some((other, _)) = source.partner(cell, face) {
					for (i, &v) in words.iter().enumerate().take(n) {
						residual += source.constraint_value(other, i, column)? * v;
					}
				} else {
					let node = source.facet_velocity_node(face, mode)?;
					let normal = source.normal(cell % source.permutation_count(), face);
					for axis in 0..d {
						residual -= normal[axis] * c.prescribed[face][axis * (n / d) + node];
					}
				}
				if !residual.is_finite() {
					return Err(invalid());
				}
				defect = defect.max(residual.abs());
			}
		}
		for pressure in 0..source.pressure_modes() {
			let column =
				source.pressure_constraint_start() + cell * source.pressure_modes() + pressure;
			let mut residual = 0.;
			for (i, &v) in c.lifting.iter().enumerate().take(n) {
				residual += source.constraint_value(cell, i, column)? * v;
			}
			if !residual.is_finite() {
				return Err(invalid());
			}
			defect = defect.max(residual.abs());
		}
		let threshold = self.limits.compatibility_tolerance * scale;
		if !threshold.is_finite() || defect > threshold {
			return Err(invalid());
		}
		Ok(defect)
	}
}
