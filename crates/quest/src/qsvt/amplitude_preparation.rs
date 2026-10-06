//! Coherent preparation from contiguous, genuinely local amplitude partitions.
//!
//! Only owned coefficient tables and bounded transport buffers persist. Scalar
//! coefficient delivery is a charged baseline, not a logarithmic gate-count claim.
use crate::{
	Complex64, Error, MemoryBudget, Result,
	collective::{CollectiveEnvironment, CollectiveRegister, equal},
	environment::{Reservation, RuntimeResources},
	error::BackendResult,
	values::reserve_vec,
};
use quest_qsvt::{
	ReplayGate,
	state_preparation::{map_preparation_gate, visit_preparation_gates},
};
use quest_sys::mpi::MpiCollectiveLane;
mod tree;
mod wire;
use wire::{agree, broadcast, fatal};

/// One disjoint contiguous partition of the declared padded input.
/// Values contain only logical amplitudes in this partition; padding is implicit.
pub struct AmplitudeShard {
	logical: usize,
	dimension: usize,
	rank: usize,
	parts: usize,
	start: usize,
	values: Vec<Complex64>,
}
impl AmplitudeShard {
	/// Create input directly from global indices owned by this rank.
	/// # Errors
	/// Rejects unsupported partitioning, local storage overflow, allocation or generator failure.
	pub fn from_fn(
		logical: usize,
		rank: usize,
		parts: usize,
		max_local_bytes: usize,
		mut value: impl FnMut(usize) -> Result<Complex64>,
	) -> Result<Self> {
		let dimension = logical.checked_next_power_of_two().ok_or(Error::Overflow)?;
		let (start, count) = partition(logical, dimension, rank, parts)?;
		if count
			.checked_mul(size_of::<Complex64>())
			.is_none_or(|n| n > max_local_bytes)
		{
			return Err(Error::Value("amplitude shard creation budget"));
		}
		let mut values = reserve_vec(count)?;
		for index in start..start.checked_add(count).ok_or(Error::Overflow)? {
			values.push(value(index)?);
		}
		Self::from_parts(logical, dimension, rank, parts, start, values)
	}
	/// Import already local values with explicit ownership. No complete vector is partitioned.
	/// # Errors
	/// Rejects malformed logical/padded dimensions, ownership or nonfinite input.
	pub fn from_parts(
		logical: usize,
		dimension: usize,
		rank: usize,
		parts: usize,
		start: usize,
		values: Vec<Complex64>,
	) -> Result<Self> {
		let expected = partition(logical, dimension, rank, parts)?;
		if expected != (start, values.len())
			|| values
				.iter()
				.any(|v| !v.re.is_finite() || !v.im.is_finite())
		{
			return Err(Error::Value("malformed amplitude shard"));
		}
		Ok(Self {
			logical,
			dimension,
			rank,
			parts,
			start,
			values,
		})
	}
	#[must_use]
	pub fn local_values(&self) -> &[Complex64] {
		&self.values
	}
	#[must_use]
	pub const fn start(&self) -> usize {
		self.start
	}
}
fn partition(
	logical: usize,
	dimension: usize,
	rank: usize,
	parts: usize,
) -> Result<(usize, usize)> {
	if logical == 0
		|| !parts.is_power_of_two()
		|| parts > dimension
		|| rank >= parts
		|| logical.checked_next_power_of_two() != Some(dimension)
	{
		return Err(Error::Value("unsupported amplitude partition"));
	}
	let extent = dimension.checked_div(parts).ok_or(Error::Overflow)?;
	let start = rank.checked_mul(extent).ok_or(Error::Overflow)?;
	Ok((start, logical.saturating_sub(start).min(extent)))
}
/// All limits bind construction and every later replay, including caller state memory.
#[derive(Clone, Copy, Debug)]
pub struct DistributedPreparationLimits {
	pub max_dimension: usize,
	pub max_compile_work: usize,
	pub max_gates: usize,
	pub max_query_work: usize,
	pub max_native_dispatches: usize,
	pub max_transport_bytes: usize,
	pub max_local_bytes: usize,
	pub chunk_elements: usize,
	pub ranks_per_node: usize,
	pub node_budget: MemoryBudget,
}
impl Default for DistributedPreparationLimits {
	fn default() -> Self {
		Self {
			max_dimension: 1_048_576,
			max_compile_work: 67_108_864,
			max_gates: 8_388_608,
			max_query_work: 67_108_864,
			max_native_dispatches: 67_108_864,
			max_transport_bytes: usize::MAX,
			max_local_bytes: 268_435_456,
			chunk_elements: 64,
			ranks_per_node: usize::MAX,
			node_budget: MemoryBudget::new(usize::MAX),
		}
	}
}
/// Application-byte and logical transport accounting, excluding MPI implementation internals.
#[derive(Clone, Copy, Debug)]
pub struct DistributedPreparationResources {
	pub padded_dimension: usize,
	pub local_coefficients: usize,
	pub elementary_gates: usize,
	pub compile_work: usize,
	pub coefficient_queries: usize,
	/// Logical index lookup and mapping work. Native apply also charges full-register control scans.
	pub query_work: usize,
	pub retained_bytes: usize,
	pub construction_peak_bytes: usize,
	/// Global directed-payload ceiling with a conservative coordination allowance.
	pub construction_transport_bytes: usize,
	/// Global directed-payload ceiling for one complete replay, including coordination.
	pub replay_transport_bytes: usize,
	pub max_packet_bytes: usize,
	/// Exact payload and frame counts for this rank's Walsh pair exchanges.
	pub butterfly_sent_bytes: usize,
	pub butterfly_received_bytes: usize,
	pub butterfly_messages: usize,
}
struct Level {
	y: Vec<f64>,
	z: Vec<f64>,
}
impl Level {
	fn owner_index(count: usize, parts: usize, index: usize) -> Result<(usize, usize)> {
		if index >= count {
			return Err(Error::Value("amplitude coefficient index"));
		}
		if count >= parts {
			let local = count.checked_div(parts).ok_or(Error::Overflow)?;
			Ok((
				index.checked_div(local).ok_or(Error::Overflow)?,
				index.checked_rem(local).ok_or(Error::Overflow)?,
			))
		} else {
			Ok((
				index
					.checked_mul(parts.checked_div(count).ok_or(Error::Overflow)?)
					.ok_or(Error::Overflow)?,
				0,
			))
		}
	}
	fn count(count: usize, rank: usize, parts: usize) -> Result<usize> {
		Ok(if count >= parts {
			count.checked_div(parts).ok_or(Error::Overflow)?
		} else {
			usize::from(
				rank.checked_rem(parts.checked_div(count).ok_or(Error::Overflow)?) == Some(0),
			)
		})
	}
}
/// Zero RHS has no normalized state; this outcome owns no compiled preparation.
#[allow(
	clippy::large_enum_variant,
	reason = "The fallibly admitted owner returns by value without an additional infallible heap allocation"
)]
pub enum PreparationOutcome<'env, 'comm, 'runtime> {
	Zero { source_identity: u64 },
	Prepared(PreparedAmplitudes<'env, 'comm, 'runtime>),
}
/// Environment-bound immutable coefficient owner, norm, identity and reusable bounded buffers.
pub struct PreparedAmplitudes<'env, 'comm, 'runtime> {
	environment: &'env CollectiveEnvironment<'comm, 'runtime>,
	id: u64,
	logical: usize,
	dimension: usize,
	rank: usize,
	parts: usize,
	levels: Vec<Level>,
	norm: f64,
	phase: f64,
	identity: [u64; 2],
	resources: DistributedPreparationResources,
	limits: DistributedPreparationLimits,
	fingerprint: quest_sys::NumericalFingerprint,
	send: Vec<u8>,
	receive: Vec<u8>,
	_reservation: Reservation<'env>,
}
impl<'comm, 'runtime> CollectiveEnvironment<'comm, 'runtime> {
	/// Generate only this rank's input range after collective input-storage admission.
	///
	/// Local allocation/generator failures agree before table construction. Generator
	/// work beyond its admitted invocation count is caller-owned.
	/// # Errors
	/// Rejects common metadata/capacity or any local generator/allocation failure.
	pub fn prepare_amplitudes_from_fn(
		&self,
		logical: usize,
		mut limits: DistributedPreparationLimits,
		value: impl FnMut(usize) -> Result<Complex64>,
	) -> Result<PreparationOutcome<'_, 'comm, 'runtime>> {
		let id = self.identifier();
		let mut lane = self.begin(83, id, 0, 0)?;
		let parts = usize::try_from(self.size()?).map_err(|_| Error::Overflow)?;
		let rank = usize::try_from(self.rank()?).map_err(|_| Error::Overflow)?;
		if limits.ranks_per_node == usize::MAX {
			limits.ranks_per_node = parts;
		}
		for word in [
			logical,
			limits.max_local_bytes,
			limits.ranks_per_node,
			limits.node_budget.bytes(),
		] {
			equal(
				&mut lane,
				&u64::try_from(word)
					.map_err(|_| Error::Overflow)?
					.to_le_bytes(),
			)?;
		}
		let input_bytes = agree(
			&mut lane,
			(|| {
				let dimension = logical.checked_next_power_of_two().ok_or(Error::Overflow)?;
				if self.resources.capabilities().gpu
					|| dimension > limits.max_dimension
					|| limits.ranks_per_node == 0
					|| limits.ranks_per_node > parts
				{
					return Err(Error::Value("amplitude input creation admission"));
				}
				let (_, count) = partition(logical, dimension, rank, parts)?;
				count
					.checked_mul(size_of::<Complex64>())
					.ok_or(Error::Overflow)
			})(),
		)?;
		capacity(
			&mut lane,
			&self.resources,
			input_bytes.checked_add(8192).ok_or(Error::Overflow)?,
			limits,
			parts,
		)?;
		let input_storage = agree(&mut lane, self.resources.reserve(input_bytes))?;
		let input = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			AmplitudeShard::from_fn(logical, rank, parts, input_bytes, value)
		}))
		.unwrap_or(Err(Error::Value("amplitude input generator panicked")));
		let shard = agree(&mut lane, input)?;
		drop(lane);
		// The next construction admission charges the consumed shard's actual capacity.
		drop(input_storage);
		self.prepare_amplitudes(shard, limits)
	}

	/// Compile local input shards into disjoint immutable tables and bounded replay resources.
	/// Imported shard creation failures must be agreed by the caller; use
	/// `prepare_amplitudes_from_fn` for collectively admitted input generation.
	/// # Errors
	/// Collectively rejects ownership/metadata, capacities, nonfinite input and allocation failures.
	#[allow(
		clippy::too_many_lines,
		reason = "Collective construction agreements remain beside each protocol phase"
	)]
	pub fn prepare_amplitudes(
		&self,
		shard: AmplitudeShard,
		mut limits: DistributedPreparationLimits,
	) -> Result<PreparationOutcome<'_, 'comm, 'runtime>> {
		let id = self.identifier();
		let mut lane = self.begin(80, id, 0, 0)?;
		agree(
			&mut lane,
			(|| {
				if self.resources.capabilities().gpu
					|| shard.rank != usize::try_from(self.rank()?).map_err(|_| Error::Overflow)?
					|| shard.parts != usize::try_from(self.size()?).map_err(|_| Error::Overflow)?
				{
					return Err(Error::Value("amplitude shard communicator/CPU ownership"));
				}
				partition(shard.logical, shard.dimension, shard.rank, shard.parts)?;
				Ok(())
			})(),
		)?;
		if limits.ranks_per_node == usize::MAX {
			limits.ranks_per_node = shard.parts;
		}
		for word in [
			shard.logical,
			shard.dimension,
			shard.parts,
			limits.max_dimension,
			limits.max_compile_work,
			limits.max_gates,
			limits.max_query_work,
			limits.max_native_dispatches,
			limits.max_transport_bytes,
			limits.max_local_bytes,
			limits.chunk_elements,
			limits.ranks_per_node,
			limits.node_budget.bytes(),
		] {
			equal(
				&mut lane,
				&u64::try_from(word)
					.map_err(|_| Error::Overflow)?
					.to_le_bytes(),
			)?;
		}
		let resources = agree(&mut lane, resource_plan(&shard, limits))?;
		capacity(
			&mut lane,
			&self.resources,
			resources.construction_peak_bytes,
			limits,
			shard.parts,
		)?;
		let reservation = agree(&mut lane, self.resources.reserve(resources.retained_bytes))?;
		let temporary = agree(
			&mut lane,
			self.resources.reserve(
				resources
					.construction_peak_bytes
					.checked_sub(resources.retained_bytes)
					.ok_or(Error::Overflow)?,
			),
		)?;
		let (mut levels, mut mass, mut phases, mut send, mut receive) = agree(
			&mut lane,
			(|| {
				let qubits =
					usize::try_from(shard.dimension.ilog2()).map_err(|_| Error::Overflow)?;
				let mut levels = reserve_vec(qubits)?;
				for depth in 0..qubits {
					let count = Level::count(1usize << depth, shard.rank, shard.parts)?;
					levels.push(Level {
						y: tree::zeroes(count)?,
						z: tree::zeroes(count)?,
					});
				}
				let extent = shard
					.dimension
					.checked_div(shard.parts)
					.ok_or(Error::Overflow)?;
				let mass = tree::zeroes(extent.checked_mul(2).ok_or(Error::Overflow)?)?;
				let phases = tree::zeroes(extent.checked_mul(2).ok_or(Error::Overflow)?)?;
				let mut send = reserve_vec(resources.max_packet_bytes)?;
				send.resize(resources.max_packet_bytes, 0);
				let mut receive = reserve_vec(resources.max_packet_bytes)?;
				receive.resize(resources.max_packet_bytes, 0);
				Ok((levels, mass, phases, send, receive))
			})(),
		)?;
		let fingerprint = agree(
			&mut lane,
			quest_sys::get_numerical_fingerprint().context("freezing amplitude environment"),
		)?;
		let (scale, source_identity) = tree::input_summary(&mut lane, &shard)?;
		if scale == 0. {
			return Ok(PreparationOutcome::Zero { source_identity });
		}
		let (norm, phase) = fatal(|| {
			tree::compile(
				&mut lane,
				&shard,
				scale,
				&mut levels,
				&mut mass,
				&mut phases,
				&mut send,
				&mut receive,
				limits.chunk_elements,
			)
		});
		agree(
			&mut lane,
			if norm.is_finite()
				&& norm > 0.
				&& phase.is_finite()
				&& levels
					.iter()
					.flat_map(|l| l.y.iter().chain(&l.z))
					.all(|v| v.is_finite())
			{
				Ok(())
			} else {
				Err(Error::Value("nonfinite preparation tree"))
			},
		)?;
		let table_identity = tree::table_identity(&mut lane, &levels, shard.rank, shard.parts)?;
		drop(mass);
		drop(phases);
		drop(shard.values);
		drop(temporary);
		Ok(PreparationOutcome::Prepared(PreparedAmplitudes {
			environment: self,
			id,
			logical: shard.logical,
			dimension: shard.dimension,
			rank: shard.rank,
			parts: shard.parts,
			levels,
			norm,
			phase,
			identity: [source_identity, table_identity],
			resources,
			limits,
			fingerprint,
			send,
			receive,
			_reservation: reservation,
		}))
	}
}
#[allow(
	clippy::too_many_lines,
	reason = "One checked formula admits simultaneous input, tree, tables, buffers and replay costs before allocation"
)]
fn resource_plan(
	shard: &AmplitudeShard,
	limits: DistributedPreparationLimits,
) -> Result<DistributedPreparationResources> {
	let dimension = shard.dimension;
	let qubits = usize::try_from(dimension.ilog2()).map_err(|_| Error::Overflow)?;
	if dimension > limits.max_dimension
		|| limits.chunk_elements == 0
		|| limits.ranks_per_node == 0
		|| limits.ranks_per_node > shard.parts
	{
		return Err(Error::Value("amplitude capacity declaration"));
	}
	let packet = limits
		.chunk_elements
		.checked_mul(16)
		.filter(|n| i32::try_from(*n).is_ok())
		.ok_or(Error::Overflow)?;
	let mut coefficients = 0usize;
	for depth in 0..qubits {
		coefficients = coefficients
			.checked_add(
				Level::count(1usize << depth, shard.rank, shard.parts)?
					.checked_mul(2)
					.ok_or(Error::Overflow)?,
			)
			.ok_or(Error::Overflow)?;
	}
	let retained = coefficients
		.checked_mul(8)
		.and_then(|n| n.checked_add(qubits.checked_mul(size_of::<Level>())?))
		.and_then(|n| n.checked_add(packet.checked_mul(2)?))
		.and_then(|n| n.checked_add(const { size_of::<PreparedAmplitudes<'_, '_, '_>>() + 256 }))
		.ok_or(Error::Overflow)?;
	let extent = dimension.checked_div(shard.parts).ok_or(Error::Overflow)?;
	let peak = extent
		.checked_mul(32)
		.and_then(|n| n.checked_add(shard.values.capacity().checked_mul(16)?))
		.and_then(|n| n.checked_add(retained))
		.and_then(|n| n.checked_add(8192))
		.ok_or(Error::Overflow)?;
	let work = qubits
		.checked_mul(2)
		.and_then(|n| n.checked_add(16))
		.and_then(|n| n.checked_mul(extent))
		.and_then(|n| n.checked_add(shard.parts.checked_mul(64)?))
		.and_then(|n| n.checked_add(qubits.checked_mul(8)?))
		.ok_or(Error::Overflow)?;
	let queries = dimension
		.checked_sub(1)
		.and_then(|n| n.checked_mul(2))
		.ok_or(Error::Overflow)?;

	let gates = if dimension == 1 {
		1
	} else {
		dimension
			.checked_mul(5)
			.and_then(|n| n.checked_sub(6))
			.ok_or(Error::Overflow)?
	};
	let query_work = queries
		.checked_mul(qubits.checked_add(1).ok_or(Error::Overflow)?)
		.and_then(|n| {
			gates
				.checked_mul(qubits.checked_mul(2)?.checked_add(1)?)
				.and_then(|g| n.checked_add(g))
		})
		.ok_or(Error::Overflow)?;
	let (butterfly_sent_bytes, butterfly_messages) =
		tree::butterfly_cost(dimension, shard.rank, shard.parts, limits.chunk_elements)?;
	let replay_transport = queries
		.checked_mul(8)
		.and_then(|n| n.checked_mul(shard.parts.saturating_sub(1)))
		.and_then(|n| n.checked_add(shard.parts.checked_mul(shard.parts)?.checked_mul(4096)?))
		.ok_or(Error::Overflow)?;
	let construction_transport = dimension
		.checked_mul(qubits)
		.and_then(|n| n.checked_mul(32))
		.and_then(|n| n.checked_add(shard.parts.checked_mul(shard.parts)?.checked_mul(4096)?))
		.ok_or(Error::Overflow)?;
	if peak > limits.max_local_bytes
		|| work > limits.max_compile_work
		|| gates > limits.max_gates
		|| query_work > limits.max_query_work
		|| construction_transport
			.checked_add(replay_transport)
			.is_none_or(|n| n > limits.max_transport_bytes)
	{
		return Err(Error::Value(
			"amplitude preparation work/storage/transport budget",
		));
	}
	Ok(DistributedPreparationResources {
		padded_dimension: dimension,
		local_coefficients: coefficients,
		elementary_gates: gates,
		compile_work: work,
		coefficient_queries: queries,
		query_work,
		retained_bytes: retained,
		construction_peak_bytes: peak,
		construction_transport_bytes: construction_transport,
		replay_transport_bytes: replay_transport,
		max_packet_bytes: packet,
		butterfly_sent_bytes,
		butterfly_received_bytes: butterfly_sent_bytes,
		butterfly_messages,
	})
}
fn capacity(
	lane: &mut MpiCollectiveLane<'_>,
	resources: &RuntimeResources,
	extra: usize,
	limits: DistributedPreparationLimits,
	parts: usize,
) -> Result<()> {
	let peak = agree(
		lane,
		resources
			.allocated_bytes()
			.checked_add(extra)
			.ok_or(Error::Overflow),
	)?;
	let mut maximum = 0usize;
	for peer in 0..parts {
		let mut packet = u64::try_from(peak)
			.map_err(|_| Error::Overflow)?
			.to_le_bytes();
		broadcast(lane, peer, &mut packet)?;
		maximum =
			maximum.max(usize::try_from(u64::from_le_bytes(packet)).map_err(|_| Error::Overflow)?);
	}
	agree(
		lane,
		if maximum > limits.max_local_bytes
			|| maximum
				.checked_mul(limits.ranks_per_node)
				.is_none_or(|n| n > limits.node_budget.bytes())
		{
			Err(Error::Value("amplitude live rank/node capacity"))
		} else {
			Ok(())
		},
	)
}
impl PreparedAmplitudes<'_, '_, '_> {
	#[must_use]
	pub const fn norm(&self) -> f64 {
		self.norm
	}
	#[must_use]
	/// Deterministic input/table fingerprints, not a cryptographic or numerical certificate.
	pub const fn source_identity(&self) -> [u64; 2] {
		self.identity
	}
	#[must_use]
	pub const fn resources(&self) -> DistributedPreparationResources {
		self.resources
	}
	fn admit(
		&self,
		lane: &mut MpiCollectiveLane<'_>,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
		width: Option<usize>,
	) -> Result<()> {
		agree(
			lane,
			(|| {
				if targets.len() != self.levels.len() || value & !mask != 0 {
					return Err(Error::Value("amplitude replay operands"));
				}
				let mut occupied = mask;
				for &target in targets {
					let bit = 1usize
						.checked_shl(u32::try_from(target).map_err(|_| Error::Overflow)?)
						.ok_or(Error::Overflow)?;
					if occupied & bit != 0 || width.is_some_and(|w| target >= w) {
						return Err(Error::Value("amplitude replay overlap/width"));
					}
					occupied |= bit;
				}
				if width.is_some_and(|w| {
					mask >= 1usize
						.checked_shl(u32::try_from(w).unwrap_or(u32::MAX))
						.unwrap_or(0)
				}) {
					return Err(Error::Value("amplitude replay control width"));
				}
				let fingerprint = quest_sys::get_numerical_fingerprint()
					.context("checking amplitude replay environment")?;
				if fingerprint != self.fingerprint {
					return Err(Error::Value("amplitude environment changed"));
				}
				Ok(())
			})(),
		)?;
		for word in [
			self.identity[0],
			self.identity[1],
			self.norm.to_bits(),
			self.phase.to_bits(),
		] {
			equal(lane, &word.to_le_bytes())?;
		}
		for word in [
			self.logical,
			self.dimension,
			mask,
			value,
			usize::from(adjoint),
			width.unwrap_or(usize::MAX),
		] {
			equal(
				lane,
				&u64::try_from(word)
					.map_err(|_| Error::Overflow)?
					.to_le_bytes(),
			)?;
		}
		for &target in targets {
			equal(
				lane,
				&u64::try_from(target)
					.map_err(|_| Error::Overflow)?
					.to_le_bytes(),
			)?;
		}
		capacity(
			lane,
			&self.environment.resources,
			8192,
			self.limits,
			self.parts,
		)
	}
	fn replay(
		&mut self,
		lane: &mut MpiCollectiveLane<'_>,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> quest_qsvt::Result<()>,
	) -> quest_qsvt::Result<()> {
		visit_preparation_gates(
			self.levels.len(),
			self.phase,
			adjoint,
			&mut |depth, is_z, index| {
				let (owner, offset) = Level::owner_index(1usize << depth, self.parts, index)
					.map_err(|_| quest_qsvt::Error::Encoding("amplitude coefficient ownership"))?;
				let packet = self
					.receive
					.get_mut(..8)
					.ok_or(quest_qsvt::Error::Encoding("amplitude receive buffer"))?;
				if self.rank == owner {
					let level = self
						.levels
						.get(depth)
						.ok_or(quest_qsvt::Error::Encoding("amplitude table level"))?;
					let word = (if is_z { &level.z } else { &level.y })
						.get(offset)
						.ok_or(quest_qsvt::Error::Encoding("amplitude table coefficient"))?
						.to_le_bytes();
					let send = self
						.send
						.get_mut(..8)
						.ok_or(quest_qsvt::Error::Encoding("amplitude send buffer"))?;
					send.copy_from_slice(&word);
					packet.copy_from_slice(send);
				} else {
					packet.fill(0);
				}
				broadcast(lane, owner, packet)
					.map_err(|_| quest_qsvt::Error::Encoding("amplitude coefficient transport"))?;
				Ok(f64::from_le_bytes(packet.try_into().map_err(|_| {
					quest_qsvt::Error::Encoding("amplitude scalar packet")
				})?))
			},
			&mut |gate| visitor(map_preparation_gate(gate, targets, mask, value)?),
		)
	}
	/// Collectively stream a bounded primitive replay from frozen distributed coefficients.
	/// # Errors
	/// Rejects metadata/layout/capacity before emission. Any subsequent visitor or transport failure aborts MPI.
	pub fn visit_mapped_gates(
		&mut self,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> quest_qsvt::Result<()>,
	) -> Result<()> {
		let mut lane = self.environment.begin(81, self.id, 0, 0)?;
		self.admit(&mut lane, targets, mask, value, adjoint, None)?;
		fatal(|| {
			self.replay(&mut lane, targets, mask, value, adjoint, visitor)
				.map_err(|_| Error::Value("amplitude visitor failed after replay entry"))
		});
		Ok(())
	}
	fn native_executor<'register>(
		&self,
		lane: &mut MpiCollectiveLane<'_>,
		register: &CollectiveRegister<'register, '_, '_>,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
	) -> Result<super::replay_native::ReplayGateExecutor<'register>> {
		agree(
			lane,
			if std::ptr::eq(self.environment, register.environment) {
				Ok(())
			} else {
				Err(Error::Value("amplitude register environment mismatch"))
			},
		)?;
		let executor = agree(
			lane,
			super::replay_native::ReplayGateExecutor::new(&register.inner),
		)?;
		self.admit(
			lane,
			targets,
			mask,
			value,
			adjoint,
			Some(register.num_qubits().get()),
		)?;
		let width = register.num_qubits().get();
		agree(
			lane,
			(|| {
				let dispatches = self
					.resources
					.elementary_gates
					.checked_mul(
						width
							.checked_mul(2)
							.and_then(|n| n.checked_add(1))
							.ok_or(Error::Overflow)?,
					)
					.ok_or(Error::Overflow)?;
				let work = self
					.resources
					.elementary_gates
					.checked_mul(width.checked_add(1).ok_or(Error::Overflow)?)
					.and_then(|n| n.checked_add(self.resources.query_work))
					.ok_or(Error::Overflow)?;
				if dispatches > self.limits.max_native_dispatches
					|| work > self.limits.max_query_work
				{
					return Err(Error::Value("amplitude native replay work budget"));
				}
				Ok(())
			})(),
		)?;
		agree(
			lane,
			crate::native_admission::admit_dense_partition(
				register.deployment().local_amplitudes(),
				1,
			),
		)?;
		Ok(executor)
	}
	/// Collectively check complete native replay admission without changing state or coefficients.
	///
	/// Checks the same owner, layout, limits, live capacity, native partition and
	/// ephemeral executor reservation as `apply`; releases scratch before returning.
	/// Actual `apply` repeats admission, so callers must not introduce new resource
	/// owners or change numerical settings between preflight and replay.
	/// # Errors
	/// Collectively rejects any native replay admission failure before mutation.
	pub fn admit_apply(
		&self,
		register: &CollectiveRegister<'_, '_, '_>,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
	) -> Result<()> {
		let mut lane = self.environment.begin(84, self.id, register.id, 0)?;
		self.native_executor(&mut lane, register, targets, mask, value, adjoint)
			.map(drop)
	}

	/// Replay a full coherent preparation or its adjoint using checked native primitives.
	/// # Errors
	/// Collectively rejects register/layout/budget mismatch before mutation; subsequent failure is fatal.
	pub fn apply(
		&mut self,
		register: &mut CollectiveRegister<'_, '_, '_>,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
	) -> Result<()> {
		let mut lane = self.environment.begin(82, self.id, register.id, 0)?;
		let mut executor =
			self.native_executor(&mut lane, register, targets, mask, value, adjoint)?;
		fatal(|| {
			self.replay(&mut lane, targets, mask, value, adjoint, &mut |gate| {
				executor
					.apply(&mut register.inner, gate)
					.map_err(|_| quest_qsvt::Error::Encoding("native amplitude replay failed"))
			})
			.map_err(|_| Error::Value("amplitude native replay failed"))
		});
		Ok(())
	}
}
