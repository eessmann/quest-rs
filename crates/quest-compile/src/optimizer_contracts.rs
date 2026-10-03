//! Immutable optimizer admission, budget, and cost contracts.
//! The native score predicts preferences; it is not a wall-clock estimate.

use crate::{
	BoundGate, BoundRegion, BoundSnapshotId, Control, ControlState, DependencyEdge, DependencyKind,
	Error, NumericalOperator, OccurrenceId, Operation, ParameterId, Program, QuantumRegion,
	QubitId, RBig, RegionPlan, RegionSnapshotId, Result, Verified,
	dispatch_recipe::{self, DispatchStep, PreparedRecipeInventory, PrimitiveGate, RecipeLimits},
};
use dashu_base::BitTest;
use dashu_int::IBig;

use std::{
	cmp::Ordering as Comparison,
	collections::{BTreeMap, BTreeSet},
	sync::{
		Arc,
		atomic::{AtomicU64, Ordering},
	},
};

/// Hard ceilings shared by every stage of one optimizer request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptimizationLimits {
	max_work: u64,
	max_bytes: u64,
}
impl Default for OptimizationLimits {
	fn default() -> Self {
		Self {
			max_work: 10_000_000,
			max_bytes: 256 * 1024 * 1024,
		}
	}
}
impl OptimizationLimits {
	/// # Errors
	/// Rejects zero limits and limits above the fixed task-wide ceilings.
	pub fn new(max_work: u64, max_bytes: u64) -> Result<Self> {
		let hard = Self::default();
		if max_work == 0 || max_bytes == 0 || max_work > hard.max_work || max_bytes > hard.max_bytes
		{
			return Err(Error::Budget("optimizer limits"));
		}
		Ok(Self {
			max_work,
			max_bytes,
		})
	}
	#[must_use]
	pub const fn max_work(self) -> u64 {
		self.max_work
	}
	#[must_use]
	pub const fn max_bytes(self) -> u64 {
		self.max_bytes
	}
}

/// Every retained allowance contributes to the same byte ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetCategory {
	Candidate,
	Verification,
	Frontier,
	Provenance,
	Worker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetUsage {
	pub work: u64,
	pub retained_bytes: u64,
}

#[derive(Debug, Default)]
struct LedgerState {
	work: AtomicU64,
	bytes: AtomicU64,
}

/// Clones share cumulative work and concurrent retained-byte admission.
#[derive(Debug, Clone)]
pub struct BudgetLedger {
	limits: OptimizationLimits,
	state: Arc<LedgerState>,
}
impl BudgetLedger {
	#[must_use]
	pub fn new(limits: OptimizationLimits) -> Self {
		Self {
			limits,
			state: Arc::new(LedgerState::default()),
		}
	}
	#[must_use]
	pub fn usage(&self) -> BudgetUsage {
		BudgetUsage {
			work: self.state.work.load(Ordering::Acquire),
			retained_bytes: self.state.bytes.load(Ordering::Acquire),
		}
	}
	/// Reserve retained bytes before work. Failed work admission releases bytes.
	/// Work remains spent after a successful reservation, even if its lease drops.
	/// # Errors
	/// Rejects either shared ceiling before publishing an allowance.
	pub fn reserve(&self, category: BudgetCategory, work: u64, bytes: u64) -> Result<BudgetLease> {
		self.state
			.bytes
			.try_update(Ordering::AcqRel, Ordering::Acquire, |old| {
				old.checked_add(bytes)
					.filter(|next| *next <= self.limits.max_bytes)
			})
			.map_err(|_| Error::Budget("optimizer retained bytes"))?;
		if self
			.state
			.work
			.try_update(Ordering::AcqRel, Ordering::Acquire, |old| {
				old.checked_add(work)
					.filter(|next| *next <= self.limits.max_work)
			})
			.is_err()
		{
			self.state.bytes.fetch_sub(bytes, Ordering::AcqRel);
			return Err(Error::Budget("optimizer work"));
		}
		Ok(BudgetLease {
			category,
			bytes,
			state: Arc::clone(&self.state),
		})
	}
	pub(crate) fn reserve_work_allowance(&self, maximum: u64) -> Result<WorkAllowance> {
		self.state
			.work
			.try_update(Ordering::AcqRel, Ordering::Acquire, |old| {
				old.checked_add(maximum)
					.filter(|next| *next <= self.limits.max_work)
			})
			.map_err(|_| Error::Budget("optimizer work"))?;
		Ok(WorkAllowance {
			reserved: maximum,
			state: Arc::clone(&self.state),
		})
	}
	pub(crate) fn remaining(&self) -> Result<(u64, u64)> {
		let usage = self.usage();
		Ok((
			self.limits
				.max_work
				.checked_sub(usage.work)
				.ok_or(Error::Budget("optimizer work"))?,
			self.limits
				.max_bytes
				.checked_sub(usage.retained_bytes)
				.ok_or(Error::Budget("optimizer retained bytes"))?,
		))
	}
}

/// Failure conservatively spends its full reservation; only successful
/// admission can refund a measured unused portion.
pub struct WorkAllowance {
	reserved: u64,
	state: Arc<LedgerState>,
}
impl WorkAllowance {
	pub(crate) fn commit(self, actual: u64) -> Result<()> {
		let refund = self
			.reserved
			.checked_sub(actual)
			.ok_or(Error::Budget("optimizer work"))?;
		self.state.work.fetch_sub(refund, Ordering::AcqRel);
		Ok(())
	}
}
/// RAII allowance for candidate, proof, frontier, provenance, or worker storage.
#[derive(Debug)]
pub struct BudgetLease {
	category: BudgetCategory,
	bytes: u64,
	state: Arc<LedgerState>,
}
impl BudgetLease {
	pub(crate) fn shrink(&mut self, bytes: u64) -> Result<()> {
		let refund = self
			.bytes
			.checked_sub(bytes)
			.ok_or(Error::Budget("optimizer lease shrink"))?;
		self.state.bytes.fetch_sub(refund, Ordering::AcqRel);
		self.bytes = bytes;
		Ok(())
	}
	#[must_use]
	pub const fn category(&self) -> BudgetCategory {
		self.category
	}
	#[must_use]
	pub const fn bytes(&self) -> u64 {
		self.bytes
	}
}
impl Drop for BudgetLease {
	fn drop(&mut self) {
		self.state.bytes.fetch_sub(self.bytes, Ordering::AcqRel);
	}
}

/// The register's actual native representation, sampled after allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentKind {
	StateVector,
	DensityMatrix,
}

