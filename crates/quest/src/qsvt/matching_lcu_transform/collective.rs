//! Collective complete-transform admission and fatal post-emission replay.
use super::{
	Error, ReplayGateExecutor, Reservation, Result, TransformExecutionLimits,
	TransformExecutionResources, TransformLayout, TransformSchedule, TransformStep, add, counts,
	dryrun, mul, pattern_floor, peak, preflight_floor,
};
use crate::qsvt::matching_lcu::collective::PreparedMatchingLcu;
use crate::qsvt::matching_lcu::telemetry::RoutingTelemetry;
use crate::{
	collective::{CollectiveEnvironment, CollectiveRegister, equal},
	error::BackendResult,
};
use quest_sys::mpi::MpiCollectiveLane;
fn agree<T>(lane: &mut MpiCollectiveLane<'_>, result: Result<T>) -> Result<T> {
	if !lane
		.all_agree(result.is_ok())
		.context("agreeing LCU transform admission")?
	{
		return Err(Error::Value("collective LCU transform admission").into());
	}
	result
}
fn fatal<T>(f: impl FnOnce() -> Result<T>) -> T {
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
}
fn word(n: usize) -> crate::Result<u64> {
	u64::try_from(n).map_err(|_| Error::Overflow)
}
fn payload(step: TransformStep) -> [u8; 32] {
	let words = match step {
		TransformStep::Hadamard => [0, 0, 0, 0],
		TransformStep::ResponseRotation(a) => [1, a.to_bits(), 0, 0],
		TransformStep::Projector {
			left,
			angle,
			response,
		} => [2, angle.to_bits(), u64::from(left), u64::from(response)],
		TransformStep::Oracle { adjoint, response } => {
			[3, u64::from(adjoint), u64::from(response), 0]
		}
	};
	let mut b = [0; 32];
	for (chunk, w) in b.as_chunks_mut::<8>().0.iter_mut().zip(words) {
		chunk.copy_from_slice(&w.to_le_bytes());
	}
	b
}
fn frame(
	source: &PreparedMatchingLcu<'_, '_, '_>,
	response: usize,
	schedule: &TransformSchedule,
	l: TransformExecutionLimits,
) -> Result<Vec<u8>> {
	let mut b = crate::values::reserve_vec(1024)?;
	if b.capacity() > 4096 {
		return Err(Error::Value("LCU transform frame capacity").into());
	}
	let mut push = |n: u64| b.extend_from_slice(&n.to_le_bytes());
	for n in [
		source.num_qubits().get(),
		response,
		schedule.degree(),
		l.max_degree,
		l.max_schedule_steps,
		l.max_queries,
		l.max_constructor_work,
		l.max_preflight_work,
		l.max_local_bytes,
		l.node_budget.bytes(),
		l.ranks_per_node,
		l.max_rank_work,
		l.max_aggregate_work,
		l.max_native_dispatches,
		l.max_application_bytes,
		l.max_control_calls,
	] {
		push(word(n)?);
	}
	let d = schedule.descriptor();
	for n in [
		d.rows,
		d.cols,
		d.layout.num_qubits,
		d.layout.system_mask,
		d.layout.workspace_mask,
		d.layout.clean_workspace_mask,
		d.layout.clean_workspace_value,
		d.left.fixed_mask,
		d.left.fixed_value,
		d.left.logical_range.start,
		d.left.logical_range.end,
		d.right.fixed_mask,
		d.right.fixed_value,
		d.right.logical_range.start,
		d.right.logical_range.end,
		source.targets().len(),
	] {
		push(word(n)?);
	}
	for n in [
		d.normalization.to_bits(),
		d.source_identity,
		d.construction_identity,
		u64::from(d.errors.binary64_parameters),
		u64::from(d.errors.preparation.is_some()),
		d.errors.preparation.unwrap_or(0.).to_bits(),
		u64::from(d.errors.encoding.is_some()),
		d.errors.encoding.unwrap_or(0.).to_bits(),
		schedule.readout_phase().to_bits(),
		schedule.conversion_roundoff_estimate().to_bits(),
		u64::from(schedule.projector_response_bound().is_some()),
		schedule.projector_response_bound().unwrap_or(0.).to_bits(),
	] {
		push(n);
	}
	for &t in source.targets() {
		push(word(t)?);
	}
	Ok(b)
}
fn node_peak(
	lane: &mut MpiCollectiveLane<'_>,
	environment: &CollectiveEnvironment<'_, '_>,
	l: TransformExecutionLimits,
	parts: usize,
) -> Result<usize> {
	let own = agree(
		lane,
		peak(&environment.resources, l, parts).map_err(Into::into),
	)?;
	let mut maximum = own;
	for peer in 0..parts {
		let mut b = word(own)?.to_le_bytes();
		lane.broadcast_bytes(i32::try_from(peer).map_err(|_| Error::Overflow)?, &mut b)
			.context("admitting transform node peak")?;
		maximum = maximum.max(usize::try_from(u64::from_le_bytes(b)).map_err(|_| Error::Overflow)?);
	}
	agree(
		lane,
		(|| -> Result<()> {
			if mul(maximum, l.ranks_per_node)? > l.node_budget.bytes() {
				return Err(Error::Value("transform maximum node byte budget").into());
			}
			Ok(())
		})(),
	)?;
	Ok(own)
}
/// Constructor comparison only, excluding source construction and protocol/native wire.
#[derive(Clone, Copy, Debug)]
pub struct TransformPreparationCommunication {
	pub compared_steps: usize,
	pub comparison_collective_calls: usize,
	pub aggregate_comparison_broadcast_payload_bytes: usize,
	/// Additional scalar construction agreements/node-admission calls, excluding protocol/native internals.
	pub other_constructor_collective_calls_upper_bound: usize,
}
/// One immutable schedule over K independently guarded sparse sources and native scratches.
pub struct PreparedMatchingLcuTransform<'env, 'comm, 'runtime> {
	environment: &'env CollectiveEnvironment<'comm, 'runtime>,
	source: PreparedMatchingLcu<'env, 'comm, 'runtime>,
	schedule: TransformSchedule,
	layout: TransformLayout,
	executor: ReplayGateExecutor<'env>,
	_reservation: Reservation<'env>,
	limits: TransformExecutionLimits,
	id: u64,
	constructor_work: usize,
	communication: TransformPreparationCommunication,
	last_apply_telemetry: Option<RoutingTelemetry>,
	#[cfg(test)]
	test_preflight_failure: Option<bool>,
	#[cfg(test)]
	test_post_emission_failure: Option<bool>,
	#[cfg(test)]
	test_query_failure: Option<bool>,
}
impl<'comm, 'runtime> CollectiveEnvironment<'comm, 'runtime> {
	/// Bind identical compiled descriptor/schedule contracts without gathering any child source.
	/// # Errors
	/// Rejects any rank's owner, layout, work, compiled stream or simultaneous live bytes.
	#[allow(
		clippy::too_many_lines,
		reason = "Collective construction keeps local validation/common agreement/compiled comparison in explicit protocol order"
	)]
	pub fn prepare_matching_lcu_transform<'env>(
		&'env self,
		source: PreparedMatchingLcu<'env, 'comm, 'runtime>,
		response: usize,
		schedule: TransformSchedule,
		limits: TransformExecutionLimits,
	) -> Result<PreparedMatchingLcuTransform<'env, 'comm, 'runtime>> {
		let id = self.identifier();
		let mut lane = self.begin(0x4c54_4630, id, 0, 0)?;
		let parts = usize::try_from(self.size()?).map_err(|_| Error::Overflow)?;
		let (_, constructor_work) = agree(
			&mut lane,
			(|| -> Result<_> {
				if !std::ptr::eq(source.environment(), self)
					|| source.plan().descriptor() != schedule.descriptor()
				{
					return Err(Error::Value("LCU transform source contract").into());
				}
				counts(&schedule, limits, parts)
			})(),
		)?;
		let planned = agree(
			&mut lane,
			TransformLayout::planned_bytes(
				&schedule,
				size_of::<PreparedMatchingLcuTransform<'_, '_, '_>>(),
			),
		)?;
		let mut reservation = agree(
			&mut lane,
			self.resources.reserve(planned).map_err(Into::into),
		)?;
		node_peak(&mut lane, self, limits, parts)?;
		let count = source.num_qubits();
		let base = agree(
			&mut lane,
			planned
				.checked_sub(mul(
					schedule.descriptor().layout.num_qubits,
					size_of::<[(usize, usize); 2]>(),
				)?)
				.ok_or(Error::Overflow)
				.map_err(Into::into),
		)?;
		let mut vectors = 0;
		let mut remaining = 2usize;
		let one_vector = mul(
			schedule.descriptor().layout.num_qubits,
			size_of::<(usize, usize)>(),
		)?;
		let layout = agree(
			&mut lane,
			TransformLayout::new(
				schedule.descriptor(),
				count,
				source.targets(),
				response,
				|bytes| {
					vectors = add(vectors, bytes)?;
					remaining = remaining.checked_sub(1).ok_or(Error::Overflow)?;
					reservation.resize(add(add(base, vectors)?, mul(remaining, one_vector)?)?)?;
					peak(&self.resources, limits, parts)?;
					Ok(())
				},
			),
		)?;
		agree(
			&mut lane,
			reservation
				.resize(layout.bytes(
					&schedule,
					size_of::<PreparedMatchingLcuTransform<'_, '_, '_>>(),
				)?)
				.map_err(Into::into),
		)?;
		node_peak(&mut lane, self, limits, parts)?;
		let executor = agree(
			&mut lane,
			ReplayGateExecutor::with_resources(&self.resources, count).map_err(Into::into),
		)?;
		node_peak(&mut lane, self, limits, parts)?;
		// Caught complete immutable local validation before data-dependent comparison loops.
		agree(
			&mut lane,
			std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
				for adjoint in [false, true] {
					schedule.visit_steps(adjoint, |step| -> Result<()> {
						let _ = payload(step);
						Ok(())
					})?;
				}
				Ok(())
			}))
			.unwrap_or_else(|_| {
				Err(Error::Value("LCU transform compiled validation panic").into())
			}),
		)?;
		let raw = agree(&mut lane, frame(&source, response, &schedule, limits))?;
		equal(&mut lane, &raw)?;
		let raw_len = raw.len();
		drop(raw);
		for adjoint in [false, true] {
			schedule.visit_steps(adjoint, |step| -> Result<()> {
				equal(&mut lane, &payload(step))?;
				Ok(())
			})?;
		}
		let steps = mul(add(mul(schedule.degree(), 4)?, 5)?, 2)?;
		let communication = TransformPreparationCommunication {
			compared_steps: steps,
			other_constructor_collective_calls_upper_bound: mul(256, parts)?,
			comparison_collective_calls: add(mul(steps, 4)?, 4)?,
			aggregate_comparison_broadcast_payload_bytes: mul(
				parts.checked_sub(1).ok_or(Error::Overflow)?,
				add(add(raw_len, 8)?, mul(steps, 40)?)?,
			)?,
		};
		Ok(PreparedMatchingLcuTransform {
			environment: self,
			source,
			schedule,
			layout,
			executor,
			_reservation: reservation,
			limits,
			id,
			constructor_work,
			communication,
			last_apply_telemetry: None,
			#[cfg(test)]
			test_preflight_failure: None,
			#[cfg(test)]
			test_post_emission_failure: None,
			#[cfg(test)]
			test_query_failure: None,
		})
	}
}
impl PreparedMatchingLcuTransform<'_, '_, '_> {
	/// Local cumulative matching-routing facts for the most recent apply attempt.
	/// Only full success installs a receipt; apply clears it before preflight.
	/// Standalone `admit_apply` preserves it. Response/projector/PREP/native internal,
	/// constructor, separate coordinator and MPI protocol traffic are excluded.
	#[must_use]
	pub const fn last_apply_telemetry(&self) -> Option<RoutingTelemetry> {
		self.last_apply_telemetry
	}
	#[must_use]
	pub const fn schedule(&self) -> &TransformSchedule {
		&self.schedule
	}
	#[must_use]
	pub const fn source_plan(&self) -> &quest_qsvt::portfolio::LcuPlan {
		self.source.plan()
	}
	#[must_use]
	pub const fn constructor_work(&self) -> usize {
		self.constructor_work
	}
	#[must_use]
	pub const fn constructor_communication(&self) -> TransformPreparationCommunication {
		self.communication
	}
	fn admit_patterns(
		&self,
		register: &CollectiveRegister<'_, '_, '_>,
		adjoint: bool,
		mask: usize,
		value: usize,
	) -> Result<(TransformExecutionResources, [bool; 4])> {
		let mut lane =
			self.environment
				.begin(0x4c54_4631, self.id, register.id, u64::from(adjoint))?;
		agree(
			&mut lane,
			if !std::ptr::eq(register.environment, self.environment)
				|| register.num_qubits() != self.layout.count
			{
				Err(Error::Value("LCU transform register owner/width").into())
			} else {
				Ok(())
			},
		)?;
		for n in [mask, value] {
			equal(&mut lane, &word(n)?.to_le_bytes())?;
		}
		let parts = register.deployment().nodes();
		agree(
			&mut lane,
			(|| -> Result<()> {
				self.layout.controls(mask, value)?;
				preflight_floor(
					&self.schedule,
					self.limits,
					self.layout.count.get(),
					parts,
					pattern_floor(self.source.plan(), self.layout.count.get())?,
				)?;
				Ok(())
			})(),
		)?;
		let peak = node_peak(&mut lane, self.environment, self.limits, parts)?;
		let (mut r, used) = agree(
			&mut lane,
			std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
				dryrun(
					&self.schedule,
					&self.layout,
					self.limits,
					register.deployment().local_amplitudes(),
					parts,
					adjoint,
					mask,
					value,
					pattern_floor(self.source.plan(), self.layout.count.get())?,
					|a, b| {
						#[cfg(test)]
						if a && b
							&& let Some(panic) = self.test_preflight_failure
						{
							failure_tests::inject(panic)?;
						}
						let (m, v) = self.layout.query_controls(b, mask, value);
						self.source.validate_apply_locally(register, a, m, v)
					},
					|g| {
						self.executor
							.validate(&register.inner, g)
							.map_err(Into::into)
					},
				)
			}))
			.unwrap_or_else(|_| Err(Error::Value("LCU transform local admission panic").into())),
		)?;
		r.managed_rank_peak_bytes = peak;
		drop(lane);
		// Common fixed pattern loop; parent coordination lane is absent at every source entry.
		for (i, &used) in used.iter().enumerate() {
			if used {
				let (m, v) = self.layout.query_controls(i % 2 != 0, mask, value);
				self.source.admit_apply(register, i / 2 != 0, m, v)?;
			}
		}
		Ok((r, used))
	}
	/// Complete common admission of every source query pattern and response/phase before H.
	/// # Errors
	/// Any rank's local failure/panic, source/node cap or aggregate budget rejects unchanged.
	pub fn admit_apply(
		&self,
		register: &CollectiveRegister<'_, '_, '_>,
		adjoint: bool,
		mask: usize,
		value: usize,
	) -> Result<TransformExecutionResources> {
		self.admit_patterns(register, adjoint, mask, value)
			.map(|(r, _)| r)
	}
	/// Full transform or literal U†; every error/panic after emission aborts the MPI job.
	/// # Errors
	/// Returns only whole-operation preflight failures; no recoverable modified-state return.
	#[allow(
		clippy::single_match_else,
		reason = "Oracle steps release parent lanes; all nonoracle variants enter the same fatal guarded lane"
	)]
	pub fn apply(
		&mut self,
		register: &mut CollectiveRegister<'_, '_, '_>,
		adjoint: bool,
		mask: usize,
		value: usize,
	) -> Result<TransformExecutionResources> {
		self.last_apply_telemetry = None;
		let r = self.admit_apply(register, adjoint, mask, value)?;
		let mut telemetry = RoutingTelemetry::default();
		#[cfg(test)]
		let mut fault = self.test_post_emission_failure;
		fatal(|| {
			self.schedule.visit_steps(adjoint, |step| -> Result<()> {
				match step {
					TransformStep::Oracle { adjoint, response } => {
						#[cfg(test)]
						if let Some(panic) = self.test_query_failure {
							use std::io::Write as _;
							println!("LCU_TRANSFORM_LATE_QUERY_FAILURE");
							let _ = std::io::stdout().flush();
							failure_tests::inject(panic)?;
						}
						let (m, v) = self.layout.query_controls(response, mask, value);
						self.source.apply(register, adjoint, m, v)?;
						telemetry.source(self.source.last_apply_telemetry().ok_or(
							Error::Value("successful LCU source omitted routing telemetry"),
						)?);
					}
					_ => {
						let _lane = self
							.environment
							.begin(0x4c54_4632, self.id, register.id, 0)?;
						match step {
							TransformStep::Projector {
								left,
								angle,
								response,
							} => self.layout.phase(
								&mut register.inner,
								left,
								angle,
								response,
								mask,
								value,
							)?,
							_ => self.layout.response_gates(step, mask, value, |g| {
								self.executor
									.apply(&mut register.inner, g)
									.map_err(Into::into)
							})?,
						}
						#[cfg(test)]
						if let Some(panic) = fault.take() {
							use std::io::Write as _;
							println!("LCU_TRANSFORM_POST_EMISSION_FAILURE");
							let _ = std::io::stdout().flush();
							failure_tests::inject(panic)?;
						}
					}
				}
				Ok(())
			})
		});
		self.last_apply_telemetry = Some(telemetry);
		Ok(r)
	}
}
#[cfg(test)]
mod failure_tests;
