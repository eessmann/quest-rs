//! Scalar collective directories; one requested record lives at a time.
use super::{Data, PersistenceError, ResourceLoadLimits, Result, agree, index, reserve, word};
use crate::{Error, qsvt::matching::preprocess::ReverseRecord};
use quest_qsvt::{
	MatchingHeader, MatchingShard,
	matching_resource::{MatchingResource, ResourceMatchingRecord},
};
use quest_sys::mpi::MpiCollectiveLane;
use std::cell::{Cell, RefCell};

pub(super) fn mpi<T>(result: quest_sys::QuestResult<T>) -> T {
	result.unwrap_or_else(|_| quest_sys::mpi::abort_job())
}
pub(super) fn maximum(
	lane: &mut MpiCollectiveLane<'_>,
	rank: usize,
	parts: usize,
	value: usize,
) -> Result<usize> {
	let mut maximum = 0;
	for peer in 0..parts {
		let mut packet = if peer == rank { word(value)? } else { 0 }.to_le_bytes();
		mpi(lane.broadcast_bytes(
			i32::try_from(peer).map_err(|_| Error::Overflow)?,
			&mut packet,
		));
		maximum = maximum.max(index(u64::from_le_bytes(packet))?);
	}
	Ok(maximum)
}
pub(super) fn reduce_summary(
	lane: &mut MpiCollectiveLane<'_>,
	rank: usize,
	parts: usize,
	records: &[ResourceMatchingRecord],
	header: MatchingHeader,
) -> Result<()> {
	let mut own = 0u64;
	for record in records {
		own = own.wrapping_add(
			MatchingShard::summarize_records(std::slice::from_ref(&record.column))?.1,
		);
	}
	let mut count = 0usize;
	let mut digest = 0u64;
	for peer in 0..parts {
		let mut packet = [0; 16];
		if rank == peer {
			packet
				.get_mut(..8)
				.ok_or(Error::Overflow)?
				.copy_from_slice(&word(records.len())?.to_le_bytes());
			packet
				.get_mut(8..)
				.ok_or(Error::Overflow)?
				.copy_from_slice(&own.to_le_bytes());
		}
		mpi(lane.broadcast_bytes(
			i32::try_from(peer).map_err(|_| Error::Overflow)?,
			&mut packet,
		));
		count = count
			.checked_add(index(u64::from_le_bytes(
				packet
					.get(..8)
					.ok_or(Error::Overflow)?
					.try_into()
					.map_err(|_| Error::Overflow)?,
			))?)
			.ok_or(Error::Overflow)?;
		digest = digest.wrapping_add(u64::from_le_bytes(
			packet
				.get(8..)
				.ok_or(Error::Overflow)?
				.try_into()
				.map_err(|_| Error::Overflow)?,
		));
	}
	agree(
		lane,
		if count == header.record_count && digest == header.record_digest {
			Ok(())
		} else {
			Err(PersistenceError::Collective("restart payload integrity"))
		},
	)
}
#[allow(
	clippy::too_many_lines,
	reason = "Scalar owner routing and collective closure validation form one protocol"
)]
pub(super) fn build_reverse(
	lane: &mut MpiCollectiveLane<'_>,
	rank: usize,
	parts: usize,
	records: &[ResourceMatchingRecord],
	record_capacity: usize,
	header: MatchingHeader,
	limits: ResourceLoadLimits,
) -> Result<(Vec<ReverseRecord>, u64)> {
	let wire = header
		.record_count
		.checked_mul(
			parts
				.checked_sub(1)
				.and_then(|n| n.checked_mul(8))
				.and_then(|n| n.checked_add(24))
				.ok_or(Error::Overflow)?,
		)
		.and_then(|n| n.checked_add(parts.checked_mul(8)?.checked_mul(parts.saturating_sub(1))?))
		.ok_or(Error::Overflow)?;
	agree(
		lane,
		if wire > limits.max_communication_bytes || header.record_count > limits.max_work {
			Err(PersistenceError::Collective(
				"reverse directory wire/work budget",
			))
		} else {
			Ok(())
		},
	)?;
	let mut reverse = agree(lane, reserve(records.len()))?;
	#[cfg(test)]
	agree(
		lane,
		super::loading::capacity_tests::inflate(&mut reverse, rank, "reverse"),
	)?;
	agree(
		lane,
		super::loading::caught(|| {
			super::loading::admit(record_capacity, reverse.capacity(), limits)
		}),
	)?;
	#[cfg(test)]
	super::loading::capacity_tests::routing();
	let mut valid = true;
	let mut broadcasts = 0u64;
	for sender in 0..parts {
		let mut count = if sender == rank {
			word(records.len())?
		} else {
			0
		}
		.to_le_bytes();
		broadcasts = broadcasts.checked_add(1).ok_or(Error::Overflow)?;
		mpi(lane.broadcast_bytes(
			i32::try_from(sender).map_err(|_| Error::Overflow)?,
			&mut count,
		));
		for ordinal in 0..index(u64::from_le_bytes(count))? {
			let mut owner = [0; 8];
			let mut packet = [0; 24];
			if rank == sender {
				let c = records.get(ordinal).ok_or(Error::Overflow)?.column;
				owner =
					word(c.destination.checked_rem(parts).ok_or(Error::Overflow)?)?.to_le_bytes();
				for (value, bytes) in [word(c.color)?, word(c.destination)?, word(c.source)?]
					.into_iter()
					.zip(packet.as_chunks_mut::<8>().0)
				{
					bytes.copy_from_slice(&value.to_le_bytes());
				}
			}
			broadcasts = broadcasts.checked_add(1).ok_or(Error::Overflow)?;
			mpi(lane.broadcast_bytes(
				i32::try_from(sender).map_err(|_| Error::Overflow)?,
				&mut owner,
			));
			let receiver = index(u64::from_le_bytes(owner))?;
			if receiver >= parts {
				quest_sys::mpi::abort_job();
			}
			if sender != receiver {
				if rank == sender {
					mpi(lane.send_bytes(
						&packet,
						i32::try_from(receiver).map_err(|_| Error::Overflow)?,
						30220,
					));
				} else if rank == receiver {
					let packet_length = mpi(lane.receive_bytes(
						&mut packet,
						i32::try_from(sender).map_err(|_| Error::Overflow)?,
						30220,
					));
					if packet_length != packet.len() {
						quest_sys::mpi::abort_job();
					}
				}
			}
			if rank == receiver {
				let fields = packet.as_chunks::<8>().0;
				let color = index(u64::from_le_bytes(*fields.first().ok_or(Error::Overflow)?))?;
				let destination =
					index(u64::from_le_bytes(*fields.get(1).ok_or(Error::Overflow)?))?;
				let source = index(u64::from_le_bytes(*fields.get(2).ok_or(Error::Overflow)?))?;
				if records
					.binary_search_by_key(&(color, destination), |r| {
						(r.column.color, r.column.source)
					})
					.is_err()
					|| reverse.len() == records.len()
				{
					valid = false;
				} else {
					reverse.push(ReverseRecord {
						color,
						destination,
						source,
					});
				}
			}
		}
	}
	reverse.sort_unstable_by_key(|r| (r.color, r.destination));
	valid &= reverse.len() == records.len()
		&& !reverse
			.windows(2)
			.any(|pair| matches!(pair,[a,b] if (a.color,a.destination)==(b.color,b.destination)));
	agree(
		lane,
		if valid {
			Ok((reverse, broadcasts))
		} else {
			Err(PersistenceError::Collective(
				"restart permutation is not closed/bijective",
			))
		},
	)
}

