//! One-window, parent-certified exact MITM candidate adapter for the beam.
use crate::model::{Occurrence, SemanticOperation};
use crate::workers::{
    ProvenanceBudget, QuantumWindow, WorkerError, admit_output, candidate_replacement,
    quantum_window,
};
#[allow(unused_imports)]
use crate::{
    BoundParityPasses, ExactPasses, LinearPasses, NumericalPasses, OracleExport, ParityPasses,
    TerminalPasses,
};
use crate::{Error, ExactRegionCertificate, Gate, ProvenanceGraph, QuantumRegion};
use quest_math::Limits;
use quest_optimizer_client::{Client, MitmResult};
use quest_optimizer_protocol::MitmLimits;
use std::{mem::size_of, sync::Arc};

/// The selected original span and its typed worker result. Only Candidate
/// contains a transformed program and a parent-owned exact certificate.
#[derive(Debug)]
pub struct ExactMitmReport {
    pub provenance: Arc<ProvenanceGraph>,
    pub before_operations: usize,
    pub after_operations: usize,
    pub candidate_window: Option<(usize, usize)>,
    /// Conservatively reserved local scan, proof-copy, and output work units.
    pub local_work: u64,
    pub outcome: Option<MitmResult<ExactRegionCertificate>>,
}

#[derive(Clone, Copy)]
struct ScanWeight {
    operands: u64,
    replay: u64,
}

fn scan_weights(occurrences: &[Occurrence]) -> Result<Vec<ScanWeight>, WorkerError> {
    let mut weights = Vec::new();
    weights
        .try_reserve_exact(occurrences.len())
        .map_err(|_| WorkerError::Budget("exact MITM scan metadata"))?;
    for occurrence in occurrences {
        let operands = occurrence.operation.operands();
        let operands = operands
            .targets()
            .len()
            .checked_add(operands.controls().len())
            .ok_or(WorkerError::Budget("exact MITM scan operands"))?;
        let replay = if matches!(
            &occurrence.operation,
            SemanticOperation::Gate {
                gate: Gate::Phase(_),
                ..
            } | SemanticOperation::GlobalPhase { .. }
        ) {
            occurrence
                .operation
                .binding_work_estimate()?
                .checked_add(
                    u64::try_from(occurrence.operation.retained_bytes()?)
                        .map_err(|_| WorkerError::Budget("exact MITM scan source"))?,
                )
                .ok_or(WorkerError::Budget("exact MITM scan source"))?
        } else {
            1
        };
        weights.push(ScanWeight {
            operands: u64::try_from(operands)
                .map_err(|_| WorkerError::Budget("exact MITM scan operands"))?,
            replay,
        });
    }
    Ok(weights)
}

fn scan_work(weights: &[ScanWeight]) -> Result<u64, WorkerError> {
    let count =
        u64::try_from(weights.len()).map_err(|_| WorkerError::Budget("exact MITM scan work"))?;
    weights
        .iter()
        .enumerate()
        .try_fold(0u64, |total, (index, weight)| {
            let repeats = count.checked_sub(u64::try_from(index).ok()?)?;
            // The shared window helper extends a linear interface, then rebuilds
            // every prefix. Account for a wide rejected operation's quadratic
            // deduplication and for source/bit conversion on every replay.
            let interface = weight.operands.checked_add(2)?;
            let per_replay = weight
                .replay
                .checked_add(weight.operands.checked_mul(16)?)?
                .checked_add(32)?;
            total
                .checked_add(interface.checked_mul(interface)?)?
                .checked_add(per_replay.checked_mul(repeats)?)?
                .checked_add(64)
        })
        .ok_or(WorkerError::Budget("exact MITM scan work"))
}

