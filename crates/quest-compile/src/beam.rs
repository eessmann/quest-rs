//! Deterministic, bounded search over algebraic circuit publications.
#[cfg(feature = "workers")]
use crate::ParameterId;
use crate::Result;
use crate::{
	Angle, ApproximationMode, BoundAngleTarget, BoundGate, BoundRegion, BudgetCategory,
	BudgetLease, BudgetLedger, CliffordCost, CommunicationCost, CostComparison, CostComponents,
	CostProfile, DependencyKind, Error, ExactOptions, ExpansionLimits, Gate, Instruction,
	LinearCandidateStrategy, LinearOptions, NativeCost, Operation, OptimizationEvidence,
	OptimizationOptions, OptimizationTarget, OptimizerInput, OptimizerSnapshot, ParityOptions,
	QuantumRegion, RBig, RoundingStatus, StopReason, StructuredTerminalError, TerminalOptions,
	TerminalStatus,
};
#[cfg(feature = "workers")]
use crate::{ApproximateBeamPasses, MeetInTheMiddlePasses, RotationSynthesisPasses, ZxPasses};
use crate::{
	BoundParityPasses, CompilerError, ExactPasses, LinearPasses, ParityPasses, TerminalPasses,
};
#[cfg(feature = "workers")]
use dashu_base::BitTest;
use quest_language::quantum::model::{Occurrence, SemanticOperation};
#[cfg(feature = "workers")]
use quest_optimizer_client::MitmResult;
use std::collections::BTreeMap;
type CompilerResult<T> = std::result::Result<T, CompilerError>;
use std::sync::Arc;

/// Immutable, validated search shape. All ceilings are hard maxima per request.
#[derive(Debug, Clone, Copy)]
pub struct BeamOptions {
	width: usize,
	rounds: usize,
	candidates: usize,
	workers: usize,
}
impl Default for BeamOptions {
	fn default() -> Self {
		Self {
			width: 4,
			rounds: 8,
			candidates: 128,
			workers: 16,
		}
	}
}
impl BeamOptions {
	/// # Errors
	/// Rejects zero or above-ceiling search bounds.
	pub fn new(width: usize, rounds: usize, candidates: usize, workers: usize) -> Result<Self> {
		let hard = Self::default();
		if width == 0
			|| width > hard.width
			|| rounds == 0
			|| rounds > hard.rounds
			|| candidates == 0
			|| candidates > hard.candidates
			|| workers == 0
			|| workers > hard.workers
		{
			return Err(Error::Budget("beam options"));
		}
		Ok(Self {
			width,
			rounds,
			candidates,
			workers,
		})
	}
	#[must_use]
	pub const fn width(self) -> usize {
		self.width
	}
	#[must_use]
	pub const fn rounds(self) -> usize {
		self.rounds
	}
	#[must_use]
	pub const fn candidates(self) -> usize {
		self.candidates
	}
	#[must_use]
	pub const fn workers(self) -> usize {
		self.workers
	}
}

/// Search accounting tied to the immutable original input publication.
#[derive(Debug, Clone)]
pub struct BeamReport {
	pub(crate) input_snapshot: OptimizerSnapshot,
	pub(crate) rounds: usize,
	pub(crate) generated: usize,
	pub(crate) admitted: usize,
	pub(crate) maximum_frontier_operations: usize,
	pub(crate) unscorable_comparisons: usize,
	pub(crate) generator_budget_exhaustions: usize,
	pub(crate) structured: Option<Arc<crate::StructuredPipelineReport>>,
	#[cfg(feature = "workers")]
	pub(crate) worker_requests: usize,
	#[cfg(feature = "workers")]
	pub(crate) exact_regions: Vec<Arc<crate::ExactRegionCertificate>>,
	#[cfg(feature = "workers")]
	retained_exact_history: Option<Arc<ExactHistory>>,
	#[cfg(feature = "workers")]
	pub(crate) worker_failures: Vec<String>,
	#[cfg(feature = "workers")]
	pub(crate) worker_declines: usize,
	#[cfg(feature = "workers")]
	pub(crate) exact_mitm_statuses: Vec<BeamMitmStatus>,
	#[cfg(feature = "workers")]
	pub(crate) approx_mitm_statuses: Vec<BeamMitmStatus>,
	#[cfg(feature = "workers")]
	pub(crate) local_rotations: Vec<Arc<crate::RotationCertificate>>,
	#[cfg(feature = "workers")]
	pub(crate) approx_regions: Vec<Arc<crate::ApproxRegionCertificate>>,
	#[cfg(feature = "workers")]
	pub(crate) cumulative_error: Option<RBig>,
	#[cfg(feature = "workers")]
	retained_approx_history: Option<Arc<ApproxHistory>>,
}
impl BeamReport {
	#[must_use]
	pub const fn input_snapshot(&self) -> OptimizerSnapshot {
		self.input_snapshot
	}
	#[must_use]
	pub const fn rounds(&self) -> usize {
		self.rounds
	}
	#[must_use]
	pub const fn generated(&self) -> usize {
		self.generated
	}
	#[must_use]
	pub const fn admitted(&self) -> usize {
		self.admitted
	}
	#[must_use]
	pub const fn maximum_frontier_operations(&self) -> usize {
		self.maximum_frontier_operations
	}
	#[must_use]
	pub const fn unscorable_comparisons(&self) -> usize {
		self.unscorable_comparisons
	}
	#[must_use]
	pub const fn generator_budget_exhaustions(&self) -> usize {
		self.generator_budget_exhaustions
	}
	#[must_use]
	pub fn structured(&self) -> Option<&crate::StructuredPipelineReport> {
		self.structured.as_deref()
	}
	#[cfg(feature = "workers")]
	#[must_use]
	/// Conservatively admitted worker request slots. A failed MITM preflight
	/// may consume a slot; a successful no-window probe refunds it.
	pub const fn worker_requests(&self) -> usize {
		self.worker_requests
	}
	#[cfg(feature = "workers")]
	#[must_use]
	pub fn exact_regions(&self) -> &[Arc<crate::ExactRegionCertificate>] {
		&self.exact_regions
	}
	#[cfg(feature = "workers")]
	#[must_use]
	pub fn worker_failures(&self) -> &[String] {
		&self.worker_failures
	}
	#[cfg(feature = "workers")]
	#[must_use]
	pub const fn worker_declines(&self) -> usize {
		self.worker_declines
	}
	#[cfg(feature = "workers")]
	#[must_use]
	pub fn exact_mitm_statuses(&self) -> &[BeamMitmStatus] {
		&self.exact_mitm_statuses
	}
	#[cfg(feature = "workers")]
	#[must_use]
	pub fn approx_mitm_statuses(&self) -> &[BeamMitmStatus] {
		&self.approx_mitm_statuses
	}
	#[cfg(feature = "workers")]
	#[must_use]
	pub fn local_rotations(&self) -> &[Arc<crate::RotationCertificate>] {
		&self.local_rotations
	}
	#[cfg(feature = "workers")]
	#[must_use]
	pub fn approx_regions(&self) -> &[Arc<crate::ApproxRegionCertificate>] {
		&self.approx_regions
	}
	#[cfg(feature = "workers")]
	#[must_use]
	/// Exact sum of local allowances on the selected path. This is a
	/// whole-program bound only when the outcome is `GlobalCertified`.
	pub const fn cumulative_error(&self) -> Option<&RBig> {
		self.cumulative_error.as_ref()
	}
}

/// Exact status of one optional MITM request; certificates on the published
/// path are retained separately in the corresponding report history.
#[cfg(feature = "workers")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeamMitmStatus {
	Candidate {
		window: (usize, usize),
	},
	NoCandidate {
		explored: u64,
	},
	Incomplete {
		reason: String,
		explored: u64,
	},
	Exhausted {
		explored: u64,
	},
	Unresolved {
		precision_bits: usize,
		explored: u64,
	},
}

#[cfg(feature = "workers")]
fn mitm_status<T>(result: &MitmResult<T>, window: (usize, usize)) -> BeamMitmStatus {
	match result {
		MitmResult::Candidate(_) => BeamMitmStatus::Candidate { window },
		MitmResult::NoCandidate { explored } => BeamMitmStatus::NoCandidate {
			explored: *explored,
		},
		MitmResult::Incomplete { reason, explored } => BeamMitmStatus::Incomplete {
			reason: reason.chars().take(256).collect(),
			explored: *explored,
		},
		MitmResult::Exhausted { explored } => BeamMitmStatus::Exhausted {
			explored: *explored,
		},
		MitmResult::Unresolved {
			precision_bits,
			explored,
		} => BeamMitmStatus::Unresolved {
			precision_bits: *precision_bits,
			explored: *explored,
		},
	}
}

#[cfg(feature = "workers")]
#[derive(Clone, Copy)]
struct WorkerContext<'a> {
	client: &'a quest_optimizer_client::Client,
	seed: u64,
	limits: quest_math::Limits,
}

#[cfg(feature = "workers")]
#[derive(Debug)]
struct ExactHistory {
	parent: Option<Arc<Self>>,
	certificate: Arc<crate::ExactRegionCertificate>,
	_proof_allowance: Arc<BudgetLease>,
}
#[cfg(feature = "workers")]
#[derive(Debug)]
struct ApproxHistory {
	parent: Option<Arc<Self>>,
	certificate: ApproxEvidence,
	_proof_allowance: Arc<BudgetLease>,
}

#[cfg(feature = "workers")]
#[derive(Debug)]
enum ApproxEvidence {
	Synthesis(Arc<crate::RotationCertificate>),
	Mitm(Arc<crate::ApproxRegionCertificate>),
}

#[cfg(feature = "workers")]
fn zx_generation(
	source: &QuantumRegion,
	offset: usize,
	expanded: bool,
	worker: WorkerContext<'_>,
	seed: u64,
	max_output: usize,
) -> std::result::Result<(QuantumRegion, crate::ZxReport), crate::WorkerError> {
	if expanded {
		source.clone().zx_expanded_candidate_from(
			offset,
			worker.client,
			seed,
			worker.limits,
			1,
			max_output,
		)
	} else {
		source
			.clone()
			.zx_candidate_from(offset, worker.client, seed, worker.limits, 1, max_output)
	}
}

#[cfg(feature = "workers")]
fn rotation_count(source: &QuantumRegion) -> usize {
	source
		.occurrences()
		.iter()
		.filter(|item| {
			matches!(
				&item.operation,
				SemanticOperation::Gate {
					gate: Gate::Rx(_) | Gate::Ry(_) | Gate::Rz(_),
					..
				}
			)
		})
		.count()
}

#[cfg(feature = "workers")]
fn exact_mitm_hint(source: &QuantumRegion) -> bool {
	source.occurrences().iter().any(|item| {
		matches!(
			&item.operation,
			SemanticOperation::Gate {
				gate: Gate::H
					| Gate::X
					| Gate::Y
					| Gate::Z
					| Gate::S
					| Gate::Sdg
					| Gate::T
					| Gate::Tdg
					| Gate::Swap
					| Gate::Phase(_),
				..
			} | SemanticOperation::GlobalPhase { .. }
		)
	})
}

