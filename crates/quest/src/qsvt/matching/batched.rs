//! Bounded ownership routing. Each native owner contributes at most B flag pairs,
//! so all per-rank sends/receives contain at most 2B amplitude records even when
//! coefficient or permutation ownership is maximally imbalanced.
use super::{MatchingLayout, collective::get, staging::RoutingState};
use crate::{Complex64, error::BackendResult, values::reserve_vec};
use quest_qsvt::MatchingShard;
use quest_sys::mpi::MpiCollectiveLane;

const PAIR_LIMIT: usize = 64;
const PACKET_BYTES: usize = 40;
#[derive(Debug, Clone, Copy, Default)]
pub struct RoutingStatistics {
	pub batches: usize,
	/// Flag-zero basis candidates visited by this native state owner only.
	pub local_pair_candidates: usize,
	pub maximum_batch_pairs: usize,
	pub maximum_routed_amplitudes: usize,
	pub coordination_calls: usize,
	pub indexed_reads: usize,
	pub indexed_writes: usize,
	/// Application bytes sent by this rank, including batch counts; saturates on overflow.
	/// Excludes MPI protocol overhead and separate collective/native traffic.
	pub point_to_point_sent_bytes: usize,
	/// Application bytes received by this rank, with the same exclusions as sent bytes.
	pub point_to_point_received_bytes: usize,
}
#[derive(Clone, Copy)]
struct Packet {
	destination: usize,
	key: usize,
	index: usize,
	flag: usize,
	value: Complex64,
}
struct Pair {
	key: usize,
	values: [Complex64; 2],
	seen: u8,
}
#[derive(Clone, Copy)]
struct Route {
	rank: usize,
	parts: usize,
	local: usize,
}

fn local_pairs(route: Route, owner: usize, flag: usize) -> crate::Result<usize> {
	let start = owner
		.checked_mul(route.local)
		.ok_or(crate::Error::Overflow)?;
	if flag < route.local {
		route.local.checked_div(2).ok_or(crate::Error::Overflow)
	} else {
		Ok(if start & flag == 0 { route.local } else { 0 })
	}
}
fn local_pair_basis(route: Route, owner: usize, flag: usize, index: usize) -> crate::Result<usize> {
	let start = owner
		.checked_mul(route.local)
		.ok_or(crate::Error::Overflow)?;
	let offset = if flag < route.local {
		// Insert the omitted zero flag bit into the packed local pair index.
		let low = index & flag.saturating_sub(1);
		let high = (index & !flag.saturating_sub(1))
			.checked_mul(2)
			.ok_or(crate::Error::Overflow)?;
		low | high
	} else {
		index
	};
	start.checked_add(offset).ok_or(crate::Error::Overflow)
}

