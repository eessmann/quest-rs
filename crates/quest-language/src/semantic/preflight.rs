//! Iterative resource admission before recursive typing and definition cloning.
use super::{CompileLimits, SemanticError};
use crate::{
    ResourceKind,
    syntax::{self, ExpressionKind as E, StatementKind as S},
};

enum Node<'a> {
    Statement(&'a syntax::Statement),
    Expression(&'a syntax::Expression),
    Type(&'a syntax::Type),
}
struct Scan<'a> {
    pending: Vec<(Node<'a>, usize)>,
    visited: usize,
    limits: CompileLimits,
}
pub(super) fn check(module: &syntax::Module, limits: CompileLimits) -> Result<(), SemanticError> {
    let mut scan = Scan {
        pending: Vec::new(),
        visited: 0,
        limits,
    };
    scan.body(&module.statements, 0)?;
    while let Some((node, depth)) = scan.pending.pop() {
        scan.visited = scan
            .visited
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("syntax node count overflow"))?;
        let child = depth
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("syntax depth overflow"))?;
        match node {
            Node::Statement(statement) => scan.statement(statement, child)?,
            Node::Expression(expression) => scan.expression(expression, child)?,
            Node::Type(ty) => scan.ty(ty, child)?,
        }
    }
    Ok(())
}
impl<'a> Scan<'a> {
    fn push(&mut self, node: Node<'a>, depth: usize) -> Result<(), SemanticError> {
        let count = self
            .visited
            .checked_add(self.pending.len())
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| SemanticError::budget("syntax count overflow"))?;
        if count > self.limits.nodes {
            return Err(SemanticError::limit(
                ResourceKind::CompileNodes,
                count,
                self.limits.nodes,
                "syntax node budget exceeded",
            ));
        }
        let nesting = self.limits.call_depth.saturating_mul(4);
        if depth > nesting {
            return Err(SemanticError::limit(
                ResourceKind::SyntaxNesting,
                depth,
                nesting,
                "syntax nesting budget exceeded",
            ));
        }
        self.pending.push((node, depth));
        Ok(())
    }
    fn body(&mut self, body: &'a [syntax::Statement], depth: usize) -> Result<(), SemanticError> {
        for statement in body {
            self.push(Node::Statement(statement), depth)?;
        }
        Ok(())
    }
    fn expressions(
        &mut self,
        expressions: &'a [syntax::Expression],
        depth: usize,
    ) -> Result<(), SemanticError> {
        for expression in expressions {
            self.push(Node::Expression(expression), depth)?;
        }
        Ok(())
    }
    fn optional(
        &mut self,
        value: Option<&'a syntax::Expression>,
        depth: usize,
    ) -> Result<(), SemanticError> {
        if let Some(value) = value {
            self.push(Node::Expression(value), depth)?;
        }
        Ok(())
    }
    fn statement(
        &mut self,
        statement: &'a syntax::Statement,
        depth: usize,
    ) -> Result<(), SemanticError> {
        match &statement.kind {
            S::Qubit { size, .. } => self.optional(size.as_ref(), depth)?,
            S::Declare {
                ty, initializer, ..
            } => {
                self.push(Node::Type(ty), depth)?;
                self.optional(initializer.as_ref(), depth)?;
            }
            S::Alias { value, .. } | S::Reset(value) | S::Expression(value) => {
                self.push(Node::Expression(value), depth)?;
            }
            S::Assign { target, value, .. } => {
                self.push(Node::Expression(target), depth)?;
                self.push(Node::Expression(value), depth)?;
            }
            S::Gate {
                arguments,
                operands,
                modifiers,
                ..
            } => {
                self.expressions(arguments, depth)?;
                self.expressions(operands, depth)?;
                for modifier in modifiers {
                    match modifier {
                        syntax::Modifier::Control { count, .. } => {
                            self.optional(count.as_ref(), depth)?;
                        }
                        syntax::Modifier::Power(value) => {
                            self.push(Node::Expression(value), depth)?;
                        }
                        syntax::Modifier::Inverse => {}
                    }
                }
            }
            S::GateDeclaration { body, .. } => self.body(body, depth)?,
            S::Subroutine {
                parameters,
                result,
                body,
                ..
            } => {
                for parameter in parameters {
                    self.push(Node::Type(&parameter.ty), depth)?;
                }
                if let Some(ty) = result {
                    self.push(Node::Type(ty), depth)?;
                }
                self.body(body, depth)?;
            }
            S::Barrier(values) => self.expressions(values, depth)?,
            S::Return(value) => self.optional(value.as_ref(), depth)?,
            S::Include(_) | S::Break | S::Continue | S::End => {}
            _ => self.control(statement, depth)?,
        }
        Ok(())
    }
    fn control(
        &mut self,
        statement: &'a syntax::Statement,
        depth: usize,
    ) -> Result<(), SemanticError> {
        match &statement.kind {
            S::If {
                condition,
                then_body,
                else_body,
            } => {
                self.push(Node::Expression(condition), depth)?;
                self.body(then_body, depth)?;
                self.body(else_body, depth)?;
            }
            S::While { condition, body } => {
                self.push(Node::Expression(condition), depth)?;
                self.body(body, depth)?;
            }
            S::Switch {
                selector,
                cases,
                default,
            } => {
                self.push(Node::Expression(selector), depth)?;
                for (labels, body) in cases {
                    self.expressions(labels, depth)?;
                    self.body(body, depth)?;
                }
                self.body(default, depth)?;
            }
            S::For {
                ty, iterable, body, ..
            } => {
                self.push(Node::Type(ty), depth)?;
                match iterable {
                    syntax::Iterable::Range { start, step, end } => {
                        self.push(Node::Expression(start), depth)?;
                        self.optional(step.as_ref(), depth)?;
                        self.push(Node::Expression(end), depth)?;
                    }
                    syntax::Iterable::Set(values) => self.expressions(values, depth)?,
                    syntax::Iterable::Expression(value) => {
                        self.push(Node::Expression(value), depth)?;
                    }
                }
                self.body(body, depth)?;
            }
            _ => return Err(SemanticError::invalid("unexpected control in syntax scan")),
        }
        Ok(())
    }
    fn expression(
        &mut self,
        expression: &'a syntax::Expression,
        depth: usize,
    ) -> Result<(), SemanticError> {
        match &expression.kind {
            E::Unary(_, value) | E::Measure(value) => self.push(Node::Expression(value), depth)?,
            E::Binary(_, left, right) | E::Index(left, right) => {
                self.push(Node::Expression(left), depth)?;
                self.push(Node::Expression(right), depth)?;
            }
            E::Call(_, arguments) | E::Array(arguments) => self.expressions(arguments, depth)?,
            E::Cast(ty, value) => {
                self.push(Node::Type(ty), depth)?;
                self.push(Node::Expression(value), depth)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn ty(&mut self, ty: &'a syntax::Type, depth: usize) -> Result<(), SemanticError> {
        match ty {
            syntax::Type::Scalar(_, width) | syntax::Type::Qubit(width) => {
                self.optional(width.as_deref(), depth)?;
            }
            syntax::Type::Array {
                element,
                dimensions,
                ..
            } => {
                self.push(Node::Type(element), depth)?;
                self.expressions(dimensions, depth)?;
            }
        }
        Ok(())
    }
}
