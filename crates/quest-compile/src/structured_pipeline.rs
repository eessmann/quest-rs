//! Conservative composition of analysis, exact SSA cleanup and terminal fusion.
#[allow(unused_imports)]
use crate::{
    BoundParityPasses, ExactPasses, LinearPasses, NumericalPasses, OracleExport, ParityPasses,
    TerminalPasses,
};
use crate::{
    BudgetCategory, BudgetLease, BudgetLedger, CostComparison, CostProfile, LanguageError,
    OptimizationOptions, Program, QuantumRegionBuilder, StructuredQuantumOptions,
    StructuredQuantumReport, StructuredTerminalError, StructuredTerminalReport, TerminalOptions,
    TerminalStatus, Verified,
    structured_optimize::{self as exact, Context, Wire},
};
use quest_language::{
    classical::ScalarValue,
    semantic::ErrorKind,
    ssa::{self, InstructionKind as K, QuantumFlow, QuantumFlowLimits, QuantumFlowUsage},
};
use std::{collections::BTreeMap, sync::Arc};

type Result<T> = std::result::Result<T, StructuredTerminalError>;

/// Stages which need an additional structured adapter or a known cost comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuredSkippedStage {
    /// Worker beam candidates currently require an ideal or bound finite circuit.
    WorkerSearch,
    /// Runtime communication is unknown and the proposed trace would change.
    UnknownCommunication,
    /// At least one changed block has a dynamic gate or an opaque call.
    StaticCostComparison,
    /// The complete terminal candidate did not strictly improve any block.
    NoStrictImprovement,
}

#[derive(Debug, Clone)]
pub struct StructuredPipelineReport {
    input: ssa::SnapshotId,
    flow: Option<QuantumFlowUsage>,
    exact: Option<StructuredQuantumReport>,
    terminal: Option<StructuredTerminalReport>,
    status: TerminalStatus,
    skipped: Vec<StructuredSkippedStage>,
}
impl StructuredPipelineReport {
    #[must_use]
    pub const fn input_snapshot(&self) -> ssa::SnapshotId {
        self.input
    }
    #[must_use]
    pub const fn flow_usage(&self) -> Option<QuantumFlowUsage> {
        self.flow
    }
    #[must_use]
    pub const fn exact_report(&self) -> Option<&StructuredQuantumReport> {
        self.exact.as_ref()
    }
    #[must_use]
    pub const fn terminal_report(&self) -> Option<&StructuredTerminalReport> {
        self.terminal.as_ref()
    }
    #[must_use]
    pub const fn status(&self) -> TerminalStatus {
        self.status
    }
    #[must_use]
    pub fn skipped_stages(&self) -> &[StructuredSkippedStage] {
        &self.skipped
    }
}

#[derive(Debug)]
pub struct StructuredPipelineOutcome {
    program: Program<Verified>,
    report: Arc<StructuredPipelineReport>,
    allowances: Vec<BudgetLease>,
}
impl StructuredPipelineOutcome {
    #[must_use]
    pub const fn program(&self) -> &Program<Verified> {
        &self.program
    }
    #[must_use]
    pub fn report(&self) -> &StructuredPipelineReport {
        &self.report
    }
    #[must_use]
    pub fn into_program(self) -> Program<Verified> {
        self.program
    }
    pub(crate) fn into_parts(
        self,
    ) -> (
        Program<Verified>,
        Arc<StructuredPipelineReport>,
        Vec<BudgetLease>,
    ) {
        (self.program, self.report, self.allowances)
    }
}

