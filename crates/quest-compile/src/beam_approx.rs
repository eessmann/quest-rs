//! One-original-rotation approximate MITM proposals for the deterministic beam.
use crate::model::{Occurrence, SemanticOperation};
use crate::workers::{
    ProvenanceBudget, WorkerError, admit_output, candidate_replacement, rotation,
};
use crate::{
    BoundAngleTarget, Control, ControlState, Error, Gate, OccurrenceId, ProvenanceGraph,
    ProvenanceId, QuantumRegion, QubitId,
};
#[allow(unused_imports)]
use crate::{
    BoundParityPasses, ExactPasses, LinearPasses, NumericalPasses, OracleExport, ParityPasses,
    TerminalPasses,
};
use quest_math::{AngleTarget, ControlledApproxCertificate, Limits, Rational};
use quest_optimizer_client::{Client, MitmResult};
use quest_optimizer_protocol::MitmLimits;
use std::{mem::size_of, sync::Arc};

/// Parent-certified local approximation and the exact original occurrence identity.
#[derive(Debug, Clone)]
pub struct ApproxRegionCertificate {
    pub occurrence: OccurrenceId,
    pub input: ProvenanceId,
    pub provenance: ProvenanceId,
    pub targets: Arc<[QubitId]>,
    pub controls: Arc<[Control]>,
    pub seed: u64,
    pub original_target: BoundAngleTarget,
    pub certificate: ControlledApproxCertificate,
}

/// One worker request at an original occurrence, including typed terminal outcomes.
#[derive(Debug)]
pub struct ApproxMitmReport {
    pub provenance: Arc<ProvenanceGraph>,
    pub before_operations: usize,
    pub after_operations: usize,
    pub candidate_window: Option<(usize, usize)>,
    pub target_identity: Option<BoundAngleTarget>,
    /// Conservatively reserved local scan, source replay, lift and output work.
    pub local_work: u64,
    pub outcome: Option<MitmResult<ApproxRegionCertificate>>,
}

fn bound_identity(angle: &AngleTarget) -> BoundAngleTarget {
    match angle {
        AngleTarget::DyadicRadians { bits } => BoundAngleTarget::DyadicRadians { bits: *bits },
        AngleTarget::RationalPi {
            numerator,
            denominator,
        } => BoundAngleTarget::RationalPi {
            numerator: numerator.clone(),
            denominator: denominator.clone(),
        },
        AngleTarget::AffinePi {
            radians_numerator,
            radians_denominator,
            pi_numerator,
            pi_denominator,
        } => BoundAngleTarget::AffinePi {
            radians_numerator: radians_numerator.clone(),
            radians_denominator: radians_denominator.clone(),
            pi_numerator: pi_numerator.clone(),
            pi_denominator: pi_denominator.clone(),
        },
    }
}

fn charge(
    remaining: &mut u64,
    report: &mut ApproxMitmReport,
    work: u64,
) -> Result<(), WorkerError> {
    *remaining = remaining
        .checked_sub(work)
        .filter(|value| *value > 0)
        .ok_or(WorkerError::Budget("approx MITM local work"))?;
    report.local_work = report
        .local_work
        .checked_add(work)
        .ok_or(WorkerError::Budget("approx MITM local work"))?;
    Ok(())
}

