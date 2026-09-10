use crate::{
    BoundProgram, Control, Error, Gate, Instruction, MatrixPolicy, NumericalOperator, OccurrenceId,
    Operation, QubitId, Result, ValidatedProgram,
    model::{Occurrence, SemanticOperation},
};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteKind {
    Identity,
    InverseCancellation,
    ExactAngleMerge,
    NumericalFusion,
}
#[derive(Debug, Clone)]
pub struct Rewrite {
    pub kind: RewriteKind,
    pub inputs: Vec<OccurrenceId>,
    pub output: Option<OccurrenceId>,
}
#[derive(Debug, Clone)]
pub struct OptimizationReport {
    pub before_operations: usize,
    pub after_operations: usize,
    pub before_depth: usize,
    pub after_depth: usize,
    pub removed: Vec<OccurrenceId>,
    pub rewrites: Vec<Rewrite>,
    pub elapsed: Duration,
    pub matrix_bytes: usize,
    /// Conservative peak payload and temporary matrix allocation during the
    /// pass; shared immutable inputs are counted once per occurrence.
    pub peak_matrix_bytes: usize,
    /// Numeric fusion changes rounding; no certified approximation bound is claimed.
    pub numerical_rounding_changed: bool,
    pub simulator_before: Option<SimulatorCost>,
    pub simulator_after: Option<SimulatorCost>,
}
impl OptimizationReport {
    const fn new(before: usize, depth: usize) -> Self {
        Self {
            before_operations: before,
            after_operations: before,
            before_depth: depth,
            after_depth: depth,
            removed: vec![],
            rewrites: vec![],
            elapsed: Duration::ZERO,
            matrix_bytes: 0,
            peak_matrix_bytes: 0,
            numerical_rounding_changed: false,
            simulator_before: None,
            simulator_after: None,
        }
    }
}

fn identity(op: &SemanticOperation) -> bool {
    match op {
        SemanticOperation::Gate { gate: Gate::Id, .. } => true,
        SemanticOperation::Gate {
            gate: Gate::Rx(a) | Gate::Ry(a) | Gate::Rz(a) | Gate::Phase(a),
            ..
        } => a.is_zero(),
        SemanticOperation::GlobalPhase { angle, .. } => angle.is_zero(),
        _ => false,
    }
}

fn combine(
    a: &SemanticOperation,
    b: &SemanticOperation,
) -> Option<(RewriteKind, Option<SemanticOperation>)> {
    match (a, b) {
        (
            SemanticOperation::Gate {
                gate: x,
                targets: tx,
                controls: cx,
            },
            SemanticOperation::Gate {
                gate: y,
                targets: ty,
                controls: cy,
            },
        ) if tx == ty && cx == cy => {
            if x.angles().iter().all(|a| a.is_exact())
                && y.angles().iter().all(|a| a.is_exact())
                && x.adjoint() == *y
            {
                return Some((RewriteKind::InverseCancellation, None));
            }
            let gate = match (x, y) {
                (Gate::Rx(a), Gate::Rx(b)) => Gate::Rx(a.plus_exact(b)?),
                (Gate::Ry(a), Gate::Ry(b)) => Gate::Ry(a.plus_exact(b)?),
                (Gate::Rz(a), Gate::Rz(b)) => Gate::Rz(a.plus_exact(b)?),
                (Gate::Phase(a), Gate::Phase(b)) => Gate::Phase(a.plus_exact(b)?),
                _ => return None,
            };
            let op = SemanticOperation::Gate {
                gate,
                targets: tx.clone(),
                controls: cx.clone(),
            };
            Some((
                RewriteKind::ExactAngleMerge,
                if identity(&op) { None } else { Some(op) },
            ))
        }
        (
            SemanticOperation::GlobalPhase {
                angle: a,
                controls: ca,
            },
            SemanticOperation::GlobalPhase {
                angle: b,
                controls: cb,
            },
        ) if ca == cb => {
            let angle = a.plus_exact(b)?;
            Some((
                RewriteKind::ExactAngleMerge,
                if angle.is_zero() {
                    None
                } else {
                    Some(SemanticOperation::GlobalPhase {
                        angle,
                        controls: ca.clone(),
                    })
                },
            ))
        }
        _ => None,
    }
}

