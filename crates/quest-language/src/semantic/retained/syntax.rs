use super::{Heap, sum};
use crate::{
    semantic::SemanticError,
    syntax::{self, ExpressionKind as E, StatementKind as S},
};
impl Heap for syntax::Module {
    fn heap(&self) -> Result<usize, SemanticError> {
        self.statements.heap()
    }
}
impl Heap for syntax::Statement {
    fn heap(&self) -> Result<usize, SemanticError> {
        self.kind.heap()
    }
}
impl Heap for S {
    fn heap(&self) -> Result<usize, SemanticError> {
        match self {
            Self::Include(name) => name.heap(),
            Self::Oracle { name, arity, .. } => sum([name.heap()?, arity.heap()?]),
            Self::Alias { name, value } => sum([name.heap()?, value.heap()?]),
            Self::Qubit { name, size } => sum([name.heap()?, size.heap()?]),
            Self::Declare {
                name,
                ty,
                initializer,
                ..
            } => sum([name.heap()?, ty.heap()?, initializer.heap()?]),
            Self::Assign { target, value, .. } => sum([target.heap()?, value.heap()?]),
            Self::Gate {
                name,
                arguments,
                operands,
                modifiers,
            } => sum([
                name.heap()?,
                arguments.heap()?,
                operands.heap()?,
                modifiers.heap()?,
            ]),
            Self::GateDeclaration {
                name,
                parameters,
                qubits,
                body,
            } => sum([
                name.heap()?,
                parameters.heap()?,
                qubits.heap()?,
                body.heap()?,
            ]),
            Self::Subroutine {
                name,
                parameters,
                result,
                body,
            } => sum([
                name.heap()?,
                parameters.heap()?,
                result.heap()?,
                body.heap()?,
            ]),
            Self::If {
                condition,
                then_body,
                else_body,
            } => sum([condition.heap()?, then_body.heap()?, else_body.heap()?]),
            Self::Switch {
                selector,
                cases,
                default,
            } => sum([selector.heap()?, cases.heap()?, default.heap()?]),
            Self::For {
                name,
                ty,
                iterable,
                body,
            } => sum([name.heap()?, ty.heap()?, iterable.heap()?, body.heap()?]),
            Self::While { condition, body } => sum([condition.heap()?, body.heap()?]),
            Self::Reset(value) | Self::Expression(value) => value.heap(),
            Self::Barrier(values)
            | Self::Payload {
                operands: values, ..
            } => values.heap(),
            Self::Return(value) => value.heap(),
            Self::Break | Self::Continue | Self::End => Ok(0),
        }
    }
}
impl Heap for syntax::Parameter {
    fn heap(&self) -> Result<usize, SemanticError> {
        sum([self.name.heap()?, self.ty.heap()?])
    }
}
impl Heap for syntax::Type {
    fn heap(&self) -> Result<usize, SemanticError> {
        match self {
            Self::Scalar(_, width) | Self::Qubit(width) => width.heap(),
            Self::Array {
                element,
                dimensions,
                ..
            } => sum([element.heap()?, dimensions.heap()?]),
        }
    }
}
impl Heap for syntax::Iterable {
    fn heap(&self) -> Result<usize, SemanticError> {
        match self {
            Self::Range { start, step, end } => sum([start.heap()?, step.heap()?, end.heap()?]),
            Self::Set(values) => values.heap(),
            Self::Expression(value) => value.heap(),
        }
    }
}
impl Heap for syntax::Modifier {
    fn heap(&self) -> Result<usize, SemanticError> {
        match self {
            Self::Inverse | Self::Adjoint => Ok(0),
            Self::Control { count, .. } => count.heap(),
            Self::Power(value) => value.heap(),
        }
    }
}
impl Heap for syntax::Expression {
    fn heap(&self) -> Result<usize, SemanticError> {
        match &self.kind {
            E::Number(text) | E::BitString(text) | E::Name(text) => text.heap(),
            E::Bool(_) | E::Capture(_) => Ok(0),
            E::Unary(_, value) | E::Measure(value) => value.heap(),
            E::Binary(_, left, right) | E::Index(left, right) => sum([left.heap()?, right.heap()?]),
            E::Call(name, arguments) => sum([name.heap()?, arguments.heap()?]),
            E::Cast(ty, value) => sum([ty.heap()?, value.heap()?]),
            E::Array(values) => values.heap(),
        }
    }
}
