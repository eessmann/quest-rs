//! Research workflows: execution reports never promote resource estimates into solved cases.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Dimensions and CLI inputs are checked before bounded numerical assembly"
)]
use clap::{Args, Parser, Subcommand};
use quest_cfd::{
	CfdError, PeriodicBdm1, cases,
	configuration::ConfigurationGrid,
	history::HistorySystem,
	resources::{ResourceRequest, estimate, estimate_symbolic},
	simplex::SimplexBdm,
	solve::{SolveBackend, SolveBudget, solve_history_with_backend},
};
use quest_numerics::SparseLimits;
use serde_json::{Value, json};

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
		#[arg(long, default_value_t = 0.001)]
		dt: f64,
		#[arg(long, default_value_t = 1)]
		steps: u32,
	},
	/// Assemble the full configuration generator and global causal time operator.
	Build {
		#[command(flatten)]
		case: CaseOptions,
		#[command(flatten)]
		discretization: Discretization,
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
		#[arg(long, value_enum, default_value = "scalar-reference")]
		backend: SolveBackend,
	},
	/// Exact untruncated tensor accounting; does not construct a quantum state.
	Estimate {
		#[command(flatten)]
		case: CaseOptions,
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
struct CaseOptions {
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
#[derive(Clone, Args)]
struct Discretization {
	#[arg(long, default_value_t = 1.0)]
	configuration_extent: f64,
	#[arg(long, default_value_t = 2)]
	configuration_cells: usize,
	#[arg(long, default_value_t = 1)]
	configuration_order: usize,
	#[arg(long, default_value_t = 1.2)]
	regularization_width: f64,
	#[arg(long, default_value_t = 0.01)]
	horizon: f64,
	#[arg(long, default_value_t = 1)]
	time_cells: usize,
	#[arg(long, default_value_t = 1)]
	time_order: usize,
	#[arg(long, default_value_t = 1_048_576)]
	max_dimension: usize,
	#[arg(long, default_value_t = 4_194_304)]
	max_entries: usize,
	#[arg(long, default_value_t = 268_435_456)]
	max_bytes: usize,
}
enum Physical {
	Smoke(Box<PeriodicBdm1>),
	General(Box<SimplexBdm>),
}
impl Physical {
	fn dimension(&self) -> usize {
		match self {
			Self::Smoke(m) => m.dimension(),
			Self::General(m) => m.dimension(),
		}
	}
	fn drift(&self, x: &[f64]) -> Result<Vec<f64>, CfdError> {
		match self {
			Self::Smoke(m) => m.drift(x),
			Self::General(m) => m.drift(x),
		}
	}
	fn energy(&self, x: &[f64]) -> Result<f64, CfdError> {
		match self {
			Self::Smoke(m) => m.energy(x),
			Self::General(m) => m.energy(x),
		}
	}
	fn diagnostics(&self) -> Value {
		match self {
			Self::Smoke(m) => json!(m.diagnostics()),
			Self::General(m) => json!(m.diagnostics()),
		}
	}
}
fn physical(case: &CaseOptions) -> Result<(Physical, Vec<f64>), CfdError> {
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
fn reference(case: &CaseOptions, dt: f64, steps: u32) -> Result<Value, CfdError> {
	let data = if case.case == "smoke" {
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
	} else {
		json!(cases::box_reference(&case.case, case.reynolds, case.mesh)?.reference(dt, steps)?)
	};
	Ok(
		json!({"status":"classical-reference-executed","benchmark_convergence_established":false,"quantum_execution":false,"reference":data}),
	)
}
fn build(
	case: &CaseOptions,
	args: &Discretization,
) -> Result<(Physical, ConfigurationGrid, HistorySystem, Value), CfdError> {
	let (model, center) = physical(case)?;
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
		args.horizon,
		args.time_cells,
		args.time_order,
		limits,
	)?;
	let (bound, bound_rejection) = match history.spectral_bounds(limits) {
		Ok(b) => (
			Some(
				json!({"lower":b.lower(),"upper":b.upper(),"evidence":"temporal DG1 coercivity and causal inverse bound"}),
			),
			None,
		),
		Err(error) => (None, Some(error.to_string())),
	};
	let report = json!({"status":"construction-only","case":case.case,"full_dg_diagnostics":model.diagnostics(),"independent_coordinates":model.dimension(),"configuration_dimension":grid.dimension(),"configuration_order":args.configuration_order,"configuration_cells_per_axis":args.configuration_cells,"configuration_bounds":grid.bounds(),"configuration_boundary":"central periodic numerical closure; truncation leakage requires refinement","regularization_width":args.regularization_width,"initial_observables":initial_observables,"generator_nonzeros":generator.nnz(),"history_dimension":history.operator().rows(),"history_nonzeros":history.operator().nnz(),"time_order":args.time_order,"horizon":args.horizon,"spectral_bounds":bound,"spectral_bound_rejection":bound_rejection,"normal_equations":false,"quantum_execution":false});
	Ok((model, grid, history, report))
}
fn resource(
	case: &CaseOptions,
	axis: u32,
	time: u64,
	temporal: u32,
	ancillas: usize,
	budget: String,
) -> Result<Value, CfdError> {
	let (local, rank) = if case.case == "smoke" {
		(12, 7)
	} else if !case.case.starts_with("shedding") {
		let manifest = cases::manifest(&case.case)?;
		manifest.viscosity(case.reynolds)?;
		quest_cfd::simplex::box_chart_dimensions(
			manifest.dimension,
			case.mesh,
			case.case.starts_with("tgv"),
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
	let (decimal_estimate, decimal_rejection) = match estimate(&request) {
		Ok(value) => (Some(value), None),
		Err(error) => (None, Some(error.to_string())),
	};
	Ok(
		json!({"case":case.case,"request":request,"estimate":decimal_estimate,"decimal_expansion_rejection":decimal_rejection,"symbolic_estimate":estimate_symbolic(&request)?,"all_independent_coordinates_retained":true,"ancillas":"caller-supplied explicit allowance; requires encoding-specific verification"}),
	)
}
fn run(command: Command) -> Result<Value, CfdError> {
	match command {
		Command::Reference { case, dt, steps } => reference(&case, dt, steps),
		Command::Build {
			case,
			discretization,
		} => Ok(build(&case, &discretization)?.3),
		Command::Estimate {
			case,
			configuration_coefficients,
			time_elements,
			temporal_coefficients,
			auxiliary_qubits,
			statevector_budget,
		} => resource(
			&case,
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
			backend,
		} => {
			let (model, grid, history, construction) = build(&case, &discretization)?;
			let mut result = solve_history_with_backend(
				&history,
				SolveBudget {
					max_bytes: discretization.max_bytes,
					max_degree,
					max_state_query_work,
					relative_residual: residual,
					certify: !no_certify,
				},
				backend,
			)?;
			result.retained_physical_coordinates = Some(model.dimension());
			let final_state = result
				.solution
				.get(result.solution.len().saturating_sub(grid.dimension())..)
				.ok_or(CfdError::Assembly("final history slice"))?;
			let observables = grid.observables(final_state, |x| model.energy(x))?;
			Ok(
				json!({"construction":construction,"solve":result,"final_observables":observables,"physical_convergence_established":false}),
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
