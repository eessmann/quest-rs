//! Prepared standard QSVT schedules over owned sparse matching shards.
//!
//! The small schedule contains no matrix, circuit or global state. Projector
//! phases scan native local partitions in bounded chunks, including unsuccessful
//! flag/color sectors and arbitrary spectator qubits.
use super::{Result, matching::PreparedMatching};
use crate::{
	Environment, QubitCount, Register, StateVector, environment::Reservation, error::BackendResult,
};
use quest_qsvt::{
	MatchingHeader, MatchingShard,
	replay_transform::{MatchingSchedule, TransformStep},
};

const PHASE_CHUNK: usize = 256;
const PHASE_BYTES: usize = PHASE_CHUNK * size_of::<quest_sys::QuestComplex>();
// Include collective metadata comparison storage and scalar routing frames as
// well as the phase buffer, even though their peaks do not usually coincide.
const SCRATCH_BYTES: usize = PHASE_BYTES + 8192 + 512;

struct ProjectorLayout {
	count: QubitCount,
	response: usize,
	response_mask: usize,
	zero_mask: usize,
	system: Vec<(usize, usize)>,
	contiguous_system: Option<(usize, usize)>,
	rows: usize,
	cols: usize,
}
impl ProjectorLayout {
	fn new(
		header: MatchingHeader,
		count: QubitCount,
		targets: &[usize],
		response: usize,
	) -> crate::Result<Self> {
		header
			.validate()
			.map_err(|_| crate::Error::Value("invalid matching header"))?;
		if targets.len() != header.num_qubits().map_err(|_| crate::Error::Overflow)? {
			return Err(crate::Error::Value("QSVT matching target count"));
		}
		let mut active = 0;
		for &target in targets {
			if target >= count.get() || active & super::matching::bit(target)? != 0 {
				return Err(crate::Error::Value("invalid QSVT matching targets"));
			}
			active |= super::matching::bit(target)?;
		}
		if response >= count.get() || targets.contains(&response) {
			return Err(crate::Error::Value(
				"QSVT response overlaps matching targets or exceeds register",
			));
		}
		let response_mask = super::matching::bit(response)?;
		let mut system = Vec::new();
		system
			.try_reserve_exact(header.system_qubits)
			.map_err(|_| crate::Error::Overflow)?;
		let color_start = header
			.system_qubits
			.checked_add(1)
			.ok_or(crate::Error::Overflow)?;
		for (packed, index) in (1..color_start).enumerate() {
			let physical = *targets
				.get(index)
				.ok_or(crate::Error::Value("QSVT system target"))?;
			system.push((
				super::matching::bit(physical)?,
				super::matching::bit(packed)?,
			));
		}
		let mut zero_mask = super::matching::bit(
			*targets
				.first()
				.ok_or(crate::Error::Value("QSVT flag target"))?,
		)?;
		for index in color_start..targets.len() {
			zero_mask |= super::matching::bit(
				*targets
					.get(index)
					.ok_or(crate::Error::Value("QSVT color target"))?,
			)?;
		}
		let system_targets = targets.get(1..color_start).ok_or(crate::Error::Overflow)?;
		let contiguous_system = system_targets
			.windows(2)
			.all(|pair| {
				pair.first()
					.and_then(|target| target.checked_add(1))
					.as_ref()
					== pair.get(1)
			})
			.then(|| {
				(
					system
						.iter()
						.fold(0, |mask, &(physical, _)| mask | physical),
					system_targets.first().copied().unwrap_or(0),
				)
			});
		Ok(Self {
			count,
			response,
			response_mask,
			zero_mask,
			system,
			contiguous_system,
			rows: header.rows,
			cols: header.cols,
		})
	}
	fn logical_index(&self, basis: usize) -> usize {
		if let Some((mask, shift)) = self.contiguous_system {
			return (basis & mask) >> shift;
		}
		self.system.iter().fold(0, |index, &(physical, logical)| {
			if basis & physical == 0 {
				index
			} else {
				index | logical
			}
		})
	}
	fn bytes(&self, schedule: &MatchingSchedule) -> Result<usize> {
		self.system
			.capacity()
			.checked_mul(size_of::<(usize, usize)>())
			.and_then(|n| n.checked_add(SCRATCH_BYTES))
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(schedule.retained_bytes().ok()?))
			.ok_or_else(|| crate::Error::Overflow.into())
	}
	#[allow(
		clippy::arithmetic_side_effects,
		reason = "Unit-modulus finite phase factors multiply already admitted native amplitudes"
	)]
	fn phase(
		&self,
		register: &mut Register<'_, StateVector>,
		left: bool,
		angle: f64,
		response: bool,
	) -> crate::Result<()> {
		let positive = crate::Complex64::from_polar(1.0, angle);
		let negative = positive.conj();
		let bound = if left { self.rows } else { self.cols };
		let local = register.deployment().local_amplitudes();
		let global_start = register
			.deployment()
			.rank()
			.checked_mul(local)
			.ok_or(crate::Error::Overflow)?;
		let mut buffer = [quest_sys::QuestComplex { re: 0.0, im: 0.0 }; PHASE_CHUNK];
		for start in (0..local).step_by(PHASE_CHUNK) {
			let length = local
				.checked_sub(start)
				.ok_or(crate::Error::Overflow)?
				.min(PHASE_CHUNK);
			let chunk_start = global_start
				.checked_add(start)
				.ok_or(crate::Error::Overflow)?;
			// Native state partitions and these chunks have aligned power-of-two
			// lengths, so sufficiently high response bits are constant in a chunk.
			if self.response_mask >= length && (chunk_start & self.response_mask != 0) != response {
				continue;
			}
			let chunk = buffer.get_mut(..length).ok_or(crate::Error::Overflow)?;
			let native_start = i64::try_from(start).map_err(|_| crate::Error::Overflow)?;
			quest_sys::read_local_qureg_amps(&register.native, native_start, chunk)
				.context("reading QSVT projector partition")?;
			for (offset, value) in chunk.iter_mut().enumerate() {
				let basis = chunk_start
					.checked_add(offset)
					.ok_or(crate::Error::Overflow)?;
				if (basis & self.response_mask != 0) != response {
					continue;
				}
				let factor = if basis & self.zero_mask == 0 && self.logical_index(basis) < bound {
					positive
				} else {
					negative
				};
				let updated = crate::Complex64::new(value.re, value.im) * factor;
				*value = quest_sys::QuestComplex {
					re: updated.re,
					im: updated.im,
				};
			}
			quest_sys::write_local_qureg_amps(register.pin(), native_start, chunk)
				.context("writing QSVT projector partition")?;
		}
		Ok(())
	}
	fn native_step(
		&self,
		register: &mut Register<'_, StateVector>,
		step: TransformStep,
	) -> crate::Result<()> {
		match step {
			TransformStep::Hadamard => register.h(self.response),
			TransformStep::ResponseRotation(angle) => quest_sys::apply_rotate_z(
				register.pin(),
				i32::try_from(self.response).map_err(|_| crate::Error::Overflow)?,
				angle,
			)
			.context("applying QSVT response rotation"),
			TransformStep::Projector {
				left,
				angle,
				response,
			} => self.phase(register, left, angle, response),
			TransformStep::Oracle { .. } => {
				Err(crate::Error::Value("oracle requires matching resource"))
			}
		}
	}
}