/// Compiler extension over shared semantic capabilities.
pub trait MeetInTheMiddlePasses: Sized {
    /// # Errors
    /// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
    fn exact_mitm_candidate_from(
        self,
        start_offset: usize,
        client: &Client,
        seed: u64,
        search_limits: MitmLimits,
        proof_limits: Limits,
        max_output: usize,
    ) -> Result<(Self, ExactMitmReport), WorkerError>;
}
impl MeetInTheMiddlePasses for QuantumRegion {
    /// Generate one full-phase exact candidate in the first eligible one- or
    /// two-qubit region at or after `start_offset`. The parent client independently
    /// verifies a worker candidate before any replacement is published.
    /// # Errors
    /// Rejects malformed source, unsupported ordering, exhausted budgets,
    /// worker/process failure, or failed parent certification.
    #[expect(
        clippy::too_many_lines,
        reason = "One-window search and transactional publication share a resource admission"
    )]
    fn exact_mitm_candidate_from(
        self,
        start_offset: usize,
        client: &Client,
        seed: u64,
        search_limits: MitmLimits,
        proof_limits: Limits,
        max_output: usize,
    ) -> Result<(Self, ExactMitmReport), WorkerError> {
        // One-qubit hard caps contain valid two-qubit settings. Validate the
        // request even when this source has no eligible quantum window.
        search_limits
            .validate(1)
            .map_err(|_| quest_optimizer_client::Error::Limits)?;
        if start_offset > self.occurrences().len() {
            return Err(Error::InvalidId.into());
        }
        if !self.explicit_edges().is_empty() {
            return Err(WorkerError::Ordering);
        }
        if self.occurrences().len() > 16_384 {
            return Err(WorkerError::Budget("exact MITM input operations"));
        }
        let output_cap = max_output.min(self.limits().max_operations).min(16_384);
        // The worker cannot return more than max_depth operations in the
        // replaced window. Reserve only an output reachable by one request.
        let reachable_output = self
            .occurrences()
            .len()
            .checked_add(search_limits.max_depth)
            .ok_or(WorkerError::Budget("exact MITM reachable output"))?
            .min(output_cap);
        let output_bytes = reachable_output
            .checked_mul(
                size_of::<Occurrence>()
                    .checked_add(512)
                    .ok_or(WorkerError::Budget("exact MITM output storage"))?,
            )
            .ok_or(WorkerError::Budget("exact MITM output storage"))?;
        let retained = self.retained_bytes()?;
        let metadata_bytes = self
            .occurrences()
            .len()
            .checked_mul(size_of::<ScanWeight>())
            .ok_or(WorkerError::Budget("exact MITM scan metadata"))?;
        let live_bytes = retained
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(output_bytes))
            .and_then(|bytes| bytes.checked_add(metadata_bytes))
            .and_then(|bytes| bytes.checked_add(4096))
            .ok_or(WorkerError::Budget("exact MITM live storage"))?;
        if live_bytes > proof_limits.bytes {
            return Err(WorkerError::Budget("exact MITM live storage"));
        }
        let retained_work = u64::try_from(self.provenance_arc().copy_work()?)
            .map_err(|_| WorkerError::Budget("exact MITM retained work"))?;
        let output_work = u64::try_from(reachable_output)
            .ok()
            .and_then(|count| count.checked_mul(64))
            .and_then(|count| count.checked_add(retained_work))
            .ok_or(WorkerError::Budget("exact MITM output work"))?;
        let mut remaining_work = search_limits
            .max_work
            .checked_sub(output_work)
            .filter(|remaining| *remaining > 0)
            .ok_or(WorkerError::Budget("exact MITM output work"))?;
        // Metadata extraction itself traverses retained source payload once.
        let metadata_work =
            u64::try_from(retained).map_err(|_| WorkerError::Budget("exact MITM scan work"))?;
        remaining_work = remaining_work
            .checked_sub(metadata_work)
            .filter(|remaining| *remaining > 0)
            .ok_or(WorkerError::Budget("exact MITM scan work"))?;
        let weights = scan_weights(self.occurrences())?;
        let mut report = ExactMitmReport {
            provenance: Arc::clone(self.provenance_arc()),
            before_operations: self.occurrences().len(),
            after_operations: self.occurrences().len(),
            candidate_window: None,
            local_work: output_work
                .checked_add(metadata_work)
                .ok_or(WorkerError::Budget("exact MITM scan work"))?,
            outcome: None,
        };
        let mut budget = ProvenanceBudget::new(proof_limits);
        let mut graph = budget.edit(&self)?;
        let mut next = graph.next_occurrence();
        let region_limits = Limits {
            qubits: proof_limits.qubits.min(2),
            ..proof_limits
        };
        for offset in start_offset..self.occurrences().len() {
            // quantum_window replays every prefix up to 128 occurrences. Charge
            // its triangular worst case before looking at this offset.
            let end = offset.saturating_add(128).min(weights.len());
            let scan_work = scan_work(weights.get(offset..end).ok_or(Error::InvalidId)?)?;
            remaining_work = remaining_work
                .checked_sub(scan_work)
                .filter(|remaining| *remaining > 0)
                .ok_or(WorkerError::Budget("exact MITM scan work"))?;
            report.local_work = report
                .local_work
                .checked_add(scan_work)
                .ok_or(WorkerError::Budget("exact MITM scan work"))?;
            let QuantumWindow {
                end,
                interface,
                sequence,
            } = quantum_window(self.occurrences(), offset, region_limits)?;
            let Some(sequence) = sequence else { continue };
            if !(1..=2).contains(&sequence.qubits) {
                continue;
            }
            let worker_limits = MitmLimits {
                max_work: remaining_work,
                ..search_limits
            };
            worker_limits
                .validate(sequence.qubits)
                .map_err(|_| quest_optimizer_client::Error::Limits)?;
            let region = self
                .occurrences()
                .get(offset..end)
                .ok_or(Error::InvalidId)?;
            report.candidate_window = Some((offset, end));
            match client.exact_mitm(&sequence, seed, worker_limits, proof_limits)? {
                MitmResult::Candidate(certificate) => {
                    if certificate.candidate().operations.len() > search_limits.max_depth {
                        return Err(WorkerError::RejectedOutput("exact MITM candidate depth"));
                    }
                    let outside = self
                        .occurrences()
                        .len()
                        .checked_sub(region.len())
                        .ok_or(WorkerError::Budget("exact MITM output operations"))?;
                    admit_output(
                        outside,
                        certificate.candidate().operations.len(),
                        output_cap,
                    )
                    .map_err(|_| WorkerError::RejectedOutput("exact MITM output operations"))?;
                    let inputs = budget.copy(region)?;
                    let (replacement, provenance) = candidate_replacement(
                        certificate.candidate(),
                        &interface,
                        region,
                        &mut next,
                        &mut budget,
                        &mut graph,
                    )?;
                    let size = outside
                        .checked_add(replacement.len())
                        .ok_or(WorkerError::Budget("exact MITM output operations"))?;
                    let mut output = Vec::new();
                    output
                        .try_reserve_exact(size)
                        .map_err(|_| WorkerError::Budget("exact MITM output allocation"))?;
                    output.extend_from_slice(
                        self.occurrences().get(..offset).ok_or(Error::InvalidId)?,
                    );
                    output.extend(replacement);
                    output
                        .extend_from_slice(self.occurrences().get(end..).ok_or(Error::InvalidId)?);
                    report.after_operations = output.len();
                    report.provenance = budget.finish(graph, next)?;
                    report.outcome = Some(MitmResult::Candidate(ExactRegionCertificate {
                        occurrences: inputs,
                        provenance,
                        interface,
                        seed,
                        certificate,
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
