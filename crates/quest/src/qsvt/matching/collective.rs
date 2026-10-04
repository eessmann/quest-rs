//! Whole matching unitaries using owned coefficient shards and local native states.
pub use super::batched::RoutingStatistics;
use super::batched::{BatchExecution, RoutingWorkspace};
use super::{MatchingLayout, read_local, reserve_descriptor, write_local};
use crate::qsvt::Result;
use crate::{
	Complex64, QubitCount, Register, StateVector,
	collective::{CollectiveEnvironment, CollectiveRegister, equal},
	environment::Reservation,
	error::BackendResult,
};
use quest_qsvt::{MatchingHeader, MatchingShard};
use quest_sys::mpi::MpiCollectiveLane;

fn agree<T>(lane: &mut MpiCollectiveLane<'_>, value: crate::Result<T>) -> crate::Result<T> {
	if !lane
		.all_agree(value.is_ok())
		.context("agreeing matching admission")?
	{
		return Err(crate::Error::Value(
			"collective matching admission rejected",
		));
	}
	value
}
fn fatal<T>(operation: impl FnOnce() -> crate::Result<T>) -> T {
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
}
fn transfer(
	lane: &mut MpiCollectiveLane<'_>,
	rank: usize,
	sender: usize,
	receiver: usize,
	packet: &mut [u8],
) -> crate::Result<()> {
	if sender == receiver {
		return Ok(());
	}
	if rank == sender {
		lane.send_bytes(
			packet,
			i32::try_from(receiver).map_err(|_| crate::Error::Overflow)?,
			3010,
		)
		.context("routing matching packet")?;
	} else if rank == receiver {
		let payload_length = lane
			.receive_bytes(
				packet,
				i32::try_from(sender).map_err(|_| crate::Error::Overflow)?,
				3010,
			)
			.context("receiving matching packet")?;
		if payload_length != packet.len() {
			return Err(crate::Error::Value("matching packet length"));
		}
	}
	Ok(())
}
pub(super) fn put(packet: &mut [u8], index: usize, value: u64) -> crate::Result<()> {
	let start = index.checked_mul(8).ok_or(crate::Error::Overflow)?;
	packet
		.get_mut(start..start.checked_add(8).ok_or(crate::Error::Overflow)?)
		.ok_or(crate::Error::Value("matching packet field"))?
		.copy_from_slice(&value.to_le_bytes());
	Ok(())
}
pub(super) fn get(packet: &[u8], index: usize) -> crate::Result<u64> {
	let start = index.checked_mul(8).ok_or(crate::Error::Overflow)?;
	let field = packet
		.get(start..start.checked_add(8).ok_or(crate::Error::Overflow)?)
		.ok_or(crate::Error::Value("matching packet field"))?;
	Ok(u64::from_le_bytes(field.try_into().map_err(|_| {
		crate::Error::Value("matching packet field")
	})?))
}
fn manifest(
	header: MatchingHeader,
	layout: &MatchingLayout,
	parts: usize,
) -> crate::Result<Vec<u8>> {
	let mut payload = Vec::new();
	for value in [
		header.rows,
		header.cols,
		header.system_qubits,
		header.color_qubits,
		header.num_colors,
		header.record_count,
		layout.count.get(),
		parts,
	] {
		payload.extend_from_slice(
			&u64::try_from(value)
				.map_err(|_| crate::Error::Overflow)?
				.to_le_bytes(),
		);
	}
	for value in [
		header.beta.to_bits(),
		header.alpha.to_bits(),
		header.source_identity,
		header.record_digest,
	] {
		payload.extend_from_slice(&value.to_le_bytes());
	}
	for &target in &layout.targets {
		payload.extend_from_slice(
			&u64::try_from(target)
				.map_err(|_| crate::Error::Overflow)?
				.to_le_bytes(),
		);
	}
	Ok(payload)
}
fn admit_payload(lane: &mut MpiCollectiveLane<'_>, shard: &MatchingShard) -> crate::Result<()> {
	let own = agree(
		lane,
		shard.payload_summary().map_err(|_| crate::Error::Overflow),
	)?;
	let mut count = 0usize;
	let mut digest = 0u64;
	for peer in 0..shard.parts() {
		let mut packet = [0u8; 16];
		put(
			&mut packet,
			0,
			u64::try_from(own.0).map_err(|_| crate::Error::Overflow)?,
		)?;
		put(&mut packet, 1, own.1)?;
		lane.broadcast_bytes(
			i32::try_from(peer).map_err(|_| crate::Error::Overflow)?,
			&mut packet,
		)
		.context("reducing matching payload summary")?;
		count = count
			.checked_add(usize::try_from(get(&packet, 0)?).map_err(|_| crate::Error::Overflow)?)
			.ok_or(crate::Error::Overflow)?;
		digest = digest.wrapping_add(get(&packet, 1)?);
	}
	agree(
		lane,
		if count == shard.header().record_count && digest == shard.header().record_digest {
			Ok(())
		} else {
			Err(crate::Error::Value(
				"matching payload differs from manifest",
			))
		},
	)
}
// Only counts are broadcast. Completion columns travel directly to their
// destination owner, which checks uniqueness and exact closure against its keys.
fn admit_permutation(lane: &mut MpiCollectiveLane<'_>, shard: &MatchingShard) -> crate::Result<()> {
	let rank = shard.rank();
	let parts = shard.parts();
	let mut incoming = agree(
		lane,
		crate::values::reserve_vec::<(usize, usize)>(shard.records().len()),
	)?;
	let mut valid = true;
	for sender in 0..parts {
		let mut count = u64::try_from(if sender == rank {
			shard.records().len()
		} else {
			0
		})
		.map_err(|_| crate::Error::Overflow)?
		.to_le_bytes();
		lane.broadcast_bytes(
			i32::try_from(sender).map_err(|_| crate::Error::Overflow)?,
			&mut count,
		)
		.context("sharing matching shard length")?;
		let count =
			usize::try_from(u64::from_le_bytes(count)).map_err(|_| crate::Error::Overflow)?;
		for index in 0..count {
			for receiver in 0..parts {
				let mut packet = [0u8; 24];
				if rank == sender {
					let record = shard
						.records()
						.get(index)
						.ok_or(crate::Error::Value("matching shard record index"))?;
					if record.destination.checked_rem(parts) == Some(receiver) {
						put(&mut packet, 0, 1)?;
						put(
							&mut packet,
							1,
							u64::try_from(record.color).map_err(|_| crate::Error::Overflow)?,
						)?;
						put(
							&mut packet,
							2,
							u64::try_from(record.destination)
								.map_err(|_| crate::Error::Overflow)?,
						)?;
					}
				}
				transfer(lane, rank, sender, receiver, &mut packet)?;
				if rank == receiver && get(&packet, 0)? == 1 {
					let key = (
						usize::try_from(get(&packet, 1)?).map_err(|_| crate::Error::Overflow)?,
						usize::try_from(get(&packet, 2)?).map_err(|_| crate::Error::Overflow)?,
					);
					if incoming.len() >= shard.records().len() {
						valid = false;
					} else {
						incoming.push(key);
					}
					if shard
						.records()
						.binary_search_by_key(&key, |record| (record.color, record.source))
						.is_err()
					{
						valid = false;
					}
				}
			}
		}
	}
	incoming.sort_unstable();
	if incoming.windows(2).any(|pair| matches!(pair,[a,b] if a==b)) {
		valid = false;
	}
	if incoming.len() != shard.records().len() {
		valid = false;
	}
	agree(
		lane,
		if valid {
			Ok(())
		} else {
			Err(crate::Error::Value(
				"matching completion is not a closed bijection",
			))
		},
	)
}