/// Owning QSVT schedule and matching resource tied to the allocating environment.
pub struct PreparedMatchingTransform<'env> {
	matching: PreparedMatching<'env>,
	schedule: MatchingSchedule,
	layout: ProjectorLayout,
	reservation: Reservation<'env>,
}
impl Environment {
	/// Prepare a source-independent QSVT schedule over one immutable local shard.
	/// `targets` maps flag, system, then color bits; `response` is separate.
	/// # Errors
	/// Rejects source mismatch, invalid layouts, unsupported deployment and budgets.
	pub fn prepare_matching_transform(
		&self,
		shard: MatchingShard,
		count: QubitCount,
		targets: Vec<usize>,
		response: usize,
		schedule: MatchingSchedule,
	) -> Result<PreparedMatchingTransform<'_>> {
		if shard.header() != schedule.header() {
			return Err(crate::Error::Value("QSVT schedule and matching source differ").into());
		}
		let layout = ProjectorLayout::new(shard.header(), count, &targets, response)?;
		let reservation = self.resources.reserve(layout.bytes(&schedule)?)?;
		let matching = self.prepare_matching(shard, count, targets)?;
		Ok(PreparedMatchingTransform {
			matching,
			schedule,
			layout,
			reservation,
		})
	}
}
impl PreparedMatchingTransform<'_> {
	#[must_use]
	pub const fn schedule(&self) -> &MatchingSchedule {
		&self.schedule
	}
	/// Replay the whole transform or its adjoint, retaining all failure amplitudes.
	/// # Errors
	/// Rejects a different environment or width before state mutation, and native failures.
	pub fn apply(&mut self, register: &mut Register<'_, StateVector>, adjoint: bool) -> Result<()> {
		if !std::ptr::eq(register.resources(), self.reservation.environment)
			|| register.num_qubits() != self.layout.count
		{
			return Err(crate::Error::Value("QSVT register owner or width").into());
		}
		self.schedule.visit_steps(adjoint, |step| {
			if let TransformStep::Oracle { adjoint, response } = step {
				self.matching.apply(
					register,
					adjoint,
					self.layout.response_mask,
					if response {
						self.layout.response_mask
					} else {
						0
					},
				)
			} else {
				self.layout.native_step(register, step).map_err(Into::into)
			}
		})
	}
}