#[cfg(feature = "workers")]
fn admitted_epsilon(
	budget: &RBig,
	spent: &RBig,
	count: usize,
	limits: quest_math::Limits,
) -> Result<Option<(f64, RBig)>> {
	if count == 0 {
		return Ok(None);
	}
	let remaining = std::ops::Sub::sub(budget.clone(), spent.clone());
	if remaining <= RBig::ZERO {
		return Ok(None);
	}
	let per = std::ops::Div::div(remaining, RBig::from(dashu_int::IBig::from(count)));
	// numerator >= 2^(nbits-1), denominator < 2^dbits; this power of two
	// lies strictly below the rational allowance without floating conversion.
	let exponent = i64::try_from(per.numerator().bit_len())
		.and_then(|numerator| {
			i64::try_from(per.denominator().bit_len())
				.map(|denominator| numerator.saturating_sub(denominator).saturating_sub(2))
		})
		.map_err(|_| Error::Budget("beam approximation precision"))?;
	let bits = if exponent >= -1 {
		0.5f64.to_bits()
	} else if exponent < -1074 {
		return Ok(None);
	} else if exponent >= -1022 {
		u64::try_from(
			exponent
				.checked_add(1023)
				.ok_or(Error::Budget("beam approximation precision"))?,
		)
		.map_err(|_| Error::Budget("beam approximation precision"))?
		.checked_shl(52)
		.ok_or(Error::Budget("beam approximation precision"))?
	} else {
		1u64.checked_shl(
			u32::try_from(
				exponent
					.checked_add(1074)
					.ok_or(Error::Budget("beam approximation precision"))?,
			)
			.map_err(|_| Error::Budget("beam approximation precision"))?,
		)
		.ok_or(Error::Budget("beam approximation precision"))?
	};
	let epsilon = f64::from_bits(bits);
	let exact = quest_math::dyadic_from_bits(bits, limits)
		.map_err(|_| Error::Budget("beam approximation precision"))?;
	if exact > per {
		return Err(Error::Budget("beam approximation rounding"));
	}
	Ok(Some((epsilon, exact)))
}

pub struct BeamRun {
	pub(crate) input: OptimizerInput,
	pub(crate) stop_reason: StopReason,
	pub(crate) evidence: OptimizationEvidence,
	pub(crate) rounding: RoundingStatus,
	pub(crate) report: BeamReport,
	pub(crate) allowances: Vec<BudgetLease>,
}

fn clifford_cost(bound: &BoundRegion, ledger: &BudgetLedger) -> Result<CostComponents> {
	let count = bound
		.instructions()
		.len()
		.checked_add(bound.dependencies().len())
		.and_then(|n| n.checked_mul(64))
		.ok_or(Error::Budget("beam cost work"))?;
	let _work = ledger.reserve(
		BudgetCategory::Verification,
		u64::try_from(count).map_err(|_| Error::Budget("beam cost work"))?,
		0,
	)?;
	let mut t = 0u64;
	let mut two = 0u64;
	for instruction in bound.instructions() {
		if let Operation::Gate {
			gate,
			targets,
			controls,
		} = instruction.operation()
		{
			if matches!(gate, BoundGate::T | BoundGate::Tdg) {
				t = t.checked_add(1).ok_or(Error::Budget("beam cost"))?;
			}
			if targets
				.len()
				.checked_add(controls.len())
				.ok_or(Error::Budget("beam cost"))?
				>= 2
			{
				two = two.checked_add(1).ok_or(Error::Budget("beam cost"))?;
			}
		}
	}
	let operations =
		u64::try_from(bound.instructions().len()).map_err(|_| Error::Budget("beam cost"))?;
	let depth = u64::try_from(bound.dependency_depth()).map_err(|_| Error::Budget("beam cost"))?;
	let native = NativeCost::new(0, 0, 0, 0, 0, CommunicationCost::known(0));
	Ok(CostComponents::new(
		native,
		CliffordCost::new(t, two, depth, operations),
	))
}

pub fn score_bound(
	bound: &BoundRegion,
	target: &OptimizationTarget,
	ledger: &BudgetLedger,
) -> Result<CostComponents> {
	if target.profile() == CostProfile::CliffordTV1 {
		return clifford_cost(bound, ledger);
	}
	let retained = bound.retained_bytes()?;
	let _copy = ledger.reserve(
		BudgetCategory::Candidate,
		0,
		u64::try_from(retained).map_err(|_| Error::Budget("beam score storage"))?,
	)?;
	let setup = bound
		.instructions()
		.len()
		.checked_add(bound.dependencies().len())
		.and_then(|n| n.checked_mul(64))
		.ok_or(Error::Budget("beam score work"))?;
	let _setup = ledger.reserve(
		BudgetCategory::Verification,
		u64::try_from(setup).map_err(|_| Error::Budget("beam score work"))?,
		0,
	)?;
	let plan = bound.clone().plan()?;
	let native = NativeCost::from_embedded_plan(&plan, target.deployment(), ledger, 0)?;
	Ok(CostComponents::new(native, CliffordCost::new(0, 0, 0, 0)))
}

fn precedes(
	a: &CostComponents,
	b: &CostComponents,
	target: &OptimizationTarget,
	ledger: &BudgetLedger,
	unscorable: &mut usize,
) -> Result<bool> {
	let _work = ledger.reserve(BudgetCategory::Verification, 4096, 0)?;
	match target.compare(a, b)? {
		CostComparison::Better => Ok(true),
		CostComparison::Unscorable => {
			*unscorable = unscorable
				.checked_add(1)
				.ok_or(Error::Budget("beam comparisons"))?;
			Ok(false)
		}
		CostComparison::Equal | CostComparison::Worse => Ok(false),
	}
}

fn source_angle(target: Option<&BoundAngleTarget>, radians: f64) -> Result<Angle> {
	Ok(match target {
		Some(BoundAngleTarget::DyadicRadians { bits }) => Angle::radians(f64::from_bits(*bits)),
		Some(BoundAngleTarget::RationalPi {
			numerator,
			denominator,
		}) => Angle::rational_pi(RBig::from_parts_signed(
			numerator.clone(),
			denominator.clone(),
		)),
		Some(BoundAngleTarget::AffinePi {
			radians_numerator,
			radians_denominator,
			pi_numerator,
			pi_denominator,
		}) => Angle::affine(
			RBig::from_parts_signed(radians_numerator.clone(), radians_denominator.clone()),
			RBig::from_parts_signed(pi_numerator.clone(), pi_denominator.clone()),
		),
		None => Angle::radians(radians),
	}?)
}

fn source_gate(gate: &BoundGate, targets: &[Option<BoundAngleTarget>]) -> Result<Gate> {
	let at = |index| targets.get(index).and_then(Option::as_ref);
	Ok(match gate {
		BoundGate::Rx(value) => Gate::Rx(source_angle(at(0), *value)?),
		BoundGate::Ry(value) => Gate::Ry(source_angle(at(0), *value)?),
		BoundGate::Rz(value) => Gate::Rz(source_angle(at(0), *value)?),
		BoundGate::Phase(value) => Gate::Phase(source_angle(at(0), *value)?),
		BoundGate::U { theta, phi, lambda } => Gate::U {
			theta: source_angle(at(0), *theta)?,
			phi: source_angle(at(1), *phi)?,
			lambda: source_angle(at(2), *lambda)?,
		},
		_ => Gate::from_bound(gate)?,
	})
}

fn source_operation(instruction: &Instruction) -> Result<Option<SemanticOperation>> {
	let targets = instruction.angle_targets();
	Ok(Some(match instruction.operation() {
		Operation::Gate {
			gate,
			targets: wires,
			controls,
		} => SemanticOperation::Gate {
			gate: source_gate(gate, targets)?,
			targets: Arc::clone(wires),
			controls: Arc::clone(controls),
		},
		Operation::GlobalPhase { radians, controls } => SemanticOperation::GlobalPhase {
			angle: source_angle(targets.first().and_then(Option::as_ref), *radians)?,
			controls: Arc::clone(controls),
		},
		Operation::Numerical {
			matrix,
			targets,
			controls,
		} => SemanticOperation::Numerical {
			matrix: matrix.clone(),
			targets: Arc::clone(targets),
			controls: Arc::clone(controls),
		},
		Operation::Oracle {
			fragment,
			targets,
			controls,
		} => SemanticOperation::Oracle {
			fragment: fragment.clone(),
			targets: Arc::clone(targets),
			controls: Arc::clone(controls),
		},
		Operation::Measure { qubit, bit } => SemanticOperation::Measure {
			qubit: *qubit,
			bit: *bit,
		},
		Operation::Reset { qubit } => SemanticOperation::Reset { qubit: *qubit },
		Operation::Barrier { qubits } => SemanticOperation::Barrier {
			qubits: Arc::clone(qubits),
		},
		Operation::Channel { kraus, targets } => SemanticOperation::Channel {
			kraus: Arc::clone(kraus),
			targets: Arc::clone(targets),
		},
		Operation::Conditional { .. } => return Ok(None),
	}))
}

fn bound_as_ideal(bound: &BoundRegion) -> Result<Option<QuantumRegion>> {
	let Some(owner) = bound.instructions().iter().find_map(|instruction| {
		let operands = instruction.operation().operands();
		operands
			.targets()
			.first()
			.map(|q| q.owner)
			.or_else(|| operands.controls().first().map(|c| c.qubit().owner))
	}) else {
		return Ok(None);
	};
	let mut occurrences = Vec::new();
	occurrences
		.try_reserve_exact(bound.instructions().len())
		.map_err(|_| Error::Budget("beam bound adapter storage"))?;
	for instruction in bound.instructions() {
		let Some(operation) = source_operation(instruction)? else {
			return Ok(None);
		};
		occurrences.push(Occurrence {
			id: instruction.id(),
			provenance: instruction.provenance(),
			source: instruction.source().cloned(),
			operation,
		});
	}
	let explicit = bound
		.dependencies()
		.iter()
		.filter(|edge| edge.kind == DependencyKind::Explicit)
		.map(|edge| (edge.before, edge.after))
		.collect();
	let ideal = QuantumRegion::from_parts(
		owner,
		bound.num_qubits(),
		bound.num_bits(),
		Vec::new(),
		occurrences,
		explicit,
		bound.limits(),
		Arc::clone(bound.provenance_arc()),
	)?;
	if ideal.dependencies() != bound.dependencies() {
		return Ok(None);
	}
	Ok(Some(ideal))
}

fn reserve_candidate(program: &QuantumRegion, ledger: &BudgetLedger) -> Result<BudgetLease> {
	let bytes = program
		.retained_bytes()?
		.checked_mul(4)
		.and_then(|amount| amount.checked_add(1024 * 1024))
		.ok_or(Error::Budget("beam candidate storage"))?;
	ledger.reserve(
		BudgetCategory::Candidate,
		0,
		u64::try_from(bytes).map_err(|_| Error::Budget("beam candidate storage"))?,
	)
}

#[cfg(feature = "workers")]
fn bind_candidate(
	candidate: &QuantumRegion,
	pairs: &[(ParameterId, f64)],
	parent: &BoundRegion,
	ledger: &BudgetLedger,
) -> Result<BoundRegion> {
	let binding_work = candidate.binding_work_estimate()?;
	let binding_allowance = ledger.reserve_work_allowance(binding_work)?;
	let mut bound = candidate.clone().bind(pairs)?;
	binding_allowance.commit(binding_work)?;
	bound.retain_source(
		parent.source_snapshot_id(),
		parent.binding_storage().clone(),
	)?;
	Ok(bound)
}

#[cfg(feature = "workers")]
fn budget_stop(reason: &str) -> StopReason {
	if reason.contains("byte") || reason.contains("storage") || reason.contains("allocation") {
		StopReason::StorageLimit
	} else {
		StopReason::WorkLimit
	}
}

fn structured_error(error: StructuredTerminalError) -> CompilerError {
	match error {
		StructuredTerminalError::Circuit(error) => error.into(),
		error => CompilerError::Structured(error),
	}
}

struct Candidate {
	source: Box<QuantumRegion>,
	bound: BoundRegion,
	score: CostComponents,
	terminal_bound: Option<BoundRegion>,
	rounding: RoundingStatus,
	allowances: Vec<BudgetLease>,
	expanded: bool,
	bound_only: bool,
	#[cfg(feature = "workers")]
	exact_history: Option<Arc<ExactHistory>>,
	#[cfg(feature = "workers")]
	approx_history: Option<Arc<ApproxHistory>>,
	#[cfg(feature = "workers")]
	error_bound: Option<RBig>,
}

#[cfg(feature = "workers")]
fn same_history<T>(left: Option<&Arc<T>>, right: Option<&Arc<T>>) -> bool {
	match (left, right) {
		(None, None) => true,
		(Some(left), Some(right)) => Arc::ptr_eq(left, right),
		_ => false,
	}
}