pub(super) struct RoutingWorkspace {
	bases: Vec<usize>,
	input: Vec<Packet>,
	received: Vec<Packet>,
	send_bytes: Vec<u8>,
	receive_bytes: Vec<u8>,
	indices: Vec<i64>,
	values: Vec<quest_sys::QuestComplex>,
	pairs: Vec<Pair>,
}
impl RoutingWorkspace {
	pub(super) fn bytes() -> crate::Result<usize> {
		let amplitudes = PAIR_LIMIT.checked_mul(2).ok_or(crate::Error::Overflow)?;
		amplitudes
			.checked_mul(
				size_of::<Packet>()
					.saturating_mul(2)
					.saturating_add(PACKET_BYTES.saturating_mul(2))
					.saturating_add(size_of::<i64>())
					.saturating_add(size_of::<quest_sys::QuestComplex>()),
			)
			.and_then(|n| {
				n.checked_add(
					PAIR_LIMIT.checked_mul(size_of::<usize>().saturating_add(size_of::<Pair>()))?,
				)
			})
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.ok_or(crate::Error::Overflow)
	}
	pub(super) fn retained_bytes(&self) -> crate::Result<usize> {
		[
			(self.bases.capacity(), size_of::<usize>()),
			(self.input.capacity(), size_of::<Packet>()),
			(self.received.capacity(), size_of::<Packet>()),
			(self.send_bytes.capacity(), 1),
			(self.receive_bytes.capacity(), 1),
			(self.indices.capacity(), size_of::<i64>()),
			(self.values.capacity(), size_of::<quest_sys::QuestComplex>()),
			(self.pairs.capacity(), size_of::<Pair>()),
		]
		.into_iter()
		.try_fold(size_of::<Self>(), |sum, (count, width)| {
			count
				.checked_mul(width)
				.and_then(|n| sum.checked_add(n))
				.ok_or(crate::Error::Overflow)
		})
	}
	pub(super) fn new() -> crate::Result<Self> {
		let amplitudes = PAIR_LIMIT.checked_mul(2).ok_or(crate::Error::Overflow)?;
		let bytes = amplitudes
			.checked_mul(PACKET_BYTES)
			.ok_or(crate::Error::Overflow)?;
		Ok(Self {
			bases: reserve_vec(PAIR_LIMIT)?,
			input: reserve_vec(amplitudes)?,
			received: reserve_vec(amplitudes)?,
			send_bytes: reserve_vec(bytes)?,
			receive_bytes: reserve_vec(bytes)?,
			indices: reserve_vec(amplitudes)?,
			values: reserve_vec(amplitudes)?,
			pairs: reserve_vec(PAIR_LIMIT)?,
		})
	}
	fn exchange(
		&mut self,
		lane: &mut MpiCollectiveLane<'_>,
		route: Route,
		statistics: &mut RoutingStatistics,
	) -> crate::Result<()> {
		let limit = PAIR_LIMIT.checked_mul(2).ok_or(crate::Error::Overflow)?;
		if self.input.len() > limit {
			return Err(crate::Error::Value("matching send batch exceeds bound"));
		}
		self.received.clear();
		for offset in 0..route.parts {
			let peer = route.rank ^ offset;
			if peer == route.rank {
				self.received.extend(
					self.input
						.iter()
						.filter(|packet| packet.destination == peer)
						.copied(),
				);
				continue;
			}
			self.send_bytes.clear();
			for packet in self
				.input
				.iter()
				.filter(|packet| packet.destination == peer)
			{
				for word in [
					u64::try_from(packet.key).map_err(|_| crate::Error::Overflow)?,
					u64::try_from(packet.index).map_err(|_| crate::Error::Overflow)?,
					u64::try_from(packet.flag).map_err(|_| crate::Error::Overflow)?,
					packet.value.re.to_bits(),
					packet.value.im.to_bits(),
				] {
					self.send_bytes.extend_from_slice(&word.to_le_bytes());
				}
			}
			let count = u64::try_from(
				self.send_bytes
					.len()
					.checked_div(PACKET_BYTES)
					.ok_or(crate::Error::Overflow)?,
			)
			.map_err(|_| crate::Error::Overflow)?
			.to_le_bytes();
			let mut incoming = [0u8; 8];
			let peer = i32::try_from(peer).map_err(|_| crate::Error::Overflow)?;
			let received = lane
				.send_receive_bytes(&count, peer, 3020, &mut incoming)
				.context("exchanging matching batch counts")?;
			if received != incoming.len() {
				return Err(crate::Error::Value("matching batch count payload"));
			}
			let count = usize::try_from(u64::from_le_bytes(incoming))
				.map_err(|_| crate::Error::Overflow)?;
			if count > limit.saturating_sub(self.received.len()) {
				return Err(crate::Error::Value("matching receive batch exceeds bound"));
			}
			let bytes = count
				.checked_mul(PACKET_BYTES)
				.ok_or(crate::Error::Overflow)?;
			self.receive_bytes.resize(bytes, 0);
			self.exchange_frames(lane, peer)?;
			for bytes in self.receive_bytes.as_chunks::<PACKET_BYTES>().0 {
				self.received.push(Packet {
					destination: route.rank,
					key: usize::try_from(get(bytes, 0)?).map_err(|_| crate::Error::Overflow)?,
					index: usize::try_from(get(bytes, 1)?).map_err(|_| crate::Error::Overflow)?,
					flag: usize::try_from(get(bytes, 2)?).map_err(|_| crate::Error::Overflow)?,
					value: Complex64::new(
						f64::from_bits(get(bytes, 3)?),
						f64::from_bits(get(bytes, 4)?),
					),
				});
			}
			statistics.coordination_calls = statistics.coordination_calls.saturating_add(2);
			statistics.point_to_point_sent_bytes = statistics
				.point_to_point_sent_bytes
				.saturating_add(size_of::<u64>())
				.saturating_add(self.send_bytes.len());
			statistics.point_to_point_received_bytes = statistics
				.point_to_point_received_bytes
				.saturating_add(incoming.len())
				.saturating_add(bytes);
		}
		statistics.maximum_routed_amplitudes = statistics
			.maximum_routed_amplitudes
			.max(self.received.len())
			.max(self.input.len());
		Ok(())
	}
	fn exchange_frames(
		&mut self,
		lane: &mut MpiCollectiveLane<'_>,
		peer: i32,
	) -> crate::Result<()> {
		// The immutable workspace bound currently makes this one frame. Keep
		// native-count chunking explicit if the admitted batch bound grows.
		for frame in crate::native_admission::CountChunks::new(
			self.send_bytes.len().max(self.receive_bytes.len()),
			1,
		)? {
			let send = self
				.send_bytes
				.get(frame.start.min(self.send_bytes.len())..frame.end.min(self.send_bytes.len()))
				.ok_or(crate::Error::Overflow)?;
			let receive_len = self.receive_bytes.len();
			let receive = self
				.receive_bytes
				.get_mut(frame.start.min(receive_len)..frame.end.min(receive_len))
				.ok_or(crate::Error::Overflow)?;
			let expected = receive.len();
			let received = lane
				.send_receive_bytes(send, peer, 3021, receive)
				.context("exchanging matching amplitude batch")?;
			if received != expected {
				return Err(crate::Error::Value("matching batch amplitude payload"));
			}
		}
		Ok(())
	}

