//! Private source factory's coherent-RHS/collective transaction boundary.
#![allow(
	clippy::panic_in_result_fn,
	reason = "Bounded independent collective assertions and child exit checks"
)]
use super::*;
use quest::collective::MpiRuntime;
use std::cell::Cell;

struct ZeroDynamics;
impl HistoryRowDynamics for ZeroDynamics {
	fn dimension(&self) -> usize {
		3
	}
	fn max_row_entries(&self) -> usize {
		0
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(0)
	}
	fn row_query_bytes(&self) -> usize {
		0
	}
	fn row_query_work(&self) -> usize {
		1
	}
	fn visit_row(
		&self,
		_: f64,
		_: usize,
		_: &mut dyn FnMut(usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		Ok(())
	}
	fn source_entry(&self, _: f64, _: usize) -> Result<Complex64, CfdError> {
		Ok(Complex64::new(0., 0.))
	}
}

fn launch(test: &str) -> Result<bool, Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_CFD_FACTORY_CHILD").is_some() {
		return Ok(false);
	}
	for ranks in [1, 2] {
		let status = quest_test_support::mpi::MpiTest::new(
			usize::try_from(ranks)?,
			std::time::Duration::from_secs(90),
		)?
		.args(["--exact", test, "--nocapture", "--test-threads=1"])
		.env("QUEST_CFD_FACTORY_CHILD", "1")
		.status()?;
		assert!(status.success(), "factory test MPI {ranks} failed");
	}
	Ok(true)
}

fn spectrum() -> Result<SpectralBounds, CfdError> {
	Ok(SpectralBounds::new(
		0.5,
		1.,
		SpectralEvidence::CallerPremise {
			description: "factory boundary only, spectral synthesis is not reached".into(),
		},
	)?)
}

#[test]
fn zero_rhs_skips_factory() -> Result<(), Box<dyn std::error::Error>> {
	if launch("distributed_history::factory_tests::zero_rhs_skips_factory")? {
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(quest::MemoryBudget::new(16 * 1024 * 1024))
		.build()?;
	let recipe = TemporalHistoryRecipe::new(
		&ZeroDynamics,
		0.1,
		1,
		1,
		crate::stream_history::HistoryStreamLimits::default(),
	)?;
	let before = env.view().allocated_bytes();
	let invoked = Cell::new(false);
	let outcome = prepare_history_inverse_from_factory(
		&env,
		&recipe,
		|_| Ok(Complex64::new(0., 0.)),
		&spectrum()?,
		DistributedHistoryLimits::default(),
		|_| -> Result<
			std::iter::Empty<quest_numerics::Result<quest_numerics::sparse_stream::SparseEntry>>,
			CfdError,
		> {
			invoked.set(true);
			Err(CfdError::InvalidInput("zero RHS invoked construction"))
		},
	)?;
	assert!(matches!(outcome, DistributedHistoryOutcome::ZeroRhs { .. }));
	assert!(!invoked.get());
	drop(outcome);
	assert_eq!(env.view().allocated_bytes(), before);
	Ok(())
}

#[test]
fn one_rank_factory_failure_rejects_collectively() -> Result<(), Box<dyn std::error::Error>> {
	if launch("distributed_history::factory_tests::one_rank_factory_failure_rejects_collectively")?
	{
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(quest::MemoryBudget::new(16 * 1024 * 1024))
		.build()?;
	let recipe = TemporalHistoryRecipe::new(
		&ZeroDynamics,
		0.1,
		1,
		1,
		crate::stream_history::HistoryStreamLimits::default(),
	)?;
	let before = env.view().allocated_bytes();
	let invoked = Cell::new(false);
	let initial_queries = Cell::new(0usize);
	let outcome = prepare_history_inverse_from_factory(
		&env,
		&recipe,
		|_| {
			initial_queries.set(initial_queries.get().saturating_add(1));
			Ok(Complex64::new(1., 0.))
		},
		&spectrum()?,
		DistributedHistoryLimits::default(),
		|range| {
			invoked.set(true);
			let mut queried = [u8::from(initial_queries.get() > 0)];
			let mut lane = env.communicator().collective_lane().map_err(native)?;
			lane.broadcast_bytes(0, &mut queried).map_err(native)?;
			drop(lane);
			assert_eq!(queried, [1], "factory ran before coherent RHS");
			assert!(range.end <= recipe.dimension());
			if comm.rank().map_err(native)? == comm.size().map_err(native)? - 1 {
				Err(CfdError::InvalidInput("one owner spool failure"))
			} else {
				Ok(std::iter::empty::<
					quest_numerics::Result<quest_numerics::sparse_stream::SparseEntry>,
				>())
			}
		},
	);
	assert!(outcome.is_err(), "one-rank factory failure was not agreed");
	assert!(invoked.get());
	drop(outcome);
	assert_eq!(env.view().allocated_bytes(), before);
	Ok(())
}