/// Prepared CPU/MPI unitary. Only this rank's immutable coefficient partition,
/// scalar manifest, mapped targets and one local native scratch state are owned.
pub struct PreparedMatching<'env, 'comm, 'runtime> {
	environment: &'env CollectiveEnvironment<'comm, 'runtime>,
	scratch: CollectiveRegister<'env, 'comm, 'runtime>,
	shard: MatchingShard,
	layout: MatchingLayout,
	_reservation: Reservation<'env>,
	id: u64,
	routing: RoutingWorkspace,
	_routing_reservation: Reservation<'env>,
	statistics: RoutingStatistics,
}
impl<'comm, 'runtime> CollectiveEnvironment<'comm, 'runtime> {
	/// Admit scalar identity, local shard ownership and global permutation closure
	/// before allocating a local-accounted scratch partition. No circuit is retained.
	/// # Errors
	/// Rejects inconsistent manifests/layouts, malformed shards, GPU execution or budgets collectively.
	pub fn prepare_matching(
		&self,
		shard: MatchingShard,
		count: QubitCount,
		targets: Vec<usize>,
	) -> Result<PreparedMatching<'_, 'comm, 'runtime>> {
		let id = self.identifier();
		let mut lane = self.begin(43, id, 0, 0)?;
		let layout = agree(
			&mut lane,
			(|| {
				if self.resources.capabilities().gpu
					|| shard.rank()
						!= usize::try_from(self.rank()?).map_err(|_| crate::Error::Overflow)?
					|| shard.parts()
						!= usize::try_from(self.size()?).map_err(|_| crate::Error::Overflow)?
				{
					return Err(crate::Error::Value(
						"matching shard ownership or CPU deployment",
					));
				}
				MatchingLayout::new(shard.header(), count, targets)
			})(),
		)?;
		let payload = agree(&mut lane, manifest(shard.header(), &layout, shard.parts()))?;
		equal(&mut lane, &payload)?;
		let reservation = agree(
			&mut lane,
			reserve_descriptor(&self.resources, &shard, &layout),
		)?;
		let validation = agree(
			&mut lane,
			self.resources.reserve(
				shard
					.records()
					.len()
					.checked_mul(size_of::<(usize, usize)>())
					.ok_or(crate::Error::Overflow)?,
			),
		)?;
		admit_payload(&mut lane, &shard)?;
		admit_permutation(&mut lane, &shard)?;
		drop(validation);
		let routing_reservation = agree(
			&mut lane,
			self.resources.reserve(RoutingWorkspace::bytes()?),
		)?;
		let routing = agree(&mut lane, RoutingWorkspace::new())?;
		drop(lane);
		let scratch = self.state_vector_local(count)?;
		Ok(PreparedMatching {
			environment: self,
			scratch,
			shard,
			layout,
			_reservation: reservation,
			id,
			routing,
			_routing_reservation: routing_reservation,
			statistics: RoutingStatistics::default(),
		})
	}
}