#[cfg(feature = "workers")]
fn same_evidence(left: &Candidate, right: &Candidate) -> bool {
	same_history(left.exact_history.as_ref(), right.exact_history.as_ref())
		&& same_history(left.approx_history.as_ref(), right.approx_history.as_ref())
		&& left.error_bound == right.error_bound
}

#[cfg(not(feature = "workers"))]
const fn same_evidence(_left: &Candidate, _right: &Candidate) -> bool {
	true
}

type ScoredPublication = (
	CostComponents,
	Option<BoundRegion>,
	RoundingStatus,
	Vec<BudgetLease>,
	TerminalStatus,
);

fn score_candidate(
	bound: &BoundRegion,
	config: &OptimizationOptions,
	ledger: &BudgetLedger,
) -> Result<ScoredPublication> {
	if config.target().profile() != CostProfile::NativeV1
		|| matches!(config.approximation(), ApproximationMode::Global(_))
	{
		return Ok((
			score_bound(bound, config.target(), ledger)?,
			None,
			RoundingStatus::Unchanged,
			Vec::new(),
			TerminalStatus::Complete,
		));
	}
	let retained = bound.retained_bytes()?;
	let _copy = ledger.reserve(
		BudgetCategory::Candidate,
		0,
		u64::try_from(retained).map_err(|_| Error::Budget("beam terminal storage"))?,
	)?;
	let terminal =
		bound
			.clone()
			.schedule_and_fuse(TerminalOptions::default(), config.target(), ledger)?;
	if terminal.report().status() == TerminalStatus::Complete
		&& let Some(score) = terminal.report().published_cost().cloned()
	{
		let rounding = if terminal.report().rounding_changed() {
			RoundingStatus::NumericalFusionChanged
		} else {
			RoundingStatus::Unchanged
		};
		let (program, allowances) = terminal.into_parts();
		return Ok((
			score,
			Some(program),
			rounding,
			allowances,
			TerminalStatus::Complete,
		));
	}
	let status = terminal.report().status();
	Ok((
		score_bound(bound, config.target(), ledger)?,
		None,
		RoundingStatus::Unchanged,
		Vec::new(),
		status,
	))
}

#[derive(Clone, Copy)]
enum Generator {
	Exact,
	Gaussian,
	Pmh,
	Parity(usize),
}

fn generation(
	source: &QuantumRegion,
	generator: Generator,
	maximum_work: usize,
	maximum_output: usize,
) -> Result<(QuantumRegion, usize, bool)> {
	match generator {
		Generator::Exact => {
			let (candidate, report) = source.clone().optimize_exact_with_options(ExactOptions {
				max_work: maximum_work.min(4_000_000),
				max_bytes: 64 * 1024 * 1024,
			})?;
			Ok((candidate, report.work, !report.rewrites.is_empty()))
		}
		Generator::Gaussian | Generator::Pmh => {
			let strategy = if matches!(generator, Generator::Gaussian) {
				LinearCandidateStrategy::Gaussian
			} else {
				LinearCandidateStrategy::Pmh
			};
			let options = LinearOptions {
				max_work: maximum_work.min(1_000_000),
				..LinearOptions::default()
			};
			let (candidate, report) =
				source
					.clone()
					.resynthesize_linear_candidate(options, strategy, maximum_output)?;
			Ok((candidate, report.work, report.accepted_windows > 0))
		}
		Generator::Parity(offset) => {
			let options = ParityOptions {
				linear: LinearOptions {
					max_work: maximum_work.min(1_000_000),
					..LinearOptions::default()
				},
				..ParityOptions::default()
			};
			let (candidate, report) =
				source
					.clone()
					.parity_candidate_from(offset, options, maximum_output)?;
			Ok((candidate, report.work, report.candidate_window.is_some()))
		}
	}
}

fn operation_equivalent(left: &SemanticOperation, right: &SemanticOperation) -> Result<bool> {
	match (left, right) {
		(
			SemanticOperation::Gate {
				gate: a,
				targets: at,
				controls: ac,
			},
			SemanticOperation::Gate {
				gate: b,
				targets: bt,
				controls: bc,
			},
		) => Ok(at == bt && ac == bc && a.equivalent_checked(b)?),
		(
			SemanticOperation::GlobalPhase {
				angle: a,
				controls: ac,
			},
			SemanticOperation::GlobalPhase {
				angle: b,
				controls: bc,
			},
		) => Ok(ac == bc && a.equivalent_checked(b)?),
		_ => Ok(false),
	}
}

/// Equality requires matching source obligations as well as execution semantics.
/// Distinct opaque/effect occurrences conservatively remain distinct.
fn same_candidate(a: &QuantumRegion, b: &QuantumRegion, ledger: &BudgetLedger) -> Result<bool> {
	if a.occurrences().len() != b.occurrences().len() || a.explicit_edges() != b.explicit_edges() {
		return Ok(false);
	}
	let mut work = 0u64;
	let mut bytes = 0u64;
	for program in [a, b] {
		let graph = program.provenance();
		let nodes =
			u64::try_from(graph.node_count()).map_err(|_| Error::Budget("beam comparison"))?;
		let edges =
			u64::try_from(graph.edge_count()).map_err(|_| Error::Budget("beam comparison"))?;
		let log = u64::from(nodes.checked_ilog2().unwrap_or(0))
			.checked_add(3)
			.ok_or(Error::Budget("beam comparison"))?;
		let per_root = nodes
			.checked_mul(log)
			.and_then(|n| n.checked_add(edges))
			.ok_or(Error::Budget("beam comparison"))?;
		let count = u64::try_from(program.occurrences().len())
			.map_err(|_| Error::Budget("beam comparison"))?;
		work = work
			.checked_add(
				per_root
					.checked_mul(count)
					.ok_or(Error::Budget("beam comparison"))?,
			)
			.ok_or(Error::Budget("beam comparison"))?;
		bytes = bytes
			.checked_add(
				nodes
					.checked_mul(64)
					.ok_or(Error::Budget("beam comparison"))?,
			)
			.ok_or(Error::Budget("beam comparison"))?;
		for occurrence in program.occurrences() {
			let source_work = occurrence.operation.equivalence_work_estimate()?;
			work = work
				.checked_add(source_work)
				.ok_or(Error::Budget("beam comparison"))?;
			bytes = bytes
				.checked_add(
					source_work
						.checked_mul(32)
						.ok_or(Error::Budget("beam comparison"))?,
				)
				.ok_or(Error::Budget("beam comparison"))?;
		}
	}
	let _allowance = ledger.reserve(BudgetCategory::Verification, work, bytes)?;
	for (left, right) in a.occurrences().iter().zip(b.occurrences()) {
		if !operation_equivalent(&left.operation, &right.operation)? {
			return Ok(false);
		}
		let al = a
			.provenance()
			.source_leaves(left.provenance, ExpansionLimits::default())?;
		let bl = b
			.provenance()
			.source_leaves(right.provenance, ExpansionLimits::default())?;
		if al != bl {
			return Ok(false);
		}
	}
	Ok(true)
}

fn bound_operation_equivalent(a: &Operation, b: &Operation) -> bool {
	match (a, b) {
		(
			Operation::Gate {
				gate: ag,
				targets: at,
				controls: ac,
			},
			Operation::Gate {
				gate: bg,
				targets: bt,
				controls: bc,
			},
		) => ag == bg && at == bt && ac == bc,
		(
			Operation::GlobalPhase {
				radians: ar,
				controls: ac,
			},
			Operation::GlobalPhase {
				radians: br,
				controls: bc,
			},
		) => ar.to_bits() == br.to_bits() && ac == bc,
		_ => false,
	}
}

fn edge_positions(bound: &BoundRegion) -> Result<Vec<(usize, usize, u8)>> {
	let positions: BTreeMap<_, _> = bound
		.instructions()
		.iter()
		.enumerate()
		.map(|(index, instruction)| (instruction.id(), index))
		.collect();
	let mut edges = bound
		.dependencies()
		.iter()
		.map(|edge| {
			let before = *positions.get(&edge.before).ok_or(Error::InvalidId)?;
			let after = *positions.get(&edge.after).ok_or(Error::InvalidId)?;
			let kind = match edge.kind {
				DependencyKind::Quantum => 0,
				DependencyKind::Classical => 1,
				DependencyKind::Stochastic => 2,
				DependencyKind::Explicit => 3,
			};
			Ok((before, after, kind))
		})
		.collect::<Result<Vec<_>>>()?;
	edges.sort_unstable();
	Ok(edges)
}

fn same_bound(a: &BoundRegion, b: &BoundRegion, ledger: &BudgetLedger) -> Result<bool> {
	if a.instructions().len() != b.instructions().len()
		|| a.dependencies().len() != b.dependencies().len()
		|| a.binding_storage() != b.binding_storage()
		|| a.source_snapshot_id() != b.source_snapshot_id()
	{
		return Ok(false);
	}
	let count = a
		.instructions()
		.len()
		.checked_add(b.instructions().len())
		.and_then(|n| n.checked_add(a.dependencies().len()))
		.and_then(|n| n.checked_add(b.dependencies().len()))
		.ok_or(Error::Budget("beam bound comparison"))?;
	let nodes = a
		.provenance()
		.node_count()
		.checked_add(b.provenance().node_count())
		.ok_or(Error::Budget("beam bound comparison"))?;
	let work = count
		.checked_add(
			nodes
				.checked_mul(count.max(1))
				.ok_or(Error::Budget("beam bound comparison"))?,
		)
		.and_then(|n| n.checked_mul(32))
		.ok_or(Error::Budget("beam bound comparison"))?;
	let bytes = count
		.checked_add(nodes)
		.and_then(|n| n.checked_mul(128))
		.ok_or(Error::Budget("beam bound comparison"))?;
	let _allowance = ledger.reserve(
		BudgetCategory::Verification,
		u64::try_from(work).map_err(|_| Error::Budget("beam bound comparison"))?,
		u64::try_from(bytes).map_err(|_| Error::Budget("beam bound comparison"))?,
	)?;
	if edge_positions(a)? != edge_positions(b)? {
		return Ok(false);
	}
	for (left, right) in a.instructions().iter().zip(b.instructions()) {
		if left.angle_targets() != right.angle_targets()
			|| !bound_operation_equivalent(left.operation(), right.operation())
		{
			return Ok(false);
		}
		if a.provenance()
			.source_leaves(left.provenance(), ExpansionLimits::default())?
			!= b.provenance()
				.source_leaves(right.provenance(), ExpansionLimits::default())?
		{
			return Ok(false);
		}
	}
	Ok(true)
}

