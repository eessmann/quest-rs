//! Collective publication, restart and gate replay from immutable matching resources.
//!
//! Each participant retains only its source-owned columns and destination-owned
//! inverse directory. Logical bucket ownership can change with communicator size.
use super::matching::preprocess::{ProducedMatching, ReverseRecord};
use crate::{
	Error,
	collective::{CollectiveRegister, equal},
	error::BackendResult,
};
use quest_qsvt::{
	MatchingHeader, MatchingShard, NumericalPolicy, ReplayGate,
	matching_resource::{
		AdmittedMatchingReplay, ResourceMatchingRecord, ResourceReplayLimits,
		ResourceReplayStatistics,
	},
};
use quest_qsvt_io::sharded_matching::{
	MatchingBucketReceipt, MatchingManifest, PersistedMatchingRecord, ShardIoLimits, write_bucket,
};
use quest_sys::mpi::{MpiCollectiveLane, MpiCommunicator};
use std::{path::Path, sync::Arc};
mod directory;
mod loading;
mod preparation;
use directory::{Directory, build_reverse, maximum, mpi, reduce_summary};
pub use loading::{LoadDirectoryStatistics, LoadStatistics};
pub use preparation::{PersistedPreparationLimits, PersistedPreparationResources};

/// Typed persistence and collective admission failure; MPI corruption is fatal.
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
	#[error(transparent)]
	Runtime(#[from] crate::Error),
	#[error(transparent)]
	Model(#[from] quest_qsvt::Error),
	#[error(transparent)]
	Io(#[from] quest_qsvt_io::Error),
	#[error("collective matching persistence rejected: {0}")]
	Collective(&'static str),
}
pub type Result<T> = std::result::Result<T, PersistenceError>;
/// Local storage, IO and transport admission; replay has independent work/gate limits.
#[derive(Clone, Copy, Debug)]
pub struct PersistenceLimits {
	pub io: ShardIoLimits,
	pub max_local_records: usize,
	/// Loading: simultaneous explicit Rust payload envelope, including declared
	/// manifest/chunk allowances, actual record/reverse capacities and fixed bookkeeping.
	/// Returned overcapacity is rejected before downstream work, but may exist
	/// temporarily; this is not an allocator quota or an HDF5/MPI/RSS bound.
	/// Publication has its separate producer/storage admission.
	pub max_bytes: usize,
	pub max_communication_bytes: usize,
	pub replay: ResourceReplayLimits,
}
impl Default for PersistenceLimits {
	fn default() -> Self {
		Self {
			io: ShardIoLimits::default(),
			max_local_records: 1_048_576,
			max_bytes: 268_435_456,
			max_communication_bytes: 1_073_741_824,
			replay: ResourceReplayLimits::default(),
		}
	}
}
/// Limits for verified immutable resource loading, independent of gate replay.
#[derive(Clone, Copy, Debug)]
pub struct ResourceLoadLimits {
	pub io: ShardIoLimits,
	pub max_local_records: usize,
	/// Explicit Rust payload envelope; native IO, allocator and MPI costs are separate.
	pub max_bytes: usize,
	pub max_work: usize,
	pub max_communication_bytes: usize,
}
impl From<PersistenceLimits> for ResourceLoadLimits {
	fn from(limits: PersistenceLimits) -> Self {
		Self {
			io: limits.io,
			max_local_records: limits.max_local_records,
			max_bytes: limits.max_bytes,
			max_work: limits.replay.max_work,
			max_communication_bytes: limits.max_communication_bytes,
		}
	}
}
impl Default for ResourceLoadLimits {
	fn default() -> Self {
		PersistenceLimits::default().into()
	}
}
fn word(n: usize) -> Result<u64> {
	Ok(u64::try_from(n).map_err(|_| Error::Overflow)?)
}
fn index(n: u64) -> Result<usize> {
	Ok(usize::try_from(n).map_err(|_| Error::Overflow)?)
}
fn agree<T>(lane: &mut MpiCollectiveLane<'_>, value: Result<T>) -> Result<T> {
	if !mpi(lane.all_agree(value.is_ok())) {
		return Err(value
			.err()
			.unwrap_or(PersistenceError::Collective("owner failure")));
	}
	value
}
fn header_words(h: MatchingHeader) -> Result<[u64; 10]> {
	Ok([
		word(h.rows)?,
		word(h.cols)?,
		word(h.system_qubits)?,
		word(h.color_qubits)?,
		word(h.num_colors)?,
		h.beta.to_bits(),
		h.alpha.to_bits(),
		h.source_identity,
		word(h.record_count)?,
		h.record_digest,
	])
}
fn common_resource(
	lane: &mut MpiCollectiveLane<'_>,
	header: MatchingHeader,
	parts: usize,
	buckets: usize,
	limits: ResourceLoadLimits,
) -> Result<()> {
	header.validate()?;
	for value in header_words(header)? {
		equal(lane, &value.to_le_bytes())?;
	}
	for value in [
		parts,
		buckets,
		limits.max_local_records,
		limits.max_bytes,
		limits.max_communication_bytes,
		limits.io.chunk_records,
		limits.io.max_buffer_bytes,
		limits.io.max_manifest_bytes,
		limits.io.max_buckets,
		limits.io.max_records,
		limits.max_work,
	] {
		equal(lane, &word(value)?.to_le_bytes())?;
	}
	equal(lane, &limits.io.max_file_bytes.to_le_bytes())?;
	Ok(())
}
fn common(
	lane: &mut MpiCollectiveLane<'_>,
	header: MatchingHeader,
	parts: usize,
	buckets: usize,
	limits: PersistenceLimits,
) -> Result<()> {
	common_resource(lane, header, parts, buckets, limits.into())?;
	for value in [
		limits.replay.max_bytes,
		limits.replay.max_queries,
		limits.replay.max_work,
		limits.replay.max_communication_bytes,
		limits.replay.max_gates,
	] {
		equal(lane, &word(value)?.to_le_bytes())?;
	}
	Ok(())
}

fn retained(records: usize) -> Result<usize> {
	Ok(records
		.checked_mul(
			size_of::<ResourceMatchingRecord>()
				.checked_add(size_of::<ReverseRecord>())
				.ok_or(Error::Overflow)?,
		)
		.and_then(|n| n.checked_add(4096))
		.ok_or(Error::Overflow)?)
}
fn reserve<T>(count: usize) -> Result<Vec<T>> {
	let mut values = Vec::new();
	values
		.try_reserve_exact(count)
		.map_err(|_| Error::Allocation)?;
	Ok(values)
}
const fn from_disk(record: PersistedMatchingRecord) -> ResourceMatchingRecord {
	ResourceMatchingRecord {
		column: record.column,
		theta: record.theta,
		phase_angle: record.phase_angle,
		is_edge: record.is_edge,
	}
}
const fn to_disk(record: ResourceMatchingRecord) -> PersistedMatchingRecord {
	PersistedMatchingRecord {
		column: record.column,
		theta: record.theta,
		phase_angle: record.phase_angle,
		is_edge: record.is_edge,
	}
}

/// Publish producer-owned buckets, then a common manifest after every owner succeeds.
/// Files are never replaced. Failed publication can leave unpublished owner buckets.
/// # Errors
/// Rejects incompatible bucket ownership, failed owners, or IO/work/storage/wire budgets.
#[allow(
	clippy::too_many_lines,
	reason = "Linear collective publication protocol keeps owner agreements beside IO stages"
)]
pub fn publish_produced(
	communicator: &MpiCommunicator<'_>,
	produced: &ProducedMatching,
	directory: impl AsRef<Path>,
	manifest_path: impl AsRef<Path>,
	buckets: usize,
	limits: PersistenceLimits,
) -> Result<MatchingManifest> {
	let rank = index(
		u64::try_from(communicator.rank().context("reading persistence rank")?)
			.map_err(|_| Error::Overflow)?,
	)?;
	let parts = usize::try_from(communicator.size().context("reading persistence size")?)
		.map_err(|_| Error::Overflow)?;
	let mut lane = communicator
		.collective_lane()
		.context("borrowing persistence lane")?;
	let header = produced.shard().header();
	agree(
		&mut lane,
		(|| {
			header.validate()?;
			if produced.shard().rank() != rank
				|| produced.shard().parts() != parts
				|| !buckets.is_power_of_two()
				|| buckets.checked_rem(parts) != Some(0)
				|| buckets > limits.io.max_buckets
			{
				return Err(PersistenceError::Collective("bucket/producer ownership"));
			}
			Ok(())
		})(),
	)?;
	common(&mut lane, header, parts, buckets, limits)?;
	let metadata = buckets
		.checked_mul(
			size_of::<MatchingBucketReceipt>()
				.checked_add(64)
				.ok_or(Error::Overflow)?,
		)
		.and_then(|n| n.checked_mul(2))
		.ok_or(Error::Overflow)?;
	let edge_bytes = produced
		.edges()
		.len()
		.checked_mul(size_of::<usize>())
		.ok_or(Error::Overflow)?;
	let search_work = usize::try_from(produced.edges().len().max(1).ilog2())
		.map_err(|_| Error::Overflow)?
		.checked_add(2)
		.ok_or(Error::Overflow)?;
	let work = produced
		.shard()
		.records()
		.len()
		.checked_mul(
			buckets
				.checked_div(parts)
				.ok_or(Error::Overflow)?
				.checked_add(search_work)
				.ok_or(Error::Overflow)?,
		)
		.and_then(|n| n.checked_add(produced.edges().len().checked_mul(search_work)?))
		.ok_or(Error::Overflow)?;
	let produced_bytes = agree(
		&mut lane,
		produced.retained_bytes().map_err(PersistenceError::from),
	)?;
	agree(
		&mut lane,
		if metadata
			.checked_add(edge_bytes)
			.and_then(|n| n.checked_add(produced_bytes))
			.and_then(|n| n.checked_add(limits.io.max_buffer_bytes))
			.is_none_or(|n| n > limits.max_bytes)
			|| metadata > limits.io.max_manifest_bytes
			|| work > limits.replay.max_work
			|| buckets
				.checked_mul(96)
				.and_then(|n| n.checked_mul(parts.saturating_sub(1)))
				.is_none_or(|n| n > limits.max_communication_bytes)
		{
			Err(PersistenceError::Collective("publication resource budget"))
		} else {
			Ok(())
		},
	)?;
	let mut order = agree(&mut lane, reserve(produced.edges().len()))?;
	order.extend(0..produced.edges().len());
	order.sort_unstable_by_key(|&i| produced.edges().get(i).map(|e| (e.color, e.column)));
	let local = (|| {
		let mut receipts = reserve(buckets.checked_div(parts).ok_or(Error::Overflow)?)?;
		for bucket in (rank..buckets).step_by(parts) {
			let records = produced
				.shard()
				.records()
				.iter()
				.filter(|c| c.source.checked_rem(buckets) == Some(bucket))
				.map(|column| {
					let edge = order
						.binary_search_by_key(&(column.color, column.source), |&i| {
							produced
								.edges()
								.get(i)
								.map_or((usize::MAX, usize::MAX), |e| (e.color, e.column))
						})
						.ok()
						.and_then(|i| order.get(i))
						.and_then(|&i| produced.edges().get(i));
					Ok(to_disk(ResourceMatchingRecord {
						column: *column,
						theta: edge.map_or(std::f64::consts::PI, |e| e.theta),
						phase_angle: edge.map_or(0.0, |e| e.phase),
						is_edge: edge.is_some(),
					}))
				});
			receipts.push(write_bucket(
				directory.as_ref(),
				header,
				bucket,
				buckets,
				records,
				limits.io,
			)?);
		}
		Ok(receipts)
	})();
	let local = agree(&mut lane, local)?;
	let mut receipts = agree(&mut lane, reserve(buckets))?;
	for bucket in 0..buckets {
		let owner = bucket.checked_rem(parts).ok_or(Error::Overflow)?;
		let mut packet = [0; 96];
		if rank == owner {
			let receipt = local
				.get(bucket.checked_div(parts).ok_or(Error::Overflow)?)
				.ok_or(PersistenceError::Collective("missing owner receipt"))?;
			for (word, bytes) in [
				word(receipt.bucket)?,
				word(receipt.buckets)?,
				word(receipt.records)?,
				receipt.size_bytes,
			]
			.into_iter()
			.zip(
				packet
					.get_mut(..32)
					.ok_or(Error::Overflow)?
					.as_chunks_mut::<8>()
					.0,
			) {
				bytes.copy_from_slice(&word.to_le_bytes());
			}
			packet
				.get_mut(32..64)
				.ok_or(Error::Overflow)?
				.copy_from_slice(&receipt.sha256);
			packet
				.get_mut(64..)
				.ok_or(Error::Overflow)?
				.copy_from_slice(&receipt.semantic_sha256);
		}
		mpi(lane.broadcast_bytes(
			i32::try_from(owner).map_err(|_| Error::Overflow)?,
			&mut packet,
		));
		let read = |offset: usize| -> Result<u64> {
			Ok(u64::from_le_bytes(
				packet
					.get(offset..offset.checked_add(8).ok_or(Error::Overflow)?)
					.ok_or(Error::Overflow)?
					.try_into()
					.map_err(|_| Error::Overflow)?,
			))
		};
		receipts.push(MatchingBucketReceipt {
			bucket: index(read(0)?)?,
			buckets: index(read(8)?)?,
			records: index(read(16)?)?,
			size_bytes: read(24)?,
			file: format!("matching-{bucket:016x}.h5"),
			sha256: packet
				.get(32..64)
				.ok_or(Error::Overflow)?
				.try_into()
				.map_err(|_| Error::Overflow)?,
			semantic_sha256: packet
				.get(64..)
				.ok_or(Error::Overflow)?
				.try_into()
				.map_err(|_| Error::Overflow)?,
		});
	}
	let manifest = agree(
		&mut lane,
		MatchingManifest::new(header, receipts, limits.io).map_err(PersistenceError::from),
	)?;
	let published = if rank == 0 {
		manifest
			.publish(manifest_path, limits.io)
			.map_err(PersistenceError::from)
	} else {
		Ok(())
	};
	agree(&mut lane, published)?;
	Ok(manifest)
}

#[derive(Debug)]
struct Data {
	header: MatchingHeader,
	rank: usize,
	parts: usize,
	records: Vec<ResourceMatchingRecord>,
	reverse: Vec<ReverseRecord>,
	frozen_identity: u64,
	manifest_identity: [u8; 32],
	common_bytes: usize,
	query_work: usize,
	query_wire: usize,
	load_statistics: LoadStatistics,
}
/// Verified source-owned columns and destination-owned inverse directory.
///
/// Loading checks coefficients, frozen angles, integrity and permutation closure.
/// It does not admit or claim a portable gate recipe. Clones share immutable data;
/// consuming native preparation still requires unique ownership on every rank.
#[derive(Clone, Debug)]
pub struct LoadedMatchingResource {
	data: Arc<Data>,
}

/// Owns verified immutable local records and an admitted portable whole-unitary recipe.
///
/// Clones share the snapshot; source-file replacement cannot alter later replay.
#[derive(Clone, Debug)]
pub struct LoadedMatching {
	data: Arc<Data>,
	recipe: AdmittedMatchingReplay,
	statistics: LoadStatistics,
}
impl LoadedMatchingResource {
	/// Observed loading work; `replay_admitted` remains false for this resource.
	#[must_use]
	pub fn load_statistics(&self) -> LoadStatistics {
		self.data.load_statistics
	}
	#[must_use]
	pub fn header(&self) -> MatchingHeader {
		self.data.header
	}
	#[must_use]
	pub fn local_records(&self) -> &[ResourceMatchingRecord] {
		&self.data.records
	}
	#[must_use]
	pub fn retained_bytes(&self) -> usize {
		self.data.common_bytes
	}
	/// Bounded compatibility snapshot for existing fused native matching execution.
	/// # Errors
	/// Rejects simultaneous retained-resource and owned-column allocation beyond policy.
	pub fn matching_shard(&self, policy: NumericalPolicy) -> Result<MatchingShard> {
		let bytes = self
			.data
			.records
			.len()
			.checked_mul(size_of::<quest_qsvt::MatchingColumn>())
			.and_then(|n| n.checked_add(self.retained_bytes()))
			.ok_or(Error::Overflow)?;
		if bytes > policy.max_bytes {
			return Err(PersistenceError::Collective("fused shard clone budget"));
		}
		let mut columns = reserve(self.data.records.len())?;
		columns.extend(self.data.records.iter().map(|r| r.column));
		Ok(MatchingShard::from_parts(
			self.data.header,
			self.data.rank,
			self.data.parts,
			columns,
			policy,
		)?)
	}

	/// Collectively admit the portable forward/adjoint gate recipe and one replay.
	///
	/// The immutable source is shared with any existing aliases. No native state
	/// changes, and no visitor runs during admission.
	/// # Errors
	/// Rejects communicator/source/limit mismatch and portable replay budgets.
	pub fn admit_replay(
		self,
		communicator: &MpiCommunicator<'_>,
		limits: ResourceReplayLimits,
	) -> Result<LoadedMatching> {
		let started = std::time::Instant::now();
		let rank = usize::try_from(communicator.rank().context("reading admission rank")?)
			.map_err(|_| Error::Overflow)?;
		let parts = usize::try_from(communicator.size().context("reading admission size")?)
			.map_err(|_| Error::Overflow)?;
		let mut lane = communicator
			.collective_lane()
			.context("borrowing replay admission lane")?;
		agree(
			&mut lane,
			if rank == self.data.rank && parts == self.data.parts {
				Ok(())
			} else {
				Err(PersistenceError::Collective("replay admission ownership"))
			},
		)?;
		equal(&mut lane, &self.data.manifest_identity)?;
		for value in [
			limits.max_bytes,
			limits.max_queries,
			limits.max_work,
			limits.max_communication_bytes,
			limits.max_gates,
		] {
			equal(&mut lane, &word(value)?.to_le_bytes())?;
		}
		let directory = Directory::new(&self.data, lane);
		let recipe = AdmittedMatchingReplay::admit(&directory, limits)?;
		let mut statistics = self.data.load_statistics;
		statistics.admission = directory.statistics();
		statistics.replay_admitted = true;
		statistics.replay_admission = started.elapsed();
		statistics.total = statistics
			.total
			.checked_add(statistics.replay_admission)
			.ok_or(Error::Overflow)?;
		drop(directory);
		Ok(LoadedMatching {
			data: self.data,
			recipe,
			statistics,
		})
	}
}
impl LoadedMatching {
	/// Rank-local load timings and exact logical directory call counts.
	///
	/// Shared clones preserve these observations; subsequent gate replay does not
	/// modify them. See [`LoadStatistics`] for timing and communication scope.
	#[must_use]
	pub const fn load_statistics(&self) -> LoadStatistics {
		self.statistics
	}
	#[must_use]
	pub fn header(&self) -> MatchingHeader {
		self.data.header
	}
	#[must_use]
	pub fn local_records(&self) -> &[ResourceMatchingRecord] {
		&self.data.records
	}
	#[must_use]
	pub const fn recipe(&self) -> &AdmittedMatchingReplay {
		&self.recipe
	}
	#[must_use]
	pub fn retained_bytes(&self) -> usize {
		self.data.common_bytes
	}
	/// Bounded compatibility snapshot for existing fused native matching execution.
	/// # Errors
	/// Rejects simultaneous retained-resource and owned-column allocation beyond policy.
	pub fn matching_shard(&self, policy: NumericalPolicy) -> Result<MatchingShard> {
		let bytes = self
			.data
			.records
			.len()
			.checked_mul(size_of::<quest_qsvt::MatchingColumn>())
			.and_then(|n| n.checked_add(self.retained_bytes()))
			.ok_or(Error::Overflow)?;
		if bytes > policy.max_bytes {
			return Err(PersistenceError::Collective("fused shard clone budget"));
		}
		let mut columns = reserve(self.data.records.len())?;
		columns.extend(self.data.records.iter().map(|r| r.column));
		Ok(MatchingShard::from_parts(
			self.data.header,
			self.data.rank,
			self.data.parts,
			columns,
			policy,
		)?)
	}
	/// Collective portable gate replay. Every participant supplies the same layout.
	/// Visitor failure/panic during emission is fatal; complete admission precedes it.
	/// # Errors
	/// Rejects communicator/layout/source metadata mismatch before emission.
	pub fn visit_gates(
		&self,
		communicator: &MpiCommunicator<'_>,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
		mut visitor: impl FnMut(ReplayGate) -> crate::Result<()>,
	) -> Result<ResourceReplayStatistics> {
		let rank = usize::try_from(communicator.rank().context("reading replay rank")?)
			.map_err(|_| Error::Overflow)?;
		let parts = usize::try_from(communicator.size().context("reading replay size")?)
			.map_err(|_| Error::Overflow)?;
		let mut lane = communicator
			.collective_lane()
			.context("borrowing resource replay lane")?;
		self.admit_layout(&mut lane, rank, parts, targets, mask, value, adjoint, None)?;
		let directory = Directory::new(&self.data, lane);
		fatal(|| {
			self.recipe
				.visit_mapped_gates(&directory, targets, mask, value, adjoint, |gate| {
					visitor(gate)
						.map_err(|_| quest_qsvt::Error::Encoding("resource visitor failed"))
				})
		});
		Ok(self.recipe.statistics())
	}
	/// Execute actual gates on the distributed state with reused checked control scratch.
	/// # Errors
	/// Rejects layout, CPU deployment, ownership and scratch admission before mutation.
	pub fn apply_native(
		&self,
		register: &mut CollectiveRegister<'_, '_, '_>,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
	) -> Result<ResourceReplayStatistics> {
		let mut lane = register.environment.begin(
			62,
			register.id,
			u64::from(adjoint),
			self.data.frozen_identity,
		)?;
		let rank = usize::try_from(register.environment.rank()?).map_err(|_| Error::Overflow)?;
		let parts = usize::try_from(register.environment.size()?).map_err(|_| Error::Overflow)?;
		self.admit_layout(
			&mut lane,
			rank,
			parts,
			targets,
			mask,
			value,
			adjoint,
			Some(register.num_qubits().get()),
		)?;
		let mut executor = agree(
			&mut lane,
			super::replay_native::ReplayGateExecutor::new(&register.inner)
				.map_err(PersistenceError::from),
		)?;
		let directory = Directory::new(&self.data, lane);
		fatal(|| {
			self.recipe
				.visit_mapped_gates(&directory, targets, mask, value, adjoint, |gate| {
					executor
						.apply(&mut register.inner, gate)
						.map_err(|_| quest_qsvt::Error::Encoding("native resource replay failed"))
				})
		});
		Ok(self.recipe.statistics())
	}
	#[allow(
		clippy::too_many_arguments,
		reason = "All physical layout and communicator operands require common admission"
	)]
	fn admit_layout(
		&self,
		lane: &mut MpiCollectiveLane<'_>,
		rank: usize,
		parts: usize,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
		width: Option<usize>,
	) -> Result<()> {
		let valid = (|| {
			if rank != self.data.rank
				|| parts != self.data.parts
				|| targets.len() != self.data.header.num_qubits()?
				|| value & !mask != 0
			{
				return Err(PersistenceError::Collective(
					"replay ownership/operand count",
				));
			}
			let mut occupied = mask;
			for &target in targets {
				let bit = 1usize
					.checked_shl(u32::try_from(target).map_err(|_| Error::Overflow)?)
					.ok_or(Error::Overflow)?;
				if occupied & bit != 0 || width.is_some_and(|w| target >= w) {
					return Err(PersistenceError::Collective("replay operand overlap/width"));
				}
				occupied |= bit;
			}
			if width.is_some_and(|w| {
				mask >= 1usize
					.checked_shl(u32::try_from(w).unwrap_or(u32::MAX))
					.unwrap_or(0)
			}) {
				return Err(PersistenceError::Collective("replay controls width"));
			}
			Ok(())
		})();
		agree(lane, valid)?;
		equal(lane, &self.data.manifest_identity)?;
		for value in [
			word(targets.len())?,
			word(mask)?,
			word(value)?,
			u64::from(adjoint),
			word(width.unwrap_or(usize::MAX))?,
		] {
			equal(lane, &value.to_le_bytes())?;
		}
		for &target in targets {
			equal(lane, &word(target)?.to_le_bytes())?;
		}
		Ok(())
	}
}
fn fatal<T>(operation: impl FnOnce() -> quest_qsvt::Result<T>) -> T {
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
}

