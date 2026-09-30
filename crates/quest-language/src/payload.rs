//! Immutable numerical payloads with explicit effectful dispatch semantics.
use crate::NumericalOperator;
use std::sync::Arc;
#[derive(Debug, Clone)]
pub enum QuantumPayload {
    /// Ordered wires contain controls first, then matrix targets.
    Matrix {
        matrix: NumericalOperator,
        control_states: Arc<[bool]>,
    },
    /// Ordered wires contain the Kraus targets. This operation needs a density matrix.
    Channel { kraus: Arc<[NumericalOperator]> },
}
impl QuantumPayload {
    #[must_use]
    pub fn num_wires(&self) -> usize {
        match self {
            Self::Matrix {
                matrix,
                control_states,
            } => matrix.num_qubits().saturating_add(control_states.len()),
            Self::Channel { kraus } => kraus.first().map_or(0, NumericalOperator::num_qubits),
        }
    }
    #[must_use]
    pub const fn requires_density_matrix(&self) -> bool {
        matches!(self, Self::Channel { .. })
    }
    #[must_use]
    pub fn retained_bytes(&self) -> Option<usize> {
        match self {
            Self::Matrix {
                matrix,
                control_states,
            } => matrix.bytes().checked_add(control_states.len()),
            Self::Channel { kraus } => kraus
                .iter()
                .try_fold(0usize, |n, m| n.checked_add(m.bytes())),
        }
    }
}

impl QuantumPayload {
    pub fn retained_bytes_with(
        &self,
        storage: &mut crate::quantum::RetainedStorage,
    ) -> Option<usize> {
        match self {
            Self::Matrix {
                matrix,
                control_states,
            } => storage.matrix(matrix).checked_add(control_states.len()),
            Self::Channel { kraus } => kraus
                .iter()
                .try_fold(0usize, |n, m| n.checked_add(storage.matrix(m))),
        }
    }
}