#[expect(
	clippy::too_many_lines,
	reason = "Candidate admission, shared ledger and best-publication fallback form one transaction"
)]
fn run_region(
	source: Box<QuantumRegion>,
	bound: BoundRegion,
	config: &OptimizationOptions,
	ledger: &BudgetLedger,
	beam: BeamOptions,
	mut report: BeamReport,
	#[cfg(feature = "workers")] worker: Option<WorkerContext<'_>>,
) -> CompilerResult<BeamRun> {
	let root_lease = match reserve_candidate(&source, ledger) {
		Ok(value) => value,
		Err(Error::Budget(_)) => {
			return Ok(BeamRun {
				input: OptimizerInput::Region { source, bound },
				stop_reason: StopReason::StorageLimit,
				evidence: OptimizationEvidence::OriginalInput,
				rounding: RoundingStatus::Unchanged,
				report,
				allowances: Vec::new(),
			});
		}
		Err(error) => return Err(error.into()),
	};
	let (
		original_score,
		original_terminal,
		original_rounding,
		mut original_allowances,
		original_status,
	) = match score_candidate(&bound, config, ledger) {
		Ok(value) => value,
		Err(Error::Budget(_)) => {
			return Ok(BeamRun {
				input: OptimizerInput::Region { source, bound },
				stop_reason: StopReason::WorkLimit,
				evidence: OptimizationEvidence::OriginalInput,
				rounding: RoundingStatus::Unchanged,
				report,
				allowances: Vec::new(),
			});
		}
		Err(error) => return Err(error.into()),
	};
	let mut pool = vec![Candidate {
		source: Box::new(source.as_ref().clone()),
		score: original_score.clone(),
		bound,
		terminal_bound: original_terminal,
		rounding: original_rounding,
		allowances: {
			original_allowances.push(root_lease);
			original_allowances
		},
		expanded: false,
		bound_only: false,
		#[cfg(feature = "workers")]
		exact_history: None,
		#[cfg(feature = "workers")]
		approx_history: None,
		#[cfg(feature = "workers")]
		error_bound: Some(RBig::from(0)),
	}];
	let mut evidence = OptimizationEvidence::OriginalInput;
	let mut stop_reason = if original_status == TerminalStatus::Unscorable {
		StopReason::Unscorable
	} else {
		StopReason::Complete
	};
	let mut best = 0usize;
	#[cfg(feature = "workers")]
	let mut worker = worker;
	#[cfg(feature = "workers")]
	let worker_report_allowance = if worker.is_some() {
		let bytes = beam
			.workers
			.checked_mul(2048)
			.ok_or(Error::Budget("beam worker report"))?;
		match ledger.reserve(
			BudgetCategory::Worker,
			0,
			u64::try_from(bytes).map_err(|_| Error::Budget("beam worker report"))?,
		) {
			Ok(value) => Some(value),
			Err(Error::Budget(_)) => {
				stop_reason = StopReason::StorageLimit;
				worker = None;
				None
			}
			Err(error) => return Err(error.into()),
		}
	} else {
		None
	};
	{
		'rounds: for round in 0..beam.rounds {
			if report.generated >= beam.candidates {
				break;
			}
			let mut frontier: Vec<_> = pool
				.iter()
				.enumerate()
				.filter_map(|(index, candidate)| {
					(!candidate.expanded && !candidate.bound_only).then_some(index)
				})
				.collect();
			for index in 1..frontier.len() {
				let mut cursor = index;
				while cursor > 0 {
					let previous = cursor.checked_sub(1).ok_or(Error::Budget("beam order"))?;
					let current_id = *frontier.get(cursor).ok_or(Error::InvalidId)?;
					let previous_id = *frontier.get(previous).ok_or(Error::InvalidId)?;
					let ahead = match precedes(
						&pool.get(current_id).ok_or(Error::InvalidId)?.score,
						&pool.get(previous_id).ok_or(Error::InvalidId)?.score,
						config.target(),
						ledger,
						&mut report.unscorable_comparisons,
					) {
						Ok(value) => value,
						Err(Error::Budget(_)) => {
							stop_reason = StopReason::WorkLimit;
							break 'rounds;
						}
						Err(error) => return Err(error.into()),
					};
					if !ahead {
						break;
					}
					frontier.swap(cursor, previous);
					cursor = cursor.checked_sub(1).ok_or(Error::Budget("beam order"))?;
				}
			}
			frontier.truncate(beam.width);
			if frontier.is_empty() {
				break;
			}
			report.rounds = round.checked_add(1).ok_or(Error::Budget("beam rounds"))?;
			for index in frontier {
				pool.get_mut(index).ok_or(Error::InvalidId)?.expanded = true;
				let parent = pool.get(index).ok_or(Error::InvalidId)?;
				let pairs: Vec<_> = parent
					.source
					.parameters()
					.filter_map(|(id, _)| {
						parent
							.bound
							.binding_storage()
							.get(&id)
							.map(|value| (id, *value))
					})
					.collect();
				for generator in [
					Generator::Exact,
					Generator::Gaussian,
					Generator::Pmh,
					Generator::Parity(0),
				] {
					if report.generated >= beam.candidates {
						break;
					}
					let parent = &pool.get(index).ok_or(Error::InvalidId)?.source;
					let maximum_output = parent
						.schedule()
						.len()
						.checked_add(64)
						.ok_or(Error::Budget("beam output"))?
						.min(parent.limits().max_operations);
					let lease = match reserve_candidate(parent, ledger) {
						Ok(value) => value,
						Err(Error::Budget(_)) => {
							stop_reason = StopReason::StorageLimit;
							break;
						}
						Err(error) => return Err(error.into()),
					};
					let (remaining, _) = ledger.remaining()?;
					let ceiling = if matches!(generator, Generator::Exact) {
						4_000_000
					} else {
						1_000_000
					};
					let maximum = remaining.min(ceiling);
					if maximum == 0 {
						stop_reason = StopReason::WorkLimit;
						break;
					}
					let work = ledger.reserve_work_allowance(maximum)?;
					let result = generation(
						parent,
						generator,
						usize::try_from(maximum).map_err(|_| Error::Budget("beam work"))?,
						maximum_output,
					);
					report.generated = report
						.generated
						.checked_add(1)
						.ok_or(Error::Budget("beam candidates"))?;
					let (candidate, actual, changed) = match result {
						Ok(value) => value,
						Err(Error::Budget(_)) => {
							report.generator_budget_exhaustions = report
								.generator_budget_exhaustions
								.checked_add(1)
								.ok_or(Error::Budget("beam generator reports"))?;
							continue;
						}
						Err(Error::Unsupported(_)) => continue,
						Err(error) => return Err(error.into()),
					};
					work.commit(u64::try_from(actual).map_err(|_| Error::Budget("beam work"))?)?;
					if !changed {
						continue;
					}
					let parent_evidence = pool.get(index).ok_or(Error::InvalidId)?;
					let duplicate = pool.iter().try_fold(false, |found, item| {
						if found {
							Ok(true)
						} else if same_evidence(item, parent_evidence) {
							same_candidate(&item.source, &candidate, ledger)
						} else {
							Ok(false)
						}
					});
					match duplicate {
						Ok(true) => continue,
						Ok(false) => (),
						Err(Error::Budget(_)) => {
							stop_reason = StopReason::WorkLimit;
							break;
						}
						Err(error) => return Err(error.into()),
					}
					let binding_work = candidate.binding_work_estimate()?;
					let binding_allowance = match ledger.reserve_work_allowance(binding_work) {
						Ok(value) => value,
						Err(Error::Budget(_)) => {
							stop_reason = StopReason::WorkLimit;
							break;
						}
						Err(error) => return Err(error.into()),
					};
					let mut candidate_bound = candidate.clone().bind(&pairs)?;
					let source = &pool.get(index).ok_or(Error::InvalidId)?.bound;
					candidate_bound.retain_source(
						source.source_snapshot_id(),
						source.binding_storage().clone(),
					)?;
					binding_allowance.commit(binding_work)?;
					let (cost, terminal_bound, rounding, mut terminal_allowances, terminal_status) =
						match score_candidate(&candidate_bound, config, ledger) {
							Ok(value) => value,
							Err(Error::Budget(_)) => {
								stop_reason = StopReason::WorkLimit;
								break 'rounds;
							}
							Err(error) => return Err(error.into()),
						};
					if terminal_status == TerminalStatus::Unscorable {
						stop_reason = StopReason::Unscorable;
					}
					report.admitted = report
						.admitted
						.checked_add(1)
						.ok_or(Error::Budget("beam candidates"))?;
					report.maximum_frontier_operations = report
						.maximum_frontier_operations
						.max(candidate.schedule().len());
					pool.push(Candidate {
						source: Box::new(candidate),
						bound: candidate_bound,
						score: cost,
						terminal_bound,
						rounding,
						allowances: {
							terminal_allowances.push(lease);
							terminal_allowances
						},
						expanded: false,
						bound_only: false,
						#[cfg(feature = "workers")]
						exact_history: pool
							.get(index)
							.ok_or(Error::InvalidId)?
							.exact_history
							.clone(),
						#[cfg(feature = "workers")]
						approx_history: pool
							.get(index)
							.ok_or(Error::InvalidId)?
							.approx_history
							.clone(),
						#[cfg(feature = "workers")]
						error_bound: pool.get(index).ok_or(Error::InvalidId)?.error_bound.clone(),
					});
					let latest = pool
						.len()
						.checked_sub(1)
						.ok_or(Error::Budget("beam candidates"))?;
					match precedes(
						&pool.get(latest).ok_or(Error::InvalidId)?.score,
						&pool.get(best).ok_or(Error::InvalidId)?.score,
						config.target(),
						ledger,
						&mut report.unscorable_comparisons,
					) {
						Ok(true) => best = latest,
						Ok(false) => (),
						Err(Error::Budget(_)) => {
							stop_reason = StopReason::WorkLimit;
							break 'rounds;
						}
						Err(error) => return Err(error.into()),
					}
				}
				'bound_parity: {
					if report.generated >= beam.candidates {
						break 'bound_parity;
					}
					let parent = pool.get(index).ok_or(Error::InvalidId)?;
					if parent.source.snapshot_id() != parent.bound.source_snapshot_id() {
						break 'bound_parity;
					}
					let maximum_output = parent
						.bound
						.instructions()
						.len()
						.checked_add(64)
						.ok_or(Error::Budget("beam output"))?
						.min(parent.source.limits().max_operations);
					let lease = match reserve_candidate(&parent.source, ledger) {
						Ok(value) => value,
						Err(Error::Budget(_)) => {
							stop_reason = StopReason::StorageLimit;
							break 'rounds;
						}
						Err(error) => return Err(error.into()),
					};
					let (remaining, _) = ledger.remaining()?;
					let maximum = remaining.min(1_000_000);
					if maximum == 0 {
						stop_reason = StopReason::WorkLimit;
						break 'rounds;
					}
					let work = ledger.reserve_work_allowance(maximum)?;
					let options = ParityOptions {
						linear: LinearOptions {
							max_work: usize::try_from(maximum)
								.map_err(|_| Error::Budget("beam work"))?,
							..LinearOptions::default()
						},
						..ParityOptions::default()
					};
					let result = parent.source.parity_bound_candidate_from(
						&parent.bound,
						0,
						options,
						maximum_output,
					);
					report.generated = report
						.generated
						.checked_add(1)
						.ok_or(Error::Budget("beam candidates"))?;
					let (candidate_bound, pass) = match result {
						Ok(value) => value,
						Err(Error::Budget(_)) => {
							report.generator_budget_exhaustions = report
								.generator_budget_exhaustions
								.checked_add(1)
								.ok_or(Error::Budget("beam generator reports"))?;
							break 'bound_parity;
						}
						Err(Error::Unsupported(_)) => break 'bound_parity,
						Err(error) => return Err(error.into()),
					};
					work.commit(u64::try_from(pass.work).map_err(|_| Error::Budget("beam work"))?)?;
					if pass.accepted_windows == 0 {
						break 'bound_parity;
					}
					let duplicate = pool.iter().try_fold(false, |found, item| {
						if found {
							Ok(true)
						} else if same_evidence(item, parent) {
							same_bound(&item.bound, &candidate_bound, ledger)
						} else {
							Ok(false)
						}
					});
					match duplicate {
						Ok(true) => break 'bound_parity,
						Ok(false) => (),
						Err(Error::Budget(_)) => {
							stop_reason = StopReason::WorkLimit;
							break 'rounds;
						}
						Err(error) => return Err(error.into()),
					}
					let (cost, terminal_bound, rounding, mut terminal_allowances, terminal_status) =
						match score_candidate(&candidate_bound, config, ledger) {
							Ok(value) => value,
							Err(Error::Budget(_)) => {
								stop_reason = StopReason::WorkLimit;
								break 'rounds;
							}
							Err(error) => return Err(error.into()),
						};
					if terminal_status == TerminalStatus::Unscorable {
						stop_reason = StopReason::Unscorable;
					}
					report.admitted = report
						.admitted
						.checked_add(1)
						.ok_or(Error::Budget("beam candidates"))?;
					report.maximum_frontier_operations = report
						.maximum_frontier_operations
						.max(candidate_bound.instructions().len());
					let materialized_source = bound_as_ideal(&candidate_bound)?;
					let can_expand = materialized_source.is_some();
					let parent_source = materialized_source.map_or_else(
						|| {
							pool.get(index)
								.ok_or(Error::InvalidId)
								.map(|item| item.source.clone())
						},
						|source| Ok(Box::new(source)),
					)?;
					pool.push(Candidate {
						source: parent_source,
						bound: candidate_bound,
						score: cost,
						terminal_bound,
						rounding,
						allowances: {
							terminal_allowances.push(lease);
							terminal_allowances
						},
						expanded: false,
						bound_only: !can_expand,
						#[cfg(feature = "workers")]
						exact_history: pool
							.get(index)
							.ok_or(Error::InvalidId)?
							.exact_history
							.clone(),
						#[cfg(feature = "workers")]
						approx_history: pool
							.get(index)
							.ok_or(Error::InvalidId)?
							.approx_history
							.clone(),
						#[cfg(feature = "workers")]
						error_bound: pool.get(index).ok_or(Error::InvalidId)?.error_bound.clone(),
					});
					let latest = pool
						.len()
						.checked_sub(1)
						.ok_or(Error::Budget("beam candidates"))?;
					match precedes(
						&pool.get(latest).ok_or(Error::InvalidId)?.score,
						&pool.get(best).ok_or(Error::InvalidId)?.score,
						config.target(),
						ledger,
						&mut report.unscorable_comparisons,
					) {
						Ok(true) => best = latest,
						Ok(false) => (),
						Err(Error::Budget(_)) => {
							stop_reason = StopReason::WorkLimit;
							break 'rounds;
						}
						Err(error) => return Err(error.into()),
					}
				}
				#[cfg(feature = "workers")]
				if let Some(worker) = worker {
					for expanded in [false, true] {
						let mut offset = 0usize;
						while offset
							< pool
								.get(index)
								.ok_or(Error::InvalidId)?
								.source
								.schedule()
								.len()
						{
							if report.generated >= beam.candidates {
								break;
							}
							if report.worker_requests >= beam.workers {
								if stop_reason == StopReason::Complete {
									stop_reason = StopReason::WorkerLimit;
								}
								break;
							}
							if report.worker_requests >= beam.workers.div_ceil(2) {
								break;
							}
							let parent = pool.get(index).ok_or(Error::InvalidId)?;
							let maximum_output = parent
								.source
								.schedule()
								.len()
								.checked_add(64)
								.ok_or(Error::Budget("beam output"))?
								.min(parent.source.limits().max_operations);
							let lease = match reserve_candidate(&parent.source, ledger) {
								Ok(value) => value,
								Err(Error::Budget(_)) => {
									stop_reason = StopReason::StorageLimit;
									break 'rounds;
								}
								Err(error) => return Err(error.into()),
							};
							let (worker_work, worker_bytes) = ledger.remaining()?;
							if worker_bytes
								< u64::try_from(worker.limits.bytes)
									.map_err(|_| Error::Budget("beam worker storage"))?
							{
								stop_reason = StopReason::StorageLimit;
								break 'rounds;
							}
							if worker_work < 1_000_000 {
								stop_reason = StopReason::WorkLimit;
								break 'rounds;
							}
							let proof = match ledger.reserve(
								BudgetCategory::Worker,
								1_000_000,
								u64::try_from(worker.limits.bytes)
									.map_err(|_| Error::Budget("beam worker storage"))?,
							) {
								Ok(value) => value,
								Err(Error::Budget(_)) => {
									stop_reason = StopReason::WorkLimit;
									break 'rounds;
								}
								Err(error) => return Err(error.into()),
							};
							let seed = worker
								.seed
								.checked_add(
									u64::try_from(report.worker_requests)
										.map_err(|_| Error::Budget("beam worker seed"))?,
								)
								.ok_or(Error::Budget("beam worker seed"))?;
							let response = zx_generation(
								&parent.source,
								offset,
								expanded,
								worker,
								seed,
								maximum_output,
							);
							report.generated = report
								.generated
								.checked_add(1)
								.ok_or(Error::Budget("beam candidates"))?;
							let (candidate, pass) = match response {
								Ok(value) => value,
								Err(crate::WorkerError::Ordering) => break,
								Err(crate::WorkerError::Worker(
									quest_optimizer_client::Error::Candidate { code, message },
								)) => {
									report.worker_requests = report
										.worker_requests
										.checked_add(1)
										.ok_or(Error::Budget("beam worker requests"))?;
									report.worker_declines = report
										.worker_declines
										.checked_add(1)
										.ok_or(Error::Budget("beam worker declines"))?;
									let mut summary = String::new();
									summary
										.try_reserve_exact(1024)
										.map_err(|_| Error::Budget("beam worker report"))?;
									summary.extend(
										code.chars()
											.chain(": ".chars())
											.chain(message.chars())
											.take(256),
									);
									report.worker_failures.push(summary);
									break;
								}
								Err(crate::WorkerError::Worker(
									quest_optimizer_client::Error::Timeout,
								)) => {
									report.worker_requests = report
										.worker_requests
										.checked_add(1)
										.ok_or(Error::Budget("beam worker requests"))?;
									stop_reason = StopReason::Timeout;
									break 'rounds;
								}
								Err(error) => {
									report.worker_requests = report
										.worker_requests
										.checked_add(1)
										.ok_or(Error::Budget("beam worker requests"))?;
									return Err(CompilerError::from(error));
								}
							};
							let Some((_, end)) = pass.candidate_window else {
								break;
							};
							report.worker_requests = report
								.worker_requests
								.checked_add(1)
								.ok_or(Error::Budget("beam worker requests"))?;
							offset = end.max(
								offset
									.checked_add(1)
									.ok_or(Error::Budget("beam worker offset"))?,
							);
							if pass.accepted.is_empty() {
								continue;
							}
							let candidate_bound = match bind_candidate(
								&candidate,
								&pairs,
								&pool.get(index).ok_or(Error::InvalidId)?.bound,
								ledger,
							) {
								Ok(value) => value,
								Err(Error::Budget(reason)) => {
									stop_reason = budget_stop(reason);
									break 'rounds;
								}
								Err(error) => return Err(error.into()),
							};
							let (cost, terminal_bound, rounding, mut terminal_allowances, status) =
								match score_candidate(&candidate_bound, config, ledger) {
									Ok(value) => value,
									Err(Error::Budget(reason)) => {
										stop_reason = budget_stop(reason);
										break 'rounds;
									}
									Err(error) => return Err(error.into()),
								};
							if status == TerminalStatus::Unscorable {
								stop_reason = StopReason::Unscorable;
							}
							let mut history = pool
								.get(index)
								.ok_or(Error::InvalidId)?
								.exact_history
								.clone();
							let proof = Arc::new(proof);
							for certificate in pass.accepted {
								history = Some(Arc::new(ExactHistory {
									parent: history,
									certificate: Arc::new(certificate),
									_proof_allowance: Arc::clone(&proof),
								}));
							}
							report.admitted = report
								.admitted
								.checked_add(1)
								.ok_or(Error::Budget("beam candidates"))?;
							report.maximum_frontier_operations = report
								.maximum_frontier_operations
								.max(candidate.schedule().len());
							pool.push(Candidate {
								source: Box::new(candidate),
								bound: candidate_bound,
								score: cost,
								terminal_bound,
								rounding,
								allowances: {
									terminal_allowances.push(lease);
									terminal_allowances
								},
								expanded: false,
								bound_only: false,
								exact_history: history,
								approx_history: pool
									.get(index)
									.ok_or(Error::InvalidId)?
									.approx_history
									.clone(),
								error_bound: pool
									.get(index)
									.ok_or(Error::InvalidId)?
									.error_bound
									.clone(),
							});
							let latest = pool
								.len()
								.checked_sub(1)
								.ok_or(Error::Budget("beam candidates"))?;
							match precedes(
								&pool.get(latest).ok_or(Error::InvalidId)?.score,
								&pool.get(best).ok_or(Error::InvalidId)?.score,
								config.target(),
								ledger,
								&mut report.unscorable_comparisons,
							) {
								Ok(true) => best = latest,
								Ok(false) => (),
								Err(Error::Budget(_)) => {
									stop_reason = StopReason::WorkLimit;
									break 'rounds;
								}
								Err(error) => return Err(error.into()),
							}
						}
					}
				}
				#[cfg(feature = "workers")]
				if let Some(worker) = worker {
					let mut offset = 0usize;
					let mut requests = 0usize;
					while offset
						< pool
							.get(index)
							.ok_or(Error::InvalidId)?
							.source
							.schedule()
							.len()
					{
						if !exact_mitm_hint(&pool.get(index).ok_or(Error::InvalidId)?.source) {
							break;
						}
						if report.generated >= beam.candidates {
							break;
						}
						if requests >= beam.workers.div_ceil(4) {
							break;
						}
						if report.worker_requests >= beam.workers {
							if stop_reason == StopReason::Complete {
								stop_reason = StopReason::WorkerLimit;
							}
							break;
						}
						let parent = pool.get(index).ok_or(Error::InvalidId)?;
						let max_output = parent
							.source
							.schedule()
							.len()
							.checked_add(64)
							.ok_or(Error::Budget("beam exact output"))?
							.min(parent.source.limits().max_operations);
						let candidate_lease = match reserve_candidate(&parent.source, ledger) {
							Ok(value) => value,
							Err(Error::Budget(_)) => {
								stop_reason = StopReason::StorageLimit;
								break 'rounds;
							}
							Err(error) => return Err(error.into()),
						};
						let (remaining_work, remaining_bytes) = ledger.remaining()?;
						let maximum = remaining_work.min(1_000_000);
						if maximum == 0 {
							stop_reason = StopReason::WorkLimit;
							break 'rounds;
						}
						if remaining_bytes
							< u64::try_from(worker.limits.bytes)
								.map_err(|_| Error::Budget("beam exact proof storage"))?
						{
							stop_reason = StopReason::StorageLimit;
							break 'rounds;
						}
						let work = ledger.reserve_work_allowance(maximum)?;
						let proof = ledger.reserve(
							BudgetCategory::Worker,
							0,
							u64::try_from(worker.limits.bytes)
								.map_err(|_| Error::Budget("beam exact proof storage"))?,
						)?;
						let search_limits = quest_optimizer_protocol::MitmLimits {
							max_work: maximum,
							..quest_optimizer_protocol::MitmLimits::for_qubits(2)
								.map_err(|_| Error::Budget("beam exact MITM limits"))?
						};
						let seed = worker
							.seed
							.checked_add(
								u64::try_from(report.worker_requests)
									.map_err(|_| Error::Budget("beam worker seed"))?,
							)
							.ok_or(Error::Budget("beam worker seed"))?;
						report.worker_requests = report
							.worker_requests
							.checked_add(1)
							.ok_or(Error::Budget("beam worker request admissions"))?;
						let response = parent.source.clone().exact_mitm_candidate_from(
							offset,
							worker.client,
							seed,
							search_limits,
							worker.limits,
							max_output,
						);
						report.generated = report
							.generated
							.checked_add(1)
							.ok_or(Error::Budget("beam candidates"))?;
						let (candidate, mut pass) = match response {
							Ok(value) => value,
							Err(crate::WorkerError::Ordering) => break,
							Err(crate::WorkerError::Budget(_)) => {
								report.generator_budget_exhaustions = report
									.generator_budget_exhaustions
									.checked_add(1)
									.ok_or(Error::Budget("beam generator reports"))?;
								break;
							}
							Err(crate::WorkerError::Worker(
								quest_optimizer_client::Error::Candidate { code, message },
							)) => {
								report.worker_declines = report
									.worker_declines
									.checked_add(1)
									.ok_or(Error::Budget("beam worker declines"))?;
								report.worker_failures.push(
									code.chars()
										.chain(": ".chars())
										.chain(message.chars())
										.take(256)
										.collect(),
								);
								break;
							}
							Err(crate::WorkerError::Worker(
								quest_optimizer_client::Error::Timeout,
							)) => {
								stop_reason = StopReason::Timeout;
								break 'rounds;
							}
							Err(error) => return Err(CompilerError::from(error)),
						};
						let Some((start, end)) = pass.candidate_window else {
							work.commit(pass.local_work)?;
							report.worker_requests = report
								.worker_requests
								.checked_sub(1)
								.ok_or(Error::Budget("beam worker request refund"))?;
							break;
						};
						work.commit(maximum)?;
						requests = requests
							.checked_add(1)
							.ok_or(Error::Budget("beam worker requests"))?;
						offset = end.max(
							offset
								.checked_add(1)
								.ok_or(Error::Budget("beam exact offset"))?,
						);
						let outcome = pass.outcome.take().ok_or(Error::InvalidId)?;
						report
							.exact_mitm_statuses
							.push(mitm_status(&outcome, (start, end)));
						if matches!(
							outcome,
							MitmResult::Incomplete { .. }
								| MitmResult::Unresolved { .. }
								| MitmResult::Exhausted { .. }
						) && stop_reason == StopReason::Complete
						{
							stop_reason = StopReason::GeneratorLimit;
						}
						let MitmResult::Candidate(certificate) = outcome else {
							continue;
						};
						let candidate_bound = match bind_candidate(
							&candidate,
							&pairs,
							&pool.get(index).ok_or(Error::InvalidId)?.bound,
							ledger,
						) {
							Ok(value) => value,
							Err(Error::Budget(reason)) => {
								stop_reason = budget_stop(reason);
								break 'rounds;
							}
							Err(error) => return Err(error.into()),
						};
						let (cost, terminal_bound, rounding, mut terminal_allowances, status) =
							match score_candidate(&candidate_bound, config, ledger) {
								Ok(value) => value,
								Err(Error::Budget(reason)) => {
									stop_reason = budget_stop(reason);
									break 'rounds;
								}
								Err(error) => return Err(error.into()),
							};
						if status == TerminalStatus::Unscorable {
							stop_reason = StopReason::Unscorable;
						}
						let history = Some(Arc::new(ExactHistory {
							parent: pool
								.get(index)
								.ok_or(Error::InvalidId)?
								.exact_history
								.clone(),
							certificate: Arc::new(certificate),
							_proof_allowance: Arc::new(proof),
						}));
						report.admitted = report
							.admitted
							.checked_add(1)
							.ok_or(Error::Budget("beam candidates"))?;
						report.maximum_frontier_operations = report
							.maximum_frontier_operations
							.max(candidate.schedule().len());
						pool.push(Candidate {
							source: Box::new(candidate),
							bound: candidate_bound,
							score: cost,
							terminal_bound,
							rounding,
							allowances: {
								terminal_allowances.push(candidate_lease);
								terminal_allowances
							},
							expanded: false,
							bound_only: false,
							exact_history: history,
							approx_history: pool
								.get(index)
								.ok_or(Error::InvalidId)?
								.approx_history
								.clone(),
							error_bound: pool
								.get(index)
								.ok_or(Error::InvalidId)?
								.error_bound
								.clone(),
						});
						let latest = pool
							.len()
							.checked_sub(1)
							.ok_or(Error::Budget("beam candidates"))?;
						match precedes(
							&pool.get(latest).ok_or(Error::InvalidId)?.score,
							&pool.get(best).ok_or(Error::InvalidId)?.score,
							config.target(),
							ledger,
							&mut report.unscorable_comparisons,
						) {
							Ok(true) => best = latest,
							Ok(false) => (),
							Err(Error::Budget(_)) => {
								stop_reason = StopReason::WorkLimit;
								break 'rounds;
							}
							Err(error) => return Err(error.into()),
						}
					}
				}
				#[cfg(feature = "workers")]
				'synthesis: {
					let Some(worker) = worker else {
						break 'synthesis;
					};
					let Some(budget) = (match config.approximation() {
						ApproximationMode::Local(value) | ApproximationMode::Global(value) => {
							Some(value.value())
						}
						ApproximationMode::Disabled => None,
					}) else {
						break 'synthesis;
					};
					if report.generated >= beam.candidates {
						break 'synthesis;
					}
					if report.worker_requests >= beam.workers {
						if stop_reason == StopReason::Complete {
							stop_reason = StopReason::WorkerLimit;
						}
						break 'synthesis;
					}
					let parent = pool.get(index).ok_or(Error::InvalidId)?;
					if parent.source.parameters().len() != 0 || rotation_count(&parent.source) != 1
					{
						break 'synthesis;
					}
					if matches!(config.approximation(), ApproximationMode::Global(_))
						&& !parent
							.source
							.occurrences()
							.iter()
							.all(|item| item.operation.exact_unitary())
					{
						break 'synthesis;
					}
					let Some(spent) = parent.error_bound.as_ref() else {
						break 'synthesis;
					};
					let _arithmetic =
						match ledger.reserve(BudgetCategory::Verification, 100_000, 1024 * 1024) {
							Ok(value) => value,
							Err(Error::Budget(_)) => {
								stop_reason = StopReason::WorkLimit;
								break 'rounds;
							}
							Err(error) => return Err(error.into()),
						};
					let Some((epsilon, exact_epsilon)) =
						admitted_epsilon(budget, spent, 1, worker.limits)?
					else {
						break 'synthesis;
					};
					let next_error = std::ops::Add::add(spent, &exact_epsilon);
					if next_error.numerator().bit_len() > 16_384
						|| next_error.denominator().bit_len() > 16_384
					{
						report.generator_budget_exhaustions = report
							.generator_budget_exhaustions
							.checked_add(1)
							.ok_or(Error::Budget("beam approximation reports"))?;
						break 'synthesis;
					}
					let maximum_output = parent
						.source
						.schedule()
						.len()
						.checked_add(64)
						.ok_or(Error::Budget("beam output"))?
						.min(parent.source.limits().max_operations);
					let retained_other = parent
						.source
						.schedule()
						.len()
						.checked_sub(1)
						.ok_or(Error::Budget("beam output"))?;
					let available_output = maximum_output
						.checked_sub(retained_other)
						.ok_or(Error::Budget("beam output"))?;
					if available_output == 0 {
						break 'synthesis;
					}
					let mut proof_limits = worker.limits;
					proof_limits.gates = proof_limits.gates.min(available_output);
					let candidate_lease = match reserve_candidate(&parent.source, ledger) {
						Ok(value) => value,
						Err(Error::Budget(_)) => {
							stop_reason = StopReason::StorageLimit;
							break 'rounds;
						}
						Err(error) => return Err(error.into()),
					};
					let (remaining_work, remaining_bytes) = ledger.remaining()?;
					if remaining_work < 1_000_000 {
						stop_reason = StopReason::WorkLimit;
						break 'rounds;
					}
					if remaining_bytes
						< u64::try_from(proof_limits.bytes)
							.map_err(|_| Error::Budget("beam proof storage"))?
					{
						stop_reason = StopReason::StorageLimit;
						break 'rounds;
					}
					let proof = ledger.reserve(
						BudgetCategory::Worker,
						1_000_000,
						u64::try_from(proof_limits.bytes)
							.map_err(|_| Error::Budget("beam proof storage"))?,
					)?;
					let seed = worker
						.seed
						.checked_add(
							u64::try_from(report.worker_requests)
								.map_err(|_| Error::Budget("beam worker seed"))?,
						)
						.ok_or(Error::Budget("beam worker seed"))?;
					report.worker_requests = report
						.worker_requests
						.checked_add(1)
						.ok_or(Error::Budget("beam worker requests"))?;
					report.generated = report
						.generated
						.checked_add(1)
						.ok_or(Error::Budget("beam candidates"))?;
					let response = parent.source.clone().synthesize_rotations(
						worker.client,
						epsilon,
						seed,
						proof_limits,
					);
					let (candidate, pass) = match response {
						Ok(value) => value,
						Err(crate::WorkerError::Ordering) => break 'synthesis,
						Err(crate::WorkerError::Budget(_)) => {
							report.generator_budget_exhaustions = report
								.generator_budget_exhaustions
								.checked_add(1)
								.ok_or(Error::Budget("beam generator reports"))?;
							break 'synthesis;
						}
						Err(crate::WorkerError::Worker(
							quest_optimizer_client::Error::Candidate { code, message },
						)) => {
							report.worker_declines = report
								.worker_declines
								.checked_add(1)
								.ok_or(Error::Budget("beam worker declines"))?;
							let summary: String = code
								.chars()
								.chain(": ".chars())
								.chain(message.chars())
								.take(256)
								.collect();
							report.worker_failures.push(summary);
							break 'synthesis;
						}
						Err(crate::WorkerError::Worker(quest_optimizer_client::Error::Timeout)) => {
							stop_reason = StopReason::Timeout;
							break 'rounds;
						}
						Err(error) => return Err(CompilerError::from(error)),
					};
					if pass.rotations.len() != 1 || candidate.schedule().len() > maximum_output {
						return Err(Error::Budget("beam synthesis output").into());
					}
					if matches!(config.approximation(), ApproximationMode::Global(_))
						&& pass.operator_error_bound.is_none()
					{
						return Err(Error::Budget("beam global synthesis proof").into());
					}
					let candidate_bound = match bind_candidate(
						&candidate,
						&pairs,
						&pool.get(index).ok_or(Error::InvalidId)?.bound,
						ledger,
					) {
						Ok(value) => value,
						Err(Error::Budget(reason)) => {
							stop_reason = budget_stop(reason);
							break 'rounds;
						}
						Err(error) => return Err(error.into()),
					};
					let (cost, terminal_bound, rounding, mut terminal_allowances, status) =
						match score_candidate(&candidate_bound, config, ledger) {
							Ok(value) => value,
							Err(Error::Budget(reason)) => {
								stop_reason = budget_stop(reason);
								break 'rounds;
							}
							Err(error) => return Err(error.into()),
						};
					if status == TerminalStatus::Unscorable {
						stop_reason = StopReason::Unscorable;
					}
					let mut rotations = pass.rotations.into_iter();
					let certificate = rotations.next().ok_or(Error::InvalidId)?;
					let history = Some(Arc::new(ApproxHistory {
						parent: parent.approx_history.clone(),
						certificate: ApproxEvidence::Synthesis(Arc::new(certificate)),
						_proof_allowance: Arc::new(proof),
					}));
					report.admitted = report
						.admitted
						.checked_add(1)
						.ok_or(Error::Budget("beam candidates"))?;
					report.maximum_frontier_operations = report
						.maximum_frontier_operations
						.max(candidate.schedule().len());
					pool.push(Candidate {
						source: Box::new(candidate),
						bound: candidate_bound,
						score: cost,
						terminal_bound,
						rounding,
						allowances: {
							terminal_allowances.push(candidate_lease);
							terminal_allowances
						},
						expanded: false,
						bound_only: false,
						exact_history: pool
							.get(index)
							.ok_or(Error::InvalidId)?
							.exact_history
							.clone(),
						approx_history: history,
						error_bound: Some(next_error),
					});
					let latest = pool
						.len()
						.checked_sub(1)
						.ok_or(Error::Budget("beam candidates"))?;
					match precedes(
						&pool.get(latest).ok_or(Error::InvalidId)?.score,
						&pool.get(best).ok_or(Error::InvalidId)?.score,
						config.target(),
						ledger,
						&mut report.unscorable_comparisons,
					) {
						Ok(true) => best = latest,
						Ok(false) => (),
						Err(Error::Budget(_)) => {
							stop_reason = StopReason::WorkLimit;
							break 'rounds;
						}
						Err(error) => return Err(error.into()),
					}
				}
				#[cfg(feature = "workers")]
				if let Some(worker) = worker
					&& let ApproximationMode::Local(budget) | ApproximationMode::Global(budget) =
						config.approximation()
				{
					let mut offset = 0usize;
					while offset
						< pool
							.get(index)
							.ok_or(Error::InvalidId)?
							.source
							.schedule()
							.len()
					{
						if report.generated >= beam.candidates {
							break;
						}
						if report.worker_requests >= beam.workers {
							if stop_reason == StopReason::Complete {
								stop_reason = StopReason::WorkerLimit;
							}
							break;
						}
						let parent = pool.get(index).ok_or(Error::InvalidId)?;
						let count = rotation_count(&parent.source);
						if count == 0 || parent.source.parameters().len() != 0 {
							break;
						}
						if matches!(config.approximation(), ApproximationMode::Global(_))
							&& !parent
								.source
								.occurrences()
								.iter()
								.all(|item| item.operation.exact_unitary())
						{
							break;
						}
						let Some(spent) = parent.error_bound.as_ref() else {
							break;
						};
						let arithmetic = match ledger.reserve(
							BudgetCategory::Verification,
							100_000,
							1024 * 1024,
						) {
							Ok(value) => value,
							Err(Error::Budget(_)) => {
								stop_reason = StopReason::WorkLimit;
								break 'rounds;
							}
							Err(error) => return Err(error.into()),
						};
						let Some((epsilon, exact_epsilon)) =
							admitted_epsilon(budget.value(), spent, count, worker.limits)?
						else {
							break;
						};
						let next_error = std::ops::Add::add(spent, &exact_epsilon);
						if next_error.numerator().bit_len() > 16_384
							|| next_error.denominator().bit_len() > 16_384
						{
							report.generator_budget_exhaustions = report
								.generator_budget_exhaustions
								.checked_add(1)
								.ok_or(Error::Budget("beam approximation reports"))?;
							break;
						}
						drop(arithmetic);
						let maximum_output = parent
							.source
							.schedule()
							.len()
							.checked_add(64)
							.ok_or(Error::Budget("beam approx output"))?
							.min(parent.source.limits().max_operations);
						let candidate_lease = match reserve_candidate(&parent.source, ledger) {
							Ok(value) => value,
							Err(Error::Budget(_)) => {
								stop_reason = StopReason::StorageLimit;
								break 'rounds;
							}
							Err(error) => return Err(error.into()),
						};
						let (remaining_work, remaining_bytes) = ledger.remaining()?;
						let maximum = remaining_work.min(1_000_000);
						if maximum == 0 {
							stop_reason = StopReason::WorkLimit;
							break 'rounds;
						}
						if remaining_bytes
							< u64::try_from(worker.limits.bytes)
								.map_err(|_| Error::Budget("beam approx proof storage"))?
						{
							stop_reason = StopReason::StorageLimit;
							break 'rounds;
						}
						let work = ledger.reserve_work_allowance(maximum)?;
						let proof = ledger.reserve(
							BudgetCategory::Worker,
							0,
							u64::try_from(worker.limits.bytes)
								.map_err(|_| Error::Budget("beam approx proof storage"))?,
						)?;
						let search_limits = quest_optimizer_protocol::MitmLimits {
							max_work: maximum,
							..quest_optimizer_protocol::MitmLimits::for_qubits(1)
								.map_err(|_| Error::Budget("beam approx MITM limits"))?
						};
						let seed = worker
							.seed
							.checked_add(
								u64::try_from(report.worker_requests)
									.map_err(|_| Error::Budget("beam worker seed"))?,
							)
							.ok_or(Error::Budget("beam worker seed"))?;
						report.worker_requests = report
							.worker_requests
							.checked_add(1)
							.ok_or(Error::Budget("beam worker request admissions"))?;
						let response = parent.source.clone().approx_mitm_candidate_from(
							offset,
							worker.client,
							epsilon.to_bits(),
							seed,
							search_limits,
							worker.limits,
							maximum_output,
						);
						report.generated = report
							.generated
							.checked_add(1)
							.ok_or(Error::Budget("beam candidates"))?;
						let (candidate, mut pass) = match response {
							Ok(value) => value,
							Err(crate::WorkerError::Ordering) => break,
							Err(crate::WorkerError::Budget(_)) => {
								report.generator_budget_exhaustions = report
									.generator_budget_exhaustions
									.checked_add(1)
									.ok_or(Error::Budget("beam generator reports"))?;
								break;
							}
							Err(crate::WorkerError::Worker(
								quest_optimizer_client::Error::Candidate { code, message },
							)) => {
								report.worker_declines = report
									.worker_declines
									.checked_add(1)
									.ok_or(Error::Budget("beam worker declines"))?;
								report.worker_failures.push(
									code.chars()
										.chain(": ".chars())
										.chain(message.chars())
										.take(256)
										.collect(),
								);
								break;
							}
							Err(crate::WorkerError::Worker(
								quest_optimizer_client::Error::Timeout,
							)) => {
								stop_reason = StopReason::Timeout;
								break 'rounds;
							}
							Err(error) => return Err(CompilerError::from(error)),
						};
						let Some((start, end)) = pass.candidate_window else {
							work.commit(pass.local_work)?;
							report.worker_requests = report
								.worker_requests
								.checked_sub(1)
								.ok_or(Error::Budget("beam worker request refund"))?;
							break;
						};
						work.commit(maximum)?;
						offset = end.max(
							offset
								.checked_add(1)
								.ok_or(Error::Budget("beam approx offset"))?,
						);
						let outcome = pass.outcome.take().ok_or(Error::InvalidId)?;
						report
							.approx_mitm_statuses
							.push(mitm_status(&outcome, (start, end)));
						if matches!(
							outcome,
							MitmResult::Incomplete { .. }
								| MitmResult::Unresolved { .. }
								| MitmResult::Exhausted { .. }
						) && stop_reason == StopReason::Complete
						{
							stop_reason = StopReason::GeneratorLimit;
						}
						let MitmResult::Candidate(certificate) = outcome else {
							continue;
						};
						let candidate_bound = match bind_candidate(
							&candidate,
							&pairs,
							&pool.get(index).ok_or(Error::InvalidId)?.bound,
							ledger,
						) {
							Ok(value) => value,
							Err(Error::Budget(reason)) => {
								stop_reason = budget_stop(reason);
								break 'rounds;
							}
							Err(error) => return Err(error.into()),
						};
						let (cost, terminal_bound, rounding, mut terminal_allowances, status) =
							match score_candidate(&candidate_bound, config, ledger) {
								Ok(value) => value,
								Err(Error::Budget(reason)) => {
									stop_reason = budget_stop(reason);
									break 'rounds;
								}
								Err(error) => return Err(error.into()),
							};
						if status == TerminalStatus::Unscorable {
							stop_reason = StopReason::Unscorable;
						}
						let history = Some(Arc::new(ApproxHistory {
							parent: pool
								.get(index)
								.ok_or(Error::InvalidId)?
								.approx_history
								.clone(),
							certificate: ApproxEvidence::Mitm(Arc::new(certificate)),
							_proof_allowance: Arc::new(proof),
						}));
						report.admitted = report
							.admitted
							.checked_add(1)
							.ok_or(Error::Budget("beam candidates"))?;
						report.maximum_frontier_operations = report
							.maximum_frontier_operations
							.max(candidate.schedule().len());
						pool.push(Candidate {
							source: Box::new(candidate),
							bound: candidate_bound,
							score: cost,
							terminal_bound,
							rounding,
							allowances: {
								terminal_allowances.push(candidate_lease);
								terminal_allowances
							},
							expanded: false,
							bound_only: false,
							exact_history: pool
								.get(index)
								.ok_or(Error::InvalidId)?
								.exact_history
								.clone(),
							approx_history: history,
							error_bound: Some(next_error),
						});
						let latest = pool
							.len()
							.checked_sub(1)
							.ok_or(Error::Budget("beam candidates"))?;
						match precedes(
							&pool.get(latest).ok_or(Error::InvalidId)?.score,
							&pool.get(best).ok_or(Error::InvalidId)?.score,
							config.target(),
							ledger,
							&mut report.unscorable_comparisons,
						) {
							Ok(true) => best = latest,
							Ok(false) => (),
							Err(Error::Budget(_)) => {
								stop_reason = StopReason::WorkLimit;
								break 'rounds;
							}
							Err(error) => return Err(error.into()),
						}
					}
				}
			}
		}
	}
	if stop_reason == StopReason::Complete {
		if report.unscorable_comparisons != 0 {
			stop_reason = StopReason::Unscorable;
		} else if report.generator_budget_exhaustions != 0 {
			stop_reason = StopReason::GeneratorLimit;
		} else if report.generated >= beam.candidates {
			stop_reason = StopReason::CandidateLimit;
		} else if pool
			.iter()
			.any(|candidate| !candidate.expanded && !candidate.bound_only)
		{
			stop_reason = StopReason::RoundLimit;
		}
	}
	let selected = pool.swap_remove(best);
	#[cfg(feature = "workers")]
	{
		report
			.retained_exact_history
			.clone_from(&selected.exact_history);
		let mut current = selected.exact_history.as_ref();
		while let Some(history) = current {
			report.exact_regions.push(Arc::clone(&history.certificate));
			current = history.parent.as_ref();
		}
		report.exact_regions.reverse();
		report
			.retained_approx_history
			.clone_from(&selected.approx_history);
		let mut current = selected.approx_history.as_ref();
		while let Some(history) = current {
			match &history.certificate {
				ApproxEvidence::Synthesis(certificate) => {
					report.local_rotations.push(Arc::clone(certificate));
				}
				ApproxEvidence::Mitm(certificate) => {
					report.approx_regions.push(Arc::clone(certificate));
				}
			}
			current = history.parent.as_ref();
		}
		report.local_rotations.reverse();
		report.approx_regions.reverse();
		report.cumulative_error.clone_from(&selected.error_bound);
	}
	match config.target().compare(&selected.score, &original_score)? {
		CostComparison::Better => {
			evidence = OptimizationEvidence::ExactVerified;
			#[cfg(feature = "workers")]
			if selected.approx_history.is_some() {
				evidence = if matches!(config.approximation(), ApproximationMode::Global(budget)
                    if selected.error_bound.as_ref().is_some_and(|error| error <= budget.value())
                    && source.occurrences().iter().all(|item| item.operation.exact_unitary()))
				{
					OptimizationEvidence::GlobalCertified
				} else {
					OptimizationEvidence::LocalCertified
				};
			}
		}
		CostComparison::Unscorable => {
			report.unscorable_comparisons = report
				.unscorable_comparisons
				.checked_add(1)
				.ok_or(Error::Budget("beam comparisons"))?;
			if stop_reason == StopReason::Complete {
				stop_reason = StopReason::Unscorable;
			}
		}
		CostComparison::Equal | CostComparison::Worse => (),
	}
	// The original ideal publication remains the binding and conversion authority.
	// A beam candidate is published only as a bound execution snapshot.
	let rounding = selected.rounding;
	let bound = selected.terminal_bound.unwrap_or(selected.bound);
	#[cfg(feature = "workers")]
	let mut allowances = selected.allowances;
	#[cfg(not(feature = "workers"))]
	let allowances = selected.allowances;
	#[cfg(feature = "workers")]
	if let Some(lease) = worker_report_allowance {
		allowances.push(lease);
	}
	Ok(BeamRun {
		input: OptimizerInput::Region { source, bound },
		stop_reason,
		evidence,
		rounding,
		report,
		allowances,
	})
}

