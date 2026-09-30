#![forbid(unsafe_code)]
//! Native-independent circuits with owned identifiers and consuming compiler stages.
//!
//! Matrix target zero is the least-significant local basis bit. Targets retain
//! their supplied order; fusion explicitly maps any embedded controls. Exact algebra
//! preserves ideal global phase, but does not promise bit-identical simulation.
//!
//! ```
//! use quest_compile::{Angle, Gate, QuantumRegionBuilder};
//! let mut builder = QuantumRegionBuilder::new(1, 0)?;
//! let theta = builder.parameter("theta")?;
//! builder.gate(Gate::Rx(Angle::parameter(theta)?), &[builder.qubit(0)?], &[])?;
//! let plan = builder.finish()?.bind(&[(theta, 0.25)])?.plan()?;
//! assert_eq!(plan.instructions().len(), 1);
//! # Ok::<(), quest_compile::Error>(())
//! ```

mod payload;
pub use payload::QuantumPayload;
mod capture;
pub mod classical;
mod coherent;
mod import;
pub use capture::{AngleCapture, CapturedAngle, capture_angle};
mod builder;
pub use builder::ProgramBuilder;
pub use quest_language::semantic::builder::{
    Array, ArrayFunction, ArrayRef, ArraySubroutine, Bit, Bool, Expr, Float, Function,
    GateDefinition, Int, Local, Modifier, Procedure, Qubit, QubitArrayParameter, QubitParameter,
    RankedArray, RankedArrayRef, Signature, Subroutine, Uint, ValueParameter,
};
pub mod dispatch_recipe;
mod oracle;
pub use quest_language::quantum::{
    NeedsOracleTolerance, OracleBuilder, OracleFragment, OracleTolerance,
};

mod linear;
pub use linear::{
    Cnot, LinearCandidateStrategy, LinearOptions, LinearReport, LinearRewrite, LinearSynthesis,
    synthesize_cnot,
};
mod parity;
pub use parity::{
    AffinePhaseOperation, ParityOptions, ParityReport, ParitySynthesis, fold_parity,
    fold_parity_candidate,
};
mod beam;
mod matrix;
mod model;
mod optimize;
#[cfg(any(feature = "workers", feature = "synthesis"))]
pub use beam::BeamMitmStatus;
pub use beam::{BeamOptions, BeamReport};
mod optimizer_contracts;
mod terminal;
pub use terminal::*;
mod program;
mod provenance;
pub use provenance::{ExpansionLimits, ProvenanceGraph, ProvenanceId, ProvenanceNode};
mod rational;
mod structured;
mod structured_optimize;
mod structured_terminal;
pub use structured_terminal::*;
mod structured_pipeline;
pub use quest_language as language;
pub use quest_qasm as qasm;
pub use rational::BigRational;
pub use structured::*;
pub use structured_optimize::{
    StructuredOccurrence, StructuredQuantumError, StructuredQuantumOptions,
    StructuredQuantumReport, StructuredQuantumRewrite,
};
pub use structured_pipeline::*;

pub use matrix::*;
pub use model::*;
pub use optimize::*;
pub use optimizer_contracts::*;
pub use program::*;

pub use quest_language::quantum::{Error, Result};
#[cfg(any(feature = "workers", feature = "synthesis"))]
impl From<WorkerError> for Error {
    fn from(error: WorkerError) -> Self {
        Self::Worker(Box::new(error))
    }
}
#[cfg(any(feature = "workers", feature = "synthesis"))]
pub use quest_math as certified;
#[cfg(any(feature = "workers", feature = "synthesis"))]
pub use quest_optimizer_client as optimizer;

#[cfg(any(feature = "workers", feature = "synthesis"))]
mod workers;
#[cfg(any(feature = "workers", feature = "synthesis"))]
#[cfg(any(feature = "workers", feature = "synthesis"))]
pub use workers::*;

#[cfg(any(feature = "workers", feature = "synthesis"))]
mod beam_mitm;
#[cfg(any(feature = "workers", feature = "synthesis"))]
#[cfg(any(feature = "workers", feature = "synthesis"))]
pub use beam_mitm::*;
#[cfg(any(feature = "workers", feature = "synthesis"))]
mod beam_approx;
#[cfg(any(feature = "workers", feature = "synthesis"))]
#[cfg(any(feature = "workers", feature = "synthesis"))]
pub use beam_approx::*;

#[cfg(any(feature = "workers", feature = "synthesis"))]
mod structured_workers;
#[cfg(any(feature = "workers", feature = "synthesis"))]
pub use structured_workers::*;

#[cfg(any(feature = "workers", feature = "synthesis"))]
pub use beam_approx::ApproximateBeamPasses;
#[cfg(any(feature = "workers", feature = "synthesis"))]
pub use beam_mitm::MeetInTheMiddlePasses;
pub use linear::LinearPasses;
pub use optimize::ExactPasses;
pub use optimize::NumericalPasses;
pub use oracle::OracleExport;
pub use parity::BoundParityPasses;
pub use parity::ParityPasses;
pub use terminal::TerminalPasses;
#[cfg(any(feature = "workers", feature = "synthesis"))]
pub use workers::RotationSynthesisPasses;
#[cfg(any(feature = "workers", feature = "synthesis"))]
pub use workers::ZxPasses;

/// Compiler extension traits for shared semantic regions.
pub mod prelude {
    #[cfg(any(feature = "workers", feature = "synthesis"))]
    pub use crate::{
        ApproximateBeamPasses, MeetInTheMiddlePasses, RotationSynthesisPasses, ZxPasses,
    };
    pub use crate::{
        BoundParityPasses, ExactPasses, LinearPasses, NumericalPasses, OracleExport, ParityPasses,
        TerminalPasses,
    };
}

#[cfg(any(feature = "workers", feature = "synthesis"))]
mod generator;
#[cfg(any(feature = "workers", feature = "synthesis"))]
pub use generator::*;