/// A version-independent immutable copy of the actual register deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeploymentSnapshot {
	kind: DeploymentKind,
	width: usize,
	gpu: bool,
	distributed: bool,
	multithreaded: bool,
	rank: usize,
	nodes: usize,
	local_amplitudes: u64,
}
impl DeploymentSnapshot {
	/// # Errors
	/// Rejects impossible rank, width, or local-amplitude metadata.
	#[expect(
		clippy::too_many_arguments,
		reason = "A register snapshot must record all native deployment facts together"
	)]
	pub fn new(
		kind: DeploymentKind,
		width: usize,
		gpu: bool,
		distributed: bool,
		multithreaded: bool,
		rank: usize,
		nodes: usize,
		local_amplitudes: u64,
	) -> Result<Self> {
		if width == 0
			|| nodes == 0
			|| rank >= nodes
			|| local_amplitudes == 0
			|| (!distributed && (nodes != 1 || rank != 0))
		{
			return Err(Error::Budget("register deployment"));
		}
		let state_bits = match kind {
			DeploymentKind::StateVector => width,
			DeploymentKind::DensityMatrix => match width.checked_mul(2) {
				Some(bits) => bits,
				None => return Err(Error::Budget("register deployment")),
			},
		};
		if state_bits >= 64 {
			return Err(Error::Budget("register deployment"));
		}
		let total = 1u64 << state_bits;
		let node_count_u64 =
			u64::try_from(nodes).map_err(|_| Error::Budget("register deployment"))?;
		let Some(distributed_total) = local_amplitudes.checked_mul(node_count_u64) else {
			return Err(Error::Budget("register deployment"));
		};
		if distributed_total != total {
			return Err(Error::Budget("register deployment"));
		}
		Ok(Self {
			kind,
			width,
			gpu,
			distributed,
			multithreaded,
			rank,
			nodes,
			local_amplitudes,
		})
	}
	#[must_use]
	pub const fn kind(self) -> DeploymentKind {
		self.kind
	}
	#[must_use]
	pub const fn width(self) -> usize {
		self.width
	}
	#[must_use]
	pub const fn gpu(self) -> bool {
		self.gpu
	}
	#[must_use]
	pub const fn distributed(self) -> bool {
		self.distributed
	}
	#[must_use]
	pub const fn multithreaded(self) -> bool {
		self.multithreaded
	}
	#[must_use]
	pub const fn rank(self) -> usize {
		self.rank
	}
	#[must_use]
	pub const fn nodes(self) -> usize {
		self.nodes
	}
	#[must_use]
	pub const fn local_amplitudes(self) -> u64 {
		self.local_amplitudes
	}
}

/// Versioned scoring policy. These are exact relative scores, not timings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostProfile {
	NativeV1,
	CliffordTV1,
}

