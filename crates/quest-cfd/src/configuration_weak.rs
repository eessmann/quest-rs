//! Bounded initial weak rates of the complete periodic five-coordinate DG source.
//!
//! This is a classical diagnostic of a supplied configuration state, not a history
//! solve, quantum measurement, flux calculation or convergence certificate.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Checked admissions bound fixed-array indexing; every published numerical result is finite-checked"
)]
use crate::{
	CfdError, PeriodicBdm1,
	configuration::ConfigurationGrid,
	kvn_recipe::{KvnHistoryRecipe, KvnRecipeLimits},
	polynomial::{PolynomialSnapshot, SnapshotEvidence},
	stream_history::HistoryRowDynamics,
};
use mathcore::multivariate::PolynomialLimits;
use quest_numerics::Complex64;

const SOURCE_BYTES: usize = 8 * 1024 * 1024;
const SOURCE_WORK: usize = 100_000_000;
const PHYSICAL_CALL_WORK: usize = 100_000;
const EXTRACTION_CALLS: usize = 58;
const SCRATCH_BYTES: usize = 16 * 1024;

/// Single cumulative arithmetic and managed-payload ceilings. These are not OS quotas.
#[derive(Clone, Copy, Debug)]
pub struct WeakLimits {
	pub max_bytes: usize,
	pub max_source_work: usize,
	pub max_physical_work: usize,
	pub max_physical_calls: usize,
}
impl Default for WeakLimits {
	fn default() -> Self {
		Self {
			max_bytes: 268_435_456,
			max_source_work: 1_000_000_000,
			max_physical_work: 100_000_000_000,
			max_physical_calls: 1_000_000,
		}
	}
}
/// Per-query controls; hidden slice backing capacity and other live owners must be declared.
#[derive(Clone, Copy, Debug, Default)]
pub struct WeakRequest {
	pub time: f64,
	pub additional_external_bytes: usize,
	/// Prior grid/state preparation is unknown/outside this request when None.
	/// Some is a caller declaration, not retroactive admission of prior allocations.
	pub caller_declared_input_preparation_work: Option<usize>,
	pub nonlinear_witness: bool,
}
/// Conservative cumulative admission plus distinct actual callback counts.
#[derive(Clone, Copy, Debug, Default)]
pub struct WeakResources {
	pub source_work: usize,
	pub caller_declared_input_preparation_work: Option<usize>,
	pub physical_work: usize,
	pub physical_calls: usize,
	pub physical_calls_attempted: usize,
	pub constructor_peak_bytes: usize,
	pub retained_source_bytes: usize,
	pub grid_bytes: usize,
	pub accessible_state_bytes: usize,
	pub external_bytes: usize,
	pub result_bytes: usize,
	pub scratch_bytes: usize,
	pub peak_bytes: usize,
	pub supported_rows: usize,
	pub row_query_work: usize,
	pub maximum_row_entries: usize,
}
/// Failed preparation retains the attempted constructor envelope.
pub struct WeakPreparation {
	pub outcome: Result<PeriodicWeakSource, CfdError>,
	pub resources: WeakResources,
}
/// One observable rate. Scaled defect uses max(1, absolute physical rate).
#[derive(Clone, Copy, Debug, Default)]
pub struct WeakRate {
	pub expectation: f64,
	pub raw_skew_rate: f64,
	pub normalized_rate: f64,
	pub physical_rate: f64,
	pub absolute_defect: f64,
	pub scaled_defect: f64,
}
/// Numerical initial consistency only. Entries are five coordinates then integrated energy.
#[derive(Clone, Debug)]
pub struct WeakReport {
	pub time: f64,
	/// Noncryptographic identity of the fixed physical recipe, viscosity and full chart.
	pub source_identity: u64,
	pub dimension: usize,
	pub probability: f64,
	pub probability_rate: f64,
	pub rates: [WeakRate; 6],
	pub zero_exterior_trace: bool,
	pub outer_cell_occupation: f64,
	pub coordinate_standard_deviations: [f64; 5],
	pub maximum_coefficient_probability: f64,
	pub effective_coefficients: f64,
	pub nonlinear_mean_square: Option<f64>,
	pub cartesian_energy_discrepancy: f64,
	pub chart_mass_residual: f64,
	pub extraction_probe_error: f64,
	pub convergence_certified: bool,
}
/// A rejection never discards the latest planned receipt or claims a computed rate.
pub struct WeakAttempt {
	pub outcome: Result<WeakReport, CfdError>,
	pub resources: WeakResources,
	pub phase: &'static str,
	pub validated_rows: usize,
	pub visited_rows: usize,
}
/// Owns the original full DG source and its complete independently checked polynomial snapshot.
pub struct PeriodicWeakSource {
	model: PeriodicBdm1,
	snapshot: PolynomialSnapshot,
	resources: WeakResources,
	source_identity: u64,
}
const fn invalid() -> CfdError {
	CfdError::InvalidInput("weak diagnostic admission or nonfinite arithmetic")
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(invalid)
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(invalid)
}
const fn finite(x: f64) -> Result<f64, CfdError> {
	if x.is_finite() { Ok(x) } else { Err(invalid()) }
}
fn limits_valid(l: WeakLimits) -> Result<(), CfdError> {
	let hard = WeakLimits::default();
	if l.max_bytes == 0
		|| l.max_bytes > hard.max_bytes
		|| l.max_source_work == 0
		|| l.max_source_work > hard.max_source_work
		|| l.max_physical_work == 0
		|| l.max_physical_work > hard.max_physical_work
		|| l.max_physical_calls == 0
		|| l.max_physical_calls > hard.max_physical_calls
	{
		return Err(invalid());
	}
	Ok(())
}
fn admit(r: WeakResources, l: WeakLimits) -> Result<(), CfdError> {
	limits_valid(l)?;
	if r.peak_bytes > l.max_bytes
		|| r.source_work > l.max_source_work
		|| r.physical_work > l.max_physical_work
		|| r.physical_calls > l.max_physical_calls
	{
		Err(invalid())
	} else {
		Ok(())
	}
}
fn exact_zero(z: Complex64) -> bool {
	z.re == 0. && z.im == 0.
}
impl PeriodicWeakSource {
	/// Prepare the fixed complete source, retaining failed resource admission.
	/// The 8 MiB / 100 million construction envelope covers fixed assembly and
	/// bounded shared polynomial extraction; its 58 original force calls are separate.
	#[must_use]
	pub fn prepare(
		viscosity: f64,
		external_retained_bytes: usize,
		limits: WeakLimits,
	) -> WeakPreparation {
		let mut r = WeakResources::default();
		let outcome = (|| {
			r.external_bytes = external_retained_bytes;
			r.constructor_peak_bytes = add(SOURCE_BYTES, external_retained_bytes)?;
			r.peak_bytes = r.constructor_peak_bytes;
			r.source_work = SOURCE_WORK;
			r.physical_calls = EXTRACTION_CALLS;
			r.physical_work = mul(EXTRACTION_CALLS, PHYSICAL_CALL_WORK)?;
			admit(r, limits)?;
			let model = PeriodicBdm1::assemble(viscosity)?;
			let polynomial_limits = PolynomialLimits {
				max_variables: 6,
				max_terms: 256,
				max_degree: 4,
				max_coefficient_bits: 256,
				max_bytes: 1024 * 1024,
				max_work: 1_000_000,
			};
			// Fixed complete quadratic extraction has exactly 1+2*5+4*10+7 probes.
			let attempted = std::cell::Cell::new(0usize);
			let extracted = crate::polynomial::snapshot_quadratic(
				5,
				|state| {
					attempted.set(add(attempted.get(), 1)?);
					model.drift(state)
				},
				polynomial_limits,
			);
			r.physical_calls_attempted = attempted.get();
			let snapshot = extracted?;
			if model.dimension() != 5 || snapshot.evidence.residual_evaluations != EXTRACTION_CALLS
			{
				return Err(invalid());
			}
			r.retained_source_bytes = add(
				add(size_of::<Self>(), model.retained_bytes()?)?,
				add(
					snapshot.dynamics.retained_bytes(),
					snapshot.evidence.status.capacity(),
				)?,
			)?;
			if add(r.retained_source_bytes, SCRATCH_BYTES)? > SOURCE_BYTES {
				return Err(invalid());
			}
			let mut identity = 0xcbf2_9ce4_8422_2325_u64;
			for word in std::iter::once(1_u64)
				.chain(std::iter::once(viscosity.to_bits()))
				.chain(model.chart().iter().flatten().map(|x| x.to_bits()))
			{
				for byte in word.to_le_bytes() {
					identity = (identity ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
				}
			}
			Ok(Self {
				model,
				snapshot,
				resources: r,
				source_identity: identity,
			})
		})();
		WeakPreparation {
			outcome,
			resources: r,
		}
	}
	/// Original complete five-coordinate physical assembly, without a reduced model.
	#[must_use]
	pub const fn model(&self) -> &PeriodicBdm1 {
		&self.model
	}
	/// Numerical extraction comparison evidence, not an exact assembly certificate.
	#[must_use]
	pub const fn extraction_evidence(&self) -> &SnapshotEvidence {
		&self.snapshot.evidence
	}
	/// Complete source construction and actual retained-owner admission.
	#[must_use]
	pub const fn resources(&self) -> WeakResources {
		self.resources
	}
	/// Diagnose a complete supplied state without storing a generator or its action.
	/// Limits cover source construction plus one diagnostic invocation; they are not
	/// a decremented lifetime budget. Construction is charged again on every query.
	///
	/// All state entries are validated. Only exactly zero real AND imaginary components
	/// omit a contracted row. Borrowed slice payload is counted; inaccessible allocation
	/// capacity must appear in the request's additional external bytes.
	#[must_use]
	pub fn diagnose(
		&self,
		grid: &ConfigurationGrid,
		state: &[Complex64],
		request: WeakRequest,
		limits: WeakLimits,
	) -> WeakAttempt {
		let mut attempt = WeakAttempt {
			outcome: Err(invalid()),
			resources: self.resources,
			phase: "input admission",
			validated_rows: 0,
			visited_rows: 0,
		};
		attempt.outcome = self.run(
			grid,
			state,
			request,
			limits,
			&mut attempt.resources,
			&mut attempt.phase,
			&mut attempt.validated_rows,
			&mut attempt.visited_rows,
		);
		attempt
	}
	#[allow(
		clippy::too_many_arguments,
		clippy::too_many_lines,
		reason = "One auditable phase sequence preserves cumulative admission, independent physical queries and partial receipts on failure"
	)]
	fn run(
		&self,
		grid: &ConfigurationGrid,
		state: &[Complex64],
		request: WeakRequest,
		limits: WeakLimits,
		r: &mut WeakResources,
		phase: &mut &'static str,
		validated: &mut usize,
		visited: &mut usize,
	) -> Result<WeakReport, CfdError> {
		if grid.axes() != 5 || state.len() != grid.dimension() || !request.time.is_finite() {
			return Err(invalid());
		}
		r.external_bytes = add(r.external_bytes, request.additional_external_bytes)?;
		r.caller_declared_input_preparation_work = request.caller_declared_input_preparation_work;
		if let Some(work) = request.caller_declared_input_preparation_work {
			r.source_work = add(r.source_work, work)?;
		}
		r.accessible_state_bytes = mul(state.len(), size_of::<Complex64>())?;
		r.result_bytes = add(size_of::<WeakReport>(), size_of::<WeakAttempt>())?;
		r.scratch_bytes = SCRATCH_BYTES;
		// Charge traversal of all axis metadata before its capacity/validity scan.
		r.source_work = add(
			r.source_work,
			add(mul(grid.axis_dimension(), 4096)?, mul(state.len(), 4096)?)?,
		)?;
		r.source_work = add(
			r.source_work,
			add(mul(self.snapshot.dynamics.retained_bytes(), 128)?, 4096)?,
		)?;
		r.peak_bytes = r.peak_bytes.max(add(
			add(
				add(r.retained_source_bytes, r.external_bytes)?,
				r.accessible_state_bytes,
			)?,
			add(r.result_bytes, r.scratch_bytes)?,
		)?);
		admit(*r, limits)?;
		r.grid_bytes = grid.retained_bytes()?;
		r.peak_bytes = r.peak_bytes.max(add(
			add(
				add(
					add(r.retained_source_bytes, r.external_bytes)?,
					r.accessible_state_bytes,
				)?,
				r.grid_bytes,
			)?,
			add(r.result_bytes, r.scratch_bytes)?,
		)?);
		admit(*r, limits)?;
		let recipe = KvnHistoryRecipe::new(
			grid,
			&self.snapshot.dynamics,
			KvnRecipeLimits {
				max_dimension: grid.dimension(),
				max_bytes: limits.max_bytes,
				max_query_work: limits.max_source_work,
			},
		)?;
		let qr = recipe.resources();
		if add(qr.row_query_bytes, 4096)? > r.scratch_bytes {
			return Err(invalid());
		}
		r.row_query_work = qr.row_query_work;
		r.maximum_row_entries = qr.maximum_row_entries;
		*phase = "state validation";
		let scan = scan_state(grid, state, validated)?;
		r.supported_rows = scan.supported;
		*phase = "contraction admission";
		let per_row = add(
			qr.row_query_work,
			add(mul(qr.maximum_row_entries, 64)?, 16384)?,
		)?;
		r.source_work = add(r.source_work, mul(scan.supported, per_row)?)?;
		let calls = if request.nonlinear_witness {
			add(mul(scan.supported, 2)?, 1)?
		} else {
			scan.supported
		};
		r.physical_calls = add(r.physical_calls, calls)?;
		r.physical_work = add(r.physical_work, mul(calls, PHYSICAL_CALL_WORK)?)?;
		admit(*r, limits)?;
		*phase = "contraction";
		let zero = if request.nonlinear_witness {
			r.physical_calls_attempted = add(r.physical_calls_attempted, 1)?;
			Some(self.model.drift(&[0.; 5])?)
		} else {
			None
		};
		let mut accumulator = Accumulator::<6>::default();
		let mut witness = 0.;
		let mut energy_discrepancy = 0_f64;
		for (row, &z) in state.iter().enumerate() {
			if exact_zero(z) {
				continue;
			}
			*visited = add(*visited, 1)?;
			let action = contract_row(&recipe, request.time, row, state)?;
			let point = grid.point(row).ok_or_else(invalid)?;
			r.physical_calls_attempted = add(r.physical_calls_attempted, 1)?;
			let drift = self.model.drift(&point)?;
			let coefficients = self.model.coefficients(&point)?;
			let velocity_dot = self.model.coefficients(&drift)?;
			let energy = finite(0.5 * cartesian_inner(&coefficients, &coefficients))?;
			let energy_rate = finite(cartesian_inner(&coefficients, &velocity_dot))?;
			let mass_energy = self.model.energy(&point)?;
			energy_discrepancy = energy_discrepancy.max(finite((energy - mass_energy).abs())?);
			let coordinate_power = finite(point.iter().zip(&drift).map(|(a, f)| a * f).sum())?;
			energy_discrepancy =
				energy_discrepancy.max(finite((energy_rate - coordinate_power).abs())?);
			audit_query_capacity(r, qr.row_query_bytes, &[&point, &drift], zero.as_ref())?;
			let observable = [point[0], point[1], point[2], point[3], point[4], energy];
			let reference = [
				drift[0],
				drift[1],
				drift[2],
				drift[3],
				drift[4],
				energy_rate,
			];
			accumulator.push(z, action, observable, reference)?;
			if let Some(zero) = &zero {
				let negative: Vec<_> = point.iter().map(|x| -x).collect();
				r.physical_calls_attempted = add(r.physical_calls_attempted, 1)?;
				let minus = self.model.drift(&negative)?;
				audit_query_capacity(
					r,
					qr.row_query_bytes,
					&[&point, &drift, &negative, &minus],
					Some(zero),
				)?;
				let squared = finite(
					drift
						.iter()
						.zip(&minus)
						.zip(zero)
						.map(|((p, m), c)| {
							let f = 0.5 * (p + m - 2. * c);
							f * f
						})
						.sum(),
				)?;
				witness = finite(witness + finite(z.norm_sqr() * squared)?)?;
			}
		}
		let rates = accumulator.finish(scan.probability)?;
		let mut deviations = [0.; 5];
		for (i, &z) in state.iter().enumerate() {
			if !exact_zero(z) {
				let point = grid.point(i).ok_or_else(invalid)?;
				for axis in 0..5 {
					let d = point[axis] - rates[axis].expectation;
					deviations[axis] = finite(
						deviations[axis] + finite((z.norm_sqr() / scan.probability) * d * d)?,
					)?;
				}
			}
		}
		for d in &mut deviations {
			*d = finite(d.sqrt())?;
		}
		let report = WeakReport {
			time: request.time,
			source_identity: self.source_identity,
			dimension: state.len(),
			probability: scan.probability,
			probability_rate: accumulator.qdot,
			rates,
			zero_exterior_trace: scan.zero_trace,
			outer_cell_occupation: finite(scan.outer / scan.probability)?,
			coordinate_standard_deviations: deviations,
			maximum_coefficient_probability: finite(scan.maximum / scan.probability)?,
			effective_coefficients: effective_coefficients(state, scan.probability)?,
			nonlinear_mean_square: if request.nonlinear_witness {
				Some(finite(witness / scan.probability)?)
			} else {
				None
			},
			cartesian_energy_discrepancy: energy_discrepancy,
			chart_mass_residual: self.model.diagnostics().mass_orthogonality_residual,
			extraction_probe_error: self.snapshot.evidence.independent_probe_max_error,
			convergence_certified: false,
		};
		*phase = "complete";
		Ok(report)
	}
}
fn audit_query_capacity(
	r: &WeakResources,
	row_bytes: usize,
	vectors: &[&Vec<f64>],
	zero: Option<&Vec<f64>>,
) -> Result<(), CfdError> {
	let mut bytes = add(row_bytes, 4096)?;
	for v in vectors.iter().copied().chain(zero) {
		bytes = add(bytes, mul(v.capacity(), size_of::<f64>())?)?;
	}
	if bytes > r.scratch_bytes {
		Err(invalid())
	} else {
		Ok(())
	}
}
struct Scan {
	probability: f64,
	supported: usize,
	zero_trace: bool,
	outer: f64,
	maximum: f64,
}
fn scan_state(
	grid: &ConfigurationGrid,
	state: &[Complex64],
	validated: &mut usize,
) -> Result<Scan, CfdError> {
	let n = grid.axis_dimension();
	// The public grid stores its uniform cell/order metadata; endpoint row sizes
	// identify q=2 or q=3 without rounded coordinate comparisons.
	let q = grid
		.axis_derivative_row(0)
		.ok_or_else(invalid)?
		.len()
		.checked_sub(2)
		.ok_or_else(invalid)?;
	if !(q == 2 || q == 3) || !n.is_multiple_of(q) {
		return Err(invalid());
	}
	let mut out = Scan {
		probability: 0.,
		supported: 0,
		zero_trace: true,
		outer: 0.,
		maximum: 0.,
	};
	for (i, &z) in state.iter().enumerate() {
		finite(z.re)?;
		finite(z.im)?;
		let p = finite(z.norm_sqr())?;
		out.probability = finite(out.probability + p)?;
		out.maximum = out.maximum.max(p);
		if !exact_zero(z) {
			out.supported = add(out.supported, 1)?;
		}
		let mut index = i;
		let mut exterior = false;
		let mut outer = false;
		for _ in 0..grid.axes() {
			let a = index % n;
			index /= n;
			exterior |= a == 0 || a == n - 1;
			outer |= a / q == 0 || a / q == n / q - 1;
		}
		if exterior && !exact_zero(z) {
			out.zero_trace = false;
		}
		if outer {
			out.outer = finite(out.outer + p)?;
		}
		*validated = add(*validated, 1)?;
	}
	if out.probability <= 0. {
		return Err(invalid());
	}
	Ok(out)
}
fn effective_coefficients(state: &[Complex64], q: f64) -> Result<f64, CfdError> {
	let mut square = 0.;
	for z in state {
		let p = z.norm_sqr() / q;
		square = finite(square + p * p)?;
	}
	finite(1. / square)
}
fn contract_row(
	source: &impl HistoryRowDynamics,
	time: f64,
	row: usize,
	state: &[Complex64],
) -> Result<Complex64, CfdError> {
	let mut action = Complex64::new(0., 0.);
	source.visit_row(time, row, &mut |column, value| {
		let z = *state.get(column).ok_or_else(invalid)?;
		action += value * z;
		finite(action.re)?;
		finite(action.im)?;
		Ok(())
	})?;
	Ok(action)
}
struct Accumulator<const K: usize> {
	moment: [f64; K],
	rate: [f64; K],
	physical: [f64; K],
	qdot: f64,
}
impl<const K: usize> Default for Accumulator<K> {
	fn default() -> Self {
		Self {
			moment: [0.; K],
			rate: [0.; K],
			physical: [0.; K],
			qdot: 0.,
		}
	}
}
impl<const K: usize> Accumulator<K> {
	fn push(
		&mut self,
		z: Complex64,
		action: Complex64,
		observable: [f64; K],
		physical: [f64; K],
	) -> Result<(), CfdError> {
		let p = finite(z.norm_sqr())?;
		let rate = finite(2. * (z.conj() * action).re)?;
		self.qdot = finite(self.qdot + rate)?;
		for i in 0..K {
			self.moment[i] = finite(self.moment[i] + finite(p * observable[i])?)?;
			self.rate[i] = finite(self.rate[i] + finite(rate * observable[i])?)?;
			self.physical[i] = finite(self.physical[i] + finite(p * physical[i])?)?;
		}
		Ok(())
	}
	fn finish(&self, q: f64) -> Result<[WeakRate; K], CfdError> {
		let mut result = [WeakRate::default(); K];
		for (i, r) in result.iter_mut().enumerate() {
			r.expectation = finite(self.moment[i] / q)?;
			r.raw_skew_rate = finite(self.rate[i] / q)?;
			r.normalized_rate =
				finite(r.raw_skew_rate - finite(r.expectation * finite(self.qdot / q)?)?)?;
			r.physical_rate = finite(self.physical[i] / q)?;
			r.absolute_defect = finite((r.normalized_rate - r.physical_rate).abs())?;
			r.scaled_defect = finite(r.absolute_defect / r.physical_rate.abs().max(1.))?;
		}
		Ok(result)
	}
}
// Exact integrals of Cartesian affine products, independent of the runtime mass
// matrix/quadrature. Cell0: 0<=y<=x<=1; cell1: 0<=x<=y<=1.
fn cartesian_inner(a: &[f64; 12], b: &[f64; 12]) -> f64 {
	let mass = [
		[
			[0.5, 1. / 3., 1. / 6.],
			[1. / 3., 0.25, 0.125],
			[1. / 6., 0.125, 1. / 12.],
		],
		[
			[0.5, 1. / 6., 1. / 3.],
			[1. / 6., 1. / 12., 0.125],
			[1. / 3., 0.125, 0.25],
		],
	];
	let mut value = 0.;
	for (cell, block) in mass.iter().enumerate() {
		for component in 0..2 {
			for i in 0..3 {
				for j in 0..3 {
					value += a[cell * 6 + component * 3 + i]
						* block[i][j]
						* b[cell * 6 + component * 3 + j];
				}
			}
		}
	}
	value
}