fn commutes(a: &SemanticOperation, b: &SemanticOperation) -> bool {
    if !matches!(
        a,
        SemanticOperation::Gate { .. } | SemanticOperation::GlobalPhase { .. }
    ) || !matches!(
        b,
        SemanticOperation::Gate { .. } | SemanticOperation::GlobalPhase { .. }
    ) {
        return false;
    }
    if diagonal(a) && diagonal(b) {
        return true;
    }
    let left = a.qubits();
    let right = b.qubits();
    !left.iter().any(|wire| right.contains(wire))
}
const fn diagonal(operation: &SemanticOperation) -> bool {
    matches!(
        operation,
        SemanticOperation::GlobalPhase { .. }
            | SemanticOperation::Gate {
                gate: Gate::Id
                    | Gate::Z
                    | Gate::S
                    | Gate::Sdg
                    | Gate::T
                    | Gate::Tdg
                    | Gate::Rz(_)
                    | Gate::Phase(_),
                ..
            }
    )
}
fn commuting_candidate(
    output: &[Occurrence],
    operation: &SemanticOperation,
) -> Option<(usize, RewriteKind, Option<SemanticOperation>)> {
    // Bounded search prevents quadratic work on very large independent circuits.
    for (index, previous) in output.iter().enumerate().rev().take(128) {
        if let Some((kind, combined)) = combine(&previous.operation, operation) {
            return Some((index, kind, combined));
        }
        if !commutes(&previous.operation, operation) {
            break;
        }
    }
    None
}

impl ValidatedProgram {
    /// Guarded exact algebra with a bounded dependency search. Disjoint symbolic
    /// unitaries and diagonal gates may commute. Effects, barriers and unknown
    /// matrix payloads stop the search. Explicit user order constraints
    /// conservatively disable these rewrites. Successful finite
    /// bindings stay valid. Exact identities may eliminate rational constants
    /// that would otherwise exceed the machine-radian range during binding.
    /// # Errors
    /// Rejects invalid identifiers or cyclic dependencies when rebuilding the optimized program.
    pub fn optimize_exact(self) -> Result<(Self, OptimizationReport)> {
        let start = Instant::now();
        let mut report = OptimizationReport::new(self.occurrences.len(), self.dependency_depth());
        report.matrix_bytes = self.occurrences.iter().try_fold(0usize, |sum, o| {
            let bytes = match &o.operation {
                SemanticOperation::Numerical { matrix, .. } => matrix.bytes(),
                SemanticOperation::Channel { kraus, .. } => {
                    kraus.iter().try_fold(0usize, |sum, k| {
                        sum.checked_add(k.bytes())
                            .ok_or(Error::Budget("program matrices"))
                    })?
                }
                _ => 0,
            };
            sum.checked_add(bytes)
                .ok_or(Error::Budget("program matrices"))
        })?;
        report.peak_matrix_bytes = report.matrix_bytes;
        if !self.explicit_edges.is_empty() {
            report.elapsed = start.elapsed();
            return Ok((self, report));
        }
        let mut output: Vec<Occurrence> = vec![];
        for mut op in self.occurrences {
            if identity(&op.operation) {
                report.removed.extend_from_slice(&op.provenance);
                report.rewrites.push(Rewrite {
                    kind: RewriteKind::Identity,
                    inputs: op.provenance,
                    output: None,
                });
                continue;
            }
            if let Some((index, kind, combined)) = commuting_candidate(&output, &op.operation) {
                // The index comes from this unchanged output slice.
                let previous = output.remove(index);
                let inputs: Vec<_> = previous
                    .provenance
                    .into_iter()
                    .chain(op.provenance)
                    .collect();
                let output_id = combined.as_ref().map(|_| previous.id);
                report.rewrites.push(Rewrite {
                    kind,
                    inputs: inputs.clone(),
                    output: output_id,
                });
                if let Some(operation) = combined {
                    op = Occurrence {
                        id: previous.id,
                        provenance: inputs,
                        source: previous.source,
                        operation,
                    };
                } else {
                    report.removed.extend(inputs);
                    continue;
                }
            }
            output.push(op);
        }
        report.after_operations = output.len();
        report.elapsed = start.elapsed();
        let p = Self::from_parts(
            self.owner,
            self.num_qubits,
            self.num_bits,
            self.parameters,
            output,
            self.explicit_edges,
            self.limits,
        )?;
        report.after_depth = p.dependency_depth();
        report.elapsed = start.elapsed();
        Ok((p, report))
    }
}

