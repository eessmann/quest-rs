//! Fresh program-owned identities for one unverified transformation transaction.
use super::{Program, ProgramId, Type, Value, ValueId};
use crate::semantic::{CompileLimits, SemanticError};

/// An edit-local identity allocator. Its outputs are unverified values.
///
/// Use one allocator per candidate transaction and publish only through `Program::verify`.
/// Two transactions derived from the same source must not be merged without verification.
#[derive(Debug)]
pub struct ValueAllocator {
    owner: ProgramId,
    next: usize,
    remaining: usize,
    limit: usize,
}
impl ValueAllocator {
    /// Allocate a fresh identity within this edit, without publishing a definition.
    /// # Errors
    /// Rejects identity overflow and exhaustion of the value budget.
    pub fn allocate(&mut self, ty: Type) -> Result<Value, SemanticError> {
        if self.remaining == 0 {
            return Err(SemanticError::limit(
                crate::ResourceKind::CompileNodes,
                self.limit.saturating_add(1),
                self.limit,
                "SSA edit value budget exhausted",
            ));
        }
        let next = self
            .next
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("SSA edit identity overflow"))?;
        let id = ValueId::new(self.owner, self.next);
        self.next = next;
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or_else(|| SemanticError::budget("SSA edit value budget"))?;
        Ok(Value { id, ty })
    }
}
impl Program {
    /// Start a bounded identity allocation transaction from a verified candidate.
    /// The candidate stays unverified after edits; IDs carry no dominance/type proof.
    /// # Errors
    /// Rejects malformed source IR, budgets and identity overflow.
    pub fn value_allocator(&self, limits: CompileLimits) -> Result<ValueAllocator, SemanticError> {
        self.validate(limits)?;
        let mut next = 0usize;
        let mut count = 0usize;
        for value in self.blocks.iter().flat_map(|block| {
            block.arguments.iter().chain(
                block
                    .instructions
                    .iter()
                    .flat_map(|instruction| &instruction.results),
            )
        }) {
            next = next.max(
                value
                    .id
                    .index()
                    .checked_add(1)
                    .ok_or_else(|| SemanticError::budget("SSA edit identity overflow"))?,
            );
            count = count
                .checked_add(1)
                .ok_or_else(|| SemanticError::budget("SSA value count overflow"))?;
        }
        Ok(ValueAllocator {
            owner: self.id,
            next,
            remaining: limits
                .nodes
                .checked_sub(count)
                .ok_or_else(|| SemanticError::budget("SSA edit value budget"))?,
            limit: limits.nodes,
        })
    }
}