pub(super) struct Directory<'a, 'lane> {
	data: &'a Data,
	lane: RefCell<MpiCollectiveLane<'lane>>,
	next_calls: Cell<u64>,
	forward_calls: Cell<u64>,
	reverse_calls: Cell<u64>,
	broadcasts: Cell<u64>,
}
impl std::fmt::Debug for Directory<'_, '_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("CollectiveMatchingDirectory")
			.field("rank", &self.data.rank)
			.field("parts", &self.data.parts)
			.finish_non_exhaustive()
	}
}
impl<'a, 'lane> Directory<'a, 'lane> {
	pub const fn new(data: &'a Data, lane: MpiCollectiveLane<'lane>) -> Self {
		Self {
			data,
			lane: RefCell::new(lane),
			next_calls: Cell::new(0),
			forward_calls: Cell::new(0),
			reverse_calls: Cell::new(0),
			broadcasts: Cell::new(0),
		}
	}
}
impl Directory<'_, '_> {
	pub const fn statistics(&self) -> super::LoadDirectoryStatistics {
		super::LoadDirectoryStatistics {
			next_record_calls: self.next_calls.get(),
			forward_calls: self.forward_calls.get(),
			reverse_calls: self.reverse_calls.get(),
			broadcasts: self.broadcasts.get(),
		}
	}
	fn count(counter: &Cell<u64>) -> quest_qsvt::Result<()> {
		counter.set(
			counter
				.get()
				.checked_add(1)
				.ok_or(quest_qsvt::Error::Budget(
					"loader diagnostic counter overflow",
				))?,
		);
		Ok(())
	}
	fn broadcast(&self, root: i32, packet: &mut [u8]) -> quest_qsvt::Result<()> {
		Self::count(&self.broadcasts)?;
		mpi(self.lane.borrow_mut().broadcast_bytes(root, packet));
		Ok(())
	}
}