pub fn run(
	input: OptimizerInput,
	config: &OptimizationOptions,
	ledger: &BudgetLedger,
	beam: BeamOptions,
) -> CompilerResult<BeamRun> {
	run_inner(
		input,
		config,
		ledger,
		beam,
		#[cfg(feature = "workers")]
		None,
	)
}
#[cfg(feature = "workers")]
pub fn run_with_workers(
	input: OptimizerInput,
	config: &OptimizationOptions,
	ledger: &BudgetLedger,
	beam: BeamOptions,
	client: &quest_optimizer_client::Client,
	seed: u64,
	limits: quest_math::Limits,
) -> CompilerResult<BeamRun> {
	run_inner(
		input,
		config,
		ledger,
		beam,
		Some(WorkerContext {
			client,
			seed,
			limits,
		}),
	)
}
fn run_inner(
	input: OptimizerInput,
	config: &OptimizationOptions,
	ledger: &BudgetLedger,
	beam: BeamOptions,
	#[cfg(feature = "workers")] worker: Option<WorkerContext<'_>>,
) -> CompilerResult<BeamRun> {
	let report = BeamReport {
		input_snapshot: input.snapshot_id(),
		rounds: 0,
		generated: 0,
		admitted: 0,
		maximum_frontier_operations: 0,
		unscorable_comparisons: 0,
		generator_budget_exhaustions: 0,
		structured: None,
		#[cfg(feature = "workers")]
		worker_requests: 0,
		#[cfg(feature = "workers")]
		exact_regions: Vec::new(),
		#[cfg(feature = "workers")]
		retained_exact_history: None,
		#[cfg(feature = "workers")]
		local_rotations: Vec::new(),
		#[cfg(feature = "workers")]
		cumulative_error: None,
		#[cfg(feature = "workers")]
		retained_approx_history: None,
		#[cfg(feature = "workers")]
		worker_failures: Vec::new(),
		#[cfg(feature = "workers")]
		worker_declines: 0,
		#[cfg(feature = "workers")]
		exact_mitm_statuses: Vec::new(),
		#[cfg(feature = "workers")]
		approx_mitm_statuses: Vec::new(),
		#[cfg(feature = "workers")]
		approx_regions: Vec::new(),
	};
	match input {
		OptimizerInput::Region { source, bound } => run_region(
			source,
			bound,
			config,
			ledger,
			beam,
			report,
			#[cfg(feature = "workers")]
			worker,
		),
		OptimizerInput::Bound(bound) => run_bound(
			bound,
			config,
			ledger,
			beam,
			report,
			#[cfg(feature = "workers")]
			worker,
		),
		OptimizerInput::VerifiedStructured(program) => {
			let outcome = program
				.optimize_structured(config, ledger)
				.map_err(structured_error)?;
			let mut report = report;
			report.rounds = 1;
			report.generated = 1;
			report.admitted = outcome
				.report()
				.exact_report()
				.map_or(0, |exact| exact.rewrites.len());
			let rounding = if outcome
				.report()
				.terminal_report()
				.is_some_and(crate::StructuredTerminalReport::rounding_changed)
			{
				RoundingStatus::NumericalFusionChanged
			} else {
				RoundingStatus::Unchanged
			};
			let stop_reason = match outcome.report().status() {
				TerminalStatus::WorkLimit => StopReason::WorkLimit,
				TerminalStatus::StorageLimit => StopReason::StorageLimit,
				TerminalStatus::Unscorable => StopReason::Unscorable,
				TerminalStatus::Complete => StopReason::Complete,
			};
			let evidence = if outcome.report().exact_report().is_some() {
				OptimizationEvidence::ExactVerified
			} else {
				OptimizationEvidence::OriginalInput
			};
			let (program, structured, allowances) = outcome.into_parts();
			report.structured = Some(structured);
			Ok(BeamRun {
				input: OptimizerInput::VerifiedStructured(program),
				stop_reason,
				evidence,
				rounding,
				report,
				allowances,
			})
		}
	}
}

