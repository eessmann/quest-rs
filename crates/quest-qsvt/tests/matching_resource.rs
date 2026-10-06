#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Bounded whole-register independent portable references"
)]
use quest_qsvt::{
	Complex64, MatchingEncoding, MatchingHeader, MatchingShard, NumericalPolicy, ReplayEncoding,
	matching_resource::{
		MatchingResource, ResourceMatchingEncoding, ResourceMatchingRecord, ResourceReplayLimits,
	},
};
#[derive(Debug)]
struct Directory {
	header: MatchingHeader,
	records: Vec<ResourceMatchingRecord>,
}
impl MatchingResource for Directory {
	fn header(&self) -> MatchingHeader {
		self.header
	}
	fn frozen_identity(&self) -> u64 {
		17
	}
	fn retained_bytes(&self) -> quest_qsvt::Result<usize> {
		Ok(self.records.capacity() * size_of::<ResourceMatchingRecord>() + size_of::<Self>())
	}
	fn query_work_bound(&self) -> usize {
		self.records.len().max(1)
	}
	fn query_communication_bound(&self) -> usize {
		0
	}
	fn next_record(
		&self,
		color: usize,
		bound: Option<usize>,
		reverse: bool,
	) -> quest_qsvt::Result<Option<ResourceMatchingRecord>> {
		let mut entries = self.records.iter().copied().filter(|r| {
			r.column.color == color
				&& bound.is_none_or(|b| {
					if reverse {
						r.column.source < b
					} else {
						r.column.source > b
					}
				})
		});
		Ok(if reverse {
			entries.next_back()
		} else {
			entries.next()
		})
	}
	fn forward(
		&self,
		color: usize,
		source: usize,
	) -> quest_qsvt::Result<Option<ResourceMatchingRecord>> {
		Ok(self
			.records
			.iter()
			.copied()
			.find(|r| r.column.color == color && r.column.source == source))
	}
	fn reverse(&self, color: usize, destination: usize) -> quest_qsvt::Result<Option<usize>> {
		Ok(self
			.records
			.iter()
			.find(|r| r.column.color == color && r.column.destination == destination)
			.map(|r| r.column.source))
	}
}
fn fixture() -> quest_qsvt::Result<(MatchingEncoding, Directory)> {
	let policy = NumericalPolicy::default();
	let matrix = quest_numerics::SparseMatrix::from_triplets(
		3,
		5,
		quest_numerics::SparseFormat::Csr,
		vec![
			(1, 0, Complex64::new(0.3, 0.4)),
			(2, 1, Complex64::new(-0.8, 0.1)),
			(0, 2, Complex64::new(0.0, 0.5)),
			(0, 4, Complex64::new(0.2, -0.7)),
		],
		quest_numerics::SparseLimits::default(),
	)?;
	let encoding = MatchingEncoding::from_sparse(&matrix, policy)?;
	let shard = MatchingShard::from_encoding(&encoding, 0, 1, policy)?;
	let records = shard
		.records()
		.iter()
		.map(|column| {
			let edge = encoding
				.matchings()
				.get(column.color)
				.and_then(|m| m.entry(column.source));
			ResourceMatchingRecord {
				column: *column,
				theta: edge.map_or(std::f64::consts::PI, |e| e.theta),
				phase_angle: edge.map_or(0.0, |e| e.phase),
				is_edge: edge.is_some(),
			}
		})
		.collect();
	Ok((
		encoding,
		Directory {
			header: shard.header(),
			records,
		},
	))
}
#[googletest::gtest]
fn resource_stream_matches_complete_matching_unitary_and_exact_gate_adjoint()
-> googletest::Result<()> {
	let policy = NumericalPolicy::default();
	let (reference, directory) = fixture()?;
	let source = ResourceMatchingEncoding::new(directory, ResourceReplayLimits::default())?;
	let matrix = quest_qsvt::materialize_oracle(&source.replay_oracle(policy)?, policy)?;
	let expected = quest_qsvt::materialize_oracle(&reference.to_oracle(policy)?, policy)?;
	for row in 0..matrix.nrows() {
		for col in 0..matrix.ncols() {
			googletest::expect_true!((matrix[(row, col)] - expected[(row, col)]).norm() < 1e-11);
		}
	}
	let mut forward = Vec::new();
	source.visit_replay(false, &mut |g| {
		forward.push(g);
		Ok(())
	})?;
	let mut reverse = Vec::new();
	source.visit_replay(true, &mut |g| {
		reverse.push(g);
		Ok(())
	})?;
	for (a, b) in forward.iter().rev().zip(reverse.iter()) {
		let mut expected = *a;
		expected.kind = match a.kind {
			quest_qsvt::ReplayKind::Ry(v) => quest_qsvt::ReplayKind::Ry(-v),
			quest_qsvt::ReplayKind::Phase(v) => quest_qsvt::ReplayKind::Phase(-v),
			k => k,
		};
		googletest::expect_true!(expected == *b);
	}
	googletest::expect_true!(forward.len() == reverse.len());
	Ok(())
}
#[googletest::gtest]
fn resource_admission_rejects_broken_directory_and_work_before_gate_emission()
-> googletest::Result<()> {
	let (_, mut broken) = fixture()?;
	broken.records[0].column.destination = 7;
	googletest::expect_true!(
		ResourceMatchingEncoding::new(broken, ResourceReplayLimits::default()).is_err()
	);
	let (_, resource) = fixture()?;
	googletest::expect_true!(
		ResourceMatchingEncoding::new(
			resource,
			ResourceReplayLimits {
				max_work: 1,
				..ResourceReplayLimits::default()
			}
		)
		.is_err()
	);
	let (_, resource) = fixture()?;
	googletest::expect_true!(
		ResourceMatchingEncoding::new(
			resource,
			ResourceReplayLimits {
				max_queries: 1,
				..ResourceReplayLimits::default()
			}
		)
		.is_err()
	);
	Ok(())
}
#[googletest::gtest]
fn touched_directory_replays_large_padding_with_bounded_workspace() -> googletest::Result<()> {
	let (_, mut directory) = fixture()?;
	directory.header.system_qubits = 40;
	directory.header.rows = 1usize << 40;
	directory.header.cols = 1usize << 40;
	let source = ResourceMatchingEncoding::new(directory, ResourceReplayLimits::default())?;
	let mut gates = 0;
	source.visit_replay(false, &mut |_| {
		gates += 1;
		Ok(())
	})?;
	googletest::expect_true!(gates < 512);
	googletest::expect_true!(source.retained_bytes()? < 16_384);
	let cloned = source.clone();
	googletest::expect_true!(cloned.retained_bytes()? == source.retained_bytes()?);
	Ok(())
}
#[googletest::gtest]
fn resource_rejects_retained_capacity_and_gate_limits() -> googletest::Result<()> {
	let (_, mut directory) = fixture()?;
	directory.records.reserve_exact(131_072);
	googletest::expect_true!(
		ResourceMatchingEncoding::new(
			directory,
			ResourceReplayLimits {
				max_bytes: 16_384,
				..ResourceReplayLimits::default()
			}
		)
		.is_err()
	);
	let (_, directory) = fixture()?;
	googletest::expect_true!(
		ResourceMatchingEncoding::new(
			directory,
			ResourceReplayLimits {
				max_gates: 1,
				..ResourceReplayLimits::default()
			}
		)
		.is_err()
	);
	Ok(())
}

#[googletest::gtest]
fn completed_record_hashes_are_admitted_as_constructor_work() -> googletest::Result<()> {
	let (_, resource) = fixture()?;
	let records = resource.records.len();
	let encoding = ResourceMatchingEncoding::new(resource, ResourceReplayLimits::default())?;
	let minimum = records
		.checked_mul(quest_qsvt::record_fingerprint_work(7)?)
		.ok_or(quest_qsvt::Error::Budget("test hash work"))?;
	googletest::expect_true!(encoding.recipe().statistics().work >= minimum);
	let (_, resource) = fixture()?;
	googletest::expect_true!(
		ResourceMatchingEncoding::new(
			resource,
			ResourceReplayLimits {
				max_work: quest_qsvt::record_fingerprint_work(7)?
					.checked_sub(1)
					.ok_or(quest_qsvt::Error::Budget("test hash work"))?,
				..ResourceReplayLimits::default()
			}
		)
		.is_err()
	);
	Ok(())
}
