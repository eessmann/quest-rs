//! Collective weighted matching with nonnested child lanes and whole-operation admission.
use super::telemetry::RoutingTelemetry;
use super::{
	Complex64, EncodingDescriptor, Error, LcuPlan, LcuStep, MatchingLcuLimits,
	MatchingLcuPreparationCommunication, MatchingLcuResources, QubitCount, ReplayGateExecutor,
	ReplayKind, Reservation, Result, RuntimeResources, STACK_ALLOWANCE, add, admit_cost,
	checked_peak, constructor_metadata_work, input_bytes, mapping, mul, reserve_vec,
	selector_shape,
};
use crate::qsvt::matching::collective::PreparedMatching as Child;
use crate::{
	collective::{CollectiveEnvironment, CollectiveRegister, equal},
	error::BackendResult,
};
use quest_sys::mpi::MpiCollectiveLane;

fn agree<T>(lane: &mut MpiCollectiveLane<'_>, result: Result<T>) -> Result<T> {
	if !lane
		.all_agree(result.is_ok())
		.context("agreeing weighted matching admission")?
	{
		return Err(Error::Value("collective weighted matching admission").into());
	}
	result
}
fn word(n: usize) -> crate::Result<u64> {
	u64::try_from(n).map_err(|_| Error::Overflow)
}
fn frame(
	terms: &[(Complex64, Child<'_, '_, '_>)],
	selectors: &[usize],
	limits: MatchingLcuLimits,
) -> Result<Vec<u8>> {
	let mut out = reserve_vec(add(mul(terms.len(), 1024)?, mul(selectors.len(), 8)?)?)?;
	let mut push = |n: u64| out.extend_from_slice(&n.to_le_bytes());
	for n in [
		terms.len(),
		selectors.len(),
		limits.plan.max_terms,
		limits.plan.max_bytes,
		limits.plan.max_compile_work,
		limits.plan.max_primitives,
		limits.plan.preparation.max_dimension,
		limits.plan.preparation.max_bytes,
		limits.plan.preparation.max_compile_work,
		limits.plan.preparation.max_gates,
		limits.max_local_bytes,
		limits.node_budget.bytes(),
		limits.ranks_per_node,
		limits.max_constructor_work,
		limits.max_rank_work,
		limits.max_aggregate_work,
		limits.max_native_dispatches,
		limits.max_application_bytes,
		limits.max_control_calls,
	] {
		push(word(n)?);
	}
	for &n in selectors {
		push(word(n)?);
	}
	for (weight, source) in terms {
		push(weight.re.to_bits());
		push(weight.im.to_bits());
		let count = source.num_qubits();
		let targets = source.targets();
		let h = source.shard().header();
		for n in [
			count.get(),
			targets.len(),
			h.rows,
			h.cols,
			h.system_qubits,
			h.color_qubits,
			h.num_colors,
			h.record_count,
		] {
			push(word(n)?);
		}
		for n in [
			h.beta.to_bits(),
			h.alpha.to_bits(),
			h.source_identity,
			h.record_digest,
		] {
			push(n);
		}
		for &n in targets {
			push(word(n)?);
		}
	}
	Ok(out)
}
fn step_words(step: LcuStep) -> crate::Result<[u64; 6]> {
	Ok(match step {
		LcuStep::Gate(gate) => {
			let (kind, angle) = match gate.kind {
				ReplayKind::H => (0, 0),
				ReplayKind::X => (1, 0),
				ReplayKind::Ry(a) => (2, a.to_bits()),
				ReplayKind::Phase(a) => (3, a.to_bits()),
			};
			[
				kind,
				angle,
				gate.target.map_or(Ok(u64::MAX), word)?,
				word(gate.control_mask)?,
				word(gate.control_value)?,
				0,
			]
		}
		LcuStep::Child {
			index,
			adjoint,
			control_mask,
			control_value,
		} => [
			4,
			word(index)?,
			u64::from(adjoint),
			word(control_mask)?,
			word(control_value)?,
			0,
		],
	})
}
fn compiled_equal(
	lane: &mut MpiCollectiveLane<'_>,
	plan: &LcuPlan,
	targets: &[usize],
) -> Result<[usize; 2]> {
	// First validate both entire immutable streams without MPI. No rank can leave peers
	// in an event comparison after a local validation failure/panic.
	let counts =
		std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<[usize; 2]> {
			plan.descriptor().validate()?;
			let mut counts = [0usize; 2];
			for (count, adjoint) in counts.iter_mut().zip([false, true]) {
				plan.visit_mapped_steps(targets, 0, 0, adjoint, |step| -> Result<()> {
					step_words(step)?;
					*count = add(*count, 1)?;
					Ok(())
				})?;
			}
			Ok(counts)
		}))
		.unwrap_or_else(|_| {
			Err(Error::Value("weighted matching compiled validation panic").into())
		});
	let counts = agree(lane, counts)?;
	for n in [
		word(counts[0])?,
		word(counts[1])?,
		plan.descriptor().normalization.to_bits(),
		plan.preparation().norm().to_bits(),
		plan.resources().normalization_roundoff.to_bits(),
		plan.descriptor().source_identity,
		plan.descriptor().construction_identity,
	] {
		equal(lane, &n.to_le_bytes())?;
	}
	for adjoint in [false, true] {
		plan.visit_mapped_steps(targets, 0, 0, adjoint, |step| -> Result<()> {
			let words = step_words(step)?;
			let mut bytes = [0u8; 48];
			for (chunk, word) in bytes.as_chunks_mut::<8>().0.iter_mut().zip(words) {
				chunk.copy_from_slice(&word.to_le_bytes());
			}
			equal(lane, &bytes)?;
			Ok(())
		})?;
	}
	Ok(counts)
}
fn node_peak(
	lane: &mut MpiCollectiveLane<'_>,
	resources: &RuntimeResources,
	limits: MatchingLcuLimits,
	parts: usize,
) -> Result<usize> {
	let own = agree(
		lane,
		checked_peak(resources, limits, parts).map_err(Into::into),
	)?;
	let mut maximum = own;
	for peer in 0..parts {
		let mut bytes = word(own)?.to_le_bytes();
		lane.broadcast_bytes(
			i32::try_from(peer).map_err(|_| Error::Overflow)?,
			&mut bytes,
		)
		.context("admitting weighted matching node peak")?;
		maximum =
			maximum.max(usize::try_from(u64::from_le_bytes(bytes)).map_err(|_| Error::Overflow)?);
	}
	agree(
		lane,
		(|| {
			let node = mul(maximum, limits.ranks_per_node)?;
			if node > limits.node_budget.bytes() {
				return Err(Error::Budget {
					requested: node,
					available: limits.node_budget.bytes(),
				});
			}
			Ok(())
		})()
		.map_err(Into::into),
	)?;
	Ok(own)
}
fn fatal<T>(operation: impl FnOnce() -> Result<T>) -> T {
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
}
/// Owns K independently guarded local native scratch partitions and immutable child shards.
///
/// All ranks construct/admit/apply in the same order. The register and communicator
/// remain borrowed from the original collective environment.
pub struct PreparedMatchingLcu<'env, 'comm, 'runtime> {
	environment: &'env CollectiveEnvironment<'comm, 'runtime>,
	terms: Vec<(Complex64, Child<'env, 'comm, 'runtime>)>,
	plan: LcuPlan,
	targets: Vec<usize>,
	count: QubitCount,
	executor: ReplayGateExecutor<'env>,
	metadata: Reservation<'env>,
	plan_storage: Reservation<'env>,
	limits: MatchingLcuLimits,
	id: u64,
	constructor_work: usize,
	communication: MatchingLcuPreparationCommunication,
	last_apply_telemetry: Option<RoutingTelemetry>,
	#[cfg(test)]
	test_post_emission_failure: Option<bool>,
	#[cfg(test)]
	test_preflight_failure: Option<bool>,
}
impl<'comm, 'runtime> CollectiveEnvironment<'comm, 'runtime> {
	/// Collectively freeze complete child order, raw weights, layout and limits
	/// before plan compilation. The entire declared plan ceiling is reserved during
	/// compilation, then reconciled to retained capacities. Caller placement is explicit.
	/// # Errors
	/// Rejects one-rank input/owner/allocation/budget failures before native mutation.
	#[allow(
		clippy::too_many_lines,
		reason = "the ordered admission protocol keeps every reservation and common agreement visible"
	)]
	pub fn prepare_matching_lcu<'env>(
		&'env self,
		terms: Vec<(Complex64, Child<'env, 'comm, 'runtime>)>,
		selectors: Vec<usize>,
		limits: MatchingLcuLimits,
	) -> Result<PreparedMatchingLcu<'env, 'comm, 'runtime>> {
		let id = self.identifier();
		let mut lane = self.begin(0x4c43_5530, id, 0, 0)?;
		let (count, width) = agree(
			&mut lane,
			(|| -> Result<_> {
				let first = terms
					.first()
					.ok_or(Error::Value("empty native matching LCU"))?;
				if terms.len() > limits.plan.max_terms {
					return Err(Error::Value("weighted matching term count").into());
				}
				selector_shape(
					first.1.num_qubits(),
					first.1.targets().len(),
					selectors.len(),
				)?;
				Ok((first.1.num_qubits(), first.1.targets().len()))
			})(),
		)?;
		let parts = usize::try_from(self.size()?).map_err(|_| Error::Overflow)?;
		let metadata_work = agree(
			&mut lane,
			constructor_metadata_work(terms.len(), count.get(), parts, limits).map_err(Into::into),
		)?;
		let bytes = agree(
			&mut lane,
			input_bytes(
				&terms,
				&selectors,
				width,
				size_of::<PreparedMatchingLcu<'_, '_, '_>>(),
			)
			.map_err(Into::into),
		)?;
		let mut metadata = agree(&mut lane, self.resources.reserve(bytes).map_err(Into::into))?;
		let mut plan_storage = agree(
			&mut lane,
			self.resources
				.reserve(limits.plan.max_bytes)
				.map_err(Into::into),
		)?;
		let payload = agree(&mut lane, frame(&terms, &selectors, limits))?;
		let actual = agree(
			&mut lane,
			add(bytes, payload.capacity()).map_err(Into::into),
		)?;
		agree(&mut lane, metadata.resize(actual).map_err(Into::into))?;
		node_peak(&mut lane, &self.resources, limits, parts)?;
		equal(&mut lane, &payload)?;
		let raw_bytes = payload.len();
		drop(payload);
		node_peak(&mut lane, &self.resources, limits, parts)?;
		let targets = agree(
			&mut lane,
			mapping(
				terms
					.first()
					.ok_or(Error::Value("weighted matching first source"))?
					.1
					.targets(),
				&selectors,
				count,
			)
			.map_err(Into::into),
		)?;
		let actual = agree(
			&mut lane,
			(|| add(bytes, mul(targets.capacity(), size_of::<usize>())?))().map_err(Into::into),
		)?;
		agree(&mut lane, metadata.resize(actual).map_err(Into::into))?;
		node_peak(&mut lane, &self.resources, limits, parts)?;
		let descriptors = agree(
			&mut lane,
			(|| -> Result<_> {
				let first = terms
					.first()
					.ok_or(Error::Value("weighted matching first source"))?;
				let mut descriptors = reserve_vec(terms.len())?;
				for (weight, c) in &terms {
					if !std::ptr::eq(c.environment(), self)
						|| c.num_qubits() != count
						|| c.targets() != first.1.targets()
					{
						return Err(Error::Value("weighted matching child owner/layout").into());
					}
					descriptors.push((
						*weight,
						EncodingDescriptor::from_matching_header(c.shard().header())?,
					));
				}
				Ok(descriptors)
			})(),
		)?;
		let mut plan_limits = limits.plan;
		plan_limits.max_compile_work = agree(
			&mut lane,
			limits
				.max_constructor_work
				.checked_sub(metadata_work)
				.ok_or(Error::Overflow)
				.map_err(Into::into),
		)?
		.min(plan_limits.max_compile_work);
		let plan = agree(
			&mut lane,
			LcuPlan::new(descriptors, plan_limits).map_err(Into::into),
		)?;
		let constructor_work = agree(
			&mut lane,
			add(metadata_work, plan.resources().compile_work).map_err(Into::into),
		)?;
		agree(
			&mut lane,
			if selectors.len() == plan.selector_qubits() {
				Ok(())
			} else {
				Err(Error::Value("weighted matching selector count").into())
			},
		)?;
		drop(selectors);
		let counts = compiled_equal(&mut lane, &plan, &targets)?;
		let communication = agree(
			&mut lane,
			(|| -> crate::Result<_> {
				let events = add(counts[0], counts[1])?;
				Ok(MatchingLcuPreparationCommunication {
					compiled_events: events,
					comparison_collective_calls: add(
						add(mul(4, events)?, 29)?,
						add(3, raw_bytes.div_ceil(8192))?,
					)?,
					aggregate_comparison_broadcast_payload_bytes: mul(
						parts.checked_sub(1).ok_or(Error::Overflow)?,
						add(add(mul(56, events)?, 112)?, add(8, raw_bytes)?)?,
					)?,
					other_constructor_collective_calls_upper_bound: mul(
						256,
						mul(parts, add(terms.len(), 1)?)?,
					)?,
				})
			})()
			.map_err(Into::into),
		)?;
		let retained = agree(&mut lane, plan.retained_bytes().map_err(Into::into))?;
		agree(&mut lane, plan_storage.resize(retained).map_err(Into::into))?;
		let executor = agree(
			&mut lane,
			ReplayGateExecutor::with_resources(&self.resources, count).map_err(Into::into),
		)?;
		let retained = agree(
			&mut lane,
			(|| {
				add(
					add(
						mul(
							terms.capacity(),
							size_of::<(Complex64, Child<'_, '_, '_>)>(),
						)?,
						mul(targets.capacity(), size_of::<usize>())?,
					)?,
					add(
						size_of::<PreparedMatchingLcu<'_, '_, '_>>(),
						STACK_ALLOWANCE,
					)?,
				)
			})()
			.map_err(Into::into),
		)?;
		agree(&mut lane, metadata.resize(retained).map_err(Into::into))?;
		node_peak(&mut lane, &self.resources, limits, parts)?;
		Ok(PreparedMatchingLcu {
			environment: self,
			terms,
			plan,
			targets,
			count,
			executor,
			metadata,
			plan_storage,
			limits,
			id,
			constructor_work,
			communication,
			last_apply_telemetry: None,
			#[cfg(test)]
			test_post_emission_failure: None,
			#[cfg(test)]
			test_preflight_failure: None,
		})
	}
}
impl<'env, 'comm, 'runtime> PreparedMatchingLcu<'env, 'comm, 'runtime> {
	/// Local routing facts from the most recent apply attempt, only after full success.
	/// Every apply clears this before admission; standalone `admit_apply` leaves it
	/// unchanged. Constructor, PREP, native internal and MPI protocol traffic are excluded.
	#[must_use]
	pub const fn last_apply_telemetry(&self) -> Option<RoutingTelemetry> {
		self.last_apply_telemetry
	}
	pub(crate) const fn environment(&self) -> &'env CollectiveEnvironment<'comm, 'runtime> {
		self.environment
	}
	pub(crate) const fn num_qubits(&self) -> QubitCount {
		self.count
	}
	#[must_use]
	pub const fn plan(&self) -> &LcuPlan {
		&self.plan
	}
	/// Maximum-rank composition metadata/PREP work; prior source/child phases are separate.
	#[must_use]
	pub const fn composition_compile_work(&self) -> usize {
		self.constructor_work
	}
	/// Prepared comparison traffic only; excludes independent source/child stages.
	#[must_use]
	pub const fn composition_communication(&self) -> MatchingLcuPreparationCommunication {
		self.communication
	}
	#[must_use]
	pub fn targets(&self) -> &[usize] {
		&self.targets
	}
	/// Owned reservation model, with native allowances distinct from actual Rust capacities.
	/// # Errors
	/// Rejects count overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		let base = add(
			add(self.metadata.bytes(), self.plan_storage.bytes())?,
			self.executor.retained_bytes(),
		)?;
		self.terms
			.iter()
			.try_fold(base, |sum, (_, c)| Ok(add(sum, c.retained_bytes()?)?))
	}
	/// MPI-free complete validation/cost pass. Callers agree its caught result before child lanes.
	pub(crate) fn validate_apply_locally(
		&self,
		register: &CollectiveRegister<'_, '_, '_>,
		adjoint: bool,
		mask: usize,
		value: usize,
	) -> Result<MatchingLcuResources> {
		if !std::ptr::eq(register.environment, self.environment)
			|| register.num_qubits() != self.count
		{
			return Err(Error::Value("weighted matching register owner/width").into());
		}
		let parts = register.deployment().nodes();
		let peak = checked_peak(&self.environment.resources, self.limits, parts)?;
		let local = register.deployment().local_amplitudes();

		let initial = (|| -> Result<_> {
			Ok(MatchingLcuResources {
				managed_rank_peak_bytes: peak,
				child_native_state_payload_bytes: mul(
					self.terms.len(),
					mul(local, size_of::<Complex64>())?,
				)?,
				child_native_accounted_bytes: self
					.terms
					.iter()
					.try_fold(0, |sum, (_, c)| add(sum, c.native_accounted_bytes()))?,
				wrapper_accounted_rust_bytes: add(
					add(self.metadata.bytes(), self.plan_storage.bytes())?,
					self.executor.retained_bytes(),
				)?,
				// Parent agreements/frames plus child entry node/equality frames, modeled call ceiling.
				control_calls: mul(256, mul(parts, add(self.terms.len(), 1)?)?)?,
				..MatchingLcuResources::default()
			})
		})()?;
		let mut r = initial;
		self.plan.visit_mapped_steps(
			&self.targets,
			mask,
			value,
			adjoint,
			|step| -> Result<()> {
				match step {
					LcuStep::Gate(gate) => {
						self.executor.validate(&register.inner, gate)?;
						r.gate(gate, self.count.get(), local)?;
					}
					LcuStep::Child {
						index,
						control_mask,
						control_value,
						..
					} => {
						#[cfg(test)]
						if self.plan.surviving_indices().len().checked_sub(1) == Some(index)
							&& let Some(panic) = self.test_preflight_failure
						{
							failure_tests::inject(panic)?;
						}

						let original = *self
							.plan
							.surviving_indices()
							.get(index)
							.ok_or(Error::Value("weighted matching child index"))?;
						r.child(
							self.terms
								.get(original)
								.ok_or(Error::Value("weighted matching original index"))?
								.1
								.validate_apply_locally(register, control_mask, control_value)?,
						)?;
					}
				}
				Ok(())
			},
		)?;
		r.aggregate_work = mul(parts, r.maximum_rank_work)?;
		Ok(admit_cost(r, self.limits)?)
	}

	/// Complete common preflight before PREP. No child or primitive lane is entered
	/// while the parent's lane is retained.
	/// # Errors
	/// Rejects any rank's owner, mapping, live-capacity or execution budget mismatch.
	#[allow(
		clippy::too_many_lines,
		reason = "whole local dryrun must agree before the fixed child collective loop"
	)]
	pub fn admit_apply(
		&self,
		register: &CollectiveRegister<'_, '_, '_>,
		adjoint: bool,
		mask: usize,
		value: usize,
	) -> Result<MatchingLcuResources> {
		let mut lane =
			self.environment
				.begin(0x4c43_5541, self.id, register.id, u64::from(adjoint))?;
		agree(
			&mut lane,
			if std::ptr::eq(register.environment, self.environment)
				&& register.num_qubits() == self.count
			{
				Ok(())
			} else {
				Err(Error::Value("weighted matching register owner/width").into())
			},
		)?;
		let controls = agree(
			&mut lane,
			(|| Ok::<_, Error>([word(mask)?, word(value)?]))().map_err(Into::into),
		)?;
		for word in controls {
			equal(&mut lane, &word.to_le_bytes())?;
		}
		let parts = usize::try_from(self.environment.size()?).map_err(|_| Error::Overflow)?;
		node_peak(&mut lane, &self.environment.resources, self.limits, parts)?;
		let dryrun = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			self.validate_apply_locally(register, adjoint, mask, value)
		}))
		.unwrap_or_else(|_| Err(Error::Value("weighted matching local admission panic").into()));
		let admitted = agree(&mut lane, dryrun)?;
		drop(lane);
		// A fixed, common SELECT index loop. Each control computation agrees before the child.
		for (index, &original) in self.plan.surviving_indices().iter().enumerate() {
			let mut lane =
				self.environment
					.begin(0x4c43_5542, self.id, register.id, u64::from(adjoint))?;
			let controls = agree(
				&mut lane,
				(|| -> Result<_> {
					let mut child_mask = mask;
					let mut child_value = value;
					for (bit, &target) in self
						.targets
						.get(self.plan.child_width()..)
						.ok_or(Error::Value("weighted matching selector range"))?
						.iter()
						.enumerate()
					{
						let position = 1usize
							.checked_shl(u32::try_from(target).map_err(|_| Error::Overflow)?)
							.ok_or(Error::Overflow)?;
						child_mask |= position;
						if index & (1usize << bit) != 0 {
							child_value |= position;
						}
					}
					Ok((
						child_mask,
						child_value,
						self.terms
							.get(original)
							.ok_or(Error::Value("weighted matching child index"))?,
					))
				})(),
			)?;
			drop(lane);
			controls
				.2
				.1
				.admit_apply(register, adjoint, controls.0, controls.1)?;
		}
		Ok(admitted)
	}
	/// Apply U or standalone U† with arbitrary spectator/failure/label states.
	/// Admission is recoverable before emission; every error/panic after emission
	/// aborts the MPI job. Native internal wire/OS allocator overhead is unmeasured.
	/// # Errors
	/// Returns only complete preflight failures; execution failures are fatal.
	pub fn apply(
		&mut self,
		register: &mut CollectiveRegister<'_, '_, '_>,
		adjoint: bool,
		mask: usize,
		value: usize,
	) -> Result<MatchingLcuResources> {
		self.last_apply_telemetry = None;
		let admitted = self.admit_apply(register, adjoint, mask, value)?;
		let mut telemetry = RoutingTelemetry::default();
		let terms = &mut self.terms;
		let executor = &mut self.executor;
		#[cfg(test)]
		let mut fault = self.test_post_emission_failure;
		fatal(|| {
			self.plan.visit_mapped_steps(
				&self.targets,
				mask,
				value,
				adjoint,
				|step| -> Result<()> {
					match step {
						LcuStep::Gate(gate) => {
							executor.apply(&mut register.inner, gate)?;
							#[cfg(test)]
							if matches!(gate.kind,ReplayKind::Ry(angle) if angle.abs()>f64::MIN_POSITIVE)
								&& let Some(panic) = fault.take()
							{
								use std::io::Write as _;
								println!("LCU_POST_EMISSION_FAILURE");
								let _ = std::io::stdout().flush();
								failure_tests::inject(panic)?;
							}
						}
						LcuStep::Child {
							index,
							adjoint,
							control_mask,
							control_value,
						} => {
							let original = *self
								.plan
								.surviving_indices()
								.get(index)
								.ok_or(Error::Value("weighted matching child index"))?;
							let child = &mut terms
								.get_mut(original)
								.ok_or(Error::Value("weighted matching source index"))?
								.1;
							child.apply(register, adjoint, control_mask, control_value)?;
							telemetry.child(child.last_statistics());
						}
					}
					Ok(())
				},
			)
		});
		self.last_apply_telemetry = Some(telemetry);
		Ok(admitted)
	}
}

#[cfg(test)]
mod failure_tests;
