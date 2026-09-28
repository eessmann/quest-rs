//! Optional candidate replacements. Certificates are owned independently of native resources.
use crate::model::{Occurrence, SemanticOperation};
use crate::{
    Angle, BoundAngleTarget, Control, ControlState, Error, Gate, OccurrenceId, QubitId,
    ValidatedProgram,
};
use crate::{ProvenanceGraph, ProvenanceId};
use num_traits::ToPrimitive;
use quest_math::{
    AngleTarget, ApproxCertificate, Axis, ExactCertificate, Limits, Rational, Sequence, Target,
};
use quest_optimizer_client::Client;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error(transparent)]
    Circuit(#[from] Error),
    #[error(transparent)]
    Worker(#[from] quest_optimizer_client::Error),
    #[error(transparent)]
    Mathematics(#[from] quest_math::Error),
    #[error("worker transformation resource limit: {0}")]
    Budget(&'static str),
    #[error("explicit ordering constraints are not supported by this worker transformation")]
    Ordering,
}
/// Certificate for one source occurrence, with its physical ordered interface.
#[derive(Debug, Clone)]
pub struct RotationCertificate {
    pub occurrence: OccurrenceId,
    pub input: ProvenanceId,
    pub provenance: ProvenanceId,
    pub targets: Arc<[QubitId]>,
    pub controls: Arc<[Control]>,
    pub seed: u64,
    pub certificate: ApproxCertificate,
}
#[derive(Debug, Clone)]
pub struct SynthesisReport {
    pub rotations: Vec<RotationCertificate>,
    /// Sum of requested certified local bounds, only through exact unitary operations.
    /// None retains local certificates without claiming a whole-program bound.
    pub operator_error_bound: Option<Rational>,
    pub provenance: Arc<ProvenanceGraph>,
    pub before_operations: usize,
    pub after_operations: usize,
}
#[derive(Debug, Clone)]
pub struct ExactRegionCertificate {
    pub occurrences: Vec<ProvenanceId>,
    pub provenance: ProvenanceId,
    pub interface: Vec<QubitId>,
    pub seed: u64,
    pub certificate: ExactCertificate,
}
#[derive(Debug, Clone)]
pub struct SkippedCandidate {
    pub occurrences: Vec<ProvenanceId>,
    pub reason: String,
}
#[derive(Debug, Clone, Default)]
pub struct ZxReport {
    pub provenance: Arc<ProvenanceGraph>,
    pub accepted: Vec<ExactRegionCertificate>,
    pub skipped: Vec<SkippedCandidate>,
    pub before_operations: usize,
    pub after_operations: usize,
}
fn target_angle(angle: &Angle, limits: Limits) -> Result<AngleTarget, WorkerError> {
    let (_, target) = angle.evaluate_target(&std::collections::BTreeMap::new())?;
    let cap = limits.coefficient_bits.min(16_384);
    let check = |values: &[&num_bigint::BigInt]| {
        if values.iter().any(|value| value.bits() > cap) {
            Err(WorkerError::Budget("angle identity bits"))
        } else {
            Ok(())
        }
    };
    Ok(match target {
        BoundAngleTarget::DyadicRadians { bits } => AngleTarget::DyadicRadians { bits },
        BoundAngleTarget::RationalPi {
            numerator,
            denominator,
        } => {
            check(&[&numerator, &denominator])?;
            AngleTarget::RationalPi {
                numerator,
                denominator,
            }
        }
        BoundAngleTarget::AffinePi {
            radians_numerator,
            radians_denominator,
            pi_numerator,
            pi_denominator,
        } => {
            check(&[
                &radians_numerator,
                &radians_denominator,
                &pi_numerator,
                &pi_denominator,
            ])?;
            AngleTarget::AffinePi {
                radians_numerator,
                radians_denominator,
                pi_numerator,
                pi_denominator,
            }
        }
    })
}
fn rotation(operation: &SemanticOperation, limits: Limits) -> Result<Option<Target>, WorkerError> {
    if let SemanticOperation::Gate { gate, .. } = operation {
        let (axis, angle) = match gate {
            Gate::Rx(angle) => (Axis::X, angle),
            Gate::Ry(angle) => (Axis::Y, angle),
            Gate::Rz(angle) => (Axis::Z, angle),
            _ => return Ok(None),
        };
        return Ok(Some(Target {
            axis,
            angle: target_angle(angle, limits)?,
        }));
    }
    Ok(None)
}
fn positional_controls(controls: &[Control]) -> Result<Vec<quest_math::Control>, WorkerError> {
    controls
        .iter()
        .enumerate()
        .map(|(index, control)| {
            Ok(quest_math::Control {
                qubit: index
                    .checked_add(1)
                    .ok_or(WorkerError::Budget("control index"))?,
                positive: control.state() == ControlState::One,
            })
        })
        .collect()
}
fn admit_output(count: usize, extra: usize, maximum: usize) -> Result<(), WorkerError> {
    if count
        .checked_add(extra)
        .is_none_or(|count| count > maximum.min(16_384))
    {
        return Err(WorkerError::Budget("output operations"));
    }
    Ok(())
}
fn semantic(
    operation: &quest_math::Operation,
    wires: &[QubitId],
) -> Result<SemanticOperation, WorkerError> {
    let mut controls = operation
        .controls
        .iter()
        .map(|control| {
            Ok(Control::new(
                *wires.get(control.qubit).ok_or(Error::InvalidId)?,
                if control.positive {
                    ControlState::One
                } else {
                    ControlState::Zero
                },
            ))
        })
        .collect::<crate::Result<Vec<_>>>()?;
    let mut targets = operation
        .targets
        .iter()
        .map(|target| wires.get(*target).copied().ok_or(Error::InvalidId))
        .collect::<crate::Result<Vec<_>>>()?;
    let gate = match operation.gate {
        quest_math::Gate::H => Gate::H,
        quest_math::Gate::X => Gate::X,
        quest_math::Gate::Y => Gate::Y,
        quest_math::Gate::Z => Gate::Z,
        quest_math::Gate::S => Gate::S,
        quest_math::Gate::Sdg => Gate::Sdg,
        quest_math::Gate::T => Gate::T,
        quest_math::Gate::Tdg => Gate::Tdg,
        quest_math::Gate::Swap => Gate::Swap,
        quest_math::Gate::W => {
            return Ok(SemanticOperation::GlobalPhase {
                angle: Angle::pi(1, 4)?,
                controls: controls.into(),
            });
        }
        quest_math::Gate::Cx | quest_math::Gate::Cz => {
            let control = targets.first().copied().ok_or(Error::InvalidId)?;
            targets.remove(0);
            controls.push(Control::new(control, ControlState::One));
            if operation.gate == quest_math::Gate::Cx {
                Gate::X
            } else {
                Gate::Z
            }
        }
    };
    Ok(SemanticOperation::Gate {
        gate,
        targets: targets.into(),
        controls: controls.into(),
    })
}
struct ProvenanceBudget {
    remaining: usize,
}
impl ProvenanceBudget {
    const fn new(limits: Limits) -> Self {
        Self {
            remaining: if limits.bytes < 64 * 1024 * 1024 {
                limits.bytes
            } else {
                64 * 1024 * 1024
            },
        }
    }
    fn charge(&mut self, count: usize) -> Result<(), WorkerError> {
        let bytes = count
            .checked_mul(size_of::<ProvenanceId>())
            .ok_or(WorkerError::Budget("aggregate provenance bytes"))?;
        self.remaining = self
            .remaining
            .checked_sub(bytes)
            .ok_or(WorkerError::Budget("aggregate provenance bytes"))?;
        Ok(())
    }
    fn edit(&mut self, program: &ValidatedProgram) -> Result<ProvenanceGraph, WorkerError> {
        self.remaining = self.remaining.min(program.limits.max_provenance_bytes);
        Ok(ProvenanceGraph::edit(
            Arc::clone(&program.provenance),
            self.remaining,
        )?)
    }
    fn finish(
        self,
        mut graph: ProvenanceGraph,
        next: usize,
    ) -> Result<Arc<ProvenanceGraph>, WorkerError> {
        if graph.retained_bytes()? > self.remaining {
            return Err(WorkerError::Budget("aggregate provenance bytes"));
        }
        graph.retain_next_occurrence(next);
        Ok(Arc::new(graph))
    }
    fn copy(&mut self, inputs: &[Occurrence]) -> Result<Vec<ProvenanceId>, WorkerError> {
        let count = inputs.len();
        self.charge(count)?;
        let mut ids = Vec::new();
        ids.try_reserve_exact(count)
            .map_err(|_| WorkerError::Budget("provenance allocation"))?;
        ids.extend(inputs.iter().map(|o| o.provenance));
        Ok(ids)
    }
}
fn replacement(
    sequence: &Sequence,
    interface: &[QubitId],
    inputs: &[Occurrence],
    next: &mut usize,
    budget: &mut ProvenanceBudget,
    graph: &mut ProvenanceGraph,
) -> Result<(Vec<Occurrence>, ProvenanceId), WorkerError> {
    let first = inputs.first().ok_or(Error::InvalidId)?;
    budget.charge(sequence.operations.len())?;
    budget.charge(1)?; // Report root, including a rewrite with no surviving output.
    let inputs = budget.copy(inputs)?;
    let provenance = graph.rewrite(&inputs, budget.remaining)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(sequence.operations.len())
        .map_err(|_| WorkerError::Budget("replacement allocation"))?;
    for (index, operation) in sequence.operations.iter().enumerate() {
        let id = if index == 0 {
            first.id
        } else {
            let id = OccurrenceId {
                owner: first.id.owner,
                index: *next,
            };
            *next = next
                .checked_add(1)
                .ok_or(WorkerError::Budget("occurrence identities"))?;
            id
        };
        output.push(Occurrence {
            id,
            provenance,
            source: first.source.clone(),
            operation: semantic(operation, interface)?,
        });
    }
    Ok((output, provenance))
}
impl ValidatedProgram {
    /// Explicit single-qubit synthesis. Every admitted rotation must receive a
    /// full-phase certificate; any failure returns an error. At most 32 requests
    /// and 16384 output operations are admitted per invocation.
    /// The error tolerance must be finite and strictly between zero and one.
    /// Immediate provenance references, reports and retained history are bounded by
    /// the smaller of `limits.bytes` and 64 MiB across the invocation.
    /// # Errors
    /// Rejects unbound rotation angles, unsupported ordering, budgets and failed certificates.
    pub fn synthesize_rotations(
        self,
        client: &Client,
        epsilon_per_rotation: f64,
        seed: u64,
        limits: Limits,
    ) -> Result<(Self, SynthesisReport), WorkerError> {
        if !epsilon_per_rotation.is_finite()
            || epsilon_per_rotation <= 0.0
            || epsilon_per_rotation >= 1.0
        {
            return Err(quest_optimizer_client::Error::Limits.into());
        }
        if !self.explicit_edges.is_empty() {
            return Err(WorkerError::Ordering);
        }
        let mut report = SynthesisReport {
            rotations: Vec::new(),
            operator_error_bound: self
                .occurrences
                .iter()
                .all(|o| o.operation.exact_unitary())
                .then(|| Rational::from_integer(0.into())),
            provenance: Arc::clone(&self.provenance),
            before_operations: self.occurrences.len(),
            after_operations: 0,
        };
        let epsilon = quest_math::dyadic_from_bits(epsilon_per_rotation.to_bits(), limits)?;
        let mut budget = ProvenanceBudget::new(limits);
        let mut graph = budget.edit(&self)?;
        let mut next = graph.next_occurrence();
        let mut output = Vec::new();
        for occurrence in &self.occurrences {
            if let Some(target) = rotation(&occurrence.operation, limits)? {
                if report.rotations.len() >= 32 {
                    return Err(WorkerError::Budget("synthesis requests"));
                }
                let request_seed = seed
                    .checked_add(
                        u64::try_from(report.rotations.len())
                            .map_err(|_| WorkerError::Budget("seed"))?,
                    )
                    .ok_or(WorkerError::Budget("seed"))?;
                let certificate =
                    client.synthesize(&target, epsilon_per_rotation, request_seed, limits)?;
                let SemanticOperation::Gate {
                    targets, controls, ..
                } = &occurrence.operation
                else {
                    return Err(Error::NotUnitary.into());
                };
                let interface = occurrence.operation.qubits().collect::<Vec<_>>();
                let added_controls = positional_controls(controls)?;
                let lifted = quest_math::lift_controlled_rotation(
                    &certificate,
                    interface.len(),
                    0,
                    &added_controls,
                    limits,
                )?;
                admit_output(
                    output.len(),
                    lifted.sequence().operations.len(),
                    self.limits.max_operations,
                )?;
                let (replacement, provenance) = replacement(
                    lifted.sequence(),
                    &interface,
                    std::slice::from_ref(occurrence),
                    &mut next,
                    &mut budget,
                    &mut graph,
                )?;
                output.extend(replacement);
                if let Some(bound) = &mut report.operator_error_bound {
                    *bound = std::ops::Add::add(&*bound, &epsilon);
                }
                report.rotations.push(RotationCertificate {
                    occurrence: occurrence.id,
                    input: occurrence.provenance,
                    provenance,
                    targets: targets.clone(),
                    controls: controls.clone(),
                    seed: request_seed,
                    certificate,
                });
            } else {
                admit_output(output.len(), 1, self.limits.max_operations)?;
                budget.charge(1)?;
                output.push(occurrence.clone());
            }
        }
        report.after_operations = output.len();
        report.provenance = budget.finish(graph, next)?;
        let program = Self::from_parts(
            self.owner,
            self.num_qubits,
            self.num_bits,
            self.parameters,
            output,
            self.explicit_edges,
            self.limits,
            Arc::clone(&report.provenance),
        )?;
        Ok((program, report))
    }
}

fn quarter_turns(angle: &Angle) -> Option<usize> {
    let value = angle.rational_pi_identity()?;
    angle.evaluate(&std::collections::BTreeMap::new()).ok()?;
    if value.numer().bits() > 16_384 || value.denom().bits() > 16_384 {
        return None;
    }
    let numerator = std::ops::Mul::mul(value.numer(), 4u8);
    if std::ops::Rem::rem(&numerator, value.denom()) != 0.into() {
        return None;
    }
    let quotient = std::ops::Div::div(numerator, value.denom());
    let power: num_bigint::BigInt = std::ops::Rem::rem(quotient, 8u8);
    let power = if power.sign() == num_bigint::Sign::Minus {
        std::ops::Add::add(power, 8u8)
    } else {
        power
    };
    power.to_usize()
}
fn quantum_sequence(operations: &[Occurrence], interface: &[QubitId]) -> Option<Sequence> {
    let mut output = Vec::new();
    for occurrence in operations {
        let (gate, repeat, targets, controls) = match &occurrence.operation {
            SemanticOperation::Gate {
                gate,
                targets,
                controls,
            } => {
                let (gate, repeat) = match gate {
                    Gate::Id => (quest_math::Gate::X, 0),
                    Gate::H => (quest_math::Gate::H, 1),
                    Gate::X => (quest_math::Gate::X, 1),
                    Gate::Y => (quest_math::Gate::Y, 1),
                    Gate::Z => (quest_math::Gate::Z, 1),
                    Gate::S => (quest_math::Gate::S, 1),
                    Gate::Sdg => (quest_math::Gate::Sdg, 1),
                    Gate::T => (quest_math::Gate::T, 1),
                    Gate::Tdg => (quest_math::Gate::Tdg, 1),
                    Gate::Swap => (quest_math::Gate::Swap, 1),
                    Gate::Phase(angle) => (quest_math::Gate::T, quarter_turns(angle)?),
                    _ => return None,
                };
                (gate, repeat, targets.as_ref(), controls.as_ref())
            }
            SemanticOperation::GlobalPhase { angle, controls } => (
                quest_math::Gate::W,
                quarter_turns(angle)?,
                [].as_slice(),
                controls.as_ref(),
            ),
            _ => return None,
        };
        let targets = targets
            .iter()
            .map(|q| interface.iter().position(|wire| wire == q))
            .collect::<Option<Vec<_>>>()?;
        let controls = controls
            .iter()
            .map(|c| {
                Some(quest_math::Control {
                    qubit: interface.iter().position(|q| *q == c.qubit())?,
                    positive: c.state() == ControlState::One,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        if output.len().checked_add(repeat)? > 128 {
            return None;
        }
        for _ in 0..repeat {
            output.push(quest_math::Operation {
                gate,
                targets: targets.clone(),
                controls: controls.clone(),
            });
        }
    }
    Some(Sequence {
        qubits: interface.len(),
        operations: output,
    })
}
struct QuantumWindow {
    end: usize,
    interface: Vec<QubitId>,
    sequence: Option<Sequence>,
}
fn quantum_window(
    occurrences: &[Occurrence],
    offset: usize,
    limits: Limits,
) -> Result<QuantumWindow, WorkerError> {
    let mut end = offset;
    let mut interface = Vec::new();
    let mut sequence = None;
    for operation in occurrences.iter().skip(offset).take(128) {
        let mut candidate_interface = interface.clone();
        for qubit in operation.operation.qubits() {
            if !candidate_interface.contains(&qubit) {
                candidate_interface.push(qubit);
            }
        }
        if candidate_interface.len() > limits.qubits.min(4) {
            break;
        }
        let candidate_end = end
            .checked_add(1)
            .ok_or(WorkerError::Budget("region end"))?;
        let Some(candidate) = quantum_sequence(
            occurrences
                .get(offset..candidate_end)
                .ok_or(Error::InvalidId)?,
            &candidate_interface,
        ) else {
            break;
        };
        interface = candidate_interface;
        sequence = Some(candidate);
        end = candidate_end;
    }
    Ok(QuantumWindow {
        end,
        interface,
        sequence,
    })
}
impl ValidatedProgram {
    /// Optional bounded ZX regions. Failures and unprofitable candidates retain
    /// every original occurrence and are recorded as skipped. Each replacement
    /// requires complete exact matrix equality, including its recovered phase.
    /// Immediate provenance references, reports and retained history are bounded by
    /// the smaller of `limits.bytes` and 64 MiB across the invocation.
    /// # Errors
    /// Rejects internal resource arithmetic and graph reconstruction failures.
    pub fn optimize_zx(
        self,
        client: &Client,
        seed: u64,
        limits: Limits,
    ) -> Result<(Self, ZxReport), WorkerError> {
        let mut report = ZxReport {
            provenance: Arc::clone(&self.provenance),
            before_operations: self.occurrences.len(),
            after_operations: self.occurrences.len(),
            ..ZxReport::default()
        };
        if !self.explicit_edges.is_empty() || self.occurrences.len() > 16_384 {
            report.skipped.push(SkippedCandidate {
                occurrences: vec![],
                reason: "explicit ordering or program worker budget".into(),
            });
            return Ok((self, report));
        }
        let mut budget = ProvenanceBudget::new(limits);
        let mut graph = budget.edit(&self)?;
        let mut next = graph.next_occurrence();
        let mut output = Vec::new();
        output
            .try_reserve_exact(self.occurrences.len())
            .map_err(|_| WorkerError::Budget("output allocation"))?;
        let mut offset = 0usize;
        let mut requests = 0u64;
        while offset < self.occurrences.len() {
            let QuantumWindow {
                end,
                interface,
                sequence,
            } = quantum_window(&self.occurrences, offset, limits)?;
            if let Some(sequence) = sequence.filter(|_| requests < 32) {
                let region = self.occurrences.get(offset..end).ok_or(Error::InvalidId)?;
                let ids = budget.copy(region)?;
                let request_seed = seed
                    .checked_add(requests)
                    .ok_or(WorkerError::Budget("request seed"))?;
                requests = requests
                    .checked_add(1)
                    .ok_or(WorkerError::Budget("requests"))?;
                match client.optimize_zx(&sequence, request_seed, limits) {
                    Ok(certificate) if certificate.candidate().operations.len() < region.len() => {
                        let (replacement, provenance) = replacement(
                            certificate.candidate(),
                            &interface,
                            region,
                            &mut next,
                            &mut budget,
                            &mut graph,
                        )?;
                        output.extend(replacement);
                        report.accepted.push(ExactRegionCertificate {
                            occurrences: ids,
                            provenance,
                            interface,
                            seed: request_seed,
                            certificate,
                        });
                    }
                    Ok(_) => {
                        report.skipped.push(SkippedCandidate {
                            occurrences: ids,
                            reason: "candidate does not reduce native operations".into(),
                        });
                        budget.charge(region.len())?;
                        output.extend_from_slice(region);
                    }
                    Err(error) => {
                        report.skipped.push(SkippedCandidate {
                            occurrences: ids,
                            reason: error.to_string(),
                        });
                        budget.charge(region.len())?;
                        output.extend_from_slice(region);
                    }
                }
                offset = end;
            } else {
                let occurrence = self.occurrences.get(offset).ok_or(Error::InvalidId)?;
                budget.charge(1)?;
                output.push(occurrence.clone());
                offset = offset
                    .checked_add(1)
                    .ok_or(WorkerError::Budget("region offset"))?;
            }
        }
        report.after_operations = output.len();
        report.provenance = budget.finish(graph, next)?;
        let program = Self::from_parts(
            self.owner,
            self.num_qubits,
            self.num_bits,
            self.parameters,
            output,
            self.explicit_edges,
            self.limits,
            Arc::clone(&report.provenance),
        )?;
        Ok((program, report))
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use googletest::{Result, prelude::*};
    #[gtest]
    fn borrowed_negative_targets_preserve_identity_and_apply_leaf_bit_limits() -> Result<()> {
        let value = Angle::pi(3, 8)?;
        let negative = value.negated()?;
        expect_eq!(
            target_angle(&negative, Limits::default())?,
            AngleTarget::RationalPi {
                numerator: (-3).into(),
                denominator: 8.into()
            }
        );
        expect_true!(
            target_angle(
                &negative,
                Limits {
                    coefficient_bits: 2,
                    ..Limits::default()
                }
            )
            .is_err()
        );
        let zero = Angle::radians(0.0)?.negated()?;
        expect_eq!(
            target_angle(&zero, Limits::default())?,
            AngleTarget::DyadicRadians {
                bits: (-0.0f64).to_bits()
            }
        );
        Ok(())
    }
}