// The processor sends an index only to its actual state owner. Other ranks
// receive an empty request. No rank learns the other owners' coefficient data.
fn remote_read(
	lane: &mut MpiCollectiveLane<'_>,
	rank: usize,
	parts: usize,
	processor: usize,
	local: usize,
	register: &Register<'_, StateVector>,
	index: Option<usize>,
) -> crate::Result<Complex64> {
	let mut result = Complex64::new(0.0, 0.0);
	for peer in 0..parts {
		let mut request = [0xffu8; 8];
		if rank == processor {
			let index = index.ok_or(crate::Error::Value("missing matching read index"))?;
			if index.checked_div(local) == Some(peer) {
				request = u64::try_from(index)
					.map_err(|_| crate::Error::Overflow)?
					.to_le_bytes();
			}
		}
		transfer(lane, rank, processor, peer, &mut request)?;
		let mut response = [0u8; 16];
		let active = (rank == processor || rank == peer) && u64::from_le_bytes(request) != u64::MAX;
		if rank == peer && active {
			let position =
				usize::try_from(u64::from_le_bytes(request)).map_err(|_| crate::Error::Overflow)?;
			if position.checked_div(local) != Some(peer) {
				return Err(crate::Error::Value("matching read outside owner"));
			}
			let value = read_local(
				register,
				position.checked_rem(local).ok_or(crate::Error::Overflow)?,
			)?;
			put(&mut response, 0, value.re.to_bits())?;
			put(&mut response, 1, value.im.to_bits())?;
		}
		if active {
			transfer(lane, rank, peer, processor, &mut response)?;
		}
		if rank == processor && active {
			result = Complex64::new(
				f64::from_bits(get(&response, 0)?),
				f64::from_bits(get(&response, 1)?),
			);
		}
	}
	Ok(result)
}
fn remote_write(
	lane: &mut MpiCollectiveLane<'_>,
	rank: usize,
	parts: usize,
	processor: usize,
	local: usize,
	register: &mut Register<'_, StateVector>,
	output: Option<(usize, Complex64)>,
) -> crate::Result<()> {
	for peer in 0..parts {
		let mut packet = [0u8; 24];
		put(&mut packet, 0, u64::MAX)?;
		if rank == processor {
			let (index, value) =
				output.ok_or(crate::Error::Value("missing matching write value"))?;
			if index.checked_div(local) == Some(peer) {
				put(
					&mut packet,
					0,
					u64::try_from(index).map_err(|_| crate::Error::Overflow)?,
				)?;
				put(&mut packet, 1, value.re.to_bits())?;
				put(&mut packet, 2, value.im.to_bits())?;
			}
		}
		transfer(lane, rank, processor, peer, &mut packet)?;
		if rank == peer && get(&packet, 0)? != u64::MAX {
			let index = usize::try_from(get(&packet, 0)?).map_err(|_| crate::Error::Overflow)?;
			if index.checked_div(local) != Some(peer) {
				return Err(crate::Error::Value("matching write outside owner"));
			}
			let value = Complex64::new(
				f64::from_bits(get(&packet, 1)?),
				f64::from_bits(get(&packet, 2)?),
			);
			write_local(
				register,
				index.checked_rem(local).ok_or(crate::Error::Overflow)?,
				value,
			)?;
		}
	}
	Ok(())
}
impl<'env, 'comm, 'runtime> PreparedMatching<'env, 'comm, 'runtime> {
	/// Collective execution context used by this prepared unitary.
	#[must_use]
	pub const fn environment(&self) -> &'env CollectiveEnvironment<'comm, 'runtime> {
		self.environment
	}

	#[must_use]
	pub fn targets(&self) -> &[usize] {
		&self.layout.targets
	}
	#[must_use]
	pub const fn num_qubits(&self) -> QubitCount {
		self.layout.count
	}
	#[must_use]
	pub const fn shard(&self) -> &MatchingShard {
		&self.shard
	}
	/// Apply the identical whole matching unitary or adjoint with outer controls.
	/// Pair requests and replies have fixed bounded storage; coefficient records
	/// remain on their original-source owner, even when state partitions differ.
	/// # Errors
	/// Rejects mismatched register/width/control/adjoint calls collectively before mutation.
	pub fn apply_scalar(
		&mut self,
		register: &mut CollectiveRegister<'_, '_, '_>,
		adjoint: bool,
		outer_mask: usize,
		outer_value: usize,
	) -> Result<()> {
		let mut lane = self
			.environment
			.begin(44, self.id, register.id, u64::from(adjoint))?;
		let (positions, outcomes) = agree(
			&mut lane,
			(|| {
				if !std::ptr::eq(register.environment, self.environment)
					|| register.num_qubits() != self.layout.count
					|| register.deployment().nodes() != self.shard.parts()
				{
					return Err(crate::Error::Value("matching register owner or deployment"));
				}
				self.layout.controls(outer_mask, outer_value)
			})(),
		)?;
		let mut controls = [0u8; 16];
		put(
			&mut controls,
			0,
			u64::try_from(outer_mask).map_err(|_| crate::Error::Overflow)?,
		)?;
		put(
			&mut controls,
			1,
			u64::try_from(outer_value).map_err(|_| crate::Error::Overflow)?,
		)?;
		equal(&mut lane, &controls)?;
		fatal(|| {
			self.apply_native(
				register,
				adjoint,
				(outer_mask, outer_value),
				&positions,
				&outcomes,
				&mut lane,
			)
		});
		Ok(())
	}
	#[must_use]
	pub const fn last_statistics(&self) -> RoutingStatistics {
		self.statistics
	}
	/// Apply the whole unitary using bounded global batches and indexed native
	/// transfers. All packet/state/coefficient storage remains local or bounded.
	/// # Errors
	/// Rejects inconsistent calls and invalid controls collectively before mutation.
	pub fn apply(
		&mut self,
		register: &mut CollectiveRegister<'_, '_, '_>,
		adjoint: bool,
		outer_mask: usize,
		outer_value: usize,
	) -> Result<()> {
		let mut lane = self
			.environment
			.begin(45, self.id, register.id, u64::from(adjoint))?;
		let (positions, outcomes) = agree(
			&mut lane,
			(|| {
				if !std::ptr::eq(register.environment, self.environment)
					|| register.num_qubits() != self.layout.count
					|| register.deployment().nodes() != self.shard.parts()
				{
					return Err(crate::Error::Value("matching register owner or deployment"));
				}
				self.layout.controls(outer_mask, outer_value)
			})(),
		)?;
		let mut controls = [0u8; 16];
		put(
			&mut controls,
			0,
			u64::try_from(outer_mask).map_err(|_| crate::Error::Overflow)?,
		)?;
		put(
			&mut controls,
			1,
			u64::try_from(outer_value).map_err(|_| crate::Error::Overflow)?,
		)?;
		equal(&mut lane, &controls)?;
		self.statistics = fatal(|| {
			self.layout
				.hadamards(&mut register.inner, &positions, &outcomes)?;
			quest_sys::set_qureg_to_clone(self.scratch.inner.pin(), &register.inner.native)
				.context("staging batched matching permutation")?;
			let mut execution = BatchExecution {
				layout: &self.layout,
				shard: &self.shard,
				input: &register.inner,
				output: &mut self.scratch.inner,
				adjoint,
				outer_mask,
				outer_value,
			};
			let statistics = self.routing.apply(&mut lane, &mut execution)?;
			quest_sys::set_qureg_to_clone(register.inner.pin(), &self.scratch.inner.native)
				.context("installing batched matching permutation")?;
			self.layout
				.hadamards(&mut register.inner, &positions, &outcomes)?;
			Ok(statistics)
		});
		Ok(())
	}

	fn apply_native(
		&mut self,
		register: &mut CollectiveRegister<'_, '_, '_>,
		adjoint: bool,
		outer: (usize, usize),
		positions: &[i32],
		outcomes: &[i32],
		lane: &mut MpiCollectiveLane<'_>,
	) -> crate::Result<()> {
		let rank = self.shard.rank();
		let parts = self.shard.parts();
		let local = register.deployment().local_amplitudes();
		self.layout
			.hadamards(&mut register.inner, positions, outcomes)?;
		quest_sys::set_qureg_to_clone(self.scratch.inner.pin(), &register.inner.native)
			.context("staging distributed matching permutation")?;
		let flag = self.layout.flag()?;
		for basis in 0..self.layout.count.dimension() {
			if basis & flag != 0 || basis & outer.0 != outer.1 {
				continue;
			}
			let system = self.layout.extract(basis, self.layout.system_range())?;
			let color = self.layout.extract(basis, self.layout.color_range())?;
			let processor = system.checked_rem(parts).ok_or(crate::Error::Overflow)?;
			let column = if rank == processor {
				Some(
					self.shard
						.column(color, system)
						.map_err(|_| crate::Error::Value("matching local coefficient"))?,
				)
			} else {
				None
			};
			let routed = column
				.map(|column| self.layout.replace_system(basis, column.destination))
				.transpose()?;
			let input = if adjoint {
				routed
			} else {
				(rank == processor).then_some(basis)
			};
			let first = remote_read(lane, rank, parts, processor, local, &register.inner, input)?;
			let second = remote_read(
				lane,
				rank,
				parts,
				processor,
				local,
				&register.inner,
				input.map(|index| index | flag),
			)?;
			let result = column.map(|column| column.rotate([first, second], adjoint));
			let output = if adjoint {
				(rank == processor).then_some(basis)
			} else {
				routed
			};
			remote_write(
				lane,
				rank,
				parts,
				processor,
				local,
				&mut self.scratch.inner,
				output.zip(result.map(|pair| pair[0])),
			)?;
			remote_write(
				lane,
				rank,
				parts,
				processor,
				local,
				&mut self.scratch.inner,
				output
					.map(|index| index | flag)
					.zip(result.map(|pair| pair[1])),
			)?;
		}
		quest_sys::set_qureg_to_clone(register.inner.pin(), &self.scratch.inner.native)
			.context("installing distributed matching permutation")?;
		self.layout
			.hadamards(&mut register.inner, positions, outcomes)
	}
}

#[cfg(test)]
mod failure_tests;
