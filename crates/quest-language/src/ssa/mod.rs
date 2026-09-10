//! Executable SSA with explicit storage and memory-state block arguments.
pub mod optimization;
mod quantum;
mod verify;
use super::semantic::{CompileLimits, SemanticError};
use crate::{
    GateKind, SourceSpan,
    classical::{ScalarType, ScalarValue},
    syntax::{BinaryOperator, UnaryOperator},
};
pub use quantum::{QuantumDag, QuantumNode};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_PROGRAM: AtomicU64 = AtomicU64::new(1);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProgramId(u64);
impl ProgramId {
    pub(crate) fn fresh() -> Result<Self, SemanticError> {
        NEXT_PROGRAM
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map(Self)
            .map_err(|_| SemanticError::budget("program identity exhausted"))
    }
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}
macro_rules! identity {
    ($($name:ident),*) => { $(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name { owner: ProgramId, index: usize }
        impl $name {
            pub(crate) const fn new(owner: ProgramId, index: usize) -> Self { Self { owner, index } }
            #[must_use] pub const fn owner(self) -> ProgramId { self.owner }
            #[must_use] pub const fn index(self) -> usize { self.index }
        }
    )* };
}
// Separate newtypes prevent mixing logical identifier categories.
identity!(SlotId, ValueId, BlockId, RegionId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Scalar(ScalarType),
    Array {
        element: ScalarType,
        dimensions: Vec<usize>,
    },
    Qubit(usize),
    Memory,
    Void,
}
impl Type {
    #[must_use]
    pub fn indexed(&self) -> Option<Self> {
        match self {
            Self::Array {
                element,
                dimensions,
            } => {
                let tail = dimensions.get(1..)?;
                Some(if tail.is_empty() {
                    Self::Scalar(*element)
                } else {
                    Self::Array {
                        element: *element,
                        dimensions: tail.to_vec(),
                    }
                })
            }
            Self::Qubit(_) => Some(Self::Qubit(1)),
            Self::Scalar(ScalarType::Bit(_)) => crate::classical::Width::new(1)
                .ok()
                .map(|width| Self::Scalar(ScalarType::Bit(width))),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    pub id: ValueId,
    pub ty: Type,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interface {
    Local,
    Input,
    Output,
    Parameter,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub id: SlotId,
    pub region: RegionId,
    pub name: String,
    pub ty: Type,
    pub mutable: bool,
    pub interface: Interface,
    pub reference: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub slot: SlotId,
    pub indices: Vec<ValueId>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Pure,
    Read,
    Write,
    Quantum,
    Observe,
    Call,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    Read,
    Write,
    Quantum,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    pub place: Place,
    pub mode: AccessMode,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallArgument {
    Value(ValueId),
    Reference { place: Place, mutable: bool },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateModifier {
    Inverse,
    Control { positive: bool, count: usize },
    Power(ValueId),
}
#[derive(Debug, Clone, PartialEq)]
pub enum InstructionKind {
    Constant(ScalarValue),
    Unary {
        operator: UnaryOperator,
        value: ValueId,
    },
    Binary {
        operator: BinaryOperator,
        left: ValueId,
        right: ValueId,
    },
    Cast {
        value: ValueId,
        ty: ScalarType,
    },
    /// Interpret a real scalar as a gate parameter in radians, separately from source casts.
    GateParameter {
        value: ValueId,
    },
    Array {
        values: Vec<ValueId>,
    },
    Builtin {
        name: String,
        arguments: Vec<ValueId>,
    },
    Index {
        value: ValueId,
        index: ValueId,
    },
    Capture {
        index: usize,
        ty: Type,
    },
    /// Compute a range successor without overflowing the declared integer width.
    /// Results are the successor (or current value at termination) and a bool.
    RangeAdvance {
        current: ValueId,
        step: ValueId,
        end: ValueId,
    },
    Assert {
        condition: ValueId,
        message: String,
        memory: ValueId,
    },
    Input {
        slot: SlotId,
        memory: ValueId,
    },
    AllocateArray {
        slot: SlotId,
        memory: ValueId,
    },
    Allocate {
        slot: SlotId,
        memory: ValueId,
    },
    Load {
        place: Place,
        memory: ValueId,
    },
    Store {
        place: Place,
        value: ValueId,
        memory: ValueId,
        initializing: bool,
    },
    Call {
        region: RegionId,
        arguments: Vec<CallArgument>,
        controls: Vec<Place>,
        modifiers: Vec<GateModifier>,
        memory: ValueId,
    },
    Gate {
        gate: GateKind,
        arguments: Vec<ValueId>,
        operands: Vec<Place>,
        modifiers: Vec<GateModifier>,
        memory: ValueId,
    },
    Measure {
        place: Place,
        memory: ValueId,
    },
    Reset {
        place: Place,
        memory: ValueId,
    },
    Barrier {
        places: Vec<Place>,
        memory: ValueId,
    },
}
impl InstructionKind {
    /// Pure arithmetic can still trap; transformations must preserve those failures.
    #[must_use]
    pub const fn may_trap(&self) -> bool {
        !matches!(self, Self::Constant(_))
    }
    #[must_use]
    pub const fn memory(&self) -> Option<ValueId> {
        match self {
            Self::Assert { memory, .. }
            | Self::Input { memory, .. }
            | Self::AllocateArray { memory, .. }
            | Self::Allocate { memory, .. }
            | Self::Load { memory, .. }
            | Self::Store { memory, .. }
            | Self::Call { memory, .. }
            | Self::Gate { memory, .. }
            | Self::Measure { memory, .. }
            | Self::Reset { memory, .. }
            | Self::Barrier { memory, .. } => Some(*memory),
            _ => None,
        }
    }
    #[must_use]
    pub const fn effect(&self) -> Effect {
        match self {
            Self::Load { .. } => Effect::Read,
            Self::Input { .. }
            | Self::AllocateArray { .. }
            | Self::Allocate { .. }
            | Self::Store { .. } => Effect::Write,
            Self::Gate { .. } | Self::Reset { .. } | Self::Barrier { .. } => Effect::Quantum,
            Self::Measure { .. } | Self::Assert { .. } => Effect::Observe,
            Self::Call { .. } => Effect::Call,
            _ => Effect::Pure,
        }
    }
    #[must_use]
    pub fn accesses(&self) -> Vec<Access> {
        let access = |place: &Place, mode| Access {
            place: place.clone(),
            mode,
        };
        match self {
            Self::Load { place, .. } => vec![access(place, AccessMode::Read)],
            Self::Store { place, .. } => vec![access(place, AccessMode::Write)],
            Self::Gate { operands, .. } => operands
                .iter()
                .map(|place| access(place, AccessMode::Quantum))
                .collect(),
            Self::Measure { place, .. } | Self::Reset { place, .. } => {
                vec![access(place, AccessMode::Quantum)]
            }
            Self::Barrier { places, .. } => places
                .iter()
                .map(|place| access(place, AccessMode::Quantum))
                .collect(),
            Self::Call {
                arguments,
                controls,
                ..
            } => arguments
                .iter()
                .filter_map(|argument| match argument {
                    CallArgument::Reference { place, mutable } => Some(access(
                        place,
                        if *mutable {
                            AccessMode::Write
                        } else {
                            AccessMode::Read
                        },
                    )),
                    CallArgument::Value(_) => None,
                })
                .chain(
                    controls
                        .iter()
                        .map(|place| access(place, AccessMode::Quantum)),
                )
                .collect(),
            Self::Input { slot, .. }
            | Self::AllocateArray { slot, .. }
            | Self::Allocate { slot, .. } => vec![Access {
                place: Place {
                    slot: *slot,
                    indices: Vec::new(),
                },
                mode: AccessMode::Write,
            }],
            _ => Vec::new(),
        }
    }
    #[must_use]
    pub fn operands(&self) -> Vec<ValueId> {
        let mut values = self.memory().into_iter().collect::<Vec<_>>();
        match self {
            Self::Assert { condition, .. } => values.push(*condition),
            Self::RangeAdvance { current, step, end } => values.extend([*current, *step, *end]),
            Self::Unary { value, .. }
            | Self::Cast { value, .. }
            | Self::GateParameter { value }
            | Self::Store { value, .. } => {
                values.push(*value);
            }
            Self::Binary { left, right, .. } => values.extend([*left, *right]),
            Self::Array { values: items }
            | Self::Builtin {
                arguments: items, ..
            } => values.extend(items),
            Self::Index { value, index } => values.extend([*value, *index]),
            Self::Gate {
                arguments,
                modifiers,
                ..
            } => {
                values.extend(arguments);
                append_modifiers(&mut values, modifiers);
            }
            Self::Call {
                arguments,
                modifiers,
                ..
            } => {
                values.extend(arguments.iter().filter_map(|argument| match argument {
                    CallArgument::Value(id) => Some(*id),
                    CallArgument::Reference { .. } => None,
                }));
                append_modifiers(&mut values, modifiers);
            }
            _ => {}
        }
        for access in self.accesses() {
            values.extend(access.place.indices);
        }
        values
    }
}
fn append_modifiers(values: &mut Vec<ValueId>, modifiers: &[GateModifier]) {
    values.extend(modifiers.iter().filter_map(|modifier| match modifier {
        GateModifier::Power(value) => Some(*value),
        _ => None,
    }));
}
#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    pub results: Vec<Value>,
    pub kind: InstructionKind,
    pub effect: Effect,
    pub accesses: Vec<Access>,
    pub span: Option<SourceSpan>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub target: BlockId,
    pub arguments: Vec<ValueId>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    Jump(Edge),
    Branch {
        condition: ValueId,
        then_edge: Edge,
        else_edge: Edge,
    },
    Return {
        value: Option<ValueId>,
        memory: ValueId,
    },
    End {
        memory: ValueId,
    },
}
impl Terminator {
    #[must_use]
    pub fn edges(&self) -> Vec<&Edge> {
        match self {
            Self::Jump(edge) => vec![edge],
            Self::Branch {
                then_edge,
                else_edge,
                ..
            } => vec![then_edge, else_edge],
            _ => Vec::new(),
        }
    }
    #[must_use]
    pub fn operands(&self) -> Vec<ValueId> {
        let mut values = self
            .edges()
            .iter()
            .flat_map(|edge| edge.arguments.iter().copied())
            .collect::<Vec<_>>();
        match self {
            Self::Branch { condition, .. } => values.push(*condition),
            Self::Return { value, memory } => {
                values.extend(value);
                values.push(*memory);
            }
            Self::End { memory } => values.push(*memory),
            Self::Jump(_) => {}
        }
        values
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub id: BlockId,
    pub region: RegionId,
    pub arguments: Vec<Value>,
    pub instructions: Vec<Instruction>,
    pub terminator: Option<Terminator>,
    pub predecessors: Vec<BlockId>,
    pub sealed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub id: RegionId,
    pub name: String,
    pub entry: BlockId,
    pub parameters: Vec<SlotId>,
    pub result: Type,
    pub gate: bool,
}
/// Mutable candidate representation. No executor should accept it directly.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub id: ProgramId,
    pub entry: RegionId,
    pub regions: Vec<Region>,
    pub blocks: Vec<Block>,
    pub slots: Vec<Slot>,
}
impl Program {
    pub(crate) fn validate(&self, limits: CompileLimits) -> Result<(), SemanticError> {
        verify::verify(self, limits)
    }

    /// Independently verify ownership, CFG, dominance, types, memory effects and resources.
    ///
    /// # Errors
    /// Rejects any malformed or unsupported executable representation.
    pub fn verify(self, limits: CompileLimits) -> Result<VerifiedProgram, SemanticError> {
        verify::verify(&self, limits)?;
        Ok(VerifiedProgram { program: self })
    }
}
/// A program whose executable invariants passed independent verification.
///
/// ```compile_fail
/// use quest_language::ssa::{Program, VerifiedProgram};
/// fn forge(program: Program) -> VerifiedProgram { VerifiedProgram { program } }
/// ```
///
/// ```compile_fail
/// use quest_language::ssa::{ValueId, SlotId};
/// fn confuse(value: ValueId) -> SlotId { value }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedProgram {
    program: Program,
}
impl VerifiedProgram {
    /// Owned bytes including nested allocation capacities; excludes allocator bookkeeping.
    ///
    /// # Errors
    /// Returns a resource error if byte accounting overflows.
    pub fn retained_bytes(&self) -> Result<usize, SemanticError> {
        use crate::semantic::retained::Heap as _;
        crate::semantic::retained::sum([std::mem::size_of::<Self>(), self.program.heap()?])
    }

    #[must_use]
    pub fn blocks(&self) -> &[Block] {
        &self.program.blocks
    }
    #[must_use]
    pub fn slots(&self) -> &[Slot] {
        &self.program.slots
    }
    #[must_use]
    pub fn regions(&self) -> &[Region] {
        &self.program.regions
    }
    #[must_use]
    pub const fn program(&self) -> &Program {
        &self.program
    }
    #[must_use]
    pub fn into_unverified(self) -> Program {
        self.program
    }
}

mod edit;
pub use edit::ValueAllocator;
