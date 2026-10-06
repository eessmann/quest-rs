//! Fixed, bounded classical comparison of complete KvN and Carleman histories.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::too_many_lines,
	clippy::suboptimal_flops,
	reason = "Fixed five-coordinate shapes and checked phase admission precede bounded numerical loops"
)]
use crate::{
	CfdError, PeriodicBdm1,
	carleman::{CarlemanLimits, SymmetricCarleman},
	classical_history::{ReferenceBudget, solve_reference},
	configuration::ConfigurationGrid,
	history::{HistoryDynamics, HistorySystem},
	kvn_recipe::{KvnHistoryRecipe, KvnRecipeLimits},
	polynomial::PolynomialOde,
	stream_history::{HistoryRowDynamics, HistoryStreamLimits, TemporalHistoryRecipe},
	temporal_observation::{TemporalInterpolation, TemporalInterpolationLimits},
};
use quest_numerics::{Complex64, SparseLimits};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Instant};
const BYTES: usize = 268_435_456;
const WORK: usize = 10_000_000_000;
const CONSTRUCTOR: usize = 8 * 1024 * 1024;
const T: f64 = 0.01;
const CENTER: [f64; 5] = [0.04, -0.03, 0.02, 0.05, -0.01];
const fn invalid() -> CfdError {
	CfdError::InvalidInput("paired history fixed fixture, finite arithmetic or admission")
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(invalid)
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(invalid)
}
fn real(n: usize) -> Result<f64, CfdError> {
	Ok(f64::from(u32::try_from(n).map_err(|_| invalid())?))
}
const fn finite(x: f64) -> Result<f64, CfdError> {
	if x.is_finite() { Ok(x) } else { Err(invalid()) }
}
fn buffer<T: Clone>(n: usize, value: T) -> Result<Vec<T>, CfdError> {
	let mut v = Vec::new();
	v.try_reserve_exact(n).map_err(|_| invalid())?;
	v.resize(n, value);
	Ok(v)
}
fn payload<T>(v: &Vec<T>) -> Result<usize, CfdError> {
	mul(v.capacity(), size_of::<T>())
}
/// Fixed scientific comparison route; every physical coordinate remains present.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PairedLift {
	Kvn,
	Carleman { order: usize },
}
/// One externally timed subprocess row.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PairedHistoryRequest {
	EnsembleReference {
		steps: usize,
	},
	History {
		lift: PairedLift,
		time_cells: usize,
		time_order: usize,
	},
}
/// Component caps may be lowered; raising the frozen hard ceilings rejects.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct PairedHistoryLimits {
	pub max_bytes: usize,
	pub max_work: usize,
	pub max_source_work: usize,
	pub max_physical_work: usize,
	pub max_drift_calls: usize,
}
impl Default for PairedHistoryLimits {
	fn default() -> Self {
		Self {
			max_bytes: BYTES,
			max_work: WORK,
			max_source_work: 1_000_000_000,
			max_physical_work: 100_000_000_000,
			max_drift_calls: 1_000_000,
		}
	}
}
/// Common finite sampled probability ensemble, not a continuum regularization proof.
#[derive(Debug, Serialize)]
pub struct InitialEnsemble {
	pub sample_count: usize,
	pub support_count: usize,
	pub identity: String,
	pub coordinate_mean: [f64; 5],
	pub covariance_trace: f64,
	pub energy: f64,
	pub probability: f64,
	pub outer_occupation: f64,
	pub scale: f64,
	pub moment_reconstruction_error: Option<f64>,
}
/// Physical expectation at one explicit time, after amplitude interpolation for `KvN`.
#[derive(Debug, Serialize)]
pub struct PairedObservation {
	pub time: f64,
	pub slab: Option<usize>,
	pub side: String,
	pub coordinate_mean: [f64; 5],
	pub energy: f64,
	pub probability: Option<f64>,
	pub imaginary_residue: f64,
	pub interpolation_norm_upper: Option<f64>,
}
/// Completed reference evidence; quantum and convergence flags remain false.
#[derive(Debug, Serialize)]
pub struct PairedHistoryRow {
	pub schema: &'static str,
	pub request: PairedHistoryRequest,
	pub physical_dimension: usize,
	pub history_dimension: Option<usize>,
	pub initial: InitialEnsemble,
	pub observations: Vec<PairedObservation>,
	pub history_relative_residual: Option<f64>,
	pub modeled_peak_bytes: usize,
	pub history_reference_work: usize,
	/// Separate stored-assembly/interval-bound allowance, not part of query or solve work.
	pub history_assembly_work_allowance: usize,
	pub source_query_work: usize,
	pub physical_reference_work: usize,
	pub physical_drift_calls: usize,
	pub constructor_work_allowance: usize,
	pub extraction_probe_error: Option<f64>,
	pub nonlinear_initial_action: Option<f64>,
	pub limits: PairedHistoryLimits,
	pub elapsed_seconds: f64,
	pub quantum_execution: bool,
	pub convergence_certified: bool,
	pub truncation_evidence: &'static str,
}
fn check_request(
	request: PairedHistoryRequest,
	limits: PairedHistoryLimits,
) -> Result<Option<usize>, CfdError> {
	let hard = PairedHistoryLimits::default();
	if limits.max_bytes < CONSTRUCTOR
		|| limits.max_bytes > hard.max_bytes
		|| limits.max_work > hard.max_work
		|| limits.max_source_work > hard.max_source_work
		|| limits.max_physical_work > hard.max_physical_work
		|| limits.max_drift_calls > hard.max_drift_calls
	{
		return Err(invalid());
	}
	match request {
		PairedHistoryRequest::EnsembleReference { steps } => {
			if ![256, 512].contains(&steps) {
				return Err(invalid());
			}
			let calls = mul(mul(4, 243)?, steps)?;
			let work = add(mul(calls, 100_000)?, mul(243, mul(steps, 1024)?)?)?;
			if calls > limits.max_drift_calls || work > limits.max_physical_work {
				return Err(invalid());
			}
			Ok(None)
		}
		PairedHistoryRequest::History {
			lift,
			time_cells,
			time_order,
		} => {
			if ![(1, 1), (2, 1), (1, 2)].contains(&(time_cells, time_order)) {
				return Err(invalid());
			}
			let n = match lift {
				PairedLift::Kvn => 243,
				PairedLift::Carleman { order: 2 } => 20,
				PairedLift::Carleman { order: 3 } => 55,
				PairedLift::Carleman { order: 4 } => 125,
				PairedLift::Carleman { .. } => return Err(invalid()),
			};
			let total = mul(mul(n, time_cells)?, time_order + 1)?;
			if limits.max_drift_calls < 2048 || limits.max_physical_work < 204_800_000 {
				return Err(invalid());
			}
			if mul(mul(mul(total, total)?, total)?, 8)? > limits.max_work {
				return Err(invalid());
			}
			Ok(Some(total))
		}
	}
}
fn hash_word(hash: &mut u64, word: u64) {
	for byte in word.to_le_bytes() {
		*hash = (*hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
	}
}
struct Ensemble {
	grid: ConfigurationGrid,
	amplitudes: Vec<Complex64>,
	probabilities: Vec<f64>,
	initial: InitialEnsemble,
}
impl Ensemble {
	fn new(model: &PeriodicBdm1) -> Result<Self, CfdError> {
		if model.dimension() != 5 {
			return Err(invalid());
		}
		let grid = ConfigurationGrid::uniform(5, -0.2, 0.2, 1, 2, 243)?;
		let mut amplitudes = grid.initial_bump(&CENTER, 0.55)?;
		let norm = finite(amplitudes.iter().map(Complex64::norm_sqr).sum::<f64>())?.sqrt();
		if norm == 0. {
			return Err(invalid());
		}
		for a in &mut amplitudes {
			*a /= norm;
		}
		let mut probabilities = buffer(243, 0.)?;
		let mut mean = [0.; 5];
		let mut energy = 0.;
		let mut max_norm = 0_f64;
		let mut support = 0;
		let mut hash = 0xcbf2_9ce4_8422_2325;
		for x in [0.01_f64, -0.2, 0.2, 0.55, T].iter().chain(&CENTER) {
			hash_word(&mut hash, x.to_bits());
		}
		for q in model.chart() {
			for x in q {
				hash_word(&mut hash, x.to_bits());
			}
		}
		for (i, z) in amplitudes.iter().enumerate() {
			let point = grid.point(i).ok_or_else(invalid)?;
			let p = z.norm_sqr();
			probabilities[i] = p;
			for (&a, m) in point.iter().zip(&mut mean) {
				*m += p * a;
				hash_word(&mut hash, a.to_bits());
			}
			hash_word(&mut hash, p.to_bits());
			let e = model.energy(&point)?;
			energy += p * e;
			if p > 0. {
				support += 1;
				max_norm = max_norm.max((2. * e).sqrt());
			}
		}
		let mut variance = 0.;
		for (i, &p) in probabilities.iter().enumerate() {
			let point = grid.point(i).ok_or_else(invalid)?;
			for (&x, &m) in point.iter().zip(&mean) {
				variance += p * (x - m).powi(2);
			}
		}
		let initial = InitialEnsemble {
			sample_count: 243,
			support_count: support,
			identity: format!("fnv1a64:{hash:016x}"),
			coordinate_mean: mean,
			covariance_trace: finite(variance)?,
			energy: finite(energy)?,
			probability: finite(probabilities.iter().sum())?,
			outer_occupation: grid.boundary_mass(&amplitudes)?,
			scale: finite(2. * max_norm)?,
			moment_reconstruction_error: None,
		};
		if initial.scale <= 0. || support < 2 {
			return Err(invalid());
		}
		Ok(Self {
			grid,
			amplitudes,
			probabilities,
			initial,
		})
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		add(
			add(
				add(self.grid.retained_bytes()?, payload(&self.amplitudes)?)?,
				payload(&self.probabilities)?,
			)?,
			size_of::<Self>() + self.initial.identity.capacity(),
		)
	}
}
fn moments(h: &SymmetricCarleman, ensemble: &Ensemble) -> Result<Vec<Complex64>, CfdError> {
	let mut initial = buffer(h.dimension(), Complex64::new(0., 0.))?;
	for (i, &p) in ensemble.probabilities.iter().enumerate() {
		let point = ensemble.grid.point(i).ok_or_else(invalid)?;
		for (row, value) in initial.iter_mut().enumerate() {
			value.re += p * h.lift_entry(row, &point)?;
		}
	}
	Ok(initial)
}
fn hierarchy_observation(
	h: &SymmetricCarleman,
	values: &[Complex64],
) -> Result<([f64; 5], f64, f64), CfdError> {
	if values.len() != h.dimension() {
		return Err(invalid());
	}
	let mut mean = [0.; 5];
	let mut energy = 0.;
	let mut imag = 0_f64;
	for (alpha, value) in h.powers().iter().zip(values) {
		let degree: u32 = alpha.iter().sum();
		if degree == 1 {
			let axis = alpha.iter().position(|&p| p == 1).ok_or_else(invalid)?;
			mean[axis] = finite(h.scale() * value.re)?;
			imag = imag.max((h.scale() * value.im).abs());
		}
		if degree == 2 && alpha.contains(&2) {
			energy += 0.5 * h.scale() * h.scale() * value.re;
			imag = imag.max((0.5 * h.scale() * h.scale() * value.im).abs());
		}
	}
	if !imag.is_finite() || imag > 1e-10 {
		return Err(invalid());
	}
	Ok((mean, finite(energy)?, imag))
}
struct StoredAdapter<'a, D> {
	d: &'a D,
	external: usize,
}
impl<D: HistoryRowDynamics> HistoryDynamics for StoredAdapter<'_, D> {
	fn dimension(&self) -> usize {
		self.d.dimension()
	}
	fn max_generator_entries(&self) -> usize {
		self.d.dimension().saturating_mul(self.d.max_row_entries())
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		add(
			add(self.d.retained_bytes()?, self.d.row_query_bytes())?,
			self.external,
		)
	}
	fn visit_generator(
		&self,
		t: f64,
		f: &mut dyn FnMut(usize, usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		for row in 0..self.d.dimension() {
			self.d
				.visit_row(t, row, &mut |column, value| f(row, column, value))?;
		}
		Ok(())
	}
	fn source(&self, t: f64, output: &mut [Complex64]) -> Result<(), CfdError> {
		for (row, v) in output.iter_mut().enumerate() {
			*v = self.d.source_entry(t, row)?;
		}
		Ok(())
	}
}
fn reference_observations(
	model: &PeriodicBdm1,
	e: &Ensemble,
	steps: usize,
) -> Result<Vec<PairedObservation>, CfdError> {
	let mut means = [[0.; 5]; 2];
	let mut energies = [0.; 2];
	let dt = T / real(steps)?;
	for (i, &p) in e.probabilities.iter().enumerate() {
		let point = e.grid.point(i).ok_or_else(invalid)?;
		let middle = model.integrate_rk4(&point, dt, steps / 4)?;
		let end = model.integrate_rk4(&middle, dt, 3 * steps / 4)?;
		for (k, state) in [&middle, &end].iter().enumerate() {
			for (&x, m) in state.iter().zip(&mut means[k]) {
				*m += p * x;
			}
			energies[k] += p * model.energy(state)?;
		}
	}
	[T / 4., T]
		.into_iter()
		.enumerate()
		.map(|(k, time)| {
			for x in means[k] {
				finite(x)?;
			}
			Ok(PairedObservation {
				time,
				slab: None,
				side: "classical physical time".into(),
				coordinate_mean: means[k],
				energy: finite(energies[k])?,
				probability: None,
				imaginary_residue: 0.,
				interpolation_norm_upper: None,
			})
		})
		.collect()
}
// Each component is admitted before the diagnostic allocates. Physical drift calls
// are charged separately; these bounds cover assembly, sparse sorting and readout.
fn nonlinear_action_work(grid: &ConfigurationGrid) -> Result<(usize, usize, usize), CfdError> {
	let n = grid.dimension();
	let mut max_row = 0;
	for row in 0..grid.axis_dimension() {
		max_row = max_row.max(grid.axis_derivative_row(row).ok_or_else(invalid)?.len());
	}
	let samples = mul(n, grid.axes())?;
	let entries = mul(samples, max_row)?;
	let levels = add(
		usize::try_from(entries.checked_ilog2().unwrap_or(0)).map_err(|_| invalid())?,
		2,
	)?;
	let sorting = mul(
		2,
		add(mul(mul(entries, levels)?, 16)?, mul(add(n, 1)?, 8)?)?,
	)?;
	let assembly = add(mul(entries, 128)?, mul(samples, 64)?)?;
	let matvec = add(mul(entries, 8)?, mul(n, 2)?)?;
	let readout = mul(n, 32)?;
	let total = add(add(sorting, assembly)?, add(matvec, readout)?)?;
	Ok((entries, sorting.max(matvec), total))
}
fn nonlinear_action(
	e: &Ensemble,
	model: &PeriodicBdm1,
	limits: PairedHistoryLimits,
) -> Result<f64, CfdError> {
	// Independent original-force polarization. This is bounded reference assembly only.
	let (entries, sparse_work, diagnostic_work) = nonlinear_action_work(&e.grid)?;
	if diagnostic_work > limits.max_source_work {
		return Err(invalid());
	}
	let bound = mul(243, 3)?;
	if bound > limits.max_drift_calls || mul(bound, 100_000)? > limits.max_physical_work {
		return Err(invalid());
	}
	let zero = model.drift(&[0.; 5])?;
	let generator = e.grid.generator_from(
		5,
		|a| {
			let negative = a.iter().map(|x| -x).collect::<Vec<_>>();
			let plus = model.drift(a)?;
			let minus = model.drift(&negative)?;
			Ok(plus
				.iter()
				.zip(minus)
				.zip(&zero)
				.map(|((p, m), z)| 0.5 * (p + m - 2. * z))
				.collect())
		},
		SparseLimits {
			max_dimension: 243,
			max_entries: entries,
			max_bytes: CONSTRUCTOR,
			max_work: sparse_work,
		},
	)?;
	let action = generator.matvec(
		&e.amplitudes,
		SparseLimits {
			max_dimension: 243,
			max_entries: entries,
			max_bytes: CONSTRUCTOR,
			max_work: sparse_work,
		},
	)?;
	finite(action.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt())
}
fn source_work<D: HistoryRowDynamics>(
	d: &D,
	cells: usize,
	order: usize,
	initial_work: usize,
	limits: PairedHistoryLimits,
) -> Result<usize, CfdError> {
	let nodes = mul(cells, order + 1)?;
	let rows = mul(mul(mul(nodes, d.dimension())?, 2)?, d.row_query_work())?;
	let readout = mul(mul(2, d.dimension())?, mul(order + 2, 512)?)?;
	let work = add(add(rows, readout)?, initial_work)?;
	if work > limits.max_source_work {
		return Err(invalid());
	}
	Ok(work)
}
#[derive(Clone, Copy)]
enum ObservationSource<'a> {
	Kvn(&'a PeriodicBdm1),
	Carleman(&'a SymmetricCarleman),
}
fn history_observations<D: HistoryRowDynamics>(
	d: &D,
	initial: &[Complex64],
	e: &Ensemble,
	observation: ObservationSource<'_>,
	temporal: (usize, usize),
	limits: PairedHistoryLimits,
	initial_work: usize,
) -> Result<(Vec<PairedObservation>, f64, usize, usize, usize), CfdError> {
	let (cells, order) = temporal;
	let query_work = source_work(d, cells, order, initial_work, limits)?;
	// Constructor envelope bounds fixed source owners. Charge hierarchy/grid/ODE again
	// conservatively rather than subtract shared Arc contents from their public receipts.
	let external = add(
		CONSTRUCTOR,
		add(
			d.retained_bytes()?,
			add(
				mul(initial.len(), size_of::<Complex64>())?,
				d.row_query_bytes(),
			)?,
		)?,
	)?;
	let adapter = StoredAdapter {
		d,
		external: CONSTRUCTOR,
	};
	// Reserve the complete duplicate-triplet construction, not just the final CSR.
	// 512 bytes/contribution covers simultaneous COO, sort, compressed arrays and
	// iterator/index scratch; the nested constructor repeats the fixed envelope.
	let temporal_nodes = mul(cells, order + 1)?;
	let total = mul(d.dimension(), temporal_nodes)?;
	let entries = add(
		mul(total, order + 2)?,
		mul(temporal_nodes, mul(d.dimension(), d.max_row_entries())?)?,
	)?;
	let assembly_peak = add(
		add(external, CONSTRUCTOR)?,
		add(
			mul(entries, 512)?,
			add(mul(add(total, d.dimension())?, 128)?, 4096)?,
		)?,
	)?;
	if assembly_peak > limits.max_bytes {
		return Err(invalid());
	}
	let max_bytes = assembly_peak.checked_sub(CONSTRUCTOR).ok_or_else(invalid)?;
	let history = HistorySystem::assemble_dynamics(
		&adapter,
		initial,
		T,
		cells,
		order,
		SparseLimits {
			max_dimension: 972,
			max_entries: 200_000,
			max_bytes,
			max_work: 1_000_000_000,
		},
	)?;
	let reference = solve_reference(
		&history,
		ReferenceBudget {
			max_dimension: 972,
			max_bytes: limits.max_bytes.checked_sub(external).ok_or_else(invalid)?,
			max_work: limits.max_work,
			relative_residual: 1e-10,
		},
	)?;
	let peak = add(reference.modeled_peak_bytes, external)?.max(assembly_peak);
	let recipe = TemporalHistoryRecipe::new(
		d,
		T,
		cells,
		order,
		HistoryStreamLimits {
			max_dimension: 972,
			max_bytes: limits.max_bytes,
			..Default::default()
		},
	)?;
	let mut out = Vec::new();
	out.try_reserve_exact(2).map_err(|_| invalid())?;
	for (slab, fraction) in [(0, real(cells)? / 4.), (cells - 1, 1.)] {
		let interpolation = TemporalInterpolation::new(
			&recipe,
			slab,
			fraction,
			TemporalInterpolationLimits {
				amplitude_source_retained_bytes: add(
					external,
					add(history.retained_bytes()?, payload(&reference.solution)?)?,
				)?,
				max_bytes: limits.max_bytes,
				..Default::default()
			},
		)?;
		let mut amplitudes = buffer(d.dimension(), Complex64::new(0., 0.))?;
		for (i, v) in amplitudes.iter_mut().enumerate() {
			*v = interpolation.amplitude(i, |index| {
				reference.solution.get(index).copied().ok_or_else(invalid)
			})?;
		}
		let (mean, energy, imag, probability) = if let ObservationSource::Carleman(h) = observation
		{
			let (m, en, im) = hierarchy_observation(h, &amplitudes)?;
			(m, en, im, None)
		} else {
			let ObservationSource::Kvn(model) = observation else {
				return Err(invalid());
			};
			let o = e.grid.observables(&amplitudes, |a| model.energy(a))?;
			(
				o.coordinate_means.try_into().map_err(|_| invalid())?,
				o.mean_kinetic_energy,
				0.,
				Some(o.probability),
			)
		};
		out.push(PairedObservation {
			time: interpolation.physical_time(),
			slab: Some(slab),
			side: format!("{:?}", interpolation.side()),
			coordinate_mean: mean,
			energy,
			probability,
			imaginary_residue: imag,
			interpolation_norm_upper: Some(interpolation.norm_upper_bound()),
		});
	}
	Ok((
		out,
		reference.relative_residual,
		peak,
		reference.modeled_work,
		query_work,
	))
}
/// Execute one fixed complete-coordinate classical reference row.
///
/// No quantum measurement/execution or convergence certificate is produced.
/// # Errors
/// Rejects unsupported rows, finite-domain failures and any frozen budget violation.
pub fn run_paired_history_row(
	request: PairedHistoryRequest,
	limits: PairedHistoryLimits,
) -> Result<PairedHistoryRow, CfdError> {
	let dimension = check_request(request, limits)?;
	let started = Instant::now();
	let model = PeriodicBdm1::assemble(0.01)?;
	let mut ensemble = Ensemble::new(&model)?;
	if add(model.retained_bytes()?, ensemble.retained_bytes()?)? > CONSTRUCTOR {
		return Err(invalid());
	}
	let mut result = PairedHistoryRow {
		schema: "quest-cfd-paired-history-row-v1",
		request,
		physical_dimension: 5,
		history_dimension: dimension,
		initial: InitialEnsemble {
			sample_count: 0,
			support_count: 0,
			identity: String::new(),
			coordinate_mean: [0.; 5],
			covariance_trace: 0.,
			energy: 0.,
			probability: 0.,
			outer_occupation: 0.,
			scale: 0.,
			moment_reconstruction_error: None,
		},
		observations: Vec::new(),
		history_relative_residual: None,
		modeled_peak_bytes: CONSTRUCTOR,
		history_reference_work: 0,
		history_assembly_work_allowance: 0,
		source_query_work: 0,
		physical_reference_work: 0,
		physical_drift_calls: 0,
		constructor_work_allowance: 100_000_000,
		extraction_probe_error: None,
		nonlinear_initial_action: None,
		limits,
		elapsed_seconds: 0.,
		quantum_execution: false,
		convergence_certified: false,
		truncation_evidence: "ensemble hierarchy truncation unverified; compare independent order differences, not a single-trajectory certificate",
	};
	match request {
		PairedHistoryRequest::EnsembleReference { steps } => {
			result.observations = reference_observations(&model, &ensemble, steps)?;
			result.physical_drift_calls = 4 * 243 * steps;
			result.physical_reference_work =
				result.physical_drift_calls * 100_000 + 243 * steps * 1024;
		}
		PairedHistoryRequest::History {
			lift,
			time_cells,
			time_order,
		} => {
			result.history_assembly_work_allowance = 1_000_000_000;
			let snapshot = PolynomialOde::from_periodic_bdm1(
				&model,
				mathcore::multivariate::PolynomialLimits {
					max_variables: 6,
					max_terms: 256,
					max_degree: 4,
					max_coefficient_bits: 256,
					max_bytes: 1024 * 1024,
					max_work: 1_000_000,
				},
			)?;
			result.extraction_probe_error = Some(snapshot.evidence.independent_probe_max_error);
			result.physical_drift_calls = snapshot.evidence.residual_evaluations;
			let ode = Arc::new(snapshot.dynamics);
			if add(
				add(model.retained_bytes()?, ensemble.retained_bytes()?)?,
				ode.retained_bytes(),
			)? > CONSTRUCTOR
			{
				return Err(invalid());
			}
			let (observations, residual, peak, work, source_work) = match lift {
				PairedLift::Kvn => {
					let recipe =
						KvnHistoryRecipe::new(&ensemble.grid, &ode, KvnRecipeLimits::default())?;
					let (_, _, diagnostic_work) = nonlinear_action_work(&ensemble.grid)?;
					source_work(&recipe, time_cells, time_order, diagnostic_work, limits)?;
					result.nonlinear_initial_action =
						Some(nonlinear_action(&ensemble, &model, limits)?);
					if result.nonlinear_initial_action.is_none_or(|x| x <= 1e-12) {
						return Err(invalid());
					}
					result.physical_drift_calls += 487;
					history_observations(
						&recipe,
						&ensemble.amplitudes,
						&ensemble,
						ObservationSource::Kvn(&model),
						(time_cells, time_order),
						limits,
						diagnostic_work,
					)?
				}
				PairedLift::Carleman { order } => {
					let hierarchy = SymmetricCarleman::new(
						ode,
						order,
						ensemble.initial.scale,
						CarlemanLimits {
							max_dimension: 125,
							max_order: 4,
							max_entries: 50_000,
							max_bytes: limits.max_bytes - CONSTRUCTOR,
							max_work: 100_000_000,
						},
					)?;
					let moment_work = mul(
						mul(243, hierarchy.dimension())?,
						add(256, mul(160, order)?)?,
					)?;
					source_work(&hierarchy, time_cells, time_order, moment_work, limits)?;
					let initial = moments(&hierarchy, &ensemble)?;
					if add(
						add(
							add(model.retained_bytes()?, ensemble.retained_bytes()?)?,
							payload(&initial)?,
						)?,
						4096,
					)? > CONSTRUCTOR
					{
						return Err(invalid());
					}
					let (mean, energy, _) = hierarchy_observation(&hierarchy, &initial)?;
					ensemble.initial.moment_reconstruction_error = Some(
						mean.iter()
							.zip(ensemble.initial.coordinate_mean)
							.map(|(a, b)| (a - b).abs())
							.fold((energy - ensemble.initial.energy).abs(), f64::max),
					);
					history_observations(
						&hierarchy,
						&initial,
						&ensemble,
						ObservationSource::Carleman(&hierarchy),
						(time_cells, time_order),
						limits,
						moment_work,
					)?
				}
			};
			result.observations = observations;
			result.history_relative_residual = Some(residual);
			result.modeled_peak_bytes = peak;
			result.history_reference_work = work;
			result.source_query_work = source_work;
			result.physical_reference_work = mul(result.physical_drift_calls, 100_000)?;
			if result.physical_drift_calls > limits.max_drift_calls
				|| result.physical_reference_work > limits.max_physical_work
			{
				return Err(invalid());
			}
		}
	}
	result.initial = ensemble.initial;
	result.elapsed_seconds = started.elapsed().as_secs_f64();
	Ok(result)
}

#[cfg(test)]
#[allow(
	clippy::panic_in_result_fn,
	reason = "Independent fixture identities intentionally fail tests by assertion"
)]
mod tests {
	use super::*;
	#[test]
	fn independent_nonlinear_action_fits_declared_source_budget() -> Result<(), CfdError> {
		let model = PeriodicBdm1::assemble(0.01)?;
		let ensemble = Ensemble::new(&model)?;
		let action = nonlinear_action(&ensemble, &model, PairedHistoryLimits::default())?;
		assert!(action > 1e-12);
		assert!(
			nonlinear_action(
				&ensemble,
				&model,
				PairedHistoryLimits {
					max_source_work: 1,
					..Default::default()
				}
			)
			.is_err()
		);
		Ok(())
	}
	#[test]
	fn opposite_full_states_preserve_second_and_cross_moments() -> Result<(), CfdError> {
		let model = PeriodicBdm1::assemble(0.01)?;
		let mut ensemble = Ensemble::new(&model)?;
		ensemble.probabilities.fill(0.);
		ensemble.probabilities[0] = 0.5;
		ensemble.probabilities[242] = 0.5;
		let ode = Arc::new(
			PolynomialOde::from_periodic_bdm1(
				&model,
				mathcore::multivariate::PolynomialLimits::default(),
			)?
			.dynamics,
		);
		let h = SymmetricCarleman::new(ode, 2, 1., CarlemanLimits::default())?;
		let initial = moments(&h, &ensemble)?;
		let (mean, energy, imag) = hierarchy_observation(&h, &initial)?;
		assert!(mean.iter().all(|x| x.abs() < 1e-15));
		assert!((energy - 0.1).abs() < 1e-15);
		assert!(imag < 1e-15);
		let cross = h
			.powers()
			.iter()
			.position(|p| p == &[1, 1, 0, 0, 0])
			.ok_or_else(invalid)?;
		assert!((initial[cross].re - 0.04 * 2_f64.sqrt()).abs() < 1e-15);
		Ok(())
	}
	#[test]
	fn fixed_counts_and_caps_are_checked_before_work() -> Result<(), CfdError> {
		for (lift, sizes) in [
			(PairedLift::Kvn, [486, 972, 729]),
			(PairedLift::Carleman { order: 2 }, [40, 80, 60]),
			(PairedLift::Carleman { order: 3 }, [110, 220, 165]),
			(PairedLift::Carleman { order: 4 }, [250, 500, 375]),
		] {
			for ((time_cells, time_order), n) in [(1, 1), (2, 1), (1, 2)].into_iter().zip(sizes) {
				assert_eq!(
					check_request(
						PairedHistoryRequest::History {
							lift,
							time_cells,
							time_order
						},
						PairedHistoryLimits::default()
					)?,
					Some(n)
				);
			}
		}
		assert!(
			check_request(
				PairedHistoryRequest::EnsembleReference { steps: 512 },
				PairedHistoryLimits {
					max_drift_calls: 1,
					..Default::default()
				}
			)
			.is_err()
		);
		assert!(
			check_request(
				PairedHistoryRequest::History {
					lift: PairedLift::Kvn,
					time_cells: 2,
					time_order: 1
				},
				PairedHistoryLimits {
					max_work: 7_346_640_383,
					..Default::default()
				}
			)
			.is_err()
		);
		Ok(())
	}
}
