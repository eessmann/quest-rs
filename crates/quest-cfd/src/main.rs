//! Research workflows: execution reports never promote resource estimates into solved cases.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Dimensions and CLI inputs are checked before bounded numerical assembly"
)]
use clap::{Args, Parser, Subcommand};
use mathcore::multivariate::PolynomialLimits;
use quest_cfd::{
	CfdError, LiftKind, PeriodicBdm1,
	burgers::BurgersDg,
	carleman::{CarlemanLimits, SymmetricCarleman},
	carleman_certificate::{TruncationCertificate, admit_truncation},
	cases,
	configuration::ConfigurationGrid,
	constructed_resources::{
		ConstructedResourceLimits, InverseResourceRequest, ObservationResourceRequest,
		constructed_history_resources,
	},
	history::HistorySystem,
	kdv::KdvDg,
	physical_space::PhysicalSpace,
	polynomial::{PolynomialOde, ScalingEvidence},
	probability_observation::{DiagonalRange, SamplingRequest},
	resources::{ResourceRequest, estimate, estimate_carleman, estimate_symbolic},
	simplex::SimplexBdm,
	solve::{
		RhsPreparation, SolveBackend, SolveBudget, solve_history_with_backend,
		solve_history_with_reference_spectrum,
	},
};
use quest_numerics::{Complex64, SparseLimits};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Parser)]
#[command(about = "Full-DG CFD / KvN / global QSVT research example", version)]
struct Cli {
	#[command(subcommand)]
	command: Command,
}
#[derive(Subcommand)]
enum Command {
	/// Integrate the identical complete physical DG system classically.
	Reference {
		#[command(flatten)]
		case: CaseOptions,
		#[command(flatten)]
		lift: LiftOptions,
		#[arg(long)]
		dt: Option<f64>,
		#[arg(long)]
		steps: Option<u32>,
		/// Explicit BDM2 box-reference integration allowance; default is one billion work units.
		#[arg(long)]
		max_classical_work: Option<usize>,
	},
	/// Assemble the full configuration generator and global causal time operator.
	Build {
		#[command(flatten)]
		case: CaseOptions,
		#[command(flatten)]
		discretization: Discretization,
	},
	/// Construct and count real encoding/preparation costs on an explicitly bounded stored history.
	ResourceBuild {
		#[command(flatten)]
		case: CaseOptions,
		#[command(flatten)]
		discretization: Discretization,
		#[command(flatten)]
		resources: ConstructedOptions,
	},
	/// Execute a bounded whole-state QSVT circuit reference and check the physical residual.
	Solve {
		#[command(flatten)]
		case: CaseOptions,
		#[command(flatten)]
		discretization: Discretization,
		#[arg(long, default_value_t = 0.01)]
		residual: f64,
		#[arg(long, default_value_t = 2047)]
		max_degree: usize,
		#[arg(long, default_value_t = 1_000_000_000)]
		max_state_query_work: u64,
		#[arg(long)]
		no_certify: bool,
		/// Opt in to bounded dense classical spectral evidence (at most 512 history coordinates).
		#[arg(long)]
		reference_spectral_bound: bool,
		#[arg(long, value_enum, default_value = "scalar-reference")]
		backend: SolveBackend,
		#[arg(long, value_enum, default_value = "coherent")]
		rhs_preparation: RhsPreparation,
	},
	/// Exact untruncated tensor accounting; does not construct a quantum state.
	Estimate {
		#[command(flatten)]
		case: CaseOptions,
		#[command(flatten)]
		lift: LiftOptions,
		#[arg(long, default_value_t = 4)]
		configuration_coefficients: u32,
		#[arg(long, default_value_t = 10)]
		time_elements: u64,
		#[arg(long, default_value_t = 2)]
		temporal_coefficients: u32,
		#[arg(long, default_value_t = 16)]
		auxiliary_qubits: usize,
		#[arg(long, default_value = "268435456")]
		statevector_budget: String,
	},
}
#[derive(Clone, Args)]
struct ConstructedOptions {
	#[arg(long, default_value_t = 20_000_000)]
	count_work: usize,
	#[arg(long, default_value_t = 10_000_000)]
	gates_per_orientation: usize,
	#[arg(long, default_value_t = 1_000_000_000_000_u64)]
	matching_work: u64,
	#[arg(long)]
	inverse_plan: bool,
	#[arg(long, default_value_t = 0.03)]
	approximation_tolerance: f64,
	#[arg(long, default_value_t = 2047)]
	max_degree: usize,
	#[arg(long, default_value_t = 8_589_934_592_usize)]
	synthesis_work: usize,
	#[arg(long, requires_all = ["inverse_plan", "joint_success_lower_bound", "success_provenance"])]
	sample_error: Option<f64>,
	#[arg(long)]
	joint_success_lower_bound: Option<f64>,
	#[arg(long)]
	success_provenance: Option<String>,
	#[arg(long, default_value_t = 0.05)]
	sample_failure: f64,
	#[arg(long)]
	systematic_bias_bound: Option<f64>,
	#[arg(long, default_value_t = 1_000_000)]
	max_selected_shots: u64,
	#[arg(long, default_value_t = 1_000_000_000)]
	max_attempted_shots: u64,
}
#[derive(Clone, Args)]
struct CaseOptions {
	#[arg(long, default_value_t = 4)]
	cells: u32,
	#[arg(long)]
	physical_order: Option<usize>,
	#[arg(long)]
	initial_amplitude: Option<f64>,
	#[arg(long, default_value = "smoke")]
	case: String,
	#[arg(long, default_value_t = 100)]
	reynolds: u32,
	#[arg(long, default_value_t = 1)]
	mesh: u32,
	#[arg(long, default_value_t = 4)]
	cylinder_sectors: u32,
	#[arg(long, default_value_t = 1)]
	cylinder_layers: u32,
	#[arg(long, default_value_t = 1)]
	span_layers: u32,
}
impl CaseOptions {
	fn is_one_dimensional(&self) -> bool {
		matches!(self.case.as_str(), "burgers" | "kdv" | "airy")
	}
	fn is_kdv(&self) -> bool {
		matches!(self.case.as_str(), "kdv" | "airy")
	}
	fn physical_order(&self) -> usize {
		self.physical_order
			.unwrap_or_else(|| if self.is_kdv() { 2 } else { 1 })
	}
	fn initial_amplitude(&self) -> f64 {
		self.initial_amplitude
			.unwrap_or_else(|| if self.is_kdv() { 0.05 } else { 0.01 })
	}
}
#[derive(Clone, Args)]
struct LiftOptions {
	#[arg(long, value_enum, default_value = "kvn")]
	lift: LiftKind,
	#[arg(long, default_value_t = 4)]
	carleman_order: usize,
	/// Explicit experimental physical scale; no theorem certificate is implied.
	#[arg(long, conflicts_with = "certify_carleman")]
	carleman_scale: Option<f64>,
	/// Require an interval-checked continuous Carleman truncation bound for the recorded ODE.
	#[arg(long)]
	certify_carleman: bool,
}
#[derive(Clone, Args)]
struct Discretization {
	#[command(flatten)]
	lift: LiftOptions,
	#[arg(long, default_value_t = 1.0)]
	configuration_extent: f64,
	#[arg(long, default_value_t = 2)]
	configuration_cells: usize,
	#[arg(long, default_value_t = 1)]
	configuration_order: usize,
	#[arg(long, default_value_t = 1.2)]
	regularization_width: f64,
	#[arg(long)]
	horizon: Option<f64>,
	#[arg(long)]
	time_cells: Option<usize>,
	#[arg(long, default_value_t = 1)]
	time_order: usize,
	#[arg(long, default_value_t = 1_048_576)]
	max_dimension: usize,
	#[arg(long, default_value_t = 4_194_304)]
	max_entries: usize,
	#[arg(long, default_value_t = 268_435_456)]
	max_bytes: usize,
}
impl Discretization {
	fn horizon(&self, case: &CaseOptions) -> f64 {
		self.horizon
			.unwrap_or_else(|| if case.is_one_dimensional() { 0.1 } else { 0.01 })
	}
	fn time_cells(&self, case: &CaseOptions) -> usize {
		self.time_cells
			.unwrap_or_else(|| if case.is_one_dimensional() { 2 } else { 1 })
	}
}
enum Physical {
	Burgers(Box<BurgersDg>),
	Kdv(Box<KdvDg>),
	Smoke(Box<PeriodicBdm1>),
	General(Box<SimplexBdm>),
	Higher(Box<PhysicalSpace>),
}
impl Physical {
	fn dimension(&self) -> usize {
		match self {
			Self::Burgers(m) => m.dimension(),
			Self::Kdv(m) => m.dimension(),
			Self::Smoke(m) => m.dimension(),
			Self::General(m) => m.dimension(),
			Self::Higher(m) => m.dimension(),
		}
	}
	fn drift(&self, x: &[f64]) -> Result<Vec<f64>, CfdError> {
		match self {
			Self::Burgers(m) => m.drift(x),
			Self::Kdv(m) => m.drift(x),
			Self::Smoke(m) => m.drift(x),
			Self::General(m) => m.drift(x),
			Self::Higher(m) => m.drift(x),
		}
	}
	fn energy(&self, x: &[f64]) -> Result<f64, CfdError> {
		match self {
			Self::Burgers(m) => m.energy(x),
			Self::Kdv(m) => m.energy(x),
			Self::Smoke(m) => m.energy(x),
			Self::General(m) => m.energy(x),
			Self::Higher(m) => m.energy(x),
		}
	}
	fn diagnostics(&self) -> Value {
		match self {
			Self::Burgers(m) => {
				json!({"manifest":quest_cfd::burgers::manifest(),"physical_dimension":m.dimension()})
			}
			Self::Kdv(m) => {
				json!({"manifest":quest_cfd::kdv::manifest(),"physical_dimension":m.dimension(),"auxiliary_dimension":m.field_dimension(),"mode":m.mode(),"energy_convention":"integral of u squared plus auxiliary field squared"})
			}
			Self::Smoke(m) => json!(m.diagnostics()),
			Self::General(m) => json!(m.diagnostics()),
			Self::Higher(m) => json!(m.diagnostics()),
		}
	}
}
fn validate_physical_order(case: &CaseOptions) -> Result<(), CfdError> {
	if !case.is_one_dimensional()
		&& case.physical_order() != 1
		&& !(case.physical_order() == 2
			&& matches!(
				case.case.as_str(),
				"tgv2d" | "tgv3d" | "cavity2d" | "cavity3d"
			)) {
		return Err(CfdError::Unsupported("BDM2 is supported on Taylor-Green/cavity boxes; other incompressible cases require BDM1".into()));
	}
	Ok(())
}
fn physical(case: &CaseOptions) -> Result<(Physical, Vec<f64>), CfdError> {
	if case.is_kdv() {
		let model = if case.case == "airy" {
			KdvDg::linear_airy(case.cells, case.physical_order())?
		} else {
			KdvDg::new(case.cells, case.physical_order())?
		};
		let initial = model.initial_state(case.initial_amplitude())?;
		return Ok((Physical::Kdv(Box::new(model)), initial));
	}
	if case.case == "burgers" {
		let model = BurgersDg::new(case.cells, case.physical_order(), 0.1)?;
		let initial = model.initial_state(case.initial_amplitude())?;
		return Ok((Physical::Burgers(Box::new(model)), initial));
	}
	validate_physical_order(case)?;
	if case.physical_order() == 2 {
		let reference =
			quest_cfd::cases_high_order::box_reference(&case.case, case.reynolds, case.mesh, 2)?;
		return Ok((
			Physical::Higher(Box::new(reference.model)),
			reference.initial_state,
		));
	}
	if case.case == "smoke" {
		if case.reynolds == 0 {
			return Err(CfdError::InvalidInput("positive Reynolds number required"));
		}
		return Ok((
			Physical::Smoke(Box::new(PeriodicBdm1::assemble(
				1.0 / f64::from(case.reynolds),
			)?)),
			vec![0.15, -0.1, 0.07, 0.11, -0.04],
		));
	}
	if case.case.starts_with("shedding") {
		let reference = quest_cfd::cylinder::reference(
			&case.case,
			case.reynolds,
			case.cylinder_sectors,
			case.cylinder_layers,
			case.span_layers,
		)?;
		Ok((
			Physical::General(Box::new(reference.model)),
			reference.initial_state,
		))
	} else {
		let reference = cases::box_reference(&case.case, case.reynolds, case.mesh)?;
		Ok((
			Physical::General(Box::new(reference.model)),
			reference.initial_state,
		))
	}
}
fn reference(
	case: &CaseOptions,
	lift: &LiftOptions,
	dt: Option<f64>,
	steps: Option<u32>,
	max_classical_work: Option<usize>,
) -> Result<Value, CfdError> {
	validate_physical_order(case)?;
	if max_classical_work.is_some()
		&& (case.physical_order() != 2
			|| lift.lift == LiftKind::Carleman
			|| !matches!(
				case.case.as_str(),
				"tgv2d" | "tgv3d" | "cavity2d" | "cavity3d"
			)) {
		return Err(CfdError::InvalidInput(
			"--max-classical-work requires a direct BDM2 box reference",
		));
	}
	let dt = dt.unwrap_or_else(|| {
		if case.is_one_dimensional() {
			0.0001
		} else {
			0.001
		}
	});
	let steps = steps.unwrap_or_else(|| if case.is_one_dimensional() { 1000 } else { 1 });

	if lift.lift == LiftKind::Carleman {
		return reference_carleman(case, lift, dt, steps);
	}
	let data = if case.is_kdv() {
		let (Physical::Kdv(model), _) = physical(case)? else {
			return Err(CfdError::Assembly("KdV dispatch"));
		};
		json!(model.classical_reference(case.initial_amplitude(), dt * f64::from(steps), steps)?)
	} else if case.case == "burgers" {
		let (Physical::Burgers(model), _) = physical(case)? else {
			return Err(CfdError::Assembly("Burgers dispatch"));
		};
		json!(model.classical_reference(case.initial_amplitude(), dt * f64::from(steps), steps)?)
	} else if case.case == "smoke" {
		let (model, initial) = physical(case)?;
		let Physical::Smoke(model) = model else {
			return Err(CfdError::Assembly("smoke dispatch"));
		};
		let state = model.integrate_rk4(
			&initial,
			dt,
			usize::try_from(steps).map_err(|_| CfdError::InvalidInput("step count"))?,
		)?;
		json!({"case":"smoke","time":dt*f64::from(steps),"diagnostics":model.diagnostics(),"energy":model.energy(&state)?,"pressure":model.reconstruct_pressure(&state)?,"full_coordinates":state})
	} else if case.case.starts_with("shedding") {
		json!(
			quest_cfd::cylinder::reference(
				&case.case,
				case.reynolds,
				case.cylinder_sectors,
				case.cylinder_layers,
				case.span_layers
			)?
			.reference(dt, steps)?
		)
	} else if case.physical_order() == 2 {
		json!(
			quest_cfd::cases_high_order::box_reference(&case.case, case.reynolds, case.mesh, 2)?
				.reference_with_work_limit(
					dt,
					steps,
					max_classical_work.unwrap_or(1_000_000_000)
				)?
		)
	} else {
		json!(cases::box_reference(&case.case, case.reynolds, case.mesh)?.reference(dt, steps)?)
	};
	Ok(
		json!({"status":"classical-reference-executed","benchmark_convergence_established":false,"quantum_execution":false,"reference":data}),
	)
}
fn select_carleman_scaling(
	ode: &PolynomialOde,
	initial: &[f64],
	horizon: f64,
	options: &LiftOptions,
) -> Result<(ScalingEvidence, Option<TruncationCertificate>), CfdError> {
	let mut scaling = ode.experimental_scaling(initial, horizon)?;
	if !options.certify_carleman || scaling.scale.is_none() {
		return Ok((scaling, None));
	}
	let admission = admit_truncation(ode, initial, horizon, options.carleman_order)?;
	let certificate = admission
		.certificate
		.ok_or_else(|| CfdError::Unsupported(admission.reason.into()))?;
	scaling.scale = Some(certificate.scale());
	scaling.rc_upper = Some(certificate.rc_upper());
	scaling.corrected_forcing_hypothesis = true;
	scaling.status = "admitted continuous Carleman truncation scale for recorded polynomial ODE; other errors remain separate".into();
	Ok((scaling, Some(certificate)))
}
fn reference_carleman(
	case: &CaseOptions,
	options: &LiftOptions,
	dt: f64,
	steps: u32,
) -> Result<Value, CfdError> {
	if !dt.is_finite() || dt <= 0. || steps == 0 || steps > 1_000_000 {
		return Err(CfdError::InvalidInput(
			"invalid classical reference step/count",
		));
	}
	let (model, initial) = physical(case)?;
	let horizon = dt * f64::from(steps);
	let (ode, extraction) = polynomial_model(&model, CarlemanLimits::default().max_bytes)?;
	let evidence = ode.coefficient_evidence(horizon)?;
	let (scaling, truncation_certificate) =
		select_carleman_scaling(&ode, &initial, horizon, options)?;
	let Some(default_scale) = scaling.scale else {
		return Ok(
			json!({"status":"exact-zero-trajectory","quantum_execution":false,"physical_coordinates":initial,"scaling_evidence":scaling}),
		);
	};
	let hierarchy = SymmetricCarleman::new(
		ode,
		options.carleman_order,
		options.carleman_scale.unwrap_or(default_scale),
		CarlemanLimits::default(),
	)?;
	let lifted = hierarchy.integrate_rk4(&initial, dt, steps)?;
	let recovered = hierarchy.recover(&lifted)?;
	let reference = match &model {
		Physical::Smoke(m) => m.integrate_rk4(
			&initial,
			dt,
			usize::try_from(steps).map_err(|_| CfdError::InvalidInput("step count"))?,
		)?,
		Physical::General(m) => m.integrate_rk4(
			&initial,
			dt,
			usize::try_from(steps).map_err(|_| CfdError::InvalidInput("step count"))?,
		)?,
		Physical::Higher(m) => m.integrate_rk4(
			&initial,
			horizon,
			usize::try_from(steps).map_err(|_| CfdError::InvalidInput("step count"))?,
		)?,
		Physical::Burgers(m) => m.polynomial_ode().integrate_rk4(&initial, dt, steps)?,
		Physical::Kdv(m) => m.integrate_rk4(&initial, dt, steps)?,
	};
	let error = recovered
		.iter()
		.zip(&reference)
		.fold(0_f64, |sum, (a, b)| sum.hypot(a - b));
	Ok(
		json!({"status":"classical-reference-executed","quantum_execution":false,"benchmark_convergence_established":false,"case":case.case,"horizon":horizon,"steps":steps,"coefficient_extraction":extraction,"coefficient_evidence":evidence,"scaling_evidence":scaling,"continuous_carleman_truncation_certificate":truncation_certificate,"explicit_scale_override":options.carleman_scale,"scaling_evidence_applies_to_override":options.carleman_scale.is_none(),"reference":{"physical_coordinates":reference,"energy":model.energy(&reference)?},"carleman_reference":{"order":hierarchy.order(),"dimension":hierarchy.dimension(),"physical_scale":hierarchy.scale(),"physical_coordinates":recovered,"absolute_coordinate_error":error,"physical_reconstruction_defect":hierarchy.physical_reconstruction_defect(horizon,&lifted)?,"lift_chain_rule_truncation_defect":hierarchy.reconstruction_defect(horizon,&recovered)?},"temporal_discretization":"classical RK4 in both trajectories; global quantum history uses temporal DG"}),
	)
}
enum BuiltLift {
	Kvn(ConfigurationGrid),
	Carleman(SymmetricCarleman),
}
struct Built {
	model: Physical,
	lift: Option<BuiltLift>,
	history: Option<HistorySystem>,
	report: Value,
	exact_zero: bool,
}
fn build(case: &CaseOptions, args: &Discretization) -> Result<Built, CfdError> {
	let (model, center) = physical(case)?;
	if args.lift.lift == LiftKind::Carleman {
		return build_carleman(model, &center, case, args);
	}
	let grid = ConfigurationGrid::uniform(
		model.dimension(),
		-args.configuration_extent,
		args.configuration_extent,
		args.configuration_cells,
		args.configuration_order,
		args.max_dimension,
	)?;
	let limits = SparseLimits {
		max_dimension: args.max_dimension,
		max_entries: args.max_entries,
		max_bytes: args.max_bytes,
		..SparseLimits::default()
	};
	let generator = grid.generator_from(model.dimension(), |x| model.drift(x), limits)?;
	let initial = grid.initial_bump(&center, args.regularization_width)?;
	let initial_observables = grid.observables(&initial, |x| model.energy(x))?;
	let history = HistorySystem::assemble(
		&generator,
		&initial,
		args.horizon(case),
		args.time_cells(case),
		args.time_order,
		limits,
	)?;
	let (bound, bound_rejection) = match history.spectral_bounds(limits) {
		Ok(b) => (
			Some(
				json!({"lower":b.lower(),"upper":b.upper(),"evidence":format!("{:?}",b.evidence())}),
			),
			None,
		),
		Err(error) => (None, Some(error.to_string())),
	};
	let report = json!({"status":"construction-only","case":case.case,"full_dg_diagnostics":model.diagnostics(),"independent_coordinates":model.dimension(),"configuration_dimension":grid.dimension(),"configuration_order":args.configuration_order,"configuration_cells_per_axis":args.configuration_cells,"configuration_bounds":grid.bounds(),"configuration_boundary":"central periodic numerical closure; truncation leakage requires refinement","regularization_width":args.regularization_width,"initial_observables":initial_observables,"generator_nonzeros":generator.nnz(),"history_dimension":history.operator().rows(),"history_nonzeros":history.operator().nnz(),"time_order":args.time_order,"horizon":args.horizon(case),"lift":"kvn","spectral_bounds":bound,"spectral_bound_rejection":bound_rejection,"normal_equations":false,"quantum_execution":false});
	Ok(Built {
		model,
		lift: Some(BuiltLift::Kvn(grid)),
		history: Some(history),
		report,
		exact_zero: false,
	})
}
fn polynomial_model(
	model: &Physical,
	max_bytes: usize,
) -> Result<(Arc<PolynomialOde>, Value), CfdError> {
	let polynomial_limits = PolynomialLimits {
		max_bytes,
		..PolynomialLimits::default()
	};
	let result = match model {
		Physical::Kdv(model) => (
			model.polynomial_ode(),
			json!({"status":"MathCore exact polynomial forms with recorded Fu-Shu quadrature coefficients; both fields retained"}),
		),
		Physical::Burgers(model) => (
			model.polynomial_ode(),
			json!({"status":"MathCore exact polynomial forms with recorded quadrature coefficients"}),
		),
		Physical::Smoke(model) => {
			let snapshot = PolynomialOde::from_periodic_bdm1(model, polynomial_limits)?;
			(Arc::new(snapshot.dynamics), json!(snapshot.evidence))
		}
		Physical::Higher(model) => {
			let snapshot = PolynomialOde::from_physical_space(model, polynomial_limits)?;
			(Arc::new(snapshot.dynamics), json!(snapshot.evidence))
		}
		Physical::General(model) => {
			let snapshot = PolynomialOde::from_simplex_bdm1(model, polynomial_limits)?;
			(Arc::new(snapshot.dynamics), json!(snapshot.evidence))
		}
	};
	Ok(result)
}
fn build_carleman(
	model: Physical,
	center: &[f64],
	case: &CaseOptions,
	args: &Discretization,
) -> Result<Built, CfdError> {
	let (ode, extraction) = polynomial_model(&model, args.max_bytes)?;
	let horizon = args.horizon(case);
	let coefficient_evidence = ode.coefficient_evidence(horizon)?;
	let (scaling, truncation_certificate) =
		select_carleman_scaling(&ode, center, horizon, &args.lift)?;
	let exact_zero = scaling.scale.is_none();
	let scale = args
		.lift
		.carleman_scale
		.unwrap_or_else(|| scaling.scale.unwrap_or(1.));

	if !scale.is_finite() || scale <= 0. {
		return Err(CfdError::InvalidInput(
			"positive finite Carleman scale required",
		));
	}
	if exact_zero {
		let report = json!({"status":"exact-zero-trajectory", "case":case.case,"lift":"carleman","independent_coordinates":model.dimension(),"physical_coordinates":center,"scaling_evidence":scaling,"coefficient_evidence":coefficient_evidence,"coefficient_extraction":extraction,"quantum_execution":false,"history_construction_required":false});
		return Ok(Built {
			model,
			lift: None,
			history: None,
			report,
			exact_zero: true,
		});
	}
	let hierarchy = SymmetricCarleman::new(
		ode,
		args.lift.carleman_order,
		scale,
		CarlemanLimits {
			max_dimension: args.max_dimension,
			max_entries: args.max_entries,
			max_bytes: args.max_bytes,
			..CarlemanLimits::default()
		},
	)?;
	let initial: Vec<_> = hierarchy
		.lift(center)?
		.into_iter()
		.map(|x| Complex64::new(x, 0.))
		.collect();
	let limits = SparseLimits {
		max_dimension: args.max_dimension,
		max_entries: args.max_entries,
		max_bytes: args.max_bytes,
		..SparseLimits::default()
	};
	let history = HistorySystem::assemble_dynamics(
		&hierarchy,
		&initial,
		horizon,
		args.time_cells(case),
		args.time_order,
		limits,
	)?;
	let (bound, rejection) = match history.spectral_bounds(limits) {
		Ok(b) => (
			Some(
				json!({"lower":b.lower(),"upper":b.upper(),"evidence":format!("{:?}",b.evidence())}),
			),
			None,
		),
		Err(e) => (None, Some(e.to_string())),
	};
	let report = json!({"status":"construction-only","case":case.case,"lift":"carleman", "representation":"all normalized symmetric monomials; degree zero is external forcing", "independent_coordinates":model.dimension(),"all_independent_coordinates_retained":true,"physical_diagnostics":model.diagnostics(),"physical_order":case.physical_order(),"physical_cells_1d":if case.is_one_dimensional() {Some(case.cells)} else {None},"lift_order":hierarchy.order(),"lift_dimension":hierarchy.dimension(),"physical_scale":scale,"scaling_evidence":scaling,"continuous_carleman_truncation_certificate":truncation_certificate,"explicit_scale_override":args.lift.carleman_scale,"scaling_evidence_applies_to_override":args.lift.carleman_scale.is_none(),"coefficient_evidence":coefficient_evidence,"coefficient_extraction":extraction,"theorem_convergence_certified":false,"truncation_accuracy_established":false,"exact_zero_trajectory":exact_zero,"history_dimension":history.operator().rows(),"history_nonzeros":history.operator().nnz(),"time_order":args.time_order,"time_cells":args.time_cells(case),"horizon":horizon,"spectral_bounds":bound,"spectral_bound_rejection":rejection,"normal_equations":false,"quantum_execution":false});
	Ok(Built {
		model,
		lift: Some(BuiltLift::Carleman(hierarchy)),
		history: Some(history),
		report,
		exact_zero,
	})
}
fn carleman_observables(
	model: &Physical,
	hierarchy: &SymmetricCarleman,
	result: &quest_cfd::solve::SolveReport,
	time: f64,
) -> Result<Value, CfdError> {
	let final_state = result
		.solution
		.get(
			result
				.solution
				.len()
				.checked_sub(hierarchy.dimension())
				.ok_or(CfdError::Assembly("Carleman history length"))?..,
		)
		.ok_or(CfdError::Assembly("Carleman final slab"))?;
	let real: Vec<_> = final_state.iter().map(|z| z.re).collect();
	let physical = hierarchy.recover(&real)?;
	let solution_norm = result
		.solution
		.iter()
		.fold(0_f64, |sum, z| sum.hypot(z.norm()));
	let final_norm = final_state.iter().fold(0_f64, |sum, z| sum.hypot(z.norm()));
	let first_norm = final_state
		.iter()
		.zip(hierarchy.powers())
		.filter(|(_, p)| p.iter().sum::<u32>() == 1)
		.fold(0_f64, |sum, (z, _)| sum.hypot(z.norm()));
	if !solution_norm.is_finite() || solution_norm == 0. {
		return Err(CfdError::Assembly("invalid reconstructed solution norm"));
	}
	let selection = (first_norm / solution_norm).powi(2);
	Ok(
		json!({"physical_coordinates":physical,"energy":model.energy(&physical)?,"physical_scale":hierarchy.scale(),"solution_norm":solution_norm,"history_inverse_success_probability":result.success_probability,"final_time_selection_probability_given_inverse":(final_norm/solution_norm).powi(2),"degree_one_and_final_time_probability_given_inverse":selection,"combined_inverse_degree_time_probability":result.success_probability*selection,"physical_reconstruction_defect":hierarchy.physical_reconstruction_defect(time,&real)?,"lift_chain_rule_truncation_defect":hierarchy.reconstruction_defect(time,&physical)?,"imaginary_lift_norm":final_state.iter().fold(0_f64,|sum,z|sum.hypot(z.im)),"observable_recovery":"bounded full-state readout; sampling cost not certified","truncation_accuracy_established":false}),
	)
}
fn resource(
	case: &CaseOptions,
	lift: &LiftOptions,
	axis: u32,
	time: u64,
	temporal: u32,
	ancillas: usize,
	budget: String,
) -> Result<Value, CfdError> {
	validate_physical_order(case)?;
	let (local, rank) = if case.is_one_dimensional() {
		let dimension = usize::try_from(case.cells)
			.ok()
			.and_then(|n| n.checked_mul(case.physical_order().checked_add(1)?))
			.and_then(|n| n.checked_mul(if case.is_kdv() { 2 } else { 1 }))
			.filter(|n| *n > 0)
			.ok_or(CfdError::InvalidInput(
				"one-dimensional DG dimension overflow",
			))?;
		if (case.is_kdv() && !(2..=3).contains(&case.physical_order()))
			|| (!case.is_kdv() && !(1..=2).contains(&case.physical_order()))
		{
			return Err(CfdError::Unsupported(
				"Burgers DG order requires 1 or 2; KdV DG order requires 2 or 3".into(),
			));
		}
		(dimension, 0)
	} else if case.case == "smoke" {
		(12, 7)
	} else if !case.case.starts_with("shedding") {
		let manifest = cases::manifest(&case.case)?;
		manifest.viscosity(case.reynolds)?;
		quest_cfd::cases_high_order::box_chart_dimensions(
			manifest.dimension,
			case.mesh,
			case.case.starts_with("tgv"),
			case.physical_order(),
		)?
	} else {
		let manifest = cases::manifest(&case.case)?;
		manifest.viscosity(case.reynolds)?;
		quest_cfd::cylinder::cylinder_chart_dimensions(
			&case.case,
			case.cylinder_sectors,
			case.cylinder_layers,
			case.span_layers,
		)?
	};
	let request = ResourceRequest {
		local_velocity_dimension: local,
		constraint_rank: rank,
		configuration_coefficients_per_axis: axis,
		time_elements: time,
		temporal_coefficients: temporal,
		auxiliary_qubits: ancillas,
		statevector_budget_bytes: budget,
	};
	if lift.lift == LiftKind::Carleman {
		return Ok(
			json!({"case":case.case,"lift":"carleman","order":lift.carleman_order,"request":request,"estimate":estimate_carleman(&request,lift.carleman_order)?,"all_independent_coordinates_retained":true,"truncation_accuracy_established":false,"ancillas":"caller-supplied allowance, not a constructed encoding"}),
		);
	}
	let (decimal_estimate, decimal_rejection) = match estimate(&request) {
		Ok(value) => (Some(value), None),
		Err(error) => (None, Some(error.to_string())),
	};
	Ok(
		json!({"case":case.case,"request":request,"estimate":decimal_estimate,"decimal_expansion_rejection":decimal_rejection,"symbolic_estimate":estimate_symbolic(&request)?,"all_independent_coordinates_retained":true,"ancillas":"caller-supplied explicit allowance; requires encoding-specific verification"}),
	)
}
fn resource_build(
	case: &CaseOptions,
	args: &Discretization,
	options: &ConstructedOptions,
) -> Result<Value, CfdError> {
	let started = std::time::Instant::now();
	let Built {
		model,
		lift,
		history,
		report: construction,
		exact_zero,
	} = build(case, args)?;
	let physical_dimension = model.dimension();
	drop(model);
	drop(lift);
	let build_seconds = started.elapsed().as_secs_f64();
	if exact_zero {
		return Ok(
			json!({"status":"zero-rhs-no-circuit","construction":construction,"quantum_execution":false,"all_independent_coordinates_retained":true,"physical_coordinates":physical_dimension,"history_build_seconds":build_seconds}),
		);
	}
	let history = history.ok_or(CfdError::Assembly("missing resource history"))?;
	let dimension = history.configuration_dimension();
	let last_node = history
		.operator()
		.rows()
		.checked_sub(dimension)
		.ok_or(CfdError::InvalidInput("resource temporal selection"))?;
	// A real bounded index recipe, deliberately not a physical-coordinate reconstruction.
	let query = |index: usize| {
		Ok((
			index >= last_node,
			f64::from(u8::from(index.is_multiple_of(dimension))),
		))
	};
	let observation = if let Some(error) = options.sample_error {
		Some(ObservationResourceRequest {
			sampling: SamplingRequest {
				range: DiagonalRange::new(0., 1.)?,
				absolute_error: error,
				failure_probability: options.sample_failure,
				joint_success_lower_bound: options.joint_success_lower_bound.ok_or(
					CfdError::InvalidInput("joint sampling probability premise required"),
				)?,
				success_bound_provenance: options.success_provenance.as_deref().ok_or(
					CfdError::InvalidInput("sampling probability provenance required"),
				)?,
				max_selected_shots: options.max_selected_shots,
				max_attempted_shots: options.max_attempted_shots,
				max_provenance_bytes: 4096,
				systematic_bias_bound: options.systematic_bias_bound,
			},
			description: "conditional normalized population of first lifted/configuration basis coordinate in last stored temporal node; not physical-coordinate reconstruction",
			retained_bytes: 0,
			scratch_bytes: 0,
			selection_query_work: 1,
			observable_query_work: 2,
			max_validation_work: 1_000_000_000,
			query: &query,
		})
	} else {
		None
	};
	let result = constructed_history_resources(
		&history,
		ConstructedResourceLimits {
			max_bytes: args.max_bytes,
			max_matching_work: options.matching_work,
			max_count_work: options.count_work,
			max_gates_per_orientation: options.gates_per_orientation,
			..ConstructedResourceLimits::default()
		},
		options.inverse_plan.then_some(InverseResourceRequest {
			approximation_tolerance: options.approximation_tolerance,
			max_degree: options.max_degree,
			max_synthesis_work: options.synthesis_work,
			..InverseResourceRequest::default()
		}),
		observation,
	)?;
	Ok(
		json!({"schema":"quest-cfd-constructed-resources-v1","construction":construction,"resources":result,"physical_coordinates":physical_dimension,"all_independent_coordinates_retained":true,"history_build_seconds":build_seconds,"history_build_peak_bytes":null,"history_build_memory_scope":"existing bounded stored physical/lift/CSR construction; no end-to-end physical-builder capacity telemetry", "quantum_execution":false,"equal_accuracy_comparison":false}),
	)
}
fn solve_built(
	built: Built,
	horizon: f64,
	solve_budget: SolveBudget,
	backend: SolveBackend,
	reference_spectral_bound: bool,
) -> Result<Value, CfdError> {
	let Built {
		model,
		lift,
		history,
		report: construction,
		exact_zero,
	} = built;
	if exact_zero {
		return Ok(
			json!({"status":"exact-zero-trajectory","quantum_execution":false,"physical_coordinates":vec![0.;model.dimension()],"construction":construction}),
		);
	}
	let history = history.ok_or(CfdError::Assembly("missing history"))?;
	let lift = lift.ok_or(CfdError::Assembly("missing lift"))?;

	let (mut result, reference_spectrum) = if reference_spectral_bound {
		let (result, evidence) = solve_history_with_reference_spectrum(
			&history,
			solve_budget,
			backend,
			quest_cfd::history_spectrum::ReferenceSpectrumBudget::default(),
		)?;
		(result, Some(evidence))
	} else {
		(
			solve_history_with_backend(&history, solve_budget, backend)?,
			None,
		)
	};
	result.retained_physical_coordinates = Some(model.dimension());

	let observables = match &lift {
		BuiltLift::Kvn(grid) => {
			let final_state = result
				.solution
				.get(result.solution.len().saturating_sub(grid.dimension())..)
				.ok_or(CfdError::Assembly("final history slice"))?;
			json!(grid.observables(final_state, |x| model.energy(x))?)
		}
		BuiltLift::Carleman(hierarchy) => {
			carleman_observables(&model, hierarchy, &result, horizon)?
		}
	};
	Ok(
		json!({"construction":construction,"solve":result,"reference_spectrum":reference_spectrum,"final_observables":observables,"physical_convergence_established":false}),
	)
}

