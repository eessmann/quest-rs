#![allow(
    clippy::float_cmp,
    reason = "Exact endpoint equality and zero are mathematical predicates"
)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "Backend operations own their scalar operands uniformly"
)]
#![allow(
    clippy::many_single_char_names,
    reason = "Conventional endpoint and rounding variables in interval formulas"
)]
#![allow(
    clippy::type_complexity,
    reason = "Backend associated types preserve static dispatch and endpoint pairing"
)]
//! Backend-owned checked arithmetic. Scalars may allocate and need not be Copy.
//! `CertifyingBackend` admits arithmetic only; arbitrary functions still require
//! an explicit mathematical enclosure premise.
use crate::Interval;
pub use crate::ad::{First, FirstBackend, Gradient, GradientBackend, Jet, JetBackend};
use dashu_base::BitTest;
use dashu_float::{
    ConstCache, Context, FBig,
    round::{
        ErrorBounds, Round,
        mode::{Down, HalfEven, Up},
    },
};
use dashu_int::IBig;
use std::cmp::Ordering;
mod interchange;
pub use interchange::{BinaryRounding, exact_from_f64, to_f64};
#[derive(Debug, thiserror::Error)]
pub enum ArithmeticError {
    #[error("nonfinite arithmetic")]
    Nonfinite,
    #[error("invalid arithmetic input: {0}")]
    Interchange(&'static str),
    #[error("outside domain of {0}")]
    Domain(&'static str),
    #[error("arithmetic resource limit: {0}")]
    Budget(&'static str),
    #[error("binary64 enclosure: {0}")]
    Interval(#[from] crate::Error),
}
pub type ArithmeticResult<T> = Result<T, ArithmeticError>;
#[derive(Clone, Debug, PartialEq)]
pub enum ExactConstant {
    Binary64(f64),
    Integer(i64),
    Rational(i64, u64),
    Decimal(String),
    Ratio {
        numerator: String,
        denominator: String,
    },
}
/// Exponent and work limits apply before mathematical shortcuts.
///
/// Dashu uses bit precision and can retain one exact guard digit after add/sub.
/// Operation counts exclude internal transcendental iterations and allocations.
/// Storage admission counts the header and actual stored significand words;
/// temporary allocations and retained capacity remain outside this model.
#[derive(Clone, Copy, Debug)]
pub struct Precision {
    pub bits: usize,
    pub max_abs_exponent: i32,
    pub max_operations: usize,
}
impl Default for Precision {
    fn default() -> Self {
        Self {
            bits: 256,
            max_abs_exponent: 1_000_000,
            max_operations: 10_000_000,
        }
    }
}
pub trait Backend {
    /// Admit a stored scalar without rounding or changing its value. Custom
    /// backends whose scalar type does not enforce validity must override this
    /// hook; the default trusts their scalar admission contract.
    /// # Errors
    /// Rejects invalid values or values outside the backend's resource policy.
    fn validate(&self, value: &Self::Scalar) -> Result<(), Self::Error> {
        let _ = value;
        Ok(())
    }

    /// Account inline bytes and allocations owned by a scalar.
    /// # Errors
    /// Rejects invalid scalars or accounting overflow.
    fn storage_bytes(&self, value: &Self::Scalar) -> Result<usize, Self::Error> {
        let _ = value;
        Ok(std::mem::size_of::<Self::Scalar>())
    }
    /// Upper bound for an output at this backend's selected precision.
    fn working_scalar_bytes(&self) -> usize {
        std::mem::size_of::<Self::Scalar>()
    }

    type Scalar: Clone;
    type Error: From<ArithmeticError>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn visit(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    /// Reserve modeled work for an opaque kernel. The default preserves each
    /// visit's effects and stops at its first error; wrappers may reserve the
    /// whole batch conservatively before forwarding it to an inner backend.
    /// # Errors
    /// Propagates visit failures, including exhausted resource limits.
    fn charge(&mut self, work: usize) -> Result<(), Self::Error> {
        for _ in 0..work {
            self.visit()?;
        }
        Ok(())
    }
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn constant(&mut self, value: &ExactConstant) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn point(&mut self, value: f64) -> Result<Self::Scalar, Self::Error> {
        self.constant(&ExactConstant::Binary64(value))
    }
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn add(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn sub(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn mul(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn div(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn neg(&mut self, a: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn exp(&mut self, a: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn ln(&mut self, a: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn sin(&mut self, a: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn cos(&mut self, a: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn sqrt(&mut self, a: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
}
/// Arithmetic on finite stored scalar values at a selected precision.
///
/// Implementations must validate finite inputs and implement the declared
/// mathematical arithmetic and functions at their selected precision and rounding
/// policy. Successful operations on the same stored operands denote the same
/// mathematical operation; mutable caches and work counters may affect cost or
/// resource errors, but must not change that meaning. `compare` returns `Equal`
/// exactly when the stored scalars have the same mathematical value (including
/// signed zeros). Polynomial support and numerical algorithms rely on these laws.
pub trait PointBackend: Backend {
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn compare(&self, a: &Self::Scalar, b: &Self::Scalar) -> Result<Ordering, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn to_f64(&self, a: &Self::Scalar) -> Result<f64, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn pi(&mut self) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn epsilon(&mut self) -> Result<Self::Scalar, Self::Error>;
    fn precision_bits(&self) -> usize;
}
pub trait EnclosureBackend: Backend {
    type Endpoint: Clone;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn singleton(&mut self, value: &Self::Endpoint) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn lower_endpoint(&self, value: &Self::Scalar) -> Result<Self::Endpoint, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn upper_endpoint(&self, value: &Self::Scalar) -> Result<Self::Endpoint, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn lower(&mut self, value: &Self::Scalar) -> Result<Self::Scalar, Self::Error> {
        let x = self.lower_endpoint(value)?;
        self.singleton(&x)
    }
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn upper(&mut self, value: &Self::Scalar) -> Result<Self::Scalar, Self::Error> {
        let x = self.upper_endpoint(value)?;
        self.singleton(&x)
    }
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn hull(&mut self, a: &Self::Scalar, b: &Self::Scalar) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn intersection(
        &mut self,
        a: &Self::Scalar,
        b: &Self::Scalar,
    ) -> Result<Option<Self::Scalar>, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn contains_zero(&self, a: &Self::Scalar) -> Result<bool, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn is_zero(&self, a: &Self::Scalar) -> Result<bool, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn strict_subset(&self, a: &Self::Scalar, b: &Self::Scalar) -> Result<bool, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn same(&self, a: &Self::Scalar, b: &Self::Scalar) -> Result<bool, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn midpoint(&mut self, a: &Self::Scalar) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn bisect(
        &mut self,
        a: &Self::Scalar,
    ) -> Result<Option<(Self::Scalar, Self::Scalar)>, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn magnitude_lt_one(&self, a: &Self::Scalar) -> Result<bool, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn nonnegative(&self, a: &Self::Scalar) -> Result<bool, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn pi(&mut self) -> Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects invalid domains, backend failures, or exhausted resource limits.
    fn width_le(
        &mut self,
        a: &Self::Scalar,
        tolerance: &Self::Endpoint,
    ) -> Result<bool, Self::Error>;
}
mod sealed {
    pub trait Sealed {}
}
/// Admission is sealed to audited enclosure arithmetic; point arithmetic cannot certify.
/// ```compile_fail
/// use quest_numerics::arithmetic::{CertifyingBackend,F64Backend};
/// fn require_proof<B:CertifyingBackend>() {}
/// require_proof::<F64Backend>();
/// ```
pub trait CertifyingBackend: EnclosureBackend + sealed::Sealed {}
#[derive(Default, Debug)]
pub struct F64Backend;
const fn finite(x: f64) -> ArithmeticResult<f64> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(ArithmeticError::Nonfinite)
    }
}
// Keep adaptive exact imports outside the tiny binary64/AD constant hot path.
fn round_exact_f64(c: &ExactConstant) -> ArithmeticResult<f64> {
    // Resolve rounding from an outward enclosure; a fixed intermediate
    // precision can double-round arbitrarily close to a binary64 midpoint.
    let mut bits = 256;
    loop {
        let mut b = MpIntervalBackend::new(Precision {
            bits,
            ..Precision::default()
        })?;
        let x = b.constant(c)?;
        let lo = to_f64(&x.lower, BinaryRounding::Nearest)?;
        let hi = to_f64(&x.upper, BinaryRounding::Nearest)?;
        if lo.to_bits() == hi.to_bits() {
            return finite(lo);
        }
        bits = bits
            .checked_mul(2)
            .ok_or(ArithmeticError::Budget("constant rounding"))?;
        if bits > 1_048_576 {
            return Err(ArithmeticError::Budget("constant rounding"));
        }
    }
}
impl Backend for F64Backend {
    fn charge(&mut self, _work: usize) -> Result<(), Self::Error> {
        Ok(())
    }

    fn validate(&self, value: &f64) -> ArithmeticResult<()> {
        finite(*value).map(|_| ())
    }

    type Scalar = f64;
    type Error = ArithmeticError;
    #[allow(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "Rust integer-to-f64 conversion rounds this exact integer once"
    )]
    #[inline]
    fn constant(&mut self, c: &ExactConstant) -> ArithmeticResult<f64> {
        if let ExactConstant::Binary64(x) = c {
            return finite(*x);
        }
        if let ExactConstant::Integer(x) = c {
            return Ok(*x as f64);
        }
        round_exact_f64(c)
    }

    #[inline]
    fn point(&mut self, value: f64) -> ArithmeticResult<f64> {
        finite(value)
    }
    fn add(&mut self, a: f64, b: f64) -> ArithmeticResult<f64> {
        finite(a)?;
        finite(b)?;
        finite(a + b)
    }
    fn sub(&mut self, a: f64, b: f64) -> ArithmeticResult<f64> {
        finite(a)?;
        finite(b)?;
        finite(a - b)
    }
    fn mul(&mut self, a: f64, b: f64) -> ArithmeticResult<f64> {
        finite(a)?;
        finite(b)?;
        finite(a * b)
    }
    fn div(&mut self, a: f64, b: f64) -> ArithmeticResult<f64> {
        finite(a)?;
        finite(b)?;
        if b == 0.0 {
            return Err(ArithmeticError::Domain("division"));
        }
        finite(a / b)
    }
    fn neg(&mut self, a: f64) -> ArithmeticResult<f64> {
        finite(-a)
    }
    fn exp(&mut self, a: f64) -> ArithmeticResult<f64> {
        finite(a)?;
        finite(a.exp())
    }
    fn ln(&mut self, a: f64) -> ArithmeticResult<f64> {
        finite(a)?;
        if a <= 0.0 {
            return Err(ArithmeticError::Domain("ln"));
        }
        finite(a.ln())
    }
    fn sqrt(&mut self, a: f64) -> ArithmeticResult<f64> {
        finite(a)?;
        if a < 0.0 {
            return Err(ArithmeticError::Domain("sqrt"));
        }
        finite(a.sqrt())
    }
    fn sin(&mut self, a: f64) -> ArithmeticResult<f64> {
        finite(a)?;
        finite(a.sin())
    }
    fn cos(&mut self, a: f64) -> ArithmeticResult<f64> {
        finite(a)?;
        finite(a.cos())
    }
}
impl PointBackend for F64Backend {
    fn compare(&self, a: &f64, b: &f64) -> ArithmeticResult<Ordering> {
        finite(*a)?;
        finite(*b)?;
        Ok(if a == b {
            Ordering::Equal
        } else if a < b {
            Ordering::Less
        } else {
            Ordering::Greater
        })
    }
    fn to_f64(&self, a: &f64) -> ArithmeticResult<f64> {
        finite(*a)
    }
    fn pi(&mut self) -> ArithmeticResult<f64> {
        Ok(std::f64::consts::PI)
    }
    fn epsilon(&mut self) -> ArithmeticResult<f64> {
        Ok(f64::EPSILON)
    }
    fn precision_bits(&self) -> usize {
        53
    }
}
#[derive(Default, Debug)]
pub struct Interval64Backend;
fn enclose_exact_f64(c: &ExactConstant) -> ArithmeticResult<Interval> {
    let mut b = MpIntervalBackend::new(Precision::default())?;
    let x = b.constant(c)?;
    Ok(Interval::new(
        to_f64(&x.lower, BinaryRounding::Down)?,
        to_f64(&x.upper, BinaryRounding::Up)?,
    )?)
}
impl Backend for Interval64Backend {
    fn charge(&mut self, _work: usize) -> Result<(), Self::Error> {
        Ok(())
    }

    fn validate(&self, value: &Interval) -> ArithmeticResult<()> {
        // Construction and all operations preserve finite, ordered endpoints.
        Interval::new(value.lower(), value.upper())?;
        Ok(())
    }

    type Scalar = Interval;
    type Error = ArithmeticError;
    #[allow(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "Integers within 2^53 are represented exactly in binary64"
    )]
    #[inline]
    fn constant(&mut self, c: &ExactConstant) -> ArithmeticResult<Interval> {
        if let ExactConstant::Binary64(x) = c {
            return Ok(Interval::point(*x)?);
        }
        if let ExactConstant::Integer(x) = c
            && x.unsigned_abs() <= 1_u64 << 53
        {
            return Ok(Interval::point(*x as f64)?);
        }
        enclose_exact_f64(c)
    }

    #[inline]
    fn point(&mut self, value: f64) -> ArithmeticResult<Interval> {
        Ok(Interval::point(value)?)
    }
    fn add(&mut self, a: Interval, b: Interval) -> ArithmeticResult<Interval> {
        Ok(a.checked_add(b)?)
    }
    fn sub(&mut self, a: Interval, b: Interval) -> ArithmeticResult<Interval> {
        Ok(a.checked_sub(b)?)
    }
    fn mul(&mut self, a: Interval, b: Interval) -> ArithmeticResult<Interval> {
        Ok(a.checked_mul(b)?)
    }
    fn div(&mut self, a: Interval, b: Interval) -> ArithmeticResult<Interval> {
        Ok(a.checked_div(b)?)
    }
    fn neg(&mut self, a: Interval) -> ArithmeticResult<Interval> {
        Ok(a.checked_neg()?)
    }
    fn exp(&mut self, a: Interval) -> ArithmeticResult<Interval> {
        Ok(a.exp()?)
    }
    fn ln(&mut self, a: Interval) -> ArithmeticResult<Interval> {
        Ok(a.ln()?)
    }
    fn sqrt(&mut self, a: Interval) -> ArithmeticResult<Interval> {
        Ok(a.sqrt()?)
    }
    fn sin(&mut self, a: Interval) -> ArithmeticResult<Interval> {
        Ok(a.sin()?)
    }
    fn cos(&mut self, a: Interval) -> ArithmeticResult<Interval> {
        Ok(a.cos()?)
    }
}
impl EnclosureBackend for Interval64Backend {
    fn width_le(&mut self, a: &Interval, t: &f64) -> ArithmeticResult<bool> {
        finite(*t)?;
        if *t < 0.0 {
            return Err(ArithmeticError::Domain("negative tolerance"));
        }
        let width = Interval::point(a.upper())?.checked_sub(Interval::point(a.lower())?)?;
        Ok(width.upper() <= *t)
    }
    type Endpoint = f64;
    fn singleton(&mut self, x: &f64) -> ArithmeticResult<Interval> {
        Ok(Interval::point(*x)?)
    }
    fn lower_endpoint(&self, x: &Interval) -> ArithmeticResult<f64> {
        Ok(x.lower())
    }
    fn upper_endpoint(&self, x: &Interval) -> ArithmeticResult<f64> {
        Ok(x.upper())
    }
    fn hull(&mut self, a: &Interval, b: &Interval) -> ArithmeticResult<Interval> {
        Ok(Interval::new(
            a.lower().min(b.lower()),
            a.upper().max(b.upper()),
        )?)
    }
    fn intersection(&mut self, a: &Interval, b: &Interval) -> ArithmeticResult<Option<Interval>> {
        let l = a.lower().max(b.lower());
        let u = a.upper().min(b.upper());
        if l > u {
            Ok(None)
        } else {
            Ok(Some(Interval::new(l, u)?))
        }
    }
    fn contains_zero(&self, a: &Interval) -> ArithmeticResult<bool> {
        Ok(a.contains(0.0))
    }
    fn is_zero(&self, a: &Interval) -> ArithmeticResult<bool> {
        Ok(a.lower() == 0.0 && a.upper() == 0.0)
    }
    fn strict_subset(&self, a: &Interval, b: &Interval) -> ArithmeticResult<bool> {
        Ok(a.lower() > b.lower() && a.upper() < b.upper())
    }
    fn same(&self, a: &Interval, b: &Interval) -> ArithmeticResult<bool> {
        Ok(a.lower() == b.lower() && a.upper() == b.upper())
    }
    fn midpoint(&mut self, a: &Interval) -> ArithmeticResult<Interval> {
        Ok(Interval::point(
            a.upper()
                .mul_add(0.5, a.lower() * 0.5)
                .clamp(a.lower(), a.upper()),
        )?)
    }
    fn bisect(&mut self, a: &Interval) -> ArithmeticResult<Option<(Interval, Interval)>> {
        let m = self.midpoint(a)?.lower();
        if m <= a.lower() || m >= a.upper() {
            return Ok(None);
        }
        Ok(Some((
            Interval::new(a.lower(), m)?,
            Interval::new(m, a.upper())?,
        )))
    }
    fn magnitude_lt_one(&self, a: &Interval) -> ArithmeticResult<bool> {
        Ok(a.lower() > -1.0 && a.upper() < 1.0)
    }
    fn nonnegative(&self, a: &Interval) -> ArithmeticResult<bool> {
        Ok(a.lower() >= 0.0)
    }
    fn pi(&mut self) -> ArithmeticResult<Interval> {
        let mut b = MpIntervalBackend::new(Precision::default())?;
        let x = b.pi()?;
        Ok(Interval::new(
            to_f64(&x.lower, BinaryRounding::Down)?,
            to_f64(&x.upper, BinaryRounding::Up)?,
        )?)
    }
}
impl sealed::Sealed for Interval64Backend {}
impl CertifyingBackend for Interval64Backend {}
/// Native binary floating-point values; the rounding policy is ties-to-even.
pub type Binary = FBig<HalfEven, 2>;
/// Endpoints are exact stored dyadics; construction is backend checked.
#[derive(Clone, Debug)]
pub struct MpInterval {
    lower: Binary,
    upper: Binary,
}
impl MpInterval {
    #[must_use]
    pub const fn lower(&self) -> &Binary {
        &self.lower
    }
    #[must_use]
    pub const fn upper(&self) -> &Binary {
        &self.upper
    }
}
#[derive(Clone, Copy)]
enum Operation {
    Add,
    Sub,
    Mul,
    Div,
    Exp,
    Ln,
    Sqrt,
    Sin,
    Cos,
}
const fn fp_error(error: dashu_float::FpError) -> ArithmeticError {
    use dashu_float::FpError;
    match error {
        FpError::InfiniteInput => ArithmeticError::Nonfinite,
        FpError::OutOfDomain | FpError::Indeterminate => ArithmeticError::Domain("Dashu operation"),
        FpError::Overflow(_) => ArithmeticError::Budget("overflow"),
        FpError::Underflow(_) => ArithmeticError::Budget("underflow"),
        FpError::ZivRetryLimitExceeded => ArithmeticError::Budget("transcendental certification"),
    }
}
fn binary_operation<R: Round>(
    context: Context<R>,
    operation: Operation,
    a: &Binary,
    b: &Binary,
) -> ArithmeticResult<Binary> {
    let result = match operation {
        Operation::Add => context.add(a.repr(), b.repr()),
        Operation::Sub => context.sub(a.repr(), b.repr()),
        Operation::Mul => context.mul(a.repr(), b.repr()),
        Operation::Div => context.div(a.repr(), b.repr()),
        _ => return Err(ArithmeticError::Domain("binary operation")),
    };
    // Changing the type-level rounding mode preserves the exact result, including
    // Dashu's allowed guard digit. A second with_precision would double-round it.
    Ok(result
        .map_err(fp_error)?
        .value()
        .with_rounding::<HalfEven>())
}
fn unary_operation<R: Round + ErrorBounds>(
    context: Context<R>,
    operation: Operation,
    a: &Binary,
    cache: &mut ConstCache,
) -> ArithmeticResult<Binary> {
    let result = match operation {
        Operation::Exp => context.exp(a.repr(), Some(cache)),
        Operation::Ln => context.ln(a.repr(), Some(cache)),
        Operation::Sqrt => context.sqrt(a.repr()),
        Operation::Sin => context.sin(a.repr(), Some(cache)),
        Operation::Cos => context.cos(a.repr(), Some(cache)),
        _ => return Err(ArithmeticError::Domain("unary operation")),
    };
    Ok(result
        .map_err(fp_error)?
        .value()
        .with_rounding::<HalfEven>())
}
pub struct MpBackend {
    precision: Precision,
    operations: usize,
    nearest: Context<HalfEven>,
    down: Context<Down>,
    up: Context<Up>,
    cache: ConstCache,
}
impl MpBackend {
    /// # Errors
    /// Rejects unsupported precision or exponent policy.
    pub fn new(precision: Precision) -> ArithmeticResult<Self> {
        if !(64..=1_048_576).contains(&precision.bits)
            || !(1..=i32::MAX / 4).contains(&precision.max_abs_exponent)
        {
            return Err(ArithmeticError::Budget("precision/exponent"));
        }
        Ok(Self {
            precision,
            operations: 0,
            nearest: Context::new(precision.bits),
            down: Context::new(precision.bits),
            up: Context::new(precision.bits),
            cache: ConstCache::default(),
        })
    }
    #[must_use]
    pub const fn precision(&self) -> Precision {
        self.precision
    }
    #[must_use]
    pub const fn operations(&self) -> usize {
        self.operations
    }
    const fn tick(&mut self) -> ArithmeticResult<()> {
        if self.operations >= self.precision.max_operations {
            return Err(ArithmeticError::Budget("operations"));
        }
        self.operations = self.operations.saturating_add(1);
        Ok(())
    }
    fn validate_value(&self, value: &Binary) -> ArithmeticResult<()> {
        if !value.repr().is_finite() {
            return Err(ArithmeticError::Nonfinite);
        }
        if !value.repr().significand().is_zero() {
            let bits = isize::try_from(value.repr().significand().bit_len())
                .map_err(|_| ArithmeticError::Budget("exponent"))?;
            let top = value
                .repr()
                .exponent()
                .checked_add(bits)
                .ok_or(ArithmeticError::Budget("exponent"))?;
            if top.unsigned_abs()
                > usize::try_from(self.precision.max_abs_exponent)
                    .map_err(|_| ArithmeticError::Budget("exponent"))?
            {
                return Err(ArithmeticError::Budget("exponent"));
            }
        }
        Ok(())
    }
    fn checked(&self, value: Binary) -> ArithmeticResult<Binary> {
        self.validate_value(&value)?;
        Ok(value)
    }
    fn binary(
        &self,
        a: &Binary,
        b: &Binary,
        operation: Operation,
        direction: BinaryRounding,
    ) -> ArithmeticResult<Binary> {
        self.validate_value(a)?;
        self.validate_value(b)?;
        if matches!(operation, Operation::Div) && b.repr().significand().is_zero() {
            return Err(ArithmeticError::Domain("division"));
        }
        self.checked(match direction {
            BinaryRounding::Nearest => binary_operation(self.nearest, operation, a, b)?,
            BinaryRounding::Down => binary_operation(self.down, operation, a, b)?,
            BinaryRounding::Up => binary_operation(self.up, operation, a, b)?,
        })
    }
    fn unary(
        &mut self,
        a: &Binary,
        operation: Operation,
        direction: BinaryRounding,
    ) -> ArithmeticResult<Binary> {
        self.validate_value(a)?;
        if matches!(operation, Operation::Ln) && a <= &Binary::ZERO {
            return Err(ArithmeticError::Domain("ln"));
        }
        if matches!(operation, Operation::Sqrt) && a < &Binary::ZERO {
            return Err(ArithmeticError::Domain("sqrt"));
        }
        let value = match direction {
            BinaryRounding::Nearest => {
                unary_operation(self.nearest, operation, a, &mut self.cache)?
            }
            BinaryRounding::Down => unary_operation(self.down, operation, a, &mut self.cache)?,
            BinaryRounding::Up => unary_operation(self.up, operation, a, &mut self.cache)?,
        };
        self.checked(value)
    }
    fn pi_value(&mut self, direction: BinaryRounding) -> ArithmeticResult<Binary> {
        let value = match direction {
            BinaryRounding::Nearest => self.nearest.pi::<2>(Some(&mut self.cache)).value(),
            BinaryRounding::Down => self
                .down
                .pi::<2>(Some(&mut self.cache))
                .value()
                .with_rounding::<HalfEven>(),
            BinaryRounding::Up => self
                .up
                .pi::<2>(Some(&mut self.cache))
                .value()
                .with_rounding::<HalfEven>(),
        };
        self.checked(value)
    }
    fn integer_value(&self, integer: IBig) -> ArithmeticResult<Binary> {
        self.checked(Binary::from_parts(integer, 0))
    }
    fn ratio(
        &self,
        numerator: IBig,
        denominator: IBig,
        direction: BinaryRounding,
    ) -> ArithmeticResult<Binary> {
        if denominator.is_zero() {
            return Err(ArithmeticError::Domain("rational denominator"));
        }
        // Exact operands are admitted before cancellation can hide their size.
        let numerator = self.integer_value(numerator)?;
        let denominator = self.integer_value(denominator)?;
        self.binary(&numerator, &denominator, Operation::Div, direction)
    }
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "Exact IBig multiplication cannot overflow; decimal scale is admitted before allocation"
    )]
    fn parse(&self, text: &str, direction: BinaryRounding) -> ArithmeticResult<Binary> {
        if text.len() > self.precision.bits.saturating_mul(8) {
            return Err(ArithmeticError::Budget("constant digits"));
        }
        if !valid_decimal(text) {
            return Err(ArithmeticError::Interchange("decimal syntax"));
        }
        let mut parts = text.split(['e', 'E']);
        let mantissa = parts.next().unwrap_or("");
        let exponent = parts
            .next()
            .map(str::parse::<isize>)
            .transpose()
            .map_err(|_| ArithmeticError::Budget("decimal exponent"))?
            .unwrap_or(0);
        if exponent.unsigned_abs()
            > usize::try_from(self.precision.max_abs_exponent)
                .unwrap_or(0)
                .saturating_add(text.len())
        {
            return Err(ArithmeticError::Budget("decimal exponent"));
        }
        let fractional = mantissa.split('.').nth(1).map_or(0, str::len);
        let scale = exponent
            .checked_sub(
                isize::try_from(fractional)
                    .map_err(|_| ArithmeticError::Budget("decimal exponent"))?,
            )
            .ok_or(ArithmeticError::Budget("decimal exponent"))?;
        let digits = mantissa.replace('.', "");
        let mut numerator = IBig::from_str_radix(&digits, 10)
            .map_err(|_| ArithmeticError::Interchange("decimal syntax"))?;
        let mut denominator = IBig::ONE;
        if scale >= 0 {
            numerator *= IBig::from(10).pow(scale.unsigned_abs());
        } else {
            denominator = IBig::from(10).pow(scale.unsigned_abs());
        }
        self.ratio(numerator, denominator, direction)
    }
    fn import(&self, value: &ExactConstant, direction: BinaryRounding) -> ArithmeticResult<Binary> {
        match value {
            ExactConstant::Binary64(value) => self.checked(exact_from_f64(
                *value,
                u32::try_from(self.precision.bits)
                    .map_err(|_| ArithmeticError::Budget("precision"))?,
            )?),
            ExactConstant::Integer(value) => self.checked(
                Binary::from(*value)
                    .with_precision(self.precision.bits)
                    .value(),
            ),
            ExactConstant::Decimal(text) => self.parse(text, direction),
            ExactConstant::Rational(numerator, denominator) => {
                self.ratio(IBig::from(*numerator), IBig::from(*denominator), direction)
            }
            ExactConstant::Ratio {
                numerator,
                denominator,
            } => {
                if !valid_integer(numerator) || !valid_positive_integer(denominator) {
                    return Err(ArithmeticError::Interchange("integer ratio"));
                }
                if numerator.len().max(denominator.len()).saturating_mul(4)
                    > self.precision.bits.saturating_mul(32)
                {
                    return Err(ArithmeticError::Budget("constant digits"));
                }
                let n = IBig::from_str_radix(numerator, 10)
                    .map_err(|_| ArithmeticError::Interchange("integer ratio"))?;
                let d = IBig::from_str_radix(denominator, 10)
                    .map_err(|_| ArithmeticError::Interchange("integer ratio"))?;
                self.ratio(n, d, direction)
            }
        }
    }
}
fn valid_integer(s: &str) -> bool {
    let s = s
        .strip_prefix('-')
        .or_else(|| s.strip_prefix('+'))
        .unwrap_or(s);
    !s.is_empty() && s.bytes().all(|x| x.is_ascii_digit())
}
fn valid_positive_integer(s: &str) -> bool {
    valid_integer(s) && !s.starts_with('-') && s.bytes().any(|x| matches!(x, b'1'..=b'9'))
}
fn valid_decimal(s: &str) -> bool {
    let s = s
        .strip_prefix('-')
        .or_else(|| s.strip_prefix('+'))
        .unwrap_or(s);
    let mut parts = s.split(['e', 'E']);
    let m = parts.next().unwrap_or("");
    let exp = parts.next();
    if parts.next().is_some() || exp.is_some_and(|s| !valid_integer(s)) {
        return false;
    }
    let mut dots = 0usize;
    let mut digits = 0usize;
    for c in m.bytes() {
        if c == b'.' {
            dots = dots.saturating_add(1);
        } else if c.is_ascii_digit() {
            digits = digits.saturating_add(1);
        } else {
            return false;
        }
    }
    dots <= 1 && digits > 0
}
fn float_storage_bytes(words: usize) -> ArithmeticResult<usize> {
    // Dashu keeps up to two words inline. Count actual stored words, including
    // an add/sub guard digit, without treating a requested precision as storage.
    (if words > 2 { words } else { 0 })
        .checked_mul(std::mem::size_of::<dashu_int::Word>())
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Binary>()))
        .ok_or(ArithmeticError::Budget("scalar storage"))
}
impl Backend for MpBackend {
    type Scalar = Binary;
    type Error = ArithmeticError;
    fn validate(&self, value: &Binary) -> ArithmeticResult<()> {
        self.validate_value(value)
    }
    fn storage_bytes(&self, value: &Binary) -> ArithmeticResult<usize> {
        self.validate_value(value)?;
        float_storage_bytes(value.repr().significand().as_sign_words().1.len())
    }
    fn working_scalar_bytes(&self) -> usize {
        float_storage_bytes(
            self.precision
                .bits
                .saturating_add(1)
                .div_ceil(std::mem::size_of::<dashu_int::Word>().saturating_mul(8)),
        )
        .unwrap_or(usize::MAX)
    }
    fn visit(&mut self) -> ArithmeticResult<()> {
        self.tick()
    }
    fn constant(&mut self, value: &ExactConstant) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.import(value, BinaryRounding::Nearest)
    }
    fn add(&mut self, a: Binary, b: Binary) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.binary(&a, &b, Operation::Add, BinaryRounding::Nearest)
    }
    fn sub(&mut self, a: Binary, b: Binary) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.binary(&a, &b, Operation::Sub, BinaryRounding::Nearest)
    }
    fn mul(&mut self, a: Binary, b: Binary) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.binary(&a, &b, Operation::Mul, BinaryRounding::Nearest)
    }
    fn div(&mut self, a: Binary, b: Binary) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.binary(&a, &b, Operation::Div, BinaryRounding::Nearest)
    }
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "Negating an admitted native binary value preserves its exact significand and exponent"
    )]
    fn neg(&mut self, a: Binary) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.validate_value(&a)?;
        self.checked(-a)
    }
    fn exp(&mut self, a: Binary) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.unary(&a, Operation::Exp, BinaryRounding::Nearest)
    }
    fn ln(&mut self, a: Binary) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.unary(&a, Operation::Ln, BinaryRounding::Nearest)
    }
    fn sqrt(&mut self, a: Binary) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.unary(&a, Operation::Sqrt, BinaryRounding::Nearest)
    }
    fn sin(&mut self, a: Binary) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.unary(&a, Operation::Sin, BinaryRounding::Nearest)
    }
    fn cos(&mut self, a: Binary) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.unary(&a, Operation::Cos, BinaryRounding::Nearest)
    }
}
impl PointBackend for MpBackend {
    fn compare(&self, a: &Binary, b: &Binary) -> ArithmeticResult<Ordering> {
        self.validate_value(a)?;
        self.validate_value(b)?;
        Ok(a.cmp(b))
    }
    fn to_f64(&self, value: &Binary) -> ArithmeticResult<f64> {
        self.validate_value(value)?;
        to_f64(value, BinaryRounding::Nearest)
    }
    fn pi(&mut self) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.pi_value(BinaryRounding::Nearest)
    }
    fn epsilon(&mut self) -> ArithmeticResult<Binary> {
        self.tick()?;
        self.checked(
            Binary::from_parts(
                IBig::ONE,
                isize::try_from(self.precision.bits.saturating_sub(1))
                    .map_err(|_| ArithmeticError::Budget("precision"))?
                    .checked_neg()
                    .ok_or(ArithmeticError::Budget("precision"))?,
            )
            .with_precision(self.precision.bits)
            .value(),
        )
    }
    fn precision_bits(&self) -> usize {
        self.precision.bits
    }
}
pub struct MpIntervalBackend {
    point: MpBackend,
}
impl MpIntervalBackend {
    /// # Errors
    /// Rejects unsupported precision or exponent policy.
    pub fn new(precision: Precision) -> ArithmeticResult<Self> {
        Ok(Self {
            point: MpBackend::new(precision)?,
        })
    }
    #[must_use]
    pub const fn precision(&self) -> Precision {
        self.point.precision()
    }
    fn validate_value(&self, value: &MpInterval) -> ArithmeticResult<()> {
        self.point.validate_value(&value.lower)?;
        self.point.validate_value(&value.upper)?;
        if value.lower > value.upper {
            return Err(ArithmeticError::Domain("interval order"));
        }
        Ok(())
    }
    fn interval(&self, lower: Binary, upper: Binary) -> ArithmeticResult<MpInterval> {
        let value = MpInterval { lower, upper };
        self.validate_value(&value)?;
        Ok(value)
    }
    fn monotone(&mut self, a: MpInterval, op: Operation) -> ArithmeticResult<MpInterval> {
        self.point.tick()?;
        self.validate_value(&a)?;
        let lower = self.point.unary(&a.lower, op, BinaryRounding::Down)?;
        let upper = self.point.unary(&a.upper, op, BinaryRounding::Up)?;
        self.interval(lower, upper)
    }
}
impl Backend for MpIntervalBackend {
    type Scalar = MpInterval;
    type Error = ArithmeticError;
    fn validate(&self, value: &MpInterval) -> ArithmeticResult<()> {
        self.validate_value(value)
    }
    fn storage_bytes(&self, value: &MpInterval) -> ArithmeticResult<usize> {
        self.validate_value(value)?;
        self.point
            .storage_bytes(&value.lower)?
            .checked_add(self.point.storage_bytes(&value.upper)?)
            .ok_or(ArithmeticError::Budget("interval storage"))
    }
    fn working_scalar_bytes(&self) -> usize {
        self.point.working_scalar_bytes().saturating_mul(2)
    }
    fn visit(&mut self) -> ArithmeticResult<()> {
        self.point.tick()
    }
    fn constant(&mut self, value: &ExactConstant) -> ArithmeticResult<MpInterval> {
        self.point.tick()?;
        self.interval(
            self.point.import(value, BinaryRounding::Down)?,
            self.point.import(value, BinaryRounding::Up)?,
        )
    }
    fn add(&mut self, a: MpInterval, b: MpInterval) -> ArithmeticResult<MpInterval> {
        self.point.tick()?;
        self.validate_value(&a)?;
        self.validate_value(&b)?;
        self.interval(
            self.point
                .binary(&a.lower, &b.lower, Operation::Add, BinaryRounding::Down)?,
            self.point
                .binary(&a.upper, &b.upper, Operation::Add, BinaryRounding::Up)?,
        )
    }
    fn sub(&mut self, a: MpInterval, b: MpInterval) -> ArithmeticResult<MpInterval> {
        self.point.tick()?;
        self.validate_value(&a)?;
        self.validate_value(&b)?;
        self.interval(
            self.point
                .binary(&a.lower, &b.upper, Operation::Sub, BinaryRounding::Down)?,
            self.point
                .binary(&a.upper, &b.lower, Operation::Sub, BinaryRounding::Up)?,
        )
    }
    fn mul(&mut self, a: MpInterval, b: MpInterval) -> ArithmeticResult<MpInterval> {
        self.products(a, b, false)
    }
    fn div(&mut self, a: MpInterval, b: MpInterval) -> ArithmeticResult<MpInterval> {
        self.products(a, b, true)
    }
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "Negating admitted native endpoints preserves their exact significands and exponents"
    )]
    fn neg(&mut self, a: MpInterval) -> ArithmeticResult<MpInterval> {
        self.point.tick()?;
        self.validate_value(&a)?;
        self.interval(-a.upper, -a.lower)
    }
    fn exp(&mut self, a: MpInterval) -> ArithmeticResult<MpInterval> {
        self.monotone(a, Operation::Exp)
    }
    fn ln(&mut self, a: MpInterval) -> ArithmeticResult<MpInterval> {
        self.monotone(a, Operation::Ln)
    }
    fn sqrt(&mut self, a: MpInterval) -> ArithmeticResult<MpInterval> {
        self.monotone(a, Operation::Sqrt)
    }
    fn sin(&mut self, a: MpInterval) -> ArithmeticResult<MpInterval> {
        self.trig(a, true)
    }
    fn cos(&mut self, a: MpInterval) -> ArithmeticResult<MpInterval> {
        self.trig(a, false)
    }
}
fn minimum(a: Binary, b: Binary) -> Binary {
    if a <= b { a } else { b }
}
fn maximum(a: Binary, b: Binary) -> Binary {
    if a >= b { a } else { b }
}
impl MpIntervalBackend {
    fn products(
        &mut self,
        a: MpInterval,
        b: MpInterval,
        divide: bool,
    ) -> ArithmeticResult<MpInterval> {
        self.point.tick()?;
        self.validate_value(&a)?;
        self.validate_value(&b)?;
        if divide && self.contains_zero(&b)? {
            return Err(ArithmeticError::Domain("division"));
        }
        let op = if divide {
            Operation::Div
        } else {
            Operation::Mul
        };
        let mut lower = None;
        let mut upper = None;
        for x in [&a.lower, &a.upper] {
            for y in [&b.lower, &b.upper] {
                let lo = self.point.binary(x, y, op, BinaryRounding::Down)?;
                let hi = self.point.binary(x, y, op, BinaryRounding::Up)?;
                lower = Some(match lower {
                    None => lo,
                    Some(value) => minimum(lo, value),
                });
                upper = Some(match upper {
                    None => hi,
                    Some(value) => maximum(hi, value),
                });
            }
        }
        self.interval(
            lower.ok_or(ArithmeticError::Domain("product"))?,
            upper.ok_or(ArithmeticError::Domain("product"))?,
        )
    }
    fn integer_possible(&self, q: &MpInterval) -> ArithmeticResult<bool> {
        self.validate_value(q)?;
        let integer_bits = isize::try_from(self.point.precision.bits.saturating_sub(2))
            .map_err(|_| ArithmeticError::Budget("integer bits"))?;
        for x in [&q.lower, &q.upper] {
            if !x.repr().significand().is_zero()
                && x.repr().exponent().saturating_add(
                    isize::try_from(x.repr().significand().bit_len()).unwrap_or(isize::MAX),
                ) > integer_bits
            {
                return Ok(true);
            }
        }
        Ok(q.lower.ceil() <= q.upper.floor())
    }
    fn trig(&mut self, a: MpInterval, sine: bool) -> ArithmeticResult<MpInterval> {
        self.point.tick()?;
        self.validate_value(&a)?;
        let pi = self.pi()?;
        let q = self.div(a.clone(), pi)?;
        let two = self.point(2.0)?;
        let phase = self.point(if sine { 0.5 } else { 0.0 })?;
        let positive = self.sub(q.clone(), phase)?;
        let positive = self.div(positive, two.clone())?;
        let phase = self.point(if sine { -0.5 } else { 1.0 })?;
        let negative = self.sub(q, phase)?;
        let negative = self.div(negative, two)?;
        let has_max = self.integer_possible(&positive)?;
        let has_min = self.integer_possible(&negative)?;
        if has_max && has_min {
            return self.interval(Binary::NEG_ONE, Binary::ONE);
        }
        let op = if sine { Operation::Sin } else { Operation::Cos };
        let ll = self.point.unary(&a.lower, op, BinaryRounding::Down)?;
        let ul = self.point.unary(&a.upper, op, BinaryRounding::Down)?;
        let lu = self.point.unary(&a.lower, op, BinaryRounding::Up)?;
        let uu = self.point.unary(&a.upper, op, BinaryRounding::Up)?;
        self.interval(
            if has_min {
                Binary::NEG_ONE
            } else {
                minimum(ll, ul)
            },
            if has_max {
                Binary::ONE
            } else {
                maximum(lu, uu)
            },
        )
    }
}
impl EnclosureBackend for MpIntervalBackend {
    type Endpoint = Binary;
    fn width_le(&mut self, a: &MpInterval, tolerance: &Binary) -> ArithmeticResult<bool> {
        self.point.tick()?;
        self.validate_value(a)?;
        self.point.validate_value(tolerance)?;
        if tolerance < &Binary::ZERO {
            return Err(ArithmeticError::Domain("negative tolerance"));
        }
        let width = self
            .point
            .binary(&a.upper, &a.lower, Operation::Sub, BinaryRounding::Up)?;
        Ok(width <= *tolerance)
    }
    fn singleton(&mut self, value: &Binary) -> ArithmeticResult<MpInterval> {
        self.point.tick()?;
        self.interval(value.clone(), value.clone())
    }
    fn lower_endpoint(&self, value: &MpInterval) -> ArithmeticResult<Binary> {
        self.validate_value(value)?;
        Ok(value.lower.clone())
    }
    fn upper_endpoint(&self, value: &MpInterval) -> ArithmeticResult<Binary> {
        self.validate_value(value)?;
        Ok(value.upper.clone())
    }
    fn hull(&mut self, a: &MpInterval, b: &MpInterval) -> ArithmeticResult<MpInterval> {
        self.point.tick()?;
        self.validate_value(a)?;
        self.validate_value(b)?;
        self.interval(
            minimum(a.lower.clone(), b.lower.clone()),
            maximum(a.upper.clone(), b.upper.clone()),
        )
    }
    fn intersection(
        &mut self,
        a: &MpInterval,
        b: &MpInterval,
    ) -> ArithmeticResult<Option<MpInterval>> {
        self.point.tick()?;
        self.validate_value(a)?;
        self.validate_value(b)?;
        let lower = maximum(a.lower.clone(), b.lower.clone());
        let upper = minimum(a.upper.clone(), b.upper.clone());
        if lower > upper {
            Ok(None)
        } else {
            Ok(Some(self.interval(lower, upper)?))
        }
    }
    fn contains_zero(&self, value: &MpInterval) -> ArithmeticResult<bool> {
        self.validate_value(value)?;
        Ok(value.lower <= Binary::ZERO && value.upper >= Binary::ZERO)
    }
    fn is_zero(&self, value: &MpInterval) -> ArithmeticResult<bool> {
        self.validate_value(value)?;
        Ok(
            value.lower.repr().significand().is_zero()
                && value.upper.repr().significand().is_zero(),
        )
    }
    fn strict_subset(&self, a: &MpInterval, b: &MpInterval) -> ArithmeticResult<bool> {
        self.validate_value(a)?;
        self.validate_value(b)?;
        Ok(a.lower > b.lower && a.upper < b.upper)
    }
    fn same(&self, a: &MpInterval, b: &MpInterval) -> ArithmeticResult<bool> {
        self.validate_value(a)?;
        self.validate_value(b)?;
        Ok(a.lower == b.lower && a.upper == b.upper)
    }
    fn midpoint(&mut self, a: &MpInterval) -> ArithmeticResult<MpInterval> {
        self.point.tick()?;
        self.validate_value(a)?;
        let lo = Binary::from_parts(
            a.lower.repr().significand().clone(),
            a.lower
                .repr()
                .exponent()
                .checked_sub(1)
                .ok_or(ArithmeticError::Budget("exponent"))?,
        );
        let hi = Binary::from_parts(
            a.upper.repr().significand().clone(),
            a.upper
                .repr()
                .exponent()
                .checked_sub(1)
                .ok_or(ArithmeticError::Budget("exponent"))?,
        );
        let middle = self
            .point
            .binary(&lo, &hi, Operation::Add, BinaryRounding::Nearest)?;
        let middle = minimum(maximum(middle, a.lower.clone()), a.upper.clone());
        self.interval(middle.clone(), middle)
    }
    fn bisect(&mut self, a: &MpInterval) -> ArithmeticResult<Option<(MpInterval, MpInterval)>> {
        self.validate_value(a)?;
        let middle = self.midpoint(a)?.lower;
        if middle <= a.lower || middle >= a.upper {
            return Ok(None);
        }
        Ok(Some((
            self.interval(a.lower.clone(), middle.clone())?,
            self.interval(middle, a.upper.clone())?,
        )))
    }
    fn magnitude_lt_one(&self, value: &MpInterval) -> ArithmeticResult<bool> {
        self.validate_value(value)?;
        Ok(value.lower > Binary::NEG_ONE && value.upper < Binary::ONE)
    }
    fn nonnegative(&self, value: &MpInterval) -> ArithmeticResult<bool> {
        self.validate_value(value)?;
        Ok(value.lower >= Binary::ZERO)
    }
    fn pi(&mut self) -> ArithmeticResult<MpInterval> {
        self.point.tick()?;
        let lower = self.point.pi_value(BinaryRounding::Down)?;
        let upper = self.point.pi_value(BinaryRounding::Up)?;
        self.interval(lower, upper)
    }
}
impl sealed::Sealed for MpIntervalBackend {}
impl CertifyingBackend for MpIntervalBackend {}

mod budget;
pub use budget::{Budget, BudgetedBackend};
