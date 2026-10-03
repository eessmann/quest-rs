use std::ops::Deref;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// A typed interpreter failure paired with its immutable diagnostic snapshot.
#[derive(Debug, Error)]
#[error("{runtime}")]
pub struct StructuredExecutionError {
	runtime: Box<quest_compile::language::vm::RuntimeError<Error>>,
	diagnostic: Box<quest_compile::language::Diagnostic>,
}
impl StructuredExecutionError {
	pub(crate) fn new(
		runtime: quest_compile::language::vm::RuntimeError<Error>,
		diagnostic: quest_compile::language::Diagnostic,
	) -> Self {
		Self {
			runtime: Box::new(runtime),
			diagnostic: Box::new(diagnostic),
		}
	}
	#[must_use]
	pub const fn runtime(&self) -> &quest_compile::language::vm::RuntimeError<Error> {
		&self.runtime
	}
	#[must_use]
	pub const fn diagnostic(&self) -> &quest_compile::language::Diagnostic {
		&self.diagnostic
	}
}
impl Deref for StructuredExecutionError {
	type Target = quest_compile::language::vm::RuntimeError<Error>;
	fn deref(&self) -> &Self::Target {
		&self.runtime
	}
}

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
		source: Box<Self>,
	},
	#[error("QuEST failed while {operation}: {source}")]
	Backend {
		operation: &'static str,
		#[source]
		source: quest_sys::QuestError,
	},
	#[error(transparent)]
	Circuit(#[from] quest_compile::Error),
	#[error(transparent)]
	Language(Box<quest_compile::LanguageError>),
	#[error(transparent)]
	StructuredExecution(Box<StructuredExecutionError>),
}
impl From<quest_compile::LanguageError> for Error {
	fn from(error: quest_compile::LanguageError) -> Self {
		Self::Language(Box::new(error))
	}
}

impl Error {
	/// Borrow the owned diagnostic attached at a language or structured runtime boundary.
	#[must_use]
	pub fn diagnostic(&self) -> Option<&quest_compile::language::Diagnostic> {
		match self {
			Self::Language(error) => error.diagnostic(),
			Self::StructuredExecution(error) => Some(error.diagnostic()),
			_ => None,
		}
	}

	/// Produce an owned, serializable diagnostic for every facade error.
	#[must_use]
	pub fn report(&self) -> quest_compile::language::Diagnostic {
		use quest_compile::language::{
			Diagnostic, DiagnosticCause, LanguageFailureKind, ResourceKind, ResourceUsage, Stage,
		};
		if let Some(diagnostic) = self.diagnostic() {
			return diagnostic.clone();
		}
		let (stage, cause) = match self {
			Self::QubitCount(_) | Self::Value(_) => (
				Stage::Preparation,
				DiagnosticCause::LanguageFailure {
					kind: LanguageFailureKind::RuntimeValue,
					reason: self.to_string(),
				},
			),
			Self::Index { .. } => (
				Stage::Execution,
				DiagnosticCause::LanguageFailure {
					kind: LanguageFailureKind::RuntimeValue,
					reason: self.to_string(),
				},
			),
			Self::Budget {
				requested,
				available,
			} => (
				Stage::Preparation,
				match (u64::try_from(*requested), u64::try_from(*available)) {
					(Ok(requested), Ok(limit)) => DiagnosticCause::ResourceLimit(ResourceUsage {
						resource: ResourceKind::PreparationBytes,
						requested,
						limit,
					}),
					_ => DiagnosticCause::ResourceOverflow(ResourceKind::PreparationBytes),
				},
			),
			Self::Overflow | Self::Allocation => (
				Stage::Preparation,
				DiagnosticCause::ResourceFailure {
					reason: self.to_string(),
				},
			),
			Self::Unsupported(capability) => (
				Stage::Preparation,
				DiagnosticCause::UnsupportedCapability {
					capability: (*capability).into(),
				},
			),
			Self::ConfigurationChanged
			| Self::RegisterMismatch
			| Self::Backend { .. }
			| Self::Execution { .. } => (
				Stage::Execution,
				DiagnosticCause::Lifecycle {
					reason: self.to_string(),
				},
			),
			Self::Circuit(_) => (
				Stage::Preparation,
				DiagnosticCause::LanguageFailure {
					kind: LanguageFailureKind::InvalidIr,
					reason: self.to_string(),
				},
			),
			Self::Language(error) => return error.report(Stage::Preparation),
			Self::StructuredExecution(error) => return error.diagnostic().clone(),
		};
		let mut diagnostic = Diagnostic::new(stage, cause, self.to_string());
		if let Self::Execution {
			instruction,
			completed,
			..
		} = self
		{
			diagnostic.notes.push(format!(
				"instruction {instruction} failed after {completed} completed instructions"
			));
		}
		diagnostic
	}
}

pub trait BackendResult<T> {
	fn context(self, operation: &'static str) -> Result<T>;
}
impl<T> BackendResult<T> for quest_sys::QuestResult<T> {
	fn context(self, operation: &'static str) -> Result<T> {
		self.map_err(|source| Error::Backend { operation, source })
	}
}

impl From<quest_compile::language::angle::Error> for Error {
	fn from(value: quest_compile::language::angle::Error) -> Self {
		Self::Circuit(value.into())
	}
}

impl From<quest_compile::language::matrix::Error> for Error {
	fn from(value: quest_compile::language::matrix::Error) -> Self {
		Self::Circuit(value.into())
	}
}
