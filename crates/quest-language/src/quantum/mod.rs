//! Checked finite quantum regions and immutable semantic payloads.
mod error;
mod gate_matrix;
pub mod model;
mod oracle;
pub mod program;
pub mod provenance;
pub use crate as language;
pub use crate::rational;
pub use crate::{
	matrix::{MatrixPolicy, NumericalOperator, UnitaryAdmission},
	rational::RBig,
};
pub use error::{Error, Result};
pub use model::*;
pub use oracle::*;
pub use program::*;
pub use provenance::*;
pub mod matrix {
	pub use crate::matrix::*;
}

mod storage;
pub use storage::RetainedStorage;

pub mod evidence;
