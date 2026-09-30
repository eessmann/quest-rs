//! Reusable dispatch recipes derived using the interpreter's own gate resolver.
use super::{
    BTreeMap, Binding, Engine, Fault, Frame, GateBuffer, GateRequest, InstructionKind,
    InterpreterLimits, OwnedGate, QuantumBackend, QuantumKind, Rc, RefCell, RunInputs,
    RuntimeCause, RuntimeError, RuntimeValue, ScalarType, ScalarValue, Sink, ValueId,
    VerifiedProgram, ssa,
};

/// Stable identity of one dispatch within a verified publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DispatchId {
    pub(super) occurrence: ValueId,
    pub(super) lane: usize,
}

/// Immutable dispatch publication tied to one verified SSA snapshot and capture set.
#[derive(Debug, Clone)]
pub struct PreparedDispatch {
    pub(super) snapshot: ssa::SnapshotId,
    pub(super) captures: Vec<ScalarValue>,
    pub(super) requests: BTreeMap<ValueId, Vec<OwnedGate>>,
    bytes: usize,
}
impl PreparedDispatch {
    pub(super) fn captures_match(&self, other: &[ScalarValue]) -> bool {
        self.captures.len() == other.len()
            && self.captures.iter().zip(other).all(|(a, b)| {
                a.ty() == b.ty()
                    && match a.ty() {
                        ScalarType::Float(_) => {
                            a.to_f64().map(f64::to_bits) == b.to_f64().map(f64::to_bits)
                        }
                        _ => a == b,
                    }
            })
    }

    /// Static builtin dispatches in deterministic occurrence/lane order.
    pub fn gates(&self) -> impl Iterator<Item = (DispatchId, GateRequest<'_>)> {
        self.requests.iter().flat_map(|(occurrence, requests)| {
            requests
                .iter()
                .enumerate()
                .filter_map(move |(lane, request)| {
                    let QuantumKind::Builtin(gate) = request.kind else {
                        return None;
                    };
                    Some((
                        DispatchId {
                            occurrence: *occurrence,
                            lane,
                        },
                        GateRequest {
                            gate,
                            parameters: &request.parameters,
                            targets: &request.targets,
                            controls: &request.controls,
                            inverse: request.inverse,
                        },
                    ))
                })
        })
    }
    #[must_use]
    pub fn occurrences(&self) -> usize {
        self.requests.len()
    }
    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.bytes
    }
}
#[derive(Default)]
struct NoBackend;
impl QuantumBackend for NoBackend {
    type Error = std::convert::Infallible;
    fn apply_gate(&mut self, _: GateRequest<'_>) -> Result<(), Self::Error> {
        Ok(())
    }
    fn measure(&mut self, _: usize) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn reset(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn barrier(&mut self, _: &[usize]) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Specialize only fully static gates. Potential source traps remain in the VM.
/// The original instructions are never removed, reordered, or evaluated as effects.
/// # Errors
/// Rejects invalid setup and retained recipe storage beyond the supplied bound.
pub fn prepare_dispatch(
    program: &VerifiedProgram,
    captures: &[ScalarValue],
    limits: InterpreterLimits,
) -> Result<PreparedDispatch, RuntimeError<std::convert::Infallible>> {
    let mut backend = NoBackend;
    let inputs = RunInputs::default();
    let mut engine =
        Engine::new(program, &mut backend, &inputs, captures, limits).map_err(|f| (*f).finish())?;
    let mut frame = Frame {
        region: program.program().entry,
        slots: vec![None; program.slots().len()],
        values: vec![None; engine.value_capacity],
    };
    for (slot, qubits) in &engine.qubit_offsets {
        let target = frame.slots.get_mut(slot.index()).ok_or_else(|| {
            (*Fault::bare(RuntimeCause::InvalidVerifiedProgram(
                "prepared slot identity",
            )))
            .finish()
        })?;
        *target = Some(Binding::Owned(Rc::new(RefCell::new(Some(
            RuntimeValue::Qubits(qubits.clone()),
        )))));
    }
    let mut requests = BTreeMap::new();
    let mut bytes = std::mem::size_of_val(captures);
    for block in program.blocks() {
        if block.region != program.program().entry {
            continue;
        }
        for instruction in &block.instructions {
            match &instruction.kind {
                InstructionKind::Constant(_)
                | InstructionKind::Unary { .. }
                | InstructionKind::Binary { .. }
                | InstructionKind::Cast { .. }
                | InstructionKind::GateParameter { .. }
                | InstructionKind::Capture { .. }
                | InstructionKind::Builtin { .. } => {
                    // Failure here is retained source behavior, never a preparation failure.
                    let _ = engine.execute_instruction(&mut frame, instruction, &mut Sink::Backend);
                }
                InstructionKind::Gate {
                    gate,
                    arguments,
                    operands,
                    modifiers,
                    ..
                } => {
                    let mut buffer = GateBuffer::default();
                    if engine
                        .execute_gate(
                            &frame,
                            *gate,
                            arguments,
                            operands,
                            modifiers,
                            &mut Sink::Buffer(&mut buffer),
                        )
                        .is_ok()
                    {
                        let Some(result) = instruction.results.first() else {
                            continue;
                        };
                        let next = bytes
                            .checked_add(buffer.bytes)
                            .and_then(|n| n.checked_add(std::mem::size_of::<ValueId>()));
                        if let Some(next) = next.filter(|n| *n <= limits.storage_bytes) {
                            bytes = next;
                            requests.insert(result.id, buffer.requests);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(PreparedDispatch {
        snapshot: program.snapshot(),
        captures: captures.to_vec(),
        requests,
        bytes,
    })
}