/// Communication is either counted or an ordered opaque operation signature.
/// Owned ordered operation-content trace. Its optional allowance follows clones.
#[derive(Debug)]
pub struct OpaqueTrace {
	tokens: Vec<u64>,
	_allowance: Option<BudgetLease>,
}
impl PartialEq for OpaqueTrace {
	fn eq(&self, other: &Self) -> bool {
		self.tokens == other.tokens
	}
}
impl Eq for OpaqueTrace {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunicationCost {
	Known(u64),
	Opaque(Arc<OpaqueTrace>),
}
impl CommunicationCost {
	#[must_use]
	pub const fn known(count: u64) -> Self {
		Self::Known(count)
	}
	/// # Errors
	/// An empty signature cannot distinguish changed distributed behavior.
	pub fn opaque(signature: Vec<u64>) -> Result<Self> {
		if signature.is_empty() {
			return Err(Error::Unsupported("empty communication signature"));
		}
		Ok(Self::Opaque(Arc::new(OpaqueTrace {
			tokens: signature,
			_allowance: None,
		})))
	}
	/// Build a collision-free ordered communication component from the
	/// executable operation contents, independent of occurrence/provenance IDs.
	/// This is not an MPI packet count.
	/// # Errors
	/// Rejects unsupported dynamic effects or shared work/storage exhaustion.
	pub fn from_plan(plan: &RegionPlan, ledger: &BudgetLedger) -> Result<Self> {
		const TRACE_HEADER_BYTES: u64 = 128;
		let (remaining_work, remaining_bytes) = ledger.remaining()?;
		let count_allowance = ledger.reserve_work_allowance(remaining_work)?;
		let max_tokens = remaining_bytes.saturating_sub(TRACE_HEADER_BYTES) / 16;
		let mut count = TraceWriter::Count {
			tokens: 0,
			work: 0,
			max_work: remaining_work,
			max_tokens,
		};
		write_plan_trace(plan, &mut count)?;
		let tokens = count.len();
		let TraceWriter::Count { work, .. } = count else {
			return Err(Error::Budget("communication trace state"));
		};
		count_allowance.commit(work)?;
		let bytes = tokens
			.checked_mul(16)
			.and_then(|payload| payload.checked_add(usize::try_from(TRACE_HEADER_BYTES).ok()?))
			.ok_or(Error::Budget("communication trace storage"))?;
		let allowance = ledger.reserve(
			BudgetCategory::Candidate,
			work,
			u64::try_from(bytes).map_err(|_| Error::Budget("communication trace storage"))?,
		)?;
		let mut storage = Vec::new();
		storage
			.try_reserve_exact(tokens)
			.map_err(|_| Error::Budget("communication trace allocation"))?;
		if storage
			.capacity()
			.checked_mul(std::mem::size_of::<u64>())
			.ok_or(Error::Budget("communication trace storage"))?
			> bytes
		{
			return Err(Error::Budget("communication trace storage"));
		}
		let mut writer = TraceWriter::Write(storage);
		write_plan_trace(plan, &mut writer)?;
		let TraceWriter::Write(tokens) = writer else {
			return Err(Error::Budget("communication trace state"));
		};
		Ok(Self::Opaque(Arc::new(OpaqueTrace {
			tokens,
			_allowance: Some(allowance),
		})))
	}
}

enum TraceWriter {
	Count {
		tokens: usize,
		work: u64,
		max_work: u64,
		max_tokens: u64,
	},
	Write(Vec<u64>),
}
impl TraceWriter {
	const fn len(&self) -> usize {
		match self {
			Self::Count { tokens, .. } => *tokens,
			Self::Write(tokens) => tokens.len(),
		}
	}
	fn count(&mut self, added_tokens: usize, added_work: u64) -> Result<()> {
		if let Self::Count {
			tokens,
			work,
			max_work,
			max_tokens,
		} = self
		{
			*tokens = tokens
				.checked_add(added_tokens)
				.filter(|next| u64::try_from(*next).is_ok_and(|n| n <= *max_tokens))
				.ok_or(Error::Budget("communication trace storage"))?;
			*work = work
				.checked_add(added_work)
				.filter(|next| *next <= *max_work)
				.ok_or(Error::Budget("communication trace work"))?;
		}
		Ok(())
	}
	fn visit(&mut self) -> Result<()> {
		self.count(0, 1)
	}
	fn token(&mut self, value: u64) -> Result<()> {
		match self {
			Self::Count { .. } => self.count(1, 1)?,
			Self::Write(tokens) => {
				if tokens.len() >= tokens.capacity() {
					return Err(Error::Budget("communication trace storage"));
				}
				tokens.push(value);
			}
		}
		Ok(())
	}
	fn matrix(&mut self, matrix: &NumericalOperator) -> Result<()> {
		let dim = matrix.dimension();
		let entries = if matrix.is_diagonal() {
			dim
		} else {
			dim.checked_mul(dim)
				.ok_or(Error::Budget("communication trace matrix"))?
		};
		if matches!(self, Self::Count { .. }) {
			let extra = entries
				.checked_mul(2)
				.ok_or(Error::Budget("communication trace matrix"))?;
			self.count(
				extra,
				u64::try_from(extra).map_err(|_| Error::Budget("communication trace matrix"))?,
			)?;
			return Ok(());
		}
		let view = matrix.view();
		if matrix.is_diagonal() {
			for index in 0..dim {
				let value = view[(index, index)];
				self.token(value.re.to_bits())?;
				self.token(value.im.to_bits())?;
			}
		} else {
			for col in 0..dim {
				for row in 0..dim {
					let value = view[(row, col)];
					self.token(value.re.to_bits())?;
					self.token(value.im.to_bits())?;
				}
			}
		}
		Ok(())
	}
}

fn write_plan_trace(plan: &RegionPlan, writer: &mut TraceWriter) -> Result<()> {
	writer.token(1)?; // Trace schema version.
	for instruction in plan.instructions() {
		write_operation_trace(instruction.operation(), writer, 0)?;
	}
	Ok(())
}
fn trace_targets(writer: &mut TraceWriter, targets: &[QubitId]) -> Result<()> {
	writer.token(
		u64::try_from(targets.len()).map_err(|_| Error::Budget("communication trace targets"))?,
	)?;
	for target in targets {
		writer.token(
			u64::try_from(target.index())
				.map_err(|_| Error::Budget("communication trace targets"))?,
		)?;
	}
	Ok(())
}
fn trace_controls(writer: &mut TraceWriter, controls: &[Control]) -> Result<()> {
	writer.token(
		u64::try_from(controls.len()).map_err(|_| Error::Budget("communication trace controls"))?,
	)?;
	for control in controls {
		writer.token(
			u64::try_from(control.qubit().index())
				.map_err(|_| Error::Budget("communication trace controls"))?,
		)?;
		writer.token(u64::from(control.state() == ControlState::One))?;
	}
	Ok(())
}
const fn gate_tag(gate: &BoundGate) -> u64 {
	match gate {
		BoundGate::Id => 0,
		BoundGate::H => 1,
		BoundGate::X => 2,
		BoundGate::Y => 3,
		BoundGate::Z => 4,
		BoundGate::Swap => 5,
		BoundGate::S => 6,
		BoundGate::Sdg => 7,
		BoundGate::T => 8,
		BoundGate::Tdg => 9,
		BoundGate::Sx => 10,
		BoundGate::Sxdg => 11,
		BoundGate::Rx(_) => 12,
		BoundGate::Ry(_) => 13,
		BoundGate::Rz(_) => 14,
		BoundGate::Phase(_) => 15,
		BoundGate::U { .. } => 16,
	}
}
fn write_operation_trace(op: &Operation, writer: &mut TraceWriter, depth: usize) -> Result<()> {
	writer.visit()?;
	if depth > 64 {
		return Err(Error::Budget("communication trace oracle nesting"));
	}
	match op {
		Operation::Gate {
			gate: BoundGate::Id,
			..
		}
		| Operation::Barrier { .. } => {}
		Operation::Gate {
			gate,
			targets,
			controls,
		} => {
			writer.token(2)?;
			writer.token(gate_tag(gate))?;
			trace_targets(writer, targets)?;
			trace_controls(writer, controls)?;
			for parameter in gate.parameters() {
				writer.token(parameter.to_bits())?;
			}
		}
		Operation::GlobalPhase { radians, controls } => {
			writer.token(3)?;
			writer.token(radians.to_bits())?;
			trace_controls(writer, controls)?;
		}
		Operation::Numerical {
			matrix,
			targets,
			controls,
		} => {
			writer.token(4)?;
			trace_targets(writer, targets)?;
			trace_controls(writer, controls)?;
			writer.token(
				u64::try_from(matrix.dimension())
					.map_err(|_| Error::Budget("communication trace matrix"))?,
			)?;
			writer.token(u64::from(matrix.is_diagonal()))?;
			writer.matrix(matrix)?;
		}
		Operation::Oracle {
			fragment,
			targets,
			controls,
		} => {
			writer.token(5)?;
			trace_targets(writer, targets)?;
			trace_controls(writer, controls)?;
			writer.token(u64::from(fragment.is_adjoint()))?;
			writer.token(
				u64::try_from(fragment.operations().len())
					.map_err(|_| Error::Budget("communication trace oracle"))?,
			)?;
			for child in fragment.operations() {
				write_operation_trace(
					child,
					writer,
					depth
						.checked_add(1)
						.ok_or(Error::Budget("communication trace oracle nesting"))?,
				)?;
			}
		}
		Operation::Measure { .. }
		| Operation::Reset { .. }
		| Operation::Channel { .. }
		| Operation::Conditional { .. } => {
			return Err(Error::Unsupported("dynamic communication trace"));
		}
	}
	Ok(())
}

/// Single-sided state passes and arithmetic, plus complete dispatch/coordination counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCost {
	preparation_bytes: u64,
	dispatches: u64,
	state_passes: u64,
	arithmetic: u64,
	coordination: u64,
	communication: CommunicationCost,
}
impl NativeCost {
	#[must_use]
	pub const fn new(
		preparation_bytes: u64,
		dispatches: u64,
		state_passes: u64,
		arithmetic: u64,
		coordination: u64,
		communication: CommunicationCost,
	) -> Self {
		Self {
			preparation_bytes,
			dispatches,
			state_passes,
			arithmetic,
			coordination,
			communication,
		}
	}
	#[must_use]
	pub const fn preparation_bytes(&self) -> u64 {
		self.preparation_bytes
	}
	#[must_use]
	pub const fn dispatches(&self) -> u64 {
		self.dispatches
	}
	#[must_use]
	pub const fn state_passes(&self) -> u64 {
		self.state_passes
	}
	#[must_use]
	pub const fn arithmetic(&self) -> u64 {
		self.arithmetic
	}
	#[must_use]
	pub const fn coordination(&self) -> u64 {
		self.coordination
	}
	#[must_use]
	pub const fn communication(&self) -> &CommunicationCost {
		&self.communication
	}
	/// Build the counted dispatch, payload, and single-sided pass components
	/// from the same inventory used by native preparation. Arithmetic and
	/// coordination are explicit V1 model inputs at this low-level boundary.
	/// # Errors
	/// Rejects counts not representable in the V1 score.
	pub fn from_recipe_inventory(
		inventory: PreparedRecipeInventory,
		deployment: DeploymentSnapshot,
		arithmetic: u64,
		coordination: u64,
		communication: CommunicationCost,
	) -> Result<Self> {
		if inventory.density() != (deployment.kind == DeploymentKind::DensityMatrix) {
			return Err(Error::Unsupported("recipe deployment mismatch"));
		}
		Ok(Self::new(
			u64::try_from(inventory.payload_bytes())
				.map_err(|_| Error::Budget("preparation cost"))?,
			u64::try_from(inventory.native_dispatches())
				.map_err(|_| Error::Budget("dispatch cost"))?,
			u64::try_from(inventory.logical_state_passes())
				.map_err(|_| Error::Budget("state pass cost"))?,
			arithmetic,
			coordination,
			communication,
		))
	}
	/// Assemble V1 source-visible metrics from the facade's shared recipe.
	/// B is logical forward+adjoint matrix entries; native padding, GPU mirrors,
	/// and allocator peaks are excluded. X is an explicit coordination forecast.
	/// # Errors
	/// Rejects unsupported dynamic effects, resource exhaustion, or inconsistent deployment.
	pub fn from_plan(
		plan: &RegionPlan,
		deployment: DeploymentSnapshot,
		ledger: &BudgetLedger,
		coordination: u64,
	) -> Result<Self> {
		if plan.num_qubits() != deployment.width {
			return Err(Error::Budget("optimizer target width"));
		}
		Self::from_embedded_plan(plan, deployment, ledger, coordination)
	}
	/// Score a fixed fragment embedded into the actual wider register. Wire
	/// placement is held fixed between candidates; unknown MPI traffic cannot
	/// authorize a changed trace. Local state bytes remain the actual register's.
	/// # Errors
	/// Rejects a fragment wider than the deployment or exhausted shared resources.
	pub fn from_embedded_plan(
		plan: &RegionPlan,
		deployment: DeploymentSnapshot,
		ledger: &BudgetLedger,
		coordination: u64,
	) -> Result<Self> {
		if plan.num_qubits() > deployment.width {
			return Err(Error::Budget("optimizer embedded width"));
		}
		let density = deployment.kind == DeploymentKind::DensityMatrix;
		let (remaining_work, remaining_bytes) = ledger.remaining()?;
		let memory_allowance = ledger.reserve(BudgetCategory::Verification, 0, remaining_bytes)?;
		let work_allowance = ledger.reserve_work_allowance(remaining_work)?;
		let limits = RecipeLimits::new(
			usize::try_from(remaining_work).map_err(|_| Error::Budget("recipe work"))?,
			1_000_000,
			usize::try_from(remaining_bytes).map_err(|_| Error::Budget("recipe storage"))?,
		)?;
		let inventory = PreparedRecipeInventory::from_plan_with_limits(plan, density, limits)?;
		let recipe_work =
			u64::try_from(inventory.work()).map_err(|_| Error::Budget("recipe work"))?;
		let arithmetic_limit = remaining_work
			.checked_sub(recipe_work)
			.ok_or(Error::Budget("optimizer work"))?;
		let (arithmetic, arithmetic_work) = arithmetic_forecast(
			plan,
			density,
			arithmetic_limit,
			usize::try_from(remaining_bytes).map_err(|_| Error::Budget("arithmetic storage"))?,
		)?;
		let total_work = recipe_work
			.checked_add(arithmetic_work)
			.ok_or(Error::Budget("optimizer work"))?;
		work_allowance.commit(total_work)?;
		drop(memory_allowance);
		let communication = if deployment.distributed {
			CommunicationCost::from_plan(plan, ledger)?
		} else {
			CommunicationCost::known(0)
		};
		Self::from_recipe_inventory(
			inventory,
			deployment,
			arithmetic,
			coordination,
			communication,
		)
	}
}

