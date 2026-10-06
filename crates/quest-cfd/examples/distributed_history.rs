//! Streamed Burgers/Carleman temporal history; no stored history/reference fallback.
use clap::Parser;
use quest::{
	MemoryBudget,
	collective::{CollectiveEnvironment, MpiRuntime},
};
use quest_cfd::{
	burgers::BurgersDg,
	carleman_recipe::{CarlemanRecipeLimits, StatelessCarleman},
	distributed_history::{
		DistributedHistoryLimits, DistributedHistoryOutcome, prepare_history_inverse,
	},
	stream_history::{HistoryStreamLimits, TemporalHistoryRecipe},
};
use quest_qsvt::reciprocal::{SpectralBounds, SpectralEvidence};
#[derive(Parser)]
struct Args {
	#[arg(long, default_value_t = 2)]
	physical_cells: u32,
	#[arg(long, default_value_t = 1)]
	lift_order: usize,
	#[arg(long, default_value_t = 1)]
	time_cells: usize,
	#[arg(long, default_value_t = 1)]
	temporal_order: usize,
	#[arg(long, default_value_t = 0.01)]
	horizon: f64,
	/// Explicit caller premise for this exact complete lifted temporal matrix.
	#[arg(long)]
	spectral_lower: f64,
	/// Explicit caller premise; no dense reference is run by this executable.
	#[arg(long)]
	spectral_upper: f64,
	#[arg(long, default_value_t = 2047)]
	max_degree: usize,
	#[arg(long, default_value_t = 1e-3)]
	approximation_tolerance: f64,
	#[arg(long, default_value_t = false)]
	uncertified_phases: bool,
	/// Forward repetitions on the same prepared owner, with literal adjoints between them.
	#[arg(long, default_value_t = 1)]
	inverse_repetitions: usize,
}
#[allow(
	clippy::too_many_lines,
	reason = "Complete small executable workflow with explicit receipts"
)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
	let args = Args::parse();
	if args.inverse_repetitions == 0 {
		return Err("inverse repetitions must be positive".into());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(32 * 1024 * 1024))
		.build()?;
	let source_started = std::time::Instant::now();
	let physical = BurgersDg::new(args.physical_cells, 1, 0.1)?;
	let initial = physical.initial_state(0.001)?;
	let hierarchy = StatelessCarleman::new(
		physical.polynomial_ode(),
		args.lift_order,
		0.1,
		CarlemanRecipeLimits::default(),
	)?;
	drop(physical);
	let recipe = TemporalHistoryRecipe::new(
		&hierarchy,
		args.horizon,
		args.time_cells,
		args.temporal_order,
		HistoryStreamLimits::default(),
	)?;
	let source_seconds = source_started.elapsed().as_secs_f64();
	let spectrum=SpectralBounds::new(args.spectral_lower,args.spectral_upper,SpectralEvidence::CallerPremise{description:"command-line premise for the complete Burgers/Carleman temporal history; independently validate before use".into()})?;
	let defaults = DistributedHistoryLimits::default();
	let limits = DistributedHistoryLimits {
		max_degree: args.max_degree,
		approximation_tolerance: args.approximation_tolerance,
		initial_retained_bytes: initial
			.capacity()
			.checked_mul(size_of::<f64>())
			.ok_or("initial byte overflow")?,
		initial_query_bytes: hierarchy.resources().query_bytes,
		initial_query_work: hierarchy
			.resources()
			.index_work
			.checked_add(
				hierarchy
					.physical_dimension()
					.checked_mul(8)
					.ok_or("initial query work overflow")?,
			)
			.ok_or("initial query work overflow")?,
		readout_query_work: hierarchy
			.physical_dimension()
			.checked_add(8)
			.ok_or("readout query work overflow")?,
		certification: if args.uncertified_phases {
			None
		} else {
			defaults.certification
		},
		..defaults
	};
	let outcome = prepare_history_inverse(
		&env,
		&recipe,
		|i| Ok(hierarchy.lift_entry(i, &initial)?.into()),
		&spectrum,
		limits,
	)?;
	let DistributedHistoryOutcome::Prepared(mut prepared) = outcome else {
		if comm.rank()? == 0 {
			println!("zero RHS: no normalized state, inverse production or quantum mutation");
		}
		return Ok(());
	};
	let mut state = env.state_vector_local(prepared.qubit_count())?;
	prepared.initialize_rhs(&mut state)?;
	for repetition in 0..args.inverse_repetitions {
		prepared.apply_inverse(&mut state, false)?;
		if repetition != args.inverse_repetitions.saturating_sub(1) {
			prepared.apply_inverse(&mut state, true)?;
		}
	}
	let final_block = args
		.time_cells
		.checked_mul(
			args.temporal_order
				.checked_add(1)
				.ok_or("temporal order overflow")?,
		)
		.and_then(|v| v.checked_sub(1))
		.ok_or("time selection overflow")?;
	let reduction = prepared.reduce_observable(&state, |index| {
		let row = index
			.checked_rem(hierarchy.dimension())
			.ok_or(quest_cfd::CfdError::InvalidInput("lift dimension"))?;
		let degree_one = row < hierarchy.physical_dimension();
		let block = index
			.checked_div(hierarchy.dimension())
			.ok_or(quest_cfd::CfdError::InvalidInput("lift dimension"))?;
		Ok((
			block == final_block && degree_one,
			quest_numerics::Complex64::new(hierarchy.scale(), 0.0),
		))
	})?;
	if comm.rank()? == 0 {
		println!("physical/stateless source preparation seconds: {source_seconds}");
		println!("stateless Carleman source: {:?}", hierarchy.resources());
		println!(
			"streamed CPU/MPI inverse; caller spectral premise; finite Carleman order; no CFD convergence certificate"
		);
		println!("{:?}", prepared.report());
		println!(
			"final-time degree-one observable (mass-weighted physical coordinate sum): {reduction:?}"
		);
	}
	Ok(())
}