/// Collective execution of exactly the same schedule over distinct sparse shards.
#[cfg(all(feature = "mpi", quest_native_mpi))]
pub mod collective {
	use super::{
		MatchingSchedule, MatchingShard, ProjectorLayout, QubitCount, Reservation, Result,
		TransformStep,
	};
	use crate::{
		collective::{CollectiveEnvironment, CollectiveRegister, equal},
		error::BackendResult,
	};
	use quest_sys::mpi::MpiCollectiveLane;

	fn agree<T>(lane: &mut MpiCollectiveLane<'_>, value: Result<T>) -> Result<T> {
		if !lane
			.all_agree(value.is_ok())
			.context("agreeing QSVT schedule admission")?
		{
			return Err(crate::Error::Value("collective QSVT schedule admission rejected").into());
		}
		value
	}
	fn fatal(operation: impl FnOnce() -> crate::Result<()>) {
		std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
			.unwrap_or_else(|_| quest_sys::mpi::abort_job())
			.unwrap_or_else(|_| quest_sys::mpi::abort_job());
	}
	fn step_payload(step: TransformStep) -> [u8; 32] {
		let words = match step {
			TransformStep::Hadamard => [0, 0, 0, 0],
			TransformStep::ResponseRotation(angle) => [1, angle.to_bits(), 0, 0],
			TransformStep::Projector {
				left,
				angle,
				response,
			} => [2, angle.to_bits(), u64::from(left), u64::from(response)],
			TransformStep::Oracle { adjoint, response } => {
				[3, u64::from(adjoint), u64::from(response), 0]
			}
		};
		let mut bytes = [0; 32];
		for (slot, value) in bytes.as_chunks_mut::<8>().0.iter_mut().zip(words) {
			slot.copy_from_slice(&value.to_le_bytes());
		}
		bytes
	}

