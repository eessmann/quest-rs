#![feature(const_trait_impl, const_ops, generic_const_exprs)]
#![allow(incomplete_features)]
#![forbid(unsafe_code)]
//! Typed binary64 polynomials with explicit basis, support and numerical limits.
//!
//! Coefficients are immutable and stored in increasing basis order. A Laurent
//! basis retains its signed exponent offset. Conversion is an explicit cold
//! operation; evaluation never selects a different precision backend.

pub use quest_numerics::Interval;
pub mod typed;
pub use typed::{Expression, ExpressionMetadata, StaticExpression};
mod backend;
pub use backend::{Backend, JetBackend, ScalarBackend};
mod function;
pub use function::{
    AssumedFunction, Callable, CallbackFunction, ConsistencyAssumption, Expr, ExprNode, Function,
    GenericCallable, Jet,
};
mod norm;
pub use norm::{NormDomain, NormEvidence, NormOptions, NormOutcome, NormStatus};
mod analysis;
pub use analysis::{Conversion, Even, Odd, Parity, ParityPolynomial};
mod remez;
pub use remez::{
    ConditionalRemezFailure, ConditionalRemezResult, CriticalPoint, CriticalPoints, HasCallable,
    HasTarget, MissingTarget, ReadyCallable, ReadyRemez, RemezBuilder, RemezFailure, RemezOptions,
    RemezResult, StaticDegree, StaticRemezBuilder, StaticRemezResult, isolate_critical_points,
    remez,
};
mod basis;
pub use basis::{Basis, Chebyshev, Hermite, Jacobi, Laguerre, Laurent, Monomial};
mod polynomial;
pub use num_complex::Complex64;
pub use polynomial::Polynomial;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("polynomial value or arithmetic result is not finite")]
    NonFinite,
    #[error(transparent)]
    Interval(#[from] quest_numerics::Error),
    #[error("requested parity is not satisfied")]
    Parity,
    #[error("conversion is unsupported for this support")]
    UnsupportedConversion,
    #[error("numerical enclosure could not be established: {0}")]
    NotEstablished(&'static str),
    #[error("basis parameters are outside their mathematical domain")]
    BasisParameters,
    #[error("polynomial operation is undefined at the requested argument")]
    Domain,
    #[error("signed polynomial support overflows")]
    SupportOverflow,
    #[error("polynomial resource budget exceeded: {0}")]
    Budget(&'static str),
    #[error("operation requires real coefficients")]
    NotReal,
}

/// Caller limits for owned coefficient storage and cold transformations.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_coefficients: usize,
    pub max_bytes: usize,
    pub max_work: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_coefficients: 1_048_576,
            max_bytes: 1_073_741_824,
            max_work: 8_589_934_592,
        }
    }
}
impl Limits {
    fn check(self, count: usize, copies: usize) -> Result<()> {
        let bytes = count
            .checked_mul(size_of::<Complex64>())
            .and_then(|value| value.checked_mul(copies))
            .ok_or(Error::Budget("coefficient size overflow"))?;
        if count > self.max_coefficients
            || bytes > self.max_bytes
            || i32::try_from(count).is_err()
            || isize::try_from(bytes).is_err()
        {
            return Err(Error::Budget("coefficient storage"));
        }
        Ok(())
    }
}

const fn finite(value: Complex64) -> Result<Complex64> {
    if value.re.is_finite() && value.im.is_finite() {
        Ok(value)
    } else {
        Err(Error::NonFinite)
    }
}

fn zeros(count: usize, limits: Limits) -> Result<Vec<Complex64>> {
    limits.check(count, 1)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Error::Budget("coefficient allocation"))?;
    values.resize(count, Complex64::new(0.0, 0.0));
    Ok(values)
}
