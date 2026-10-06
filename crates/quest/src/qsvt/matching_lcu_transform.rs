//! Complete QSVT over genuinely sharded prepared matching LCU owners.
use super::{
	Result,
	matching_lcu::{MatchingLcuResources, PreparedMatchingLcu},
	replay_native::ReplayGateExecutor,
	transform_execution::TransformLayout,
};
use crate::{
	Environment, Error, MemoryBudget, Register, StateVector,
	environment::{Reservation, RuntimeResources},
};
use quest_qsvt::{
	ReplayGate, ReplayKind,
	replay_transform::{TransformSchedule, TransformStep},
};
fn add(a: usize, b: usize) -> crate::Result<usize> {
	a.checked_add(b).ok_or(Error::Overflow)
}
fn mul(a: usize, b: usize) -> crate::Result<usize> {
	a.checked_mul(b).ok_or(Error::Overflow)
}
/// Full-operation limits, additional to every original LCU and child policy.
#[derive(Debug, Clone, Copy)]
pub struct TransformExecutionLimits {
	pub max_degree: usize,
	pub max_schedule_steps: usize,
	pub max_queries: usize,
	/// Modeled local stream/metadata units plus 256*parts constructor coordination units.
	pub max_constructor_work: usize,
	/// Whole local validation work including the parts-dependent coordinator ceiling.
	pub max_preflight_work: usize,
	pub max_local_bytes: usize,
	pub node_budget: MemoryBudget,
	pub ranks_per_node: usize,
	pub max_rank_work: usize,
	pub max_aggregate_work: usize,
	pub max_native_dispatches: usize,
	pub max_application_bytes: usize,
	pub max_control_calls: usize,
}
impl Default for TransformExecutionLimits {
	fn default() -> Self {
		Self {
			max_degree: 1_000_000,
			max_schedule_steps: 4_000_005,
			max_queries: 2_000_000,
			max_constructor_work: 1_000_000_000,
			max_preflight_work: 1_000_000_000,
			max_local_bytes: 256 * 1024 * 1024,
			node_budget: MemoryBudget::new(256 * 1024 * 1024),
			ranks_per_node: 1,
			max_rank_work: 1_000_000_000,
			max_aggregate_work: 8_000_000_000,
			max_native_dispatches: 10_000_000,
			max_application_bytes: 8_000_000_000,
			max_control_calls: 10_000_000,
		}
	}
}
/// Checked whole replay envelope. Native/router payload excludes protocol/native internal wire.
#[derive(Debug, Default, Clone, Copy)]
pub struct TransformExecutionResources {
	pub semantic_steps: usize,
	pub source_queries: usize,
	pub projector_phases: usize,
	pub projector_local_amplitudes: usize,
	pub projector_read_write_calls: usize,
	pub preflight_work: usize,
	pub maximum_rank_work: usize,
	pub aggregate_work: usize,
	pub native_dispatches: usize,
	/// Whole source native state-pass elements summed over every query, per rank.
	pub source_native_state_elements_per_rank: usize,
	/// Response primitive state-pass elements; direct projector scans are reported separately.
	pub response_native_state_elements_per_rank: usize,
	pub application_bytes_per_rank: usize,
	pub aggregate_application_bytes: usize,
	pub control_calls: usize,
	pub managed_rank_peak_bytes: usize,
}
fn counts(
	schedule: &TransformSchedule,
	l: TransformExecutionLimits,
	parts: usize,
) -> Result<(usize, usize)> {
	let d = schedule.degree();
	let steps = add(mul(d, 4)?, 5)?;
	let work = add(
		mul(
			steps,
			mul(256, add(schedule.descriptor().layout.num_qubits, 1)?)?,
		)?,
		mul(256, parts)?,
	)?;
	if d > l.max_degree || steps > l.max_schedule_steps || work > l.max_constructor_work {
		return Err(Error::Value("transform constructor work/degree").into());
	}
	Ok((steps, work))
}
fn peak(
	resources: &RuntimeResources,
	l: TransformExecutionLimits,
	parts: usize,
) -> crate::Result<usize> {
	if l.ranks_per_node == 0 || l.ranks_per_node > parts {
		return Err(Error::Value("transform node placement"));
	}
	let bytes = resources.allocated_bytes();
	if bytes > l.max_local_bytes || mul(bytes, l.ranks_per_node)? > l.node_budget.bytes() {
		return Err(Error::Value("transform live byte budget"));
	}
	Ok(bytes)
}
fn sum_source(r: &mut TransformExecutionResources, c: MatchingLcuResources) -> crate::Result<()> {
	// Fixed source-receipt rollup arithmetic; conservatively charged on CPU too.
	r.maximum_rank_work = add(r.maximum_rank_work, add(c.maximum_rank_work, 128)?)?;
	r.native_dispatches = add(r.native_dispatches, c.native_dispatches)?;
	r.source_native_state_elements_per_rank = add(
		r.source_native_state_elements_per_rank,
		c.native_state_elements,
	)?;
	r.application_bytes_per_rank = add(r.application_bytes_per_rank, c.application_bytes_per_rank)?;
	r.aggregate_application_bytes =
		add(r.aggregate_application_bytes, c.aggregate_application_bytes)?;
	r.control_calls = add(r.control_calls, c.control_calls)?;
	Ok(())
}
fn response_cost(
	r: &mut TransformExecutionResources,
	gate: ReplayGate,
	width: usize,
	local: usize,
) -> crate::Result<()> {
	let calls = match gate.kind {
		ReplayKind::Phase(a) => quest_compile::dispatch_recipe::scalar_phase_recipe(
			a,
			usize::try_from((gate.control_mask & !gate.control_value).count_ones())
				.map_err(|_| Error::Overflow)?,
		)?
		.native_calls(),
		_ => 1,
	};
	r.native_dispatches = add(r.native_dispatches, calls)?;
	r.response_native_state_elements_per_rank = add(
		r.response_native_state_elements_per_rank,
		mul(calls, local)?,
	)?;
	r.maximum_rank_work = add(
		r.maximum_rank_work,
		add(mul(128, add(width, 1)?)?, mul(64, mul(calls, local)?)?)?,
	)?;
	Ok(())
}
fn pattern_floor(plan: &quest_qsvt::portfolio::LcuPlan, width: usize) -> crate::Result<usize> {
	mul(
		128,
		mul(
			add(
				plan.resources().input_terms,
				plan.resources().primitive_gates,
			)?,
			add(width, 1)?,
		)?,
	)
}
fn preflight_floor(
	schedule: &TransformSchedule,
	l: TransformExecutionLimits,
	width: usize,
	parts: usize,
	pattern: usize,
) -> Result<(usize, usize, usize)> {
	let (steps, _) = counts(schedule, l, parts)?;
	let queries = mul(schedule.degree(), 2)?;
	if queries > l.max_queries {
		return Err(Error::Value("transform query budget").into());
	}
	let control = mul(256, mul(parts, add(steps, 8)?)?)?;
	let lower = add(
		add(
			mul(steps, mul(256, add(width, 1)?)?)?,
			mul(queries.min(4), pattern)?,
		)?,
		control,
	)?;
	if lower > l.max_preflight_work
		|| control > l.max_control_calls
		|| lower > l.max_rank_work
		|| mul(parts, lower)? > l.max_aggregate_work
	{
		return Err(Error::Value("transform preflight/control work").into());
	}
	Ok((steps, lower, control))
}
#[allow(
	clippy::too_many_arguments,
	reason = "One shared whole-program dryrun receives immutable schedule/layout/policy plus two checked execution callbacks"
)]
fn dryrun(
	schedule: &TransformSchedule,
	layout: &TransformLayout,
	l: TransformExecutionLimits,
	local: usize,
	parts: usize,
	adjoint: bool,
	mask: usize,
	value: usize,
	pattern_floor: usize,
	mut pattern: impl FnMut(bool, bool) -> Result<MatchingLcuResources>,
	mut gate: impl FnMut(ReplayGate) -> Result<()>,
) -> Result<(TransformExecutionResources, [bool; 4])> {
	layout.controls(mask, value)?;
	let (steps, lower, control_calls) =
		preflight_floor(schedule, l, layout.count.get(), parts, pattern_floor)?;
	let mut used = [false; 4];
	let mut cached = [None; 4];
	let mut r = TransformExecutionResources {
		semantic_steps: steps,
		preflight_work: lower,
		control_calls,
		..Default::default()
	};
	schedule.visit_steps(adjoint, |step| -> Result<()> {
		match step {
			TransformStep::Oracle { adjoint, response } => {
				let i = add(mul(usize::from(adjoint), 2)?, usize::from(response))?;
				let slot = cached.get_mut(i).ok_or(Error::Overflow)?;
				let c = if let Some(c) = *slot {
					c
				} else {
					let c = pattern(adjoint, response)?;
					*slot = Some(c);
					c
				};
				*used.get_mut(i).ok_or(Error::Overflow)? = true;
				r.source_queries = add(r.source_queries, 1)?;
				sum_source(&mut r, c)?;
			}
			TransformStep::Projector { angle, .. } => {
				if !angle.is_finite() {
					return Err(Error::Value("transform nonfinite projector").into());
				}
				let (work, calls) = layout.phase_cost(local)?;
				r.projector_phases = add(r.projector_phases, 1)?;
				r.projector_local_amplitudes = add(r.projector_local_amplitudes, local)?;
				r.projector_read_write_calls = add(r.projector_read_write_calls, calls)?;
				r.native_dispatches = add(r.native_dispatches, calls)?;
				r.maximum_rank_work = add(r.maximum_rank_work, work)?;
			}
			_ => layout.response_gates(step, mask, value, |g| {
				gate(g)?;
				response_cost(&mut r, g, layout.count.get(), local)?;
				Ok(())
			})?,
		}
		Ok(())
	})?;
	// Both the local dryrun and fixed collective source admission validate each used pattern.
	// Charge a full source execution envelope for each pass conservatively; neither emits routing payload.
	for c in cached.into_iter().flatten() {
		r.preflight_work = add(r.preflight_work, mul(2, c.maximum_rank_work)?)?;
		r.control_calls = add(r.control_calls, c.control_calls)?;
	}
	r.maximum_rank_work = add(r.maximum_rank_work, r.preflight_work)?;
	r.aggregate_work = mul(parts, r.maximum_rank_work)?;
	if r.preflight_work > l.max_preflight_work
		|| r.maximum_rank_work > l.max_rank_work
		|| r.aggregate_work > l.max_aggregate_work
		|| r.native_dispatches > l.max_native_dispatches
		|| r.aggregate_application_bytes > l.max_application_bytes
		|| r.control_calls > l.max_control_calls
	{
		return Err(Error::Value("transform aggregate execution budget").into());
	}
	Ok((r, used))
}
/// Owns one compact schedule and K existing child scratches; no global matrix/gate/state.
pub struct PreparedMatchingLcuTransform<'env> {
	source: PreparedMatchingLcu<'env>,
	schedule: TransformSchedule,
	layout: TransformLayout,
	executor: ReplayGateExecutor<'env>,
	reservation: Reservation<'env>,
	limits: TransformExecutionLimits,
	constructor_work: usize,
}
impl Environment {
	/// Bind an exact full source descriptor and separate response target before emission.
	/// # Errors
	/// Rejects source owner/contract, target overlap, compilation and live capacities.
	pub fn prepare_matching_lcu_transform<'env>(
		&'env self,
		source: PreparedMatchingLcu<'env>,
		response: usize,
		schedule: TransformSchedule,
		limits: TransformExecutionLimits,
	) -> Result<PreparedMatchingLcuTransform<'env>> {
		if !std::ptr::eq(source.resources(), &raw const self.resources)
			|| source.plan().descriptor() != schedule.descriptor()
		{
			return Err(Error::Value("LCU transform source contract").into());
		}
		let (_, constructor_work) = counts(&schedule, limits, 1)?;
		let count = source.num_qubits();
		let planned = TransformLayout::planned_bytes(
			&schedule,
			size_of::<PreparedMatchingLcuTransform<'_>>(),
		)?;
		let mut reservation = self.resources.reserve(planned)?;
		peak(&self.resources, limits, 1)?;
		let base = planned
			.checked_sub(mul(
				schedule.descriptor().layout.num_qubits,
				size_of::<[(usize, usize); 2]>(),
			)?)
			.ok_or(Error::Overflow)?;
		let mut vectors = 0;
		let mut remaining = 2usize;
		let one_vector = mul(
			schedule.descriptor().layout.num_qubits,
			size_of::<(usize, usize)>(),
		)?;
		let layout = TransformLayout::new(
			schedule.descriptor(),
			count,
			source.targets(),
			response,
			|bytes| {
				vectors = add(vectors, bytes)?;
				remaining = remaining.checked_sub(1).ok_or(Error::Overflow)?;
				reservation.resize(add(add(base, vectors)?, mul(remaining, one_vector)?)?)?;
				peak(&self.resources, limits, 1)?;
				Ok(())
			},
		)?;
		reservation
			.resize(layout.bytes(&schedule, size_of::<PreparedMatchingLcuTransform<'_>>())?)?;
		let executor = ReplayGateExecutor::with_resources(&self.resources, count)?;
		peak(&self.resources, limits, 1)?;
		Ok(PreparedMatchingLcuTransform {
			source,
			schedule,
			layout,
			executor,
			reservation,
			limits,
			constructor_work,
		})
	}
}
impl PreparedMatchingLcuTransform<'_> {
	/// Actual weighted construction plan; child records stay in their original owners.
	#[must_use]
	pub const fn source_plan(&self) -> &quest_qsvt::portfolio::LcuPlan {
		self.source.plan()
	}
	#[must_use]
	pub const fn schedule(&self) -> &TransformSchedule {
		&self.schedule
	}
	#[must_use]
	pub const fn constructor_work(&self) -> usize {
		self.constructor_work
	}
	/// # Errors
	/// Rejects same-owner/state width, all patterns, native support and aggregate budgets before H.
	pub fn admit_apply(
		&self,
		register: &Register<'_, StateVector>,
		adjoint: bool,
		mask: usize,
		value: usize,
	) -> Result<TransformExecutionResources> {
		if !std::ptr::eq(register.resources(), self.reservation.environment)
			|| register.num_qubits() != self.layout.count
		{
			return Err(Error::Value("LCU transform register owner/width").into());
		}
		let (r, _) = dryrun(
			&self.schedule,
			&self.layout,
			self.limits,
			register.deployment().local_amplitudes(),
			1,
			adjoint,
			mask,
			value,
			pattern_floor(self.source.plan(), self.layout.count.get())?,
			|a, b| {
				let (m, v) = self.layout.query_controls(b, mask, value);
				self.source.admit_apply(register, a, m, v)
			},
			|g| self.executor.validate(register, g).map_err(Into::into),
		)?;
		Ok(TransformExecutionResources {
			managed_rank_peak_bytes: peak(self.reservation.environment, self.limits, 1)?,
			..r
		})
	}
	/// Apply full U or literal U†. A CPU native failure may leave state unspecified; no rollback.
	/// # Errors
	/// Returns preflight failures or native execution errors, never a success receipt after failure.
	pub fn apply(
		&mut self,
		register: &mut Register<'_, StateVector>,
		adjoint: bool,
		mask: usize,
		value: usize,
	) -> Result<TransformExecutionResources> {
		let r = self.admit_apply(register, adjoint, mask, value)?;
		self.schedule.visit_steps(adjoint, |step| -> Result<()> {
			match step {
				TransformStep::Oracle { adjoint, response } => {
					let (m, v) = self.layout.query_controls(response, mask, value);
					self.source.apply(register, adjoint, m, v)?;
				}
				TransformStep::Projector {
					left,
					angle,
					response,
				} => self
					.layout
					.phase(register, left, angle, response, mask, value)?,
				_ => self.layout.response_gates(step, mask, value, |g| {
					self.executor.apply(register, g).map_err(Into::into)
				})?,
			}
			Ok(())
		})?;
		Ok(r)
	}
}
#[cfg(all(feature = "mpi", quest_native_mpi))]
pub mod collective;