	fn read_indices(
		&mut self,
		state: &RoutingState<'_, '_, '_>,
		route: Route,
	) -> crate::Result<()> {
		self.indices.clear();
		self.values.clear();
		for packet in &self.received {
			if packet.index.checked_div(route.local) != Some(route.rank) {
				return Err(crate::Error::Value("matching batched read owner"));
			}
			self.indices.push(
				i64::try_from(
					packet
						.index
						.checked_rem(route.local)
						.ok_or(crate::Error::Overflow)?,
				)
				.map_err(|_| crate::Error::Overflow)?,
			);
			self.values
				.push(quest_sys::QuestComplex { re: 0.0, im: 0.0 });
		}
		if !self.indices.is_empty() {
			state.read_indexed(&self.indices, &mut self.values)?;
		}
		Ok(())
	}
	fn write_indices(
		&mut self,
		state: &mut RoutingState<'_, '_, '_>,
		route: Route,
	) -> crate::Result<()> {
		self.indices.clear();
		self.values.clear();
		for packet in &self.received {
			if packet.index.checked_div(route.local) != Some(route.rank) {
				return Err(crate::Error::Value("matching batched write owner"));
			}
			self.indices.push(
				i64::try_from(
					packet
						.index
						.checked_rem(route.local)
						.ok_or(crate::Error::Overflow)?,
				)
				.map_err(|_| crate::Error::Overflow)?,
			);
			self.values.push(quest_sys::QuestComplex {
				re: packet.value.re,
				im: packet.value.im,
			});
		}
		if !self.indices.is_empty() {
			state.write_indexed(&self.indices, &self.values)?;
		}
		Ok(())
	}
}