fn amount(value: usize) -> Result<u64> {
    u64::try_from(value).map_err(|_| LanguageError::Budget("structured pipeline count").into())
}
fn exhausted(error: &StructuredTerminalError) -> Option<TerminalStatus> {
    match error {
        StructuredTerminalError::Circuit(crate::Error::Budget(reason))
        | StructuredTerminalError::Language(LanguageError::Budget(reason)) => {
            Some(if reason.contains("work") {
                TerminalStatus::WorkLimit
            } else {
                TerminalStatus::StorageLimit
            })
        }
        StructuredTerminalError::Quantum(
            crate::StructuredQuantumError::Circuit(crate::Error::Budget(reason))
            | crate::StructuredQuantumError::Language(LanguageError::Budget(reason)),
        ) => Some(if reason.contains("work") {
            TerminalStatus::WorkLimit
        } else {
            TerminalStatus::StorageLimit
        }),
        StructuredTerminalError::Language(LanguageError::Semantic(error))
        | StructuredTerminalError::Quantum(crate::StructuredQuantumError::Language(
            LanguageError::Semantic(error),
        )) if error.kind == ErrorKind::Resource => Some(TerminalStatus::StorageLimit),
        _ => None,
    }
}

fn analyze(program: &Program<Verified>, ledger: &BudgetLedger) -> Result<QuantumFlowUsage> {
    let (work, bytes) = ledger.remaining()?;
    let work = work.min(1_000_000);
    let bytes = bytes.min(64 * 1024 * 1024);
    if work == 0 {
        return Err(LanguageError::Budget("structured analysis work").into());
    }
    if bytes == 0 {
        return Err(LanguageError::Budget("structured analysis storage").into());
    }
    let _scratch = ledger.reserve(BudgetCategory::Verification, 0, bytes)?;
    let allowance = ledger.reserve_work_allowance(work)?;
    let limits = QuantumFlowLimits::new(
        1_000_000,
        200_000,
        1_000_000,
        usize::try_from(work).map_err(|_| LanguageError::Budget("structured analysis work"))?,
        usize::try_from(bytes).map_err(|_| LanguageError::Budget("structured analysis storage"))?,
    )
    .map_err(LanguageError::from)?;
    let analysis = QuantumFlow::analyze(program.ssa(), limits).map_err(|error| {
        if error.kind == ErrorKind::Resource && error.message.contains("work") {
            LanguageError::Budget("structured analysis work")
        } else {
            LanguageError::from(error)
        }
    })?;
    let usage = analysis.usage();
    allowance.commit(amount(usage.work)?)?;
    Ok(usage)
}

fn exact_candidate(
    program: &Program<Verified>,
    ledger: &BudgetLedger,
) -> Result<(Program<Verified>, StructuredQuantumReport, BudgetLease)> {
    crate::structured_terminal::charge_verification(program.ssa().program(), ledger)?;
    let (work, bytes) = ledger.remaining()?;
    let work = work.min(1_000_000);
    let bytes = bytes.min(64 * 1024 * 1024);
    if amount(
        program
            .retained_bytes()?
            .checked_mul(2)
            .ok_or(LanguageError::Budget("structured exact storage"))?,
    )? > bytes
    {
        return Err(LanguageError::Budget("structured exact storage").into());
    }
    let mut allowance = ledger.reserve(BudgetCategory::Candidate, 0, bytes)?;
    let work_allowance = ledger.reserve_work_allowance(work)?;
    let options = StructuredQuantumOptions {
        work: usize::try_from(work).map_err(|_| LanguageError::Budget("structured exact work"))?,
        storage_bytes: usize::try_from(bytes)
            .map_err(|_| LanguageError::Budget("structured exact storage"))?,
        ..StructuredQuantumOptions::default()
    };
    let (candidate, report) = program.clone().optimize_quantum(options)?;
    work_allowance.commit(amount(report.work)?)?;
    let retained = report.rewrites.iter().try_fold(
        candidate
            .retained_bytes()?
            .checked_add(std::mem::size_of::<StructuredQuantumReport>())
            .and_then(|bytes| {
                bytes.checked_add(
                    report
                        .rewrites
                        .capacity()
                        .checked_mul(std::mem::size_of::<crate::StructuredQuantumRewrite>())?,
                )
            })
            .ok_or(LanguageError::Budget("structured exact report"))?,
        |bytes, rewrite| {
            bytes
                .checked_add(
                    rewrite
                        .inputs
                        .capacity()
                        .checked_mul(std::mem::size_of::<crate::StructuredOccurrence>())
                        .ok_or(LanguageError::Budget("structured exact report"))?,
                )
                .and_then(|bytes| {
                    bytes.checked_add(
                        rewrite
                            .outputs
                            .capacity()
                            .checked_mul(std::mem::size_of::<ssa::ValueId>())?,
                    )
                })
                .ok_or(LanguageError::Budget("structured exact report"))
        },
    )?;
    allowance.shrink(amount(retained)?)?;
    Ok((candidate, report, allowance))
}

