//! Compiler-owned failures preserve concrete stage and candidate error types.
#[derive(Debug, thiserror::Error)]
pub enum CompilerError {
	#[error(transparent)]
	Semantic(#[from] quest_language::quantum::Error),
	#[error("structured optimization: {0}")]
	Structured(#[from] crate::StructuredTerminalError),
	#[cfg(feature = "workers")]
	#[error("worker optimization: {0}")]
	Worker(#[from] crate::WorkerError),
}