pub(super) struct BatchExecution<'a, 'input, 'output> {
	pub(super) layout: &'a MatchingLayout,
	pub(super) shard: &'a MatchingShard,
	pub(super) state: RoutingState<'a, 'input, 'output>,
	pub(super) adjoint: bool,
	pub(super) outer_mask: usize,
	pub(super) outer_value: usize,
}
impl RoutingWorkspace {
	pub(super) fn apply(
		&mut self,
		lane: &mut MpiCollectiveLane<'_>,
		execution: &mut BatchExecution<'_, '_, '_>,
	) -> crate::Result<RoutingStatistics> {
		let mut statistics = RoutingStatistics::default();
		let flag = execution.layout.flag()?;
		let route = Route {
			rank: execution.shard.rank(),
			parts: execution.shard.parts(),
			local: execution.state.local_amplitudes(),
		};
		// Only the active native state owner traverses its partition. Other ranks
		// run bounded transport rounds without reconstructing another owner's bases.
		for owner in 0..route.parts {
			let count = local_pairs(route, owner, flag)?;
			for start in (0..count).step_by(PAIR_LIMIT) {
				self.bases.clear();
				if owner == route.rank {
					let end = start.saturating_add(PAIR_LIMIT).min(count);
					for index in start..end {
						let basis = local_pair_basis(route, owner, flag, index)?;
						statistics.local_pair_candidates =
							statistics.local_pair_candidates.saturating_add(1);
						if basis & execution.outer_mask == execution.outer_value {
							self.bases.push(basis);
						}
					}
				}
				if lane
					.all_agree(self.bases.is_empty())
					.context("checking matching owner batch")?
				{
					continue;
				}
				self.coefficient_requests(execution, route)?;
				self.exchange(lane, route, &mut statistics)?;
				// Retain only this coefficient owner's keys for the bounded round.
				if self.received.len() > PAIR_LIMIT {
					return Err(crate::Error::Value(
						"matching coefficient key batch exceeds bound",
					));
				}
				self.bases.clear();
				self.bases
					.extend(self.received.iter().map(|packet| packet.key));
				self.bases.sort_unstable();
				self.apply_batch(lane, execution, &mut statistics)?;
			}
		}
		Ok(statistics)
	}
	fn coefficient_requests(
		&mut self,
		execution: &BatchExecution<'_, '_, '_>,
		route: Route,
	) -> crate::Result<()> {
		self.input.clear();
		for &basis in &self.bases {
			let source = execution
				.layout
				.extract(basis, execution.layout.system_range())?;
			self.input.push(Packet {
				destination: source
					.checked_rem(route.parts)
					.ok_or(crate::Error::Overflow)?,
				key: basis,
				index: basis,
				flag: 0,
				value: Complex64::new(0.0, 0.0),
			});
		}
		Ok(())
	}