fn encode(record: Option<ResourceMatchingRecord>) -> quest_qsvt::Result<[u8; 88]> {
	let mut packet = [0; 88];
	if let Some(r) = record {
		let c = r.column;
		let words = [
			1,
			u64::try_from(c.color)
				.map_err(|_| quest_qsvt::Error::Budget("resource record word"))?,
			u64::try_from(c.source)
				.map_err(|_| quest_qsvt::Error::Budget("resource record word"))?,
			u64::try_from(c.destination)
				.map_err(|_| quest_qsvt::Error::Budget("resource record word"))?,
			c.cosine.to_bits(),
			c.sine.to_bits(),
			c.phase.re.to_bits(),
			c.phase.im.to_bits(),
			r.theta.to_bits(),
			r.phase_angle.to_bits(),
			u64::from(r.is_edge),
		];
		for (value, bytes) in words.into_iter().zip(packet.as_chunks_mut::<8>().0) {
			bytes.copy_from_slice(&value.to_le_bytes());
		}
	}
	Ok(packet)
}
fn decode(packet: [u8; 88]) -> quest_qsvt::Result<Option<ResourceMatchingRecord>> {
	let words = packet.as_chunks::<8>().0;
	let field = |index: usize| -> quest_qsvt::Result<u64> {
		Ok(u64::from_le_bytes(*words.get(index).ok_or(
			quest_qsvt::Error::Encoding("resource record packet"),
		)?))
	};
	if field(0)? == 0 {
		return Ok(None);
	}
	if field(0)? != 1 {
		return Err(quest_qsvt::Error::Encoding("resource owner lookup failed"));
	}
	let index = |i| -> quest_qsvt::Result<usize> {
		usize::try_from(field(i)?).map_err(|_| quest_qsvt::Error::Budget("resource record index"))
	};
	Ok(Some(ResourceMatchingRecord {
		column: quest_qsvt::MatchingColumn {
			color: index(1)?,
			source: index(2)?,
			destination: index(3)?,
			cosine: f64::from_bits(field(4)?),
			sine: f64::from_bits(field(5)?),
			phase: quest_qsvt::Complex64::new(f64::from_bits(field(6)?), f64::from_bits(field(7)?)),
		},
		theta: f64::from_bits(field(8)?),
		phase_angle: f64::from_bits(field(9)?),
		is_edge: field(10)? == 1,
	}))
}
impl MatchingResource for Directory<'_, '_> {
	fn header(&self) -> MatchingHeader {
		self.data.header
	}
	fn frozen_identity(&self) -> u64 {
		self.data.frozen_identity
	}
	fn retained_bytes(&self) -> quest_qsvt::Result<usize> {
		Ok(self.data.common_bytes)
	}
	fn query_work_bound(&self) -> usize {
		self.data.query_work
	}
	fn query_communication_bound(&self) -> usize {
		self.data.query_wire
	}
	fn next_record(
		&self,
		color: usize,
		bound: Option<usize>,
		reverse: bool,
	) -> quest_qsvt::Result<Option<ResourceMatchingRecord>> {
		Self::count(&self.next_calls)?;
		let candidate = if reverse {
			self.data
				.records
				.partition_point(|r| {
					r.column.color < color
						|| (r.column.color == color && bound.is_none_or(|b| r.column.source < b))
				})
				.checked_sub(1)
				.and_then(|i| self.data.records.get(i))
		} else {
			self.data
				.records
				.get(self.data.records.partition_point(|r| {
					r.column.color < color
						|| (r.column.color == color && bound.is_some_and(|b| r.column.source <= b))
				}))
		};
		let candidate = candidate
			.filter(|r| r.column.color == color)
			.map(|r| r.column.source);
		let mut selected = None;
		for peer in 0..self.data.parts {
			let mut packet = [0; 16];
			if self.data.rank == peer
				&& let Some(source) = candidate
			{
				packet
					.get_mut(..8)
					.ok_or(quest_qsvt::Error::Encoding("resource candidate packet"))?
					.copy_from_slice(&1u64.to_le_bytes());
				packet
					.get_mut(8..)
					.ok_or(quest_qsvt::Error::Encoding("resource candidate packet"))?
					.copy_from_slice(
						&u64::try_from(source)
							.map_err(|_| quest_qsvt::Error::Budget("resource candidate word"))?
							.to_le_bytes(),
					);
			}
			self.broadcast(
				i32::try_from(peer)
					.map_err(|_| quest_qsvt::Error::Budget("resource owner word"))?,
				&mut packet,
			)?;
			let fields = packet.as_chunks::<8>().0;
			if u64::from_le_bytes(
				*fields
					.first()
					.ok_or(quest_qsvt::Error::Encoding("resource candidate packet"))?,
			) != 0
			{
				let source = usize::try_from(u64::from_le_bytes(
					*fields
						.get(1)
						.ok_or(quest_qsvt::Error::Encoding("resource candidate packet"))?,
				))
				.map_err(|_| quest_qsvt::Error::Budget("resource candidate word"))?;
				selected = Some(selected.map_or(source, |old: usize| {
					if reverse {
						old.max(source)
					} else {
						old.min(source)
					}
				}));
			}
		}
		selected
			.map(|source| self.forward(color, source))
			.transpose()
			.map(Option::flatten)
	}
	fn forward(
		&self,
		color: usize,
		source: usize,
	) -> quest_qsvt::Result<Option<ResourceMatchingRecord>> {
		Self::count(&self.forward_calls)?;
		let owner = source
			.checked_rem(self.data.parts)
			.ok_or(quest_qsvt::Error::Encoding("resource ownership"))?;
		let mut packet = if owner == self.data.rank {
			let record = self
				.data
				.records
				.binary_search_by_key(&(color, source), |r| (r.column.color, r.column.source))
				.ok()
				.and_then(|i| self.data.records.get(i))
				.copied();
			encode(record).unwrap_or_else(|_| {
				let mut error = [0; 88];
				if let Some(first) = error.first_mut() {
					*first = 2;
				}
				error
			})
		} else {
			[0; 88]
		};
		self.broadcast(
			i32::try_from(owner).map_err(|_| quest_qsvt::Error::Budget("resource owner word"))?,
			&mut packet,
		)?;
		decode(packet)
	}
	fn reverse(&self, color: usize, destination: usize) -> quest_qsvt::Result<Option<usize>> {
		Self::count(&self.reverse_calls)?;
		let owner = destination
			.checked_rem(self.data.parts)
			.ok_or(quest_qsvt::Error::Encoding("resource ownership"))?;
		let mut packet = [0; 16];
		if owner == self.data.rank
			&& let Some(record) = self
				.data
				.reverse
				.binary_search_by_key(&(color, destination), |r| (r.color, r.destination))
				.ok()
				.and_then(|i| self.data.reverse.get(i))
		{
			packet
				.get_mut(..8)
				.ok_or(quest_qsvt::Error::Encoding("resource inverse packet"))?
				.copy_from_slice(&1u64.to_le_bytes());
			packet
				.get_mut(8..)
				.ok_or(quest_qsvt::Error::Encoding("resource inverse packet"))?
				.copy_from_slice(
					&u64::try_from(record.source)
						.map_err(|_| quest_qsvt::Error::Budget("resource inverse word"))?
						.to_le_bytes(),
				);
		}
		self.broadcast(
			i32::try_from(owner).map_err(|_| quest_qsvt::Error::Budget("resource owner word"))?,
			&mut packet,
		)?;
		let fields = packet.as_chunks::<8>().0;
		if u64::from_le_bytes(
			*fields
				.first()
				.ok_or(quest_qsvt::Error::Encoding("resource inverse packet"))?,
		) == 0
		{
			Ok(None)
		} else {
			Ok(Some(
				usize::try_from(u64::from_le_bytes(
					*fields
						.get(1)
						.ok_or(quest_qsvt::Error::Encoding("resource inverse packet"))?,
				))
				.map_err(|_| quest_qsvt::Error::Budget("resource inverse word"))?,
			))
		}
	}
}
