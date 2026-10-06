//! Real bounded temporary-capacity injection, only in the library test executable.
use super::*;
#[test]
#[allow(
	clippy::panic_in_result_fn,
	reason = "Bounded MPI capacity regression asserts common rejection and unchanged amplitudes/accounting"
)]
fn incoming_actual_capacity_rejects_before_validation_protocol()
-> std::result::Result<(), Box<dyn std::error::Error>> {
	if std::env::var("QUEST_MATCHING_VALIDATION_CHILD").is_err() {
		for parts in [1, 2] {
			let output=quest_test_support::mpi::MpiTest::new(usize::try_from(parts)?, std::time::Duration::from_secs(30))?.args(["--exact","qsvt::matching::collective::preparation_tests::incoming_actual_capacity_rejects_before_validation_protocol","--nocapture","--test-threads=1"]).env("QUEST_MATCHING_VALIDATION_CHILD","1").output()?;
			assert!(
				output.status.success(),
				"{parts}: {} {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
		}
		return Ok(());
	}
	let runtime = crate::collective::MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = world.duplicate()?;
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let environment = CollectiveEnvironment::builder(&comm)?
		.memory_budget(crate::MemoryBudget::new(65536))
		.build()?;
	let mut register = environment.state_vector_local(QubitCount::new(2)?)?;
	register.init_zero()?;
	let initial = register.read_local_amplitudes(0, register.deployment().local_amplitudes())?;
	let baseline = environment.view().allocated_bytes();
	let policy = quest_qsvt::NumericalPolicy::default();
	let sparse = quest_numerics::SparseMatrix::from_triplets(
		2,
		2,
		quest_numerics::SparseFormat::Csr,
		vec![
			(0, 0, Complex64::new(1.0, 0.0)),
			(1, 1, Complex64::new(1.0, 0.0)),
		],
		quest_numerics::SparseLimits::default(),
	)?;
	let encoding = quest_qsvt::MatchingEncoding::from_sparse(&sparse, policy)?;
	let shard = MatchingShard::from_encoding(&encoding, rank, parts, policy)?;
	assert!(
		environment
			.prepare_matching_with_capacity(
				shard,
				QubitCount::new(2)?,
				vec![0, 1],
				RoutingCapacity {
					ranks_per_node: parts,
					node_budget: crate::MemoryBudget::new(131_072)
				}
			)
			.is_err()
	);
	assert_eq!(environment.view().allocated_bytes(), baseline);
	assert_eq!(register.read_local_amplitudes(0, initial.len())?, initial);
	Ok(())
}
pub(super) fn inject_capacity(
	incoming: &mut Vec<(usize, usize)>,
	rank: usize,
) -> crate::Result<()> {
	if rank == 0 && std::env::var("QUEST_MATCHING_VALIDATION_CHILD").is_ok() {
		incoming
			.try_reserve_exact(8192)
			.map_err(|_| crate::Error::Allocation)?;
	}
	Ok(())
}
