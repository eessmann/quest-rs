#[cfg(feature = "codespan-reporting")]
use super::SourceSpan;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
	#[error("identifier does not belong to this program, or is out of bounds")]
	InvalidId,
	#[error("duplicate operand or overlapping control and target")]
	DuplicateOperand,
	#[error("operation expects {expected} targets, received {actual}")]
	Arity { expected: usize, actual: usize },
	#[error("gate expects {expected} parameters, received {actual}")]
	ParameterArity { expected: usize, actual: usize },
	#[error("invalid finite numerical value")]
	NonFinite,
	#[error("rational angle denominator must be nonzero")]
	ZeroDenominator,
	#[error("exact symbolic angle rejected: {0}")]
	Symbolic(#[from] quest_symbolic::Error),
	#[error("resource budget exceeded: {0}")]
	Budget(&'static str),
	#[error("matrix must be a nonempty square with power-of-two dimension")]
	MatrixShape,
	#[error("matrix dimension mismatch")]
	MatrixDimension,
	#[error("unitarity residual {residual} exceeds tolerance {tolerance}")]
	Unitarity { residual: f64, tolerance: f64 },
	#[error("channel completeness residual {residual} exceeds tolerance {tolerance}")]
	ChannelCompleteness { residual: f64, tolerance: f64 },
	#[error("parameter name is empty or already declared")]
	ParameterName,
	#[error("parameter bindings must be complete, unique, finite, and program-owned")]
	Binding,
	#[error("dependency graph contains a cycle")]
	Cycle,
	#[error("program contains an effect or numerical operator without exact unitary semantics")]
	NotUnitary,
	#[error("native index is not representable")]
	NativeIndex,
	#[error("unsupported capability: {0}")]
	Unsupported(&'static str),
	#[error("source range end precedes its start")]
	SourceRange,
}

#[cfg(feature = "codespan-reporting")]
impl Error {
	/// Render this structured error at a frontend-owned source range. Rendering
	/// borrows the original text; invalid byte offsets and UTF-8 boundaries are
	/// rejected before invoking the optional presentation layer.
	///
	/// # Errors
	/// Rejects source bounds or UTF-8 boundaries that cannot be rendered.
	pub fn render_source(
		&self,
		span: &SourceSpan,
		text: &str,
	) -> std::result::Result<String, codespan_reporting::files::Error> {
		use codespan_reporting::{
			diagnostic::{Diagnostic, Label},
			files, term,
		};
		let range = span.range();
		for index in [range.start, range.end] {
			if index > text.len() {
				return Err(files::Error::IndexTooLarge {
					given: index,
					max: text.len(),
				});
			}
			if !text.is_char_boundary(index) {
				return Err(files::Error::InvalidCharBoundary { given: index });
			}
		}
		let file = files::SimpleFile::new(span.source(), text);
		let diagnostic = Diagnostic::error()
			.with_message(self.to_string())
			.with_labels(vec![Label::primary((), range)]);
		term::emit_into_string(&term::Config::default(), &file, &diagnostic)
	}
}

impl From<crate::angle::Error> for Error {
	fn from(error: crate::angle::Error) -> Self {
		use crate::angle::Error as A;
		match error {
			A::InvalidId => Self::InvalidId,
			A::NonFinite => Self::NonFinite,
			A::ZeroDenominator => Self::ZeroDenominator,
			A::Symbolic(error) => Self::Symbolic(error),
			A::Budget(reason) => Self::Budget(reason),
			A::Binding => Self::Binding,
			A::Unsupported(reason) => Self::Unsupported(reason),
		}
	}
}
impl From<crate::matrix::Error> for Error {
	fn from(error: crate::matrix::Error) -> Self {
		use crate::matrix::Error as M;
		match error {
			M::NonFinite => Self::NonFinite,
			M::Budget(reason) => Self::Budget(reason),
			M::MatrixShape => Self::MatrixShape,
			M::MatrixDimension => Self::MatrixDimension,
			M::Unitarity {
				residual,
				tolerance,
			} => Self::Unitarity {
				residual,
				tolerance,
			},
			M::ChannelCompleteness {
				residual,
				tolerance,
			} => Self::ChannelCompleteness {
				residual,
				tolerance,
			},
		}
	}
}