struct ArithmeticWalk {
	work: u64,
	max_work: u64,
	max_profile_bytes: usize,
}
impl ArithmeticWalk {
	fn charge(&mut self, amount: usize) -> Result<()> {
		let amount = u64::try_from(amount).map_err(|_| Error::Budget("arithmetic work"))?;
		self.work = self
			.work
			.checked_add(amount)
			.filter(|next| *next <= self.max_work)
			.ok_or(Error::Budget("arithmetic work"))?;
		Ok(())
	}
}
fn arithmetic_forecast(
	plan: &RegionPlan,
	density: bool,
	max_work: u64,
	max_profile_bytes: usize,
) -> Result<(u64, u64)> {
	let mut total = 0u64;
	let mut walk = ArithmeticWalk {
		work: 0,
		max_work,
		max_profile_bytes,
	};
	for instruction in plan.instructions() {
		let (amount, _) =
			arithmetic_operation(instruction.operation(), density, 0, 0, 0, &mut walk)?;
		total = total
			.checked_add(amount)
			.ok_or(Error::Budget("arithmetic cost"))?;
	}
	Ok((total, walk.work))
}
#[expect(
	clippy::too_many_lines,
	reason = "Keep the per-operation V1 heuristic beside the shared dispatch recipe interpretation"
)]
fn arithmetic_operation(
	op: &Operation,
	density: bool,
	inherited_controls: usize,
	inherited_zero: usize,
	depth: usize,
	walk: &mut ArithmeticWalk,
) -> Result<(u64, u64)> {
	walk.charge(1)?;
	if depth > 64 {
		return Err(Error::Budget("oracle arithmetic nesting"));
	}
	match op {
		Operation::Gate { gate, controls, .. } => {
			walk.charge(controls.len())?;
			let zero = inherited_zero
				.checked_add(
					controls
						.iter()
						.filter(|control| control.state() == ControlState::Zero)
						.count(),
				)
				.ok_or(Error::Budget("arithmetic controls"))?;
			let all = inherited_controls
				.checked_add(controls.len())
				.ok_or(Error::Budget("arithmetic controls"))?;
			let recipe = dispatch_recipe::gate_recipe(gate, zero)?;
			walk.charge(recipe.steps().count())?;
			let mut amount = 0u64;
			for step in recipe.steps() {
				let step_amount = match step {
					DispatchStep::Native(primitive) => match primitive {
						PrimitiveGate::Swap => 4,
						PrimitiveGate::Z | PrimitiveGate::Rz(_) => 1,
						_ => 2,
					},
					DispatchStep::PhaseGate(_) => 1u64
						.checked_add(
							u64::try_from(zero)
								.map_err(|_| Error::Budget("arithmetic controls"))?
								.checked_mul(4)
								.ok_or(Error::Budget("arithmetic controls"))?,
						)
						.ok_or(Error::Budget("arithmetic controls"))?,
					DispatchStep::ScalarPhase(_) => {
						let phase = u64::from(!density || all != 0);
						phase
							.checked_add(
								u64::try_from(zero)
									.map_err(|_| Error::Budget("arithmetic controls"))?
									.checked_mul(4)
									.ok_or(Error::Budget("arithmetic controls"))?,
							)
							.ok_or(Error::Budget("arithmetic controls"))?
					}
				};
				amount = amount
					.checked_add(step_amount)
					.ok_or(Error::Budget("arithmetic cost"))?;
			}
			Ok((amount, 1))
		}
		Operation::GlobalPhase { controls, radians } => {
			walk.charge(controls.len())?;
			let zero = inherited_zero
				.checked_add(
					controls
						.iter()
						.filter(|control| control.state() == ControlState::Zero)
						.count(),
				)
				.ok_or(Error::Budget("arithmetic controls"))?;
			let all = inherited_controls
				.checked_add(controls.len())
				.ok_or(Error::Budget("arithmetic controls"))?;
			let _ = dispatch_recipe::scalar_phase_recipe(*radians, zero)?;
			walk.charge(1)?;
			let phase = u64::from(!density || all != 0);
			Ok((
				phase
					.checked_add(
						u64::try_from(zero)
							.map_err(|_| Error::Budget("arithmetic controls"))?
							.checked_mul(4)
							.ok_or(Error::Budget("arithmetic controls"))?,
					)
					.ok_or(Error::Budget("arithmetic controls"))?,
				1,
			))
		}
		Operation::Numerical {
			matrix, controls, ..
		} => {
			walk.charge(controls.len())?;
			let all = inherited_controls
				.checked_add(controls.len())
				.ok_or(Error::Budget("arithmetic controls"))?;
			walk.charge(all)?;
			if all > walk.max_profile_bytes {
				return Err(Error::Budget("arithmetic controls"));
			}
			let mut profile = Vec::new();
			profile
				.try_reserve_exact(all)
				.map_err(|_| Error::Budget("arithmetic controls"))?;
			profile.extend(std::iter::repeat_n(true, inherited_controls));
			profile.extend(controls.iter().map(|c| c.state() == ControlState::One));
			if profile.capacity() > walk.max_profile_bytes {
				return Err(Error::Budget("arithmetic controls"));
			}
			let recipe = dispatch_recipe::MatrixRecipe::new(matrix, &profile)?;
			Ok((
				if recipe.is_diagonal() {
					1
				} else {
					u64::try_from(recipe.dimension())
						.map_err(|_| Error::Budget("arithmetic matrix"))?
				},
				1,
			))
		}
		Operation::Oracle {
			fragment, controls, ..
		} => {
			walk.charge(controls.len())?;
			let all = inherited_controls
				.checked_add(controls.len())
				.ok_or(Error::Budget("arithmetic controls"))?;
			let zero = inherited_zero
				.checked_add(
					controls
						.iter()
						.filter(|c| c.state() == ControlState::Zero)
						.count(),
				)
				.ok_or(Error::Budget("arithmetic controls"))?;
			let mut amount = 0u64;
			let mut subtree_work = 1u64;
			for child in fragment.operations() {
				let (child_amount, child_work) = arithmetic_operation(
					child,
					density,
					all,
					zero,
					depth
						.checked_add(1)
						.ok_or(Error::Budget("oracle arithmetic nesting"))?,
					walk,
				)?;
				amount = amount
					.checked_add(child_amount)
					.ok_or(Error::Budget("arithmetic cost"))?;
				subtree_work = subtree_work
					.checked_add(child_work)
					.ok_or(Error::Budget("arithmetic work"))?;
			}
			Ok((amount, subtree_work))
		}
		Operation::Barrier { .. } => Ok((0, 1)),
		Operation::Measure { .. }
		| Operation::Reset { .. }
		| Operation::Channel { .. }
		| Operation::Conditional { .. } => Err(Error::Unsupported("dynamic arithmetic forecast")),
	}
}