#[cfg(test)]
mod constructor_tests {
	#[test]
	fn source_rollup_arithmetic_is_admitted_before_execution() {
		let mut resources = super::TransformExecutionResources::default();
		let child = super::MatchingLcuResources {
			maximum_rank_work: 17,
			..Default::default()
		};
		assert!(super::sum_source(&mut resources, child).is_ok());
		assert_eq!(resources.maximum_rank_work, 145);
	}
	#[test]
	#[allow(
		clippy::panic_in_result_fn,
		reason = "Admission regression exercises the minimum constructor coordination allowance before any allocation"
	)]
	fn constructor_includes_coordination_before_allocation() -> super::Result<()> {
		use quest_qsvt::ReplayEncoding;
		let p = quest_qsvt::NumericalPolicy::default();
		let descriptor = quest_qsvt::TensorShiftEncoding::new(1, vec![], p)?.descriptor()?;
		let schedule = super::TransformSchedule::from_parts(descriptor, vec![0.1], 0., p)?;
		let limits = super::TransformExecutionLimits {
			max_constructor_work: 2560,
			..Default::default()
		};
		assert!(super::counts(&schedule, limits, 1).is_err());
		Ok(())
	}
	#[test]
	#[allow(
		clippy::panic_in_result_fn,
		reason = "No-query schedule must still admit the parts-dependent coordinator loop before callbacks"
	)]
	fn degree_zero_preflight_includes_coordination() -> super::Result<()> {
		use quest_qsvt::ReplayEncoding;
		let p = quest_qsvt::NumericalPolicy::default();
		let descriptor = quest_qsvt::TensorShiftEncoding::new(1, vec![], p)?.descriptor()?;
		let layout =
			super::TransformLayout::new(&descriptor, crate::QubitCount::new(2)?, &[0], 1, |_| {
				Ok(())
			})?;
		let schedule = super::TransformSchedule::from_parts(descriptor, vec![0.1], 0., p)?;
		let limits = super::TransformExecutionLimits {
			max_preflight_work: 3840,
			..Default::default()
		};
		assert!(
			super::dryrun(
				&schedule,
				&layout,
				limits,
				4,
				32,
				false,
				0,
				0,
				0,
				|_, _| Err(crate::Error::Value("unexpected query").into()),
				|_| Ok(())
			)
			.is_err()
		);
		Ok(())
	}
	#[test]
	#[allow(
		clippy::panic_in_result_fn,
		reason = "Cheap whole-rank/global floors must reject before entering the parts-dependent node loop"
	)]
	fn preflight_checks_overall_work_floors_before_coordination() -> super::Result<()> {
		use quest_qsvt::ReplayEncoding;
		let p = quest_qsvt::NumericalPolicy::default();
		let descriptor = quest_qsvt::TensorShiftEncoding::new(1, vec![], p)?.descriptor()?;
		let schedule = super::TransformSchedule::from_parts(descriptor, vec![0.1], 0., p)?;
		for limits in [
			super::TransformExecutionLimits {
				max_rank_work: 0,
				..Default::default()
			},
			super::TransformExecutionLimits {
				max_aggregate_work: 0,
				..Default::default()
			},
		] {
			assert!(super::preflight_floor(&schedule, limits, 2, 32, 0).is_err());
		}
		Ok(())
	}
}
