#![forbid(unsafe_code)]
//! Reusable numerical kernels and statically selected arithmetic policies.
//!
//! FFT kernels retain binary64 arithmetic. FFTs use a fixed
//! caller-selected backend; roundoff is not a certificate of an exact result.
//! The default build and execution policy are sequential and scalar.

mod fft;
mod interval;
pub mod observer;
mod policy;
pub mod sparse;
pub mod sparse_stream;
pub use sparse::{
	MissingEntries, SparseFormat, SparseLimits, SparseMatrix, SparseMatrixBuilder, SparseNorms,
	SuppliedEntries,
};

pub use fft::{
	ConvolutionWorkspace, FftBackend, FftDirection, FftWorkspace, Normalization,
	SharedConvolutionSession, SharedConvolutionWorkspace,
};
pub use interval::Interval;
pub use num_complex::Complex64;
pub use policy::{Error, ExecutionPolicy, Limits, ResourceUsage, Result};

pub mod ad;
pub mod arithmetic;

pub mod roots;

pub mod shapes;

pub mod constraint_chart;

pub use mathcore::resources::{
	Accounted, MemoryReservation, OperationLimits, OperationResources, ResourceError,
	ResourceLimits, ResourceReport, ShapeLimits,
};
