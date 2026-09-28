//! Guarded exact quantum transformations within independently verified SSA blocks.
//! CFG edges never enter a quantum dependency window. Floating constants grant
//! structural inverse equality only; they never become ideal rational-pi angles.
//!
//! Scalar statically indexed storage is identified by its resolved slot and
//! normalized index. Reference parameters, broadcasts, dynamic indices, powers,
//! unsupported effects and calls stop a window. Only nontrapping SSA constants
//! may be hoisted before a replacement; potentially trapping classical work is
//! retained as a boundary. Guarded cancellation can cross only disjoint gates or
//! known diagonal gates. Clifford+T affine windows additionally use exact phase
//! polynomial and binary-map verification. Quarter-turn output is encoded with
//! parameterless gates, including the proved scalar word `(S H)^3 = omega I`.
//!
//! Strictly shorter candidates reuse owned input memory-result IDs and report
//! every original block position, ID and source span. Removed tokens are remapped
//! in subsequent effects and CFG edges; effect/access metadata is recomputed.
//! Independent SSA verification admits the complete transformed graph before it
//! is returned. Original syntax, source snapshots and captures remain unchanged
//! for export; optimized executable SSA is separate from that source authority.
//!
//! Work and conservative storage forecasts bound scanning, candidate adapters,
//! provenance copies and memory-remap traversal. These are logical work/allocation
//! budgets, not process-RSS or wall-clock guarantees. Numerical fusion and optional
//! external candidate stages are separate from this exact pass.
use crate::{
    AffinePhaseOperation as A, BigRational, Cnot, LanguageError, LinearOptions, ParityOptions,
    VerifiedStructuredProgram,
};
use num_traits::ToPrimitive;
use quest_language::{
    GateKind as G, SourceSpan,
    classical::ScalarValue,
    semantic::CompileLimits,
    ssa::{self, GateModifier as M, InstructionKind as K, Place, SlotId, ValueId},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy)]