/// Clifford+T tuple ordered lexicographically by T, two-qubit, depth, then total count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CliffordCost {
	t: u64,
	two_qubit: u64,
	depth: u64,
	operations: u64,
}
impl CliffordCost {
	#[must_use]
	pub const fn new(t: u64, two_qubit: u64, depth: u64, operations: u64) -> Self {
		Self {
			t,
			two_qubit,
			depth,
			operations,
		}
	}
	#[must_use]
	pub const fn t_count(self) -> u64 {
		self.t
	}
	#[must_use]
	pub const fn two_qubit_count(self) -> u64 {
		self.two_qubit
	}
	#[must_use]
	pub const fn dependency_depth(self) -> u64 {
		self.depth
	}
	#[must_use]
	pub const fn total_operations(self) -> u64 {
		self.operations
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostComponents {
	native: NativeCost,
	clifford: CliffordCost,
}
impl CostComponents {
	#[must_use]
	pub const fn new(native: NativeCost, clifford: CliffordCost) -> Self {
		Self { native, clifford }
	}
	#[must_use]
	pub const fn native(&self) -> &NativeCost {
		&self.native
	}
	#[must_use]
	pub const fn clifford(&self) -> CliffordCost {
		self.clifford
	}
}

/// A score with its ordered unknown-communication component still attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeScore {
	known: RBig,
	opaque: Option<Arc<OpaqueTrace>>,
}
impl NativeScore {
	#[must_use]
	pub const fn known(&self) -> &RBig {
		&self.known
	}
	#[must_use]
	pub fn opaque(&self) -> Option<&[u64]> {
		self.opaque.as_ref().map(|trace| trace.tokens.as_slice())
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostComparison {
	Better,
	Equal,
	Worse,
	Unscorable,
}

/// Scoring target tied to actual deployment, declared reuse, and a profile version.
#[derive(Debug, Clone)]
pub struct OptimizationTarget {
	deployment: DeploymentSnapshot,
	reuse: RBig,
	profile: CostProfile,
}
impl OptimizationTarget {
	/// # Errors
	/// Reuse must be positive, normalized, and bounded before score arithmetic.
	pub fn new(deployment: DeploymentSnapshot, reuse: RBig, profile: CostProfile) -> Result<Self> {
		if reuse.numerator() <= &IBig::ZERO
			|| reuse.numerator().bit_len() > 16_384
			|| reuse.denominator().bit_len() > 16_384
		{
			return Err(Error::Budget("optimizer reuse"));
		}
		Ok(Self {
			deployment,
			reuse,
			profile,
		})
	}
	/// One requested execution is the default reuse convention.
	/// # Errors
	/// Rejects invalid deployment metadata.
	pub fn once(deployment: DeploymentSnapshot, profile: CostProfile) -> Result<Self> {
		Self::new(deployment, RBig::ONE, profile)
	}
	#[must_use]
	pub const fn deployment(&self) -> DeploymentSnapshot {
		self.deployment
	}
	#[must_use]
	pub const fn reuse(&self) -> &RBig {
		&self.reuse
	}
	#[must_use]
	pub const fn profile(&self) -> CostProfile {
		self.profile
	}
	/// Exact V1 score: B/1024 + R*[D + k*(64P+A) + X + 64C/L].
	/// A changed opaque C makes comparison unscorable.
	/// # Errors
	/// Rejects inconsistent local communication evidence.
	#[expect(
		clippy::arithmetic_side_effects,
		reason = "Arbitrary-precision exact rational score over bounded V1 operands"
	)]
	pub fn score(&self, cost: &CostComponents) -> Result<NativeScore> {
		let c = &cost.native;
		// QuEST amplitudes are Complex64; L is local state bytes, not entries.
		let l = IBig::from(self.deployment.local_amplitudes) * IBig::from(16u8);
		let k = match self.deployment.kind {
			DeploymentKind::StateVector => 1u8,
			DeploymentKind::DensityMatrix => 2u8,
		};
		let mut bracket = RBig::from(IBig::from(c.dispatches))
			+ RBig::from(
				IBig::from(k)
					* (IBig::from(64u8) * IBig::from(c.state_passes) + IBig::from(c.arithmetic)),
			)
			+ RBig::from(IBig::from(c.coordination));
		let opaque = match &c.communication {
			CommunicationCost::Known(value) => {
				if !self.deployment.distributed && *value != 0 {
					return Err(Error::Unsupported("communication on local register"));
				}
				bracket += RBig::from_parts_signed(IBig::from(64u8) * IBig::from(*value), l);
				None
			}
			CommunicationCost::Opaque(signature) => {
				if !self.deployment.distributed {
					return Err(Error::Unsupported("opaque communication on local register"));
				}
				Some(Arc::clone(signature))
			}
		};
		let preparation =
			RBig::from_parts_signed(IBig::from(c.preparation_bytes), IBig::from(1024u16));
		Ok(NativeScore {
			known: preparation + &self.reuse * bracket,
			opaque,
		})
	}
	/// Compare candidates under the selected versioned profile.
	/// # Errors
	/// Rejects invalid deployment/communication combinations.
	pub fn compare(
		&self,
		candidate: &CostComponents,
		baseline: &CostComponents,
	) -> Result<CostComparison> {
		let candidate_communication = candidate.native.communication();
		let baseline_communication = baseline.native.communication();
		if (matches!(candidate_communication, CommunicationCost::Opaque(_))
			|| matches!(baseline_communication, CommunicationCost::Opaque(_)))
			&& candidate_communication != baseline_communication
		{
			return Ok(CostComparison::Unscorable);
		}
		let ordering = match self.profile {
			CostProfile::CliffordTV1 => candidate.clifford.cmp(&baseline.clifford),
			CostProfile::NativeV1 => {
				let candidate = self.score(candidate)?;
				let baseline = self.score(baseline)?;
				candidate.known.cmp(&baseline.known)
			}
		};
		Ok(match ordering {
			Comparison::Less => CostComparison::Better,
			Comparison::Equal => CostComparison::Equal,
			Comparison::Greater => CostComparison::Worse,
		})
	}
}

/// Validated mathematical error bound; its field is private to preserve sign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApproximationBudget(RBig);
impl ApproximationBudget {
	#[must_use]
	pub const fn value(&self) -> &RBig {
		&self.0
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Approximation is opt-in and keeps local and required-global claims distinct.
pub enum ApproximationMode {
	Disabled,
	Local(ApproximationBudget),
	Global(ApproximationBudget),
}
impl ApproximationMode {
	/// # Errors
	/// Rejects a nonpositive or invalid exact bound.
	pub fn local(epsilon: RBig) -> Result<Self> {
		validate_epsilon(epsilon).map(|value| Self::Local(ApproximationBudget(value)))
	}
	/// # Errors
	/// Rejects a nonpositive or invalid exact bound.
	pub fn global(epsilon: RBig) -> Result<Self> {
		validate_epsilon(epsilon).map(|value| Self::Global(ApproximationBudget(value)))
	}
}
fn validate_epsilon(epsilon: RBig) -> Result<RBig> {
	if epsilon.numerator() <= &IBig::ZERO
		|| epsilon.numerator().bit_len() > 16_384
		|| epsilon.denominator().bit_len() > 16_384
	{
		return Err(Error::Budget("approximation bound"));
	}
	Ok(epsilon)
}

/// Immutable configuration for one consuming optimizer request.
#[derive(Debug, Clone)]
pub struct OptimizationOptions {
	target: OptimizationTarget,
	limits: OptimizationLimits,
	approximation: ApproximationMode,
}
impl OptimizationOptions {
	/// # Errors
	/// This version is infallible because all constituent fields have already
	/// passed their private constructors; program-specific checks occur at admission.
	pub const fn new(
		target: OptimizationTarget,
		limits: OptimizationLimits,
		approximation: ApproximationMode,
	) -> Result<Self> {
		Ok(Self {
			target,
			limits,
			approximation,
		})
	}
	#[must_use]
	pub const fn target(&self) -> &OptimizationTarget {
		&self.target
	}
	#[must_use]
	pub const fn limits(&self) -> OptimizationLimits {
		self.limits
	}
	#[must_use]
	pub const fn approximation(&self) -> &ApproximationMode {
		&self.approximation
	}
}

/// Which consuming admission path supplied this request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizerInputKind {
	Region,
	Bound,
	VerifiedStructured,
}

/// Typed identity of the immutable input publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizerSnapshot {
	Region(RegionSnapshotId),
	Bound(BoundSnapshotId),
	Structured(crate::language::ssa::SnapshotId),
}

/// A request's owned, already validated input. Its original source authority is retained.
#[derive(Debug, Clone)]
pub enum OptimizerInput {
	Region {
		source: Box<QuantumRegion>,
		bound: BoundRegion,
	},
	Bound(BoundRegion),
	VerifiedStructured(Program<Verified>),
}
impl OptimizerInput {
	#[must_use]
	pub const fn kind(&self) -> OptimizerInputKind {
		match self {
			Self::Region { .. } => OptimizerInputKind::Region,
			Self::Bound(_) => OptimizerInputKind::Bound,
			Self::VerifiedStructured(_) => OptimizerInputKind::VerifiedStructured,
		}
	}
	#[must_use]
	pub const fn snapshot_id(&self) -> OptimizerSnapshot {
		match self {
			Self::Region { source, .. } => OptimizerSnapshot::Region(source.snapshot_id()),
			Self::Bound(program) => OptimizerSnapshot::Bound(program.snapshot_id()),
			Self::VerifiedStructured(program) => {
				OptimizerSnapshot::Structured(program.ssa().snapshot())
			}
		}
	}
}

/// Why a deterministic request stopped. Timeout never publishes unfinished work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
	NoSearchConfigured,
	Complete,
	Exhausted,
	RoundLimit,
	CandidateLimit,
	GeneratorLimit,
	WorkerLimit,
	Unscorable,
	WorkLimit,
	StorageLimit,
	Timeout,
}