	/// Prepared schedule, rank-owned coefficients and local scratch only.
	pub struct PreparedMatchingTransform<'env, 'comm, 'runtime> {
		environment: &'env CollectiveEnvironment<'comm, 'runtime>,
		matching: crate::qsvt::matching::collective::PreparedMatching<'env, 'comm, 'runtime>,
		schedule: MatchingSchedule,
		layout: ProjectorLayout,
		_reservation: Reservation<'env>,
		id: u64,
	}
	impl<'comm, 'runtime> CollectiveEnvironment<'comm, 'runtime> {
		/// Admit identical compact schedules and distinct owned coefficient shards.
		/// Neither preparation nor execution gathers matrices, circuits or states.
		/// # Errors
		/// Rejects inconsistent schedules, source identities, ownership, layouts or budgets collectively.
		pub fn prepare_matching_transform(
			&self,
			shard: MatchingShard,
			count: QubitCount,
			targets: Vec<usize>,
			response: usize,
			schedule: MatchingSchedule,
		) -> Result<PreparedMatchingTransform<'_, 'comm, 'runtime>> {
			let id = self.identifier();
			let mut lane = self.begin(50, id, 0, 0)?;
			let layout = agree(
				&mut lane,
				(|| {
					if shard.header() != schedule.header() {
						return Err(crate::Error::Value(
							"QSVT schedule and matching source differ",
						)
						.into());
					}
					ProjectorLayout::new(shard.header(), count, &targets, response)
						.map_err(Into::into)
				})(),
			)?;
			let reservation = agree(
				&mut lane,
				layout
					.bytes(&schedule)
					.and_then(|bytes| self.resources.reserve(bytes).map_err(Into::into)),
			)?;
			let metadata = agree(
				&mut lane,
				(|| {
					Ok([
						u64::try_from(schedule.degree()).map_err(|_| crate::Error::Overflow)?,
						u64::try_from(response).map_err(|_| crate::Error::Overflow)?,
						schedule.conversion_roundoff_estimate().to_bits(),
						u64::from(schedule.projector_response_bound().is_some()),
						schedule.projector_response_bound().unwrap_or(0.0).to_bits(),
					])
				})(),
			)?;
			for word in metadata {
				equal(&mut lane, &word.to_le_bytes())?;
			}
			schedule.visit_steps::<super::super::Error>(false, |step| {
				equal(&mut lane, &step_payload(step)).map_err(Into::into)
			})?;
			drop(lane);
			let matching = self.prepare_matching(shard, count, targets)?;
			Ok(PreparedMatchingTransform {
				environment: self,
				matching,
				schedule,
				layout,
				_reservation: reservation,
				id,
			})
		}
	}
	impl PreparedMatchingTransform<'_, '_, '_> {
		#[must_use]
		pub const fn schedule(&self) -> &MatchingSchedule {
			&self.schedule
		}
		/// Replay on native local partitions, preserving all successful and failure sectors.
		/// # Errors
		/// Rejects inconsistent operation order, register ownership or adjoint direction collectively.
		pub fn apply(
			&mut self,
			register: &mut CollectiveRegister<'_, '_, '_>,
			adjoint: bool,
		) -> Result<()> {
			let mut lane = self
				.environment
				.begin(51, self.id, register.id, u64::from(adjoint))?;
			agree(
				&mut lane,
				if std::ptr::eq(register.environment, self.environment)
					&& register.num_qubits() == self.layout.count
				{
					Ok(())
				} else {
					Err(crate::Error::Value("QSVT register owner or width").into())
				},
			)?;
			drop(lane);
			self.schedule.visit_steps(adjoint, |step| {
				if let TransformStep::Oracle { adjoint, response } = step {
					self.matching.apply(
						register,
						adjoint,
						self.layout.response_mask,
						if response {
							self.layout.response_mask
						} else {
							0
						},
					)
				} else {
					let _lane = self.environment.begin(52, self.id, register.id, 0)?;
					fatal(|| self.layout.native_step(&mut register.inner, step));
					Ok(())
				}
			})
		}
	}
}
