//! Shared fixed initial weak-generator producer, schema and complete admission ledger.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Fixed five-coordinate finite inputs, checked allocation/work preflight and finite output validation"
)]
use quest_cfd::{
	CfdError,
	configuration::ConfigurationGrid,
	configuration_diagnostics::{concentration, regularization_resolution},
	configuration_weak::{PeriodicWeakSource, WeakLimits, WeakReport, WeakRequest, WeakResources},
};
use quest_numerics::Complex64;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;
const CENTER: [f64; 5] = [0.15, -0.1, 0.07, 0.11, -0.04];
const SERIALIZER: usize = 65_536;
const HELPER: usize = 65_536;
const GRID_ENVELOPE: usize = 65_536;
const ROWS: [&str; 7] = [
	"p1-c3-e1-w1_2",
	"p1-c4-e1-w1_2",
	"p1-c5-e1-w1_2",
	"p2-c1-e1-w1_2",
	"p2-c3-e1-w1_2",
	"p1-c5-e5_3-w1_2",
	"p1-c3-e1-w3_5",
];
/// Explicit entry protocols; the historical seven-row set never grows implicitly.
#[allow(
	dead_code,
	reason = "Two independent example binaries instantiate different protocol variants"
)]
#[derive(Clone, Copy)]
pub enum Protocol {
	SevenRows,
	EnergyShellV2,
}
const ENERGY_ROW: &str = "p2-c2-e1-w1_2";
fn protocol_request(protocol: Protocol, id: &str) -> Result<Request, CfdError> {
	match protocol {
		Protocol::SevenRows => request(id),
		Protocol::EnergyShellV2 if id == ENERGY_ROW => {
			let mut row = request(ROWS[0])?;
			row.row_id = ENERGY_ROW;
			row.order = 2;
			row.cells = 2;
			Ok(row)
		}
		Protocol::EnergyShellV2 => Err(invalid()),
	}
}
#[derive(Clone, Copy, Serialize)]
struct Request {
	row_id: &'static str,
	order: usize,
	cells: usize,
	extent: f64,
	width: f64,
	center: [f64; 5],
	viscosity: f64,
	time: f64,
	physical_dimension: usize,
	minimum_support_samples: usize,
	nonlinear_witness: bool,
}
fn request(id: &str) -> Result<Request, CfdError> {
	let index = ROWS.iter().position(|x| *x == id).ok_or_else(invalid)?;
	let (order, cells, extent, width) = match index {
		0 => (1, 3, 1., 0.5),
		1 => (1, 4, 1., 0.5),
		2 => (1, 5, 1., 0.5),
		3 => (2, 1, 1., 0.5),
		4 => (2, 3, 1., 0.5),
		5 => (1, 5, 5_f64 / 3., 0.5),
		_ => (1, 3, 1., 3_f64 / 5.),
	};
	Ok(Request {
		row_id: ROWS[index],
		order,
		cells,
		extent,
		width,
		center: CENTER,
		viscosity: 0.01,
		time: 0.,
		physical_dimension: 5,
		minimum_support_samples: 2,
		nonlinear_witness: true,
	})
}
const fn invalid() -> CfdError {
	CfdError::InvalidInput("fixed weak protocol admission or metadata")
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(invalid)
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(invalid)
}
fn budget(bytes: usize, work: usize) -> Result<(), CfdError> {
	let l = WeakLimits::default();
	if bytes > l.max_bytes || work > l.max_source_work {
		Err(invalid())
	} else {
		Ok(())
	}
}
#[derive(Serialize)]
struct Plan {
	axis_dimension: usize,
	dimension: usize,
	input_work: usize,
	state_payload_bytes: usize,
}
fn plan(r: Request) -> Result<Plan, CfdError> {
	let n = mul(r.cells, add(r.order, 1)?)?;
	let dimension = n.checked_pow(5).ok_or_else(invalid)?;
	if dimension > 100_000 {
		return Err(invalid());
	}
	let input_work = add(1_048_576, add(mul(16_384, n)?, mul(4096, dimension)?)?)?;
	let state_payload_bytes = mul(16, dimension)?;
	budget(
		add(
			add(SERIALIZER, HELPER)?,
			add(GRID_ENVELOPE, state_payload_bytes)?,
		)?,
		add(100_000_000, input_work)?,
	)?;
	Ok(Plan {
		axis_dimension: n,
		dimension,
		input_work,
		state_payload_bytes,
	})
}
#[derive(Serialize, Default)]
struct InputResources {
	input_work: usize,
	peak_bytes: usize,
	grid_bytes: usize,
	state_capacity_bytes: usize,
	support_capacity_bytes: usize,
	metadata_capacity_bytes: usize,
	statistics_capacity_bytes: usize,
	declared_source_external_bytes: usize,
	declared_query_extra_bytes: usize,
	serializer_bytes: usize,
	helper_bytes: usize,
}
#[derive(Serialize)]
struct Sampling {
	distinct_samples: [usize; 5],
	minimum_required: usize,
	maximum_distinct_spacing: f64,
	width_per_spacing: f64,
	support_margin: f64,
	support_intersects_boundary: bool,
	policy_passed: bool,
	convergence_certified: bool,
}
#[derive(Serialize)]
struct GridRecord {
	axis_dimension: usize,
	dimension: usize,
	nodes: Vec<f64>,
	weights: Vec<f64>,
	node_bits: Vec<u64>,
	weight_bits: Vec<u64>,
	lower_bits: u64,
	upper_bits: u64,
	spacing_bits: u64,
	grid_sha256: String,
	derivative_sha256: String,
}
#[derive(Serialize)]
struct Initial {
	probability: f64,
	coordinate_means: [f64; 5],
	coordinate_variances: [f64; 5],
	mean_minus_continuum_center: [f64; 5],
	standard_deviations_per_spacing: [f64; 5],
	maximum_coefficient_probability: f64,
	effective_coefficients: f64,
	outer_cell_occupation: f64,
	zero_exterior_trace: bool,
	supported_coefficients: usize,
	ensemble_sha256: String,
}
#[allow(
	clippy::struct_field_names,
	reason = "Wire names match the existing fixed diagnostic limit contract"
)]
#[derive(Serialize)]
struct Limits {
	max_bytes: usize,
	max_source_work: usize,
	max_physical_work: usize,
	max_physical_calls: usize,
}
impl Default for Limits {
	fn default() -> Self {
		let l = WeakLimits::default();
		Self {
			max_bytes: l.max_bytes,
			max_source_work: l.max_source_work,
			max_physical_work: l.max_physical_work,
			max_physical_calls: l.max_physical_calls,
		}
	}
}
#[derive(Serialize)]
struct Resources {
	source_work: usize,
	caller_declared_input_preparation_work: Option<usize>,
	physical_work: usize,
	physical_calls: usize,
	physical_calls_attempted: usize,
	constructor_peak_bytes: usize,
	retained_source_bytes: usize,
	grid_bytes: usize,
	accessible_state_bytes: usize,
	external_bytes: usize,
	result_bytes: usize,
	scratch_bytes: usize,
	peak_bytes: usize,
	supported_rows: usize,
	row_query_work: usize,
	maximum_row_entries: usize,
}
impl From<WeakResources> for Resources {
	fn from(r: WeakResources) -> Self {
		Self {
			source_work: r.source_work,
			caller_declared_input_preparation_work: r.caller_declared_input_preparation_work,
			physical_work: r.physical_work,
			physical_calls: r.physical_calls,
			physical_calls_attempted: r.physical_calls_attempted,
			constructor_peak_bytes: r.constructor_peak_bytes,
			retained_source_bytes: r.retained_source_bytes,
			grid_bytes: r.grid_bytes,
			accessible_state_bytes: r.accessible_state_bytes,
			external_bytes: r.external_bytes,
			result_bytes: r.result_bytes,
			scratch_bytes: r.scratch_bytes,
			peak_bytes: r.peak_bytes,
			supported_rows: r.supported_rows,
			row_query_work: r.row_query_work,
			maximum_row_entries: r.maximum_row_entries,
		}
	}
}
#[derive(Serialize)]
struct Evidence {
	physical_recipe_sha256: String,
	chart_mass_residual: f64,
	constraint_residual: f64,
	extraction_probe_error: f64,
	extraction_scaled_error: f64,
	coefficient_roundoff_estimate: f64,
	residual_evaluations: usize,
	resources: Resources,
}
#[derive(Serialize)]
struct Diagnostic {
	source_identity: u64,
	time: f64,
	dimension: usize,
	probability: f64,
	probability_rate: f64,
	expectation: [f64; 6],
	raw_skew_rate: [f64; 6],
	normalized_rate: [f64; 6],
	physical_rate: [f64; 6],
	absolute_defect: [f64; 6],
	scaled_defect: [f64; 6],
	zero_exterior_trace: bool,
	outer_cell_occupation: f64,
	coordinate_standard_deviations: [f64; 5],
	maximum_coefficient_probability: f64,
	effective_coefficients: f64,
	nonlinear_mean_square: Option<f64>,
	cartesian_energy_discrepancy: f64,
	chart_mass_residual: f64,
	extraction_probe_error: f64,
	convergence_certified: bool,
}
impl From<WeakReport> for Diagnostic {
	fn from(r: WeakReport) -> Self {
		Self {
			source_identity: r.source_identity,
			time: r.time,
			dimension: r.dimension,
			probability: r.probability,
			probability_rate: r.probability_rate,
			expectation: r.rates.map(|x| x.expectation),
			raw_skew_rate: r.rates.map(|x| x.raw_skew_rate),
			normalized_rate: r.rates.map(|x| x.normalized_rate),
			physical_rate: r.rates.map(|x| x.physical_rate),
			absolute_defect: r.rates.map(|x| x.absolute_defect),
			scaled_defect: r.rates.map(|x| x.scaled_defect),
			zero_exterior_trace: r.zero_exterior_trace,
			outer_cell_occupation: r.outer_cell_occupation,
			coordinate_standard_deviations: r.coordinate_standard_deviations,
			maximum_coefficient_probability: r.maximum_coefficient_probability,
			effective_coefficients: r.effective_coefficients,
			nonlinear_mean_square: r.nonlinear_mean_square,
			cartesian_energy_discrepancy: r.cartesian_energy_discrepancy,
			chart_mass_residual: r.chart_mass_residual,
			extraction_probe_error: r.extraction_probe_error,
			convergence_certified: r.convergence_certified,
		}
	}
}
#[derive(Serialize)]
struct Output {
	schema: &'static str,
	status: &'static str,
	phase: &'static str,
	error: Option<&'static str>,
	request: Request,
	limits: Limits,
	plan: Option<Plan>,
	input_resources: InputResources,
	sampling: Option<Sampling>,
	represented_grid: Option<GridRecord>,
	initial: Option<Initial>,
	source: Option<Evidence>,
	diagnostic_resources: Option<Resources>,
	validated_rows: usize,
	visited_rows: usize,
	diagnostic: Option<Diagnostic>,
	quantum_execution: bool,
	history_execution: bool,
	convergence_certified: bool,
}
fn word(hash: &mut Sha256, x: u64) {
	hash.update(x.to_le_bytes());
}
fn count(hash: &mut Sha256, x: usize) -> Result<(), CfdError> {
	word(hash, u64::try_from(x).map_err(|_| invalid())?);
	Ok(())
}
fn tag(hash: &mut Sha256, text: &str) -> Result<(), CfdError> {
	count(hash, text.len())?;
	hash.update(text.as_bytes());
	Ok(())
}
fn hex(hash: Sha256) -> String {
	let mut text = String::with_capacity(64);
	let digits = b"0123456789abcdef";
	for byte in hash.finalize() {
		text.push(char::from(digits[usize::from(byte >> 4)]));
		text.push(char::from(digits[usize::from(byte & 15)]));
	}
	text
}
fn capacity<T>(v: &Vec<T>) -> Result<usize, CfdError> {
	mul(v.capacity(), size_of::<T>())
}
fn grid_record(grid: &ConfigurationGrid, r: Request) -> Result<GridRecord, CfdError> {
	let n = grid.axis_dimension();
	let mut nodes = Vec::new();
	let mut weights = Vec::new();
	let mut node_bits = Vec::new();
	let mut weight_bits = Vec::new();
	nodes.try_reserve_exact(n).map_err(|_| invalid())?;
	weights.try_reserve_exact(n).map_err(|_| invalid())?;
	node_bits.try_reserve_exact(n).map_err(|_| invalid())?;
	weight_bits.try_reserve_exact(n).map_err(|_| invalid())?;
	let mut hash = Sha256::new();
	tag(&mut hash, "quest-weak-grid-v1")?;
	for x in [5, r.order, r.cells, n, grid.dimension()] {
		count(&mut hash, x)?;
	}
	for x in [-r.extent, r.extent] {
		word(&mut hash, x.to_bits());
	}
	tag(&mut hash, "axis")?;
	count(&mut hash, n)?;
	let mut derivative = Sha256::new();
	tag(&mut derivative, "quest-weak-derivative-v1")?;
	count(&mut derivative, n)?;
	for i in 0..n {
		let x = grid.axis_node(i).ok_or_else(invalid)?;
		let w = grid.axis_weight(i).ok_or_else(invalid)?;
		nodes.push(x);
		weights.push(w);
		node_bits.push(x.to_bits());
		weight_bits.push(w.to_bits());
		word(&mut hash, x.to_bits());
		word(&mut hash, w.to_bits());
		let row = grid.axis_derivative_row(i).ok_or_else(invalid)?;
		count(&mut derivative, row.len())?;
		for &(j, d) in row {
			count(&mut derivative, j)?;
			word(&mut derivative, d.to_bits());
		}
	}
	let digest = derivative.clone().finalize();
	tag(&mut hash, "derivative-sha256")?;
	hash.update(digest);
	let cells = f64::from(u32::try_from(r.cells).map_err(|_| invalid())?);
	Ok(GridRecord {
		axis_dimension: n,
		dimension: grid.dimension(),
		nodes,
		weights,
		node_bits,
		weight_bits,
		lower_bits: (-r.extent).to_bits(),
		upper_bits: r.extent.to_bits(),
		spacing_bits: (2. * r.extent / cells).to_bits(),
		grid_sha256: hex(hash),
		derivative_sha256: hex(derivative),
	})
}
#[allow(
	clippy::float_cmp,
	reason = "Exact repeated physical endpoint locations define distinct samples; no near-node merging"
)]
fn sampling(grid: &ConfigurationGrid, r: Request) -> Result<Sampling, CfdError> {
	let mut counts = [0; 5];
	let mut previous = None;
	let mut spacing = 0_f64;
	for i in 0..grid.axis_dimension() {
		let x = grid.axis_node(i).ok_or_else(invalid)?;
		if previous == Some(x) {
			continue;
		}
		if let Some(p) = previous {
			spacing = spacing.max(x - p);
		}
		previous = Some(x);
		for (j, &c) in r.center.iter().enumerate() {
			if ((x - c) / r.width).abs() < 1. {
				counts[j] += 1;
			}
		}
	}
	if !spacing.is_finite() || spacing <= 0. {
		return Err(invalid());
	}
	let margin = r
		.center
		.iter()
		.map(|c| r.extent - c.abs() - r.width)
		.fold(f64::INFINITY, f64::min);
	Ok(Sampling {
		distinct_samples: counts,
		minimum_required: 2,
		maximum_distinct_spacing: spacing,
		width_per_spacing: r.width / spacing,
		support_margin: margin,
		support_intersects_boundary: margin < 0.,
		policy_passed: false,
		convergence_certified: false,
	})
}
fn ensemble_sha(
	grid: &ConfigurationGrid,
	state: &[Complex64],
	r: Request,
	grid_sha: &str,
) -> Result<String, CfdError> {
	let mut hash = Sha256::new();
	tag(&mut hash, "quest-weak-ensemble-v1")?;
	tag(&mut hash, grid_sha)?;
	count(&mut hash, 5)?;
	for c in r.center {
		word(&mut hash, c.to_bits());
	}
	word(&mut hash, r.width.to_bits());
	count(&mut hash, grid.dimension())?;
	for z in state {
		word(&mut hash, z.re.to_bits());
		word(&mut hash, z.im.to_bits());
	}
	Ok(hex(hash))
}
fn physical_sha(source: &PeriodicWeakSource) -> Result<String, CfdError> {
	let mut hash = Sha256::new();
	tag(&mut hash, "quest-weak-physical-v1")?;
	word(&mut hash, 0.01_f64.to_bits());
	count(&mut hash, 5)?;
	count(&mut hash, 12)?;
	for col in source.model().chart() {
		for x in col {
			word(&mut hash, x.to_bits());
		}
	}
	Ok(hex(hash))
}
fn initial(
	grid: &ConfigurationGrid,
	state: &[Complex64],
	r: Request,
	grid_sha: &str,
) -> Result<(Initial, usize), CfdError> {
	let moments = grid.observables(state, |_| Ok(0.))?;
	let concentration = concentration(grid, state, None)?;
	let means: [f64; 5] = moments
		.coordinate_means
		.as_slice()
		.try_into()
		.map_err(|_| invalid())?;
	let variances = moments
		.coordinate_variances
		.as_slice()
		.try_into()
		.map_err(|_| invalid())?;
	let widths = concentration
		.standard_deviations_per_spacing
		.as_slice()
		.try_into()
		.map_err(|_| invalid())?;
	let bytes = add(
		add(
			capacity(&moments.coordinate_means)?,
			capacity(&moments.coordinate_variances)?,
		)?,
		capacity(&concentration.standard_deviations_per_spacing)?,
	)?;
	let mut support = 0;
	let mut trace = true;
	for (i, z) in state.iter().enumerate() {
		if z.re != 0. || z.im != 0. {
			support += 1;
			let mut index = i;
			for _ in 0..5 {
				let a = index % grid.axis_dimension();
				index /= grid.axis_dimension();
				if a == 0 || a == grid.axis_dimension() - 1 {
					trace = false;
				}
			}
		}
	}
	Ok((
		Initial {
			probability: moments.probability,
			coordinate_means: means,
			coordinate_variances: variances,
			mean_minus_continuum_center: std::array::from_fn(|i| means[i] - r.center[i]),
			standard_deviations_per_spacing: widths,
			maximum_coefficient_probability: concentration.maximum_nodal_probability,
			effective_coefficients: concentration.effective_nodal_coefficients,
			outer_cell_occupation: concentration.boundary_occupation_fraction,
			zero_exterior_trace: trace,
			supported_coefficients: support,
			ensemble_sha256: ensemble_sha(grid, state, r, grid_sha)?,
		},
		bytes,
	))
}
fn blank(r: Request) -> Output {
	Output {
		schema: "quest-configuration-weak-row-v1",
		status: "construction-rejected",
		phase: "input admission",
		error: None,
		request: r,
		limits: Limits::default(),
		plan: None,
		input_resources: InputResources {
			serializer_bytes: SERIALIZER,
			helper_bytes: HELPER,
			..InputResources::default()
		},
		sampling: None,
		represented_grid: None,
		initial: None,
		source: None,
		diagnostic_resources: None,
		validated_rows: 0,
		visited_rows: 0,
		diagnostic: None,
		quantum_execution: false,
		history_execution: false,
		convergence_certified: false,
	}
}
#[allow(
	clippy::too_many_lines,
	reason = "One ordered producer ledger preserves all partial phase receipts before allocation and diagnostic admission"
)]
fn execute(out: &mut Output) -> Result<(), CfdError> {
	let r = out.request;
	let planned = plan(r)?;
	out.input_resources.input_work = planned.input_work;
	out.plan = Some(planned);
	let p = out.plan.as_ref().ok_or_else(invalid)?;
	let fixed = add(SERIALIZER, HELPER)?;
	out.input_resources.peak_bytes = add(fixed, GRID_ENVELOPE)?;
	budget(out.input_resources.peak_bytes, p.input_work)?;
	out.phase = "grid";
	#[cfg(test)]
	if FAIL_GRID.with(std::cell::Cell::get) {
		return Err(invalid());
	}
	let grid = ConfigurationGrid::uniform(5, -r.extent, r.extent, r.cells, r.order, p.dimension)?;
	out.input_resources.grid_bytes = grid.retained_bytes()?;
	let record = grid_record(&grid, r)?;
	let metadata = add(
		add(capacity(&record.nodes)?, capacity(&record.weights)?)?,
		add(capacity(&record.node_bits)?, capacity(&record.weight_bits)?)?,
	)?;
	out.input_resources.metadata_capacity_bytes = add(
		metadata,
		add(
			record.grid_sha256.capacity(),
			record.derivative_sha256.capacity(),
		)?,
	)?;
	let sample = sampling(&grid, r)?;
	out.represented_grid = Some(record);
	out.sampling = Some(sample);
	if add(
		out.input_resources.grid_bytes,
		out.input_resources.metadata_capacity_bytes,
	)? > GRID_ENVELOPE
	{
		return Err(invalid());
	}
	out.phase = "sampling policy";
	match regularization_resolution(&grid, &r.center, r.width, 2) {
		Ok(resolution) => {
			out.input_resources.support_capacity_bytes =
				capacity(&resolution.distinct_samples_in_support)?;
			out.sampling.as_mut().ok_or_else(invalid)?.policy_passed = true;
		}
		Err(error) => {
			out.status = "sampling-rejected";
			return Err(error);
		}
	}
	out.phase = "source construction";
	let external = add(
		fixed,
		add(
			out.input_resources.grid_bytes,
			add(
				out.input_resources.metadata_capacity_bytes,
				out.input_resources.support_capacity_bytes,
			)?,
		)?,
	)?;
	out.input_resources.declared_source_external_bytes = external;
	budget(
		add(8 * 1024 * 1024, external)?,
		add(100_000_000, p.input_work)?,
	)?;
	let prepared = PeriodicWeakSource::prepare(r.viscosity, external, WeakLimits::default());
	out.input_resources.peak_bytes = out
		.input_resources
		.peak_bytes
		.max(prepared.resources.peak_bytes);
	out.diagnostic_resources = Some(prepared.resources.into());
	let source = prepared.outcome?;
	let evidence = source.extraction_evidence();
	out.source = Some(Evidence {
		physical_recipe_sha256: physical_sha(&source)?,
		chart_mass_residual: source.model().diagnostics().mass_orthogonality_residual,
		constraint_residual: source.model().diagnostics().constraint_residual,
		extraction_probe_error: evidence.independent_probe_max_error,
		extraction_scaled_error: evidence.independent_probe_scaled_error,
		coefficient_roundoff_estimate: evidence.coefficient_roundoff_scale_estimate,
		residual_evaluations: evidence.residual_evaluations,
		resources: source.resources().into(),
	});
	out.phase = "state construction";
	let state_peak = add(
		add(source.resources().retained_source_bytes, external)?,
		add(p.state_payload_bytes, HELPER)?,
	)?;
	out.input_resources.peak_bytes = out.input_resources.peak_bytes.max(state_peak);
	budget(
		out.input_resources.peak_bytes,
		add(100_000_000, p.input_work)?,
	)?;
	let state = grid.initial_bump(&r.center, r.width)?;
	out.input_resources.state_capacity_bytes = capacity(&state)?;
	let hidden = out
		.input_resources
		.state_capacity_bytes
		.checked_sub(p.state_payload_bytes)
		.ok_or_else(invalid)?;
	out.input_resources.declared_query_extra_bytes = hidden;
	out.input_resources.peak_bytes = out.input_resources.peak_bytes.max(add(state_peak, hidden)?);
	budget(
		out.input_resources.peak_bytes,
		add(100_000_000, p.input_work)?,
	)?;
	out.phase = "input diagnostics";
	let (sampled, bytes) = initial(
		&grid,
		&state,
		r,
		&out.represented_grid
			.as_ref()
			.ok_or_else(invalid)?
			.grid_sha256,
	)?;
	out.input_resources.statistics_capacity_bytes = bytes;
	if add(bytes, sampled.ensemble_sha256.capacity())? > HELPER {
		return Err(invalid());
	}
	out.initial = Some(sampled);
	out.phase = "diagnostic";
	let attempt = source.diagnose(
		&grid,
		&state,
		WeakRequest {
			time: 0.,
			additional_external_bytes: hidden,
			caller_declared_input_preparation_work: Some(p.input_work),
			nonlinear_witness: true,
		},
		WeakLimits::default(),
	);
	out.phase = attempt.phase;
	out.validated_rows = attempt.validated_rows;
	out.visited_rows = attempt.visited_rows;
	out.input_resources.peak_bytes = out
		.input_resources
		.peak_bytes
		.max(attempt.resources.peak_bytes);
	out.diagnostic_resources = Some(attempt.resources.into());
	match attempt.outcome {
		Ok(report) => {
			out.diagnostic = Some(report.into());
			out.status = "completed";
			Ok(())
		}
		Err(error) => {
			out.status = failure_status(attempt.phase);
			Err(error)
		}
	}
}
fn failure_status(phase: &str) -> &'static str {
	if phase == "contraction" {
		"numerical-failure"
	} else {
		"diagnostic-rejected"
	}
}
fn run(r: Request) -> Output {
	let mut out = blank(r);
	if let Err(error) = execute(&mut out) {
		eprintln!("{error}");
		out.error = Some("see retained phase, resource receipt and bounded stderr");
	}
	out
}
#[allow(
	clippy::large_stack_arrays,
	reason = "Fixed serializer storage is predeclared in every phase's external bytes"
)]
fn emit(out: &Output) -> Result<(), Box<dyn std::error::Error>> {
	let mut bytes = [0_u8; SERIALIZER];
	let length = encode(out, &mut bytes)?;
	std::io::stdout()
		.lock()
		.write_all(bytes.get(..length).ok_or("serializer range")?)?;
	println!();
	Ok(())
}
fn encode(out: &Output, bytes: &mut [u8]) -> Result<usize, Box<dyn std::error::Error>> {
	let mut cursor = std::io::Cursor::new(bytes);
	serde_json::to_writer_pretty(&mut cursor, out)?;
	Ok(usize::try_from(cursor.position())?)
}
/// Execute exactly one explicitly selected protocol row.
/// # Errors
/// Rejects unsupported arguments, serializer exhaustion or output I/O failure.
pub fn main_entry(protocol: Protocol) -> Result<(), Box<dyn std::error::Error>> {
	let mut args = std::env::args().skip(1);
	let id = args.next().ok_or("expected fixed row ID")?;
	if id.len() > 64 || args.next().is_some() {
		return Err("expected exactly one fixed row ID".into());
	}
	emit(&run(protocol_request(protocol, &id)?))
}

