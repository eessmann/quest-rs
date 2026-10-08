//! Bounded execution of independently verified SSA.

mod dispatch;
mod frames;
mod prepared;
mod storage;
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
use storage::{
	address_qubit_count, binding_address, classical_matches, edge_values, estimate_storage,
	get_value, index_runtime_value, initialize_binding, merge_broadcast_width, read_address,
	scalar_value, select_qubit, selected_argument, set_value, type_length, unique_qubits,
	validate_gate, validate_reference_aliases, validate_selected_aliases, write_address,
	zero_runtime_value,
};
pub use storage::{output_storage, runtime_storage};

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
					}) {
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
