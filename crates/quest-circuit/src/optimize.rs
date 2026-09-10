use crate::{
    model::{Occurrence, SemanticOperation},
    *,
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
}
impl OptimizationReport {
    fn new(before: usize, depth: usize) -> Self {
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

impl ValidatedProgram {
    /// Conservative adjacent exact algebra. No operation is commuted across an
    /// effect, barrier, or differently ordered operand list. Explicit user order
    /// constraints conservatively disable these rewrites. Successful finite
    /// bindings stay valid. Exact identities may eliminate rational constants
    /// that would otherwise exceed the machine-radian range during binding.
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
            if let Some(previous) = output.last()
                && let Some((kind, combined)) = combine(&previous.operation, &op.operation)
            {
                let previous = output.pop().ok_or(Error::InvalidId)?;
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
    /// Fuse only adjacent operations with identical ordered target/control
    /// interfaces. Effects and conditionals terminate blocks. The numerical
    /// result carries all original occurrence identities as provenance.
    pub fn fuse(mut self, options: FusionOptions) -> Result<(Self, OptimizationReport)> {
        let start = Instant::now();
        let mut report = OptimizationReport::new(self.instructions.len(), self.dependency_depth());
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
                let (a, ca) = operands(&previous.operation)?;
                let (b, cb) = operands(&instruction.operation)?;
                if a == b
                    && ca == cb
                    && a.len() <= options.max_qubits
                    && block_len < options.max_fused_operations
                {
                    Some((a.to_vec(), ca.to_vec()))
                } else {
                    None
                }
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
                let temporaries =
                    1 + usize::from(previous_bytes == 0) + usize::from(instruction_bytes == 0);
                let admitted = policy.check(dim, 3).ok().filter(|bytes| {
                    bytes
                        .checked_mul(temporaries)
                        .and_then(|extra| resident_bytes.checked_add(extra))
                        .is_some_and(|peak| peak <= self.limits.max_matrix_bytes)
                });
                if let Some(result_bytes) = admitted {
                    report.peak_matrix_bytes = report
                        .peak_matrix_bytes
                        .max(resident_bytes + result_bytes * temporaries);
                    let left = realize(&instruction.operation, policy)?;
                    let right = realize(&previous.operation, policy)?;
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
                    resident_bytes =
                        resident_bytes - previous_bytes - instruction_bytes + result_bytes;
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
                    block_len += 1;
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
            .filter_map(|(a, b)| {
                let a = representatives[&a];
                let b = representatives[&b];
                (a != b).then_some((a, b))
            })
            .collect();
        report.after_operations = output.len();
        self.instructions = output;
        report.after_depth = self.dependency_depth();
        report.elapsed = start.elapsed();
        Ok((self, report))
    }
}