#[expect(
	clippy::too_many_lines,
	reason = "Bound authority, exact adapter fallback, and terminal publication share one transaction"
)]
fn run_bound(
	bound: BoundRegion,
	config: &OptimizationOptions,
	ledger: &BudgetLedger,
	beam: BeamOptions,
	mut report: BeamReport,
	#[cfg(feature = "workers")] worker: Option<WorkerContext<'_>>,
) -> CompilerResult<BeamRun> {
	let preparation_bytes = bound
		.retained_bytes()?
		.checked_mul(4)
		.and_then(|amount| amount.checked_add(1024 * 1024))
		.ok_or(Error::Budget("beam bound adapter storage"))?;
	let preparation_work = bound
		.instructions()
		.len()
		.checked_mul(8192)
		.ok_or(Error::Budget("beam bound adapter work"))?;
	let preparation = ledger.reserve(
		BudgetCategory::Verification,
		u64::try_from(preparation_work).map_err(|_| Error::Budget("beam bound adapter work"))?,
		u64::try_from(preparation_bytes)
			.map_err(|_| Error::Budget("beam bound adapter storage"))?,
	);
	if let Ok(preparation) = preparation {
		let adapter = bound_as_ideal(&bound)?;
		drop(preparation);
		if let Some(ideal) = adapter {
			let original_bindings = bound.binding_storage().clone();
			let original_source = bound.source_snapshot_id();
			let mut bound = bound;
			bound.retain_source(ideal.snapshot_id(), bound.binding_storage().clone())?;
			let mut result = run_region(
				Box::new(ideal),
				bound,
				config,
				ledger,
				beam,
				report,
				#[cfg(feature = "workers")]
				worker,
			)?;
			if let OptimizerInput::Region { mut bound, .. } = result.input {
				bound.retain_source(original_source, original_bindings)?;
				result.input = OptimizerInput::Bound(bound);
			}
			return Ok(result);
		}
	}
	let (original_score, published, mut rounding, mut allowances, original_status) =
		match score_candidate(&bound, config, ledger) {
			Ok(value) => value,
			Err(Error::Budget(_)) => {
				return Ok(BeamRun {
					input: OptimizerInput::Bound(bound),
					stop_reason: StopReason::WorkLimit,
					evidence: OptimizationEvidence::OriginalInput,
					rounding: RoundingStatus::Unchanged,
					report,
					allowances: Vec::new(),
				});
			}
			Err(error) => return Err(error.into()),
		};
	report.rounds = 1;
	report.generated = 1;
	let mut evidence = OptimizationEvidence::OriginalInput;
	let mut stop_reason = if original_status == TerminalStatus::Unscorable {
		StopReason::Unscorable
	} else {
		StopReason::Complete
	};
	let mut selected = published.map_or_else(
		|| bound.clone(),
		|published| {
			if published.snapshot_id() != bound.snapshot_id() {
				report.admitted = 1;
			}
			published
		},
	);
	let candidate_bytes = bound
		.retained_bytes()?
		.checked_mul(4)
		.and_then(|n| n.checked_add(1024 * 1024))
		.ok_or(Error::Budget("beam bound candidate storage"))?;
	let candidate_lease = match ledger.reserve(
		BudgetCategory::Candidate,
		0,
		u64::try_from(candidate_bytes)
			.map_err(|_| Error::Budget("beam bound candidate storage"))?,
	) {
		Ok(value) => Some(value),
		Err(Error::Budget(_)) => {
			stop_reason = StopReason::StorageLimit;
			None
		}
		Err(error) => return Err(error.into()),
	};
	if let Some(candidate_lease) = candidate_lease {
		let preparation = bound
			.instructions()
			.len()
			.checked_mul(8192)
			.ok_or(Error::Budget("beam bound adapter work"))?;
		let preparation =
			u64::try_from(preparation).map_err(|_| Error::Budget("beam bound adapter work"))?;
		if ledger
			.reserve(BudgetCategory::Verification, preparation, 0)
			.is_err()
		{
			stop_reason = StopReason::WorkLimit;
		} else if let Some(ideal) = bound_as_ideal(&bound)? {
			let (remaining, _) = ledger.remaining()?;
			let maximum = remaining.min(4_000_000);
			if maximum == 0 {
				stop_reason = StopReason::WorkLimit;
			} else {
				let work = ledger.reserve_work_allowance(maximum)?;
				let pass = ideal.optimize_exact_with_options(ExactOptions {
					max_work: usize::try_from(maximum).map_err(|_| Error::Budget("beam work"))?,
					max_bytes: 64 * 1024 * 1024,
				});
				report.generated = report
					.generated
					.checked_add(1)
					.ok_or(Error::Budget("beam candidates"))?;
				match pass {
					Ok((candidate, pass)) => {
						work.commit(
							u64::try_from(pass.work).map_err(|_| Error::Budget("beam work"))?,
						)?;
						if !pass.rewrites.is_empty() {
							let binding_work = candidate.binding_work_estimate()?;
							let allowance = ledger.reserve_work_allowance(binding_work)?;
							let mut rebound = candidate.bind(&[])?;
							allowance.commit(binding_work)?;
							rebound.retain_source(
								bound.source_snapshot_id(),
								bound.binding_storage().clone(),
							)?;
							let (
								cost,
								terminal,
								candidate_rounding,
								mut candidate_allowances,
								candidate_status,
							) = score_candidate(&rebound, config, ledger)?;
							if candidate_status == TerminalStatus::Unscorable {
								stop_reason = StopReason::Unscorable;
							}
							report.admitted = report
								.admitted
								.checked_add(1)
								.ok_or(Error::Budget("beam candidates"))?;
							report.maximum_frontier_operations = report
								.maximum_frontier_operations
								.max(rebound.instructions().len());
							match config.target().compare(&cost, &original_score)? {
								CostComparison::Better => {
									selected = terminal.unwrap_or(rebound);
									rounding = candidate_rounding;
									evidence = OptimizationEvidence::ExactVerified;
									allowances.clear();
									allowances.append(&mut candidate_allowances);
									allowances.push(candidate_lease);
								}
								CostComparison::Unscorable => {
									report.unscorable_comparisons = report
										.unscorable_comparisons
										.checked_add(1)
										.ok_or(Error::Budget("beam comparisons"))?;
									stop_reason = StopReason::Unscorable;
								}
								CostComparison::Equal | CostComparison::Worse => (),
							}
						}
					}
					Err(Error::Budget(_)) => stop_reason = StopReason::WorkLimit,
					Err(error) => return Err(error.into()),
				}
			}
		}
	}
	Ok(BeamRun {
		input: OptimizerInput::Bound(selected),
		stop_reason,
		evidence,
		rounding,
		report,
		allowances,
	})
}