/// Evidence attached to the published result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationEvidence {
	OriginalInput,
	ExactVerified,
	LocalCertified,
	GlobalCertified,
}

/// Native numerical fusion may change rounding without an ideal certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundingStatus {
	Unchanged,
	NumericalFusionChanged,
}

/// Immutable publication of the best fully admitted result.
#[derive(Debug)]
pub struct OptimizationOutcome {
	input: OptimizerInput,
	options: OptimizationOptions,
	budget: BudgetUsage,
	stop_reason: StopReason,
	evidence: OptimizationEvidence,
	rounding: RoundingStatus,
	search_report: Option<crate::BeamReport>,
	_input_allowance: BudgetLease,
	_search_allowances: Vec<BudgetLease>,
}
impl OptimizationOutcome {
	#[must_use]
	pub const fn input_kind(&self) -> OptimizerInputKind {
		self.input.kind()
	}
	#[must_use]
	pub const fn snapshot_id(&self) -> OptimizerSnapshot {
		self.input.snapshot_id()
	}
	#[must_use]
	pub const fn stop_reason(&self) -> StopReason {
		self.stop_reason
	}
	#[must_use]
	pub const fn evidence(&self) -> OptimizationEvidence {
		self.evidence
	}
	#[must_use]
	pub const fn rounding(&self) -> RoundingStatus {
		self.rounding
	}
	#[must_use]
	pub const fn search_report(&self) -> Option<&crate::BeamReport> {
		self.search_report.as_ref()
	}
	#[must_use]
	pub const fn budget(&self) -> BudgetUsage {
		self.budget
	}
	#[must_use]
	pub const fn options(&self) -> &OptimizationOptions {
		&self.options
	}
	#[must_use]
	pub fn into_input(self) -> OptimizerInput {
		self.input
	}
}

