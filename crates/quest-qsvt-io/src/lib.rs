#![forbid(unsafe_code)]
//! QSP and QSVT scientific interchange formats.
//!
//! IO admits shape, finite values and convention tags. Catalog provenance and
//! file metadata are not mathematical certificates. HDF5 is optional and serial;
//! distributed applications call these APIs on the coordinating root only.

mod catalog;
#[cfg(feature = "hdf5")]
pub mod hdf5;
mod json;
mod sparse;
#[rustfmt::skip]
mod catalog_data;
pub use catalog::{CatalogFamily, catalog_families, find_catalog_family};
pub use json::{PolynomialInput, QspInput, read_qsp_json, write_qsp_json};
pub use num_complex::Complex64;
pub use sparse::{
    MissingEntries, SparseFormat, SparseMatrix, SparseMatrixBuilder, SuppliedEntries,
};
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[cfg(feature = "hdf5")]
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
