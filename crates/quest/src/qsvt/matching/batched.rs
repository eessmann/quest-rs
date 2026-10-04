//! Bounded ownership routing. Every global batch contains at most B flag pairs,
//! so all per-rank sends/receives contain at most 2B amplitude records even when
//! coefficient or permutation ownership is maximally imbalanced.
use super::{MatchingLayout, collective::get};
use crate::{Complex64, Register, StateVector, error::BackendResult, values::reserve_vec};
use quest_qsvt::MatchingShard;
use quest_sys::mpi::MpiCollectiveLane;

const PAIR_LIMIT: usize = 64;
const PACKET_BYTES: usize = 40;
#[derive(Debug, Clone, Copy, Default)]
pub struct RoutingStatistics {
	pub batches: usize,
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
			.ok_or(crate::Error::Overflow)
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
			i32::try_from(bytes).map_err(|_| crate::Error::Overflow)?;
			self.receive_bytes.resize(bytes, 0);
			let received = lane
				.send_receive_bytes(&self.send_bytes, peer, 3021, &mut self.receive_bytes)
				.context("exchanging matching amplitude batch")?;
			if received != bytes {
				return Err(crate::Error::Value("matching batch amplitude payload"));
			}
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
	fn read_indices(
		&mut self,
		register: &Register<'_, StateVector>,
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
			quest_sys::read_local_indexed_qureg_amps(
				&register.native,
				&self.indices,
				&mut self.values,
			)
			.context("reading matching indexed batch")?;
		}
		Ok(())
	}
	fn write_indices(
		&mut self,
		register: &mut Register<'_, StateVector>,
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
			quest_sys::write_local_indexed_qureg_amps(register.pin(), &self.indices, &self.values)
				.context("writing matching indexed batch")?;
		}
		Ok(())
	}
}

pub(super) struct BatchExecution<'a, 'input, 'output> {
	pub(super) layout: &'a MatchingLayout,
	pub(super) shard: &'a MatchingShard,
	pub(super) input: &'a Register<'input, StateVector>,
	pub(super) output: &'a mut Register<'output, StateVector>,
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
		self.bases.clear();
		for basis in 0..execution.layout.count.dimension() {
			if basis & flag != 0 || basis & execution.outer_mask != execution.outer_value {
				continue;
			}
			self.bases.push(basis);
			if self.bases.len() == PAIR_LIMIT {
				self.apply_batch(lane, execution, &mut statistics)?;
				self.bases.clear();
			}
		}
		if !self.bases.is_empty() {
			self.apply_batch(lane, execution, &mut statistics)?;
		}
		Ok(statistics)
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
			local: execution.input.deployment().local_amplitudes(),
		};
		self.requests(execution, route)?;
		self.exchange(lane, route, statistics)?;
		self.read_indices(execution.input, route)?;
		statistics.indexed_reads = statistics
			.indexed_reads
			.saturating_add(usize::from(!self.indices.is_empty()));
		self.responses(execution, route)?;
		self.exchange(lane, route, statistics)?;
		self.pairs(execution, route)?;
		self.outputs(execution, route)?;
		self.exchange(lane, route, statistics)?;
		self.write_indices(execution.output, route)?;
		statistics.indexed_writes = statistics
			.indexed_writes
			.saturating_add(usize::from(!self.indices.is_empty()));
		statistics.maximum_batch_pairs = statistics.maximum_batch_pairs.max(self.bases.len());
		statistics.batches = statistics.batches.saturating_add(1);
		Ok(())
	}
}
