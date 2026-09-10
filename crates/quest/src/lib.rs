//! Idiomatic, environment-bound QuEST 4.3 simulation.
//!
//! The pure circuit model is re-exported from [`quest_circuit`]. Numerical
//! operators are immutable faer matrices; QuEST owns simulation storage.
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
mod environment;
mod error;
mod execution;
mod register;
mod values;

pub use environment::{Capabilities, CloseError, Environment, EnvironmentBuilder, ExecutionMode};
pub use error::{Error, Result};
pub use execution::{PreparedProgram, RunResult, SampleResult};
pub use faer;
pub use num_complex::Complex64;
pub use quest_circuit::*;
pub use register::{DensityMatrix, Register, RegisterKind, StateVector};
pub use values::{MemoryBudget, Outcome, Probability, QubitCount, Shots};
