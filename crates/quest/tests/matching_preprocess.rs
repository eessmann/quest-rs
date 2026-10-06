#![cfg(all(feature = "qsvt", feature = "mpi", quest_native_mpi))]
#![allow(
	clippy::panic_in_result_fn,
	reason = "Assertions express collective regression outcomes after fallible setup"
)]
use quest::collective::MpiRuntime;
use quest::qsvt::matching::preprocess::{ProducerLimits, produce_matching};
use quest_numerics::{
	Complex64,
	sparse_stream::{LocalCsr, SparseEntry},
};
#[path = "matching_preprocess/memory.rs"]
mod memory;
#[path = "matching_preprocess/portable.rs"]
mod portable;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
#[test]
#[allow(
	clippy::too_many_lines,
	reason = "One MPI runtime lifetime covers setup, rank-local production and recoverable collective failures"
)]
fn producer_starts_with_sharded_entries_and_closes_paths() -> TestResult {
	if std::env::var("QUEST_PREPROCESS_CHILD").is_err() {
		let mut identity = None;
		for (ranks, split, batch) in [(1, 0, 1), (2, 0, 3), (4, 0, 1), (8, 0, 3), (4, 1, 2)] {
			let output = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(90),
			)?
			.args([
				"--exact",
				"producer_starts_with_sharded_entries_and_closes_paths",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_PREPROCESS_CHILD", "1")
			.env("QUEST_PREPROCESS_SPLIT", split.to_string())
			.env("QUEST_PREPROCESS_BATCH", batch.to_string())
			.output()?;
			let stdout = String::from_utf8(output.stdout)?;
			let stderr = String::from_utf8(output.stderr)?;
			assert!(
				output.status.success(),
				"MPI ranks={ranks} split={split}: {stdout}\n{stderr}"
			);
			let mut receipts: Vec<_> = stdout
				.lines()
				.filter_map(|line| {
					line.find("PRODUCER_IDENTITY ")
						.map(|i| line.get(i..).unwrap_or_default().to_owned())
				})
				.collect();
			receipts.sort();
			receipts.dedup();
			assert_eq!(receipts.len(), 1);
			eprintln!(
				"MPI producer acceptance ranks={ranks} split={split} batch={batch} {receipts:?}"
			);
			if let Some(expected) = &identity {
				assert_eq!(&receipts, expected);
			} else {
				identity = Some(receipts);
			}
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_PREPROCESS_SPLIT").as_deref() == Ok("1") {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let batch = std::env::var("QUEST_PREPROCESS_BATCH")?.parse::<usize>()?;
	let limits = ProducerLimits {
		batch_entries: batch,
		..ProducerLimits::default()
	};
	// Each entry is constructed ONLY on its origin rank. No full-source constructor.
	let source = (0..6_u64)
		.rev()
		.filter(move |&ordinal| {
			usize::try_from(ordinal).is_ok_and(|n| n.checked_rem(parts) == Some(rank))
		})
		.map(|ordinal| {
			let (row, column, re) = match ordinal {
				0 => (1, 0, 1e16),
				1 => (1, 0, -1e16),
				2 => (1, 0, 1.0),
				3 => (2, 1, 2.0),
				4 => (0, 2, -3.0),
				_ => (0, 3, 4.0),
			};
			Ok(SparseEntry {
				row,
				column,
				ordinal,
				value: Complex64::new(re, 0.0),
			})
		});
	let result = produce_matching(&comm, 3, 4, source, limits.clone())?;
	assert_eq!(result.statistics().global_edges, 4);
	assert_eq!(result.shard().header().num_colors, 2);
	assert_eq!(result.shard().header().beta, 4.0);
	for edge in result.edges() {
		assert_eq!(edge.column.checked_rem(parts), Some(rank));
	}
	for inverse in result.reverse() {
		assert_eq!(inverse.destination.checked_rem(parts), Some(rank));
	}
	assert_eq!(
		result
			.edges()
			.iter()
			.find(|e| e.row == 1 && e.column == 0)
			.map(|e| e.value.re),
		if rank == 0 { Some(1.0) } else { None }
	);
	// Conjugate sparse operators must not alias through commutative record hashes.
	let mut identities = Vec::new();
	for (imaginary, reverse) in [(1.0, false), (-1.0, false), (1.0, true)] {
		let mut entries = Vec::new();
		for row in 0..4_usize {
			for (column, value, ordinal) in [
				(row, Complex64::new(1.0, 0.0), 2 * row),
				(row ^ 1, Complex64::new(0.0, imaginary), 2 * row + 1),
			] {
				if ordinal % parts == rank {
					entries.push(Ok(SparseEntry {
						row,
						column,
						ordinal: u64::try_from(ordinal)?,
						value,
					}));
				}
			}
		}
		if reverse {
			entries.reverse();
		}
		let produced = produce_matching(&comm, 4, 4, entries, limits.clone())?;
		identities.push((
			produced.shard().header().source_identity,
			produced.shard().header().record_digest,
		));
	}
	let first = identities.first().ok_or(quest::Error::Overflow)?;
	let conjugate = identities.get(1).ok_or(quest::Error::Overflow)?;
	assert_ne!(first.0, conjugate.0);
	assert_ne!(first.1, conjugate.1);
	assert_eq!(identities.first(), identities.get(2));
	// Adversarial endpoint conflicts require three rounds, padded to a dummy fourth label.
	let source = (10..14_u64)
		.rev()
		.filter(move |&ordinal| {
			usize::try_from(ordinal).is_ok_and(|n| n.checked_rem(parts) == Some(rank))
		})
		.map(|ordinal| {
			let (row, column, value) = match ordinal {
				10 => (0, 0, Complex64::new(1.0, 1.0)),
				11 => (0, 1, Complex64::new(-1.0, 0.0)),
				12 => (0, 2, Complex64::new(0.0, 2.0)),
				_ => (1, 0, Complex64::new(0.2, 0.0)),
			};
			Ok(SparseEntry {
				row,
				column,
				ordinal,
				value,
			})
		});
	let complex = produce_matching(&comm, 2, 3, source, limits.clone())?;
	assert_eq!(complex.shard().header().num_colors, 4);
	assert_eq!(complex.statistics().coloring_rounds, 3);
	portable::check(
		&comm,
		&complex,
		&[(0, 0, 0), (0, 1, 1), (0, 2, 2), (1, 0, 1)],
	)?;
	if rank == 0 {
		println!(
			"PRODUCER_IDENTITY {}:{}:{}:{}",
			result.shard().header().source_identity,
			result.shard().header().record_digest,
			complex.shard().header().source_identity,
			complex.shard().header().record_digest
		);
	}
	// A long path and an existing cycle: no global permutation array, only touched records.
	let source = (0..4_u64)
		.filter(move |&ordinal| {
			usize::try_from(ordinal).is_ok_and(|n| n.checked_rem(parts) == Some(rank))
		})
		.map(|ordinal| {
			let (row, column) = match ordinal {
				0 => (1, 0),
				1 => (2, 1),
				2 => (4, 3),
				_ => (3, 4),
			};
			Ok(SparseEntry {
				row,
				column,
				ordinal,
				value: Complex64::new(1.0, 0.0),
			})
		});
	let paths = produce_matching(&comm, 5, 7, source, limits.clone())?;
	assert_eq!(paths.shard().header().record_count, 5);
	for column in paths.shard().records() {
		let expected = match column.source {
			0 => 1,
			1 => 2,
			2 => 0,
			3 => 4,
			4 => 3,
			_ => usize::MAX,
		};
		assert_eq!(column.destination, expected);
	}
	// Borrowed local CSR uses only the current rank's rows and ordinal slice.
	let row = rank;
	let values = [Complex64::new(1.0, 0.0)];
	let ordinals = [u64::try_from(rank)?];
	let rows = [row];
	let cols = [row];
	let pointers = [0, 1];
	let csr = LocalCsr::new(parts, parts, &rows, &cols, &values, &ordinals, &pointers)?;
	let diagonal = produce_matching(&comm, parts, parts, csr.entries(), limits.clone())?;
	assert_eq!(diagonal.statistics().global_edges, parts);
	failures(&comm, rank, &limits)?;
	Ok(())
}
fn failures(
	comm: &quest_sys::mpi::MpiCommunicator<'_>,
	rank: usize,
	limits: &ProducerLimits,
) -> TestResult {
	let entry = SparseEntry {
		row: 0,
		column: 0,
		ordinal: 0,
		value: Complex64::new(1.0, 0.0),
	};
	// A single malformed participant must reject collectively, then the lane remains usable.
	let malformed = if rank == 0 {
		vec![Ok(SparseEntry { row: 1, ..entry })]
	} else {
		vec![]
	};
	assert!(produce_matching(comm, 1, 1, malformed, limits.clone()).is_err());
	let collision = if rank == 0 {
		vec![Ok(entry), Ok(entry)]
	} else {
		vec![]
	};
	assert!(produce_matching(comm, 1, 1, collision, limits.clone()).is_err());
	for rejected in [
		ProducerLimits {
			max_vertex_degree: 1,
			..limits.clone()
		},
		ProducerLimits {
			max_rounds: 0,
			..limits.clone()
		},
		ProducerLimits {
			max_probes: 0,
			..limits.clone()
		},
		ProducerLimits {
			max_completion_rounds: 0,
			..limits.clone()
		},
		ProducerLimits {
			max_endpoint_records: 0,
			..limits.clone()
		},
		ProducerLimits {
			max_work: 0,
			..limits.clone()
		},
		ProducerLimits {
			max_local_edges: 0,
			..limits.clone()
		},
		ProducerLimits {
			max_bytes: 0,
			..limits.clone()
		},
	] {
		let input = if rank == 0 {
			vec![
				Ok(SparseEntry { row: 1, ..entry }),
				Ok(SparseEntry {
					row: 1,
					column: 1,
					ordinal: 1,
					..entry
				}),
			]
		} else {
			vec![]
		};
		assert!(produce_matching(comm, 2, 2, input, rejected).is_err());
	}
	if comm.size()? > 1 {
		let input = if rank == 0 {
			vec![Ok(SparseEntry { row: 1, ..entry })]
		} else {
			vec![]
		};
		assert!(
			produce_matching(
				comm,
				2,
				1,
				input,
				ProducerLimits {
					max_communication_bytes: 0,
					..limits.clone()
				}
			)
			.is_err()
		);
	}
	if comm.size()? > 1 {
		let divergent = if rank == 0 {
			ProducerLimits {
				max_work: 1,
				..limits.clone()
			}
		} else {
			limits.clone()
		};
		assert!(produce_matching(comm, 1, 1, std::iter::empty(), divergent).is_err());
	}
	check_zero(comm, limits)
}
fn check_zero(comm: &quest_sys::mpi::MpiCommunicator<'_>, limits: &ProducerLimits) -> TestResult {
	let zero = produce_matching(comm, 3, 7, std::iter::empty(), limits.clone())?;
	assert_eq!(zero.shard().header().num_colors, 1);
	assert_eq!(zero.shard().header().beta, 1.0);
	assert_eq!(zero.shard().header().alpha, 1.0);
	assert_eq!(zero.shard().records(), []);
	assert_eq!(zero.edges(), []);
	assert_eq!(zero.reverse(), []);
	Ok(())
}
