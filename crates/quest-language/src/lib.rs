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

/// Exact mathematical angle semantics shared by every frontend.
pub mod angle;
/// Project-wide exact rational storage and checked conversion.
pub mod rational;
pub use angle::{Angle, BoundAngleTarget, ParameterId};

/// Immutable matrix storage and numerical admission.
pub mod matrix;
pub use matrix::{MatrixPolicy, NumericalOperator};

/// Immutable quantum effect payloads retained in checked programs.
pub mod payload;
pub use payload::QuantumPayload;

/// Shared finite-region semantics and checked capability data.
pub mod quantum;
