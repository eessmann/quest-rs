//! Rank-local sparse production; no complete sparse source is collected.
//!
//! Row and column owners arbitrate proposals against frozen endpoint snapshots.
//! The minimum uncolored edge progresses each round; the palette is bounded by
//! `max_row_degree + max_column_degree - 1` before power-of-two label padding.
//! Input ordinals must be stable across partitions. Source columns and touched
//! inverse destinations use cyclic ownership; untouched padded coordinates are implicit.
//! Only power-of-two communicator sizes are admitted. All participants must call
//! this API in matching order, with identical dimensions and logical limits.
//! Numeric and caller-supplied scratch-store limits are independent; owned-buffer
//! admission excludes caller inputs, allocator overhead and opaque MPI storage.
use crate::{Error, error::BackendResult};
use quest_numerics::sparse_stream::{
	MemoryOnly, RunStore, SparseEntry, StreamBuilder, StreamLimits,
};
use quest_qsvt::{MatchingHeader, MatchingShard, NumericalPolicy};
use std::ops::Mul;
mod color;
mod completion;
mod transport;
use quest_sys::mpi::MpiCommunicator;
use transport::{Packet, Router, index, number, numerical};

/// Independent local storage, endpoint, iteration and wire admission.
#[derive(Clone, Debug)]
pub struct ProducerLimits {
	pub stream: StreamLimits,
	pub batch_entries: usize,
	pub max_local_edges: usize,
	pub max_endpoint_records: usize,
	pub max_vertex_degree: usize,
	pub max_rounds: usize,
	pub max_completion_rounds: usize,
	pub max_probes: usize,
	pub max_work: usize,
	pub max_communication_bytes: usize,
	pub max_bytes: usize,
}
impl Default for ProducerLimits {
	fn default() -> Self {
		Self {
			stream: StreamLimits::default(),
			batch_entries: 64,
			max_local_edges: 1_048_576,
			max_endpoint_records: 2_097_152,
			max_vertex_degree: 4096,
			max_rounds: 1_048_576,
			max_completion_rounds: 1_048_576,
			max_probes: 16_777_216,
			max_work: 268_435_456,
			max_communication_bytes: 1_073_741_824,
			max_bytes: 268_435_456,
		}
	}
}
/// Original edge and frozen gate parameters, owned by its source-column rank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplayEdge {
	pub color: usize,
	pub row: usize,
	pub column: usize,
	pub value: quest_qsvt::Complex64,
	pub theta: f64,
	pub phase: f64,
}
/// Touched inverse permutation record, owned by destination modulo parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReverseRecord {
	pub color: usize,
	pub destination: usize,
	pub source: usize,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ProducerStatistics {
	pub input_entries: usize,
	pub canonical_local_edges: usize,
	pub global_edges: usize,
	pub max_row_degree: usize,
	pub max_column_degree: usize,
	pub used_colors: usize,
	pub coloring_rounds: usize,
	pub completion_rounds: usize,
	pub probes: usize,
	pub work: usize,
	/// Sent request/reply payload bytes; excludes count handshakes and MPI collective metadata.
	pub sent_bytes: usize,
	/// Conservative peak live payload, including old/new vector overlap during growth.
	/// The stream's admitted upper bound is reserved until its iterator drops;
	/// MPI internals, allocator overhead and caller-owned input are excluded.
	pub peak_managed_bytes: usize,
}
/// Immutable source-column records plus destination-owned inverse directory.
pub struct ProducedMatching {
	shard: MatchingShard,
	edges: Vec<ReplayEdge>,
	reverse: Vec<ReverseRecord>,
	statistics: ProducerStatistics,
}
impl ProducedMatching {
	#[must_use]
	pub const fn shard(&self) -> &MatchingShard {
		&self.shard
	}
	#[must_use]
	pub fn edges(&self) -> &[ReplayEdge] {
		&self.edges
	}
	#[must_use]
	pub fn reverse(&self) -> &[ReverseRecord] {
		&self.reverse
	}
	#[must_use]
	pub const fn statistics(&self) -> ProducerStatistics {
		self.statistics
	}
	/// Actual owned Rust payload retained after producer temporaries have dropped.
	///
	/// Includes the container once and vector capacities, not the historical peak.
	/// Allocator overhead and MPI storage are outside this payload accounting.
	/// # Errors
	/// Rejects byte-accounting overflow.
	pub fn retained_bytes(&self) -> crate::Result<usize> {
		let columns = self
			.shard
			.storage_bytes()
			.map_err(|_| Error::Overflow)?
			.checked_sub(size_of::<MatchingShard>())
			.ok_or(Error::Overflow)?;
		[
			size_of::<Self>(),
			columns,
			self.edges
				.capacity()
				.checked_mul(size_of::<ReplayEdge>())
				.ok_or(Error::Overflow)?,
			self.reverse
				.capacity()
				.checked_mul(size_of::<ReverseRecord>())
				.ok_or(Error::Overflow)?,
		]
		.into_iter()
		.try_fold(0usize, |sum, bytes| {
			sum.checked_add(bytes).ok_or(Error::Overflow)
		})
	}
	#[must_use]
	pub fn into_parts(
		self,
	) -> (
		MatchingShard,
		Vec<ReplayEdge>,
		Vec<ReverseRecord>,
		ProducerStatistics,
	) {
		(self.shard, self.edges, self.reverse, self.statistics)
	}
}
/// Canonicalize and color genuinely local COO streams collectively.
/// # Errors
/// Rejects malformed input and collective storage/work/communication limits.
pub fn produce_matching(
	communicator: &MpiCommunicator<'_>,
	rows: usize,
	cols: usize,
	input: impl IntoIterator<Item = quest_numerics::Result<SparseEntry>>,
	limits: ProducerLimits,
) -> crate::qsvt::Result<ProducedMatching> {
	produce_matching_with_store(communicator, rows, cols, input, limits, MemoryOnly)
}
/// Collective producer with caller-owned scratch storage supplied by the IO layer.
/// Handles remain rank-local; all calls and limits must agree across participants.
/// # Errors
/// Rejects invalid input, resources, unsupported communicator sizes and divergent limits.
pub fn produce_matching_with_store<S: RunStore>(
	communicator: &MpiCommunicator<'_>,
	rows: usize,
	cols: usize,
	input: impl IntoIterator<Item = quest_numerics::Result<SparseEntry>>,
	limits: ProducerLimits,
	store: S,
) -> crate::qsvt::Result<ProducedMatching> {
	// Panics or a broken MPI transport cannot unwind safely past peers in a protocol.
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
		produce(communicator, rows, cols, input, limits, store)
	}))
	.unwrap_or_else(|_| quest_sys::mpi::abort_job())
}
fn produce<S: RunStore>(
	communicator: &MpiCommunicator<'_>,
	rows: usize,
	cols: usize,
	input: impl IntoIterator<Item = quest_numerics::Result<SparseEntry>>,
	limits: ProducerLimits,
	store: S,
) -> crate::qsvt::Result<ProducedMatching> {
	let mut router = initialize(communicator, rows, cols, limits)?;
	let mut edges = canonicalize(&mut router, rows, cols, input, store)?;
	router.statistics.canonical_local_edges = edges.len();
	router.statistics.global_edges = router.sum(edges.len())?;
	let local_beta = edges
		.iter()
		.map(|e| e.entry.value.norm())
		.fold(0.0_f64, f64::max);
	let finite = if local_beta.is_finite() {
		Ok(())
	} else {
		Err(Error::Value("nonfinite sparse magnitude"))
	};
	router.agree(finite)?;
	let beta = router.beta(local_beta)?;
	let beta = if beta == 0.0 { 1.0 } else { beta };
	let used = color::color(&mut router, &mut edges)?;
	let colors = used
		.max(1)
		.checked_next_power_of_two()
		.ok_or(Error::Overflow)?;
	let count = u32::try_from(colors).map_err(|_| Error::Overflow)?;
	let alpha = beta.mul(f64::from(count));
	if !alpha.is_finite() {
		return Err(Error::Value("matching normalization overflow").into());
	}
	let records = completion::complete(&mut router, &edges, beta)?;
	// Admit all new SHA summaries before hashing. The single-rank shard
	// constructor independently verifies its complete payload once more.
	let charge = (|| {
		let passes = if router.parts == 1 { 2 } else { 1 };
		let columns = records
			.columns
			.len()
			.checked_mul(passes)
			.and_then(|n| n.checked_mul(quest_qsvt::record_fingerprint_work(7).ok()?))
			.ok_or(Error::Overflow)?;
		let source = edges
			.len()
			.checked_mul(quest_qsvt::record_fingerprint_work(4).map_err(|_| Error::Overflow)?)
			.ok_or(Error::Overflow)?;
		router.work(columns.checked_add(source).ok_or(Error::Overflow)?)
	})();
	router.agree(charge)?;
	let (local_records, digest) = MatchingShard::summarize_records(&records.columns)?;
	let record_count = router.sum(local_records)?;
	let record_digest = router.digest(digest)?;
	let mut source = 0_u64;
	for edge in &edges {
		source = source.wrapping_add(source_digest(edge.entry)?);
	}
	let source_identity = router
		.digest(source)?
		.wrapping_add(number(rows)?.rotate_left(17))
		.wrapping_add(number(cols)?.rotate_left(31))
		.wrapping_add(0x5350_524f_4430_3032);
	let system = rows
		.max(cols)
		.checked_next_power_of_two()
		.ok_or(Error::Overflow)?;
	let header = MatchingHeader {
		rows,
		cols,
		system_qubits: usize::try_from(system.trailing_zeros()).map_err(|_| Error::Overflow)?,
		color_qubits: usize::try_from(colors.trailing_zeros()).map_err(|_| Error::Overflow)?,
		num_colors: colors,
		beta,
		alpha,
		source_identity,
		record_count,
		record_digest,
	};
	let shard = MatchingShard::from_parts(
		header,
		router.rank,
		router.parts,
		records.columns,
		NumericalPolicy {
			max_bytes: router.limits.max_bytes,
		},
	);
	// Model admission is deterministic/local, therefore agree before returning any rank.
	let admitted = shard.map_err(|_| Error::Value("produced matching shard admission"));
	let shard = router.agree(admitted)?;
	Ok(ProducedMatching {
		shard,
		edges: records.edges,
		reverse: records.reverse,
		statistics: router.statistics,
	})
}
fn initialize<'a>(
	communicator: &'a MpiCommunicator<'_>,
	rows: usize,
	cols: usize,
	limits: ProducerLimits,
) -> crate::Result<Router<'a>> {
	let rank = usize::try_from(communicator.rank().context("reading producer rank")?)
		.map_err(|_| Error::Overflow)?;
	let parts = usize::try_from(communicator.size().context("reading producer size")?)
		.map_err(|_| Error::Overflow)?;
	let mut lane = communicator
		.collective_lane()
		.context("borrowing sparse producer lane")?;
	let admitted = rows > 0
		&& cols > 0
		&& parts.is_power_of_two()
		&& rows
			.max(cols)
			.checked_next_power_of_two()
			.and_then(|n| n.checked_mul(2))
			.is_some();
	if !lane
		.all_agree(admitted)
		.context("admitting sparse dimensions")?
	{
		return Err(Error::Value(
			"producer requires positive dimensions and power-of-two ranks",
		));
	}
	let metadata = [
		rows,
		cols,
		limits.batch_entries,
		limits.max_local_edges,
		limits.max_endpoint_records,
		limits.max_vertex_degree,
		limits.max_rounds,
		limits.max_completion_rounds,
		limits.max_probes,
		limits.max_work,
		limits.max_communication_bytes,
		limits.max_bytes,
		limits.stream.buffer_entries,
		limits.stream.max_entries,
		limits.stream.max_bytes,
		limits.stream.max_spill_bytes,
		limits.stream.max_runs,
		limits.stream.max_work,
	];
	let mut bytes = [0_u8; 144];
	for (value, chunk) in metadata.iter().zip(bytes.as_chunks_mut::<8>().0) {
		chunk.copy_from_slice(&number(*value)?.to_le_bytes());
	}
	crate::collective::equal(&mut lane, &bytes)?;
	let initialization = Router::new(lane, rank, parts, limits);
	// Admission before any exchange: Router::new is deterministic given admitted limits.
	initialization
}
#[allow(
	clippy::indexing_slicing,
	reason = "Decoded transport packets are fixed eight-word arrays"
)]
fn canonicalize<S: RunStore>(
	router: &mut Router<'_>,
	rows: usize,
	cols: usize,
	input: impl IntoIterator<Item = quest_numerics::Result<SparseEntry>>,
	store: S,
) -> crate::Result<Vec<color::Edge>> {
	let parts = router.parts;
	let builder = numerical(StreamBuilder::with_store(
		rows,
		cols,
		router.limits.stream.clone(),
		store,
	));
	let mut builder = router.agree(builder)?;
	let local_count = std::cell::Cell::new(0_usize);
	let input_limit = router.limits.stream.max_entries;
	router.exchange(
		input.into_iter().map(|entry| {
			let entry = numerical(entry)?;
			numerical(entry.validate(rows, cols))?;
			let count = local_count.get().checked_add(1).ok_or(Error::Overflow)?;
			if count > input_limit {
				return Err(Error::Value("producer input entries"));
			}
			local_count.set(count);
			Ok(Packet {
				destination: entry.row.checked_rem(parts).ok_or(Error::Overflow)?,
				words: [
					number(entry.row)?,
					number(entry.column)?,
					entry.ordinal,
					entry.value.re.to_bits(),
					entry.value.im.to_bits(),
					0,
					0,
					0,
				],
			})
		}),
		|_, p| {
			numerical(builder.push(SparseEntry {
				row: index(p[0])?,
				column: index(p[1])?,
				ordinal: p[2],
				value: quest_qsvt::Complex64::new(f64::from_bits(p[3]), f64::from_bits(p[4])),
			}))?;
			Ok([0; 8])
		},
		|_, _| Ok(()),
	)?;
	router.statistics.input_entries = local_count.get();
	let sorted = numerical(builder.finish());
	let sorted = router.agree(sorted)?;
	let mut edges = Vec::new();
	let canonical = (|| {
		for entry in sorted {
			router.push(
				&mut edges,
				color::Edge::new(numerical(entry)?),
				router.limits.max_local_edges,
			)?;
		}
		Ok(())
	})();
	// Consuming `sorted` above destroys its memory buffer or scratch-run reader.
	// The stream allowance no longer overlaps coloring/completion allocations.
	router.release(router.limits.stream.max_bytes)?;
	router.agree(canonical)?;
	Ok(edges)
}
fn source_digest(entry: SparseEntry) -> crate::Result<u64> {
	Ok(quest_qsvt::record_fingerprint(
		0x5350_454e_5452_5932,
		[
			number(entry.row)?,
			number(entry.column)?,
			entry.value.re.to_bits(),
			entry.value.im.to_bits(),
		],
	))
}

#[cfg(test)]
#[allow(
	clippy::panic_in_result_fn,
	reason = "Pure regression asserts distinct operator identities; Result propagates fixture conversion failures"
)]
mod source_identity_tests {
	use super::*;
	#[test]
	fn conjugate_source_sums_do_not_cancel_at_small_or_campaign_shape() -> crate::Result<()> {
		for dimension in [2, 4, 8, 32] {
			let mut digests = Vec::new();
			for (diagonal, imaginary) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0)] {
				let mut sum = 0_u64;
				for row in 0..dimension {
					for (column, value) in [
						(row, quest_qsvt::Complex64::new(diagonal, 0.0)),
						(row ^ 1, quest_qsvt::Complex64::new(0.0, imaginary)),
					] {
						sum = sum.wrapping_add(source_digest(SparseEntry {
							row,
							column,
							ordinal: 0,
							value,
						})?);
					}
				}
				assert!(
					!digests.contains(&sum),
					"distinct source sums aliased at dimension {dimension}"
				);
				digests.push(sum);
			}
		}
		Ok(())
	}
}