/// Conservatively admit a candidate's mandatory ordering projection.
///
/// Deleted endpoints and contractions across mandatory paths are rejected;
/// callers may later add a proved path-preserving projection instead.
/// # Errors
/// Rejects missing typed mandatory edges, endpoint deletion, cycles, or budget exhaustion.
pub fn validate_mandatory_projection(
	source: &[DependencyEdge],
	mapping: &[(OccurrenceId, Option<OccurrenceId>)],
	candidate: &[DependencyEdge],
	ledger: &BudgetLedger,
) -> Result<()> {
	let items = source
		.len()
		.checked_add(mapping.len())
		.and_then(|n| n.checked_add(candidate.len()))
		.ok_or(Error::Budget("dependency projection"))?;
	let work = items
		.checked_mul(16)
		.ok_or(Error::Budget("dependency projection work"))?;
	let bytes = items
		.checked_mul(128)
		.ok_or(Error::Budget("dependency projection storage"))?;
	let _allowance = ledger.reserve(
		BudgetCategory::Verification,
		u64::try_from(work).map_err(|_| Error::Budget("dependency projection work"))?,
		u64::try_from(bytes).map_err(|_| Error::Budget("dependency projection storage"))?,
	)?;
	let mut mapped = BTreeMap::new();
	for (original, current) in mapping {
		if mapped.insert(*original, *current).is_some() {
			return Err(Error::InvalidId);
		}
	}
	let mut retained = BTreeSet::new();
	let mut incoming = BTreeMap::<OccurrenceId, usize>::new();
	let mut outgoing = BTreeMap::<OccurrenceId, Vec<OccurrenceId>>::new();
	for edge in candidate
		.iter()
		.filter(|edge| edge.kind != DependencyKind::Quantum)
	{
		if edge.before == edge.after {
			return Err(Error::Cycle);
		}
		retained.insert((edge.before, edge.after, dependency_rank(edge.kind)));
		incoming.entry(edge.before).or_insert(0);
		let degree = incoming.entry(edge.after).or_insert(0);
		*degree = degree
			.checked_add(1)
			.ok_or(Error::Budget("dependency projection degree"))?;
		outgoing.entry(edge.before).or_default().push(edge.after);
	}
	for edge in source
		.iter()
		.filter(|edge| edge.kind != DependencyKind::Quantum)
	{
		let Some(Some(before)) = mapped.get(&edge.before).copied() else {
			return Err(Error::Unsupported("deleted mandatory dependency"));
		};
		let Some(Some(after)) = mapped.get(&edge.after).copied() else {
			return Err(Error::Unsupported("deleted mandatory dependency"));
		};
		if before == after || !retained.contains(&(before, after, dependency_rank(edge.kind))) {
			return Err(Error::Unsupported("lost mandatory dependency"));
		}
	}
	let mut ready = incoming
		.iter()
		.filter_map(|(id, degree)| (*degree == 0).then_some(*id))
		.collect::<BTreeSet<_>>();
	let mut processed = 0usize;
	while let Some(id) = ready.pop_first() {
		processed = processed
			.checked_add(1)
			.ok_or(Error::Budget("dependency projection work"))?;
		if let Some(successors) = outgoing.get(&id) {
			for successor in successors {
				let Some(degree) = incoming.get_mut(successor) else {
					return Err(Error::InvalidId);
				};
				*degree = degree.checked_sub(1).ok_or(Error::Cycle)?;
				if *degree == 0 {
					ready.insert(*successor);
				}
			}
		}
	}
	if processed != incoming.len() {
		return Err(Error::Cycle);
	}
	Ok(())
}
const fn dependency_rank(kind: DependencyKind) -> u8 {
	match kind {
		DependencyKind::Quantum => 0,
		DependencyKind::Classical => 1,
		DependencyKind::Stochastic => 2,
		DependencyKind::Explicit => 3,
	}
}

