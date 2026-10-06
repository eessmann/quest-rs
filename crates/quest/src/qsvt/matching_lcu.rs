//! Prepared weighted whole matching SELECT with explicitly replicated bounded PREP metadata.
use super::{
	Result,
	matching::{MatchingExecutionCost, PreparedMatching},
	replay_native::ReplayGateExecutor,
};
use crate::{
	Complex64, Environment, Error, MemoryBudget, QubitCount, Register, StateVector,
	environment::{Reservation, RuntimeResources},
	values::reserve_vec,
};
use quest_qsvt::{
	EncodingDescriptor, ReplayGate, ReplayKind,
	portfolio::{LcuPlan, LcuPlanLimits, LcuStep},
};

/// Independent whole-live payload, arithmetic, dispatch and application-wire ceilings.
#[derive(Debug, Clone, Copy)]
pub struct MatchingLcuLimits {
	pub plan: LcuPlanLimits,
	pub max_local_bytes: usize,
	pub node_budget: MemoryBudget,
	pub ranks_per_node: usize,
	/// Maximum-rank metadata/map/compiled-comparison plus selector compile work; prior source/child phases are separate.
	pub max_constructor_work: usize,
	pub max_rank_work: usize,
	pub max_aggregate_work: usize,
	pub max_native_dispatches: usize,
	/// Per-apply aggregate matching-router payload; excludes construction comparisons, scalar control frames and native internal MPI/protocol bytes.
	pub max_application_bytes: usize,
	/// Per-apply scalar collective-call ceiling; construction and native-internal collectives are separate.
	pub max_control_calls: usize,
}
impl Default for MatchingLcuLimits {
	fn default() -> Self {
		Self {
			plan: LcuPlanLimits::default(),
			max_local_bytes: 256 * 1024 * 1024,
			node_budget: MemoryBudget::new(256 * 1024 * 1024),
			ranks_per_node: 1,
			max_constructor_work: 1_000_000_000,
			max_rank_work: 1_000_000_000,
			max_aggregate_work: 8_000_000_000,
			max_native_dispatches: 1_000_000,
			max_application_bytes: 1_000_000_000,
			max_control_calls: 1_000_000,
		}
	}
}
/// Checked ceilings, not measured instructions/network traffic or Clifford+T counts.
#[derive(Debug, Clone, Copy, Default)]
pub struct MatchingLcuResources {
	pub maximum_rank_work: usize,
	pub aggregate_work: usize,
	pub native_dispatches: usize,
	pub application_bytes_per_rank: usize,
	pub aggregate_application_bytes: usize,
	pub control_calls: usize,
	pub native_state_elements: usize,
	pub managed_rank_peak_bytes: usize,
	pub child_native_state_payload_bytes: usize,
	pub child_native_accounted_bytes: usize,
	/// Actual wrapper/PREP/executor Rust capacities plus scalar/Arc and 16 KiB stack allowances; excludes child fixed allowances.
	pub wrapper_accounted_rust_bytes: usize,
	/// Not established by the indexed router/application payload model.
	pub native_internal_wire_bytes: Option<usize>,
}
/// Composition-only comparison traffic, separate from child preparation and repeated execution.
#[derive(Debug, Clone, Copy, Default)]
pub struct MatchingLcuPreparationCommunication {
	/// Both immutable forward and adjoint streams, including Child events.
	pub compiled_events: usize,
	/// Raw input and compiled-stream equality calls (broadcast/all-agree), including validation agreement.
	pub comparison_collective_calls: usize,
	/// Raw input and compiled equality broadcasts, counting each nonroot recipient and length words.
	/// Excludes all-agree, coordinator, MPI protocol and native-internal bytes.
	pub aggregate_comparison_broadcast_payload_bytes: usize,
	/// Conservative additional constructor scalar collective-call ceiling.
	pub other_constructor_collective_calls_upper_bound: usize,
}
const STACK_ALLOWANCE: usize = 16 * 1024;
fn add(a: usize, b: usize) -> crate::Result<usize> {
	a.checked_add(b).ok_or(Error::Overflow)
}
fn mul(a: usize, b: usize) -> crate::Result<usize> {
	a.checked_mul(b).ok_or(Error::Overflow)
}
fn checked_peak(
	resources: &RuntimeResources,
	limits: MatchingLcuLimits,
	parts: usize,
) -> crate::Result<usize> {
	if limits.ranks_per_node == 0 || limits.ranks_per_node > parts {
		return Err(Error::Value("LCU physical node placement"));
	}
	let peak = resources.allocated_bytes();
	if peak > limits.max_local_bytes {
		return Err(Error::Budget {
			requested: peak,
			available: limits.max_local_bytes,
		});
	}
	let node = mul(peak, limits.ranks_per_node)?;
	if node > limits.node_budget.bytes() {
		return Err(Error::Budget {
			requested: node,
			available: limits.node_budget.bytes(),
		});
	}
	Ok(peak)
}
const fn admit_cost(
	r: MatchingLcuResources,
	l: MatchingLcuLimits,
) -> crate::Result<MatchingLcuResources> {
	if r.maximum_rank_work > l.max_rank_work
		|| r.aggregate_work > l.max_aggregate_work
		|| r.native_dispatches > l.max_native_dispatches
		|| r.aggregate_application_bytes > l.max_application_bytes
		|| r.control_calls > l.max_control_calls
	{
		return Err(Error::Value("weighted matching execution resource budget"));
	}
	Ok(r)
}
impl MatchingLcuResources {
	fn child(&mut self, c: MatchingExecutionCost) -> crate::Result<()> {
		// Fixed telemetry rollup arithmetic; conservatively charged on CPU too.
		self.maximum_rank_work = add(self.maximum_rank_work, add(c.maximum_rank_work, 128)?)?;
		self.native_dispatches = add(self.native_dispatches, c.native_dispatches)?;
		self.application_bytes_per_rank = add(
			self.application_bytes_per_rank,
			c.application_bytes_per_rank,
		)?;
		self.aggregate_application_bytes = add(
			self.aggregate_application_bytes,
			c.aggregate_application_bytes,
		)?;
		self.control_calls = add(self.control_calls, c.routing_collective_calls)?;
		self.native_state_elements = add(self.native_state_elements, c.native_state_elements)?;
		Ok(())
	}
	fn gate(&mut self, gate: ReplayGate, width: usize, local: usize) -> crate::Result<()> {
		let zeros = usize::try_from((gate.control_mask & !gate.control_value).count_ones())
			.map_err(|_| Error::Overflow)?;
		let calls = match gate.kind {
			ReplayKind::Phase(angle) => {
				quest_compile::dispatch_recipe::scalar_phase_recipe(angle, zeros)?.native_calls()
			}
			_ => 1,
		};
		self.native_dispatches = add(self.native_dispatches, calls)?;
		self.native_state_elements = add(self.native_state_elements, mul(calls, local)?)?;
		self.maximum_rank_work = add(
			self.maximum_rank_work,
			add(mul(128, add(width, 1)?)?, mul(64, mul(calls, local)?)?)?,
		)?;
		Ok(())
	}
}
fn selector_shape(count: QubitCount, width: usize, length: usize) -> crate::Result<()> {
	if count
		.get()
		.checked_sub(width)
		.is_none_or(|available| length > available)
	{
		return Err(Error::Value("weighted matching selector width"));
	}
	Ok(())
}
fn constructor_metadata_work(
	terms: usize,
	width: usize,
	parts: usize,
	limits: MatchingLcuLimits,
) -> crate::Result<usize> {
	let work = mul(mul(8192, terms)?, add(add(width, parts)?, 16)?)?;
	if work > limits.max_constructor_work {
		return Err(Error::Value("weighted matching constructor work budget"));
	}
	Ok(work)
}
fn input_bytes<T>(
	terms: &Vec<(Complex64, T)>,
	selectors: &Vec<usize>,
	child_width: usize,
	owner_bytes: usize,
) -> crate::Result<usize> {
	add(
		add(
			add(
				mul(terms.capacity(), size_of::<(Complex64, T)>())?,
				mul(selectors.capacity(), size_of::<usize>())?,
			)?,
			mul(
				terms.len(),
				add(
					size_of::<(Complex64, EncodingDescriptor)>(),
					add(1024, mul(16, add(child_width, 4)?)?)?,
				)?,
			)?,
		)?,
		add(owner_bytes, STACK_ALLOWANCE)?,
	)
}
fn mapping(child: &[usize], selectors: &[usize], count: QubitCount) -> crate::Result<Vec<usize>> {
	let mut out = reserve_vec(add(child.len(), selectors.len())?)?;
	out.extend_from_slice(child);
	out.extend_from_slice(selectors);
	let mut mask = 0usize;
	for &target in &out {
		let bit = 1usize
			.checked_shl(u32::try_from(target).map_err(|_| Error::Overflow)?)
			.ok_or(Error::Overflow)?;
		if target >= count.get() || mask & bit != 0 {
			return Err(Error::Value("weighted matching target overlap/width"));
		}
		mask |= bit;
	}
	Ok(out)
}
/// Retains every supplied child owner and its independent native scratch. Zero-weight
/// children are validated/charged but not queried; selected indices come from the shared plan.
pub struct PreparedMatchingLcu<'env> {
	resources: &'env RuntimeResources,
	terms: Vec<(Complex64, PreparedMatching<'env>)>,
	plan: LcuPlan,
	targets: Vec<usize>,
	count: QubitCount,
	executor: ReplayGateExecutor<'env>,
	metadata: Reservation<'env>,
	plan_storage: Reservation<'env>,
	limits: MatchingLcuLimits,
	constructor_work: usize,
}
impl Environment {
	/// Prepare a complete weighted matching unitary. All children must share this
	/// environment, complete register width and physical matching targets.
	/// The constructor reserves the full declared plan byte ceiling before compiling
	/// PREP, then reconciles actual retained capacities; that envelope can reject.
	/// # Errors
	/// Rejects incompatible children, maps, weights and simultaneous live budgets.
	pub fn prepare_matching_lcu<'env>(
		&'env self,
		terms: Vec<(Complex64, PreparedMatching<'env>)>,
		selectors: Vec<usize>,
		limits: MatchingLcuLimits,
	) -> Result<PreparedMatchingLcu<'env>> {
		let first = terms
			.first()
			.ok_or(Error::Value("empty native matching LCU"))?;
		let count = first.1.num_qubits();
		let width = first.1.targets().len();
		selector_shape(count, width, selectors.len())?;
		if terms.len() > limits.plan.max_terms {
			return Err(Error::Value("weighted matching term count").into());
		}
		let metadata_work = constructor_metadata_work(terms.len(), count.get(), 1, limits)?;
		let mut metadata = self.resources.reserve(input_bytes(
			&terms,
			&selectors,
			width,
			size_of::<PreparedMatchingLcu<'_>>(),
		)?)?;
		let mut plan_storage = self.resources.reserve(limits.plan.max_bytes)?;
		checked_peak(&self.resources, limits, 1)?;
		let targets = mapping(first.1.targets(), &selectors, count)?;
		metadata.resize(add(
			metadata.bytes(),
			mul(targets.capacity(), size_of::<usize>())?,
		)?)?;
		checked_peak(&self.resources, limits, 1)?;
		let mut descriptors = reserve_vec(terms.len())?;
		for (weight, child) in &terms {
			if !std::ptr::eq(child.resources(), &raw const self.resources)
				|| child.num_qubits() != count
				|| child.targets() != first.1.targets()
			{
				return Err(Error::Value("weighted matching child owner/layout").into());
			}
			descriptors.push((
				*weight,
				EncodingDescriptor::from_matching_header(child.shard().header())?,
			));
		}
		let mut plan_limits = limits.plan;
		plan_limits.max_compile_work = plan_limits.max_compile_work.min(
			limits
				.max_constructor_work
				.checked_sub(metadata_work)
				.ok_or(Error::Overflow)?,
		);
		let plan = LcuPlan::new(descriptors, plan_limits)?;
		let constructor_work = add(metadata_work, plan.resources().compile_work)?;
		if selectors.len() != plan.selector_qubits() {
			return Err(Error::Value("weighted matching selector count").into());
		}
		drop(selectors);
		plan_storage.resize(plan.retained_bytes()?)?;
		let executor = ReplayGateExecutor::with_resources(&self.resources, count)?;
		metadata.resize(add(
			add(
				mul(
					terms.capacity(),
					size_of::<(Complex64, PreparedMatching<'_>)>(),
				)?,
				mul(targets.capacity(), size_of::<usize>())?,
			)?,
			add(size_of::<PreparedMatchingLcu<'_>>(), STACK_ALLOWANCE)?,
		)?)?;
		checked_peak(&self.resources, limits, 1)?;
		Ok(PreparedMatchingLcu {
			resources: &self.resources,
			terms,
			plan,
			targets,
			count,
			executor,
			metadata,
			plan_storage,
			limits,
			constructor_work,
		})
	}
}
impl<'env> PreparedMatchingLcu<'env> {
	pub(crate) const fn resources(&self) -> &'env RuntimeResources {
		self.resources
	}
	pub(crate) const fn num_qubits(&self) -> QubitCount {
		self.count
	}
	#[must_use]
	pub const fn plan(&self) -> &LcuPlan {
		&self.plan
	}
	/// Composition metadata/PREP only; prior child/source preparation is separately admitted.
	#[must_use]
	pub const fn composition_compile_work(&self) -> usize {
		self.constructor_work
	}
	#[must_use]
	pub fn targets(&self) -> &[usize] {
		&self.targets
	}
	/// Owned modeled payload including actual capacities and every native child guard.
	/// # Errors
	/// Rejects count overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		let base = add(
			add(self.metadata.bytes(), self.plan_storage.bytes())?,
			self.executor.retained_bytes(),
		)?;
		self.terms.iter().try_fold(base, |sum, (_, child)| {
			Ok(add(sum, child.retained_bytes()?)?)
		})
	}
	/// Complete nonmutating admission before PREP; later native failures may change state.
	/// # Errors
	/// Rejects owner, mapping, live budgets and checked work/dispatch/wire ceilings.
	pub fn admit_apply(
		&self,
		register: &Register<'_, StateVector>,
		adjoint: bool,
		mask: usize,
		value: usize,
	) -> Result<MatchingLcuResources> {
		if !std::ptr::eq(self.resources, register.resources())
			|| register.num_qubits() != self.count
		{
			return Err(Error::Value("weighted matching register owner/width").into());
		}
		let peak = checked_peak(self.resources, self.limits, 1)?;
		let mut r = MatchingLcuResources {
			managed_rank_peak_bytes: peak,
			child_native_state_payload_bytes: mul(
				self.terms.len(),
				mul(self.count.dimension(), size_of::<Complex64>())?,
			)?,
			child_native_accounted_bytes: self.terms.iter().try_fold(0, |sum, (_, child)| {
				add(sum, child.native_accounted_bytes())
			})?,
			wrapper_accounted_rust_bytes: add(
				add(self.metadata.bytes(), self.plan_storage.bytes())?,
				self.executor.retained_bytes(),
			)?,
			..MatchingLcuResources::default()
		};
		self.plan.visit_mapped_steps(
			&self.targets,
			mask,
			value,
			adjoint,
			|step| -> Result<()> {
				match step {
					LcuStep::Gate(gate) => {
						self.executor.validate(register, gate)?;
						r.gate(gate, self.count.get(), self.count.dimension())?;
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
						r.child(
							self.terms
								.get(original)
								.ok_or(Error::Value("weighted matching source index"))?
								.1
								.admit_apply(register, adjoint, control_mask, control_value)?,
						)?;
					}
				}
				Ok(())
			},
		)?;
		r.aggregate_work = r.maximum_rank_work;
		Ok(admit_cost(r, self.limits)?)
	}
	/// Apply the whole unitary or standalone adjoint. Admission failures leave state
	/// unchanged; a native error after emission leaves the state unspecified.
	/// # Errors
	/// Rejects admission, or propagates native failure after possible mutation.
	pub fn apply(
		&mut self,
		register: &mut Register<'_, StateVector>,
		adjoint: bool,
		mask: usize,
		value: usize,
	) -> Result<MatchingLcuResources> {
		let admitted = self.admit_apply(register, adjoint, mask, value)?;
		let terms = &mut self.terms;
		let executor = &mut self.executor;
		self.plan.visit_mapped_steps(
			&self.targets,
			mask,
			value,
			adjoint,
			|step| -> Result<()> {
				match step {
					LcuStep::Gate(gate) => executor.apply(register, gate)?,
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
						terms
							.get_mut(original)
							.ok_or(Error::Value("weighted matching source index"))?
							.1
							.apply(register, adjoint, control_mask, control_value)?;
					}
				}
				Ok(())
			},
		)?;
		Ok(admitted)
	}
}
#[cfg(all(feature = "mpi", quest_native_mpi))]
pub mod collective;
#[cfg(all(feature = "mpi", quest_native_mpi))]
pub mod telemetry;
