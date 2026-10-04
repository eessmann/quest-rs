//! Full constrained BDM1 incompressible DG foundation.

mod bdm;
pub use bdm::{AssemblyDiagnostics, PeriodicBdm1, PressureRecovery};
pub mod configuration;
pub mod history;
pub mod solve;
pub mod validation;

/// Invalid data or a failed numerical certificate.
#[derive(Debug, thiserror::Error)]
pub enum CfdError {
	#[error(transparent)]
	Qsvt(#[from] quest_qsvt::Error),
	#[error(transparent)]
	Numerical(#[from] quest_numerics::Error),
	#[error("invalid CFD input: {0}")]
	InvalidInput(&'static str),
	#[error("assembly certificate failed: {0}")]
	Assembly(&'static str),
	#[error("case execution is unsupported: {0}")]
	Unsupported(String),
}
pub mod cases;
pub mod cylinder;
pub mod resources;
pub mod simplex;
