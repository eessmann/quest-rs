//! Consuming collectively admitted handoff to the existing fused native owner.
use super::{LoadedMatching, LoadedMatchingResource, PersistenceError, Result, agree};
use crate::{
	MemoryBudget, QubitCount,
	collective::CollectiveEnvironment,
	qsvt::matching::collective::{PreparedMatching, RoutingCapacity},
};
use quest_qsvt::NumericalPolicy;
/// Independent source conversion and whole-live native preparation limits.
#[derive(Clone, Copy, Debug)]
pub struct PersistedPreparationLimits {
	pub max_local_records: usize,
	pub max_bytes: usize,
	pub max_constructor_work: usize,
	pub max_application_payload_bytes: usize,
	pub policy: NumericalPolicy,
	pub capacity: RoutingCapacity,
}
impl Default for PersistedPreparationLimits {
	fn default() -> Self {
		Self {
			max_local_records: 1_048_576,
			max_bytes: 268_435_456,
			max_constructor_work: 1_073_741_824,
			max_application_payload_bytes: 1_073_741_824,
			policy: NumericalPolicy::default(),
			capacity: RoutingCapacity {
				ranks_per_node: 1,
				node_budget: MemoryBudget::new(usize::MAX),
			},
		}
	}
}
/// Successful handoff accounting; native allocation and wire scopes remain separate.
#[derive(Clone, Copy, Debug)]
pub struct PersistedPreparationResources {
	pub loaded_source_bytes: usize,
	pub snapshot_bytes: usize,
	pub target_bytes: usize,
	pub control_stack_allowance: usize,
	/// Conservative whole-stage admitted rank cap, not a measured peak.
	pub rank_peak_bytes: usize,
	/// Planned maximum including source/clone/descriptor and native requested scratch.
	pub planned_rank_peak_bytes: usize,
	pub node_peak_bytes: usize,
	pub native_preparation_scratch_bytes: usize,
	pub constructor_work: usize,
	pub application_payload_bytes: usize,
	pub local_records: usize,
	pub manifest_sha256: [u8; 32],
	pub native_source_identity: u64,
	pub native_construction_identity: u64,
}
#[cfg(test)]
mod failure_tests;
const STACK_ALLOWANCE: usize = 16_384;
fn add(a: usize, b: usize) -> Result<usize> {
	Ok(a.checked_add(b).ok_or(crate::Error::Overflow)?)
}
fn mul(a: usize, b: usize) -> Result<usize> {
	Ok(a.checked_mul(b).ok_or(crate::Error::Overflow)?)
}
fn caught<T>(operation: impl FnOnce() -> Result<T>) -> Result<T> {
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)).unwrap_or(Err(
		PersistenceError::Collective("persisted conversion panic"),
	))
}
fn work(records: usize, parts: usize, width: usize) -> Result<usize> {
	let next = add(records, 1)?;
	let logarithm = usize::try_from(
		next.checked_next_power_of_two()
			.ok_or(crate::Error::Overflow)?
			.ilog2(),
	)
	.map_err(|_| crate::Error::Overflow)?;
	add(
		add(
			mul(
				mul(256, next)?,
				add(add(add(parts, logarithm)?, width)?, 16)?,
			)?,
			mul(512, parts)?,
		)?,
		add(
			4096,
			mul(
				records,
				mul(
					if parts == 1 { 2 } else { 1 },
					quest_qsvt::record_fingerprint_work(7)?,
				)?,
			)?,
		)?,
	)
}
fn payload(records: usize, parts: usize) -> Result<usize> {
	mul(
		parts.checked_sub(1).ok_or(crate::Error::Overflow)?,
		add(add(mul(24, records)?, mul(128, parts)?)?, 16_384)?,
	)
}
fn source_bytes(data: &super::Data) -> Result<usize> {
	let actual = add(
		add(
			mul(
				data.records.capacity(),
				size_of::<quest_qsvt::matching_resource::ResourceMatchingRecord>(),
			)?,
			mul(data.reverse.capacity(), size_of::<super::ReverseRecord>())?,
		)?,
		add(size_of::<super::Data>(), mul(2, size_of::<usize>())?)?,
	)?;
	Ok(actual.max(data.common_bytes))
}
struct Frame {
	bytes: [u8; 1024],
	len: usize,
}
impl Frame {
	fn push(&mut self, bytes: &[u8]) -> Result<()> {
		let end = add(self.len, bytes.len())?;
		self.bytes
			.get_mut(self.len..end)
			.ok_or(PersistenceError::Collective("persisted metadata frame"))?
			.copy_from_slice(bytes);
		self.len = end;
		Ok(())
	}
	fn word(&mut self, n: u64) -> Result<()> {
		self.push(&n.to_le_bytes())
	}
}
fn frame(
	data: &super::Data,
	count: QubitCount,
	targets: &[usize],
	limits: PersistedPreparationLimits,
) -> Result<Frame> {
	let mut frame = Frame {
		bytes: [0; 1024],
		len: 0,
	};
	frame.word(1)?;
	for n in super::header_words(data.header)? {
		frame.word(n)?;
	}
	frame.push(&data.manifest_identity)?;
	for n in [
		count.get(),
		targets.len(),
		limits.max_local_records,
		limits.max_bytes,
		limits.max_constructor_work,
		limits.max_application_payload_bytes,
		limits.policy.max_bytes,
		limits.capacity.ranks_per_node,
		limits.capacity.node_budget.bytes(),
	] {
		frame.word(super::word(n)?)?;
	}
	for &n in targets {
		frame.word(super::word(n)?)?;
	}
	Ok(frame)
}
fn check_bytes(bytes: usize, limit: usize) -> Result<()> {
	if bytes > limit {
		return Err(crate::Error::Budget {
			requested: bytes,
			available: limit,
		}
		.into());
	}
	Ok(())
}
fn peak(
	lane: &mut quest_sys::mpi::MpiCollectiveLane<'_>,
	environment: &CollectiveEnvironment<'_, '_>,
	rank: usize,
	parts: usize,
	extra: usize,
	limit: usize,
	capacity: RoutingCapacity,
) -> Result<usize> {
	let local = agree(
		lane,
		caught(|| {
			let local = add(environment.resources.allocated_bytes(), extra)?;
			check_bytes(local, limit)?;
			Ok(local)
		}),
	)?;
	let maximum = super::maximum(lane, rank, parts, local)?;
	agree(
		lane,
		caught(|| {
			check_bytes(
				mul(maximum, capacity.ranks_per_node)?,
				capacity.node_budget.bytes(),
			)?;
			Ok(maximum)
		}),
	)
}
fn native_error(error: crate::qsvt::Error) -> PersistenceError {
	match error {
		crate::qsvt::Error::Runtime(error) => PersistenceError::Runtime(error),
		crate::qsvt::Error::Model(error) => PersistenceError::Model(error),
		_ => PersistenceError::Collective("native preparation failure"),
	}
}
impl LoadedMatchingResource {
	/// Consume an exclusively owned persisted snapshot into the existing CPU/MPI owner.
	///
	/// All ranks call in common order. Other resource or `LoadedMatching` aliases reject
	/// collectively; a cloned scalar recipe is independent. Loading guards remain caller-owned
	/// through this handoff. This method charges source, clone, target and native overlap
	/// in addition to those guards; temporary duplication is deliberate. Release the
	/// caller's guard only when its separately owned loading resources have dropped.
	/// Native MPI failure retains the underlying job-abort contract, not recoverability.
	/// # Errors
	/// Rejects aliases, mismatched source/layout/policy, caught conversion panic and
	/// constructor work, application-payload, whole-live rank/node or native budgets.
	#[allow(
		clippy::too_many_lines,
		reason = "Linear source ownership, capacity admission and collective handoff boundaries remain visible together"
	)]
	pub fn into_prepared_matching<'env, 'comm, 'runtime>(
		self,
		environment: &'env CollectiveEnvironment<'comm, 'runtime>,
		count: QubitCount,
		targets: Vec<usize>,
		limits: PersistedPreparationLimits,
	) -> Result<(
		PreparedMatching<'env, 'comm, 'runtime>,
		PersistedPreparationResources,
	)> {
		let mut lane = environment.begin(0x504d_5052, environment.identifier(), 0, 0)?;
		let rank = usize::try_from(environment.rank()?).map_err(|_| crate::Error::Overflow)?;
		let parts = usize::try_from(environment.size()?).map_err(|_| crate::Error::Overflow)?;
		let (
			source,
			snapshot,
			target_bytes,
			native_scratch,
			constructor_work,
			application_payload,
			rank_limit,
			effective_capacity,
			rank_cap,
		) = agree(
			&mut lane,
			caught(|| {
				self.data.header.validate()?;
				if environment.resources.capabilities().gpu
					|| self.data.rank != rank
					|| self.data.parts != parts
					|| targets.len() > count.get()
					|| targets.len() != self.data.header.num_qubits()?
					|| self.data.records.len() > limits.max_local_records
					|| self.data.records.len() > self.data.header.record_count
					|| limits.capacity.ranks_per_node == 0
					|| limits.capacity.ranks_per_node > parts
				{
					return Err(PersistenceError::Collective(
						"persisted preparation shape/ownership",
					));
				}
				let constructor_work = work(self.data.header.record_count, parts, count.get())?;
				if constructor_work > limits.max_constructor_work {
					return Err(PersistenceError::Collective("persisted constructor work"));
				}
				let application_payload = payload(self.data.header.record_count, parts)?;
				if application_payload > limits.max_application_payload_bytes {
					return Err(PersistenceError::Collective(
						"persisted application payload",
					));
				}
				let source = source_bytes(&self.data)?;
				let snapshot = add(
					mul(
						self.data.records.len(),
						size_of::<quest_qsvt::MatchingColumn>(),
					)?,
					size_of::<quest_qsvt::MatchingShard>(),
				)?;
				check_bytes(add(source, snapshot)?, limits.policy.max_bytes)?;
				let target_bytes = mul(targets.capacity(), size_of::<usize>())?;
				let mut occupied = 0usize;
				for &target in &targets {
					if target >= count.get() {
						return Err(PersistenceError::Collective("persisted target width"));
					}
					let bit = 1usize
						.checked_shl(u32::try_from(target).map_err(|_| crate::Error::Overflow)?)
						.ok_or(crate::Error::Overflow)?;
					if occupied & bit != 0 {
						return Err(PersistenceError::Collective("persisted target overlap"));
					}
					occupied |= bit;
				}
				let native_scratch = crate::qsvt::matching::collective::preparation_scratch_bytes(
					count,
					parts,
					self.data.records.len(),
				)?;
				let rank_limit = limits
					.max_bytes
					.min(environment.resources.memory_budget().bytes());
				// A native node ceiling also enforces this bridge's rank cap at every later
				// actual-capacity boundary, not merely at the initial requested-size estimate.
				let effective_capacity = RoutingCapacity {
					ranks_per_node: limits.capacity.ranks_per_node,
					node_budget: MemoryBudget::new(
						limits
							.capacity
							.node_budget
							.bytes()
							.min(mul(rank_limit, limits.capacity.ranks_per_node)?),
					),
				};
				let rank_cap = rank_limit.min(
					effective_capacity
						.node_budget
						.bytes()
						.checked_div(effective_capacity.ranks_per_node)
						.ok_or(crate::Error::Overflow)?,
				);
				Ok((
					source,
					snapshot,
					target_bytes,
					native_scratch,
					constructor_work,
					application_payload,
					rank_limit,
					effective_capacity,
					rank_cap,
				))
			}),
		)?;
		let common = agree(
			&mut lane,
			caught(|| frame(&self.data, count, &targets, limits)),
		)?;
		let common_bytes = agree(
			&mut lane,
			caught(|| {
				common
					.bytes
					.get(..common.len)
					.ok_or(PersistenceError::Collective("persisted frame range"))
			}),
		)?;
		crate::collective::equal(&mut lane, common_bytes)?;
		let planned = agree(
			&mut lane,
			caught(|| add(add(add(source, snapshot)?, target_bytes)?, STACK_ALLOWANCE)),
		)?;
		let native_descriptor = agree(
			&mut lane,
			caught(|| add(add(snapshot, target_bytes)?, 4096)),
		)?;
		let extra = agree(
			&mut lane,
			caught(|| add(add(planned, native_descriptor)?, native_scratch)),
		)?;
		let before = peak(
			&mut lane,
			environment,
			rank,
			parts,
			extra,
			rank_limit,
			effective_capacity,
		)?;
		let mut temporary = agree(
			&mut lane,
			environment
				.resources
				.reserve(planned)
				.map_err(PersistenceError::from),
		)?;
		let Self { data } = self;
		let data = agree(
			&mut lane,
			caught(|| {
				std::sync::Arc::try_unwrap(data)
					.map_err(|_| PersistenceError::Collective("persisted source alias"))
			}),
		)?;
		let mut columns = agree(
			&mut lane,
			caught(|| super::reserve::<quest_qsvt::MatchingColumn>(data.records.len())),
		)?;
		let (snapshot, native_descriptor) = agree(
			&mut lane,
			caught(|| {
				let snapshot = add(
					mul(columns.capacity(), size_of::<quest_qsvt::MatchingColumn>())?,
					size_of::<quest_qsvt::MatchingShard>(),
				)?;
				check_bytes(add(source, snapshot)?, limits.policy.max_bytes)?;
				temporary.resize(add(
					add(add(source, snapshot)?, target_bytes)?,
					STACK_ALLOWANCE,
				)?)?;
				Ok((snapshot, add(add(snapshot, target_bytes)?, 4096)?))
			}),
		)?;
		let extra = agree(&mut lane, caught(|| add(native_descriptor, native_scratch)))?;
		let after = peak(
			&mut lane,
			environment,
			rank,
			parts,
			extra,
			rank_limit,
			effective_capacity,
		)?;
		let shard = agree(
			&mut lane,
			caught(|| {
				#[cfg(test)]
				failure_tests::inject(rank);
				columns.extend(data.records.iter().map(|r| r.column));
				Ok(quest_qsvt::MatchingShard::from_parts(
					data.header,
					data.rank,
					data.parts,
					columns,
					limits.policy,
				)?)
			}),
		)?;
		let descriptor = agree(
			&mut lane,
			caught(|| {
				Ok(quest_qsvt::EncodingDescriptor::from_matching_header(
					shard.header(),
				)?)
			}),
		)?;
		#[cfg(test)]
		failure_tests::converted();
		drop(lane);
		let prepared = environment
			.prepare_matching_with_capacity(shard, count, targets, effective_capacity)
			.map_err(native_error)?;
		// These are conservative admitted whole-stage caps, not a measured high-water
		// mark. Native preparation audits its actual temporary/routing capacities too.
		let resources = PersistedPreparationResources {
			loaded_source_bytes: source,
			snapshot_bytes: snapshot,
			target_bytes,
			control_stack_allowance: STACK_ALLOWANCE,
			planned_rank_peak_bytes: before.max(after),
			rank_peak_bytes: rank_cap,
			node_peak_bytes: effective_capacity.node_budget.bytes(),
			native_preparation_scratch_bytes: native_scratch,
			constructor_work,
			application_payload_bytes: application_payload,
			local_records: data.records.len(),
			manifest_sha256: data.manifest_identity,
			native_source_identity: descriptor.source_identity,
			native_construction_identity: descriptor.construction_identity,
		};
		drop(data);
		drop(temporary);
		Ok((prepared, resources))
	}
}

impl LoadedMatching {
	/// Consume the verified source into native execution, discarding its scalar replay recipe.
	/// # Errors
	/// Preserves the same ownership, metadata, byte and native admission checks as
	/// [`LoadedMatchingResource::into_prepared_matching`].
	pub fn into_prepared_matching<'env, 'comm, 'runtime>(
		self,
		environment: &'env CollectiveEnvironment<'comm, 'runtime>,
		count: QubitCount,
		targets: Vec<usize>,
		limits: PersistedPreparationLimits,
	) -> Result<(
		PreparedMatching<'env, 'comm, 'runtime>,
		PersistedPreparationResources,
	)> {
		LoadedMatchingResource { data: self.data }.into_prepared_matching(
			environment,
			count,
			targets,
			limits,
		)
	}
}
