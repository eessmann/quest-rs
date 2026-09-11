#![forbid(unsafe_code)]
//! Reusable binary64 kernels and checked finite real intervals.
//!
//! Production kernels never dispatch to arbitrary precision. FFTs use a fixed
//! caller-selected backend; roundoff is not a certificate of an exact result.
//! The default build and execution policy are sequential and scalar.

mod fft;
mod interval;
pub mod observer;
mod policy;

pub use fft::{ConvolutionWorkspace, FftBackend, FftDirection, FftWorkspace, Normalization};
pub use interval::Interval;
pub use num_complex::Complex64;
pub use policy::{Error, ExecutionPolicy, Limits, ResourceUsage, Result};
