//! Owned diagnostics suitable for frontend, verifier, and runtime boundaries.
use crate::{SourceError, SourceMap, SourceSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Stage {
	Lexing,
	Parsing,
	Admission,
	Lowering,
	Verification,
	Optimization,
	Preparation,
	Execution,
	Export,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Severity {
	Error,
	Warning,
	Note,
}
/// Stable codes are independent of human-readable diagnostic wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiagnosticCode {
	Syntax,
	UnknownSymbol,
	TypeMismatch,
	ArityMismatch,
	ResourceLimit,
	UnsupportedCapability,
	InvalidControlFlow,
	InvalidSource,
	NumericalFailure,
	Lifecycle,
	IncludeFailure,
}
impl DiagnosticCode {
	#[must_use]
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Syntax => "QL0001",
			Self::UnknownSymbol => "QL0002",
			Self::TypeMismatch => "QL0003",
			Self::ArityMismatch => "QL0004",
			Self::ResourceLimit => "QL0005",
			Self::UnsupportedCapability => "QL0006",
			Self::InvalidControlFlow => "QL0007",
			Self::InvalidSource => "QL0008",
			Self::NumericalFailure => "QL0009",
			Self::Lifecycle => "QL0010",
			Self::IncludeFailure => "QL0011",
		}
	}
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ResourceKind {
	SyntaxNesting,
	SyntaxTokens,
	ExportBytes,
	ExportNesting,
	OptimizationWork,
	OptimizationRounds,
	CompileNodes,
	CompileBlocks,
	CompileSlots,
	SourceBytes,
	IncludeDepth,
	IncludeCount,
	IrNodes,
	StorageBytes,
	Qubits,
	PreparationBytes,
	ExecutionSteps,
	CallFrames,
	WorkerMilliseconds,
	WorkerBytes,
	WorkerOutputBytes,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ResourceUsage {
	pub resource: ResourceKind,
	pub requested: u64,
	pub limit: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Entity {
	Symbol(String),
	Gate(String),
	Function(String),
	Value(IrOccurrence),
	Block(IrOccurrence),
	Operation(InstructionOccurrence),
	Include(String),
}
/// Stable identity for an SSA object, including the program that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct IrOccurrence {
	pub program: u64,
	pub index: usize,
}
/// A block-local instruction occurrence with its complete owning identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InstructionOccurrence {
	pub program: u64,
	pub block: usize,
	pub instruction: usize,
}
/// A typed interpreter call-stack entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExecutionContext {
	pub region: IrOccurrence,
	pub block: IrOccurrence,
}
/// Error categories whose source error does not carry more specific structured fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LanguageFailureKind {
	Syntax,
	UnknownSymbol,
	Type,
	DuplicateSymbol,
	DefiniteAssignment,
	Alias,
	InvalidIr,
	RuntimeValue,
	RuntimeInput,
	RuntimeCapture,
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiagnosticCause {
	Syntax {
		expected: Vec<String>,
		found: Option<String>,
	},
	UnknownSymbol {
		name: String,
	},
	TypeMismatch {
		expected: String,
		found: String,
	},
	ArityMismatch {
		expected: u64,
		found: u64,
	},
	ResourceLimit(ResourceUsage),
	ResourceOverflow(ResourceKind),
	ResourceFailure {
		reason: String,
	},
	UnsupportedCapability {
		capability: String,
	},
	InvalidControlFlow {
		reason: String,
	},
	InvalidSource(SourceError),
	NumericalFailure {
		reason: String,
	},
	Lifecycle {
		reason: String,
	},
	IncludeFailure {
		include: String,
		reason: String,
	},
	LanguageFailure {
		kind: LanguageFailureKind,
		reason: String,
	},
}
impl DiagnosticCause {
	#[must_use]
	pub const fn code(&self) -> DiagnosticCode {
		match self {
			Self::Syntax { .. } => DiagnosticCode::Syntax,
			Self::UnknownSymbol { .. } => DiagnosticCode::UnknownSymbol,
			Self::TypeMismatch { .. } => DiagnosticCode::TypeMismatch,
			Self::ArityMismatch { .. } => DiagnosticCode::ArityMismatch,
			Self::ResourceLimit(_) | Self::ResourceOverflow(_) | Self::ResourceFailure { .. } => {
				DiagnosticCode::ResourceLimit
			}
			Self::UnsupportedCapability { .. } => DiagnosticCode::UnsupportedCapability,
			Self::InvalidControlFlow { .. } => DiagnosticCode::InvalidControlFlow,
			Self::InvalidSource(_) => DiagnosticCode::InvalidSource,
			Self::NumericalFailure { .. } => DiagnosticCode::NumericalFailure,
			Self::Lifecycle { .. } => DiagnosticCode::Lifecycle,
			Self::IncludeFailure { .. } => DiagnosticCode::IncludeFailure,
			Self::LanguageFailure { kind, .. } => match kind {
				LanguageFailureKind::Syntax => DiagnosticCode::Syntax,
				LanguageFailureKind::UnknownSymbol => DiagnosticCode::UnknownSymbol,
				LanguageFailureKind::Type
				| LanguageFailureKind::DuplicateSymbol
				| LanguageFailureKind::DefiniteAssignment
				| LanguageFailureKind::Alias
				| LanguageFailureKind::RuntimeValue
				| LanguageFailureKind::RuntimeInput
				| LanguageFailureKind::RuntimeCapture => DiagnosticCode::TypeMismatch,
				LanguageFailureKind::InvalidIr => DiagnosticCode::InvalidControlFlow,
			},
		}
	}
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LabelStyle {
	Primary,
	Secondary,
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Label {
	pub span: SourceSpan,
	pub style: LabelStyle,
	pub message: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TraceKind {
	Definition,
	Call,
	Include,
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraceFrame {
	pub kind: TraceKind,
	pub span: SourceSpan,
	pub message: String,
}
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Provenance {
	pub entity: Option<Entity>,
	pub trace: Vec<TraceFrame>,
	pub execution: Vec<ExecutionContext>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Suggestion {
	pub message: String,
	pub replacement: Option<Replacement>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Replacement {
	pub span: SourceSpan,
	pub text: String,
}

/// Self-contained diagnostic.
///
/// Sources are immutable owned snapshots and rendering
/// performs no filesystem access or global hook installation. Public collection
/// fields permit incremental construction; validate before consuming spans.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Diagnostic {
	pub severity: Severity,
	pub stage: Stage,
	pub cause: DiagnosticCause,
	pub message: String,
	/// Primary occurrence even when only compiler coordinates, not source text, exist.
	pub occurrence: Option<SourceSpan>,
	pub sources: SourceMap,
	pub labels: Vec<Label>,
	pub provenance: Provenance,
	pub notes: Vec<String>,
	pub suggestions: Vec<Suggestion>,
}
impl Diagnostic {
	#[must_use]
	pub fn new(stage: Stage, cause: DiagnosticCause, message: impl Into<String>) -> Self {
		Self {
			severity: Severity::Error,
			stage,
			cause,
			message: message.into(),
			occurrence: None,
			sources: SourceMap::default(),
			labels: Vec::new(),
			provenance: Provenance::default(),
			notes: Vec::new(),
			suggestions: Vec::new(),
		}
	}
	#[must_use]
	pub const fn code(&self) -> DiagnosticCode {
		self.cause.code()
	}
	/// Revalidate all source-bearing fields, including serialized input.
	///
	/// # Errors
	/// Rejects missing snapshots and invalid label, trace, or replacement spans.
	pub fn validate_sources(&self) -> Result<(), SourceError> {
		if let Some(span) = self.occurrence
			&& self.sources.get(span.source()).is_some()
		{
			self.sources.slice(span)?;
		}
		let spans = self
			.labels
			.iter()
			.map(|label| label.span)
			.chain(self.provenance.trace.iter().map(|frame| frame.span))
			.chain(self.suggestions.iter().filter_map(|suggestion| {
				suggestion
					.replacement
					.as_ref()
					.map(|replacement| replacement.span)
			}));
		for span in spans {
			self.sources.slice(span)?;
		}
		Ok(())
	}
}
