//! Transactional terminal fusion for mixed executable regions.
use crate::structured_optimize::{self as exact, Context, ResolvedGate, Wire};
use crate::{
    Angle, ApproximationMode, BoundGate, BudgetCategory, BudgetLease, BudgetLedger, Control,
    ControlState, Gate, LanguageError, MatrixPolicy, OptimizationOptions, OracleFragment, Program,
    QuantumRegionBuilder, StructuredQuantumError, TerminalOptions, TerminalStatus, Verified,
};
#[allow(unused_imports)]
use crate::{
    BoundParityPasses, ExactPasses, LinearPasses, NumericalPasses, OracleExport, ParityPasses,
    TerminalPasses,
};
use quest_language::{
    GateKind,
    classical::ScalarValue,
    semantic::CompileLimits,
    ssa::{self, InstructionKind as K},
};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum StructuredTerminalError {
    #[error(transparent)]
    Circuit(#[from] crate::Error),
    #[error(transparent)]
    Language(#[from] LanguageError),
    #[error(transparent)]
    Quantum(#[from] StructuredQuantumError),
}
type Result<T> = std::result::Result<T, StructuredTerminalError>;
#[derive(Debug, Clone)]
pub struct StructuredTerminalReport {
    input: ssa::SnapshotId,
    output: ssa::SnapshotId,
    windows: usize,
    rewrites: Vec<StructuredFusion>,
    status: TerminalStatus,
}
/// Immediate source occurrences and the fixed ordered oracle interface.
#[derive(Debug, Clone)]
pub struct StructuredFusion {
    pub inputs: Vec<crate::StructuredOccurrence>,
    pub interface: Vec<ssa::Place>,
    pub capture: usize,
}
impl StructuredTerminalReport {
    #[must_use]
    pub const fn input_snapshot(&self) -> ssa::SnapshotId {
        self.input
    }
    #[must_use]
    pub const fn output_snapshot(&self) -> ssa::SnapshotId {
        self.output
    }
    #[must_use]
    pub const fn fused_windows(&self) -> usize {
        self.windows
    }
    #[must_use]
    pub const fn status(&self) -> TerminalStatus {
        self.status
    }
    #[must_use]
    pub const fn rounding_changed(&self) -> bool {
        self.windows != 0
    }
    #[must_use]
    pub fn rewrites(&self) -> &[StructuredFusion] {
        &self.rewrites
    }
}
#[derive(Debug)]
pub struct StructuredTerminalOutcome {
    program: Program<Verified>,
    report: StructuredTerminalReport,
    allowances: Vec<BudgetLease>,
}
impl StructuredTerminalOutcome {
    #[must_use]
    pub const fn program(&self) -> &Program<Verified> {
        &self.program
    }
    #[must_use]
    pub const fn report(&self) -> &StructuredTerminalReport {
        &self.report
    }
    #[must_use]
    pub fn into_program(self) -> Program<Verified> {
        self.program
    }
    pub(crate) fn into_parts(self) -> (Program<Verified>, Vec<BudgetLease>) {
        (self.program, self.allowances)
    }
}
fn count(value: usize) -> Result<u64> {
    u64::try_from(value).map_err(|_| LanguageError::Budget("structured terminal count").into())
}
#[expect(
    clippy::redundant_pub_crate,
    reason = "Shared admission must not be reexported by the public glob"
)]
pub(crate) fn validate_target(
    program: &Program<Verified>,
    options: &OptimizationOptions,
) -> Result<()> {
    let entry = program.ssa().program().entry;
    let width = program
        .ssa()
        .slots()
        .iter()
        .filter(|slot| slot.region == entry)
        .try_fold(0usize, |sum, slot| match slot.ty {
            ssa::Type::Qubit(count) => sum
                .checked_add(count)
                .ok_or(LanguageError::Budget("terminal target width")),
            _ => Ok(sum),
        })?;
    if width != options.target().deployment().width() {
        return Err(crate::Error::Budget("terminal target width").into());
    }
    Ok(())
}
impl Program<Verified> {
    /// Fuse static windows in entry and mixed subroutines. Coherent user-gate
    /// bodies remain symbolic so inverse and negative-power calls stay valid.
    /// # Errors
    /// Rejects malformed publication or input admission. Deterministic exhaustion
    /// keeps the original verified SSA and oracle bank together.
    pub fn fuse_terminal(
        self,
        terminal: TerminalOptions,
        options: &OptimizationOptions,
        ledger: &BudgetLedger,
    ) -> Result<StructuredTerminalOutcome> {
        validate_target(&self, options)?;
        let snapshot = self.ssa().snapshot();
        let retained = self.retained_bytes()?;
        let input = ledger.reserve(BudgetCategory::Candidate, 0, count(retained)?)?;
        let mut outcome = StructuredTerminalOutcome {
            program: self,
            report: StructuredTerminalReport {
                input: snapshot,
                output: snapshot,
                windows: 0,
                rewrites: Vec::new(),
                status: TerminalStatus::Complete,
            },
            allowances: vec![input],
        };
        if matches!(options.approximation(), ApproximationMode::Global(_)) {
            return Ok(outcome);
        }
        match transform(&outcome.program, terminal, options, ledger) {
            Ok(Some((program, rewrites, status, allowances))) => {
                outcome.report.output = program.ssa().snapshot();
                outcome.program = program;
                outcome.report.windows = rewrites.len();
                outcome.report.rewrites = rewrites;
                outcome.report.status = status;
                outcome.allowances.extend(allowances);
            }
            Ok(None) => (),
            Err(
                StructuredTerminalError::Circuit(crate::Error::Budget(reason))
                | StructuredTerminalError::Language(LanguageError::Budget(reason)),
            ) => {
                outcome.report.status = if reason.contains("work") {
                    TerminalStatus::WorkLimit
                } else {
                    TerminalStatus::StorageLimit
                };
            }
            Err(StructuredTerminalError::Language(LanguageError::Semantic(error)))
                if error.kind == quest_language::semantic::ErrorKind::Resource =>
            {
                outcome.report.status = TerminalStatus::StorageLimit;
            }
            Err(error) => return Err(error),
        }
        Ok(outcome)
    }
}

fn local_program(
    gates: &[ResolvedGate],
    wires: &[(Wire, ssa::Place)],
) -> crate::Result<crate::BoundRegion> {
    let mut builder = QuantumRegionBuilder::new(wires.len(), 0)?;
    for resolved in gates {
        append_local_gate(&mut builder, resolved, wires)?;
    }
    builder.finish()?.bind(&[])
}

#[expect(
    clippy::redundant_pub_crate,
    reason = "Shared adapter must not be reexported by the public glob"
)]
pub(crate) fn append_local_gate(
    builder: &mut QuantumRegionBuilder,
    resolved: &ResolvedGate,
    wires: &[(Wire, ssa::Place)],
) -> crate::Result<()> {
    let local = resolved
        .wires
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
    let controls = local
        .iter()
        .zip(&resolved.controls)
        .map(|(&qubit, &positive)| {
            Control::new(
                qubit,
                if positive {
                    ControlState::One
                } else {
                    ControlState::Zero
                },
            )
        })
        .collect::<Vec<_>>();
    let parameters = resolved
        .parameters
        .iter()
        .copied()
        .map(f64::from_bits)
        .collect::<Vec<_>>();
    if resolved.gate == GateKind::GlobalPhase {
        let angle = Angle::radians(*parameters.first().ok_or(crate::Error::InvalidId)?)?;
        builder.global_phase(
            if resolved.inverse {
                angle.negated()?
            } else {
                angle
            },
            &controls,
        )?;
    } else {
        let gate = Gate::from_bound(&BoundGate::from_kind(resolved.gate, &parameters)?)?;
        builder.gate(
            if resolved.inverse {
                gate.adjoint()?
            } else {
                gate
            },
            local.get(controls.len()..).ok_or(crate::Error::InvalidId)?,
            &controls,
        )?;
    }
    Ok(())
}

