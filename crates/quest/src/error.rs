use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid qubit count {0}")]
    QubitCount(usize),
    #[error("index {index} is outside 0..{bound}")]
    Index { index: usize, bound: usize },
    #[error("dimension or allocation size overflow")]
    Overflow,
    #[error("memory budget exceeded: requested {requested} bytes, available {available}")]
    Budget { requested: usize, available: usize },
    #[error("could not reserve host memory")]
    Allocation,
    #[error("invalid value: {0}")]
    Value(&'static str),
    #[error("unsupported capability: {0}")]
    Unsupported(&'static str),
    #[error("native configuration changed since preparation")]
    ConfigurationChanged,
    #[error("register shape or kind does not match the executable")]
    RegisterMismatch,
    #[error(
        "execution failed at instruction {instruction}, after {completed} completed instructions: {source}"
    )]
    Execution {
        instruction: usize,
        completed: usize,
        #[source]
        source: Box<Error>,
    },
    #[error("QuEST failed while {operation}: {source}")]
    Backend {
        operation: &'static str,
        #[source]
        source: quest_sys::QuestError,
    },
    #[error(transparent)]
    Circuit(#[from] quest_circuit::Error),
}

pub(crate) trait BackendResult<T> {
    fn context(self, operation: &'static str) -> Result<T>;
}
impl<T> BackendResult<T> for quest_sys::QuestResult<T> {
    fn context(self, operation: &'static str) -> Result<T> {
        self.map_err(|source| Error::Backend { operation, source })
    }
}