	fn requests(
		&mut self,
		execution: &BatchExecution<'_, '_, '_>,
		route: Route,
	) -> crate::Result<()> {
		let flag = execution.layout.flag()?;
		self.input.clear();
		for &basis in &self.bases {
			let source = execution
				.layout
				.extract(basis, execution.layout.system_range())?;
			if source.checked_rem(route.parts) != Some(route.rank) {
				continue;
			}
			let color = execution
				.layout
				.extract(basis, execution.layout.color_range())?;
			let column = execution
				.shard
				.column(color, source)
				.map_err(|_| crate::Error::Value("matching batch coefficient owner"))?;
			let input = if execution.adjoint {
				execution.layout.replace_system(basis, column.destination)?
			} else {
				basis
			};
			for (flag_index, index) in [(0, input), (1, input | flag)] {
				self.input.push(Packet {
					destination: index
						.checked_div(route.local)
						.ok_or(crate::Error::Overflow)?,
					key: basis,
					index,
					flag: flag_index,
					value: Complex64::new(0.0, 0.0),
				});
			}
		}
		Ok(())
	}
	fn responses(
		&mut self,
		execution: &BatchExecution<'_, '_, '_>,
		route: Route,
	) -> crate::Result<()> {
		self.input.clear();
		for (packet, value) in self.received.iter().zip(&self.values) {
			let source = execution
				.layout
				.extract(packet.key, execution.layout.system_range())?;
			self.input.push(Packet {
				destination: source
					.checked_rem(route.parts)
					.ok_or(crate::Error::Overflow)?,
				value: Complex64::new(value.re, value.im),
				..*packet
			});
		}
		Ok(())
	}
	fn pairs(&mut self, execution: &BatchExecution<'_, '_, '_>, route: Route) -> crate::Result<()> {
		self.pairs.clear();
		for &basis in &self.bases {
			let source = execution
				.layout
				.extract(basis, execution.layout.system_range())?;
			if source.checked_rem(route.parts) == Some(route.rank) {
				self.pairs.push(Pair {
					key: basis,
					values: [Complex64::new(0.0, 0.0); 2],
					seen: 0,
				});
			}
		}
		for packet in &self.received {
			let pair = self
				.pairs
				.binary_search_by_key(&packet.key, |pair| pair.key)
				.ok()
				.and_then(|index| self.pairs.get_mut(index))
				.ok_or(crate::Error::Value("matching response key"))?;
			let flag_bit = 1u8
				.checked_shl(u32::try_from(packet.flag).map_err(|_| crate::Error::Overflow)?)
				.ok_or(crate::Error::Overflow)?;
			if packet.flag > 1 || pair.seen & flag_bit != 0 {
				return Err(crate::Error::Value("duplicate matching flag response"));
			}
			*pair
				.values
				.get_mut(packet.flag)
				.ok_or(crate::Error::Value("matching response flag"))? = packet.value;
			pair.seen |= flag_bit;
		}
		Ok(())
	}
	fn outputs(
		&mut self,
		execution: &BatchExecution<'_, '_, '_>,
		route: Route,
	) -> crate::Result<()> {
		let flag = execution.layout.flag()?;
		self.input.clear();
		for pair in &self.pairs {
			if pair.seen != 3 {
				return Err(crate::Error::Value("missing matching flag response"));
			}
			let source = execution
				.layout
				.extract(pair.key, execution.layout.system_range())?;
			let color = execution
				.layout
				.extract(pair.key, execution.layout.color_range())?;
			let column = execution
				.shard
				.column(color, source)
				.map_err(|_| crate::Error::Value("matching batch coefficient"))?;
			let output = if execution.adjoint {
				pair.key
			} else {
				execution
					.layout
					.replace_system(pair.key, column.destination)?
			};
			let result = column.rotate(pair.values, execution.adjoint);
			for (flag_index, index) in [(0, output), (1, output | flag)] {
				self.input.push(Packet {
					destination: index
						.checked_div(route.local)
						.ok_or(crate::Error::Overflow)?,
					key: pair.key,
					index,
					flag: flag_index,
					value: *result
						.get(flag_index)
						.ok_or(crate::Error::Value("matching output flag"))?,
				});
			}
		}
		Ok(())
	}
	fn apply_batch(
		&mut self,
		lane: &mut MpiCollectiveLane<'_>,
		execution: &mut BatchExecution<'_, '_, '_>,
		statistics: &mut RoutingStatistics,
	) -> crate::Result<()> {
		let route = Route {
			rank: execution.shard.rank(),
			parts: execution.shard.parts(),
			local: execution.state.local_amplitudes(),
		};
		self.requests(execution, route)?;
		self.exchange(lane, route, statistics)?;
		self.read_indices(&execution.state, route)?;
		statistics.indexed_reads = statistics
			.indexed_reads
			.saturating_add(usize::from(!self.indices.is_empty()));
		self.responses(execution, route)?;
		self.exchange(lane, route, statistics)?;
		self.pairs(execution, route)?;
		self.outputs(execution, route)?;
		self.exchange(lane, route, statistics)?;
		self.write_indices(&mut execution.state, route)?;
		statistics.indexed_writes = statistics
			.indexed_writes
			.saturating_add(usize::from(!self.indices.is_empty()));
		statistics.maximum_batch_pairs = statistics.maximum_batch_pairs.max(self.bases.len());
		statistics.batches = statistics.batches.saturating_add(1);
		Ok(())
	}
}

#[cfg(test)]
mod partition_tests {
	use super::*;
	#[test]
	fn local_pair_iteration_is_disjoint_complete_with_high_and_low_flags() {
		for parts in [1usize, 2, 4, 8] {
			for flag in [1usize, 4, 32, 64] {
				let local = 128 / parts;
				let mut all = Vec::new();
				for rank in 0..parts {
					let route = Route { rank, parts, local };
					let pairs = local_pairs(route, rank, flag).unwrap();
					let indices: Vec<_> = (0..pairs)
						.map(|index| local_pair_basis(route, rank, flag, index).unwrap())
						.collect();
					assert!(indices.iter().all(|&basis| basis / local == rank));
					assert!(indices.windows(2).all(|pair| pair[0] < pair[1]));
					all.extend(indices);
				}
				assert_eq!(
					all,
					(0..128)
						.filter(|basis| basis & flag == 0)
						.collect::<Vec<_>>()
				);
			}
		}
	}
}
