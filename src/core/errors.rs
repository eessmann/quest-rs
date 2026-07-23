use std::fmt;

use crate::core::register::RegisterKind;
use miette::Diagnostic;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum Error {
    /// QuEST/MPI initialization may only be attempted once per process.
    #[error(
        "a QuEST environment has already been created or initialization \
         has already been attempted in this process"
    )]
    #[diagnostic(
        code(quest::environment::already_used),
        help(
            "create one `QuestEnvironment<Active>` and use it to construct \
             all QuEST resources"
        )
    )]
    EnvironmentAlreadyUsed,

    /// The requested qubit count cannot be represented or is otherwise
    /// rejected by the safe Rust API.
    #[error("invalid qubit count: {requested}")]
    #[diagnostic(
        code(quest::register::invalid_qubit_count),
        help("the number of qubits must be positive and representable by QuEST")
    )]
    InvalidQubitCount { requested: usize },

    /// A qubit index was outside the register.
    #[error(
        "qubit index {qubit} is out of range for a register containing \
         {num_qubits} qubits"
    )]
    #[diagnostic(
        code(quest::register::qubit_out_of_range),
        help("use an index in the range 0..{num_qubits}")
    )]
    QubitOutOfRange { qubit: usize, num_qubits: usize },

    /// An operation required a state-vector register but received a density
    /// matrix, or vice versa.
    #[error("operation requires a {required} register, but this register is {actual}")]
    #[diagnostic(code(quest::register::kind_mismatch))]
    RegisterKindMismatch {
        required: RegisterKind,
        actual: RegisterKind,
    },

    /// Computing the state dimension, allocation size, or FFI length overflowed.
    #[error(
        "the state dimension for a {num_qubits}-qubit register cannot be \
         represented on this platform"
    )]
    #[diagnostic(
        code(quest::register::dimension_overflow),
        help("use fewer qubits or avoid materializing the complete state vector")
    )]
    StateDimensionOverflow { num_qubits: usize },

    /// Failure reported by the lower-level QuEST bridge.
    #[error(transparent)]
    #[diagnostic(transparent)]
    Backend(#[from] BackendError),
}

/// A failure reported by `quest-sys`.
#[derive(Debug, Error, Diagnostic)]
#[error("QuEST reported a {kind} error while {operation}")]
#[diagnostic(code(quest::backend::operation_failed))]
pub struct BackendError {
    operation: &'static str,
    kind: BackendErrorKind,

    #[source]
    source: quest_sys::QuestError,
}

impl BackendError {
    pub(crate) fn new(operation: &'static str, source: quest_sys::QuestError) -> Self {
        let kind = BackendErrorKind::from(&source);

        Self {
            operation,
            kind,
            source,
        }
    }

    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    #[must_use]
    pub const fn kind(&self) -> BackendErrorKind {
        self.kind
    }
}

/// Stable, high-level classification of a backend error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BackendErrorKind {
    Validation,
    InvalidInput,
    Lifecycle,
}

impl From<&quest_sys::QuestError> for BackendErrorKind {
    fn from(error: &quest_sys::QuestError) -> Self {
        match error {
            quest_sys::QuestError::Validation(_) => Self::Validation,
            quest_sys::QuestError::InvalidInput(_) => Self::InvalidInput,
            quest_sys::QuestError::Lifecycle(_) => Self::Lifecycle,
        }
    }
}

impl fmt::Display for BackendErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation => formatter.write_str("validation"),
            Self::InvalidInput => formatter.write_str("low-level input"),
            Self::Lifecycle => formatter.write_str("lifecycle"),
        }
    }
}

pub(crate) trait QuestSysResultExt<T> {
    fn with_quest_context(self, operation: &'static str) -> Result<T>;
}

impl<T> QuestSysResultExt<T> for quest_sys::QuestResult<T> {
    fn with_quest_context(self, operation: &'static str) -> Result<T> {
        self.map_err(|source| Error::Backend(BackendError::new(operation, source)))
    }
}