#[cfg(test)]
#[allow(
	clippy::panic_in_result_fn,
	clippy::float_cmp,
	reason = "Independent exact polynomial, dense complex and conservative byte-boundary oracles"
)]
mod tests {
	use super::*;
	use crate::polynomial::PolynomialOde;
	use mathcore::{
		RBig,
		exact::{Owner, Symbol},
		multivariate::SparsePolynomial,
	};
	fn affine(f0: i32, f1: i32) -> Result<PolynomialOde, CfdError> {
		let limits = PolynomialLimits::default();
		let symbols = vec![Symbol::new(Owner::new(123), 0)];
		PolynomialOde::from_polynomials(
			vec![SparsePolynomial::from_terms(
				symbols,
				[(vec![0], RBig::from(f0)), (vec![1], RBig::from(f1))],
				limits,
			)?],
			1,
			limits,
		)
	}
	fn hat(grid: &ConfigurationGrid) -> Result<Vec<Complex64>, CfdError> {
		(0..grid.dimension())
			.map(|i| {
				let x = grid.axis_node(i).ok_or_else(invalid)?;
				let psi = if x < -0.5 {
					2. * (x + 1.)
				} else if x < 0. {
					1.
				} else if x < 0.5 {
					1. - 2. * x
				} else {
					0.
				};
				Ok(Complex64::new(
					psi * grid.axis_weight(i).ok_or_else(invalid)?.sqrt(),
					0.,
				))
			})
			.collect()
	}
	#[test]
	fn polynomial_hat_has_exact_constant_and_affine_weak_rates() -> Result<(), CfdError> {
		let grid = ConfigurationGrid::uniform(1, -1., 1., 4, 2, 12)?;
		let state = hat(&grid)?;
		let norm: f64 = state.iter().map(Complex64::norm_sqr).sum();
		assert!((norm - 5. / 6.).abs() < 1e-14);
		for (f0, f1) in [(2, 0), (1, 2)] {
			let ode = affine(f0, f1)?;
			let recipe = KvnHistoryRecipe::new(&grid, &ode, KvnRecipeLimits::default())?;
			let mut a = Accumulator::<2>::default();
			for (row, &z) in state.iter().enumerate() {
				let x = grid.axis_node(row).ok_or_else(invalid)?;
				let f = f64::from(f0) + f64::from(f1) * x;
				a.push(
					z,
					contract_row(&recipe, 0., row, &state)?,
					[x, x * x],
					[f, 2. * x * f],
				)?;
			}
			let rates = a.finish(norm)?;
			assert!(a.qdot.abs() < 1e-14);
			assert!((rates[0].expectation + 0.25).abs() < 1e-14);
			assert!(
				(rates[0].normalized_rate - (f64::from(f0) - 0.25 * f64::from(f1))).abs() < 1e-13
			);
			if f1 == 0 {
				assert!((rates[1].normalized_rate + 1.).abs() < 1e-13);
			}
		}
		Ok(())
	}
	#[test]
	fn complex_dense_commutator_and_normalization_correction_are_independent()
	-> Result<(), CfdError> {
		// Single DG2 central periodic cell: independent W^1/2 D W^-1/2 matrix.
		// Dcentral includes periodic endpoint jumps; W=diag(1,4,1)/3.
		let d = [[0., 1., -2.], [-1., 0., 1.], [2., -1., 0.]];
		let grid = ConfigurationGrid::uniform(1, -1., 1., 1, 2, 3)?;
		let ode = affine(1, 0)?;
		let recipe = KvnHistoryRecipe::new(&grid, &ode, KvnRecipeLimits::default())?;
		let state = [
			Complex64::new(0.3, 0.2),
			Complex64::new(-0.1, 0.4),
			Complex64::new(0.5, -0.3),
		];
		let observable = [-1., 0., 1.];
		let q: f64 = state.iter().map(Complex64::norm_sqr).sum();
		let mut a = Accumulator::<1>::default();
		let mut comm = Complex64::new(0., 0.);
		for i in 0..3 {
			let action = (0..3).map(|j| -d[i][j] * state[j]).sum::<Complex64>();
			assert!((action - contract_row(&recipe, 0., i, &state)?).norm() < 1e-13);
			a.push(state[i], action, [observable[i]], [1.])?;
			for j in 0..3 {
				assert_eq!(d[i][j], -d[j][i]);
				comm += state[i].conj() * ((observable[i] - observable[j]) * (-d[i][j])) * state[j];
			}
		}
		assert!(comm.im.abs() < 1e-14);
		assert!((a.finish(q)?[0].normalized_rate - comm.re / q).abs() < 1e-13);
		// Non-skew test source deliberately gives qdot!=0: normalization must remove it.
		let mut b = Accumulator::<1>::default();
		b.push(Complex64::new(1., 1.), Complex64::new(1., 1.), [3.], [0.])?;
		let corrected = b.finish(2.)?[0];
		assert_eq!(b.qdot, 4.);
		assert_eq!(corrected.raw_skew_rate, 6.);
		assert_eq!(corrected.normalized_rate, 0.);
		Ok(())
	}
	#[test]
	fn complex_energy_shell_cancels_non_skew_norm_growth_but_not_nonconstant_energy()
	-> Result<(), CfdError> {
		// Independent integer complex matrix, deliberately NOT skew-Hermitian.
		// z=(1+i,2-i,0), Lz=(3-2i,1+4i,1+8i), q=7 and qdot=-2.
		let z = [
			Complex64::new(1., 1.),
			Complex64::new(2., -1.),
			Complex64::new(0., 0.),
		];
		let l = [
			[
				Complex64::new(1., 1.),
				Complex64::new(2., -1.),
				Complex64::new(-1., 0.),
			],
			[
				Complex64::new(0., -1.),
				Complex64::new(-1., 2.),
				Complex64::new(3., 1.),
			],
			[
				Complex64::new(4., 0.),
				Complex64::new(-2., 1.),
				Complex64::new(2., 0.),
			],
		];
		let exact_action = [
			Complex64::new(3., -2.),
			Complex64::new(1., 4.),
			Complex64::new(1., 8.),
		];
		let shell = [5. / 18., 5. / 18., 7.];
		let varied = [1., 3., 7.];
		let mut accumulator = Accumulator::<2>::default();
		for i in 0..3 {
			let action = (0..3).map(|j| l[i][j] * z[j]).sum::<Complex64>();
			assert_eq!(action, exact_action[i]);
			accumulator.push(z[i], action, [shell[i], varied[i]], [0., 0.])?;
		}
		assert_eq!(accumulator.qdot, -2.);
		let q: f64 = z.iter().map(Complex64::norm_sqr).sum();
		assert_eq!(q, 7.);
		let rates = accumulator.finish(q)?;
		assert!((rates[0].raw_skew_rate + 5. / 63.).abs() < 1e-14);
		assert!(rates[0].normalized_rate.abs() < 1e-14);
		// Direct quotient derivative: m=17,m_dot=-10,q=7,qdot=-2 -> -36/49.
		assert!((rates[1].expectation - 17. / 7.).abs() < 1e-14);
		assert!((rates[1].normalized_rate + 36. / 49.).abs() < 1e-14);
		assert!(rates[1].normalized_rate.abs() > 0.7);
		Ok(())
	}
	#[test]
	fn nonzero_exterior_trace_exhibits_periodic_coordinate_contamination() -> Result<(), CfdError> {
		let grid = ConfigurationGrid::uniform(1, -1., 1., 3, 2, 9)?;
		let ode = affine(1, 0)?;
		let recipe = KvnHistoryRecipe::new(&grid, &ode, KvnRecipeLimits::default())?;
		let state: Vec<_> = (0..9)
			.map(|i| Complex64::new(grid.axis_weight(i).unwrap_or(0.).sqrt(), 0.))
			.collect();
		let q: f64 = state.iter().map(Complex64::norm_sqr).sum();
		let mut a = Accumulator::<1>::default();
		for (i, &z) in state.iter().enumerate() {
			a.push(
				z,
				contract_row(&recipe, 0., i, &state)?,
				[grid.axis_node(i).ok_or_else(invalid)?],
				[1.],
			)?;
		}
		let r = a.finish(q)?[0];
		assert!(r.normalized_rate.abs() < 1e-13);
		assert!((r.physical_rate - 1.).abs() < 1e-14);
		assert!((r.absolute_defect - 1.).abs() < 1e-13);
		Ok(())
	}
	#[test]
	fn cartesian_moments_check_mass_chart_and_every_physical_coordinate() -> Result<(), CfdError> {
		let source = PeriodicWeakSource::prepare(0.01, 0, WeakLimits::default()).outcome?;
		let model = source.model();
		for i in 0..5 {
			for j in 0..5 {
				let actual = cartesian_inner(&model.chart()[i], &model.chart()[j]);
				assert!((actual - f64::from(i == j)).abs() < 1e-12);
			}
		}
		// Independent three-point triangle quadrature, exact for every affine product.
		let points = [
			[[1. / 3., 1. / 6.], [5. / 6., 1. / 6.], [5. / 6., 2. / 3.]],
			[[1. / 6., 1. / 3.], [2. / 3., 5. / 6.], [1. / 6., 5. / 6.]],
		];
		let a = model.coefficients(&[0.3, -0.2, 0.1, 0.4, 0.5])?;
		let b = model.coefficients(&[-0.1, 0.4, 0.2, -0.3, 0.7])?;
		let mut integral = 0.;
		for (cell, quadrature) in points.iter().enumerate() {
			for [x, y] in quadrature {
				for component in 0..2 {
					let k = 6 * cell + 3 * component;
					integral += (a[k] + a[k + 1] * x + a[k + 2] * y)
						* (b[k] + b[k + 1] * x + b[k + 2] * y)
						/ 6.;
				}
			}
		}
		assert!((integral - cartesian_inner(&a, &b)).abs() < 1e-13);
		Ok(())
	}
}