/// Compiler extension over shared semantic capabilities.
pub trait ApproximateBeamPasses: Sized {
    /// # Errors
    /// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
    #[expect(
        clippy::too_many_arguments,
        reason = "Candidate requests carry separate input, output, proof and process bounds"
    )]
    fn approx_mitm_candidate_from(
        self,
        start_offset: usize,
        client: &Client,
        epsilon_bits: u64,
        seed: u64,
        search_limits: MitmLimits,
        proof_limits: Limits,
        max_output: usize,
    ) -> Result<(Self, ApproxMitmReport), WorkerError>;
}
impl ApproximateBeamPasses for QuantumRegion {
    /// Generate one parent-certified approximate candidate from the first eligible
    /// original Rx/Ry/Rz occurrence at or after `start_offset`. At most one signed
    /// control is lifted through every certificate gate, including scalar phase.
    /// # Errors
    /// Rejects invalid target identities, explicit ordering, exhausted limits,
    /// worker failure or a candidate without an independent parent certificate.
    #[expect(
        clippy::too_many_lines,
        reason = "One-window admission and transactional publication share a work budget"
    )]
    fn approx_mitm_candidate_from(
        self,
        start_offset: usize,
        client: &Client,
        epsilon_bits: u64,
        seed: u64,
        search_limits: MitmLimits,
        proof_limits: Limits,
        max_output: usize,
    ) -> Result<(Self, ApproxMitmReport), WorkerError> {
        if start_offset > self.occurrences().len() {
            return Err(Error::InvalidId.into());
        }
        if !self.explicit_edges().is_empty() {
            return Err(WorkerError::Ordering);
        }
        search_limits
            .validate(1)
            .map_err(|_| quest_optimizer_client::Error::Limits)?;
        let epsilon = quest_math::dyadic_from_bits(epsilon_bits, proof_limits)?;
        if epsilon <= Rational::from_integer(0.into())
            || epsilon >= Rational::from_integer(1.into())
        {
            return Err(quest_optimizer_client::Error::Limits.into());
        }
        if self.occurrences().len() > 16_384 || self.occurrences().len() > max_output {
            return Err(WorkerError::Budget("approx MITM input/output operations"));
        }
        let output_cap = max_output.min(self.limits().max_operations).min(16_384);
        // A single replacement can contribute at most the requested depth;
        // the public ceiling need not be allocated or charged in full.
        let reachable_output = self
            .occurrences()
            .len()
            .checked_add(search_limits.max_depth)
            .ok_or(WorkerError::Budget("approx MITM reachable output"))?
            .min(output_cap);
        let output_bytes = reachable_output
            .checked_mul(
                size_of::<Occurrence>()
                    .checked_add(512)
                    .ok_or(WorkerError::Budget("approx MITM output storage"))?,
            )
            .ok_or(WorkerError::Budget("approx MITM output storage"))?;
        // Four independent affine coefficients may be copied into the target,
        // parent certificate, controlled certificate and retained report.
        let coefficient_bytes = usize::try_from(proof_limits.coefficient_bits.min(16_384))
            .ok()
            .and_then(|bits| bits.checked_add(7))
            .map(|bits| bits / 8)
            .and_then(|bytes| bytes.checked_mul(2).and_then(|bytes| bytes.checked_add(64)))
            .and_then(|bytes| bytes.checked_mul(24))
            .ok_or(WorkerError::Budget("approx MITM target storage"))?;
        let retained = self.retained_bytes()?;
        let live_bytes = retained
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(output_bytes))
            .and_then(|bytes| bytes.checked_add(coefficient_bytes))
            .and_then(|bytes| bytes.checked_add(64 * 1024))
            .ok_or(WorkerError::Budget("approx MITM live storage"))?;
        if live_bytes > proof_limits.bytes {
            return Err(WorkerError::Budget("approx MITM live storage"));
        }
        let output_work = u64::try_from(self.provenance_arc().copy_work()?)
            .ok()
            .and_then(|work| {
                u64::try_from(reachable_output).ok().and_then(|count| {
                    count
                        .checked_mul(64)
                        .and_then(|copy| work.checked_add(copy))
                })
            })
            .and_then(|work| {
                u64::try_from(search_limits.max_depth)
                    .ok()
                    .and_then(|depth| {
                        depth
                            .checked_mul(512)
                            .and_then(|lift| work.checked_add(lift))
                    })
            })
            .ok_or(WorkerError::Budget("approx MITM output work"))?;
        let mut remaining_work = search_limits
            .max_work
            .checked_sub(output_work)
            .filter(|remaining| *remaining > 0)
            .ok_or(WorkerError::Budget("approx MITM output work"))?;
        let mut report = ApproxMitmReport {
            provenance: Arc::clone(self.provenance_arc()),
            before_operations: self.occurrences().len(),
            after_operations: self.occurrences().len(),
            candidate_window: None,
            target_identity: None,
            local_work: output_work,
            outcome: None,
        };
        let mut budget = ProvenanceBudget::new(proof_limits);
        let mut graph = budget.edit(&self)?;
        let mut next = graph.next_occurrence();
        for offset in start_offset..self.occurrences().len() {
            let occurrence = self.occurrences().get(offset).ok_or(Error::InvalidId)?;
            let operand_count = occurrence.operation.qubits().count();
            let scan = u64::try_from(operand_count)
                .ok()
                .and_then(|n| n.checked_mul(16))
                .and_then(|n| n.checked_add(128))
                .ok_or(WorkerError::Budget("approx MITM scan work"))?;
            charge(&mut remaining_work, &mut report, scan)?;
            let SemanticOperation::Gate {
                gate,
                targets,
                controls,
            } = &occurrence.operation
            else {
                continue;
            };
            let (Gate::Rx(angle) | Gate::Ry(angle) | Gate::Rz(angle)) = gate else {
                continue;
            };
            if targets.len() != 1
                || controls.len() > 1
                || operand_count > proof_limits.qubits.min(2)
            {
                continue;
            }
            let target_work = angle
                .binding_work_estimate()?
                .checked_add(
                    proof_limits
                        .coefficient_bits
                        .min(16_384)
                        .checked_mul(4)
                        .ok_or(WorkerError::Budget("approx MITM target work"))?,
                )
                .ok_or(WorkerError::Budget("approx MITM target work"))?;
            charge(&mut remaining_work, &mut report, target_work)?;
            let Some(target) = rotation(&occurrence.operation, proof_limits)? else {
                continue;
            };
            let identity = bound_identity(&target.angle);
            let end = offset
                .checked_add(1)
                .ok_or(WorkerError::Budget("approx MITM window"))?;
            report.candidate_window = Some((offset, end));
            report.target_identity = Some(identity.clone());
            let worker_limits = MitmLimits {
                max_work: remaining_work,
                ..search_limits
            };
            let outcome =
                client.approx_mitm(&target, epsilon_bits, seed, worker_limits, proof_limits)?;
            match outcome {
                MitmResult::Candidate(certificate) => {
                    if certificate.candidate().operations.len() > search_limits.max_depth {
                        return Err(WorkerError::RejectedOutput("approx MITM candidate depth"));
                    }
                    let interface = occurrence.operation.qubits().collect::<Vec<_>>();
                    let mut added_controls = Vec::new();
                    added_controls
                        .try_reserve_exact(controls.len())
                        .map_err(|_| WorkerError::Budget("approx MITM controls allocation"))?;
                    for (index, control) in controls.iter().enumerate() {
                        added_controls.push(quest_math::Control {
                            qubit: index
                                .checked_add(1)
                                .ok_or(WorkerError::Budget("approx MITM control index"))?,
                            positive: control.state() == ControlState::One,
                        });
                    }
                    let lifted = quest_math::lift_controlled_rotation(
                        &certificate,
                        interface.len(),
                        0,
                        &added_controls,
                        proof_limits,
                    )?;
                    let outside = self
                        .occurrences()
                        .len()
                        .checked_sub(1)
                        .ok_or(WorkerError::Budget("approx MITM output operations"))?;
                    admit_output(outside, lifted.sequence().operations.len(), output_cap).map_err(
                        |_| WorkerError::RejectedOutput("approx MITM output operations"),
                    )?;
                    let source = std::slice::from_ref(occurrence);
                    let input = occurrence.provenance;
                    let original_id = occurrence.id;
                    let retained_targets = Arc::clone(targets);
                    let retained_controls = Arc::clone(controls);
                    let (replacement, provenance) = candidate_replacement(
                        lifted.sequence(),
                        &interface,
                        source,
                        &mut next,
                        &mut budget,
                        &mut graph,
                    )?;
                    let size = outside
                        .checked_add(replacement.len())
                        .ok_or(WorkerError::Budget("approx MITM output operations"))?;
                    let mut output = Vec::new();
                    output
                        .try_reserve_exact(size)
                        .map_err(|_| WorkerError::Budget("approx MITM output allocation"))?;
                    output.extend_from_slice(
                        self.occurrences().get(..offset).ok_or(Error::InvalidId)?,
                    );
                    output.extend(replacement);
                    output
                        .extend_from_slice(self.occurrences().get(end..).ok_or(Error::InvalidId)?);
                    report.after_operations = output.len();
                    report.provenance = budget.finish(graph, next)?;
                    report.outcome = Some(MitmResult::Candidate(ApproxRegionCertificate {
                        occurrence: original_id,
                        input,
                        provenance,
                        targets: retained_targets,
                        controls: retained_controls,
                        seed,
                        original_target: identity,
                        certificate: lifted,
                    }));
                    let program = Self::from_parts(
                        self.owner(),
                        self.num_qubits(),
                        self.num_bits(),
                        self.parameter_storage().clone(),
                        output,
                        self.explicit_edges().clone(),
                        self.limits(),
                        Arc::clone(&report.provenance),
                    )?;
                    return Ok((program, report));
                }
                MitmResult::NoCandidate { explored } => {
                    report.outcome = Some(MitmResult::NoCandidate { explored });
                }
                MitmResult::Incomplete { reason, explored } => {
                    report.outcome = Some(MitmResult::Incomplete { reason, explored });
                }
                MitmResult::Exhausted { explored } => {
                    report.outcome = Some(MitmResult::Exhausted { explored });
                }
                MitmResult::Unresolved {
                    precision_bits,
                    explored,
                } => {
                    report.outcome = Some(MitmResult::Unresolved {
                        precision_bits,
                        explored,
                    });
                }
            }
            return Ok((self, report));
        }
        Ok((self, report))
    }
}