type Publication = (
    Program<Verified>,
    Vec<StructuredFusion>,
    TerminalStatus,
    Vec<BudgetLease>,
);
#[expect(
    clippy::too_many_lines,
    reason = "Keep SSA and native-fragment preparation in a single atomic publication transaction"
)]
fn transform(
    source: &Program<Verified>,
    terminal: TerminalOptions,
    options: &OptimizationOptions,
    ledger: &BudgetLedger,
) -> Result<Option<Publication>> {
    let retained = source.retained_bytes()?;
    let extra = retained
        .checked_mul(8)
        .ok_or(LanguageError::Budget("structured terminal storage"))?;
    let mut allowances = vec![ledger.reserve(BudgetCategory::Candidate, 0, count(extra)?)?];
    let scan_items = source
        .ssa()
        .blocks()
        .iter()
        .try_fold(0usize, |sum, block| {
            block.instructions.iter().try_fold(sum, |sum, item| {
                let extra = match &item.kind {
                    K::Gate {
                        arguments,
                        operands,
                        modifiers,
                        ..
                    } => arguments
                        .len()
                        .checked_add(operands.len())
                        .and_then(|n| n.checked_add(modifiers.len())),
                    _ => Some(0),
                }
                .ok_or(LanguageError::Budget("terminal scan work"))?;
                sum.checked_add(extra)
                    .and_then(|n| n.checked_add(1))
                    .ok_or(LanguageError::Budget("terminal scan work"))
            })
        })?;
    ledger.reserve(
        BudgetCategory::Verification,
        count(
            scan_items
                .checked_mul(64)
                .ok_or(LanguageError::Budget("terminal scan work"))?,
        )?,
        0,
    )?;
    let constants = source
        .ssa()
        .blocks()
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|item| match (&item.kind, item.results.as_slice()) {
            (K::Constant(value), [result]) => Some((result.id, *value)),
            (K::Capture { index, .. }, [result]) => source
                .captures()
                .get(*index)
                .map(|value| (result.id, *value)),
            _ => None,
        })
        .collect::<BTreeMap<_, ScalarValue>>();
    let mut raw = source.ssa().clone().into_unverified();
    let compile = CompileLimits {
        storage_bytes: usize::try_from(options.limits().max_bytes())
            .map_err(|_| LanguageError::Budget("terminal storage"))?,
        ..CompileLimits::default()
    };
    charge_verification(&raw, ledger)?;
    let mut values = raw.value_allocator(compile).map_err(LanguageError::from)?;
    let mut bank = source.oracle_bank().clone();
    let mut capture = bank.keys().next_back().copied().map_or_else(
        || Ok(source.captures().len()),
        |last| {
            last.checked_add(1)
                .map(|next| next.max(source.captures().len()))
                .ok_or(LanguageError::Budget("terminal capture"))
        },
    )?;
    let original_blocks = raw.blocks.len();
    let mut rewrites = Vec::new();
    let mut status = TerminalStatus::Complete;
    for block_index in 0..original_blocks {
        let block = raw.blocks.get(block_index).ok_or(crate::Error::InvalidId)?;
        if raw
            .regions
            .get(block.region.index())
            .ok_or(crate::Error::InvalidId)?
            .gate
        {
            continue;
        }
        let mut output = Vec::new();
        let instructions = block.instructions.clone();
        let mut offset = 0usize;
        while offset < instructions.len() {
            let context = Context {
                slots: &raw.slots,
                constants: &constants,
            };
            let first = instructions.get(offset).ok_or(crate::Error::InvalidId)?;
            let Some(first_gate) = exact::gate(first, &context) else {
                output.push(first.clone());
                offset = offset
                    .checked_add(1)
                    .ok_or(LanguageError::Budget("terminal offset"))?;
                continue;
            };
            let mut gates = vec![first_gate];
            let mut end = offset
                .checked_add(1)
                .ok_or(LanguageError::Budget("terminal offset"))?;
            let mut last_gate = offset;
            while let Some(item) = instructions.get(end) {
                if gates.len() >= 64 {
                    break;
                }
                if let Some(gate) = exact::gate(item, &context) {
                    gates.push(gate);
                    last_gate = end;
                } else if !matches!(item.kind, K::Constant(_)) {
                    break;
                }
                end = end
                    .checked_add(1)
                    .ok_or(LanguageError::Budget("terminal offset"))?;
            }
            let operands = gates.iter().try_fold(0usize, |sum, gate| {
                sum.checked_add(gate.wires.len())
                    .ok_or(LanguageError::Budget("terminal interface work"))
            })?;
            let interface_work = operands
                .checked_mul(operands)
                .and_then(|n| n.checked_add(gates.len().checked_mul(256)?))
                .ok_or(LanguageError::Budget("terminal interface work"))?;
            let scratch = ledger.reserve(
                BudgetCategory::Candidate,
                count(interface_work)?,
                count(
                    operands
                        .checked_mul(512)
                        .ok_or(LanguageError::Budget("terminal interface storage"))?,
                )?,
            )?;
            let mut wires = Vec::<(Wire, ssa::Place)>::new();
            for gate in &gates {
                for (wire, place) in gate.wires.iter().zip(&gate.places) {
                    if !wires.iter().any(|(known, _)| known == wire) {
                        wires.push((*wire, place.clone()));
                    }
                }
            }
            if gates.len() < 2
                || wires.is_empty()
                || wires.len() > options.target().deployment().width()
            {
                output.extend_from_slice(
                    instructions
                        .get(offset..end)
                        .ok_or(crate::Error::InvalidId)?,
                );
                offset = end;
                continue;
            }
            let bound = local_program(&gates, &wires)?;
            let candidate = bound.schedule_and_fuse_embedded(terminal, options.target(), ledger)?;
            status = match candidate.report().status() {
                TerminalStatus::Complete => status,
                other => other,
            };
            if !candidate.report().rounding_changed() {
                output.extend_from_slice(
                    instructions
                        .get(offset..end)
                        .ok_or(crate::Error::InvalidId)?,
                );
                offset = end;
                continue;
            }
            let bytes = candidate
                .program()
                .retained_bytes()?
                .checked_mul(4)
                .and_then(|n| n.checked_add(wires.len().checked_mul(2048)?))
                .and_then(|n| n.checked_add(gates.len().checked_mul(256)?))
                .ok_or(LanguageError::Budget("terminal fragment storage"))?;
            let lease = ledger.reserve(
                BudgetCategory::Candidate,
                count(
                    gates
                        .len()
                        .checked_mul(128)
                        .ok_or(LanguageError::Budget("terminal fragment work"))?,
                )?,
                count(bytes)?,
            )?;
            let fragment = OracleFragment::from_program(
                candidate.into_program(),
                1e-10,
                MatrixPolicy::default(),
            )?;
            let region = raw
                .append_oracle_region(capture, wires.len(), &mut values, compile)
                .map_err(LanguageError::from)?;
            let kind = K::Call {
                region,
                arguments: wires
                    .iter()
                    .map(|(_, place)| ssa::CallArgument::Reference {
                        place: place.clone(),
                        mutable: true,
                    })
                    .collect(),
                controls: vec![],
                modifiers: vec![],
                memory: first.kind.memory().ok_or(crate::Error::InvalidId)?,
            };
            for (position, item) in instructions
                .get(offset..end)
                .ok_or(crate::Error::InvalidId)?
                .iter()
                .enumerate()
            {
                if matches!(item.kind, K::Constant(_)) {
                    output.push(item.clone());
                }
                if offset.checked_add(position) == Some(last_gate) {
                    output.push(ssa::Instruction {
                        results: item.results.clone(),
                        effect: kind.effect(),
                        accesses: kind.accesses(),
                        kind: kind.clone(),
                        span: first.span,
                    });
                }
            }
            let inputs = instructions
                .get(offset..end)
                .ok_or(crate::Error::InvalidId)?
                .iter()
                .enumerate()
                .filter(|(_, item)| matches!(item.kind, K::Gate { .. }))
                .map(|(position, item)| {
                    Ok(crate::StructuredOccurrence {
                        block: raw
                            .blocks
                            .get(block_index)
                            .ok_or(crate::Error::InvalidId)?
                            .id,
                        instruction: offset
                            .checked_add(position)
                            .ok_or(LanguageError::Budget("terminal source occurrence"))?,
                        memory: exact::memory_result(item)?,
                        span: item.span,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            rewrites.push(StructuredFusion {
                inputs,
                interface: wires.iter().map(|(_, place)| place.clone()).collect(),
                capture,
            });
            bank.insert(capture, fragment);
            capture = capture
                .checked_add(1)
                .ok_or(LanguageError::Budget("terminal capture"))?;
            allowances.push(lease);
            drop(scratch);
            offset = end;
        }
        raw.blocks
            .get_mut(block_index)
            .ok_or(crate::Error::InvalidId)?
            .instructions = output;
    }
    if rewrites.is_empty() {
        return Ok(Some((source.clone(), Vec::new(), status, Vec::new())));
    }
    charge_verification(&raw, ledger)?;
    let verified = raw.verify(compile).map_err(LanguageError::from)?;
    let published = source
        .clone()
        .publish_oracles(verified, bank, compile.storage_bytes)?;
    Ok(Some((published, rewrites, status, allowances)))
}

#[expect(
    clippy::redundant_pub_crate,
    reason = "Shared accounting must not be reexported by the public glob"
)]
pub(crate) fn charge_verification(raw: &ssa::Program, ledger: &BudgetLedger) -> Result<()> {
    let nodes = raw.blocks.iter().try_fold(0usize, |sum, block| {
        sum.checked_add(block.instructions.len())
            .ok_or(LanguageError::Budget("terminal verification work"))
    })?;
    let blocks = raw.blocks.len();
    let work = nodes
        .checked_mul(nodes)
        .and_then(|n| {
            blocks
                .checked_mul(blocks)
                .and_then(|b| b.checked_mul(blocks))
                .and_then(|b| n.checked_add(b))
        })
        .ok_or(LanguageError::Budget("terminal verification work"))?;
    ledger.reserve(BudgetCategory::Verification, count(work)?, 0)?;
    Ok(())
}