/// Thin consuming admission. Search stages attach after their independent gates.
#[derive(Debug)]
pub struct Optimizer {
	input: OptimizerInput,
	options: OptimizationOptions,
	ledger: BudgetLedger,
	input_allowance: BudgetLease,
}
impl Optimizer {
	/// Bind the quantum region without discarding original parameter obligations.
	/// # Errors
	/// Rejects invalid bindings, deployment mismatch, or unsupported global composition.
	pub fn from_region(
		program: QuantumRegion,
		bindings: &[(ParameterId, f64)],
		options: OptimizationOptions,
	) -> Result<Self> {
		let ledger = BudgetLedger::new(options.limits);
		let allowance = reserve_input(
			&ledger,
			InputReservation {
				operations: program.schedule().len(),
				edges: program.dependency_count(),
				provenance_bytes: program.provenance().retained_bytes()?,
				bindings: bindings.len(),
				extra_bytes: program.retained_bytes()?,
				extra_work: program.binding_work_estimate()?,
				copies: 2,
			},
		)?;
		let source = Box::new(program.clone());
		let bound = program.bind(bindings)?;
		Self::admit(
			OptimizerInput::Region { source, bound },
			options,
			ledger,
			allowance,
		)
	}
	/// Admit a bound circuit while retaining its own pass APIs.
	/// # Errors
	/// Rejects deployment mismatch or unsupported global composition.
	pub fn from_bound(program: BoundRegion, options: OptimizationOptions) -> Result<Self> {
		let ledger = BudgetLedger::new(options.limits);
		let allowance = reserve_input(
			&ledger,
			InputReservation {
				operations: program.instructions().len(),
				edges: program.dependencies().len(),
				provenance_bytes: program.provenance().retained_bytes()?,
				bindings: program.binding_storage().len(),
				extra_bytes: program.retained_bytes()?,
				extra_work: 0,
				copies: 1,
			},
		)?;
		Self::admit(OptimizerInput::Bound(program), options, ledger, allowance)
	}
	/// Admit independently verified structured SSA. Its original syntax stays owned.
	/// # Errors
	/// Required-global composition is deferred until structured proof adapters exist.
	pub fn from_verified_structured(
		program: Program<Verified>,
		options: OptimizationOptions,
	) -> Result<Self> {
		let ledger = BudgetLedger::new(options.limits);
		let retained = program
			.retained_bytes()
			.map_err(|_| Error::Budget("structured input storage"))?;
		let allowance = reserve_input(
			&ledger,
			InputReservation {
				operations: 0,
				edges: 0,
				provenance_bytes: 0,
				bindings: 0,
				extra_bytes: retained,
				extra_work: 0,
				copies: 1,
			},
		)?;
		Self::admit(
			OptimizerInput::VerifiedStructured(program),
			options,
			ledger,
			allowance,
		)
	}
	fn admit(
		input: OptimizerInput,
		options: OptimizationOptions,
		ledger: BudgetLedger,
		input_allowance: BudgetLease,
	) -> Result<Self> {
		match &input {
			OptimizerInput::Region { bound: program, .. } | OptimizerInput::Bound(program) => {
				if program.num_qubits() != options.target.deployment.width {
					return Err(Error::Budget("optimizer target width"));
				}
				if program
					.instructions()
					.iter()
					.any(|instruction| matches!(instruction.operation(), Operation::Channel { .. }))
					&& options.target.deployment.kind != DeploymentKind::DensityMatrix
				{
					return Err(Error::Unsupported("channel requires density deployment"));
				}
				if matches!(options.approximation, ApproximationMode::Global(_))
					&& program.instructions().iter().any(|instruction| {
						matches!(
							instruction.operation(),
							Operation::Numerical { .. }
								| Operation::Oracle { .. }
								| Operation::Conditional { .. }
								| Operation::Measure { .. }
								| Operation::Reset { .. }
								| Operation::Channel { .. }
						)
					}) {
					return Err(Error::Unsupported("global approximation composition"));
				}
			}
			OptimizerInput::VerifiedStructured(_)
				if matches!(options.approximation, ApproximationMode::Global(_)) =>
			{
				return Err(Error::Unsupported(
					"structured global approximation composition",
				));
			}
			OptimizerInput::VerifiedStructured(program) => {
				let entry = program.ssa().program().entry;
				let width = program
					.ssa()
					.slots()
					.iter()
					.filter(|slot| slot.region == entry)
					.try_fold(0usize, |total, slot| match slot.ty {
						crate::language::ssa::Type::Qubit(count) => total
							.checked_add(count)
							.ok_or(Error::Budget("structured target width")),
						_ => Ok(total),
					})?;
				if width != options.target.deployment.width {
					return Err(Error::Budget("optimizer target width"));
				}
			}
		}
		Ok(Self {
			input,
			options,
			ledger,
			input_allowance,
		})
	}
	#[must_use]
	pub const fn input_kind(&self) -> OptimizerInputKind {
		self.input.kind()
	}
	#[must_use]
	pub const fn options(&self) -> &OptimizationOptions {
		&self.options
	}
	#[must_use]
	pub const fn ledger(&self) -> &BudgetLedger {
		&self.ledger
	}
	/// Publish the unchanged input while later search stages are unavailable.
	/// This explicit stop reason makes a foundation-only run reviewable.
	#[must_use]
	pub fn finish_without_search(self) -> OptimizationOutcome {
		OptimizationOutcome {
			input: self.input,
			options: self.options,
			budget: self.ledger.usage(),
			stop_reason: StopReason::NoSearchConfigured,
			evidence: OptimizationEvidence::OriginalInput,
			rounding: RoundingStatus::Unchanged,
			search_report: None,
			_input_allowance: self.input_allowance,
			_search_allowances: Vec::new(),
		}
	}
	/// Run deterministic bounded algebraic search, then publish the best fully admitted result.
	/// # Errors
	/// Rejects invalid search admission before publishing any candidate.
	pub fn search(
		self,
		options: crate::BeamOptions,
	) -> std::result::Result<OptimizationOutcome, crate::CompilerError> {
		let result = crate::beam::run(self.input, &self.options, &self.ledger, options)?;
		Ok(OptimizationOutcome {
			input: result.input,
			options: self.options,
			budget: self.ledger.usage(),
			stop_reason: result.stop_reason,
			evidence: result.evidence,
			rounding: result.rounding,
			search_report: Some(result.report),
			_input_allowance: self.input_allowance,
			_search_allowances: result.allowances,
		})
	}
	/// Search with optional parent-certified worker proposals under the shared budget.
	/// # Errors
	/// Rejects invalid worker limits or a failed candidate proof transactionally.
	#[cfg(feature = "workers")]
	pub fn search_with_workers(
		self,
		options: crate::BeamOptions,
		client: &quest_optimizer_client::Client,
		seed: u64,
		limits: quest_math::Limits,
	) -> std::result::Result<OptimizationOutcome, crate::CompilerError> {
		let result = crate::beam::run_with_workers(
			self.input,
			&self.options,
			&self.ledger,
			options,
			client,
			seed,
			limits,
		)?;
		Ok(OptimizationOutcome {
			input: result.input,
			options: self.options,
			budget: self.ledger.usage(),
			stop_reason: result.stop_reason,
			evidence: result.evidence,
			rounding: result.rounding,
			search_report: Some(result.report),
			_input_allowance: self.input_allowance,
			_search_allowances: result.allowances,
		})
	}
}

#[derive(Clone, Copy)]
struct InputReservation {
	operations: usize,
	edges: usize,
	provenance_bytes: usize,
	bindings: usize,
	extra_bytes: usize,
	extra_work: u64,
	copies: usize,
}
fn reserve_input(ledger: &BudgetLedger, estimate: InputReservation) -> Result<BudgetLease> {
	let InputReservation {
		operations,
		edges,
		provenance_bytes,
		bindings,
		extra_bytes,
		extra_work,
		copies,
	} = estimate;
	let per_operation = std::mem::size_of::<crate::Instruction>()
		.checked_add(std::mem::size_of::<
			quest_language::quantum::model::Occurrence,
		>())
		.and_then(|n| n.checked_add(std::mem::size_of::<[usize; 8]>()))
		.ok_or(Error::Budget("optimizer input storage"))?;
	let per_edge = std::mem::size_of::<crate::DependencyEdge>()
		.checked_mul(4)
		.ok_or(Error::Budget("optimizer input storage"))?;
	let bytes = operations
		.checked_mul(per_operation)
		.and_then(|n| n.checked_add(edges.checked_mul(per_edge)?))
		.and_then(|n| n.checked_add(provenance_bytes))
		.and_then(|n| n.checked_add(extra_bytes))
		.and_then(|n| {
			n.checked_add(bindings.checked_mul(std::mem::size_of::<(ParameterId, f64)>())?)
		})
		.and_then(|n| n.checked_mul(copies))
		.and_then(|n| n.checked_add(128))
		.ok_or(Error::Budget("optimizer input storage"))?;
	let work = operations
		.checked_mul(8)
		.and_then(|n| n.checked_add(edges.checked_mul(2)?))
		.and_then(|n| n.checked_add(bindings.checked_mul(2)?))
		.and_then(|n| n.checked_add(extra_bytes.div_ceil(64)))
		.and_then(|n| n.checked_add(1))
		.ok_or(Error::Budget("optimizer binding work"))?;
	let work = u64::try_from(work)
		.map_err(|_| Error::Budget("optimizer binding work"))?
		.checked_add(extra_work)
		.ok_or(Error::Budget("optimizer binding work"))?;
	ledger.reserve(
		BudgetCategory::Provenance,
		work,
		u64::try_from(bytes).map_err(|_| Error::Budget("optimizer input storage"))?,
	)
}
