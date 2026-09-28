//! Shared construction nodes with precomputed bounds for syntax materialization.
use super::{CompileLimits, Expression, SemanticError, expression};
use crate::{
    semantic::retained::Heap as _,
    syntax::{self, ExpressionKind as E},
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(super) struct SharedExpression {
    node: Arc<Node>,
    nodes: usize,
    bytes: usize,
    retained: usize,
    depth: usize,
}
#[derive(Debug)]
enum Node {
    Leaf(Expression),
    Binary(syntax::BinaryOperator, SharedExpression, SharedExpression),
    Cast(syntax::Type, SharedExpression),
}
impl SharedExpression {
    pub fn leaf(value: Expression, limits: CompileLimits) -> Result<Self, SemanticError> {
        let bytes = size_of::<Expression>()
            .checked_add(value.heap()?)
            .ok_or_else(|| SemanticError::budget("builder expression storage overflow"))?;
        let retained = Self::retained_node_bytes()
            .checked_add(value.heap()?)
            .ok_or_else(|| SemanticError::budget("builder retained storage overflow"))?;
        Self::check(1, Self::working_bytes(bytes, retained)?, 1, limits)?;
        Ok(Self {
            node: Arc::new(Node::Leaf(value)),
            nodes: 1,
            bytes,
            retained,
            depth: 1,
        })
    }
    // Two reference counts and one conservative alignment allowance cover Arc's
    // allocation header; allocator bookkeeping is outside this capacity budget.
    const fn retained_node_bytes() -> usize {
        const { size_of::<Node>() + size_of::<[usize; 2]>() + align_of::<Node>() }
    }
    fn working_bytes(bytes: usize, retained: usize) -> Result<usize, SemanticError> {
        bytes
            .checked_add(retained)
            .ok_or_else(|| SemanticError::budget("builder working storage overflow"))
    }
    pub fn binary(
        &self,
        rhs: &Self,
        op: syntax::BinaryOperator,
        limits: CompileLimits,
    ) -> Result<Self, SemanticError> {
        let nodes = self
            .nodes
            .checked_add(rhs.nodes)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| SemanticError::budget("builder expression node overflow"))?;
        let bytes = self
            .bytes
            .checked_add(rhs.bytes)
            .and_then(|n| n.checked_add(size_of::<Expression>()))
            .ok_or_else(|| SemanticError::budget("builder expression storage overflow"))?;
        let depth = self
            .depth
            .max(rhs.depth)
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("builder expression depth overflow"))?;
        // Sum is conservative when operands share descendants. It bounds both
        // retained construction nodes and their simultaneously materialized AST.
        let retained = self
            .retained
            .checked_add(rhs.retained)
            .and_then(|n| n.checked_add(Self::retained_node_bytes()))
            .ok_or_else(|| SemanticError::budget("builder retained storage overflow"))?;
        Self::check(nodes, Self::working_bytes(bytes, retained)?, depth, limits)?;
        Ok(Self {
            node: Arc::new(Node::Binary(op, self.clone(), rhs.clone())),
            nodes,
            bytes,
            retained,
            depth,
        })
    }
    pub fn cast(&self, ty: syntax::Type, limits: CompileLimits) -> Result<Self, SemanticError> {
        let syntax::Type::Scalar(_, width) = &ty else {
            return Err(SemanticError::invalid(
                "typed builder cast requires a scalar type",
            ));
        };
        let type_nodes = 1usize.saturating_add(usize::from(width.is_some()));
        let nodes = self
            .nodes
            .checked_add(type_nodes)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| SemanticError::budget("builder expression node overflow"))?;
        let bytes = self
            .bytes
            .checked_add(ty.heap()?)
            .and_then(|n| n.checked_add(size_of::<Expression>()))
            .ok_or_else(|| SemanticError::budget("builder expression storage overflow"))?;
        let depth = self
            .depth
            .max(type_nodes)
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("builder expression depth overflow"))?;
        let retained = self
            .retained
            .checked_add(ty.heap()?)
            .and_then(|n| n.checked_add(Self::retained_node_bytes()))
            .ok_or_else(|| SemanticError::budget("builder retained storage overflow"))?;
        Self::check(nodes, Self::working_bytes(bytes, retained)?, depth, limits)?;
        Ok(Self {
            node: Arc::new(Node::Cast(ty, self.clone())),
            nodes,
            bytes,
            retained,
            depth,
        })
    }
    fn check(
        nodes: usize,
        bytes: usize,
        depth: usize,
        limits: CompileLimits,
    ) -> Result<(), SemanticError> {
        for (kind, actual, limit) in [
            (crate::ResourceKind::CompileNodes, nodes, limits.nodes),
            (
                crate::ResourceKind::StorageBytes,
                bytes,
                limits.storage_bytes,
            ),
            (
                crate::ResourceKind::SyntaxNesting,
                depth,
                limits.call_depth.saturating_mul(4).min(256),
            ),
        ] {
            if actual > limit {
                return Err(SemanticError::limit(
                    kind,
                    actual,
                    limit,
                    "typed expression exceeds construction budget",
                ));
            }
        }
        Ok(())
    }
    pub fn materialize(&self, limits: CompileLimits) -> Result<Expression, SemanticError> {
        Self::check(
            self.nodes,
            Self::working_bytes(self.bytes, self.retained)?,
            self.depth,
            limits,
        )?;
        Ok(self.expand())
    }
    fn expand(&self) -> Expression {
        match self.node.as_ref() {
            Node::Leaf(value) => value.clone(),
            Node::Binary(op, left, right) => expression(E::Binary(
                *op,
                Box::new(left.expand()),
                Box::new(right.expand()),
            )),
            Node::Cast(ty, value) => expression(E::Cast(ty.clone(), Box::new(value.expand()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[gtest]
    fn shallow_nodes_admit_retained_arc_storage_before_allocation() -> googletest::Result<()> {
        let limits = CompileLimits {
            storage_bytes: size_of::<Expression>(),
            ..CompileLimits::default()
        };
        verify_that!(
            SharedExpression::leaf(expression(E::Bool(true)), limits),
            err(anything())
        )
    }
}
