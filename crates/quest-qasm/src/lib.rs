//! Explicit-source `OpenQASM` 3.1 import and canonical structured export.
#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
mod export;
pub use export::{ExportLimits, export_syntax};
pub type Result<T> = std::result::Result<T, Box<quest_language::Diagnostic>>;
fn failure(
    stage: quest_language::Stage,
    cause: quest_language::DiagnosticCause,
    message: impl Into<String>,
) -> Box<quest_language::Diagnostic> {
    Box::new(quest_language::Diagnostic::new(stage, cause, message))
}
mod resolve;
pub use resolve::{ImportLimits, IncludeEdge, IncludeResolver, ParsedModule, ResolveError, parse};
mod stdlib;
pub use resolve::NoIncludes;
pub use stdlib::{STANDARD_GATES, StandardLibrary, UPSTREAM_COMMIT, UPSTREAM_STANDARD_GATES};
mod admit;
pub use admit::{ImportedModule, admit_expanded, export, export_typed, import};

mod registry;
