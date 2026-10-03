#![forbid(unsafe_code)]
//! Typed scientific application commands. Native execution and offline synthesis
//! are explicit features; production synthesis never falls back to offline work.
#[cfg(all(feature = "mpi", quest_native_mpi))]
mod distributed;
#[cfg(feature = "native")]
mod execution;
mod synthesis;
mod workers;
pub use workers::Workers;

use clap::{Args, Parser, Subcommand, ValueEnum};
use quest_numerics::observer::{MonotonicClock, Span, Stage, TraceObserver, observe_result};
use quest_qsvt_io::{IoPolicy, QspInput};
use serde_json::{Map, Value, json};
use std::{
	io::Read,
	path::{Path, PathBuf},
	time::Instant,
};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
	#[error(transparent)]
	Io(#[from] std::io::Error),
	#[error(transparent)]
	Json(#[from] serde_json::Error),
	#[error(transparent)]
	Interchange(#[from] quest_qsvt_io::Error),
	#[error(transparent)]
	Qsp(#[from] quest_qsp::Error),
	#[error(transparent)]
	Qsvt(#[from] quest_qsvt::Error),
	#[error(transparent)]
	Circuit(#[from] quest_compile::Error),
	#[cfg(feature = "native")]
	#[error(transparent)]
	Quest(#[from] quest::Error),
	#[cfg(feature = "native")]
	#[error(transparent)]
	Execution(#[from] quest::qsvt::Error),
	#[cfg(feature = "certification")]
	#[error(transparent)]
	Certification(#[from] quest_qsp::certification::CertificationError),
	#[cfg(feature = "certification")]
	#[error(transparent)]
	Artifact(#[from] quest_qsp::artifact::ArtifactError),
	#[cfg(feature = "offline-synthesis")]
	#[error(transparent)]
	Offline(#[from] quest_qsp::offline::OfflineError),
	#[error("invalid application input: {0}")]
	Input(&'static str),
	#[error("application resource limit: {0}")]
	Budget(&'static str),
	#[cfg(feature = "rayon")]
	#[error(transparent)]
	WorkerPool(#[from] rayon::ThreadPoolBuildError),
	#[error("this command requires the {0} feature")]
	Feature(&'static str),
	#[error("{primary}; trace export also failed: {trace}")]
	DispatchAndTrace {
		#[source]
		primary: Box<Self>,
		trace: Box<Self>,
	},
}

/// Command parser and application entrypoint, usable without installing a global error hook.
#[derive(Debug, Parser)]
#[command(
	name = "quest-qsvt",
	about = "QSP synthesis and QSVT scientific applications"
)]
pub struct Cli {
	/// Trace admitted dispatch and its failures/retries; runtime and worker preflight are excluded.
	#[arg(long, global = true)]
	pub trace: Option<PathBuf>,
	/// Worker count or auto (available local parallelism); one is serial.
	#[arg(long, global = true, default_value = "1")]
	pub workers: Workers,
	#[command(subcommand)]
	pub command: Command,
}
#[derive(Debug, Subcommand)]
pub enum Command {
	/// Construct frozen binary64 phases or matrices; certification is a separate stage.
	Synthesize(SynthesisArgs),
	/// Export a reproducible matrix for embedded or solve workflows.
	#[cfg(feature = "hdf5")]
	MatrixPreset(MatrixPresetArgs),
	/// Run explicitly requested arbitrary-precision construction and certify its frozen export.
	#[cfg(feature = "offline-synthesis")]
	OfflineSynthesize(SynthesisArgs),
	/// Inspect or actually synthesize the exact stored inverse-polynomial families.
	Catalog {
		#[command(subcommand)]
		command: CatalogCommand,
	},
	/// Execute an imported block encoding and report subnormalized logical output.
	#[cfg(feature = "native")]
	Embedded(EmbeddedArgs),
	/// Estimate the complex logical overlap with native Hadamard tests.
	#[cfg(feature = "native")]
	Overlap(OverlapArgs),
	/// Apply reciprocal SVT of A adjoint and recover the physical solution scale.
	#[cfg(feature = "native")]
	Solve(SolveArgs),
}
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum SynthesisMode {
	#[default]
	RealParityWx,
	UnitCircleResponse,
}
/// Numerical solver, independently selectable from response convention and precision.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum Algorithm {
	Rhw,
	#[default]
	InverseNlft,
}
impl Algorithm {
	const fn solver(self) -> quest_qsp::SynthesisAlgorithm {
		match self {
			Self::Rhw => quest_qsp::SynthesisAlgorithm::RhwHalfCholesky,
			Self::InverseNlft => quest_qsp::SynthesisAlgorithm::InverseNlftDivideConquer,
		}
	}
	const fn name(self) -> &'static str {
		match self {
			Self::Rhw => "rhw",
			Self::InverseNlft => "inverse-nlft",
		}
	}
}
/// Compiled exports retain target/evidence; sequence exports are explicitly weaker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ExportFormat {
	#[default]
	Compiled,
	Sequence,
}
#[derive(Debug, Clone, Args)]
pub struct SynthesisArgs {
	#[arg(long)]
	pub input: PathBuf,
	#[arg(long)]
	pub output: PathBuf,
	#[arg(long, value_enum, default_value = "real-parity-wx")]
	pub mode: SynthesisMode,
	#[arg(long, value_enum, default_value = "inverse-nlft")]
	pub algorithm: Algorithm,
	#[arg(long = "export", value_enum, default_value = "compiled")]
	pub export: ExportFormat,
	#[arg(long)]
	pub certify: bool,
	#[arg(long, default_value_t = 1e-11)]
	pub tolerance: f64,
}
#[derive(Debug, Subcommand)]
pub enum CatalogCommand {
	List(FamilySelection),
	Check {
		#[command(flatten)]
		family: FamilySelection,
		/// Independently certify every constructed export.
		#[arg(long)]
		certify: bool,
		#[arg(long, default_value_t = 1e-11)]
		tolerance: f64,
		#[arg(long, value_enum, default_value = "inverse-nlft")]
		algorithm: Algorithm,
	},
	/// Construct one exact frozen catalogue family and export its payload.
	Synthesize(CatalogSynthesisArgs),
	/// Apply one exact stored inverse family to a physical linear system.
	#[cfg(feature = "native")]
	Solve(CatalogSolveArgs),
}
#[derive(Debug, Clone, Default, Args)]
pub struct FamilySelection {
	#[arg(long, requires = "epsilon")]
	pub kappa: Option<u32>,
	#[arg(long, requires = "kappa")]
	pub epsilon: Option<f64>,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TransformRoute {
	/// Wx chooses standard; generalized controls choose full Hermitianization.
	Auto,
	Standard,
	Direct,
	HermitianizedFull,
	HermitianizedEven,
	HermitianizedOdd,
	MultiplicationEven,
	MultiplicationOdd,
}
#[derive(Debug, Clone, Args)]
pub struct TransformArgs {
	#[arg(long)]
	pub qsp: PathBuf,
	/// Explicitly synthesize a polynomial input in binary64 before preparation.
	#[arg(long)]
	pub synthesize_input: bool,
	#[arg(long, value_enum, default_value = "inverse-nlft")]
	pub algorithm: Algorithm,
	#[arg(long, requires = "synthesize_input")]
	pub certify_input: bool,
	#[arg(long, default_value_t = 1e-11)]
	pub input_tolerance: f64,
	/// Route and coefficient meaning; auto is deterministic from convention.
	#[arg(long, value_enum, default_value = "auto")]
	pub route: TransformRoute,
}
#[derive(Debug, Clone, Args)]
pub struct EmbeddedArgs {
	/// Collective MPI execution, reporting masses without gathering a state.
	#[arg(long)]
	pub distributed: bool,
	#[arg(long, required_unless_present = "matrix", conflicts_with = "matrix")]
	pub encoding: Option<PathBuf>,
	/// Construct a dense Julia dilation of this logical matrix.
	#[arg(long, requires = "alpha", conflicts_with = "distributed")]
	pub matrix: Option<PathBuf>,
	/// Explicit physical normalization for --matrix.
	#[arg(long, requires = "matrix")]
	pub alpha: Option<f64>,
	#[command(flatten)]
	pub transform: TransformArgs,
	#[arg(long)]
	pub input_state: PathBuf,
	/// Subnormalized decoded logical amplitudes (local execution only).
	#[arg(
        long,
        required_unless_present_any = ["distributed", "physical_output_state"],
        conflicts_with = "distributed"
    )]
	pub output_state: Option<PathBuf>,
	#[arg(long, conflicts_with = "distributed")]
	pub normalized_output_state: Option<PathBuf>,
	/// Interpret input as the complete physical register, including response/auxiliary qubits.
	#[arg(long, conflicts_with = "distributed")]
	pub physical_input: bool,
	/// Write subnormalized postselected physical-register amplitudes.
	#[arg(long, conflicts_with = "distributed")]
	pub physical_output_state: Option<PathBuf>,
}
#[derive(Debug, Clone, Args)]
pub struct OverlapArgs {
	#[arg(long)]
	pub distributed: bool,
	#[arg(long, required_unless_present = "matrix", conflicts_with = "matrix")]
	pub encoding: Option<PathBuf>,
	/// Construct a dense Julia dilation of this logical matrix.
	#[arg(long, requires = "alpha", conflicts_with = "distributed")]
	pub matrix: Option<PathBuf>,
	/// Explicit physical normalization for --matrix.
	#[arg(long, requires = "matrix")]
	pub alpha: Option<f64>,
	#[command(flatten)]
	pub transform: TransformArgs,
	#[arg(long)]
	pub input_state: PathBuf,
	#[arg(long)]
	pub reference_state: PathBuf,
}
#[derive(Debug, Clone, Args)]
pub struct SolveArgs {
	#[arg(long)]
	pub matrix: PathBuf,
	#[arg(long)]
	pub rhs: PathBuf,
	#[command(flatten)]
	pub transform: TransformArgs,
	/// Explicit premise that the induced singular-value response approximates this scale divided by x.
	#[arg(long)]
	pub reciprocal_scale: f64,
	/// Physical x, including the recovered norm; never silently normalized.
	#[arg(long)]
	pub output_state: PathBuf,
	#[arg(long)]
	pub normalized_output_state: Option<PathBuf>,
	/// Fail if the independently evaluated relative physical residual exceeds this value.
	#[arg(long)]
	pub residual_tolerance: Option<f64>,
}

/// Export an exact catalogue family using an explicitly selected algorithm.
#[derive(Debug, Clone, Args)]
pub struct CatalogSynthesisArgs {
	#[arg(long)]
	pub kappa: u32,
	#[arg(long)]
	pub epsilon: f64,
	#[arg(long)]
	pub output: PathBuf,
	#[arg(long, value_enum, default_value = "inverse-nlft")]
	pub algorithm: Algorithm,
	#[arg(long = "export", value_enum, default_value = "compiled")]
	pub export: ExportFormat,
	#[arg(long)]
	pub certify: bool,
	#[arg(long, default_value_t = 1e-11)]
	pub tolerance: f64,
}
#[cfg(feature = "native")]
#[derive(Debug, Clone, Args)]
pub struct CatalogSolveArgs {
	#[arg(long)]
	pub kappa: u32,
	#[arg(long)]
	pub epsilon: f64,
	#[arg(long)]
	pub matrix: PathBuf,
	#[arg(long)]
	pub rhs: PathBuf,
	#[arg(long)]
	pub output_state: PathBuf,
	#[arg(long)]
	pub normalized_output_state: Option<PathBuf>,
	#[arg(long)]
	pub residual_tolerance: Option<f64>,
	#[arg(long, value_enum, default_value = "inverse-nlft")]
	pub algorithm: Algorithm,
	#[arg(long)]
	pub certify: bool,
	#[arg(long, default_value_t = 1e-11)]
	pub tolerance: f64,
}
#[cfg(feature = "hdf5")]
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MatrixPreset {
	Diagonal,
	Hermitian,
	General,
}
/// Deterministic, seeded matrix fixtures, exported in the normal interchange format.
#[cfg(feature = "hdf5")]
#[derive(Debug, Clone, Args)]
pub struct MatrixPresetArgs {
	#[arg(long, value_enum)]
	pub preset: MatrixPreset,
	#[arg(long)]
	pub rows: usize,
	#[arg(long)]
	pub cols: usize,
	#[arg(long, default_value_t = 0)]
	pub seed: u64,
	#[arg(long, default_value_t = 1.0)]
	pub scale: f64,
	#[arg(long)]
	pub output: PathBuf,
}

struct Context<'a> {
	timings: Map<String, Value>,
	trace: &'a mut TraceObserver,
	clock: &'a MonotonicClock,
	output_rank: bool,
	execution: quest_numerics::ExecutionPolicy<'a>,
	#[cfg(feature = "rayon")]
	pool: Option<&'a rayon::ThreadPool>,
}
impl Context<'_> {
	fn measure<T>(
		&mut self,
		name: &str,
		stage: Stage,
		operation: impl FnOnce() -> Result<T>,
	) -> Result<T> {
		let start = Instant::now();
		let result = observe_result(self.trace, self.clock, stage, operation);
		let elapsed = start.elapsed().as_secs_f64();
		let previous = self
			.timings
			.get(name)
			.and_then(Value::as_f64)
			.unwrap_or(0.0);
		self.timings.insert(name.into(), json!(previous + elapsed));
		result
	}
}
impl Cli {
	/// Execute the selected application, returning machine-readable scientific evidence.
	/// # Errors
	/// Preserves typed admission, numerical, native and IO failures; no fallback is attempted.
	pub fn run(self) -> Result<Value> {
		let start = Instant::now();
		#[cfg(all(feature = "mpi", quest_native_mpi))]
		if distributed_command(&self.command) {
			return distributed::application(self, start);
		}
		let (result, teardown_started) = {
			let pool_started = Instant::now();
			let pool = workers::Pool::build(self.workers, &self.command)?;
			let pool_seconds = pool_started.elapsed().as_secs_f64();
			let result = self.run_scoped(&pool, |command, context| {
				context
					.timings
					.insert("worker_pool".into(), json!(pool_seconds));
				dispatch(command, context)
			});
			(result, Instant::now())
		};
		finalize_timing(result, start, teardown_started, "worker_pool_teardown")
	}

	fn run_scoped(
		self,
		pool: &workers::Pool,
		execute: impl FnOnce(Command, &mut Context<'_>) -> Result<Value>,
	) -> Result<Value> {
		let workers = pool.count();
		let scope = if workers > 1 {
			worker_scope(&self.command).ok_or(Error::Input("invalid worker scope"))?
		} else {
			"serial"
		};
		let clock = MonotonicClock::new();
		let mut trace = TraceObserver::new(4096);
		let mut span = Span::new(&mut trace, &clock, Stage::ApplicationDispatch);
		let mut context = Context {
			timings: Map::new(),
			trace: span.observer_mut(),
			clock: &clock,
			output_rank: true,
			execution: {
				#[cfg(feature = "rayon")]
				{
					pool.inner().map_or(
						quest_numerics::ExecutionPolicy::Sequential,
						quest_numerics::ExecutionPolicy::Rayon,
					)
				}
				#[cfg(not(feature = "rayon"))]
				{
					quest_numerics::ExecutionPolicy::Sequential
				}
			},
			#[cfg(feature = "rayon")]
			pool: pool.inner(),
		};
		let result = execute(self.command, &mut context);
		let output_rank = context.output_rank;
		let timings = context.timings;
		if result.is_ok() {
			span.finish();
		} else {
			span.fail();
		}
		let trace_result = self.trace.filter(|_| output_rank).map_or_else(
			|| Ok(()),
			|path| {
				trace
					.to_chrome_json()
					.map_err(Error::from)
					.and_then(|json| std::fs::write(path, json).map_err(Error::from))
			},
		);
		let result = match (result, trace_result) {
			(Err(primary), Err(trace)) => Err(Error::DispatchAndTrace {
				primary: Box::new(primary),
				trace: Box::new(trace),
			}),
			(Err(primary), _) => Err(primary),
			(Ok(_), Err(trace)) => Err(trace),
			(Ok(value), Ok(())) => Ok(value),
		};
		result.and_then(|mut value| {
            set(&mut value,"emit_report",json!(output_rank))?;
            set(&mut value, "timings_seconds", Value::Object(timings))?;
            set(&mut value, "parallelism", json!({"workers":workers, "selection":if self.workers == Workers::Auto { "auto" } else { "explicit" }, "scope":scope, "certification":"serial within each target", "per_family_budgets":matches!(scope,"independent catalogue families")}))?;
            Ok(value)
        })
	}
}
fn dispatch(command: Command, context: &mut Context<'_>) -> Result<Value> {
	match command {
		Command::Synthesize(args) => synthesis::run(&args, context, false),
		#[cfg(feature = "hdf5")]
		Command::MatrixPreset(args) => synthesis::matrix_preset(&args, context),
		#[cfg(feature = "offline-synthesis")]
		Command::OfflineSynthesize(args) => synthesis::run(&args, context, true),
		Command::Catalog { command } => synthesis::catalog(command, context),
		#[cfg(feature = "native")]
		Command::Embedded(args) => execution::embedded(&args, context),
		#[cfg(feature = "native")]
		Command::Overlap(args) => execution::overlap(&args, context),
		#[cfg(feature = "native")]
		Command::Solve(args) => execution::solve(&args, context),
	}
}
fn read_qsp(path: &Path) -> Result<QspInput> {
	read_qsp_with_tolerance(path, 1e-11)
}
fn read_qsp_with_tolerance(path: &Path, tolerance: f64) -> Result<QspInput> {
	let policy = IoPolicy::default();
	let limit = u64::try_from(policy.max_bytes).map_err(|_| Error::Budget("JSON length"))?;
	let mut text = String::new();
	std::fs::File::open(path)?
		.take(limit.saturating_add(1))
		.read_to_string(&mut text)?;
	Ok(quest_qsvt_io::read_qsp_json_with_tolerance(
		&text, policy, tolerance,
	)?)
}

fn set(object: &mut Value, key: &str, value: Value) -> Result<()> {
	object
		.as_object_mut()
		.ok_or(Error::Input("report must be an object"))?
		.insert(key.into(), value);
	Ok(())
}

const fn worker_scope(command: &Command) -> Option<&'static str> {
	match command {
		Command::Catalog {
			command: CatalogCommand::Check { .. },
		} => Some("independent catalogue families"),
		Command::Synthesize(_)
		| Command::Catalog {
			command: CatalogCommand::Synthesize(_),
		} => Some("binary64 completion and synthesis"),
		#[cfg(feature = "native")]
		Command::Catalog {
			command: CatalogCommand::Solve(_),
		} => Some(
			"binary64 input completion and synthesis; SVD and native execution stay sequential",
		),
		#[cfg(feature = "native")]
		Command::Embedded(args) if args.transform.synthesize_input => Some(
			"binary64 input completion and synthesis; native execution stays on the caller thread",
		),
		#[cfg(feature = "native")]
		Command::Overlap(args) if args.transform.synthesize_input => Some(
			"binary64 input completion and synthesis; native execution stays on the caller thread",
		),
		#[cfg(feature = "native")]
		Command::Solve(args) if args.transform.synthesize_input => Some(
			"binary64 input completion and synthesis; SVD and native execution stay sequential",
		),
		_ => None,
	}
}

#[cfg(all(feature = "mpi", quest_native_mpi))]
const fn distributed_command(command: &Command) -> bool {
	match command {
		Command::Embedded(args) => args.distributed,
		Command::Overlap(args) => args.distributed,
		_ => false,
	}
}

fn finalize_timing(
	result: Result<Value>,
	start: Instant,
	teardown_started: Instant,
	teardown_stage: &str,
) -> Result<Value> {
	let teardown = teardown_started.elapsed().as_secs_f64();
	let total = start.elapsed().as_secs_f64();
	result.and_then(|mut value| {
		let timings = value
			.get_mut("timings_seconds")
			.ok_or(Error::Input("missing application timings"))?;
		set(timings, teardown_stage, json!(teardown))?;
		set(timings, "total", json!(total))?;
		Ok(value)
	})
}
