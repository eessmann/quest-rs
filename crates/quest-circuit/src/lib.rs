#![forbid(unsafe_code)]
//! Native-independent circuits with owned identifiers and consuming compiler stages.
//!
//! Matrix target zero is the least-significant local basis bit. Targets retain
//! their supplied order; fusion explicitly maps any embedded controls. Exact algebra
//! preserves ideal global phase, but does not promise bit-identical simulation.
//!
//! ```
//! use quest_circuit::{Angle, Gate, ProgramBuilder};
//! let mut builder = ProgramBuilder::new(1, 0)?;
//! let theta = builder.parameter("theta")?;
//! builder.gate(Gate::Rx(Angle::parameter(theta)), &[builder.qubit(0)?], &[])?;
//! let plan = builder.finish()?.bind(&[(theta, 0.25)])?.plan()?;
//! assert_eq!(plan.instructions().len(), 1);
//! # Ok::<(), quest_circuit::Error>(())
//! ```

mod oracle;
pub use oracle::{NeedsOracleTolerance, OracleBuilder, OracleFragment, OracleTolerance};
mod linear;
pub use linear::{
    Cnot, LinearOptions, LinearReport, LinearRewrite, LinearSynthesis, synthesize_cnot,
};
mod parity;
pub use parity::{AffinePhaseOperation, ParityOptions, ParityReport, ParitySynthesis, fold_parity};
mod matrix;
mod model;
mod optimize;
mod program;
mod provenance;
pub use provenance::{ExpansionLimits, ProvenanceGraph, ProvenanceId, ProvenanceNode};
mod rational;
mod structured;
mod structured_optimize;
pub use quest_language as language;
pub use quest_qasm as qasm;
pub use rational::BigRational;
pub use structured::*;
pub use structured_optimize::{
    StructuredOccurrence, StructuredQuantumError, StructuredQuantumOptions,
    StructuredQuantumReport, StructuredQuantumRewrite,
};

pub use matrix::*;
pub use model::*;
pub use optimize::*;
pub use program::*;

extern crate self as quest_circuit;
#[cfg(feature = "macros")]
pub use quest_macros::{circuit, circuit_file, legacy_circuit};
/// Unambiguous error name for macro expansion through a runtime facade.
pub type CircuitError = Error;

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

#[cfg(feature = "workers")]
pub use quest_math as certified;
#[cfg(feature = "workers")]
pub use quest_optimizer_client as optimizer;

#[cfg(feature = "workers")]
mod workers;
#[cfg(feature = "workers")]
pub use workers::*;

#[cfg(feature = "workers")]
mod structured_workers;
#[cfg(feature = "workers")]
pub use structured_workers::*;