fn constants(program: &Program<Verified>) -> BTreeMap<ssa::ValueId, ScalarValue> {
    program
        .ssa()
        .blocks()
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|item| match (&item.kind, item.results.as_slice()) {
            (K::Constant(value), [result]) => Some((result.id, *value)),
            (K::Capture { index, .. }, [result]) => program
                .captures()
                .get(*index)
                .map(|value| (result.id, *value)),
            _ => None,
        })
        .collect()
}

enum Item {
    Gate(exact::ResolvedGate),
    Oracle(crate::OracleFragment, Vec<Wire>),
}

// A block executes its instructions equally often. Comparing every original
// block independently avoids assigning guessed frequencies to branches/loops.
#[expect(
    clippy::too_many_lines,
    reason = "Keep checked SSA operand mapping and local cost construction together"
)]
fn block_cost(
    program: &Program<Verified>,
    block: usize,
    options: &OptimizationOptions,
    ledger: &BudgetLedger,
) -> Result<Option<crate::CostComponents>> {
    let constants = constants(program);
    let context = Context {
        slots: program.ssa().slots(),
        constants: &constants,
    };
    let mut wires = Vec::new();
    let mut items = Vec::new();
    for instruction in &program
        .ssa()
        .blocks()
        .get(block)
        .ok_or(crate::Error::InvalidId)?
        .instructions
    {
        let item = match &instruction.kind {
            K::Gate { .. } => {
                let Some(gate) = exact::gate(instruction, &context) else {
                    return Ok(None);
                };
                for (wire, place) in gate.wires.iter().zip(&gate.places) {
                    if !wires.iter().any(|(known, _)| known == wire) {
                        wires.push((*wire, place.clone()));
                    }
                }
                Item::Gate(gate)
            }
            K::Call {
                region,
                arguments,
                controls,
                modifiers,
                ..
            } => {
                let Some(capture) = program
                    .ssa()
                    .program()
                    .regions
                    .get(region.index())
                    .and_then(|region| region.oracle.as_ref())
                else {
                    return Ok(None);
                };
                if !controls.is_empty()
                    || !modifiers.is_empty()
                    || options.target().profile() == CostProfile::CliffordTV1
                {
                    return Ok(None);
                }
                let fragment = program
                    .oracle_bank()
                    .get(&capture.index())
                    .ok_or(crate::Error::InvalidId)?
                    .clone();
                let mut targets = Vec::new();
                for argument in arguments {
                    let ssa::CallArgument::Reference { place, .. } = argument else {
                        return Ok(None);
                    };
                    let Some(wire) = exact::wire(place, &context) else {
                        return Ok(None);
                    };
                    if !wires.iter().any(|(known, _)| *known == wire) {
                        wires.push((wire, place.clone()));
                    }
                    targets.push(wire);
                }
                Item::Oracle(fragment, targets)
            }
            _ => continue,
        };
        items.push(item);
    }
    if wires.len() > options.target().deployment().width() {
        return Ok(None);
    }
    let mut builder = QuantumRegionBuilder::new(wires.len().max(1), 0)?;
    for item in items {
        match item {
            Item::Gate(gate) => {
                crate::structured_terminal::append_local_gate(&mut builder, &gate, &wires)?;
            }
            Item::Oracle(fragment, targets) => {
                let targets = targets
                    .iter()
                    .map(|wire| {
                        builder.qubit(
                            wires
                                .iter()
                                .position(|(known, _)| known == wire)
                                .ok_or(crate::Error::InvalidId)?,
                        )
                    })
                    .collect::<crate::Result<Vec<_>>>()?;
                builder.oracle(&fragment, &targets, &[])?;
            }
        }
    }
    let bound = builder.finish()?.bind(&[])?;
    Ok(Some(crate::beam::score_bound(
        &bound,
        options.target(),
        ledger,
    )?))
}