pub struct StructuredQuantumOptions {
    pub compile: CompileLimits,
    pub linear: LinearOptions,
    pub parity: bool,
    pub storage_bytes: usize,
    pub work: usize,
}
impl Default for StructuredQuantumOptions {
    fn default() -> Self {
        Self {
            compile: CompileLimits::default(),
            linear: LinearOptions::default(),
            parity: true,
            storage_bytes: 64 * 1024 * 1024,
            work: 1_000_000,
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum StructuredQuantumError {
    #[error(transparent)]
    Language(#[from] LanguageError),
    #[error(transparent)]
    Circuit(#[from] crate::Error),
}
type Result<T> = std::result::Result<T, StructuredQuantumError>;
#[derive(Debug, Clone)]
pub struct StructuredOccurrence {
    pub block: ssa::BlockId,
    pub instruction: usize,
    pub memory: ValueId,
    pub span: Option<SourceSpan>,
}
#[derive(Debug, Clone)]
pub struct StructuredQuantumRewrite {
    pub inputs: Vec<StructuredOccurrence>,
    pub outputs: Vec<ValueId>,
}
#[derive(Debug, Clone)]
pub struct StructuredQuantumReport {
    pub input_snapshot: ssa::SnapshotId,
    pub output_snapshot: ssa::SnapshotId,
    pub before_gates: usize,
    pub after_gates: usize,
    pub windows: usize,
    pub inverse_pairs: usize,
    pub resynthesized_windows: usize,
    pub work: usize,
    pub rewrites: Vec<StructuredQuantumRewrite>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Wire {
    pub slot: SlotId,
    pub index: usize,
}
#[derive(Clone)]
pub struct ResolvedGate {
    pub gate: G,
    pub arguments: Vec<ValueId>,
    pub parameters: Vec<u64>,
    pub places: Vec<Place>,
    pub wires: Vec<Wire>,
    pub controls: Vec<bool>,
    pub inverse: bool,
    pub span: Option<SourceSpan>,
}
pub struct Context<'a> {
    pub slots: &'a [ssa::Slot],
    pub constants: &'a BTreeMap<ValueId, ScalarValue>,
}
pub struct Budget {
    pub used: usize,
    pub limit: usize,
}
impl Budget {
    pub fn charge(&mut self, amount: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(amount)
            .ok_or(LanguageError::Budget("quantum optimization work"))?;
        if self.used > self.limit {
            return Err(LanguageError::Budget("quantum optimization work").into());
        }
        Ok(())
    }
}
pub fn wire(place: &Place, context: &Context<'_>) -> Option<Wire> {
    let slot = context.slots.get(place.slot.index())?;
    if slot.reference
        && !(slot.interface == ssa::Interface::Parameter
            && slot.mutable
            && slot.ty == ssa::Type::Qubit(1))
    {
        return None;
    }
    let ssa::Type::Qubit(count) = slot.ty else {
        return None;
    };
    let index = match place.indices.as_slice() {
        [] if count == 1 => 0,
        [index] => context.constants.get(index)?.to_index(count).ok()?,
        _ => return None,
    };
    Some(Wire {
        slot: place.slot,
        index,
    })
}
pub fn gate(item: &ssa::Instruction, context: &Context<'_>) -> Option<ResolvedGate> {
    let K::Gate {
        gate,
        arguments,
        operands,
        modifiers,
        ..
    } = &item.kind
    else {
        return None;
    };
    let parameters = arguments
        .iter()
        .map(|id| {
            context
                .constants
                .get(id)?
                .to_f64()
                .ok()
                .filter(|value| value.is_finite())
                .map(f64::to_bits)
        })
        .collect::<Option<Vec<_>>>()?;
    let wires = operands
        .iter()
        .map(|place| wire(place, context))
        .collect::<Option<Vec<_>>>()?;
    if wires.iter().collect::<BTreeSet<_>>().len() != wires.len() {
        return None;
    }
    let mut inverse = false;
    let mut controls = vec![];
    for modifier in modifiers {
        match modifier {
            M::Inverse | M::Adjoint => inverse = !inverse,
            M::Control { positive, count } => {
                controls.extend(std::iter::repeat_n(*positive, *count));
            }
            M::Power(_) => return None,
        }
    }
    controls.extend(std::iter::repeat_n(
        true,
        gate.definition().intrinsic_controls,
    ));
    let gate = match gate {
        G::Cx | G::Ccx => G::X,
        G::Cy => G::Y,
        G::Cz => G::Z,
        G::Sdg => {
            inverse = !inverse;
            G::S
        }
        G::Tdg => {
            inverse = !inverse;
            G::T
        }
        G::Sxdg => {
            inverse = !inverse;
            G::Sx
        }
        value => *value,
    };
    if self_inverse(gate) {
        inverse = false;
    }
    Some(ResolvedGate {
        gate,
        arguments: arguments.clone(),
        parameters,
        places: operands.clone(),
        wires,
        controls,
        inverse,
        span: item.span,
    })
}
const fn self_inverse(gate: G) -> bool {
    matches!(gate, G::Id | G::X | G::Y | G::Z | G::H | G::Swap)
}
fn inverses(left: &ResolvedGate, right: &ResolvedGate) -> bool {
    left.gate == right.gate
        && left.parameters == right.parameters
        && left.wires == right.wires
        && left.controls == right.controls
        && (self_inverse(left.gate) || left.inverse != right.inverse)
}
const fn diagonal(gate: &ResolvedGate) -> bool {
    matches!(
        gate.gate,
        G::Id | G::Z | G::S | G::T | G::Rz | G::Phase | G::GlobalPhase
    )
}
fn commute(left: &ResolvedGate, right: &ResolvedGate) -> bool {
    (diagonal(left) && diagonal(right)) || !left.wires.iter().any(|wire| right.wires.contains(wire))
}
fn cancel(
    input: Vec<ResolvedGate>,
    budget: &mut Budget,
    report: &mut StructuredQuantumReport,
) -> Result<Vec<ResolvedGate>> {
    let mut output: Vec<ResolvedGate> = vec![];
    for gate in input {
        if gate.gate == G::Id {
            continue;
        }
        let mut previous = None;
        for (index, candidate) in output.iter().enumerate().rev().take(128) {
            budget.charge(1)?;
            if inverses(candidate, &gate) {
                previous = Some(index);
                break;
            }
            if !commute(candidate, &gate) {
                break;
            }
        }
        if let Some(index) = previous {
            output.remove(index);
            report.inverse_pairs = report
                .inverse_pairs
                .checked_add(1)
                .ok_or(LanguageError::Budget("quantum report"))?;
        } else {
            output.push(gate);
        }
    }
    Ok(output)
}
fn affine(gate: &ResolvedGate, wires: &BTreeMap<Wire, usize>) -> Option<A> {
    if !gate.arguments.is_empty() {
        return None;
    }
    let target = *wires.get(gate.wires.last()?)?;
    match (gate.gate, gate.controls.as_slice()) {
        (G::X, [true]) => Some(A::Cnot(Cnot {
            control: *wires.get(gate.wires.first()?)?,
            target,
        })),
        (G::X, []) => Some(A::X { target }),
        (G::Z | G::S | G::T, []) => Some(A::Phase {
            target,
            coefficient: BigRational::new(
                if gate.inverse { (-1).into() } else { 1.into() },
                match gate.gate {
                    G::Z => 1,
                    G::S => 2,
                    _ => 4,
                }
                .into(),
            ),
        }),
        _ => None,
    }
}
pub fn exact_gate(
    kind: G,
    targets: &[usize],
    wires: &[(Wire, Place)],
    source: &ResolvedGate,
) -> Result<ResolvedGate> {
    let mut result = source.clone();
    result.gate = kind;
    result.arguments.clear();
    result.parameters.clear();
    result.controls.clear();
    result.inverse = false;
    result.wires.clear();
    result.places.clear();
    for target in targets {
        let (wire, place) = wires.get(*target).ok_or(crate::Error::InvalidId)?;
        result.wires.push(*wire);
        result.places.push(place.clone());
    }
    Ok(result)
}
pub fn quarter_turns(value: &BigRational) -> Option<usize> {
    let quarters = std::ops::Mul::mul(value, BigRational::from_integer(4.into()));
    if !quarters.is_integer() {
        return None;
    }
    quarters.to_integer().to_usize().filter(|value| *value < 8)
}
fn emit_phase(
    output: &mut Vec<ResolvedGate>,
    quarter: usize,
    target: usize,
    wires: &[(Wire, Place)],
    source: &ResolvedGate,
) -> Result<()> {
    let kinds: &[G] = match quarter {
        0 => &[],
        1 => &[G::T],
        2 => &[G::S],
        3 => &[G::S, G::T],
        4 => &[G::Z],
        5 => &[G::Z, G::T],
        6 => &[G::Sdg],
        7 => &[G::Tdg],
        _ => return Err(LanguageError::Unsupported("non-Clifford phase output").into()),
    };
    for &kind in kinds {
        output.push(exact_gate(kind, &[target], wires, source)?);
    }
    Ok(())
}
pub fn lower_affine(
    input: &[A],
    wires: &[(Wire, Place)],
    source: &ResolvedGate,
    cap: usize,
) -> Result<Option<Vec<ResolvedGate>>> {
    let mut output = vec![];
    for operation in input {
        match operation {
            A::X { target } => output.push(exact_gate(G::X, &[*target], wires, source)?),
            A::Cnot(cnot) => {
                let mut gate = exact_gate(G::X, &[cnot.control, cnot.target], wires, source)?;
                gate.controls.push(true);
                output.push(gate);
            }
            A::Phase {
                target,
                coefficient,
            } => {
                let Some(quarter) = quarter_turns(coefficient) else {
                    return Ok(None);
                };
                emit_phase(&mut output, quarter, *target, wires, source)?;
            }
            A::GlobalPhase { coefficient } => {
                let Some(quarter) = quarter_turns(coefficient) else {
                    return Ok(None);
                };
                if wires.is_empty() {
                    return Ok(None);
                }
                // (S H)^3 = exp(i*pi/4) I, chronological H,S,H,S,H,S.
                for _ in 0..quarter {
                    for kind in [G::H, G::S, G::H, G::S, G::H, G::S] {
                        output.push(exact_gate(kind, &[0], wires, source)?);
                    }
                }
            }
            A::Rz { .. } => return Ok(None),
        }
        if output.len() >= cap {
            return Ok(None);
        }
    }
    Ok(Some(output))
}
fn resynthesize(
    input: &[ResolvedGate],
    options: StructuredQuantumOptions,
    budget: &mut Budget,
    report: &mut StructuredQuantumReport,
) -> Result<Vec<ResolvedGate>> {
    let mut output = vec![];
    let mut offset = 0;
    while let Some(first) = input.get(offset) {
        let mut map = BTreeMap::new();
        let mut places = vec![];
        let mut operations = vec![];
        for gate in input.iter().skip(offset) {
            budget.charge(1)?;
            if !gate.arguments.is_empty()
                || !matches!(
                    (gate.gate, gate.controls.as_slice()),
                    (G::X, [] | [true]) | (G::Z | G::S | G::T, [])
                )
            {
                break;
            }
            for (wire, place) in gate.wires.iter().zip(&gate.places) {
                if !map.contains_key(wire) {
                    map.insert(*wire, places.len());
                    places.push((*wire, place.clone()));
                }
            }
            let Some(operation) = affine(gate, &map) else {
                break;
            };
            if !options.parity && !matches!(operation, A::Cnot(_)) {
                break;
            }
            operations.push(operation);
        }
        if operations.is_empty() {
            output.push(first.clone());
            offset = offset
                .checked_add(1)
                .ok_or(LanguageError::Budget("quantum offset"))?;
            continue;
        }
        let width = places.len();
        let remaining = budget
            .limit
            .checked_sub(budget.used)
            .ok_or(LanguageError::Budget("quantum work"))?;
        let linear = LinearOptions {
            max_work: remaining.min(options.linear.max_work),
            ..options.linear
        };
        // Bound adapter work conservatively before invocation, including both
        // matrix syntheses/replays and coefficient arithmetic for quarter turns.
        let forecast = width
            .checked_mul(width)
            .and_then(|n| n.checked_mul(16))
            .and_then(|n| n.checked_add(operations.len().checked_mul(128)?))
            .ok_or(LanguageError::Budget("quantum adapter work"))?;
        budget.charge(forecast)?;
        let result = if options.parity {
            crate::fold_parity(
                width,
                &operations,
                ParityOptions {
                    linear,
                    max_coefficient_bits: 64,
                },
            )?
            .operations()
            .to_vec()
        } else {
            let cnots: Vec<_> = operations
                .iter()
                .filter_map(|op| {
                    if let A::Cnot(cnot) = op {
                        Some(*cnot)
                    } else {
                        None
                    }
                })
                .collect();
            crate::synthesize_cnot(width, &cnots, linear)?
                .gates()
                .iter()
                .copied()
                .map(A::Cnot)
                .collect()
        };
        let end = offset
            .checked_add(operations.len())
            .ok_or(LanguageError::Budget("quantum offset"))?;
        if let Some(candidate) = lower_affine(&result, &places, first, operations.len())? {
            if candidate.len() < operations.len() {
                output.extend(candidate);
                report.resynthesized_windows = report
                    .resynthesized_windows
                    .checked_add(1)
                    .ok_or(LanguageError::Budget("quantum report"))?;
            } else {
                output.extend_from_slice(input.get(offset..end).ok_or(crate::Error::InvalidId)?);
            }
        } else {
            output.extend_from_slice(input.get(offset..end).ok_or(crate::Error::InvalidId)?);
        }
        offset = end;
    }
    Ok(output)
}
pub fn memory_result(item: &ssa::Instruction) -> Result<ValueId> {
    let [result] = item.results.as_slice() else {
        return Err(LanguageError::Unsupported("gate memory signature").into());
    };
    if result.ty != ssa::Type::Memory {
        return Err(LanguageError::Unsupported("gate memory type").into());
    }
    Ok(result.id)
}
pub fn emit(gate: ResolvedGate, template: &ssa::Instruction, memory: ValueId) -> ssa::Instruction {
    let mut modifiers = gate
        .controls
        .into_iter()
        .map(|positive| M::Control { positive, count: 1 })
        .collect::<Vec<_>>();
    if gate.inverse {
        modifiers.push(M::Inverse);
    }
    let kind = K::Gate {
        gate: gate.gate,
        arguments: gate.arguments,
        operands: gate.places,
        modifiers,
        memory,
    };
    ssa::Instruction {
        results: template.results.clone(),
        effect: kind.effect(),
        accesses: kind.accesses(),
        kind,
        span: gate.span,
    }
}
fn transform_block(
    block: &mut ssa::Block,
    context: &Context<'_>,
    options: StructuredQuantumOptions,
    budget: &mut Budget,
    report: &mut StructuredQuantumReport,
    replacements: &mut BTreeMap<ValueId, ValueId>,
) -> Result<()> {
    let original = std::mem::take(&mut block.instructions);
    let mut output = vec![];
    let mut offset = 0;
    while let Some(item) = original.get(offset) {
        budget.charge(1)?;
        if gate(item, context).is_none() {
            output.push(item.clone());
            offset = offset
                .checked_add(1)
                .ok_or(LanguageError::Budget("quantum offset"))?;
            continue;
        }
        let mut gates = vec![];
        let mut positions = vec![];
        let mut end = offset;
        for (position, item) in original.iter().enumerate().skip(offset) {
            if gates.len() == options.linear.max_window_operations {
                break;
            }
            budget.charge(1)?;
            if let Some(value) = gate(item, context) {
                gates.push(value);
                positions.push(position);
            } else if !matches!(item.kind, K::Constant(_)) {
                break;
            }
            end = position
                .checked_add(1)
                .ok_or(LanguageError::Budget("quantum offset"))?;
        }
        report.windows = report
            .windows
            .checked_add(1)
            .ok_or(LanguageError::Budget("quantum report"))?;
        let candidate = resynthesize(&cancel(gates, budget, report)?, options, budget, report)?;
        let window = original.get(offset..end).ok_or(crate::Error::InvalidId)?;
        if candidate.len() < positions.len() {
            // Constants are nontrapping and independent; moving them before the
            // rewritten gates makes every reused index/argument dominate its use.
            output.extend(
                window
                    .iter()
                    .filter(|item| matches!(item.kind, K::Constant(_)))
                    .cloned(),
            );
            let mut memory = item.kind.memory().ok_or(crate::Error::InvalidId)?;
            let mut outputs = vec![];
            for (candidate, &position) in candidate.into_iter().zip(&positions) {
                let template = original.get(position).ok_or(crate::Error::InvalidId)?;
                let instruction = emit(candidate, template, memory);
                memory = memory_result(&instruction)?;
                outputs.push(memory);
                output.push(instruction);
            }
            let mut inputs = vec![];
            for (ordinal, &position) in positions.iter().enumerate() {
                let source = original.get(position).ok_or(crate::Error::InvalidId)?;
                let old = memory_result(source)?;
                if ordinal >= outputs.len() {
                    replacements.insert(old, memory);
                }
                inputs.push(StructuredOccurrence {
                    block: block.id,
                    instruction: position,
                    memory: old,
                    span: source.span,
                });
            }
            report
                .rewrites
                .push(StructuredQuantumRewrite { inputs, outputs });
        } else {
            output.extend_from_slice(window);
        }
        offset = end;
    }
    block.instructions = output;
    Ok(())
}
fn reaches_block(
    blocks: &[ssa::Block],
    from: ssa::BlockId,
    target: ssa::BlockId,
    budget: &mut Budget,
) -> Result<bool> {
    let mut pending = vec![from];
    let mut visited = BTreeSet::new();
    while let Some(id) = pending.pop() {
        budget.charge(1)?;
        if id == target {
            return Ok(true);
        }
        if !visited.insert(id) {
            continue;
        }
        let block = blocks.get(id.index()).ok_or(crate::Error::InvalidId)?;
        if let Some(terminator) = &block.terminator {
            let edges = terminator.edges();
            budget.charge(edges.len())?;
            pending.extend(edges.into_iter().map(|edge| edge.target));
        }
    }
    Ok(false)
}

#[allow(clippy::too_many_lines)] // CFG proof, rewrite publication, and provenance stay in one transaction.
fn transform_straight_chains(
    blocks: &mut [ssa::Block],
    context: &Context<'_>,
    budget: &mut Budget,
    report: &mut StructuredQuantumReport,
    replacements: &mut BTreeMap<ValueId, ValueId>,
) -> Result<()> {
    for start_index in 0..blocks.len() {
        budget.charge(1)?;
        let Some(start) = blocks.get(start_index) else {
            continue;
        };
        let Some(first_position) = start
            .instructions
            .iter()
            .rposition(|item| matches!(item.kind, K::Gate { .. }))
        else {
            continue;
        };
        let after_first = first_position
            .checked_add(1)
            .ok_or(LanguageError::Budget("quantum position"))?;
        if !start
            .instructions
            .iter()
            .skip(after_first)
            .all(|item| matches!(item.kind, K::Constant(_)))
        {
            continue;
        }
        let first_item = start
            .instructions
            .get(first_position)
            .ok_or(crate::Error::InvalidId)?;
        let Some(first_gate) = gate(first_item, context) else {
            continue;
        };
        let Some(ssa::Terminator::Jump(edge)) = &start.terminator else {
            continue;
        };
        let mut next = edge.target;
        let mut previous = start.id;
        let mut seen = BTreeSet::new();
        let endpoint = loop {
            budget.charge(1)?;
            if !seen.insert(next) || reaches_block(blocks, next, start.id, budget)? {
                break None;
            }
            let block = blocks.get(next.index()).ok_or(crate::Error::InvalidId)?;
            if block.region != start.region || block.predecessors.as_slice() != [previous] {
                break None;
            }
            if let Some(item) = block.instructions.first() {
                break gate(item, context)
                    .filter(|candidate| inverses(&first_gate, candidate))
                    .map(|_| next.index());
            }
            if block.arguments.len() != 1 {
                break None;
            }
            let Some(ssa::Terminator::Jump(edge)) = &block.terminator else {
                break None;
            };
            previous = block.id;
            next = edge.target;
        };
        let Some(end_index) = endpoint else { continue };
        if end_index == start_index {
            continue;
        }
        let first = blocks
            .get(start_index)
            .ok_or(crate::Error::InvalidId)?
            .instructions
            .get(first_position)
            .ok_or(crate::Error::InvalidId)?
            .clone();
        let last = blocks
            .get(end_index)
            .ok_or(crate::Error::InvalidId)?
            .instructions
            .first()
            .ok_or(crate::Error::InvalidId)?
            .clone();
        let first_old = memory_result(&first)?;
        let last_old = memory_result(&last)?;
        let first_prior = first.kind.memory().ok_or(crate::Error::InvalidId)?;
        let last_prior = last.kind.memory().ok_or(crate::Error::InvalidId)?;
        budget.charge(4)?;
        replacements.insert(first_old, first_prior);
        replacements.insert(last_old, last_prior);
        let start_id = blocks.get(start_index).ok_or(crate::Error::InvalidId)?.id;
        let end_id = blocks.get(end_index).ok_or(crate::Error::InvalidId)?.id;
        blocks
            .get_mut(start_index)
            .ok_or(crate::Error::InvalidId)?
            .instructions
            .remove(first_position);
        blocks
            .get_mut(end_index)
            .ok_or(crate::Error::InvalidId)?
            .instructions
            .remove(0);
        report.after_gates = report
            .after_gates
            .checked_sub(2)
            .ok_or(LanguageError::Budget("quantum report"))?;
        report.inverse_pairs = report
            .inverse_pairs
            .checked_add(1)
            .ok_or(LanguageError::Budget("quantum report"))?;
        report.rewrites.push(StructuredQuantumRewrite {
            inputs: vec![
                StructuredOccurrence {
                    block: start_id,
                    instruction: first_position,
                    memory: first_old,
                    span: first.span,
                },
                StructuredOccurrence {
                    block: end_id,
                    instruction: 0,
                    memory: last_old,
                    span: last.span,
                },
            ],
            outputs: vec![],
        });
    }
    Ok(())
}
pub fn resolve(
    mut id: ValueId,
    replacements: &BTreeMap<ValueId, ValueId>,
    budget: &mut Budget,
) -> Result<ValueId> {
    for _ in 0..=replacements.len() {
        budget.charge(1)?;
        if let Some(&next) = replacements.get(&id) {
            id = next;
        } else {
            return Ok(id);
        }
    }
    Err(LanguageError::Unsupported("cyclic memory replacement").into())
}
pub fn remap(
    program: &mut ssa::Program,
    replacements: &BTreeMap<ValueId, ValueId>,
    budget: &mut Budget,
) -> Result<()> {
    for block in &mut program.blocks {
        for item in &mut block.instructions {
            budget.charge(1)?;
            match &mut item.kind {
                K::Assert { memory, .. }
                | K::Input { memory, .. }
                | K::AllocateArray { memory, .. }
                | K::Allocate { memory, .. }
                | K::Load { memory, .. }
                | K::Store { memory, .. }
                | K::Call { memory, .. }
                | K::Gate { memory, .. }
                | K::Measure { memory, .. }
                | K::Reset { memory, .. }
                | K::Barrier { memory, .. } => *memory = resolve(*memory, replacements, budget)?,
                _ => {}
            }
            item.effect = item.kind.effect();
            item.accesses = item.kind.accesses();
        }
        let mut edge = |edge: &mut ssa::Edge| -> Result<()> {
            for id in &mut edge.arguments {
                *id = resolve(*id, replacements, budget)?;
            }
            Ok(())
        };
        match &mut block.terminator {
            Some(ssa::Terminator::Jump(value)) => edge(value)?,
            Some(ssa::Terminator::Branch {
                then_edge,
                else_edge,
                ..
            }) => {
                edge(then_edge)?;
                edge(else_edge)?;
            }
            Some(ssa::Terminator::Return { memory, .. } | ssa::Terminator::End { memory }) => {
                *memory = resolve(*memory, replacements, budget)?;
            }
            None => {}
        }
    }
    Ok(())
}
#[allow(clippy::too_many_lines)] // Admission, rewrite, remap, and independent verification are sequential.
fn optimize(
    program: ssa::VerifiedProgram,
    options: StructuredQuantumOptions,
) -> Result<(ssa::VerifiedProgram, StructuredQuantumReport)> {
    if options.linear.max_window_operations == 0 {
        return Err(LanguageError::Budget("quantum window").into());
    }
    let nodes = program.blocks().iter().try_fold(0usize, |count, block| {
        count
            .checked_add(block.instructions.len())
            .ok_or(LanguageError::Budget("quantum nodes"))
    })?;
    let bytes = program
        .retained_bytes()
        .map_err(LanguageError::from)?
        .checked_mul(8)
        .and_then(|bytes| bytes.checked_add(nodes.checked_mul(2048)?))
        .ok_or(LanguageError::Budget("quantum storage"))?;
    let remaining = options
        .storage_bytes
        .checked_sub(bytes)
        .ok_or(LanguageError::Budget("quantum storage"))?;
    let options = StructuredQuantumOptions {
        linear: LinearOptions {
            max_bytes: options.linear.max_bytes.min(remaining),
            ..options.linear
        },
        ..options
    };
    let mut budget = Budget {
        used: 0,
        limit: options.work,
    };
    budget.charge(nodes)?;
    let mut constants = BTreeMap::new();
    for item in program
        .blocks()
        .iter()
        .flat_map(|block| &block.instructions)
    {
        if let (K::Constant(value), [result]) = (&item.kind, item.results.as_slice()) {
            constants.insert(result.id, *value);
        }
    }
    let snapshot = program.snapshot();
    let mut program = program.into_unverified();
    let mut report = StructuredQuantumReport {
        input_snapshot: snapshot,
        output_snapshot: snapshot,
        before_gates: 0,
        after_gates: 0,
        windows: 0,
        inverse_pairs: 0,
        resynthesized_windows: 0,
        work: 0,
        rewrites: Vec::new(),
    };
    let mut replacements = BTreeMap::new();
    let context = Context {
        slots: &program.slots,
        constants: &constants,
    };
    for block in &mut program.blocks {
        report.before_gates = report
            .before_gates
            .checked_add(
                block
                    .instructions
                    .iter()
                    .filter(|item| matches!(item.kind, K::Gate { .. }))
                    .count(),
            )
            .ok_or(LanguageError::Budget("quantum report"))?;
        transform_block(
            block,
            &context,
            options,
            &mut budget,
            &mut report,
            &mut replacements,
        )?;
        report.after_gates = report
            .after_gates
            .checked_add(
                block
                    .instructions
                    .iter()
                    .filter(|item| matches!(item.kind, K::Gate { .. }))
                    .count(),
            )
            .ok_or(LanguageError::Budget("quantum report"))?;
    }
    transform_straight_chains(
        &mut program.blocks,
        &context,
        &mut budget,
        &mut report,
        &mut replacements,
    )?;
    remap(&mut program, &replacements, &mut budget)?;
    report.work = budget.used;
    let compile = CompileLimits {
        storage_bytes: options.compile.storage_bytes.min(options.storage_bytes),
        ..options.compile
    };
    let program = program.verify(compile).map_err(LanguageError::from)?;
    report.output_snapshot = program.snapshot();
    Ok((program, report))
}
impl VerifiedStructuredProgram {
    /// Optimize independent straight-line quantum windows and reverify all SSA.
    /// Original syntax, sources and captures remain authoritative for export.
    /// # Errors
    /// Rejects exhausted work/storage/compile limits or failed independent verification.
    pub fn optimize_quantum(
        self,
        options: StructuredQuantumOptions,
    ) -> Result<(Self, StructuredQuantumReport)> {
        self.transform_ssa(|program| optimize(program, options))
    }
}
