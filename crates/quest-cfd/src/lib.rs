//! Full constrained BDM1 incompressible DG foundation.

mod bdm;
pub use bdm::{AssemblyDiagnostics, PeriodicBdm1, PressureRecovery};
pub mod configuration;
pub mod configuration_diagnostics;
pub mod configuration_flux;
pub mod configuration_weak;
pub mod history;
pub mod history_spectrum;
pub mod solve;
pub mod validation;

/// Invalid data or a failed numerical certificate.
#[derive(Debug, thiserror::Error)]
pub enum CfdError {
	#[error(transparent)]
	Algebra(#[from] mathcore::arithmetic::ArithmeticError),
	#[error(transparent)]
	Arithmetic(#[from] quest_numerics::arithmetic::ArithmeticError),
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
pub mod cavity_observations;
pub mod constructed_resources;
pub mod cylinder;
pub mod cylinder_high_order;
pub mod observations;
pub mod resources;
pub mod simplex;

pub mod burgers;
pub mod carleman;
pub mod lift;
pub mod polynomial;
pub use lift::LiftKind;
pub mod classical_history;

pub mod kdv;

pub mod physical_space;

pub mod stream_history;

pub mod cases_high_order;

#[cfg(feature = "distributed")]
pub mod distributed_history;

pub mod carleman_certificate;

pub mod carleman_recipe;

pub mod kvn_recipe;

pub mod probability_observation;

#[cfg(feature = "distributed")]
pub mod box_kvn_history;

pub mod temporal_encoding;
pub mod temporal_observation;

pub mod physical_observation;

/// Bounded common-ensemble classical history validation.
pub mod paired_history;

mod reference_rk4;
