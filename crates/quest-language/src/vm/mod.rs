//! Bounded execution of independently verified SSA.

mod prepared;
use crate::{
    GateKind, SourceSpan,
    classical::{FloatWidth, ScalarType, ScalarValue, ValueError},
    ssa::{
        self, BlockId, CallArgument, GateModifier, Instruction, InstructionKind, Interface, Place,
        RegionId, SlotId, Terminator, Type, ValueId, VerifiedProgram,
    },
};
pub use prepared::{DispatchId, PreparedDispatch, prepare_dispatch};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    error::Error as StdError,
    fmt,
    rc::Rc,
};

/// One signed quantum control in semantic operand order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuantumControl {
    pub qubit: usize,
    pub positive: bool,
}

/// A fully checked built-in gate request.
#[derive(Debug, Clone, Copy)]
pub struct GateRequest<'a> {
    pub gate: GateKind,
    pub parameters: &'a [f64],
    pub targets: &'a [usize],
    pub controls: &'a [QuantumControl],
    pub inverse: bool,
}

/// A retained oracle request; the backend resolves only the local capture ID.
#[derive(Debug, Clone, Copy)]
pub struct OracleRequest<'a> {
    pub capture: usize,
    pub targets: &'a [usize],
    pub controls: &'a [QuantumControl],
    pub adjoint: bool,
}

/// Native-independent quantum operations required by the interpreter.
pub trait QuantumBackend {
    type Error: StdError + Send + Sync + 'static;

    /// Dispatch a prevalidated static gate; native backends can reuse lowered resources by id.
    /// # Errors
    /// Returns the backend error without rolling back previously completed effects.
    fn apply_prepared_gate(
        &mut self,
        _id: DispatchId,
        request: GateRequest<'_>,
    ) -> Result<(), Self::Error> {
        self.apply_gate(request)
    }
    /// Apply an irreversible immutable channel payload on ordered target wires.
    fn apply_payload(
        &mut self,
        _capture: usize,
        _targets: &[usize],
    ) -> Option<Result<(), Self::Error>> {
        None
    }
    /// # Errors
    /// Returns a backend-specific error without rolling back prior requests.
    /// Return `None` when oracle execution is unsupported. The interpreter then
    /// reports an explicit capability failure before silently skipping any call.
    fn apply_oracle(&mut self, _request: OracleRequest<'_>) -> Option<Result<(), Self::Error>> {
        None
    }
    /// # Errors
    /// Returns a backend error without rolling back prior requests.
    fn apply_gate(&mut self, request: GateRequest<'_>) -> Result<(), Self::Error>;
    /// # Errors
    /// Returns a backend-specific error without rolling back prior requests.
    fn measure(&mut self, qubit: usize) -> Result<bool, Self::Error>;
    /// # Errors
    /// Returns a backend-specific error without rolling back prior requests.
    fn reset(&mut self, qubit: usize) -> Result<(), Self::Error>;
    /// # Errors
    /// Returns a backend-specific error without rolling back prior requests.
    fn barrier(&mut self, qubits: &[usize]) -> Result<(), Self::Error>;
}

/// Owned classical input or output, including nested fixed arrays.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ClassicalValue {
    Scalar(ScalarValue),
    Array(Vec<Self>),
}
impl ClassicalValue {
    /// Check the complete scalar type or nested fixed array shape without allocation.
    #[must_use]
    pub fn matches_type(&self, ty: &ssa::Type) -> bool {
        classical_matches(self, ty)
    }

    #[must_use]
    pub const fn as_scalar(&self) -> Option<&ScalarValue> {
        if let Self::Scalar(value) = self {
            Some(value)
        } else {
            None
        }
    }
    #[must_use]
    pub fn as_array(&self) -> Option<&[Self]> {
        if let Self::Array(values) = self {
            Some(values)
        } else {
            None
        }
    }
}