/// Heuristic execution model; these scores are not benchmark timings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SimulatorProfile {
    #[default]
    StateVector,
    DensityMatrix,
}
/// Work is normalized per state amplitude or density entry, avoiding exponential
/// register-size arithmetic. Depth and T-count are informational metrics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SimulatorCost {
    pub native_calls: usize,
    pub state_passes: usize,
    pub arithmetic_per_element: usize,
    pub preparation_bytes: usize,
    pub maximum_matrix_qubits: usize,
    pub t_count: usize,
    pub dependency_depth: usize,
}
const fn is_diagonal(operation: &Operation) -> bool {
    match operation {
        Operation::Numerical { matrix, .. } => matrix.is_diagonal(),
        Operation::Gate { gate, .. } => matches!(
            gate,
            crate::BoundGate::Id
                | crate::BoundGate::Z
                | crate::BoundGate::S
                | crate::BoundGate::Sdg
                | crate::BoundGate::T
                | crate::BoundGate::Tdg
                | crate::BoundGate::Phase(_)
                | crate::BoundGate::Rz(_)
        ),
        Operation::GlobalPhase { .. } => true,
        _ => false,
    }
}
fn operation_work(operation: &Operation) -> Result<usize> {
    if is_diagonal(operation) {
        return Ok(1);
    }
    let width = operands(operation).map_or(0, |(targets, _)| targets.len());
    1usize
        .checked_shl(u32::try_from(width).map_err(|_| Error::Budget("cost width"))?)
        .ok_or(Error::Budget("cost width"))
}
fn score(
    work: usize,
    passes: usize,
    preparation: usize,
    profile: SimulatorProfile,
) -> Option<usize> {
    let (traffic_weight, arithmetic_weight) = match profile {
        SimulatorProfile::StateVector => (64usize, 1usize),
        SimulatorProfile::DensityMatrix => (128, 2),
    };
    passes
        .checked_mul(traffic_weight)?
        .checked_add(work.checked_mul(arithmetic_weight)?)?
        .checked_add(preparation / 1024)
}
fn profitable(
    a: &Operation,
    b: &Operation,
    width: usize,
    bytes: usize,
    profile: SimulatorProfile,
) -> bool {
    let Some(original) = operation_work(a)
        .ok()
        .and_then(|a| operation_work(b).ok().and_then(|b| a.checked_add(b)))
        .and_then(|work| score(work, 2, 0, profile))
    else {
        return false;
    };
    let work = if is_diagonal(a) && is_diagonal(b) {
        Some(1)
    } else {
        u32::try_from(width)
            .ok()
            .and_then(|w| 1usize.checked_shl(w))
    };
    work.and_then(|w| score(w, 1, bytes, profile))
        .is_some_and(|candidate| candidate < original)
}

#[derive(Debug, Clone, Copy)]
pub struct FusionOptions {
    pub max_qubits: usize,
    pub max_matrix_bytes: usize,
    pub max_fused_operations: usize,
}
impl Default for FusionOptions {
    fn default() -> Self {
        Self {
            max_qubits: 4,
            max_matrix_bytes: 1024 * 1024,
            max_fused_operations: 32,
        }
    }
}

fn operands(op: &Operation) -> Option<(&[QubitId], &[Control])> {
    match op {
        Operation::Gate {
            targets, controls, ..
        }
        | Operation::Numerical {
            targets, controls, ..
        } => Some((targets, controls)),
        _ => None,
    }
}
fn realize(op: &Operation, policy: MatrixPolicy) -> Result<NumericalOperator> {
    match op {
        Operation::Gate { gate, .. } => gate.matrix(policy),
        Operation::Numerical { matrix, .. } => Ok(matrix.clone()),
        _ => Err(Error::NotUnitary),
    }
}