fn run(command: Command) -> Result<Value, CfdError> {
	let (options, can_certify) = match &command {
		Command::Reference { lift, .. } => (lift, true),
		Command::Estimate { lift, .. } => (lift, false),
		Command::Build { discretization, .. }
		| Command::Solve { discretization, .. }
		| Command::ResourceBuild { discretization, .. } => (&discretization.lift, true),
	};
	if options.certify_carleman && (options.lift != LiftKind::Carleman || !can_certify) {
		return Err(CfdError::InvalidInput(
			"--certify-carleman requires --lift carleman with reference, build or solve; estimates do not certify truncation",
		));
	}
	match command {
		Command::Reference {
			case,
			lift,
			dt,
			steps,
			max_classical_work,
		} => reference(&case, &lift, dt, steps, max_classical_work),
		Command::Build {
			case,
			discretization,
		} => Ok(build(&case, &discretization)?.report),
		Command::ResourceBuild {
			case,
			discretization,
			resources,
		} => resource_build(&case, &discretization, &resources),
		Command::Estimate {
			case,
			lift,
			configuration_coefficients,
			time_elements,
			temporal_coefficients,
			auxiliary_qubits,
			statevector_budget,
		} => resource(
			&case,
			&lift,
			configuration_coefficients,
			time_elements,
			temporal_coefficients,
			auxiliary_qubits,
			statevector_budget,
		),
		Command::Solve {
			case,
			discretization,
			residual,
			max_degree,
			max_state_query_work,
			no_certify,
			reference_spectral_bound,
			backend,
			rhs_preparation,
		} => {
			let built = build(&case, &discretization)?;
			let solve_budget = SolveBudget {
				max_bytes: discretization.max_bytes,
				max_degree,
				max_state_query_work,
				relative_residual: residual,
				certify: !no_certify,
				rhs_preparation,
			};
			solve_built(
				built,
				discretization.horizon(&case),
				solve_budget,
				backend,
				reference_spectral_bound,
			)
		}
	}
}
fn main() -> std::process::ExitCode {
	let result = run(Cli::parse().command);
	let (report, status) = match result {
		Ok(value) => (value, std::process::ExitCode::SUCCESS),
		Err(error) => (
			json!({"status":"rejected-or-failed","quantum_solution_claimed":false,"reason":error.to_string()}),
			std::process::ExitCode::FAILURE,
		),
	};
	match serde_json::to_string_pretty(&report) {
		Ok(text) => println!("{text}"),
		Err(error) => {
			eprintln!("report serialization failed: {error}");
			return std::process::ExitCode::FAILURE;
		}
	}
	status
}
