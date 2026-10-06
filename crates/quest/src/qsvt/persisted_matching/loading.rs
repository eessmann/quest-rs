//! Loader Rust payload boundaries; opaque IO/native/allocator costs are separate.
use super::{
	Data, LoadedMatching, PersistenceError, PersistenceLimits, ResourceLoadLimits, Result, agree,
	word,
};
use crate::{Error, collective::equal};
use quest_qsvt::matching_resource::ResourceMatchingRecord;
use quest_sys::mpi::MpiCollectiveLane;
/// Observed logical calls made while admitting the portable replay recipe.
///
/// A forward lookup performed by `next_record` is included in `forward_calls`.
/// Broadcasts count this library's checked broadcast calls, not MPI-internal
/// collectives or measured wire bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadDirectoryStatistics {
	pub next_record_calls: u64,
	pub forward_calls: u64,
	pub reverse_calls: u64,
	pub broadcasts: u64,
}

/// Rank-local observations of a successful immutable load.
///
/// Timings are wall-clock durations, include collective waiting, and are not
/// rank maxima. `total` covers loading plus any successful portable admission;
/// caller idle time, shard cloning and native preparation are outside this scope.
/// Unlisted bookkeeping is included in `total`, so the phases need not sum to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadStatistics {
	/// Whether these observations include successful portable gate admission.
	pub replay_admitted: bool,
	pub manifest_and_admission: std::time::Duration,
	/// Owned bucket reads, local record validation and global payload summary.
	pub read_validate: std::time::Duration,
	pub reverse_directory: std::time::Duration,
	pub replay_admission: std::time::Duration,
	pub total: std::time::Duration,
	pub local_records: usize,
	pub local_reverse_records: usize,
	pub local_record_capacity: usize,
	pub local_reverse_capacity: usize,
	pub reverse_broadcasts: u64,
	pub admission: LoadDirectoryStatistics,
}

#[cfg(test)]
pub(super) mod capacity_tests;
fn fixed() -> Result<usize> {
	[
		16_384,
		4096,
		size_of::<Data>(),
		size_of::<LoadedMatching>(),
		size_of::<quest_qsvt_io::sharded_matching::MatchingManifest>(),
		size_of::<quest_qsvt_io::sharded_matching::MatchingBucketInput>(),
		size_of::<[usize; 2]>(),
	]
	.into_iter()
	.try_fold(0usize, |a, b| {
		Ok(a.checked_add(b).ok_or(Error::Overflow)?)
	})
}
pub(super) fn peak(
	records: usize,
	reverse: usize,
	l: impl Into<ResourceLoadLimits>,
) -> Result<usize> {
	let l = l.into();
	let a = records
		.checked_mul(size_of::<ResourceMatchingRecord>())
		.ok_or(Error::Overflow)?;
	let b = reverse
		.checked_mul(size_of::<super::ReverseRecord>())
		.ok_or(Error::Overflow)?;
	[
		a,
		b,
		l.io.max_manifest_bytes,
		l.io.max_buffer_bytes,
		fixed()?,
	]
	.into_iter()
	.try_fold(0usize, |a, b| {
		Ok(a.checked_add(b).ok_or(Error::Overflow)?)
	})
}
pub(super) fn admit(
	records: usize,
	reverse: usize,
	l: impl Into<ResourceLoadLimits>,
) -> Result<()> {
	let l = l.into();
	let requested = peak(records, reverse, l)?;
	if requested > l.max_bytes {
		return Err(Error::Budget {
			requested,
			available: l.max_bytes,
		}
		.into());
	}
	Ok(())
}
pub(super) fn caught<T>(operation: impl FnOnce() -> Result<T>) -> Result<T> {
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
		.unwrap_or(Err(PersistenceError::Collective("loader local panic")))
}
pub(super) fn start(
	lane: &mut MpiCollectiveLane<'_>,
	rank: usize,
	parts: usize,
	l: PersistenceLimits,
) -> Result<()> {
	let frame = agree(
		lane,
		caught(|| {
			if !parts.is_power_of_two() || rank >= parts {
				return Err(PersistenceError::Collective("loader communicator shape"));
			}
			admit(0, 0, l)?;
			let floor = parts
				.checked_mul(512)
				.and_then(|n| n.checked_add(4096))
				.and_then(|n| n.checked_add(l.io.max_manifest_bytes))
				.ok_or(Error::Overflow)?;
			if floor > l.replay.max_work {
				return Err(PersistenceError::Collective("loader coordinator work"));
			}
			let words = [
				1,
				word(parts)?,
				word(l.max_local_records)?,
				word(l.max_bytes)?,
				word(l.max_communication_bytes)?,
				word(l.io.chunk_records)?,
				word(l.io.max_buffer_bytes)?,
				word(l.io.max_manifest_bytes)?,
				word(l.io.max_buckets)?,
				word(l.io.max_records)?,
				l.io.max_file_bytes,
				word(l.replay.max_bytes)?,
				word(l.replay.max_queries)?,
				word(l.replay.max_work)?,
				word(l.replay.max_communication_bytes)?,
				word(l.replay.max_gates)?,
			];
			let mut frame = [0u8; 128];
			for (word, slot) in words.into_iter().zip(frame.as_chunks_mut::<8>().0) {
				slot.copy_from_slice(&word.to_le_bytes());
			}
			Ok(frame)
		}),
	)?;
	equal(lane, &frame)?;
	Ok(())
}

pub(super) fn start_resource(
	lane: &mut MpiCollectiveLane<'_>,
	rank: usize,
	parts: usize,
	l: ResourceLoadLimits,
) -> Result<()> {
	let frame = agree(
		lane,
		caught(|| {
			if !parts.is_power_of_two() || rank >= parts {
				return Err(PersistenceError::Collective("loader communicator shape"));
			}
			admit(0, 0, l)?;
			let floor = parts
				.checked_mul(512)
				.and_then(|n| n.checked_add(4096))
				.and_then(|n| n.checked_add(l.io.max_manifest_bytes))
				.ok_or(Error::Overflow)?;
			if floor > l.max_work {
				return Err(PersistenceError::Collective("loader coordinator work"));
			}
			let words = [
				2,
				word(parts)?,
				word(l.max_local_records)?,
				word(l.max_bytes)?,
				word(l.max_communication_bytes)?,
				word(l.io.chunk_records)?,
				word(l.io.max_buffer_bytes)?,
				word(l.io.max_manifest_bytes)?,
				word(l.io.max_buckets)?,
				word(l.io.max_records)?,
				l.io.max_file_bytes,
				word(l.max_work)?,
			];
			let mut frame = [0u8; 96];
			for (word, slot) in words.into_iter().zip(frame.as_chunks_mut::<8>().0) {
				slot.copy_from_slice(&word.to_le_bytes());
			}
			Ok(frame)
		}),
	)?;
	equal(lane, &frame)?;
	Ok(())
}