/// Caller-owned classical inputs, matched by exact interface name.
#[derive(Debug, Clone, Default)]
pub struct RunInputs {
    values: BTreeMap<String, ClassicalValue>,
}
impl RunInputs {
    /// Borrow all named inputs in deterministic name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ClassicalValue)> {
        self.values
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }
    /// Add one input without replacing an existing value.
    ///
    /// # Errors
    /// Returns an error for duplicate input names.
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        value: ClassicalValue,
    ) -> Result<(), InputError> {
        let name = name.into();
        match self.values.entry(name) {
            Entry::Vacant(entry) => {
                entry.insert(value);
                Ok(())
            }
            Entry::Occupied(entry) => Err(InputError::Duplicate(entry.key().clone())),
        }
    }
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ClassicalValue> {
        self.values.get(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InputError {
    #[error("duplicate runtime input {0}")]
    Duplicate(String),
}

/// Per-run hard limits. Every block, instruction, terminator, call and expanded
/// gate application consumes the step budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterpreterLimits {
    pub steps: usize,
    pub call_frames: usize,
    pub storage_bytes: usize,
}
impl Default for InterpreterLimits {
    fn default() -> Self {
        Self {
            steps: 10_000_000,
            call_frames: 64,
            storage_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeContext {
    pub region: RegionId,
    pub block: BlockId,
}

#[derive(Debug)]
pub enum RuntimeCause<E> {
    Backend(E),
    Value(ValueError),
    MissingInput(String),
    UnexpectedInput(String),
    InputType(String),
    MissingCapture(usize),
    CaptureType(usize),
    Uninitialized,
    Alias,
    Assertion(String),
    StepLimit,
    FrameLimit,
    StorageLimit,
    Allocation,
    InvalidVerifiedProgram(&'static str),
    Unsupported(&'static str),
}
impl<E: fmt::Display> fmt::Display for RuntimeCause<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(capability) => {
                write!(formatter, "unsupported capability: {capability}")
            }
            Self::Backend(error) => write!(formatter, "quantum backend failed: {error}"),
            Self::Value(error) => error.fmt(formatter),
            Self::MissingInput(name) => write!(formatter, "missing runtime input {name}"),
            Self::UnexpectedInput(name) => write!(formatter, "unexpected runtime input {name}"),
            Self::InputType(name) => write!(formatter, "runtime input {name} has the wrong type"),
            Self::MissingCapture(index) => write!(formatter, "missing runtime capture {index}"),
            Self::CaptureType(index) => {
                write!(formatter, "runtime capture {index} has the wrong type")
            }
            Self::Uninitialized => formatter.write_str("runtime storage is not initialized"),
            Self::Alias => formatter.write_str("runtime mutable references alias"),
            Self::Assertion(message) => write!(formatter, "runtime assertion failed: {message}"),
            Self::StepLimit => formatter.write_str("interpreter step limit exceeded"),
            Self::FrameLimit => formatter.write_str("interpreter call-frame limit exceeded"),
            Self::StorageLimit => formatter.write_str("interpreter storage limit exceeded"),
            Self::Allocation => formatter.write_str("interpreter allocation failed"),
            Self::InvalidVerifiedProgram(message) => {
                write!(
                    formatter,
                    "verified SSA runtime invariant failed: {message}"
                )
            }
        }
    }
}

/// Failure location and progress. Completed quantum operations remain visible in
/// the caller-owned backend state.
#[derive(Debug)]
pub struct RuntimeError<E> {
    pub cause: RuntimeCause<E>,
    pub block: Option<BlockId>,
    pub instruction: Option<usize>,
    pub span: Option<Box<SourceSpan>>,
    pub completed_quantum: usize,
    pub context: Box<[RuntimeContext]>,
}
impl<E: fmt::Display> fmt::Display for RuntimeError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cause.fmt(formatter)
    }
}
impl<E: StdError + 'static> StdError for RuntimeError<E> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match &self.cause {
            RuntimeCause::Backend(error) => Some(error),
            RuntimeCause::Value(error) => Some(error),
            _ => None,
        }
    }
}
impl<E: fmt::Display> RuntimeError<E> {
    /// Build a self-contained execution diagnostic from retained source snapshots.
    ///
    /// A missing source snapshot is not treated as renderable source text. Rust
    /// macro frontends can attach their retained compiler location separately.
    #[must_use]
    pub fn diagnostic(&self, sources: &crate::SourceMap) -> crate::Diagnostic {
        let reason = self.cause.to_string();
        let cause = self.cause.diagnostic_cause(&reason);
        let mut diagnostic = crate::Diagnostic::new(crate::Stage::Execution, cause, &reason);
        diagnostic.occurrence = self.span.as_deref().copied();
        diagnostic.sources = sources.clone();
        if let Some(span) = self
            .span
            .as_deref()
            .copied()
            .filter(|span| diagnostic.sources.slice(*span).is_ok())
        {
            diagnostic.labels.push(crate::Label {
                span,
                style: crate::LabelStyle::Primary,
                message: reason,
            });
        }
        if let Some(block) = self.block {
            let program = block.owner().value();
            diagnostic.provenance.entity = self.instruction.map_or_else(
                || {
                    Some(crate::Entity::Block(crate::IrOccurrence {
                        program,
                        index: block.index(),
                    }))
                },
                |instruction| {
                    Some(crate::Entity::Operation(crate::InstructionOccurrence {
                        program,
                        block: block.index(),
                        instruction,
                    }))
                },
            );
        }
        diagnostic.provenance.execution = self
            .context
            .iter()
            .map(|context| crate::ExecutionContext {
                region: crate::IrOccurrence {
                    program: context.region.owner().value(),
                    index: context.region.index(),
                },
                block: crate::IrOccurrence {
                    program: context.block.owner().value(),
                    index: context.block.index(),
                },
            })
            .collect();
        diagnostic.notes.push(format!(
            "{} quantum operations completed before failure",
            self.completed_quantum
        ));
        diagnostic
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunOutput {
    pub outputs: BTreeMap<String, ClassicalValue>,
    pub completed_quantum: usize,
    pub allocated_qubits: usize,
    pub steps: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Interpreter {
    limits: InterpreterLimits,
}
impl Default for Interpreter {
    fn default() -> Self {
        Self::new(InterpreterLimits::default())
    }
}
impl Interpreter {
    #[must_use]
    pub const fn new(limits: InterpreterLimits) -> Self {
        Self { limits }
    }

    /// Execute with a checked reusable static-dispatch publication.
    /// # Errors
    /// Rejects a mismatched snapshot/capture set and all ordinary runtime failures.
    pub fn run_prepared<B: QuantumBackend>(
        self,
        program: &VerifiedProgram,
        prepared: &PreparedDispatch,
        backend: &mut B,
        inputs: &RunInputs,
        captures: &[ScalarValue],
    ) -> Result<RunOutput, RuntimeError<B::Error>> {
        if prepared.snapshot != program.snapshot() || !prepared.captures_match(captures) {
            return Err((*Fault::bare(RuntimeCause::InvalidVerifiedProgram(
                "prepared dispatch publication mismatch",
            )))
            .finish());
        }
        Engine::new(program, backend, inputs, captures, self.limits)
            .and_then(|mut engine| {
                engine.prepared = Some(prepared);
                engine.run()
            })
            .map_err(|fault| (*fault).finish())
    }
    /// Execute one independently verified program.
    ///
    /// # Errors
    /// Returns a typed, located error for invalid runtime data, exhausted
    /// resources, assertion failure, or a backend failure.
    pub fn run<B: QuantumBackend>(
        self,
        program: &VerifiedProgram,
        backend: &mut B,
        inputs: &RunInputs,
        captures: &[ScalarValue],
    ) -> Result<RunOutput, RuntimeError<B::Error>> {
        Engine::new(program, backend, inputs, captures, self.limits)
            .and_then(Engine::run)
            .map_err(|fault| (*fault).finish())
    }
}

#[derive(Debug, Clone, PartialEq)]
enum RuntimeValue {
    Memory,
    Classical(ClassicalValue),
    Qubits(Vec<usize>),
}

type Cell = Rc<RefCell<Option<RuntimeValue>>>;

#[derive(Clone)]
struct Address {
    cell: Cell,
    path: Vec<usize>,
}

#[derive(Clone)]
enum Binding {
    Owned(Cell),
    Reference(Address),
}

#[derive(Clone)]
struct PreparedArgument {
    slot: SlotId,
    binding: Binding,
    mutable: bool,
    broadcast_qubit: bool,
}

struct Frame {
    region: RegionId,
    slots: Vec<Option<Binding>>,
    values: Vec<Option<RuntimeValue>>,
}

#[derive(Debug, Clone, Copy)]
enum QuantumKind {
    Builtin(GateKind),
    Oracle(usize),
}
#[derive(Debug, Clone)]
struct OwnedGate {
    kind: QuantumKind,
    parameters: Vec<f64>,
    targets: Vec<usize>,
    controls: Vec<QuantumControl>,
    inverse: bool,
}

enum Sink<'a> {
    Backend,
    Buffer(&'a mut GateBuffer),
}

#[derive(Default)]
struct GateBuffer {
    requests: Vec<OwnedGate>,
    bytes: usize,
}

struct Fault<E> {
    cause: RuntimeCause<E>,
    block: Option<BlockId>,
    instruction: Option<usize>,
    span: Option<SourceSpan>,
    completed_quantum: usize,
    context: Vec<RuntimeContext>,
}
impl<E> Fault<E> {
    fn bare(cause: RuntimeCause<E>) -> Box<Self> {
        Box::new(Self {
            cause,
            block: None,
            instruction: None,
            span: None,
            completed_quantum: 0,
            context: Vec::new(),
        })
    }
    fn finish(self) -> RuntimeError<E> {
        RuntimeError {
            cause: self.cause,
            block: self.block,
            instruction: self.instruction,
            span: self.span.map(Box::new),
            completed_quantum: self.completed_quantum,
            context: self.context.into_boxed_slice(),
        }
    }
}

type VmResult<T, E> = Result<T, Box<Fault<E>>>;

struct Engine<'a, B: QuantumBackend> {
    prepared: Option<&'a PreparedDispatch>,
    program: &'a ssa::Program,
    backend: &'a mut B,
    inputs: &'a RunInputs,
    captures: &'a [ScalarValue],
    limits: InterpreterLimits,
    steps: usize,
    frames: usize,
    completed_quantum: usize,
    storage_used: usize,
    value_capacity: usize,
    qubit_offsets: BTreeMap<SlotId, Vec<usize>>,
    context: Vec<RuntimeContext>,
}

impl<'a, B: QuantumBackend> Engine<'a, B> {
    fn new(
        verified: &'a VerifiedProgram,
        backend: &'a mut B,
        inputs: &'a RunInputs,
        captures: &'a [ScalarValue],
        limits: InterpreterLimits,
    ) -> VmResult<Self, B::Error> {
        if limits.steps == 0 || limits.call_frames == 0 {
            return Err(Fault::bare(RuntimeCause::StepLimit));
        }
        let program = verified.program();
        let value_capacity = program
            .blocks
            .iter()
            .flat_map(|block| {
                block.arguments.iter().chain(
                    block
                        .instructions
                        .iter()
                        .flat_map(|instruction| &instruction.results),
                )
            })
            .map(|value| value.id.index())
            .max()
            .map_or(Ok(0), |index| index.checked_add(1).ok_or(()))
            .map_err(|()| Fault::bare(RuntimeCause::StorageLimit))?;
        let storage_used = estimate_storage(program, value_capacity, limits.call_frames)
            .ok_or_else(|| Fault::bare(RuntimeCause::StorageLimit))?;
        if storage_used > limits.storage_bytes {
            return Err(Fault::bare(RuntimeCause::StorageLimit));
        }
        let mut qubit_offsets = BTreeMap::new();
        let mut next = 0usize;
        for slot in &program.slots {
            if let Type::Qubit(count) = slot.ty
                && !slot.reference
            {
                if slot.region != program.entry {
                    return Err(Fault::bare(RuntimeCause::InvalidVerifiedProgram(
                        "non-global qubit allocation",
                    )));
                }
                let end = next
                    .checked_add(count)
                    .ok_or_else(|| Fault::bare(RuntimeCause::StorageLimit))?;
                let mut qubits = Vec::new();
                qubits
                    .try_reserve_exact(count)
                    .map_err(|_| Fault::bare(RuntimeCause::Allocation))?;
                qubits.extend(next..end);
                qubit_offsets.insert(slot.id, qubits);
                next = end;
            }
        }
        for name in inputs.values.keys() {
            if !program
                .slots
                .iter()
                .any(|slot| slot.interface == Interface::Input && slot.name == *name)
            {
                return Err(Fault::bare(RuntimeCause::UnexpectedInput(name.clone())));
            }
        }
        Ok(Self {
            prepared: None,
            program,
            backend,
            inputs,
            captures,
            limits,
            steps: 0,
            frames: 0,
            completed_quantum: 0,
            storage_used,
            value_capacity,
            qubit_offsets,
            context: Vec::new(),
        })
    }

    fn run(mut self) -> VmResult<RunOutput, B::Error> {
        let entry = self.program.entry;
        self.frames = 1;
        self.charge(1)?;
        let entry_block = self
            .program
            .regions
            .get(entry.index())
            .filter(|region| region.id == entry)
            .map(|region| region.entry)
            .ok_or_else(|| {
                self.fault(RuntimeCause::InvalidVerifiedProgram("missing entry region"))
            })?;
        let mut frame = self.make_frame(entry)?;
        self.execute_blocks(
            &mut frame,
            entry_block,
            vec![RuntimeValue::Memory],
            &mut Sink::Backend,
        )?;
        let mut outputs = BTreeMap::new();
        for slot in self
            .program
            .slots
            .iter()
            .filter(|slot| slot.region == entry && slot.interface == Interface::Output)
        {
            let binding = frame
                .slots
                .get(slot.id.index())
                .and_then(Option::as_ref)
                .ok_or_else(|| {
                    self.fault(RuntimeCause::InvalidVerifiedProgram("missing output slot"))
                })?;
            let value =
                read_address(&binding_address(binding)).map_err(|cause| self.fault(cause))?;
            let RuntimeValue::Classical(value) = value else {
                return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
                    "output is not classical",
                )));
            };
            outputs.insert(slot.name.clone(), value);
        }
        Ok(RunOutput {
            outputs,
            completed_quantum: self.completed_quantum,
            allocated_qubits: self.qubit_offsets.values().map(Vec::len).sum(),
            steps: self.steps,
        })
    }

    fn fault(&self, cause: RuntimeCause<B::Error>) -> Box<Fault<B::Error>> {
        Box::new(Fault {
            cause,
            block: self.context.last().map(|context| context.block),
            instruction: None,
            span: None,
            completed_quantum: self.completed_quantum,
            context: self.context.clone(),
        })
    }

    fn charge(&mut self, amount: usize) -> VmResult<(), B::Error> {
        self.steps = self
            .steps
            .checked_add(amount)
            .ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
        if self.steps > self.limits.steps {
            return Err(self.fault(RuntimeCause::StepLimit));
        }
        Ok(())
    }

    fn make_frame(&self, region: RegionId) -> VmResult<Frame, B::Error> {
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(self.program.slots.len())
            .map_err(|_| self.fault(RuntimeCause::Allocation))?;
        slots.resize_with(self.program.slots.len(), || None);
        for slot in self
            .program
            .slots
            .iter()
            .filter(|slot| slot.region == region)
        {
            let destination = slots
                .get_mut(slot.id.index())
                .ok_or_else(|| self.fault(RuntimeCause::InvalidVerifiedProgram("slot index")))?;
            *destination = Some(Binding::Owned(Rc::new(RefCell::new(None))));
        }
        let mut values = Vec::new();
        values
            .try_reserve_exact(self.value_capacity)
            .map_err(|_| self.fault(RuntimeCause::Allocation))?;
        values.resize_with(self.value_capacity, || None);
        Ok(Frame {
            region,
            slots,
            values,
        })
    }

    fn execute_region(
        &mut self,
        region: RegionId,
        arguments: Vec<(SlotId, Binding)>,
        sink: &mut Sink<'_>,
    ) -> VmResult<Option<RuntimeValue>, B::Error> {
        self.frames = self
            .frames
            .checked_add(1)
            .ok_or_else(|| self.fault(RuntimeCause::FrameLimit))?;
        if self.frames > self.limits.call_frames {
            self.frames = self.frames.saturating_sub(1);
            return Err(self.fault(RuntimeCause::FrameLimit));
        }
        self.charge(1)?;
        let region_data = self
            .program
            .regions
            .get(region.index())
            .filter(|candidate| candidate.id == region)
            .ok_or_else(|| self.fault(RuntimeCause::InvalidVerifiedProgram("missing region")))?;
        let mut frame = self.make_frame(region)?;
        for (slot, binding) in arguments {
            let destination = frame.slots.get_mut(slot.index()).ok_or_else(|| {
                self.fault(RuntimeCause::InvalidVerifiedProgram("missing parameter"))
            })?;
            *destination = Some(binding);
        }
        let entry = region_data.entry;
        let result = self.execute_blocks(&mut frame, entry, vec![RuntimeValue::Memory], sink);
        self.frames = self.frames.saturating_sub(1);
        result
    }

    fn execute_blocks(
        &mut self,
        frame: &mut Frame,
        mut block_id: BlockId,
        mut arguments: Vec<RuntimeValue>,
        sink: &mut Sink<'_>,
    ) -> VmResult<Option<RuntimeValue>, B::Error> {
        loop {
            self.charge(1)?;
            let block = self
                .program
                .blocks
                .get(block_id.index())
                .filter(|block| block.id == block_id && block.region == frame.region)
                .cloned()
                .ok_or_else(|| self.fault(RuntimeCause::InvalidVerifiedProgram("missing block")))?;
            self.context.push(RuntimeContext {
                region: frame.region,
                block: block_id,
            });
            if block.arguments.len() != arguments.len() {
                return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
                    "block argument count changed after verification",
                )));
            }
            for (value, argument) in block.arguments.iter().zip(std::mem::take(&mut arguments)) {
                set_value(frame, value.id, argument).map_err(|cause| self.fault(cause))?;
            }
            for (index, instruction) in block.instructions.iter().enumerate() {
                self.charge(1)?;
                if let Err(mut error) = self.execute_instruction(frame, instruction, sink) {
                    error.block = Some(block_id);
                    error.instruction = Some(index);
                    error.span = instruction.span;
                    return Err(error);
                }
            }
            self.charge(1)?;
            let terminator = block.terminator.as_ref().ok_or_else(|| {
                self.fault(RuntimeCause::InvalidVerifiedProgram("missing terminator"))
            })?;
            match terminator {
                Terminator::Jump(edge) => {
                    arguments = edge_values(frame, edge).map_err(|cause| self.fault(cause))?;
                    block_id = edge.target;
                }
                Terminator::Branch {
                    condition,
                    then_edge,
                    else_edge,
                } => {
                    let condition = scalar_value(frame, *condition)
                        .and_then(|value| value.to_bool().map_err(RuntimeCause::Value))
                        .map_err(|cause| self.fault(cause))?;
                    let edge = if condition { then_edge } else { else_edge };
                    arguments = edge_values(frame, edge).map_err(|cause| self.fault(cause))?;
                    block_id = edge.target;
                }
                Terminator::Return { value, .. } => {
                    let result = value
                        .map(|value| get_value(frame, value))
                        .transpose()
                        .map_err(|cause| self.fault(cause))?;
                    self.context.pop();
                    return Ok(result);
                }
                Terminator::End { .. } => {
                    self.context.pop();
                    return Ok(None);
                }
            }
            self.context.pop();
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Exhaustive verified SSA dispatch keeps result contracts adjacent to each operation"
    )]
    fn execute_instruction(
        &mut self,
        frame: &mut Frame,
        instruction: &Instruction,
        sink: &mut Sink<'_>,
    ) -> VmResult<(), B::Error> {
        use InstructionKind as K;
        let results = match &instruction.kind {
            K::Constant(value) => vec![RuntimeValue::Classical(ClassicalValue::Scalar(*value))],
            K::Unary { operator, value } => vec![RuntimeValue::Classical(ClassicalValue::Scalar(
                scalar_value(frame, *value)
                    .and_then(|value| value.unary(*operator).map_err(RuntimeCause::Value))
                    .map_err(|cause| self.fault(cause))?,
            ))],
            K::Binary {
                operator,
                left,
                right,
            } => vec![RuntimeValue::Classical(ClassicalValue::Scalar(
                scalar_value(frame, *left)
                    .and_then(|left| {
                        scalar_value(frame, *right).and_then(|right| {
                            left.binary(*operator, &right).map_err(RuntimeCause::Value)
                        })
                    })
                    .map_err(|cause| self.fault(cause))?,
            ))],
            K::Cast { value, ty } => vec![RuntimeValue::Classical(ClassicalValue::Scalar(
                scalar_value(frame, *value)
                    .and_then(|value| value.cast(*ty).map_err(RuntimeCause::Value))
                    .map_err(|cause| self.fault(cause))?,
            ))],
            K::GateParameter { value } => {
                let value = scalar_value(frame, *value)
                    .and_then(|value| value.to_f64().map_err(RuntimeCause::Value))
                    .and_then(|value| {
                        ScalarValue::floating(FloatWidth::F64, value).map_err(RuntimeCause::Value)
                    })
                    .map_err(|cause| self.fault(cause))?;
                vec![RuntimeValue::Classical(ClassicalValue::Scalar(value))]
            }
            K::Array { values } => {
                let values = values
                    .iter()
                    .map(|value| {
                        let value = get_value(frame, *value)?;
                        let RuntimeValue::Classical(value) = value else {
                            return Err(RuntimeCause::InvalidVerifiedProgram(
                                "array element is not classical",
                            ));
                        };
                        Ok(value)
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|cause| self.fault(cause))?;
                vec![RuntimeValue::Classical(ClassicalValue::Array(values))]
            }
            K::Builtin { name, arguments } => {
                let arguments = arguments
                    .iter()
                    .map(|value| scalar_value(frame, *value))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|cause| self.fault(cause))?;
                vec![RuntimeValue::Classical(ClassicalValue::Scalar(
                    ScalarValue::function(name, &arguments)
                        .map_err(RuntimeCause::Value)
                        .map_err(|cause| self.fault(cause))?,
                ))]
            }
            K::Index { value, index } => {
                let value = get_value(frame, *value).map_err(|cause| self.fault(cause))?;
                let index = scalar_value(frame, *index).map_err(|cause| self.fault(cause))?;
                vec![index_runtime_value(&value, &index).map_err(|cause| self.fault(cause))?]
            }
            K::Capture { index, ty } => {
                let value = self
                    .captures
                    .get(*index)
                    .ok_or_else(|| self.fault(RuntimeCause::MissingCapture(*index)))?;
                if ty != &Type::Scalar(value.ty()) {
                    return Err(self.fault(RuntimeCause::CaptureType(*index)));
                }
                vec![RuntimeValue::Classical(ClassicalValue::Scalar(*value))]
            }
            K::RangeAdvance { current, step, end } => {
                let current = scalar_value(frame, *current).map_err(|cause| self.fault(cause))?;
                let step = scalar_value(frame, *step).map_err(|cause| self.fault(cause))?;
                let end = scalar_value(frame, *end).map_err(|cause| self.fault(cause))?;
                let current_integer = current
                    .to_i128()
                    .map_err(RuntimeCause::Value)
                    .map_err(|cause| self.fault(cause))?;
                let step_integer = step
                    .to_i128()
                    .map_err(RuntimeCause::Value)
                    .map_err(|cause| self.fault(cause))?;
                let end_integer = end
                    .to_i128()
                    .map_err(RuntimeCause::Value)
                    .map_err(|cause| self.fault(cause))?;
                if step_integer == 0 {
                    return Err(
                        self.fault(RuntimeCause::Assertion("range step cannot be zero".into()))
                    );
                }
                let next = current_integer
                    .checked_add(step_integer)
                    .ok_or_else(|| self.fault(RuntimeCause::Value(ValueError::Overflow)))?;
                let more = if step_integer > 0 {
                    next <= end_integer
                } else {
                    next >= end_integer
                };
                let successor = if more {
                    scalar_from_i128(current.ty(), next).map_err(|cause| self.fault(cause))?
                } else {
                    current
                };
                vec![
                    RuntimeValue::Classical(ClassicalValue::Scalar(successor)),
                    RuntimeValue::Classical(ClassicalValue::Scalar(ScalarValue::boolean(more))),
                ]
            }
            K::Assert {
                condition, message, ..
            } => {
                if !scalar_value(frame, *condition)
                    .and_then(|value| value.to_bool().map_err(RuntimeCause::Value))
                    .map_err(|cause| self.fault(cause))?
                {
                    return Err(self.fault(RuntimeCause::Assertion(message.clone())));
                }
                vec![RuntimeValue::Memory]
            }
            K::Input { slot, .. } => {
                let slot_data = self.slot(*slot)?;
                let input = self.inputs.get(&slot_data.name).ok_or_else(|| {
                    self.fault(RuntimeCause::MissingInput(slot_data.name.clone()))
                })?;
                if !classical_matches(input, &slot_data.ty) {
                    return Err(self.fault(RuntimeCause::InputType(slot_data.name.clone())));
                }
                initialize_binding(frame, *slot, RuntimeValue::Classical(input.clone()))
                    .map_err(|cause| self.fault(cause))?;
                vec![RuntimeValue::Memory]
            }
            K::Allocate { slot, .. } => {
                let qubits = self.qubit_offsets.get(slot).cloned().ok_or_else(|| {
                    self.fault(RuntimeCause::InvalidVerifiedProgram(
                        "missing qubit allocation",
                    ))
                })?;
                initialize_binding(frame, *slot, RuntimeValue::Qubits(qubits))
                    .map_err(|cause| self.fault(cause))?;
                vec![RuntimeValue::Memory]
            }
            K::AllocateArray { slot, .. } => {
                let ty = &self.slot(*slot)?.ty;
                let value = zero_runtime_value(ty).map_err(|cause| self.fault(cause))?;
                initialize_binding(frame, *slot, value).map_err(|cause| self.fault(cause))?;
                vec![RuntimeValue::Memory]
            }
            K::Load { place, .. } => {
                let address = self.resolve_place(frame, place)?;
                vec![read_address(&address).map_err(|cause| self.fault(cause))?]
            }
            K::Store {
                place,
                value,
                initializing,
                ..
            } => {
                let address = self.resolve_place(frame, place)?;
                let value = get_value(frame, *value).map_err(|cause| self.fault(cause))?;
                write_address(&address, value, *initializing).map_err(|cause| self.fault(cause))?;
                vec![RuntimeValue::Memory]
            }
            K::Call {
                region,
                arguments,
                controls,
                modifiers,
                ..
            } => {
                let result =
                    self.execute_call(frame, *region, arguments, controls, modifiers, sink)?;
                result
                    .into_iter()
                    .chain(std::iter::once(RuntimeValue::Memory))
                    .collect()
            }
            K::Gate {
                gate,
                arguments,
                operands,
                modifiers,
                ..
            } => {
                if let Some((occurrence, requests)) =
                    instruction.results.first().and_then(|result| {
                        self.prepared
                            .and_then(|p| p.requests.get(&result.id))
                            .map(|requests| (result.id, requests.clone()))
                    })
                {
                    self.replay_prepared(occurrence, &requests, sink)?;
                } else {
                    self.execute_gate(frame, *gate, arguments, operands, modifiers, sink)?;
                }
                vec![RuntimeValue::Memory]
            }
            K::Measure { place, .. } => {
                let qubits = self.place_qubits(frame, place)?;
                let mut bits = vec![false; qubits.len()];
                for (bit, qubit) in bits.iter_mut().zip(qubits) {
                    self.charge(1)?;
                    *bit = match sink {
                        Sink::Backend => self
                            .backend
                            .measure(qubit)
                            .map_err(|error| self.fault(RuntimeCause::Backend(error)))?,
                        Sink::Buffer(_) => {
                            return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
                                "measurement inside unitary gate",
                            )));
                        }
                    };
                    self.completed_quantum = self
                        .completed_quantum
                        .checked_add(1)
                        .ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
                }
                vec![
                    RuntimeValue::Classical(ClassicalValue::Scalar(
                        bit_value(&bits).map_err(|cause| self.fault(cause))?,
                    )),
                    RuntimeValue::Memory,
                ]
            }
            K::Payload {
                capture, places, ..
            } => {
                let mut qubits = Vec::new();
                for place in places {
                    qubits.extend(self.place_qubits(frame, place)?);
                }
                unique_qubits(&qubits).map_err(|cause| self.fault(cause))?;
                self.charge(1)?;
                match sink {
                    Sink::Backend => self
                        .backend
                        .apply_payload(*capture, &qubits)
                        .ok_or_else(|| {
                            self.fault(RuntimeCause::Unsupported("quantum payload execution"))
                        })?
                        .map_err(|error| self.fault(RuntimeCause::Backend(error)))?,
                    Sink::Buffer(_) => {
                        return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
                            "payload inside unitary gate",
                        )));
                    }
                }
                self.completed_quantum = self
                    .completed_quantum
                    .checked_add(1)
                    .ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
                vec![RuntimeValue::Memory]
            }
            K::Reset { place, .. } => {
                for qubit in self.place_qubits(frame, place)? {
                    self.charge(1)?;
                    match sink {
                        Sink::Backend => self
                            .backend
                            .reset(qubit)
                            .map_err(|error| self.fault(RuntimeCause::Backend(error)))?,
                        Sink::Buffer(_) => {
                            return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
                                "reset inside unitary gate",
                            )));
                        }
                    }
                    self.completed_quantum = self
                        .completed_quantum
                        .checked_add(1)
                        .ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
                }
                vec![RuntimeValue::Memory]
            }
            K::Barrier { places, .. } => {
                let mut qubits = Vec::new();
                for place in places {
                    qubits.extend(self.place_qubits(frame, place)?);
                }
                unique_qubits(&qubits).map_err(|cause| self.fault(cause))?;
                self.charge(1)?;
                match sink {
                    Sink::Backend => self
                        .backend
                        .barrier(&qubits)
                        .map_err(|error| self.fault(RuntimeCause::Backend(error)))?,
                    Sink::Buffer(_) => {
                        return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
                            "barrier inside unitary gate",
                        )));
                    }
                }
                self.completed_quantum = self
                    .completed_quantum
                    .checked_add(1)
                    .ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
                vec![RuntimeValue::Memory]
            }
        };
        if results.len() != instruction.results.len() {
            return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
                "instruction result count changed after verification",
            )));
        }
        for (result, value) in instruction.results.iter().zip(results) {
            set_value(frame, result.id, value).map_err(|cause| self.fault(cause))?;
        }
        Ok(())
    }

    fn slot(&self, id: SlotId) -> VmResult<&ssa::Slot, B::Error> {
        self.program
            .slots
            .get(id.index())
            .filter(|slot| slot.id == id)
            .ok_or_else(|| self.fault(RuntimeCause::InvalidVerifiedProgram("missing slot")))
    }

    fn resolve_place(&self, frame: &Frame, place: &Place) -> VmResult<Address, B::Error> {
        let binding = frame
            .slots
            .get(place.slot.index())
            .and_then(Option::as_ref)
            .ok_or_else(|| {
                self.fault(RuntimeCause::InvalidVerifiedProgram(
                    "missing place binding",
                ))
            })?;
        let mut address = binding_address(binding);
        address
            .path
            .try_reserve(place.indices.len())
            .map_err(|_| self.fault(RuntimeCause::Allocation))?;
        let mut current = self.slot(place.slot)?.ty.clone();
        for index in &place.indices {
            let scalar = scalar_value(frame, *index).map_err(|cause| self.fault(cause))?;
            let length = type_length(&current).ok_or_else(|| {
                self.fault(RuntimeCause::InvalidVerifiedProgram(
                    "indexed non-collection",
                ))
            })?;
            let index = scalar
                .to_index(length)
                .map_err(RuntimeCause::Value)
                .map_err(|cause| self.fault(cause))?;
            address.path.push(index);
            current = current.indexed().ok_or_else(|| {
                self.fault(RuntimeCause::InvalidVerifiedProgram(
                    "indexed non-collection",
                ))
            })?;
        }
        Ok(address)
    }

    fn place_qubits(&self, frame: &Frame, place: &Place) -> VmResult<Vec<usize>, B::Error> {
        let value =
            read_address(&self.resolve_place(frame, place)?).map_err(|cause| self.fault(cause))?;
        let RuntimeValue::Qubits(qubits) = value else {
            return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
                "quantum place is not qubits",
            )));
        };
        Ok(qubits)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Gate calls prepare checked references, lane broadcasts, modifiers, and buffered replay atomically"
    )]
    fn execute_call(
        &mut self,
        frame: &Frame,
        region: RegionId,
        arguments: &[CallArgument],
        controls: &[Place],
        modifiers: &[GateModifier],
        sink: &mut Sink<'_>,
    ) -> VmResult<Option<RuntimeValue>, B::Error> {
        let callee = self
            .program
            .regions
            .get(region.index())
            .filter(|candidate| candidate.id == region)
            .ok_or_else(|| self.fault(RuntimeCause::InvalidVerifiedProgram("missing callee")))?;
        let oracle = callee.oracle.as_ref().map(ssa::OracleId::index);
        let parameters = callee.parameters.clone();
        let gate = callee.gate;
        if arguments.len() != parameters.len() {
            return Err(self.fault(RuntimeCause::InvalidVerifiedProgram("call argument arity")));
        }
        let mut prepared = Vec::new();
        prepared
            .try_reserve_exact(arguments.len())
            .map_err(|_| self.fault(RuntimeCause::Allocation))?;
        for (argument, parameter) in arguments.iter().zip(parameters) {
            let parameter_slot = self.slot(parameter)?;
            let (binding, mutable) = match argument {
                CallArgument::Value(value) => (
                    Binding::Owned(Rc::new(RefCell::new(Some(
                        get_value(frame, *value).map_err(|cause| self.fault(cause))?,
                    )))),
                    false,
                ),
                CallArgument::Reference {
                    place,
                    mutable: is_mutable,
                } => {
                    let address = self.resolve_place(frame, place)?;
                    (Binding::Reference(address), *is_mutable)
                }
            };
            prepared.push(PreparedArgument {
                slot: parameter,
                binding,
                mutable,
                broadcast_qubit: gate && parameter_slot.ty == Type::Qubit(1),
            });
        }
        if !gate {
            validate_reference_aliases(&prepared, &[]).map_err(|cause| self.fault(cause))?;
            let bindings = prepared
                .into_iter()
                .map(|argument| (argument.slot, argument.binding))
                .collect();
            return self.execute_region(region, bindings, sink);
        }
        let modifier = self.modifier_values(frame, modifiers)?;
        if controls.len() != modifier.controls.len() {
            return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
                "external control arity",
            )));
        }
        let external = controls
            .iter()
            .zip(&modifier.controls)
            .map(|(place, positive)| {
                self.resolve_place(frame, place)
                    .map(|address| (address, *positive))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut width = 1usize;
        for argument in &prepared {
            if argument.broadcast_qubit {
                let Binding::Reference(address) = &argument.binding else {
                    return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
                        "gate qubit parameter is not a reference",
                    )));
                };
                width = merge_broadcast_width(
                    width,
                    address_qubit_count(address).map_err(|cause| self.fault(cause))?,
                )
                .map_err(|cause| self.fault(cause))?;
            }
        }
        for (address, _) in &external {
            width = merge_broadcast_width(
                width,
                address_qubit_count(address).map_err(|cause| self.fault(cause))?,
            )
            .map_err(|cause| self.fault(cause))?;
        }
        let mut buffer = GateBuffer::default();
        for lane in 0..width {
            let bindings = prepared
                .iter()
                .map(|argument| {
                    selected_argument(argument, lane).map(|binding| (argument.slot, binding))
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|cause| self.fault(cause))?;
            let selected_controls = external
                .iter()
                .map(|(address, positive)| {
                    select_qubit(address, lane).map(|qubit| QuantumControl {
                        qubit,
                        positive: *positive,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|cause| self.fault(cause))?;
            validate_selected_aliases(&bindings, &prepared, &external, lane)
                .map_err(|cause| self.fault(cause))?;
            let first = buffer.requests.len();
            let result = if let Some(capture) = oracle {
                let targets = prepared
                    .iter()
                    .map(|argument| {
                        let Binding::Reference(address) = &argument.binding else {
                            return Err(RuntimeCause::InvalidVerifiedProgram(
                                "oracle operand is not a reference",
                            ));
                        };
                        select_qubit(address, lane)
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|cause| self.fault(cause))?;
                self.dispatch(
                    OwnedGate {
                        kind: QuantumKind::Oracle(capture),
                        parameters: Vec::new(),
                        targets,
                        controls: Vec::new(),
                        inverse: false,
                    },
                    &mut Sink::Buffer(&mut buffer),
                    None,
                )?;
                None
            } else {
                self.execute_region(region, bindings, &mut Sink::Buffer(&mut buffer))?
            };
            if result.is_some() {
                return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
                    "unitary gate returned a value",
                )));
            }
            self.add_external_controls(&mut buffer, first, &selected_controls)?;
        }
        if modifier.exact_inverse
            && buffer
                .requests
                .iter()
                .any(|request| matches!(request.kind, QuantumKind::Oracle(_)))
        {
            return Err(self.fault(RuntimeCause::Unsupported(
                "exact inverse or negative power of a numerical oracle",
            )));
        }
        if modifier.inverse {
            buffer.requests.reverse();
            for request in &mut buffer.requests {
                request.inverse = !request.inverse;
            }
        }
        self.replay(&buffer.requests, modifier.repetitions, sink)?;
        self.storage_used = self.storage_used.saturating_sub(buffer.bytes);
        Ok(None)
    }

    fn execute_gate(
        &mut self,
        frame: &Frame,
        gate: GateKind,
        arguments: &[ValueId],
        operands: &[Place],
        modifiers: &[GateModifier],
        sink: &mut Sink<'_>,
    ) -> VmResult<(), B::Error> {
        let parameters = arguments
            .iter()
            .map(|value| {
                scalar_value(frame, *value).and_then(|value| {
                    value
                        .to_f64()
                        .map_err(RuntimeCause::Value)
                        .and_then(|value| {
                            if value.is_finite() {
                                Ok(value)
                            } else {
                                Err(RuntimeCause::Value(ValueError::NonFinite))
                            }
                        })
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|cause| self.fault(cause))?;
        let modifier = self.modifier_values(frame, modifiers)?;
        let explicit_states = modifier.controls;
        let definition = gate.definition();
        let explicit_count = explicit_states.len();
        let control_count = explicit_count
            .checked_add(definition.intrinsic_controls)
            .ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
        let resolved = operands
            .iter()
            .map(|place| self.place_qubits(frame, place))
            .collect::<Result<Vec<_>, _>>()?;
        let width = resolved.iter().map(Vec::len).max().unwrap_or(1);
        if resolved
            .iter()
            .any(|qubits| qubits.len() != 1 && qubits.len() != width)
        {
            return Err(self.fault(RuntimeCause::InvalidVerifiedProgram(
                "broadcast width mismatch",
            )));
        }
        let dispatches = width
            .checked_mul(modifier.repetitions)
            .ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
        self.charge(dispatches)?;
        for _ in 0..modifier.repetitions {
            for lane in 0..width {
                let selected = resolved
                    .iter()
                    .map(|qubits| {
                        let index = if qubits.len() == 1 { 0 } else { lane };
                        qubits.get(index).copied().ok_or_else(|| {
                            self.fault(RuntimeCause::InvalidVerifiedProgram(
                                "broadcast lane is outside operand",
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let controls = selected
                    .iter()
                    .take(explicit_count)
                    .zip(&explicit_states)
                    .map(|(qubit, positive)| QuantumControl {
                        qubit: *qubit,
                        positive: *positive,
                    })
                    .chain(
                        selected
                            .iter()
                            .skip(explicit_count)
                            .take(definition.intrinsic_controls)
                            .map(|qubit| QuantumControl {
                                qubit: *qubit,
                                positive: true,
                            }),
                    )
                    .collect::<Vec<_>>();
                let targets = selected
                    .get(control_count..)
                    .ok_or_else(|| {
                        self.fault(RuntimeCause::InvalidVerifiedProgram("gate control arity"))
                    })?
                    .to_vec();
                validate_gate(gate, &parameters, &targets, &controls)
                    .map_err(|cause| self.fault(cause))?;
                let request = OwnedGate {
                    kind: QuantumKind::Builtin(gate),
                    parameters: parameters.clone(),
                    targets,
                    controls,
                    inverse: modifier.inverse,
                };
                self.dispatch(request, sink, None)?;
            }
        }
        Ok(())
    }

    fn modifier_values(
        &self,
        frame: &Frame,
        modifiers: &[GateModifier],
    ) -> VmResult<ModifierValues, B::Error> {
        let mut inverse = false;
        let mut exact_inverse = false;
        let mut repetitions = 1i128;
        let mut controls = Vec::new();
        for modifier in modifiers {
            match modifier {
                GateModifier::Inverse => {
                    inverse = !inverse;
                    exact_inverse = true;
                }
                GateModifier::Adjoint => inverse = !inverse,
                GateModifier::Control { positive, count } => {
                    controls.extend(std::iter::repeat_n(*positive, *count));
                }
                GateModifier::Power(value) => {
                    let power = scalar_value(frame, *value)
                        .and_then(|value| value.to_i128().map_err(RuntimeCause::Value))
                        .map_err(|cause| self.fault(cause))?;
                    exact_inverse |= power < 0;
                    repetitions = repetitions
                        .checked_mul(power)
                        .ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
                }
            }
        }
        if repetitions < 0 {
            inverse = !inverse;
        }
        Ok(ModifierValues {
            exact_inverse,
            inverse,
            repetitions: usize::try_from(
                repetitions
                    .checked_abs()
                    .ok_or_else(|| self.fault(RuntimeCause::StepLimit))?,
            )
            .map_err(|_| self.fault(RuntimeCause::StepLimit))?,
            controls,
        })
    }

    fn add_external_controls(
        &mut self,
        buffer: &mut GateBuffer,
        first: usize,
        external: &[QuantumControl],
    ) -> VmResult<(), B::Error> {
        let requests = buffer
            .requests
            .get_mut(first..)
            .ok_or_else(|| self.fault(RuntimeCause::InvalidVerifiedProgram("gate buffer index")))?;
        let added = requests
            .len()
            .checked_mul(external.len())
            .and_then(|count| count.checked_mul(size_of::<QuantumControl>()))
            .ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
        let total = self
            .storage_used
            .checked_add(added)
            .ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
        if total > self.limits.storage_bytes {
            return Err(self.fault(RuntimeCause::StorageLimit));
        }
        for request in requests.iter() {
            let controls = external
                .iter()
                .copied()
                .chain(request.controls.iter().copied())
                .collect::<Vec<_>>();
            match request.kind {
                QuantumKind::Builtin(gate) => {
                    validate_gate(gate, &request.parameters, &request.targets, &controls)
                }
                QuantumKind::Oracle(_) => unique_qubits(
                    &controls
                        .iter()
                        .map(|control| control.qubit)
                        .chain(request.targets.iter().copied())
                        .collect::<Vec<_>>(),
                ),
            }
            .map_err(|cause| self.fault(cause))?;
        }
        for request in requests {
            let mut controls = Vec::new();
            controls
                .try_reserve_exact(
                    external
                        .len()
                        .checked_add(request.controls.len())
                        .ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?,
                )
                .map_err(|_| self.fault(RuntimeCause::Allocation))?;
            controls.extend_from_slice(external);
            controls.extend_from_slice(&request.controls);
            request.controls = controls;
        }
        buffer.bytes = buffer
            .bytes
            .checked_add(added)
            .ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
        self.storage_used = total;
        Ok(())
    }

    fn replay(
        &mut self,
        buffer: &[OwnedGate],
        repetitions: usize,
        sink: &mut Sink<'_>,
    ) -> VmResult<(), B::Error> {
        let dispatches = buffer
            .len()
            .checked_mul(repetitions)
            .ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
        self.charge(dispatches)?;
        for _ in 0..repetitions {
            for request in buffer {
                self.dispatch(request.clone(), sink, None)?;
            }
        }
        Ok(())
    }

    fn replay_prepared(
        &mut self,
        occurrence: ValueId,
        requests: &[OwnedGate],
        sink: &mut Sink<'_>,
    ) -> VmResult<(), B::Error> {
        self.charge(requests.len())?;
        for (lane, request) in requests.iter().enumerate() {
            self.dispatch(request.clone(), sink, Some(DispatchId { occurrence, lane }))?;
        }
        Ok(())
    }
    fn dispatch(
        &mut self,
        request: OwnedGate,
        sink: &mut Sink<'_>,
        prepared: Option<DispatchId>,
    ) -> VmResult<(), B::Error> {
        match sink {
            Sink::Backend => {
                if let QuantumKind::Oracle(capture) = request.kind {
                    self.backend
                        .apply_oracle(OracleRequest {
                            capture,
                            targets: &request.targets,
                            controls: &request.controls,
                            adjoint: request.inverse,
                        })
                        .ok_or_else(|| self.fault(RuntimeCause::Unsupported("oracle execution")))?
                        .map_err(|error| self.fault(RuntimeCause::Backend(error)))?;
                } else if let QuantumKind::Builtin(gate) = request.kind {
                    let request = GateRequest {
                        gate,
                        parameters: &request.parameters,
                        targets: &request.targets,
                        controls: &request.controls,
                        inverse: request.inverse,
                    };
                    if let Some(id) = prepared {
                        self.backend.apply_prepared_gate(id, request)
                    } else {
                        self.backend.apply_gate(request)
                    }
                    .map_err(|error| self.fault(RuntimeCause::Backend(error)))?;
                }
                self.completed_quantum = self
                    .completed_quantum
                    .checked_add(1)
                    .ok_or_else(|| self.fault(RuntimeCause::StepLimit))?;
            }
            Sink::Buffer(buffer) => {
                let bytes =
                    gate_storage(&request).ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
                let total = self
                    .storage_used
                    .checked_add(bytes)
                    .ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
                if total > self.limits.storage_bytes {
                    return Err(self.fault(RuntimeCause::StorageLimit));
                }
                buffer
                    .requests
                    .try_reserve_exact(1)
                    .map_err(|_| self.fault(RuntimeCause::Allocation))?;
                buffer.requests.push(request);
                buffer.bytes = buffer
                    .bytes
                    .checked_add(bytes)
                    .ok_or_else(|| self.fault(RuntimeCause::StorageLimit))?;
                self.storage_used = total;
            }
        }
        Ok(())
    }
}

fn gate_storage(request: &OwnedGate) -> Option<usize> {
    size_of::<OwnedGate>()
        .checked_add(request.parameters.len().checked_mul(size_of::<f64>())?)?
        .checked_add(request.targets.len().checked_mul(size_of::<usize>())?)?
        .checked_add(
            request
                .controls
                .len()
                .checked_mul(size_of::<QuantumControl>())?,
        )
}

struct ModifierValues {
    exact_inverse: bool,
    inverse: bool,
    repetitions: usize,
    controls: Vec<bool>,
}

/// Conservative VM frame/storage reservation for explicit runtime frame limits.
///
/// Includes scalar/array payloads, slot cells, SSA values and returned outputs.
/// Returns `None` for arithmetic overflow. The runtime applies this same estimate.
#[must_use]
pub fn runtime_storage(program: &VerifiedProgram, call_frames: usize) -> Option<usize> {
    let values = program
        .blocks()
        .iter()
        .flat_map(|block| {
            block
                .arguments
                .iter()
                .chain(block.instructions.iter().flat_map(|i| &i.results))
        })
        .map(|value| value.id.index())
        .max()
        .map_or(Some(0), |index| index.checked_add(1))?;
    estimate_storage(program.program(), values, call_frames)
}
/// Conservative owned result storage, including map nodes, names and nested arrays.
/// Returns `None` if the declared output shapes overflow storage arithmetic.
#[must_use]
pub fn output_storage(program: &VerifiedProgram) -> Option<usize> {
    output_storage_inner(program.program())
}
fn output_storage_inner(program: &ssa::Program) -> Option<usize> {
    // A full B-tree node per entry overcounts sparse nodes and internal edges.
    const NODE: usize =
        16 * (size_of::<String>() + size_of::<ClassicalValue>() + size_of::<usize>());
    program
        .slots
        .iter()
        .filter(|slot| slot.interface == Interface::Output)
        .try_fold(size_of::<RunOutput>(), |total, slot| {
            total
                .checked_add(NODE)?
                .checked_add(slot.name.len())?
                .checked_add(type_bytes(&slot.ty)?)
        })
}
fn estimate_storage(program: &ssa::Program, values: usize, frames: usize) -> Option<usize> {
    let slot_bytes = program.slots.iter().try_fold(0usize, |total, slot| {
        total.checked_add(type_bytes(&slot.ty)?)
    })?;
    let value_bytes = values.checked_mul(size_of::<Option<RuntimeValue>>())?;
    let value_payload_bytes = program
        .blocks
        .iter()
        .flat_map(|block| {
            block.arguments.iter().chain(
                block
                    .instructions
                    .iter()
                    .flat_map(|instruction| &instruction.results),
            )
        })
        .try_fold(0usize, |total, value| {
            total.checked_add(type_bytes(&value.ty)?)
        })?;
    let cell_bytes = program
        .slots
        .len()
        .checked_mul(size_of::<RefCell<Option<RuntimeValue>>>())?;
    let frame_bytes = program
        .slots
        .len()
        .checked_mul(size_of::<Option<Binding>>())?
        .checked_add(value_bytes)?
        .checked_add(value_payload_bytes)?
        .checked_add(cell_bytes)?
        .checked_add(slot_bytes)?;
    frame_bytes
        .checked_mul(frames)?
        .checked_add(output_storage_inner(program)?)
}

fn type_bytes(ty: &Type) -> Option<usize> {
    match ty {
        Type::Scalar(scalar) => Some(scalar.storage_bytes()),
        Type::Array {
            dimensions,
            element: _,
        } => dimensions
            .iter()
            .try_fold((0usize, 1usize), |(total, parents), dimension| {
                let items = parents.checked_mul(*dimension)?;
                let bytes = items.checked_mul(size_of::<ClassicalValue>())?;
                Some((total.checked_add(bytes)?, items))
            })
            .map(|(total, _)| total),
        Type::Qubit(count) => count.checked_mul(size_of::<usize>()),
        Type::Memory => Some(1),
        Type::Void => Some(0),
    }
}

fn zero_runtime_value<E>(ty: &Type) -> Result<RuntimeValue, RuntimeCause<E>> {
    match ty {
        Type::Scalar(ScalarType::Bit(width)) => {
            let length = usize::from(width.value());
            let mut bits = String::new();
            bits.try_reserve_exact(length)
                .map_err(|_| RuntimeCause::Allocation)?;
            bits.extend(std::iter::repeat_n('0', length));
            ScalarValue::bitstring(&bits)
                .map(|value| RuntimeValue::Classical(ClassicalValue::Scalar(value)))
                .map_err(RuntimeCause::Value)
        }
        Type::Array {
            element,
            dimensions,
        } => zero_array(*element, dimensions).map(RuntimeValue::Classical),
        Type::Scalar(_) | Type::Qubit(_) | Type::Memory | Type::Void => {
            Err(RuntimeCause::InvalidVerifiedProgram("AllocateArray type"))
        }
    }
}

fn zero_array<E>(
    element: ScalarType,
    dimensions: &[usize],
) -> Result<ClassicalValue, RuntimeCause<E>> {
    let (&length, remaining) = dimensions
        .split_first()
        .ok_or(RuntimeCause::InvalidVerifiedProgram("empty array shape"))?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(length)
        .map_err(|_| RuntimeCause::Allocation)?;
    for _ in 0..length {
        let value = if remaining.is_empty() {
            zero_scalar(element)?
        } else {
            zero_array(element, remaining)?
        };
        values.push(value);
    }
    Ok(ClassicalValue::Array(values))
}

fn zero_scalar<E>(ty: ScalarType) -> Result<ClassicalValue, RuntimeCause<E>> {
    let value = match ty {
        ScalarType::Bool => ScalarValue::boolean(false),
        ScalarType::Bit(width) => {
            let length = usize::from(width.value());
            let mut bits = String::new();
            bits.try_reserve_exact(length)
                .map_err(|_| RuntimeCause::Allocation)?;
            bits.extend(std::iter::repeat_n('0', length));
            ScalarValue::bitstring(&bits).map_err(RuntimeCause::Value)?
        }
        ScalarType::Int(width) => ScalarValue::signed(width, 0).map_err(RuntimeCause::Value)?,
        ScalarType::Uint(width) => ScalarValue::unsigned(width, 0).map_err(RuntimeCause::Value)?,
        ScalarType::Angle(width) => {
            ScalarValue::angle_bits(width, 0).map_err(RuntimeCause::Value)?
        }
        ScalarType::Float(width) => {
            ScalarValue::floating(width, 0.0).map_err(RuntimeCause::Value)?
        }
    };
    Ok(ClassicalValue::Scalar(value))
}

fn classical_matches(value: &ClassicalValue, ty: &Type) -> bool {
    match (value, ty) {
        (ClassicalValue::Scalar(value), Type::Scalar(ty)) => value.ty() == *ty,
        (
            ClassicalValue::Array(values),
            Type::Array {
                element,
                dimensions,
            },
        ) => dimensions.split_first().is_some_and(|(length, remaining)| {
            *length == values.len()
                && values
                    .iter()
                    .all(|value| classical_array_element_matches(value, *element, remaining))
        }),
        _ => false,
    }
}

fn classical_array_element_matches(
    value: &ClassicalValue,
    element: ScalarType,
    remaining: &[usize],
) -> bool {
    if remaining.is_empty() {
        matches!(value, ClassicalValue::Scalar(value) if value.ty() == element)
    } else {
        let ClassicalValue::Array(values) = value else {
            return false;
        };
        remaining.split_first().is_some_and(|(length, tail)| {
            *length == values.len()
                && values
                    .iter()
                    .all(|value| classical_array_element_matches(value, element, tail))
        })
    }
}

fn set_value<E>(
    frame: &mut Frame,
    id: ValueId,
    value: RuntimeValue,
) -> Result<(), RuntimeCause<E>> {
    let destination = frame
        .values
        .get_mut(id.index())
        .ok_or(RuntimeCause::InvalidVerifiedProgram("value index"))?;
    *destination = Some(value);
    Ok(())
}
fn get_value<E>(frame: &Frame, id: ValueId) -> Result<RuntimeValue, RuntimeCause<E>> {
    frame
        .values
        .get(id.index())
        .and_then(Option::as_ref)
        .cloned()
        .ok_or(RuntimeCause::InvalidVerifiedProgram("undefined SSA value"))
}
fn scalar_value<E>(frame: &Frame, id: ValueId) -> Result<ScalarValue, RuntimeCause<E>> {
    let RuntimeValue::Classical(ClassicalValue::Scalar(value)) = get_value(frame, id)? else {
        return Err(RuntimeCause::InvalidVerifiedProgram(
            "SSA value is not scalar",
        ));
    };
    Ok(value)
}
fn edge_values<E>(frame: &Frame, edge: &ssa::Edge) -> Result<Vec<RuntimeValue>, RuntimeCause<E>> {
    edge.arguments
        .iter()
        .map(|id| get_value(frame, *id))
        .collect()
}
fn binding_address(binding: &Binding) -> Address {
    match binding {
        Binding::Owned(cell) => Address {
            cell: Rc::clone(cell),
            path: Vec::new(),
        },
        Binding::Reference(address) => address.clone(),
    }
}
fn initialize_binding<E>(
    frame: &Frame,
    slot: SlotId,
    value: RuntimeValue,
) -> Result<(), RuntimeCause<E>> {
    let binding = frame
        .slots
        .get(slot.index())
        .and_then(Option::as_ref)
        .ok_or(RuntimeCause::InvalidVerifiedProgram("slot binding"))?;
    write_address(&binding_address(binding), value, true)
}
fn read_address<E>(address: &Address) -> Result<RuntimeValue, RuntimeCause<E>> {
    let value = address
        .cell
        .borrow()
        .clone()
        .ok_or(RuntimeCause::Uninitialized)?;
    address
        .path
        .iter()
        .try_fold(value, |value, index| index_runtime_value_at(&value, *index))
}
fn write_address<E>(
    address: &Address,
    value: RuntimeValue,
    initializing: bool,
) -> Result<(), RuntimeCause<E>> {
    let mut root = address
        .cell
        .try_borrow_mut()
        .map_err(|_| RuntimeCause::Alias)?;
    if address.path.is_empty() {
        if initializing && root.is_some() {
            return Err(RuntimeCause::Alias);
        }
        if !initializing && root.is_none() {
            return Err(RuntimeCause::Uninitialized);
        }
        *root = Some(value);
        return Ok(());
    }
    let root = root.as_mut().ok_or(RuntimeCause::Uninitialized)?;
    set_runtime_value(root, &address.path, value)
}
fn runtime_length(value: &RuntimeValue) -> Option<usize> {
    match value {
        RuntimeValue::Classical(ClassicalValue::Array(values)) => Some(values.len()),
        RuntimeValue::Classical(ClassicalValue::Scalar(value))
            if matches!(value.ty(), ScalarType::Bit(_)) =>
        {
            value.ty().width().map(|width| usize::from(width.value()))
        }
        RuntimeValue::Qubits(qubits) => Some(qubits.len()),
        RuntimeValue::Memory | RuntimeValue::Classical(_) => None,
    }
}

fn type_length(ty: &Type) -> Option<usize> {
    match ty {
        Type::Array { dimensions, .. } => dimensions.first().copied(),
        Type::Scalar(ScalarType::Bit(width)) => Some(usize::from(width.value())),
        Type::Qubit(count) => Some(*count),
        Type::Scalar(_) | Type::Memory | Type::Void => None,
    }
}
fn index_runtime_value<E>(
    value: &RuntimeValue,
    index: &ScalarValue,
) -> Result<RuntimeValue, RuntimeCause<E>> {
    let length = runtime_length(value).ok_or(RuntimeCause::InvalidVerifiedProgram(
        "indexed non-collection value",
    ))?;
    let index = index.to_index(length).map_err(RuntimeCause::Value)?;
    index_runtime_value_at(value, index)
}
fn index_runtime_value_at<E>(
    value: &RuntimeValue,
    index: usize,
) -> Result<RuntimeValue, RuntimeCause<E>> {
    match value {
        RuntimeValue::Classical(ClassicalValue::Array(values)) => values
            .get(index)
            .cloned()
            .map(RuntimeValue::Classical)
            .ok_or(RuntimeCause::Value(ValueError::Index {
                index: i128::try_from(index).unwrap_or(i128::MAX),
                length: values.len(),
            })),
        RuntimeValue::Classical(ClassicalValue::Scalar(value))
            if matches!(value.ty(), ScalarType::Bit(_)) =>
        {
            let bit = value
                .raw_bits()
                .map_err(RuntimeCause::Value)?
                .checked_shr(
                    u32::try_from(index).map_err(|_| RuntimeCause::Value(ValueError::Overflow))?,
                )
                .unwrap_or(0)
                & 1;
            Ok(RuntimeValue::Classical(ClassicalValue::Scalar(
                ScalarValue::bitstring(if bit == 0 { "0" } else { "1" })
                    .map_err(RuntimeCause::Value)?,
            )))
        }
        RuntimeValue::Qubits(qubits) => qubits
            .get(index)
            .copied()
            .map(|qubit| RuntimeValue::Qubits(vec![qubit]))
            .ok_or(RuntimeCause::Value(ValueError::Index {
                index: i128::try_from(index).unwrap_or(i128::MAX),
                length: qubits.len(),
            })),
        RuntimeValue::Memory | RuntimeValue::Classical(_) => Err(
            RuntimeCause::InvalidVerifiedProgram("indexed non-collection"),
        ),
    }
}
fn set_runtime_value<E>(
    root: &mut RuntimeValue,
    path: &[usize],
    value: RuntimeValue,
) -> Result<(), RuntimeCause<E>> {
    let Some((&first, rest)) = path.split_first() else {
        *root = value;
        return Ok(());
    };
    match root {
        RuntimeValue::Classical(ClassicalValue::Array(values)) => {
            let length = values.len();
            let child = values
                .get_mut(first)
                .ok_or(RuntimeCause::Value(ValueError::Index {
                    index: i128::try_from(first).unwrap_or(i128::MAX),
                    length,
                }))?;
            let mut runtime = RuntimeValue::Classical(child.clone());
            set_runtime_value(&mut runtime, rest, value)?;
            let RuntimeValue::Classical(updated) = runtime else {
                return Err(RuntimeCause::InvalidVerifiedProgram(
                    "classical array assignment",
                ));
            };
            *child = updated;
            Ok(())
        }
        RuntimeValue::Classical(ClassicalValue::Scalar(current))
            if rest.is_empty() && matches!(current.ty(), ScalarType::Bit(_)) =>
        {
            let RuntimeValue::Classical(ClassicalValue::Scalar(bit)) = value else {
                return Err(RuntimeCause::InvalidVerifiedProgram("bit assignment type"));
            };
            let width = current
                .ty()
                .width()
                .ok_or(RuntimeCause::InvalidVerifiedProgram("bit width"))?;
            let mask = 1u64
                .checked_shl(
                    u32::try_from(first).map_err(|_| RuntimeCause::Value(ValueError::Overflow))?,
                )
                .ok_or(RuntimeCause::Value(ValueError::Overflow))?;
            let raw = if bit.raw_bits().map_err(RuntimeCause::Value)? == 0 {
                current.raw_bits().map_err(RuntimeCause::Value)? & !mask
            } else {
                current.raw_bits().map_err(RuntimeCause::Value)? | mask
            };
            *current = ScalarValue::bitstring(&format!(
                "{raw:0width$b}",
                width = usize::from(width.value())
            ))
            .map_err(RuntimeCause::Value)?;
            Ok(())
        }
        _ => Err(RuntimeCause::InvalidVerifiedProgram(
            "indexed assignment target",
        )),
    }
}
fn addresses_overlap(left: &Address, right: &Address) -> bool {
    Rc::ptr_eq(&left.cell, &right.cell)
        && (left.path.starts_with(&right.path) || right.path.starts_with(&left.path))
}
fn validate_reference_aliases<E>(
    arguments: &[PreparedArgument],
    additional: &[(Address, bool)],
) -> Result<(), RuntimeCause<E>> {
    let references = arguments
        .iter()
        .filter_map(|argument| match &argument.binding {
            Binding::Reference(address) => Some((address.clone(), argument.mutable)),
            Binding::Owned(_) => None,
        })
        .chain(additional.iter().cloned())
        .collect::<Vec<_>>();
    for (position, (left, left_mutable)) in references.iter().enumerate() {
        for (right, right_mutable) in references.iter().skip(position.saturating_add(1)) {
            if (*left_mutable || *right_mutable) && addresses_overlap(left, right) {
                return Err(RuntimeCause::Alias);
            }
        }
    }
    Ok(())
}

fn validate_selected_aliases<E>(
    bindings: &[(SlotId, Binding)],
    prepared: &[PreparedArgument],
    external: &[(Address, bool)],
    lane: usize,
) -> Result<(), RuntimeCause<E>> {
    let mut references = bindings
        .iter()
        .zip(prepared)
        .filter_map(|((_, binding), argument)| match binding {
            Binding::Reference(address) => Some((address.clone(), argument.mutable)),
            Binding::Owned(_) => None,
        })
        .collect::<Vec<_>>();
    for (address, _) in external {
        references.push((select_qubit_address(address, lane)?, true));
    }
    validate_address_aliases(&references)
}

fn validate_address_aliases<E>(references: &[(Address, bool)]) -> Result<(), RuntimeCause<E>> {
    for (position, (left, left_mutable)) in references.iter().enumerate() {
        for (right, right_mutable) in references.iter().skip(position.saturating_add(1)) {
            if (*left_mutable || *right_mutable) && addresses_overlap(left, right) {
                return Err(RuntimeCause::Alias);
            }
        }
    }
    Ok(())
}

const fn merge_broadcast_width<E>(
    current: usize,
    candidate: usize,
) -> Result<usize, RuntimeCause<E>> {
    if candidate == 0 {
        return Err(RuntimeCause::InvalidVerifiedProgram("empty qubit operand"));
    }
    if current == 1 {
        return Ok(candidate);
    }
    if candidate == 1 || candidate == current {
        Ok(current)
    } else {
        Err(RuntimeCause::InvalidVerifiedProgram(
            "broadcast width mismatch",
        ))
    }
}

fn address_qubit_count<E>(address: &Address) -> Result<usize, RuntimeCause<E>> {
    let RuntimeValue::Qubits(qubits) = read_address(address)? else {
        return Err(RuntimeCause::InvalidVerifiedProgram(
            "gate reference is not quantum",
        ));
    };
    Ok(qubits.len())
}

fn select_qubit_address<E>(address: &Address, lane: usize) -> Result<Address, RuntimeCause<E>> {
    let count = address_qubit_count(address)?;
    let index = if count == 1 { 0 } else { lane };
    if index >= count {
        return Err(RuntimeCause::InvalidVerifiedProgram(
            "broadcast lane is outside operand",
        ));
    }
    let mut selected = address.clone();
    selected
        .path
        .try_reserve_exact(1)
        .map_err(|_| RuntimeCause::Allocation)?;
    selected.path.push(index);
    Ok(selected)
}

fn select_qubit<E>(address: &Address, lane: usize) -> Result<usize, RuntimeCause<E>> {
    let selected = read_address(&select_qubit_address(address, lane)?)?;
    let RuntimeValue::Qubits(qubits) = selected else {
        return Err(RuntimeCause::InvalidVerifiedProgram(
            "selected operand is not quantum",
        ));
    };
    qubits
        .first()
        .copied()
        .ok_or(RuntimeCause::InvalidVerifiedProgram("empty qubit operand"))
}

fn selected_argument<E>(
    argument: &PreparedArgument,
    lane: usize,
) -> Result<Binding, RuntimeCause<E>> {
    if !argument.broadcast_qubit {
        return Ok(argument.binding.clone());
    }
    let Binding::Reference(address) = &argument.binding else {
        return Err(RuntimeCause::InvalidVerifiedProgram(
            "gate qubit parameter is not a reference",
        ));
    };
    select_qubit_address(address, lane).map(Binding::Reference)
}
fn unique_qubits<E>(qubits: &[usize]) -> Result<(), RuntimeCause<E>> {
    let mut unique = BTreeSet::new();
    if qubits.iter().all(|qubit| unique.insert(*qubit)) {
        Ok(())
    } else {
        Err(RuntimeCause::Alias)
    }
}
fn validate_gate<E>(
    gate: GateKind,
    parameters: &[f64],
    targets: &[usize],
    controls: &[QuantumControl],
) -> Result<(), RuntimeCause<E>> {
    let definition = gate.definition();
    if parameters.len() != definition.parameter_count
        || targets.len() != definition.target_count
        || parameters.iter().any(|value| !value.is_finite())
    {
        return Err(RuntimeCause::InvalidVerifiedProgram(
            "gate request signature",
        ));
    }
    let mut qubits = controls
        .iter()
        .map(|control| control.qubit)
        .chain(targets.iter().copied())
        .collect::<Vec<_>>();
    unique_qubits(&qubits)?;
    qubits.clear();
    Ok(())
}
fn scalar_from_i128<E>(ty: ScalarType, value: i128) -> Result<ScalarValue, RuntimeCause<E>> {
    match ty {
        ScalarType::Int(width) => ScalarValue::signed(width, value).map_err(RuntimeCause::Value),
        ScalarType::Uint(width) => ScalarValue::unsigned(
            width,
            u64::try_from(value).map_err(|_| RuntimeCause::Value(ValueError::Overflow))?,
        )
        .map_err(RuntimeCause::Value),
        ScalarType::Bit(_) | ScalarType::Angle(_) | ScalarType::Bool | ScalarType::Float(_) => {
            Err(RuntimeCause::InvalidVerifiedProgram("range scalar type"))
        }
    }
}
fn bit_value<E>(bits: &[bool]) -> Result<ScalarValue, RuntimeCause<E>> {
    let text = bits
        .iter()
        .rev()
        .map(|bit| if *bit { '1' } else { '0' })
        .collect::<String>();
    ScalarValue::bitstring(&text).map_err(RuntimeCause::Value)
}

impl<E> RuntimeCause<E> {
    fn diagnostic_cause(&self, reason: &str) -> crate::DiagnosticCause {
        match self {
            Self::MissingInput(name) | Self::UnexpectedInput(name) => {
                crate::DiagnosticCause::UnknownSymbol { name: name.clone() }
            }
            Self::Assertion(message) => crate::DiagnosticCause::InvalidControlFlow {
                reason: message.clone(),
            },
            Self::StepLimit | Self::FrameLimit | Self::StorageLimit | Self::Allocation => {
                crate::DiagnosticCause::ResourceFailure {
                    reason: reason.to_owned(),
                }
            }
            Self::Unsupported(capability) => crate::DiagnosticCause::UnsupportedCapability {
                capability: (*capability).into(),
            },
            Self::Backend(_) => crate::DiagnosticCause::Lifecycle {
                reason: reason.to_owned(),
            },
            Self::Value(_) | Self::InputType(_) => crate::DiagnosticCause::LanguageFailure {
                kind: if matches!(self, Self::InputType(_)) {
                    crate::LanguageFailureKind::RuntimeInput
                } else {
                    crate::LanguageFailureKind::RuntimeValue
                },
                reason: reason.to_owned(),
            },
            Self::MissingCapture(_) | Self::CaptureType(_) => {
                crate::DiagnosticCause::LanguageFailure {
                    kind: crate::LanguageFailureKind::RuntimeCapture,
                    reason: reason.to_owned(),
                }
            }
            Self::Uninitialized | Self::Alias => crate::DiagnosticCause::LanguageFailure {
                kind: if matches!(self, Self::Alias) {
                    crate::LanguageFailureKind::Alias
                } else {
                    crate::LanguageFailureKind::DefiniteAssignment
                },
                reason: reason.to_owned(),
            },
            Self::InvalidVerifiedProgram(_) => crate::DiagnosticCause::LanguageFailure {
                kind: crate::LanguageFailureKind::InvalidIr,
                reason: reason.to_owned(),
            },
        }
    }
}