/// Load owned buckets and verify source records and inverse permutation closure.
///
/// Portable gate admission is a separate [`LoadedMatchingResource::admit_replay`] operation.
/// All metadata and owner failures reject collectively before any native state mutation.
/// # Errors
/// Rejects incompatible rank counts, missing/malformed resources and explicit budgets.
/// Common limits and initial payload admission precede manifest IO; actual source
/// capacities are agreed before bucket reads or reverse routing. HDF5 metadata,
/// allocator overhead and temporary rejected allocations are outside the payload cap.
#[allow(
	clippy::too_many_lines,
	reason = "Linear collective restart protocol keeps owner agreements beside IO stages"
)]
pub fn load_matching_resource(
	communicator: &MpiCommunicator<'_>,
	manifest_path: impl AsRef<Path>,
	directory: impl AsRef<Path>,
	limits: ResourceLoadLimits,
) -> Result<LoadedMatchingResource> {
	let started = std::time::Instant::now();
	let mut statistics = LoadStatistics::default();
	let rank = usize::try_from(communicator.rank().context("reading restart rank")?)
		.map_err(|_| Error::Overflow)?;
	let parts = usize::try_from(communicator.size().context("reading restart size")?)
		.map_err(|_| Error::Overflow)?;
	let mut lane = communicator
		.collective_lane()
		.context("borrowing restart lane")?;
	loading::start_resource(&mut lane, rank, parts, limits)?;
	#[cfg(test)]
	loading::capacity_tests::manifest();
	let manifest = agree(
		&mut lane,
		loading::caught(|| {
			MatchingManifest::open(manifest_path, limits.io).map_err(PersistenceError::from)
		}),
	)?;
	let header = agree(&mut lane, manifest.header().map_err(PersistenceError::from))?;
	common_resource(&mut lane, header, parts, manifest.buckets().len(), limits)?;
	equal(&mut lane, &manifest.semantic_sha256())?;
	let count = agree(
		&mut lane,
		loading::caught(|| {
			let mut count = 0usize;
			for bucket in manifest.owned_buckets(rank, parts)? {
				count = count
					.checked_add(
						manifest
							.buckets()
							.get(bucket)
							.ok_or(Error::Overflow)?
							.records,
					)
					.ok_or(Error::Overflow)?;
			}
			let logarithm = usize::try_from(count.max(1).ilog2()).map_err(|_| Error::Overflow)?;
			let work = header
				.record_count
				.checked_mul(
					logarithm
						.checked_mul(4)
						.and_then(|n| n.checked_add(parts.checked_mul(32)?))
						.and_then(|n| n.checked_add(128))
						.and_then(|n| n.checked_add(quest_qsvt::record_fingerprint_work(7).ok()?))
						.ok_or(Error::Overflow)?,
				)
				.and_then(|n| n.checked_add(limits.io.max_manifest_bytes))
				.and_then(|n| n.checked_add(parts.checked_mul(512)?.checked_add(4096)?))
				.ok_or(Error::Overflow)?;
			if work > limits.max_work {
				return Err(PersistenceError::Collective("restart preparation work"));
			}
			if count > limits.max_local_records || count > header.record_count {
				return Err(PersistenceError::Collective("restart local storage"));
			}
			loading::admit(count, count, limits)?;
			Ok(count)
		}),
	)?;
	let mut records = agree(&mut lane, reserve(count))?;
	#[cfg(test)]
	agree(
		&mut lane,
		loading::capacity_tests::inflate(&mut records, rank, "records"),
	)?;
	agree(
		&mut lane,
		loading::caught(|| loading::admit(records.capacity(), count, limits)),
	)?;
	statistics.manifest_and_admission = started.elapsed();
	let phase = std::time::Instant::now();
	let own = loading::caught(|| {
		#[cfg(test)]
		loading::capacity_tests::bucket();
		for bucket in manifest.owned_buckets(rank, parts)? {
			let input = manifest.open_bucket(directory.as_ref(), bucket, limits.io)?;
			input.visit_records(false, |chunk| {
				if chunk.len() > count.saturating_sub(records.len()) {
					return Err(quest_qsvt_io::Error::Budget("restart record coverage"));
				}
				records.extend(chunk.iter().copied().map(from_disk));
				Ok(())
			})?;
		}
		records.sort_unstable_by_key(|r| (r.column.color, r.column.source));
		for record in &records {
			record.validate(header)?;
			if record.column.source.checked_rem(parts) != Some(rank) {
				return Err(PersistenceError::Collective("restart cyclic ownership"));
			}
		}
		if records.windows(2).any(|pair|matches!(pair,[a,b] if (a.column.color,a.column.source)==(b.column.color,b.column.source))) {return Err(PersistenceError::Collective("duplicate restart column"));}
		Ok(records)
	});
	let records = agree(&mut lane, own)?;
	reduce_summary(&mut lane, rank, parts, &records, header)?;
	statistics.read_validate = phase.elapsed();
	let phase = std::time::Instant::now();
	let (reverse, reverse_broadcasts) = build_reverse(
		&mut lane,
		rank,
		parts,
		&records,
		records.capacity(),
		header,
		limits,
	)?;
	statistics.reverse_directory = phase.elapsed();
	statistics.reverse_broadcasts = reverse_broadcasts;
	statistics.local_records = records.len();
	statistics.local_reverse_records = reverse.len();
	statistics.local_record_capacity = records.capacity();
	statistics.local_reverse_capacity = reverse.capacity();
	agree(
		&mut lane,
		loading::caught(|| loading::admit(records.capacity(), reverse.capacity(), limits)),
	)?;
	let local_bytes = agree(
		&mut lane,
		loading::caught(|| retained(records.capacity().max(reverse.capacity()))),
	)?;
	let common_bytes = maximum(&mut lane, rank, parts, local_bytes)?;
	let max_records = maximum(&mut lane, rank, parts, records.len())?;
	let query_work = parts
		.checked_mul(32)
		.and_then(|n| {
			n.checked_add(
				usize::try_from(max_records.max(1).ilog2())
					.ok()?
					.checked_add(1)?
					.checked_mul(8)?,
			)
		})
		.and_then(|n| n.checked_add(128))
		.ok_or(Error::Overflow)?;
	let query_wire = parts
		.checked_mul(16)
		.and_then(|n| n.checked_add(88))
		.and_then(|n| n.checked_mul(parts.saturating_sub(1)))
		.ok_or(Error::Overflow)?;
	let manifest_identity = manifest.semantic_sha256();
	let frozen_identity = u64::from_le_bytes(
		manifest_identity
			.get(..8)
			.ok_or(Error::Overflow)?
			.try_into()
			.map_err(|_| Error::Overflow)?,
	);
	let data = Data {
		header,
		rank,
		parts,
		records,
		reverse,
		frozen_identity,
		manifest_identity,
		common_bytes,
		query_work,
		query_wire,
		load_statistics: statistics,
	};
	drop(lane);
	let mut data = data;
	data.load_statistics.total = started.elapsed();
	Ok(LoadedMatchingResource {
		data: Arc::new(data),
	})
}

/// Load and fully admit a portable replay recipe, preserving the compatibility contract.
///
/// All persistence and replay limits are agreed before manifest IO. Native-only
/// callers can use [`load_matching_resource`] without paying portable gate admission.
/// # Errors
/// Rejects collective metadata, malformed resources and loading/replay budgets.
pub fn load_matching(
	communicator: &MpiCommunicator<'_>,
	manifest_path: impl AsRef<Path>,
	directory: impl AsRef<Path>,
	limits: PersistenceLimits,
) -> Result<LoadedMatching> {
	let started = std::time::Instant::now();
	let rank = usize::try_from(communicator.rank().context("reading restart rank")?)
		.map_err(|_| Error::Overflow)?;
	let parts = usize::try_from(communicator.size().context("reading restart size")?)
		.map_err(|_| Error::Overflow)?;
	let mut lane = communicator
		.collective_lane()
		.context("borrowing restart admission lane")?;
	loading::start(&mut lane, rank, parts, limits)?;
	drop(lane);
	let resource = load_matching_resource(communicator, manifest_path, directory, limits.into())?;
	let mut loaded = resource.admit_replay(communicator, limits.replay)?;
	loaded.statistics.total = started.elapsed();
	Ok(loaded)
}