fn compare(
    candidate: &Program<Verified>,
    baseline: &Program<Verified>,
    blocks: usize,
    options: &OptimizationOptions,
    ledger: &BudgetLedger,
) -> Result<CostComparison> {
    let bytes = candidate
        .retained_bytes()?
        .checked_add(baseline.retained_bytes()?)
        .and_then(|bytes| bytes.checked_mul(8))
        .ok_or(LanguageError::Budget("structured score storage"))?;
    let nodes = candidate
        .ssa()
        .blocks()
        .iter()
        .chain(baseline.ssa().blocks())
        .try_fold(0usize, |sum, block| {
            block.instructions.iter().try_fold(sum, |sum, item| {
                let operands = match &item.kind {
                    K::Gate {
                        arguments,
                        operands,
                        modifiers,
                        ..
                    } => arguments
                        .len()
                        .checked_add(operands.len())
                        .and_then(|n| n.checked_add(modifiers.len())),
                    K::Call {
                        arguments,
                        controls,
                        modifiers,
                        ..
                    } => arguments
                        .len()
                        .checked_add(controls.len())
                        .and_then(|n| n.checked_add(modifiers.len())),
                    _ => Some(0),
                }
                .ok_or(LanguageError::Budget("structured score work"))?;
                sum.checked_add(operands)
                    .and_then(|sum| sum.checked_add(1))
                    .ok_or(LanguageError::Budget("structured score work"))
            })
        })?;
    let work = nodes
        .checked_mul(nodes)
        .and_then(|value| value.checked_mul(64))
        .and_then(|value| value.checked_add(bytes))
        .ok_or(LanguageError::Budget("structured score work"))?;
    let _scratch = ledger.reserve(BudgetCategory::Verification, amount(work)?, amount(bytes)?)?;
    let mut better = false;
    for block in 0..blocks {
        if candidate.ssa().blocks().get(block) == baseline.ssa().blocks().get(block) {
            continue;
        }
        let Some(candidate) = block_cost(candidate, block, options, ledger)? else {
            return Ok(CostComparison::Unscorable);
        };
        let Some(baseline) = block_cost(baseline, block, options, ledger)? else {
            return Ok(CostComparison::Unscorable);
        };
        match options.target().compare(&candidate, &baseline)? {
            CostComparison::Better => better = true,
            CostComparison::Equal => (),
            other => return Ok(other),
        }
    }
    Ok(if better {
        CostComparison::Better
    } else {
        CostComparison::Equal
    })
}

