//! Bounded commutation proofs and terminal numerical fusion.
use crate::{
    BoundGate, BoundProgram, BoundSnapshotId, BudgetCategory, BudgetLease, BudgetLedger,
    CliffordCost, Control, CostComparison, CostComponents, CostProfile, DependencyEdge,
    DependencyKind, Error, Instruction, MatrixPolicy, NativeCost, OccurrenceId, Operation,
    OptimizationTarget, ProvenanceGraph, QubitId, Result, validate_mandatory_projection,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Constructor-validated ceilings for one deterministic terminal pass.
#[derive(Debug, Clone, Copy)]
pub struct TerminalOptions {
    window: usize,
    matrix_qubits: usize,
    cluster: usize,
    matrix_bytes: usize,
    provenance_bytes: usize,
}
impl Default for TerminalOptions {
    fn default() -> Self {
        Self {
            window: 64,
            matrix_qubits: 4,
            cluster: 32,
            matrix_bytes: 1024 * 1024,
            provenance_bytes: 64 * 1024 * 1024,
        }
    }
}
impl TerminalOptions {
    /// # Errors
    /// Rejects zero or above-ceiling limits before any search or allocation.
    pub fn new(
        window: usize,
        matrix_qubits: usize,
        cluster: usize,
        matrix_bytes: usize,
        provenance_bytes: usize,
    ) -> Result<Self> {
        let hard = Self::default();
        if window == 0
            || window > hard.window
            || matrix_qubits == 0
            || matrix_qubits > hard.matrix_qubits
            || cluster == 0
            || cluster > hard.cluster
            || matrix_bytes == 0
            || matrix_bytes > hard.matrix_bytes
            || provenance_bytes == 0
            || provenance_bytes > hard.provenance_bytes
        {
            return Err(Error::Budget("terminal options"));
        }
        Ok(Self {
            window,
            matrix_qubits,
            cluster,
            matrix_bytes,
            provenance_bytes,
        })
    }
}

/// A proposal tied to one immutable bound input; no executable is published yet.
#[derive(Debug)]
pub struct CommutationSchedule {
    snapshot: BoundSnapshotId,
    order: Vec<OccurrenceId>,
    _allowance: BudgetLease,
}
impl CommutationSchedule {
    #[must_use]
    pub const fn snapshot_id(&self) -> BoundSnapshotId {
        self.snapshot
    }
    #[must_use]
    pub fn order(&self) -> &[OccurrenceId] {
        &self.order
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalStatus {
    Complete,
    WorkLimit,
    StorageLimit,
    Unscorable,
}
/// Scores use identical reuse, deployment and terminal treatment for both orders.
#[derive(Debug)]
pub struct TerminalReport {
    status: TerminalStatus,
    original: Option<CostComponents>,
    baseline: Option<CostComponents>,
    published: Option<CostComponents>,
    reordered: bool,
    fusions: usize,
}
impl TerminalReport {
    #[must_use]
    pub const fn status(&self) -> TerminalStatus {
        self.status
    }
    #[must_use]
    pub const fn original_cost(&self) -> Option<&CostComponents> {
        self.original.as_ref()
    }
    #[must_use]
    pub const fn baseline_terminal_cost(&self) -> Option<&CostComponents> {
        self.baseline.as_ref()
    }
    #[must_use]
    pub const fn published_cost(&self) -> Option<&CostComponents> {
        self.published.as_ref()
    }
    #[must_use]
    pub const fn reordered(&self) -> bool {
        self.reordered
    }
    #[must_use]
    pub const fn rounding_changed(&self) -> bool {
        self.fusions != 0
    }
    #[must_use]
    pub const fn fusions(&self) -> usize {
        self.fusions
    }
}
#[derive(Debug)]
pub struct TerminalOutcome {
    program: BoundProgram,
    report: TerminalReport,
    allowances: Vec<BudgetLease>,
}
impl TerminalOutcome {
    #[must_use]
    pub const fn program(&self) -> &BoundProgram {
        &self.program
    }
    #[must_use]
    pub const fn report(&self) -> &TerminalReport {
        &self.report
    }
    #[must_use]
    pub fn into_program(self) -> BoundProgram {
        self.program
    }
    pub(crate) fn into_parts(self) -> (BoundProgram, Vec<BudgetLease>) {
        (self.program, self.allowances)
    }
}

fn as_u64(value: usize) -> Result<u64> {
    u64::try_from(value).map_err(|_| Error::Budget("terminal arithmetic"))
}
fn charge(ledger: &BudgetLedger, work: usize) -> Result<()> {
    ledger.reserve(BudgetCategory::Verification, as_u64(work)?, 0)?;
    Ok(())
}
fn gate_interface(op: &Operation) -> Option<(&[QubitId], &[Control])> {
    match op {
        Operation::Gate {
            targets, controls, ..
        } => Some((targets, controls)),
        Operation::GlobalPhase { controls, .. } => Some((&[], controls)),
        _ => None,
    }
}
fn wires(op: &Operation) -> impl Iterator<Item = QubitId> + '_ {
    let (targets, controls) = gate_interface(op).unwrap_or((&[], &[]));
    targets
        .iter()
        .copied()
        .chain(controls.iter().map(|c| c.qubit()))
}
const fn diagonal(op: &Operation) -> bool {
    matches!(
        op,
        Operation::GlobalPhase { .. }
            | Operation::Gate {
                gate: BoundGate::Id
                    | BoundGate::Z
                    | BoundGate::S
                    | BoundGate::Sdg
                    | BoundGate::T
                    | BoundGate::Tdg
                    | BoundGate::Rz(_)
                    | BoundGate::Phase(_),
                ..
            }
    )
}
const fn axis(gate: &BoundGate) -> Option<u8> {
    match gate {
        BoundGate::X | BoundGate::Rx(_) | BoundGate::Sx | BoundGate::Sxdg => Some(0),
        BoundGate::Y | BoundGate::Ry(_) => Some(1),
        BoundGate::Z
        | BoundGate::Rz(_)
        | BoundGate::S
        | BoundGate::Sdg
        | BoundGate::T
        | BoundGate::Tdg
        | BoundGate::Phase(_) => Some(2),
        _ => None,
    }
}
fn commute(a: &Operation, b: &Operation, ledger: &BudgetLedger) -> Result<bool> {
    let (Some((ta, ca)), Some((tb, cb))) = (gate_interface(a), gate_interface(b)) else {
        return Ok(false);
    };
    let left = ta
        .len()
        .checked_add(ca.len())
        .and_then(|v| v.checked_add(1))
        .ok_or(Error::Budget("commutation work"))?;
    let right = tb
        .len()
        .checked_add(cb.len())
        .and_then(|v| v.checked_add(1))
        .ok_or(Error::Budget("commutation work"))?;
    charge(
        ledger,
        left.checked_mul(right)
            .and_then(|n| n.checked_mul(8))
            .ok_or(Error::Budget("commutation work"))?,
    )?;
    if diagonal(a) && diagonal(b) || !wires(a).any(|x| wires(b).any(|y| x == y)) {
        return Ok(true);
    }
    if let (Operation::Gate { gate: ga, .. }, Operation::Gate { gate: gb, .. }) = (a, b) {
        if matches!(ga, BoundGate::Id) || matches!(gb, BoundGate::Id) {
            return Ok(true);
        }
        if ta == tb && ca == cb && axis(ga).is_some() && axis(ga) == axis(gb) {
            return Ok(true);
        }
    }
    // Orthogonal computational projectors commute only when neither payload
    // changes a projector wire. Cross target/control overlap invalidates proof.
    Ok(!ta.iter().any(|q| cb.iter().any(|c| c.qubit() == *q))
        && !tb.iter().any(|q| ca.iter().any(|c| c.qubit() == *q))
        && ca.iter().any(|a| {
            cb.iter()
                .any(|b| a.qubit() == b.qubit() && a.state() != b.state())
        }))
}
fn affinity(a: &Operation, b: &Operation) -> u8 {
    match (gate_interface(a), gate_interface(b)) {
        (Some((ta, ca)), Some((tb, cb))) if ta == tb && ca == cb => 0,
        (Some((ta, _)), Some((tb, _))) if ta == tb => 1,
        _ => 2,
    }
}

impl BoundProgram {
    /// Build and independently check a bounded all-pair commutation proposal.
    /// Original numerical matrices, oracle calls and effects are boundaries.
    /// # Errors
    /// Rejects exhausted shared resources or inconsistent ordering evidence.
    #[expect(
        clippy::too_many_lines,
        reason = "Admission, all-pair precedence, and independent inversion verification form one bounded transaction"
    )]
    #[expect(
        clippy::indexing_slicing,
        reason = "All indices are constructed from a window bounded by the private 64-operation ceiling"
    )]
    pub fn commutation_schedule(
        &self,
        options: TerminalOptions,
        ledger: &BudgetLedger,
    ) -> Result<CommutationSchedule> {
        let count = self.instructions.len();
        let bytes = count
            .checked_mul(256)
            .and_then(|n| {
                self.dependencies
                    .len()
                    .checked_mul(128)
                    .and_then(|e| n.checked_add(e))
            })
            .and_then(|n| n.checked_add(16 * 1024))
            .ok_or(Error::Budget("schedule storage"))?;
        let allowance =
            ledger.reserve(BudgetCategory::Candidate, as_u64(count)?, as_u64(bytes)?)?;
        charge(
            ledger,
            self.dependencies
                .len()
                .checked_mul(64)
                .ok_or(Error::Budget("schedule work"))?,
        )?;
        let mandatory = self
            .dependencies
            .iter()
            .filter(|e| e.kind != DependencyKind::Quantum)
            .map(|e| (e.before, e.after))
            .collect::<BTreeSet<_>>();
        let mut order = Vec::new();
        order
            .try_reserve_exact(count)
            .map_err(|_| Error::Budget("schedule allocation"))?;
        let mut start = 0usize;
        while start < count {
            if gate_interface(&self.instructions[start].operation).is_none() {
                order.push(self.instructions[start].id);
                start = start
                    .checked_add(1)
                    .ok_or(Error::Budget("schedule index"))?;
                continue;
            }
            let end = start.saturating_add(options.window).min(count);
            let end = (start..end)
                .find(|i| gate_interface(&self.instructions[*i].operation).is_none())
                .unwrap_or(end);
            let window = &self.instructions[start..end];
            let operand_visits = window.iter().try_fold(0usize, |sum, item| {
                let (targets, controls) =
                    gate_interface(&item.operation).ok_or(Error::InvalidId)?;
                sum.checked_add(targets.len())
                    .and_then(|n| n.checked_add(controls.len()))
                    .and_then(|n| n.checked_add(1))
                    .ok_or(Error::Budget("schedule work"))
            })?;
            charge(
                ledger,
                operand_visits
                    .checked_mul(window.len())
                    .and_then(|n| n.checked_mul(8))
                    .ok_or(Error::Budget("schedule work"))?,
            )?;
            let mut edges = [[false; 64]; 64];
            let mut degree = [0usize; 64];
            for later in 0..window.len() {
                for earlier in 0..later {
                    if mandatory.contains(&(window[earlier].id, window[later].id))
                        || !commute(&window[earlier].operation, &window[later].operation, ledger)?
                    {
                        edges[earlier][later] = true;
                        degree[later] = degree[later]
                            .checked_add(1)
                            .ok_or(Error::Budget("schedule degree"))?;
                    }
                }
            }
            let mut used = [false; 64];
            let mut positions = [0usize; 64];
            let mut previous = None::<usize>;
            for position in 0..window.len() {
                charge(
                    ledger,
                    window
                        .len()
                        .checked_mul(8)
                        .ok_or(Error::Budget("schedule work"))?,
                )?;
                let choice = (0..window.len())
                    .filter(|i| !used[*i] && degree[*i] == 0)
                    .min_by_key(|i| {
                        (
                            previous.map_or(0, |p| {
                                affinity(&window[p].operation, &window[*i].operation)
                            }),
                            *i,
                        )
                    })
                    .ok_or(Error::Cycle)?;
                used[choice] = true;
                positions[choice] = position;
                order.push(window[choice].id);
                previous = Some(choice);
                for successor in 0..window.len() {
                    if edges[choice][successor] {
                        degree[successor] = degree[successor].checked_sub(1).ok_or(Error::Cycle)?;
                    }
                }
            }
            // Check inversions independently of the ready queue and edge table.
            for later in 0..window.len() {
                for earlier in 0..later {
                    if positions[earlier] > positions[later]
                        && (mandatory.contains(&(window[earlier].id, window[later].id))
                            || !commute(
                                &window[earlier].operation,
                                &window[later].operation,
                                ledger,
                            )?)
                    {
                        return Err(Error::Unsupported("invalid commutation proposal"));
                    }
                }
            }
            start = end;
        }
        Ok(CommutationSchedule {
            snapshot: self.snapshot_id,
            order,
            _allowance: allowance,
        })
    }

    /// Compare original order and commutation order with identical terminal fusion.
    /// Only a strict Native V1 improvement is published. Numerical fusion changes
    /// rounding and carries no mathematical approximation certificate.
    /// # Errors
    /// Rejects invalid deployment or input admission; search exhaustion returns
    /// the best fully admitted result with an incomplete status.
    pub fn schedule_and_fuse(
        self,
        options: TerminalOptions,
        target: &OptimizationTarget,
        ledger: &BudgetLedger,
    ) -> Result<TerminalOutcome> {
        if target.deployment().width() != self.num_qubits {
            return Err(Error::Budget("terminal target width"));
        }
        self.schedule_and_fuse_embedded(options, target, ledger)
    }
    pub(crate) fn schedule_and_fuse_embedded(
        self,
        options: TerminalOptions,
        target: &OptimizationTarget,
        ledger: &BudgetLedger,
    ) -> Result<TerminalOutcome> {
        if self.num_qubits > target.deployment().width() {
            return Err(Error::Budget("terminal embedded width"));
        }
        let retained = self.retained_bytes()?;
        let input = ledger.reserve(BudgetCategory::Candidate, 0, as_u64(retained)?)?;
        let mut outcome = TerminalOutcome {
            program: self,
            report: TerminalReport {
                status: TerminalStatus::Complete,
                original: None,
                baseline: None,
                published: None,
                reordered: false,
                fusions: 0,
            },
            allowances: vec![input],
        };
        if target.profile() != CostProfile::NativeV1 {
            outcome.report.status = TerminalStatus::Unscorable;
            return Ok(outcome);
        }
        let result = terminal_search(&mut outcome, options, target, ledger);
        match result {
            Ok(()) => (),
            Err(Error::Budget(reason)) => {
                outcome.report.status = if reason.contains("work") {
                    TerminalStatus::WorkLimit
                } else {
                    TerminalStatus::StorageLimit
                }
            }
            Err(Error::Unsupported(_)) => outcome.report.status = TerminalStatus::Unscorable,
            Err(error) => return Err(error),
        }
        Ok(outcome)
    }
}

