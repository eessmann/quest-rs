#![forbid(unsafe_code)]
//! QSP and QSVT scientific interchange formats.
//!
//! IO admits shape, finite values and convention tags. Catalog provenance and
//! file metadata are not mathematical certificates. HDF5 is required and serial;
//! Legacy whole-file readers run on the coordinating root. Sharded readers run
//! independently on each file owner and never require the complete input.

mod catalog;
#[cfg(feature = "certification")]
mod compiled;
#[cfg(feature = "certification")]
pub use compiled::{CompiledInput, read_compiled_qsp_json};
pub mod hdf5;
mod json;
pub mod sharded_matching;
mod sparse;
pub mod sparse_stream;
pub use catalog::{CatalogFamily, CatalogSource, InverseCatalog};
pub use json::{
	GeneralizedAngleInput, PolynomialConversion, PolynomialInput, QspInput,
	read_qsp_execution_json, read_qsp_execution_json_with_tolerance, read_qsp_json,
	read_qsp_json_with_tolerance, write_qsp_execution_json, write_qsp_json,
};
pub use num_complex::Complex64;
pub use sparse::{
	MissingEntries, SparseFormat, SparseMatrix, SparseMatrixBuilder, SuppliedEntries,
};
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
	#[cfg(feature = "certification")]
	#[error(transparent)]
	Artifact(#[from] quest_qsp::artifact::ArtifactError),
	#[error(transparent)]
	Hdf5(#[from] hdf5_metno::Error),
	#[error(transparent)]
	Json(#[from] serde_json::Error),
	#[error(transparent)]
	Polynomial(#[from] quest_polynomial::Error),
	#[error(transparent)]
	Qsp(#[from] quest_qsp::Error),
	#[error(transparent)]
	Io(#[from] std::io::Error),
	#[error("invalid scientific file: {0}")]
	Format(&'static str),
	#[error("IO resource limit: {0}")]
	Budget(&'static str),
	#[error("nonfinite scientific value")]
	NonFinite,
}
#[derive(Debug, Clone, Copy)]
pub struct IoPolicy {
	pub max_bytes: usize,
	pub max_coefficients: usize,
	pub max_dimension: usize,
}
impl Default for IoPolicy {
	fn default() -> Self {
		Self {
			max_bytes: 536_870_912,
			max_coefficients: 1_048_576,
			max_dimension: 1_048_576,
		}
	}
}
impl IoPolicy {
	fn check_sparse_storage(self, entries: usize, pointers: usize) -> Result<()> {
		// Peak sparse import retains value/index buffers and pointer storage.
		let bytes = entries
			.checked_mul(64)
			.and_then(|n| pointers.checked_mul(16).and_then(|p| n.checked_add(p)))
			.ok_or(Error::Budget("sparse storage"))?;
		if bytes > self.max_bytes || isize::try_from(bytes).is_err() {
			return Err(Error::Budget("sparse storage"));
		}
		Ok(())
	}
	fn check_sparse_retained(
		self,
		data_capacity: usize,
		index_capacity: usize,
		pointer_capacity: usize,
	) -> Result<()> {
		let bytes = data_capacity
			.checked_mul(size_of::<Complex64>())
			.and_then(|bytes| {
				index_capacity
					.checked_mul(size_of::<usize>())
					.and_then(|indices| bytes.checked_add(indices))
			})
			.and_then(|bytes| {
				pointer_capacity
					.checked_mul(size_of::<usize>())
					.and_then(|pointers| bytes.checked_add(pointers))
			})
			.ok_or(Error::Budget("sparse retained storage"))?;
		if bytes > self.max_bytes || isize::try_from(bytes).is_err() {
			return Err(Error::Budget("sparse retained storage"));
		}
		Ok(())
	}
	fn check(self, count: usize, copies: usize) -> Result<()> {
		if count
			.checked_mul(size_of::<Complex64>())
			.and_then(|n| n.checked_mul(copies))
			.is_none_or(|n| n > self.max_bytes || isize::try_from(n).is_err())
		{
			return Err(Error::Budget("payload size"));
		}
		Ok(())
	}
	fn polynomial_limits(self) -> quest_polynomial::Limits {
		quest_polynomial::Limits {
			max_coefficients: self.max_coefficients,
			max_bytes: self.max_bytes,
			..quest_polynomial::Limits::default()
		}
	}
}

const fn finite(value: Complex64) -> Result<Complex64> {
	if value.re.is_finite() && value.im.is_finite() {
		Ok(value)
	} else {
		Err(Error::NonFinite)
	}
}