#[cfg(test)]
mod structured_error_tests {
	use super::*;
	use googletest::prelude::*;

	#[gtest]
	fn structured_language_error_keeps_its_specific_cause() {
		let error = structured_error(StructuredTerminalError::Language(
			crate::LanguageError::Budget("source diagnostic"),
		));
		expect_true!(matches!(
			error,
			CompilerError::Structured(inner)
				if matches!(Some(&inner), Some(StructuredTerminalError::Language(
					crate::LanguageError::Budget("source diagnostic")
				)))
		));
	}
}

#[cfg(all(test, feature = "workers"))]
mod tests {
	use super::*;
	use crate::QuantumRegionBuilder;
	use googletest::prelude::*;

	#[gtest]
	fn equal_operations_with_distinct_exact_histories_are_not_deduplicated()
	-> googletest::Result<()> {
		let mut builder = QuantumRegionBuilder::new(1, 0)?;
		let q = builder.qubit(0)?;
		builder.gate(Gate::H, &[q], &[])?;
		let source = builder.finish()?;
		let bound = source.clone().bind(&[])?;
		let occurrence = source.occurrences().first().ok_or(Error::InvalidId)?;
		let sequence = quest_math::Sequence {
			qubits: 1,
			operations: vec![quest_math::Operation {
				gate: quest_math::Gate::H,
				targets: vec![0],
				controls: Vec::new(),
			}],
		};
		let certificate = Arc::new(crate::ExactRegionCertificate {
			occurrences: vec![occurrence.provenance],
			provenance: occurrence.provenance,
			interface: vec![q],
			seed: 0,
			certificate: quest_math::verify_exact(
				&sequence,
				&sequence,
				quest_math::Limits::default(),
			)?,
		});
		let ledger = BudgetLedger::new(crate::OptimizationLimits::default());
		let proof = Arc::new(ledger.reserve(BudgetCategory::Worker, 0, 1024)?);
		let history = || {
			Arc::new(ExactHistory {
				parent: None,
				certificate: Arc::clone(&certificate),
				_proof_allowance: Arc::clone(&proof),
			})
		};
		let score = CostComponents::new(
			NativeCost::new(0, 0, 0, 0, 0, CommunicationCost::known(0)),
			CliffordCost::new(0, 0, 0, 1),
		);
		let make = |history: Arc<ExactHistory>| Candidate {
			source: Box::new(source.clone()),
			bound: bound.clone(),
			score: score.clone(),
			terminal_bound: None,
			rounding: RoundingStatus::Unchanged,
			allowances: Vec::new(),
			expanded: false,
			bound_only: false,
			exact_history: Some(history),
			approx_history: None,
			error_bound: Some(RBig::from(0)),
		};
		let a = make(history());
		let b = make(history());
		expect_true!(same_candidate(&a.source, &b.source, &ledger)?);
		expect_true!(same_bound(&a.bound, &b.bound, &ledger)?);
		expect_false!(same_evidence(&a, &b));
		Ok(())
	}
}
