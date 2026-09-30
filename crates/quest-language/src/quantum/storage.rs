//! Allocation-identity accounting for immutable payload graphs.
use super::{Error, NumericalOperator, Operation, OracleFragment, Result};
use std::collections::BTreeSet;
#[derive(Default)]
pub struct RetainedStorage {
    pub(super) matrices: BTreeSet<usize>,
    pub(super) oracles: BTreeSet<usize>,
}
impl RetainedStorage {
    pub fn matrix(&mut self, matrix: &NumericalOperator) -> usize {
        if self.matrices.insert(matrix.storage_identity()) {
            matrix.bytes()
        } else {
            0
        }
    }
    /// # Errors
    /// Rejects foreign history identities, exhausted storage limits, or accounting overflow.
    pub fn oracle(&mut self, oracle: &OracleFragment) -> Result<usize> {
        oracle.retained_with(self)
    }
    pub(super) fn operation(&mut self, operation: &Operation) -> Result<usize> {
        Ok(match operation {
            Operation::Numerical { matrix, .. } => self.matrix(matrix),
            Operation::Oracle { fragment, .. } => self.oracle(fragment)?,
            Operation::Channel { kraus, .. } => kraus.iter().try_fold(0usize, |sum, matrix| {
                sum.checked_add(self.matrix(matrix))
                    .ok_or(Error::Budget("channel storage"))
            })?,
            Operation::Conditional { operation, .. } => std::mem::size_of::<Operation>()
                .checked_add(operation.operand_storage_bytes()?)
                .and_then(|n| n.checked_add(self.operation(operation).ok()?))
                .ok_or(Error::Budget("conditional storage"))?,
            _ => 0,
        })
    }
}