fn union_interface(
    a: &Operation,
    b: &Operation,
    max_width: usize,
) -> Option<(Vec<QubitId>, Vec<Control>)> {
    let (ta, ca) = operands(a)?;
    let (tb, cb) = operands(b)?;
    let controls = if ca == cb { ca.to_vec() } else { vec![] };
    let mut targets = Vec::new();
    for qubit in ta
        .iter()
        .copied()
        .chain(
            ca.iter()
                .filter(|c| !controls.contains(c))
                .map(|c| c.qubit()),
        )
        .chain(tb.iter().copied())
        .chain(
            cb.iter()
                .filter(|c| !controls.contains(c))
                .map(|c| c.qubit()),
        )
    {
        if !targets.contains(&qubit) {
            if targets.len() >= max_width {
                return None;
            }
            targets.try_reserve(1).ok()?;
            targets.push(qubit);
        }
    }
    Some((targets, controls))
}
fn realize_on(
    op: &Operation,
    targets: &[QubitId],
    retained: &[Control],
    policy: MatrixPolicy,
) -> Result<NumericalOperator> {
    let (original, controls) = operands(op).ok_or(Error::NotUnitary)?;
    let matrix = realize(op, policy)?;
    if original == targets && controls == retained {
        return Ok(matrix);
    }
    let positions = original
        .iter()
        .map(|q| targets.iter().position(|t| t == q).ok_or(Error::InvalidId))
        .collect::<Result<Vec<_>>>()?;
    let controls = controls
        .iter()
        .filter(|c| !retained.contains(c))
        .map(|c| {
            Ok((
                targets
                    .iter()
                    .position(|q| *q == c.qubit())
                    .ok_or(Error::InvalidId)?,
                c.state() == crate::ControlState::One,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    matrix.embedded(&positions, &controls, targets.len(), policy)
}

fn matrix_bytes(operation: &Operation) -> Result<usize> {
    match operation {
        Operation::Numerical { matrix, .. } => Ok(matrix.bytes()),
        Operation::Channel { kraus, .. } => kraus.iter().try_fold(0usize, |sum, k| {
            sum.checked_add(k.bytes())
                .ok_or(Error::Budget("program matrices"))
        }),
        Operation::Conditional { operation, .. } => matrix_bytes(operation),
        _ => Ok(0),
    }
}

impl BoundProgram {
    /// Report hardware-independent estimates; native timing remains empirical.
    /// # Errors
    /// Rejects overflow in aggregate resource/work quantities.
    pub fn simulator_cost(&self, profile: SimulatorProfile) -> Result<SimulatorCost> {
        let mut cost = SimulatorCost {
            dependency_depth: self.dependency_depth(),
            ..SimulatorCost::default()
        };
        for instruction in &self.instructions {
            let operation = &instruction.operation;
            if matches!(operation, Operation::Barrier { .. }) {
                continue;
            }
            cost.native_calls = cost
                .native_calls
                .checked_add(1)
                .ok_or(Error::Budget("native call cost"))?;
            let multiplier = if profile == SimulatorProfile::DensityMatrix {
                2
            } else {
                1
            };
            cost.state_passes = cost
                .state_passes
                .checked_add(multiplier)
                .ok_or(Error::Budget("state traffic cost"))?;
            cost.arithmetic_per_element = operation_work(operation)?
                .checked_mul(multiplier)
                .and_then(|work| cost.arithmetic_per_element.checked_add(work))
                .ok_or(Error::Budget("arithmetic cost"))?;
            cost.preparation_bytes = cost
                .preparation_bytes
                .checked_add(matrix_bytes(operation)?)
                .ok_or(Error::Budget("preparation cost"))?;
            if let Operation::Numerical { matrix, .. } = operation {
                cost.maximum_matrix_qubits = cost.maximum_matrix_qubits.max(matrix.num_qubits());
            }
            if matches!(
                operation,
                Operation::Gate {
                    gate: crate::BoundGate::T | crate::BoundGate::Tdg,
                    ..
                }
            ) {
                cost.t_count = cost
                    .t_count
                    .checked_add(1)
                    .ok_or(Error::Budget("T-count"))?;
            }
        }
        Ok(cost)
    }
    /// Fuse with the default state-vector cost profile.
    /// # Errors
    /// Rejects invalid identifiers and resource-accounting overflow.
    pub fn fuse(self, options: FusionOptions) -> Result<(Self, OptimizationReport)> {
        self.fuse_with_profile(options, SimulatorProfile::StateVector)
    }
    /// Fuse adjacent operations over a bounded union of ordered operands.
    /// Shared controls remain external; differing signed controls are embedded.
    /// Effects and conditionals terminate blocks. The numerical
    /// result carries all original occurrence identities as provenance.
    /// # Errors
    /// Rejects invalid retained identifiers or resource-accounting overflow; configured fusion limits skip candidates.
    #[expect(
        clippy::too_many_lines,
        reason = "Fusion admission and retained-memory accounting remain visible in one transaction"
    )]
    pub fn fuse_with_profile(
        mut self,
        options: FusionOptions,
        profile: SimulatorProfile,
    ) -> Result<(Self, OptimizationReport)> {
        let start = Instant::now();
        let mut report = OptimizationReport::new(self.instructions.len(), self.dependency_depth());
        report.simulator_before = Some(self.simulator_cost(profile)?);
        let policy = MatrixPolicy {
            max_bytes: options.max_matrix_bytes.min(self.limits.max_matrix_bytes),
        };
        // Counts all retained input and output payloads, including operations
        // not visited yet. Shared payloads are conservatively counted per use.
        let mut resident_bytes = self.instructions.iter().try_fold(0usize, |sum, i| {
            sum.checked_add(matrix_bytes(&i.operation)?)
                .ok_or(Error::Budget("program matrices"))
        })?;
        report.peak_matrix_bytes = resident_bytes;
        let mut output: Vec<Instruction> = vec![];
        let mut block_len = 0usize;
        for instruction in self.instructions {
            let candidate = output.last().and_then(|previous| {
                if block_len >= options.max_fused_operations {
                    return None;
                }
                union_interface(
                    &previous.operation,
                    &instruction.operation,
                    options.max_qubits,
                )
            });
            if let Some((targets, controls)) = candidate {
                let dim = 1usize
                    .checked_shl(
                        u32::try_from(targets.len()).map_err(|_| Error::Budget("fusion width"))?,
                    )
                    .ok_or(Error::Budget("fusion width"))?;
                // Admission failure due to configured width/memory is a skipped
                // optimization, not a failure of the already valid program.
                let previous = output.last().ok_or(Error::InvalidId)?;
                let previous_bytes = matrix_bytes(&previous.operation)?;
                let instruction_bytes = matrix_bytes(&instruction.operation)?;
                let same_interface = operands(&previous.operation)
                    == Some((targets.as_slice(), controls.as_slice()))
                    && operands(&instruction.operation)
                        == Some((targets.as_slice(), controls.as_slice()));
                let temporaries = if same_interface {
                    match (previous_bytes == 0, instruction_bytes == 0) {
                        (true, true) => 3,
                        (false, false) => 1,
                        _ => 2,
                    }
                } else {
                    4
                }; // Two embedded matrices, result, and a base-gate realization.
                let admitted = policy.check(dim, temporaries.max(3)).ok().filter(|bytes| {
                    bytes
                        .checked_mul(temporaries)
                        .and_then(|extra| resident_bytes.checked_add(extra))
                        .is_some_and(|peak| peak <= self.limits.max_matrix_bytes)
                });
                if let Some(result_bytes) = admitted.filter(|bytes| {
                    profitable(
                        &previous.operation,
                        &instruction.operation,
                        targets.len(),
                        *bytes,
                        profile,
                    )
                }) {
                    report.peak_matrix_bytes = report.peak_matrix_bytes.max(
                        result_bytes
                            .checked_mul(temporaries)
                            .and_then(|extra| resident_bytes.checked_add(extra))
                            .ok_or(Error::Budget("fusion peak"))?,
                    );
                    let left = realize_on(&instruction.operation, &targets, &controls, policy)?;
                    let right = realize_on(&previous.operation, &targets, &controls, policy)?;
                    // Numeric overflow does not invalidate the unfused input.
                    let matrix = match left.product(&right, policy) {
                        Ok(matrix) => matrix,
                        Err(Error::NonFinite | Error::Budget(_)) => {
                            output.push(instruction);
                            block_len = 1;
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                    resident_bytes = resident_bytes
                        .checked_sub(previous_bytes)
                        .and_then(|bytes| bytes.checked_sub(instruction_bytes))
                        .and_then(|bytes| bytes.checked_add(result_bytes))
                        .ok_or(Error::Budget("fusion retained matrices"))?;
                    let previous = output.pop().ok_or(Error::InvalidId)?;
                    let provenance: Vec<_> = previous
                        .provenance
                        .into_iter()
                        .chain(instruction.provenance)
                        .collect();
                    report.rewrites.push(Rewrite {
                        kind: RewriteKind::NumericalFusion,
                        inputs: provenance.clone(),
                        output: Some(previous.id),
                    });
                    report.numerical_rounding_changed = true;
                    output.push(Instruction {
                        id: previous.id,
                        provenance,
                        source: previous.source,
                        operation: Operation::Numerical {
                            matrix,
                            targets,
                            controls,
                        },
                    });
                    block_len = block_len
                        .checked_add(1)
                        .ok_or(Error::Budget("fusion block length"))?;
                    continue;
                }
            }
            output.push(instruction);
            block_len = 1;
        }
        report.matrix_bytes = resident_bytes;
        let representatives: std::collections::BTreeMap<_, _> = output
            .iter()
            .flat_map(|i| i.provenance.iter().map(move |id| (*id, i.id)))
            .collect();
        self.dependencies = self
            .dependencies
            .into_iter()
            .map(|(a, b)| {
                let a = *representatives.get(&a).ok_or(Error::InvalidId)?;
                let b = *representatives.get(&b).ok_or(Error::InvalidId)?;
                Ok((a, b))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|(a, b)| a != b)
            .collect();
        report.after_operations = output.len();
        self.instructions = output;
        report.simulator_after = Some(self.simulator_cost(profile)?);
        report.after_depth = self.dependency_depth();
        report.elapsed = start.elapsed();
        Ok((self, report))
    }
}
