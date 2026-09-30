#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

pub mod analysis;
mod dense;
mod encoding;
mod materialize;
mod matrix;
mod projection;
mod response;
mod routes;
pub use response::{
    ArgumentDomain, ComponentSelection, EvenResponse, FullResponse, GramArgument,
    HermitianArgument, OddResponse, ResponseArgument, ResponseComponent, ResponseEvidence,
    RouteMeaning, RouteResponse, RouteTarget,
};
mod space;
mod transform;
pub use dense::{DenseEncodingBuilder, DenseNormalization};
pub use encoding::{
    AssumeUnitarity, CheckUnitarity, EncodingBuilder, ExplicitUnitaryPremise, Missing,
    Normalization, OracleUnitarityEvidence, Present, ProjectedEncoding,
};
pub use materialize::{materialize_oracle, materialize_program};
pub use num_complex::Complex64;
pub use projection::{OperandLayout, Projection, ProjectionControl, ProjectionSpace};
pub use quest_circuit::{MatrixPolicy, OracleFragment};
pub use space::{Left, LogicalSpace, ProjectorKind, Right};
#[cfg(feature = "certification")]
pub use transform::CertifiedStandardRecipe;
pub use transform::{
    GeneralizedRecipe, QueryCounts, Route, StandardConvention, StandardRecipe, SuppliedEncoding,
    TransformBuilder, TransformContinuation, TransformEvidence, ValidatedTransform,
};

/// Fallible construction or cold-reference result.
pub type Result<T> = std::result::Result<T, Error>;
/// Invalid model inputs, numerical admission failures, and resource limits.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Polynomial(#[from] quest_polynomial::Error),
    #[error(transparent)]
    Circuit(#[from] quest_circuit::Error),
    #[error(transparent)]
    Qsp(#[from] quest_qsp::Error),
    #[error("invalid logical space: {0}")]
    Space(&'static str),
    #[error("invalid projected encoding: {0}")]
    Encoding(&'static str),
    #[error("nonfinite numerical payload")]
    NonFinite,
    #[error("{operation} residual {residual} exceeds fixed admission tolerance {tolerance}")]
    Residual {
        operation: &'static str,
        residual: f64,
        tolerance: f64,
    },
    #[error("QSVT resource limit: {0}")]
    Budget(&'static str),
}

/// Resource policy. Mathematical admission thresholds are fixed by the library.
#[derive(Debug, Clone, Copy)]
pub struct NumericalPolicy {
    /// Maximum admitted numerical storage; defaults to 64 MiB.
    pub max_bytes: usize,
}
impl Default for NumericalPolicy {
    fn default() -> Self {
        Self {
            max_bytes: 67_108_864,
        }
    }
}
impl NumericalPolicy {
    /// Use the same byte limit for circuit matrix snapshots and admission.
    #[must_use]
    pub const fn matrix_policy(self) -> MatrixPolicy {
        MatrixPolicy {
            max_bytes: self.max_bytes,
        }
    }
    fn check(self, rows: usize, cols: usize, copies: usize) -> Result<usize> {
        let bytes = rows
            .checked_next_multiple_of(4)
            .and_then(|r| r.checked_mul(cols))
            .and_then(|n| n.checked_mul(size_of::<Complex64>()))
            .ok_or(Error::Budget("matrix size overflow"))?;
        if bytes
            .checked_mul(copies)
            .is_none_or(|n| n > self.max_bytes || isize::try_from(n).is_err())
        {
            return Err(Error::Budget("matrix storage"));
        }
        Ok(bytes)
    }
}

impl From<quest_circuit::language::angle::Error> for Error {
    fn from(value: quest_circuit::language::angle::Error) -> Self {
        Self::Circuit(value.into())
    }
}

impl From<quest_circuit::language::matrix::Error> for Error {
    fn from(value: quest_circuit::language::matrix::Error) -> Self {
        Self::Circuit(value.into())
    }
}
