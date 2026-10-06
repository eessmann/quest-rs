//! Prepared standard QSVT schedules over owned sparse matching shards.
//!
//! The small schedule contains no matrix, circuit or global state. Projector
//! phases scan native local partitions in bounded chunks, including unsuccessful
//! flag/color sectors and arbitrary spectator qubits.
use super::{Result, matching::PreparedMatching};
use crate::{Environment, QubitCount, Register, StateVector, environment::Reservation};
use quest_qsvt::{
	MatchingHeader, MatchingShard,
	replay_transform::{MatchingSchedule, TransformStep},
};

struct ProjectorLayout(super::transform_execution::TransformLayout);
impl ProjectorLayout {
	fn new(
		header: MatchingHeader,
		count: QubitCount,
		targets: &[usize],
		response: usize,
	) -> crate::Result<Self> {
		let descriptor = quest_qsvt::EncodingDescriptor::from_matching_header(header)
			.map_err(|_| crate::Error::Value("invalid matching header"))?;
		super::transform_execution::TransformLayout::new(
			&descriptor,
			count,
			targets,
			response,
			|_| Ok(()),
		)
		.map(Self)
		.map_err(|_| crate::Error::Value("QSVT matching target layout"))
	}
	fn bytes(&self, schedule: &MatchingSchedule) -> Result<usize> {
		self.0
			.bytes(schedule.general_schedule(), size_of::<MatchingHeader>())
	}
	fn native_step(
		&self,
		register: &mut Register<'_, StateVector>,
		step: TransformStep,
	) -> crate::Result<()> {
		self.0.legacy_step(register, step)
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
			|| register.num_qubits() != self.layout.0.count
		{
			return Err(crate::Error::Value("QSVT register owner or width").into());
		}
		self.schedule.visit_steps(adjoint, |step| {
			if let TransformStep::Oracle { adjoint, response } = step {
				self.matching.apply(
					register,
					adjoint,
					self.layout.0.response_mask,
					if response {
						self.layout.0.response_mask
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
					&& register.num_qubits() == self.layout.0.count
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
						self.layout.0.response_mask,
						if response {
							self.layout.0.response_mask
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
