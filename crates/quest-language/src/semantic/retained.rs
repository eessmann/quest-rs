//! Checked owned allocation accounting; excludes allocator bookkeeping and shared sources.
mod syntax;
use super::SemanticError;
use crate::ssa::{self, InstructionKind as K};
use std::mem::size_of;

pub trait Heap {
    fn heap(&self) -> Result<usize, SemanticError>;
}
pub fn sum(values: impl IntoIterator<Item = usize>) -> Result<usize, SemanticError> {
    values.into_iter().try_fold(0usize, |total, value| {
        total.checked_add(value).ok_or_else(overflow)
    })
}
fn overflow() -> SemanticError {
    let mut error = SemanticError::budget("retained allocation size overflow");
    error.overflow_resource = Some(crate::ResourceKind::StorageBytes);
    error
}
impl<T: Heap> Heap for Vec<T> {
    fn heap(&self) -> Result<usize, SemanticError> {
        let backing = self
            .capacity()
            .checked_mul(size_of::<T>())
            .ok_or_else(overflow)?;
        self.iter()
            .try_fold(backing, |total, item| sum([total, item.heap()?]))
    }
}
impl<T: Heap> Heap for Option<T> {
    fn heap(&self) -> Result<usize, SemanticError> {
        self.as_ref().map_or(Ok(0), Heap::heap)
    }
}
impl<T: Heap> Heap for Box<T> {
    fn heap(&self) -> Result<usize, SemanticError> {
        sum([size_of::<T>(), self.as_ref().heap()?])
    }
}
impl<A: Heap, B: Heap> Heap for (A, B) {
    fn heap(&self) -> Result<usize, SemanticError> {
        sum([self.0.heap()?, self.1.heap()?])
    }
}
impl Heap for String {
    fn heap(&self) -> Result<usize, SemanticError> {
        Ok(self.capacity())
    }
}
macro_rules! inline {
    ($($ty:ty),*) => { $(impl Heap for $ty { fn heap(&self)->Result<usize, SemanticError>{Ok(0)} })* };
}
inline!(
    usize,
    ssa::ValueId,
    ssa::BlockId,
    ssa::SlotId,
    ssa::GateModifier
);
impl Heap for ssa::Type {
    fn heap(&self) -> Result<usize, SemanticError> {
        match self {
            Self::Array { dimensions, .. } => dimensions.heap(),
            _ => Ok(0),
        }
    }
}
macro_rules! fields {
    ($ty:ty: $($field:ident),+) => { impl Heap for $ty { fn heap(&self)->Result<usize, SemanticError> { sum([$(self.$field.heap()?),+]) } } };
}
fields!(ssa::Value: ty);
fields!(ssa::Slot: name,ty);
fields!(ssa::Region: name,parameters,result);
fields!(ssa::Place: indices);
fields!(ssa::Access: place);
fields!(ssa::Instruction: results,kind,accesses);
fields!(ssa::Block: arguments,instructions,terminator,predecessors);
fields!(ssa::Program: regions,blocks,slots);
fields!(ssa::Edge: arguments);
impl Heap for ssa::CallArgument {
    fn heap(&self) -> Result<usize, SemanticError> {
        match self {
            Self::Value(_) => Ok(0),
            Self::Reference { place, .. } => place.heap(),
        }
    }
}
impl Heap for ssa::Terminator {
    fn heap(&self) -> Result<usize, SemanticError> {
        match self {
            Self::Jump(edge) => edge.heap(),
            Self::Branch {
                then_edge,
                else_edge,
                ..
            } => sum([then_edge.heap()?, else_edge.heap()?]),
            Self::Return { .. } | Self::End { .. } => Ok(0),
        }
    }
}
impl Heap for K {
    fn heap(&self) -> Result<usize, SemanticError> {
        match self {
            Self::Array { values } => values.heap(),
            Self::Builtin { name, arguments } => sum([name.heap()?, arguments.heap()?]),
            Self::Capture { ty, .. } => ty.heap(),
            Self::Assert { message, .. } => message.heap(),
            Self::Load { place, .. }
            | Self::Store { place, .. }
            | Self::Measure { place, .. }
            | Self::Reset { place, .. } => place.heap(),
            Self::Call {
                arguments,
                controls,
                modifiers,
                ..
            } => sum([arguments.heap()?, controls.heap()?, modifiers.heap()?]),
            Self::Gate {
                arguments,
                operands,
                modifiers,
                ..
            } => sum([arguments.heap()?, operands.heap()?, modifiers.heap()?]),
            Self::Barrier { places, .. } | Self::Payload { places, .. } => places.heap(),
            Self::Constant(_)
            | Self::Unary { .. }
            | Self::Binary { .. }
            | Self::Cast { .. }
            | Self::GateParameter { .. }
            | Self::Index { .. }
            | Self::RangeAdvance { .. }
            | Self::Input { .. }
            | Self::AllocateArray { .. }
            | Self::Allocate { .. } => Ok(0),
        }
    }
}