#[cfg(test)]
thread_local! {
	static FAIL_GRID: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn fixed_matrix_and_input_work_are_checked_before_allocation() {
		assert_eq!(ROWS.len(), 7);
		let b = plan(request("p1-c3-e1-w1_2").expect("row")).expect("plan");
		assert_eq!(b.dimension, 7776);
		assert_eq!(b.input_work, 32_997_376);
		assert!(request("arbitrary").is_err());
		assert_eq!(failure_status("contraction"), "numerical-failure");
		assert_eq!(
			failure_status("contraction admission"),
			"diagnostic-rejected"
		);
		assert!(add(usize::MAX, 1).is_err());
		assert!(mul(usize::MAX, 2).is_err());
		let mut huge = request(ROWS[0]).expect("row");
		huge.cells = usize::MAX;
		assert!(plan(huge).is_err());
	}

	#[test]
	fn serializer_refuses_exhaustion_without_allocating_a_growth_buffer() {
		let out = blank(request(ROWS[0]).expect("row"));
		let mut tiny = [0_u8; 1];
		assert!(encode(&out, &mut tiny).is_err());
		let mut bytes = [0_u8; 4096];
		let n = encode(&out, &mut bytes).expect("bounded encoding");
		let parsed: serde_json::Value = serde_json::from_slice(&bytes[..n]).expect("serde");
		assert_eq!(parsed["schema"], "quest-configuration-weak-row-v1");
		assert_eq!(parsed["request"]["physical_dimension"], 5);
		assert!(size_of::<Output>() * 3 + 4096 < HELPER);
	}

	#[test]
	fn actual_support_counts_preserve_duplicate_nodes_and_original_policy() {
		for id in ROWS {
			let r = request(id).expect("fixed");
			let p = plan(r).expect("preflight");
			let grid =
				ConfigurationGrid::uniform(5, -r.extent, r.extent, r.cells, r.order, p.dimension)
					.expect("grid");
			let sample = sampling(&grid, r).expect("counts");
			let original = regularization_resolution(&grid, &r.center, r.width, 2);
			if r.order == 2 && r.cells == 1 {
				assert_eq!(sample.distinct_samples, [1; 5]);
				assert!(original.is_err());
			} else {
				assert_eq!(
					original
						.expect("admitted policy")
						.distinct_samples_in_support,
					sample.distinct_samples
				);
			}
			let record = grid_record(&grid, r).expect("represented grid");
			assert_eq!(record.nodes.len(), p.axis_dimension);
			assert_eq!(record.node_bits.len(), p.axis_dimension);
			assert_eq!(record.grid_sha256.len(), 64);
			assert!(
				grid.retained_bytes().expect("bytes") + 32 * p.axis_dimension + 128 < GRID_ENVELOPE
			);
		}
	}

	#[test]
	fn injected_grid_failure_preserves_real_serializer_prefix() {
		FAIL_GRID.with(|flag| flag.set(true));
		let output = run(request(ROWS[0]).expect("row"));
		FAIL_GRID.with(|flag| flag.set(false));
		assert_eq!(output.status, "construction-rejected");
		assert_eq!(output.phase, "grid");
		assert!(output.represented_grid.is_none());
		assert!(output.source.is_none());
		let mut bytes = [0_u8; 4096];
		let n = encode(&output, &mut bytes).expect("fixed serializer");
		let parsed: serde_json::Value = serde_json::from_slice(&bytes[..n]).expect("schema");
		let expected: serde_json::Value = serde_json::from_str(include_str!(
			"../../../../docs/verification/fixtures/quest-cfd/configuration_weak_fixtures/grid_failure.json"
		))
		.expect("maintained test-only prefix");
		assert_eq!(parsed, expected);
		println!(
			"WEAK_PREFIX_JSON {}",
			serde_json::to_string(&parsed).expect("test serializer")
		);
	}

	#[test]
	fn additive_profile_has_its_own_single_request_and_small_geometry_only_preflight() {
		assert!(protocol_request(Protocol::SevenRows, ENERGY_ROW).is_err());
		for id in ROWS {
			assert!(protocol_request(Protocol::EnergyShellV2, id).is_err());
		}
		let r =
			protocol_request(Protocol::EnergyShellV2, ENERGY_ROW).expect("new explicit request");
		let p = plan(r).expect("same caps");
		assert_eq!(p.dimension, 7776);
		assert_eq!(p.input_work, 32_997_376);
		let grid =
			ConfigurationGrid::uniform(5, -r.extent, r.extent, r.cells, r.order, p.dimension)
				.expect("small axis recipes only");
		let samples =
			regularization_resolution(&grid, &r.center, r.width, 2).expect("original policy");
		assert_eq!(samples.distinct_samples_in_support, [2; 5]);
		assert_eq!(grid.axis_dimension(), 6);
	}

	#[test]
	fn additive_grid_fault_uses_real_shared_producer_and_serializer() {
		FAIL_GRID.with(|flag| flag.set(true));
		let out = run(protocol_request(Protocol::EnergyShellV2, ENERGY_ROW).expect("request"));
		FAIL_GRID.with(|flag| flag.set(false));
		assert_eq!(out.status, "construction-rejected");
		assert!(out.source.is_none());
		let mut bytes = [0_u8; 4096];
		let n = encode(&out, &mut bytes).expect("fixed serializer");
		let parsed: serde_json::Value = serde_json::from_slice(&bytes[..n]).expect("schema");
		let expected:serde_json::Value=serde_json::from_str(include_str!(
            "../../../../docs/verification/fixtures/quest-cfd/configuration_weak_energy_shell_v2_fixtures/grid_failure.json"
        )).expect("maintained additive failure prefix");
		assert_eq!(parsed, expected);
		println!(
			"WEAK_ENERGY_PREFIX_JSON {}",
			serde_json::to_string(&parsed).expect("test serializer")
		);
	}
}
