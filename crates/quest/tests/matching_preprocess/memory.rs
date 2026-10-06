//! Live producer payload admission across nonoverlapping temporary lifetimes.
use super::{Complex64, MpiRuntime, ProducerLimits, SparseEntry, TestResult, produce_matching};

fn source(
	n: usize,
	rank: usize,
	parts: usize,
) -> impl Iterator<Item = quest_numerics::Result<SparseEntry>> {
	(rank..n).step_by(parts).flat_map(|column| {
		[
			(column, Complex64::new(0.7, 0.1)),
			(column ^ 1, Complex64::new(-0.2, 0.05)),
		]
		.into_iter()
		.enumerate()
		.map(move |(slot, (row, value))| {
			Ok(SparseEntry {
				row,
				column,
				ordinal: u64::try_from(
					column
						.checked_mul(2)
						.and_then(|v| v.checked_add(slot))
						.ok_or(quest_numerics::Error::Overflow)?,
				)
				.map_err(|_| quest_numerics::Error::Overflow)?,
				value,
			})
		})
	})
}

#[test]
fn producer_budget_follows_live_buffers_and_preserves_collective_recovery() -> TestResult {
	if std::env::var("QUEST_PRODUCER_MEMORY_CHILD").is_err() {
		for ranks in [1, 2, 4, 8] {
			let output =
				quest_test_support::mpi::MpiTest::new(ranks, std::time::Duration::from_secs(90))?
					.args([
						"--exact",
						"memory::producer_budget_follows_live_buffers_and_preserves_collective_recovery",
						"--nocapture",
						"--test-threads=1",
					])
					.env("QUEST_PRODUCER_MEMORY_CHILD", "1")
					.output()?;
			assert!(
				output.status.success(),
				"ranks={ranks}: {}\n{}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let rank = usize::try_from(comm.rank()?)?;
	let n = 256usize.checked_mul(parts).ok_or(quest::Error::Overflow)?;
	let mut limits = ProducerLimits {
		batch_entries: 4,
		max_local_edges: 512,
		max_endpoint_records: 1024,
		max_bytes: 512 * 1024,
		..ProducerLimits::default()
	};
	limits.stream.buffer_entries = 512;
	limits.stream.max_entries = 512;
	limits.stream.max_bytes = 24 * 1024;
	let reference = produce_matching(&comm, n, n, source(n, rank, parts), limits.clone())?;
	let identity = reference.shard().header();
	let columns = reference.shard().records().to_vec();
	let edges = reference.edges().to_vec();
	let inverse = reference.reverse().to_vec();
	drop(reference);
	// 160 KiB holds the real simultaneously live buffers, including old/new
	// allocations during growth. The previous cumulative ledger exceeds 250 KiB.
	limits.max_bytes = 160 * 1024;
	let bounded = produce_matching(&comm, n, n, source(n, rank, parts), limits.clone())?;
	assert!(bounded.statistics().peak_managed_bytes <= limits.max_bytes);
	assert!(bounded.retained_bytes()? < bounded.statistics().peak_managed_bytes);
	assert_eq!(bounded.shard().header(), identity);
	assert_eq!(bounded.shard().records(), columns);
	assert_eq!(bounded.edges(), edges);
	assert_eq!(bounded.reverse(), inverse);
	drop(bounded);
	let mut insufficient = limits.clone();
	insufficient.max_bytes = 28 * 1024;
	assert!(produce_matching(&comm, n, n, source(n, rank, parts), insufficient).is_err());
	let malformed = source(n, rank, parts).enumerate().map(|(i, entry)| {
		entry.map(|mut entry| {
			if rank == 0 && i == 0 {
				entry.row = n;
			}
			entry
		})
	});
	assert!(produce_matching(&comm, n, n, malformed, limits.clone()).is_err());
	let recovered = produce_matching(&comm, n, n, source(n, rank, parts), limits)?;
	assert_eq!(recovered.shard().header(), identity);
	Ok(())
}
