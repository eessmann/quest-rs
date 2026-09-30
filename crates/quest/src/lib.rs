//! Idiomatic, environment-bound `QuEST` 4.3 simulation.
//!
//! The pure circuit model is re-exported from [`quest_circuit`]. Numerical
//! operators are immutable faer matrices; `QuEST` owns simulation storage.
#![forbid(unsafe_code)]
#![cfg_attr(
    not(all(feature = "mpi", quest_native_mpi)),
    doc = "The collective API requires both the optional MPI feature and native MPI/SUBCOMM support.
```compile_fail
use quest::collective::CollectiveEnvironment;
```"
)]
#![doc = include_str!("../README.md")]
#[cfg(all(feature = "mpi", quest_native_mpi))]
pub mod collective;
#[cfg(any(test, all(feature = "mpi", quest_native_mpi)))]
mod collective_admission;
#[cfg(all(feature = "mpi", quest_native_mpi))]
mod collective_payload;
mod environment;
mod error;
mod execution;
mod oracle_execution;
mod output_storage;
mod payload_execution;
#[cfg(feature = "qsvt")]
pub mod qsvt;
mod register;
mod structured_execution;
mod values;
pub use quest_circuit::language::vm::{ClassicalValue, InterpreterLimits, RunInputs, RunOutput};
pub use structured_execution::{PreparedProgram, SampleResult};

pub use environment::{
    Capabilities, Environment, EnvironmentBuilder, EnvironmentView, ExecutionMode,
};
pub use error::{Error, Result, StructuredExecutionError};
pub use faer;
pub use num_complex::Complex64;
pub use quest_circuit::*;
pub use register::{DensityMatrix, Register, RegisterDeployment, RegisterKind, StateVector};
pub use values::{MemoryBudget, Outcome, Probability, QubitCount, Shots};