fn cost(
    program: &BoundProgram,
    target: &OptimizationTarget,
    ledger: &BudgetLedger,
) -> Result<CostComponents> {
    charge(
        ledger,
        program
            .instructions
            .len()
            .checked_add(program.dependencies.len())
            .and_then(|n| n.checked_mul(64))
            .ok_or(Error::Budget("cost work"))?,
    )?;
    let plan = program.clone().plan()?;
    Ok(CostComponents::new(
        NativeCost::from_embedded_plan(&plan, target.deployment(), ledger, 0)?,
        CliffordCost::new(0, 0, 0, 0),
    ))
}

fn terminal_search(
    outcome: &mut TerminalOutcome,
    options: TerminalOptions,
    target: &OptimizationTarget,
    ledger: &BudgetLedger,
) -> Result<()> {
    // Cover simultaneous original, baseline, candidate, cloned plan, maps and
    // provenance before creating them. Matrix temporaries have their own cap.
    let operands = outcome
        .program
        .instructions
        .iter()
        .try_fold(0usize, |sum, item| {
            sum.checked_add(item.operation.qubits().count())
                .ok_or(Error::Budget("terminal operands"))
        })?;
    let operand_bytes = operands
        .checked_mul(256)
        .ok_or(Error::Budget("terminal operand storage"))?;
    let extra = outcome
        .program
        .retained_bytes()?
        .checked_mul(4)
        .and_then(|n| {
            outcome
                .program
                .instructions
                .len()
                .checked_mul(1024)
                .and_then(|e| n.checked_add(e))
        })
        .and_then(|n| {
            options
                .matrix_bytes
                .checked_mul(2)
                .and_then(|m| n.checked_add(m))
        })
        .and_then(|n| n.checked_add(operand_bytes))
        .ok_or(Error::Budget("terminal storage"))?;
    let allowance = ledger.reserve(BudgetCategory::Candidate, 0, as_u64(extra)?)?;
    outcome.allowances.push(allowance);
    let original_cost = cost(&outcome.program, target, ledger)?;
    outcome.report.original = Some(original_cost.clone());
    outcome.report.published = Some(original_cost.clone());
    let original = outcome.program.clone();
    let order = original
        .instructions
        .iter()
        .map(|i| i.id)
        .collect::<Vec<_>>();
    let (baseline, fusions, unscorable) = fuse_order(&original, &order, options, target, ledger)?;
    if unscorable {
        outcome.report.status = TerminalStatus::Unscorable;
    }
    let baseline_cost = cost(&baseline, target, ledger)?;
    outcome.report.baseline = Some(baseline_cost.clone());
    let mut best_cost = original_cost;
    let comparison = target.compare(&baseline_cost, &best_cost)?;
    if comparison == CostComparison::Unscorable {
        outcome.report.status = TerminalStatus::Unscorable;
    }
    if comparison == CostComparison::Better {
        best_cost = baseline_cost;
        outcome.program = baseline;
        outcome.report.fusions = fusions;
        outcome.report.published = Some(best_cost.clone());
    }
    let schedule = original.commutation_schedule(options, ledger)?;
    if schedule.order == order {
        return Ok(());
    }
    let (candidate, fusions, unscorable) =
        fuse_order(&original, &schedule.order, options, target, ledger)?;
    if unscorable {
        outcome.report.status = TerminalStatus::Unscorable;
    }
    let candidate_cost = cost(&candidate, target, ledger)?;
    let comparison = target.compare(&candidate_cost, &best_cost)?;
    if comparison == CostComparison::Unscorable {
        outcome.report.status = TerminalStatus::Unscorable;
    }
    if comparison == CostComparison::Better {
        outcome.program = candidate;
        outcome.report.fusions = fusions;
        outcome.report.reordered = true;
        outcome.report.published = Some(candidate_cost);
    }
    Ok(())
}

