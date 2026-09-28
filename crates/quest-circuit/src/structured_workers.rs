//! Optional certified replacements in static interfaces of structured SSA.
//! Candidate generation never changes the immutable source/export authority.
use crate::structured_optimize::{self as exact, Context, ResolvedGate, Wire};
use crate::{LanguageError, StructuredOccurrence, VerifiedStructuredProgram};
use quest_language::{
    GateKind as G, SourceSpan,
    classical::ScalarValue,
    semantic::CompileLimits,
    ssa::{self, InstructionKind as K, ValueId},
};
use quest_math::{
    ApproxCertificate, ExactCertificate, Gate as M, Limits, Operation, Rational, Sequence,
};
use quest_optimizer_client::Client;
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum StructuredWorkerError {
    #[error(transparent)]
    Language(#[from] LanguageError),
    #[error(transparent)]
    Quantum(#[from] crate::StructuredQuantumError),
    #[error(transparent)]
    Worker(#[from] quest_optimizer_client::Error),
    #[error(transparent)]
    Mathematics(#[from] quest_math::Error),
    #[error(
        "explicit synthesis requires a bound rotation and a static, nonaliased interface at {0:?}"
    )]
    Unbound(StructuredOccurrence),
}
type Result<T> = std::result::Result<T, StructuredWorkerError>;
#[derive(Debug, Clone)]
pub struct StructuredRotationCertificate {
    pub occurrence: StructuredOccurrence,
    /// Target first, followed by signed controls in caller order.
    pub interface: Vec<ssa::Place>,
    pub seed: u64,
    pub certificate: quest_math::ControlledApproxCertificate,
    pub outputs: Vec<ValueId>,
}
#[derive(Debug, Clone, Default)]
pub struct StructuredSynthesisReport {
    pub rotations: Vec<StructuredRotationCertificate>,
    /// An exact classical trace count, or maximum-path count through an acyclic CFG.
    /// A trace must complete without inputs or quantum observations within fixed budgets.
    /// Unproved cycles or observations retain local certificates without a global claim.
    pub operator_error_bound: Option<Rational>,
}
#[derive(Debug, Clone)]
pub struct StructuredExactCertificate {
    pub occurrences: Vec<StructuredOccurrence>,
    pub interface: Vec<ssa::Place>,
    pub seed: u64,
    pub certificate: ExactCertificate,
    pub outputs: Vec<ValueId>,
}
#[derive(Debug, Clone)]
pub struct StructuredSkippedCandidate {
    pub occurrences: Vec<StructuredOccurrence>,
    pub reason: String,
}
#[derive(Debug, Clone, Default)]
pub struct StructuredZxReport {
    pub accepted: Vec<StructuredExactCertificate>,
    pub skipped: Vec<StructuredSkippedCandidate>,
}
fn budget(reason: &'static str) -> StructuredWorkerError {
    LanguageError::Budget(reason).into()
}
fn constants(
    program: &ssa::VerifiedProgram,
    captures: &[ScalarValue],
) -> BTreeMap<ValueId, ScalarValue> {
    program
        .blocks()
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|item| {
            if let (K::Constant(value), [result]) = (&item.kind, item.results.as_slice()) {
                Some((result.id, *value))
            } else if let (K::Capture { index, .. }, [result]) =
                (&item.kind, item.results.as_slice())
            {
                captures.get(*index).map(|value| (result.id, *value))
            } else {
                None
            }
        })
        .collect()
}
fn source(
    block: ssa::BlockId,
    instruction: usize,
    item: &ssa::Instruction,
) -> Result<StructuredOccurrence> {
    Ok(StructuredOccurrence {
        block,
        instruction,
        memory: exact::memory_result(item)?,
        span: item.span,
    })
}
fn check_input(program: &ssa::VerifiedProgram) -> Result<()> {
    let bytes = program
        .retained_bytes()
        .map_err(LanguageError::from)?
        .checked_mul(8)
        .ok_or_else(|| budget("worker IR storage"))?;
    let instructions = program.blocks().iter().try_fold(0usize, |count, block| {
        count
            .checked_add(block.instructions.len())
            .ok_or_else(|| budget("worker IR instructions"))
    })?;
    if bytes > 64 * 1024 * 1024 || instructions > 16_384 {
        return Err(budget("worker IR storage"));
    }
    Ok(())
}
fn interface(gates: &[ResolvedGate]) -> Vec<(Wire, ssa::Place)> {
    let mut wires = Vec::new();
    for gate in gates {
        for (wire, place) in gate.wires.iter().zip(&gate.places) {
            if !wires.iter().any(|(item, _)| item == wire) {
                wires.push((*wire, place.clone()));
            }
        }
    }
    wires
}
fn as_sequence(gates: &[ResolvedGate], wires: &[(Wire, ssa::Place)]) -> Option<Sequence> {
    let mut operations = Vec::new();
    for gate in gates {
        if !gate.arguments.is_empty() {
            return None;
        }
        let kind = match (gate.gate, gate.inverse) {
            (G::Id, _) => continue,
            (G::H, _) => M::H,
            (G::X, _) => M::X,
            (G::Y, _) => M::Y,
            (G::Z, _) => M::Z,
            (G::Swap, _) => M::Swap,
            (G::S, false) => M::S,
            (G::S, true) => M::Sdg,
            (G::T, false) => M::T,
            (G::T, true) => M::Tdg,
            _ => return None,
        };
        let indices = gate
            .wires
            .iter()
            .map(|wire| wires.iter().position(|(item, _)| item == wire))
            .collect::<Option<Vec<_>>>()?;
        let controls = indices
            .iter()
            .zip(&gate.controls)
            .map(|(&qubit, &positive)| quest_math::Control { qubit, positive })
            .collect();
        let targets = indices.get(gate.controls.len()..)?.to_vec();
        operations.push(Operation {
            gate: kind,
            targets,
            controls,
        });
    }
    Some(Sequence {
        qubits: wires.len(),
        operations,
    })
}
fn lower_operation(
    operation: &Operation,
    wires: &[(Wire, ssa::Place)],
    span: Option<SourceSpan>,
) -> Result<Vec<ResolvedGate>> {
    if operation.gate == M::W {
        // Every scalar omega is retained as the exact chronological word H,S,H,S,H,S.
        let target = (0..wires.len())
            .find(|q| !operation.controls.iter().any(|c| c.qubit == *q))
            .ok_or(LanguageError::Unsupported(
                "fully controlled scalar worker phase",
            ))?;
        let mut output = Vec::new();
        for gate in [M::H, M::S, M::H, M::S, M::H, M::S] {
            output.extend(lower_operation(
                &Operation {
                    gate,
                    targets: vec![target],
                    controls: operation.controls.clone(),
                },
                wires,
                span,
            )?);
        }
        return Ok(output);
    }
    let (kind, inverse) = match operation.gate {
        M::H => (G::H, false),
        M::X | M::Cx => (G::X, false),
        M::Y => (G::Y, false),
        M::Z | M::Cz => (G::Z, false),
        M::S => (G::S, false),
        M::Sdg => (G::S, true),
        M::T => (G::T, false),
        M::Tdg => (G::T, true),
        M::Swap => (G::Swap, false),
        M::W => return Err(LanguageError::Unsupported("worker scalar phase").into()),
    };
    let mut controls = operation.controls.clone();
    let targets = if matches!(operation.gate, M::Cx | M::Cz) {
        let [control, target] = operation.targets.as_slice() else {
            return Err(LanguageError::Unsupported("worker binary signature").into());
        };
        controls.push(quest_math::Control {
            qubit: *control,
            positive: true,
        });
        vec![*target]
    } else {
        operation.targets.clone()
    };
    let operands = controls
        .iter()
        .map(|c| c.qubit)
        .chain(targets)
        .map(|q| {
            wires
                .get(q)
                .cloned()
                .ok_or(LanguageError::Unsupported("worker wire"))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(vec![ResolvedGate {
        gate: kind,
        arguments: vec![],
        parameters: vec![],
        places: operands.iter().map(|(_, p)| p.clone()).collect(),
        wires: operands.iter().map(|(w, _)| *w).collect(),
        controls: controls.iter().map(|c| c.positive).collect(),
        inverse,
        span,
    }])
}
fn lower(
    sequence: &Sequence,
    wires: &[(Wire, ssa::Place)],
    span: Option<SourceSpan>,
    limits: Limits,
) -> Result<Vec<ResolvedGate>> {
    let count = sequence
        .operations
        .iter()
        .try_fold(0usize, |count, operation| {
            count
                .checked_add(if operation.gate == M::W { 6 } else { 1 })
                .filter(|count| *count <= 8192)
                .ok_or_else(|| budget("worker lowered gates"))
        })?;
    let mut gates = Vec::new();
    gates
        .try_reserve_exact(count)
        .map_err(|_| budget("worker lowering allocation"))?;
    for operation in &sequence.operations {
        gates.extend(lower_operation(operation, wires, span)?);
    }
    let reconstructed =
        as_sequence(&gates, wires).ok_or(LanguageError::Unsupported("worker lowering"))?;
    quest_math::verify_exact(
        &reconstructed,
        sequence,
        Limits {
            gates: 8192,
            ..limits
        },
    )?;
    Ok(gates)
}
fn emit(
    gates: Vec<ResolvedGate>,
    source: &ssa::Instruction,
    allocator: &mut ssa::ValueAllocator,
) -> Result<(Vec<ssa::Instruction>, Vec<ValueId>, ValueId)> {
    let mut memory = source
        .kind
        .memory()
        .ok_or(LanguageError::Unsupported("worker memory"))?;
    let mut output = Vec::new();
    let mut ids = Vec::new();
    for gate in gates {
        let value = allocator
            .allocate(ssa::Type::Memory)
            .map_err(LanguageError::from)?;
        let mut instruction = exact::emit(gate, source, memory);
        memory = value.id;
        instruction.results = vec![value];
        ids.push(memory);
        output.push(instruction);
    }
    Ok((output, ids, memory))
}
#[derive(Default)]
struct RotationCounter {
    rotations: usize,
}
#[derive(Debug, thiserror::Error)]
#[error("quantum observation or trace count overflow prevents a deterministic bound")]
struct UnprovedTrace;
impl quest_language::vm::QuantumBackend for RotationCounter {
    type Error = UnprovedTrace;
    fn apply_gate(
        &mut self,
        request: quest_language::vm::GateRequest<'_>,
    ) -> std::result::Result<(), Self::Error> {
        if matches!(request.gate, G::Rx | G::Ry | G::Rz) {
            self.rotations = self.rotations.checked_add(1).ok_or(UnprovedTrace)?;
        }
        Ok(())
    }
    fn measure(&mut self, _: usize) -> std::result::Result<bool, Self::Error> {
        Err(UnprovedTrace)
    }
    fn reset(&mut self, _: usize) -> std::result::Result<(), Self::Error> {
        Err(UnprovedTrace)
    }
    fn barrier(&mut self, _: &[usize]) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}
fn trace_count(
    program: &ssa::VerifiedProgram,
    captures: &[ScalarValue],
    steps: usize,
) -> Option<usize> {
    use quest_language::vm::{Interpreter, InterpreterLimits, RunInputs};
    let mut backend = RotationCounter::default();
    // Quantum gates cannot affect classical execution without an observation.
    // Completing this bounded trace proves the same count for every quantum input.
    Interpreter::new(InterpreterLimits {
        steps,
        call_frames: 64,
        storage_bytes: 64 * 1024 * 1024,
    })
    .run(program, &mut backend, &RunInputs::default(), captures)
    .ok()?;
    Some(backend.rotations)
}
fn bound(
    program: &ssa::Program,
    weights: &BTreeMap<ssa::BlockId, usize>,
    epsilon: &Rational,
) -> Option<Rational> {
    // Memoization on both call and CFG dependencies detects back edges; no unproved loop multiplicity.
    fn visit(
        id: ssa::BlockId,
        program: &ssa::Program,
        weights: &BTreeMap<ssa::BlockId, usize>,
        memo: &mut BTreeMap<ssa::BlockId, Option<usize>>,
        depth: usize,
    ) -> Option<usize> {
        if depth > 256 {
            return None;
        }
        if let Some(value) = memo.get(&id) {
            return *value;
        }
        memo.insert(id, None);
        let block = program.blocks.get(id.index())?;
        let mut count = *weights.get(&id).unwrap_or(&0);
        for item in &block.instructions {
            match &item.kind {
                K::Measure { .. } | K::Reset { .. } => return None,
                K::Call { modifiers, .. }
                    if modifiers
                        .iter()
                        .any(|m| matches!(m, ssa::GateModifier::Power(_))) =>
                {
                    return None;
                }
                K::Call { region, .. } => {
                    let callee = program.regions.get(region.index())?;
                    // Captured oracles have empty SSA placeholder regions. Their
                    // numerical bodies have no composed unitary/norm certificate,
                    // so summing local errors through them is not justified.
                    if callee.oracle.is_some() {
                        return None;
                    }
                    count = count.checked_add(visit(
                        callee.entry,
                        program,
                        weights,
                        memo,
                        depth.checked_add(1)?,
                    )?)?;
                }
                _ => {}
            }
        }
        let mut longest = 0usize;
        for edge in block.terminator.as_ref()?.edges() {
            longest = longest.max(visit(
                edge.target,
                program,
                weights,
                memo,
                depth.checked_add(1)?,
            )?);
        }
        count = count.checked_add(longest)?;
        memo.insert(id, Some(count));
        Some(count)
    }
    let entry = program.regions.get(program.entry.index())?.entry;
    let count = visit(entry, program, weights, &mut BTreeMap::new(), 0)?;
    Some(std::ops::Mul::mul(
        epsilon,
        Rational::from_integer(count.into()),
    ))
}
type WireInterface = Vec<(Wire, ssa::Place)>;
fn synthesize_gate(
    resolved: &ResolvedGate,
    bits: u64,
    client: &Client,
    epsilon: f64,
    seed: u64,
    limits: Limits,
) -> Result<(WireInterface, quest_math::ControlledApproxCertificate)> {
    let angle = if resolved.inverse {
        (-f64::from_bits(bits)).to_bits()
    } else {
        bits
    };
    let axis = match resolved.gate {
        G::Rx => quest_math::Axis::X,
        G::Ry => quest_math::Axis::Y,
        _ => quest_math::Axis::Z,
    };
    let certificate: ApproxCertificate = client.synthesize(
        &quest_math::Target {
            axis,
            angle: quest_math::AngleTarget::DyadicRadians { bits: angle },
        },
        epsilon,
        seed,
        limits,
    )?;
    let mut wires = interface(std::slice::from_ref(resolved));
    let target = wires
        .pop()
        .ok_or(LanguageError::Unsupported("rotation target"))?;
    wires.insert(0, target);
    let controls = resolved
        .controls
        .iter()
        .enumerate()
        .map(|(i, &positive)| {
            Ok(quest_math::Control {
                qubit: i.checked_add(1).ok_or_else(|| budget("control index"))?,
                positive,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let certificate =
        quest_math::lift_controlled_rotation(&certificate, wires.len(), 0, &controls, limits)?;
    Ok((wires, certificate))
}
impl VerifiedStructuredProgram {
    /// Synthesize every bound Rx/Ry/Rz occurrence using a full-phase local certificate.
    /// Static nonaliased operands and constant parameters are required; classical
    /// optimization can expose constants first. Source syntax remains the export authority.
    /// # Errors
    /// Rejects unbound rotations, worker/certificate failures and bounded edit resources.
    pub fn synthesize_rotations(
        self,
        client: &Client,
        epsilon: f64,
        seed: u64,
        limits: Limits,
    ) -> Result<(Self, StructuredSynthesisReport)> {
        if !epsilon.is_finite() || epsilon <= 0.0 || epsilon >= 1.0 {
            return Err(LanguageError::Unsupported("synthesis tolerance must lie in (0,1)").into());
        }
        check_input(self.ssa())?;
        let multiplicity = trace_count(self.ssa(), self.captures(), 1_000_000);
        let constants = constants(self.ssa(), self.captures());
        self.transform_ssa(|verified| {
            let mut program = verified.into_unverified();
            let mut allocator = program
                .value_allocator(CompileLimits::default())
                .map_err(LanguageError::from)?;
            let mut replacements = BTreeMap::new();
            let mut report = StructuredSynthesisReport::default();
            let mut weights = BTreeMap::new();
            let context = Context {
                slots: &program.slots,
                constants: &constants,
            };
            let mut output_count = 0usize;
            for block in &mut program.blocks {
                let mut output = Vec::new();
                for (position, item) in block.instructions.iter().enumerate() {
                    if !matches!(
                        item.kind,
                        K::Gate {
                            gate: G::Rx | G::Ry | G::Rz,
                            ..
                        }
                    ) {
                        output.push(item.clone());
                        continue;
                    }
                    let occurrence = source(block.id, position, item)?;
                    let resolved = exact::gate(item, &context)
                        .ok_or_else(|| StructuredWorkerError::Unbound(occurrence.clone()))?;
                    let [bits] = resolved.parameters.as_slice() else {
                        return Err(StructuredWorkerError::Unbound(occurrence));
                    };
                    if report.rotations.len() >= 32 {
                        return Err(budget("synthesis requests"));
                    }
                    let request_seed = seed
                        .checked_add(
                            u64::try_from(report.rotations.len())
                                .map_err(|_| budget("worker seed"))?,
                        )
                        .ok_or_else(|| budget("worker seed"))?;
                    let (wires, certificate) =
                        synthesize_gate(&resolved, *bits, client, epsilon, request_seed, limits)?;
                    let gates = lower(certificate.sequence(), &wires, item.span, limits)?;
                    output_count = output_count
                        .checked_add(gates.len())
                        .ok_or_else(|| budget("worker output"))?;
                    if output_count > 16_384 {
                        return Err(budget("worker output"));
                    }
                    let (instructions, ids, memory) = emit(gates, item, &mut allocator)?;
                    output.extend(instructions);
                    replacements.insert(occurrence.memory, memory);
                    let count = weights.entry(block.id).or_insert(0usize);
                    *count = count
                        .checked_add(1)
                        .ok_or_else(|| budget("rotation weights"))?;
                    report.rotations.push(StructuredRotationCertificate {
                        occurrence,
                        interface: wires.into_iter().map(|(_, p)| p).collect(),
                        seed: request_seed,
                        certificate,
                        outputs: ids,
                    });
                }
                block.instructions = output;
            }
            exact::remap(
                &mut program,
                &replacements,
                &mut exact::Budget {
                    used: 0,
                    limit: 2_000_000,
                },
            )?;
            let epsilon = quest_math::dyadic_from_bits(epsilon.to_bits(), limits)?;
            report.operator_error_bound = multiplicity
                .map(|count| std::ops::Mul::mul(&epsilon, Rational::from_integer(count.into())))
                .or_else(|| bound(&program, &weights, &epsilon));
            Ok((
                program
                    .verify(CompileLimits::default())
                    .map_err(LanguageError::from)?,
                report,
            ))
        })
    }
}
impl VerifiedStructuredProgram {
    /// Request exactly checked ZX candidates for static Clifford+T windows.
    /// Effects, calls, dynamic operands and parameters stop a window. Failed,
    /// unsupported or unprofitable candidates keep all original instructions.
    /// # Errors
    /// Rejects bounded edit resource exhaustion or failed final SSA verification.
    pub fn optimize_zx(
        self,
        client: &Client,
        seed: u64,
        limits: Limits,
    ) -> Result<(Self, StructuredZxReport)> {
        if let Err(error) = check_input(self.ssa()) {
            return Ok((
                self,
                StructuredZxReport {
                    skipped: vec![StructuredSkippedCandidate {
                        occurrences: vec![],
                        reason: error.to_string(),
                    }],
                    ..StructuredZxReport::default()
                },
            ));
        }
        let constants = constants(self.ssa(), self.captures());
        self.transform_ssa(|verified| {
            let mut program = verified.into_unverified();
            let allocator = program
                .value_allocator(CompileLimits::default())
                .map_err(LanguageError::from)?;
            let mut edit = ZxEdit {
                context: Context {
                    slots: &program.slots,
                    constants: &constants,
                },
                client,
                seed,
                limits,
                requests: 0,
                allocator,
                replacements: BTreeMap::new(),
                report: StructuredZxReport::default(),
            };
            for block in &mut program.blocks {
                edit.block(block)?;
            }
            let ZxEdit {
                replacements,
                report,
                ..
            } = edit;
            exact::remap(
                &mut program,
                &replacements,
                &mut exact::Budget {
                    used: 0,
                    limit: 2_000_000,
                },
            )?;
            Ok((
                program
                    .verify(CompileLimits::default())
                    .map_err(LanguageError::from)?,
                report,
            ))
        })
    }
}

fn scan_window(
    original: &[ssa::Instruction],
    offset: usize,
    context: &Context<'_>,
    limits: Limits,
    requests: u64,
) -> Result<(Vec<ResolvedGate>, Vec<usize>, usize)> {
    let mut gates = Vec::new();
    let mut positions = Vec::new();
    let mut end = offset;
    for (position, item) in original.iter().enumerate().skip(offset) {
        if gates.len() >= 128 || requests >= 32 {
            break;
        }
        if let Some(gate) = exact::gate(item, context) {
            gates.push(gate);
            let wires = interface(&gates);
            if wires.len() > limits.qubits.min(4) || as_sequence(&gates, &wires).is_none() {
                gates.pop();
                break;
            }
            positions.push(position);
        } else if gates.is_empty() || !matches!(item.kind, K::Constant(_)) {
            break;
        }
        end = position
            .checked_add(1)
            .ok_or_else(|| budget("window index"))?;
    }
    Ok((gates, positions, end))
}

struct ZxEdit<'a> {
    context: Context<'a>,
    client: &'a Client,
    seed: u64,
    limits: Limits,
    requests: u64,
    allocator: ssa::ValueAllocator,
    replacements: BTreeMap<ValueId, ValueId>,
    report: StructuredZxReport,
}
impl ZxEdit<'_> {
    fn block(&mut self, block: &mut ssa::Block) -> Result<()> {
        let original = std::mem::take(&mut block.instructions);
        let mut output = Vec::new();
        let mut offset = 0usize;
        while let Some(item) = original.get(offset) {
            let (gates, positions, end) =
                scan_window(&original, offset, &self.context, self.limits, self.requests)?;
            if gates.is_empty() {
                output.push(item.clone());
                offset = offset
                    .checked_add(1)
                    .ok_or_else(|| budget("window index"))?;
                continue;
            }
            let wires = interface(&gates);
            let sequence =
                as_sequence(&gates, &wires).ok_or(LanguageError::Unsupported("ZX region"))?;
            let occurrences = positions
                .iter()
                .map(|&position| {
                    source(
                        block.id,
                        position,
                        original
                            .get(position)
                            .ok_or(LanguageError::Unsupported("ZX source"))?,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let request_seed = self
                .seed
                .checked_add(self.requests)
                .ok_or_else(|| budget("worker seed"))?;
            self.requests = self
                .requests
                .checked_add(1)
                .ok_or_else(|| budget("worker requests"))?;
            let candidate = self
                .client
                .optimize_zx(&sequence, request_seed, self.limits)
                .map_err(StructuredWorkerError::from)
                .and_then(|certificate| {
                    let gates = lower(certificate.candidate(), &wires, item.span, self.limits)?;
                    Ok((certificate, gates))
                });
            let window = original
                .get(offset..end)
                .ok_or(LanguageError::Unsupported("ZX window"))?;
            match candidate {
                Ok((certificate, gates)) if gates.len() < positions.len() => {
                    output.extend(
                        window
                            .iter()
                            .filter(|item| matches!(item.kind, K::Constant(_)))
                            .cloned(),
                    );
                    let (instructions, ids, memory) = emit(gates, item, &mut self.allocator)?;
                    output.extend(instructions);
                    for occurrence in &occurrences {
                        self.replacements.insert(occurrence.memory, memory);
                    }
                    self.report.accepted.push(StructuredExactCertificate {
                        occurrences,
                        interface: wires.into_iter().map(|(_, p)| p).collect(),
                        seed: request_seed,
                        certificate,
                        outputs: ids,
                    });
                }
                result => {
                    let reason = result.err().map_or_else(
                        || "candidate does not reduce native gate calls".into(),
                        |error| error.to_string(),
                    );
                    self.report.skipped.push(StructuredSkippedCandidate {
                        occurrences,
                        reason,
                    });
                    output.extend_from_slice(window);
                }
            }
            offset = end;
        }
        block.instructions = output;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[gtest]
    fn deterministic_trace_proves_loops_calls_and_rejects_observation_or_exhaustion()
    -> googletest::Result<()> {
        for (source, expected) in [
            (
                "gate turn q { rz(0.17) q; } qubit q; int n=0; while(n<3){pow(2) @ turn q; n+=1;}",
                Some(6),
            ),
            (
                "qubit q; for int i in [0:1] { for int j in [0:2] { rx(0.2) q; } }",
                Some(6),
            ),
            ("qubit q; h q; barrier q; rz(0.17) q;", Some(1)),
            ("qubit q; bit b=measure q; rz(0.17) q;", None),
            ("qubit q; reset q; rz(0.17) q;", None),
            ("qubit q; input bool choice; if(choice){rz(0.17) q;}", None),
            ("qubit q; while(true){rz(0.17) q;}", None),
        ] {
            let verified = crate::StructuredProgram::parse(source, "trace.qasm")?.verify()?;
            expect_eq!(trace_count(verified.ssa(), &[], 1_000), expected);
        }
        Ok(())
    }

    #[gtest]
    fn scalar_word_lowering_preserves_signed_control_phase_exactly() -> googletest::Result<()> {
        let verified =
            crate::StructuredProgram::parse("qubit a; qubit b; qubit c;", "scalar.qasm")?
                .verify()?;
        let wires = verified
            .ssa()
            .slots()
            .iter()
            .map(|slot| {
                (
                    Wire {
                        slot: slot.id,
                        index: 0,
                    },
                    ssa::Place {
                        slot: slot.id,
                        indices: vec![],
                    },
                )
            })
            .collect::<Vec<_>>();
        for controls in [
            vec![],
            vec![
                quest_math::Control {
                    qubit: 0,
                    positive: false,
                },
                quest_math::Control {
                    qubit: 2,
                    positive: true,
                },
            ],
        ] {
            let sequence = Sequence {
                qubits: 3,
                operations: vec![Operation {
                    gate: M::W,
                    targets: vec![],
                    controls,
                }],
            };
            let lowered = lower(&sequence, &wires, None, Limits::default())?;
            expect_eq!(lowered.len(), 6);
            let reconstructed = as_sequence(&lowered, &wires).expect("exact lowering");
            quest_math::verify_exact(&reconstructed, &sequence, Limits::default())?;
            expect_true!(
                quest_math::verify_exact(
                    &reconstructed,
                    &Sequence {
                        qubits: 3,
                        operations: vec![]
                    },
                    Limits::default()
                )
                .is_err()
            );
        }
        Ok(())
    }
}