impl Program<Verified> {
    /// Run bounded value-flow analysis, exact cleanup and terminal fusion.
    /// Exact candidates and originals receive the same terminal policy. Publish
    /// only when every comparable block is no worse and at least one improves.
    /// Worker search remains available on finite ideal/bound circuit entrypoints.
    /// # Errors
    /// Rejects malformed publications; deterministic exhaustion retains the last
    /// fully admitted publication and records an incomplete status.
    #[expect(
        clippy::too_many_lines,
        reason = "Keep transactional stage selection and proof ownership together"
    )]
    pub fn optimize_structured(
        self,
        options: &OptimizationOptions,
        ledger: &BudgetLedger,
    ) -> Result<StructuredPipelineOutcome> {
        crate::structured_terminal::validate_target(&self, options)?;
        let blocks = self.ssa().blocks().len();
        let mut report = StructuredPipelineReport {
            input: self.ssa().snapshot(),
            flow: None,
            exact: None,
            terminal: None,
            status: TerminalStatus::Complete,
            skipped: vec![StructuredSkippedStage::WorkerSearch],
        };
        let input = ledger.reserve(
            BudgetCategory::Candidate,
            0,
            amount(self.retained_bytes()?)?,
        )?;
        let original = |program, report| StructuredPipelineOutcome {
            program,
            report: Arc::new(report),
            allowances: vec![input],
        };
        match analyze(&self, ledger) {
            Ok(usage) => report.flow = Some(usage),
            Err(error) => {
                let Some(status) = exhausted(&error) else {
                    return Err(error);
                };
                report.status = status;
                return Ok(original(self, report));
            }
        }
        let candidate = if options.target().deployment().distributed() {
            report
                .skipped
                .push(StructuredSkippedStage::UnknownCommunication);
            None
        } else {
            match exact_candidate(&self, ledger) {
                Ok(candidate) => Some(candidate),
                Err(error) => {
                    let Some(status) = exhausted(&error) else {
                        return Err(error);
                    };
                    report.status = status;
                    return Ok(original(self, report));
                }
            }
        };
        let copy = match ledger.reserve(
            BudgetCategory::Candidate,
            0,
            amount(self.retained_bytes()?)?,
        ) {
            Ok(copy) => copy,
            Err(error) => {
                report.status = TerminalStatus::StorageLimit;
                if !matches!(error, crate::Error::Budget(_)) {
                    return Err(error.into());
                }
                return Ok(original(self, report));
            }
        };
        let baseline = match self
            .clone()
            .fuse_terminal(TerminalOptions::default(), options, ledger)
        {
            Ok(baseline) => baseline,
            Err(error) => {
                let Some(status) = exhausted(&error) else {
                    return Err(error);
                };
                report.status = status;
                return Ok(original(self, report));
            }
        };
        drop(copy);
        report.terminal = Some(baseline.report().clone());
        report.status = baseline.report().status();
        if let Some((candidate, exact_report, allowance)) = candidate {
            let outcome = match candidate.fuse_terminal(TerminalOptions::default(), options, ledger)
            {
                Ok(outcome) => outcome,
                Err(error) => {
                    let Some(status) = exhausted(&error) else {
                        return Err(error);
                    };
                    report.status = status;
                    let (program, allowances) = baseline.into_parts();
                    return Ok(StructuredPipelineOutcome {
                        program,
                        report: Arc::new(report),
                        allowances,
                    });
                }
            };
            if outcome.report().status() != TerminalStatus::Complete {
                report.status = outcome.report().status();
            }
            match compare(
                outcome.program(),
                baseline.program(),
                blocks,
                options,
                ledger,
            ) {
                Ok(CostComparison::Better) => {
                    report.terminal = Some(outcome.report().clone());
                    report.status = outcome.report().status();
                    report.exact = Some(exact_report);
                    let (program, mut allowances) = outcome.into_parts();
                    allowances.push(allowance);
                    return Ok(StructuredPipelineOutcome {
                        program,
                        report: Arc::new(report),
                        allowances,
                    });
                }
                Ok(CostComparison::Unscorable) => report
                    .skipped
                    .push(StructuredSkippedStage::StaticCostComparison),
                Ok(_) => report
                    .skipped
                    .push(StructuredSkippedStage::NoStrictImprovement),
                Err(error) => {
                    let Some(status) = exhausted(&error) else {
                        return Err(error);
                    };
                    report.status = status;
                }
            }
        }
        let (program, allowances) = baseline.into_parts();
        Ok(StructuredPipelineOutcome {
            program,
            report: Arc::new(report),
            allowances,
        })
    }
}