fn pair_cost(
    source: &BoundProgram,
    instructions: Vec<Instruction>,
    target: &OptimizationTarget,
    ledger: &BudgetLedger,
) -> Result<CostComponents> {
    let program = BoundProgram {
        num_qubits: source.num_qubits,
        num_bits: source.num_bits,
        source_snapshot_id: source.source_snapshot_id,
        snapshot_id: source.snapshot_id,
        instructions,
        bindings: BTreeMap::new(),
        limits: source.limits,
        dependencies: vec![],
        provenance: Arc::clone(&source.provenance),
    };
    cost(&program, target, ledger)
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep terminal replacement, provenance, storage and mandatory dependency publication in one transaction"
)]
fn fuse_order(
    source: &BoundProgram,
    order: &[OccurrenceId],
    options: TerminalOptions,
    target: &OptimizationTarget,
    ledger: &BudgetLedger,
) -> Result<(BoundProgram, usize, bool)> {
    charge(
        ledger,
        source
            .provenance
            .copy_work()?
            .checked_add(
                order
                    .len()
                    .checked_mul(256)
                    .ok_or(Error::Budget("fusion work"))?,
            )
            .ok_or(Error::Budget("fusion work"))?,
    )?;
    let mut program = source.clone();
    let by_id = source
        .instructions
        .iter()
        .map(|i| (i.id, i))
        .collect::<BTreeMap<_, _>>();
    let protected = source
        .dependencies
        .iter()
        .filter(|e| e.kind != DependencyKind::Quantum)
        .flat_map(|e| [e.before, e.after])
        .collect::<BTreeSet<_>>();
    let mut mapping = source
        .instructions
        .iter()
        .map(|i| (i.id, Some(i.id)))
        .collect::<BTreeMap<_, _>>();
    let cap = options
        .provenance_bytes
        .min(source.limits.max_provenance_bytes);
    let mut provenance = ProvenanceGraph::edit(Arc::clone(&source.provenance), cap)?;
    let mut next_occurrence = provenance.next_occurrence();
    let mut output: Vec<Instruction> = Vec::new();
    output
        .try_reserve_exact(order.len())
        .map_err(|_| Error::Budget("fusion allocation"))?;
    let mut cluster = 0usize;
    let mut window = 0usize;
    let mut matrix_bytes = 0usize;
    let mut fusions = 0usize;
    let mut unscorable = false;
    for id in order {
        let instruction = *by_id.get(id).ok_or(Error::InvalidId)?;
        let eligible =
            matches!(instruction.operation, Operation::Gate { .. }) && !protected.contains(id);
        if !eligible || window >= options.window {
            cluster = 0;
            window = 0;
        }
        window = window
            .checked_add(1)
            .ok_or(Error::Budget("fusion window"))?;
        if eligible && cluster > 0 && cluster < options.cluster {
            let previous = output.last().ok_or(Error::InvalidId)?;
            if let Some((targets, controls)) = crate::optimize::union_interface(
                &previous.operation,
                &instruction.operation,
                options.matrix_qubits,
            ) {
                let effective = targets
                    .len()
                    .checked_add(controls.len())
                    .ok_or(Error::Budget("fusion width"))?;
                if effective <= options.matrix_qubits {
                    let dim = 1usize
                        .checked_shl(
                            u32::try_from(targets.len()).map_err(|_| Error::MatrixDimension)?,
                        )
                        .ok_or(Error::MatrixDimension)?;
                    let policy = MatrixPolicy {
                        max_bytes: options.matrix_bytes,
                    };
                    if let Ok(bytes) = policy.check(dim, 4) {
                        let peak = bytes
                            .checked_mul(4)
                            .and_then(|n| n.checked_add(matrix_bytes))
                            .ok_or(Error::Budget("fusion matrix storage"))?;
                        if peak <= options.matrix_bytes {
                            let work = dim
                                .checked_mul(dim)
                                .and_then(|n| n.checked_mul(dim))
                                .and_then(|n| n.checked_mul(16))
                                .ok_or(Error::Budget("fusion matrix work"))?;
                            charge(ledger, work)?;
                            let later = crate::optimize::realize_on(
                                &instruction.operation,
                                &targets,
                                &controls,
                                policy,
                            )?;
                            let earlier = crate::optimize::realize_on(
                                &previous.operation,
                                &targets,
                                &controls,
                                policy,
                            )?;
                            let matrix = later.product(&earlier, policy)?;
                            let replacement = Instruction {
                                id: previous.id,
                                provenance: previous.provenance,
                                source: previous.source.clone(),
                                angle_targets: Arc::from([]),
                                operation: Operation::Numerical {
                                    matrix,
                                    targets: targets.into(),
                                    controls: controls.into(),
                                },
                            };
                            let before = pair_cost(
                                source,
                                vec![previous.clone(), instruction.clone()],
                                target,
                                ledger,
                            )?;
                            let after =
                                pair_cost(source, vec![replacement.clone()], target, ledger)?;
                            let comparison = target.compare(&after, &before)?;
                            unscorable |= comparison == CostComparison::Unscorable;
                            if comparison == CostComparison::Better {
                                charge(ledger, mapping.len())?;
                                let mut replacement = replacement;
                                replacement.id = OccurrenceId {
                                    owner: previous.id.owner,
                                    index: next_occurrence,
                                };
                                next_occurrence = next_occurrence
                                    .checked_add(1)
                                    .ok_or(Error::Budget("fusion occurrence identity"))?;
                                replacement.provenance = provenance
                                    .rewrite(&[previous.provenance, instruction.provenance], cap)?;
                                for (original, mapped) in &mut mapping {
                                    if *mapped == Some(previous.id) || original == id {
                                        *mapped = Some(replacement.id);
                                    }
                                }
                                let old_bytes = match &previous.operation {
                                    Operation::Numerical { matrix, .. } => matrix.bytes(),
                                    _ => 0,
                                };
                                matrix_bytes = matrix_bytes
                                    .checked_sub(old_bytes)
                                    .and_then(|n| n.checked_add(bytes))
                                    .ok_or(Error::Budget("fusion matrix storage"))?;
                                output.pop();
                                output.push(replacement);
                                cluster = cluster
                                    .checked_add(1)
                                    .ok_or(Error::Budget("fusion cluster"))?;
                                fusions = fusions
                                    .checked_add(1)
                                    .ok_or(Error::Budget("fusion count"))?;
                                continue;
                            }
                        }
                    }
                }
            }
        }
        output.push(instruction.clone());
        cluster = usize::from(eligible);
    }
    let mut dependencies = source
        .dependencies
        .iter()
        .filter(|e| e.kind != DependencyKind::Quantum)
        .copied()
        .collect::<Vec<_>>();
    let mut last = BTreeMap::new();
    for instruction in &output {
        for wire in instruction.operation.qubits() {
            if let Some(before) = last.insert(wire, instruction.id) {
                dependencies.push(DependencyEdge {
                    before,
                    after: instruction.id,
                    kind: DependencyKind::Quantum,
                });
            }
        }
    }
    validate_mandatory_projection(
        &source.dependencies,
        &mapping.into_iter().collect::<Vec<_>>(),
        &dependencies,
        ledger,
    )?;
    program.instructions = output;
    program.dependencies = dependencies;
    provenance.retain_next_occurrence(next_occurrence);
    program.provenance = Arc::new(provenance);
    let changed = fusions != 0
        || order
            .iter()
            .copied()
            .ne(source.instructions.iter().map(|i| i.id));
    if changed {
        program.snapshot_id = crate::program::fresh_bound_snapshot_id()?;
    }
    program.clone().plan()?;
    Ok((program, fusions, unscorable))
}
