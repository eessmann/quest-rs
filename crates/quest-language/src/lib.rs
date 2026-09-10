//! Native-independent language semantics, immutable sources, and owned diagnostics.
#![forbid(unsafe_code)]
mod gates;
mod source;
pub use gates::*;
pub use source::*;
mod diagnostic;
pub use diagnostic::*;
#[cfg(feature = "codespan-reporting")]
mod render;
#[cfg(feature = "codespan-reporting")]
pub use render::*;

/// Shared frontend grammar and source/token adapters.
pub mod syntax;

/// Checked fixed-width classical values.
pub mod classical;

/// Typed structured semantic admission.
pub mod semantic;
/// Independently verified classical SSA and quantum effects.
pub mod ssa;

/// Bounded interpreter for independently verified SSA.
pub mod vm;
