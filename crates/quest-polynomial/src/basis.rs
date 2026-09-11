use crate::function::Real;
use crate::{Error, Interval, Result};

mod sealed {
    pub trait Sealed {}
}

/// A three-term basis, with signed support supplied only by Laurent.
pub trait Basis: sealed::Sealed + Clone + std::fmt::Debug {
    #[doc(hidden)]
    fn recurrence(&self, degree: u32) -> Result<(f64, f64, f64)>;
    #[doc(hidden)]
    fn interval_recurrence(&self, degree: u32) -> Result<(Interval, Interval, Interval)> {
        let (a, b, c) = self.recurrence(degree)?;
        Ok((
            Interval::point(a)?,
            Interval::point(b)?,
            Interval::point(c)?,
        ))
    }
    #[doc(hidden)]
    fn symmetric(&self) -> bool {
        true
    }
    fn offset(&self) -> i32 {
        0
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Monomial;
#[derive(Debug, Clone, Copy, Default)]
pub struct Chebyshev;
#[derive(Debug, Clone, Copy)]
pub struct Laurent {
    offset: i32,
}
impl Laurent {
    #[must_use]
    pub const fn new(offset: i32) -> Self {
        Self { offset }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Hermite {
    physicists: bool,
}
impl Hermite {
    #[must_use]
    pub const fn physicists() -> Self {
        Self { physicists: true }
    }
    #[must_use]
    pub const fn scale(self) -> f64 {
        if self.physicists { 2.0 } else { 1.0 }
    }
    #[must_use]
    pub const fn probabilists() -> Self {
        Self { physicists: false }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Laguerre {
    alpha: f64,
}
impl Laguerre {
    /// # Errors
    /// Rejects a nonfinite parameter or alpha at or below minus one.
    pub fn new(alpha: f64) -> Result<Self> {
        if !alpha.is_finite() || alpha <= -1.0 {
            return Err(Error::BasisParameters);
        }
        Ok(Self { alpha })
    }
    #[must_use]
    pub const fn alpha(self) -> f64 {
        self.alpha
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Jacobi {
    alpha: f64,
    beta: f64,
}
impl Jacobi {
    /// # Errors
    /// Rejects nonfinite parameters or parameters at or below minus one.
    pub fn new(alpha: f64, beta: f64) -> Result<Self> {
        if !alpha.is_finite() || !beta.is_finite() || alpha <= -1.0 || beta <= -1.0 {
            return Err(Error::BasisParameters);
        }
        Ok(Self { alpha, beta })
    }
    #[must_use]
    pub const fn parameters(self) -> (f64, f64) {
        (self.alpha, self.beta)
    }
}

impl sealed::Sealed for Monomial {}
impl sealed::Sealed for Chebyshev {}
impl sealed::Sealed for Laurent {}
impl sealed::Sealed for Hermite {}
impl sealed::Sealed for Laguerre {}
impl sealed::Sealed for Jacobi {}
impl Basis for Monomial {
    fn recurrence(&self, _: u32) -> Result<(f64, f64, f64)> {
        Ok((1.0, 0.0, 0.0))
    }
}
impl Basis for Laurent {
    fn recurrence(&self, _: u32) -> Result<(f64, f64, f64)> {
        Ok((1.0, 0.0, 0.0))
    }
    fn offset(&self) -> i32 {
        self.offset
    }
}
impl Basis for Chebyshev {
    fn recurrence(&self, degree: u32) -> Result<(f64, f64, f64)> {
        Ok(if degree <= 1 {
            (1.0, 0.0, 0.0)
        } else {
            (2.0, 0.0, 1.0)
        })
    }
}
impl Basis for Hermite {
    fn recurrence(&self, degree: u32) -> Result<(f64, f64, f64)> {
        let scale = if self.physicists { 2.0 } else { 1.0 };
        Ok((scale, 0.0, scale * f64::from(degree.saturating_sub(1))))
    }
}
impl Basis for Laguerre {
    fn symmetric(&self) -> bool {
        false
    }
    fn interval_recurrence(&self, degree: u32) -> Result<(Interval, Interval, Interval)> {
        laguerre_interval(degree, self.alpha)
    }
    fn recurrence(&self, degree: u32) -> Result<(f64, f64, f64)> {
        let n = f64::from(degree);
        if degree == 0 {
            return Err(Error::BasisParameters);
        }
        checked((
            -1.0 / n,
            (2.0_f64.mul_add(n, -1.0) + self.alpha) / n,
            if degree == 1 {
                0.0
            } else {
                (n - 1.0 + self.alpha) / n
            },
        ))
    }
}
impl Basis for Jacobi {
    #[expect(
        clippy::float_cmp,
        reason = "Exact equality of basis parameters is required for parity"
    )]
    fn symmetric(&self) -> bool {
        self.alpha == self.beta
    }
    fn interval_recurrence(&self, degree: u32) -> Result<(Interval, Interval, Interval)> {
        jacobi_interval(degree, self.alpha, self.beta)
    }
    fn recurrence(&self, degree: u32) -> Result<(f64, f64, f64)> {
        let (a, b) = (self.alpha, self.beta);
        if degree <= 1 {
            return checked(((a + b + 2.0) / 2.0, (a - b) / 2.0, 0.0));
        }
        let n = f64::from(degree);
        let sum = a + b;
        let twice = 2.0_f64.mul_add(n, sum);
        let denominator = 2.0 * n * (n + sum) * (twice - 2.0);
        checked((
            (twice - 1.0) * twice * (twice - 2.0) / denominator,
            (twice - 1.0) * b.mul_add(-b, a * a) / denominator,
            2.0 * (n + a - 1.0) * (n + b - 1.0) * twice / denominator,
        ))
    }
}
const fn checked(value: (f64, f64, f64)) -> Result<(f64, f64, f64)> {
    if value.0.is_finite() && value.1.is_finite() && value.2.is_finite() {
        Ok(value)
    } else {
        Err(Error::NonFinite)
    }
}

fn laguerre_interval(degree: u32, alpha: f64) -> Result<(Interval, Interval, Interval)> {
    let n = Interval::point(f64::from(degree))?;
    let a = Interval::point(alpha)?;
    let one = Interval::point(1.0)?;
    Ok((
        one.neg()?.div(n)?,
        n.mul(Interval::point(2.0)?)?.sub(one)?.add(a)?.div(n)?,
        if degree == 1 {
            Interval::point(0.0)?
        } else {
            n.sub(one)?.add(a)?.div(n)?
        },
    ))
}
fn jacobi_interval(degree: u32, alpha: f64, beta: f64) -> Result<(Interval, Interval, Interval)> {
    let a = Interval::point(alpha)?;
    let b = Interval::point(beta)?;
    let one = Interval::point(1.0)?;
    let two = Interval::point(2.0)?;
    if degree <= 1 {
        return Ok((
            a.add(b)?.add(two)?.div(two)?,
            a.sub(b)?.div(two)?,
            Interval::point(0.0)?,
        ));
    }
    let n = Interval::point(f64::from(degree))?;
    let sum = a.add(b)?;
    let twice = two.mul(n)?.add(sum)?;
    let denominator = two.mul(n)?.mul(n.add(sum)?)?.mul(twice.sub(two)?)?;
    Ok((
        twice
            .sub(one)?
            .mul(twice)?
            .mul(twice.sub(two)?)?
            .div(denominator)?,
        twice
            .sub(one)?
            .mul(a.mul(a)?.sub(b.mul(b)?)?)?
            .div(denominator)?,
        two.mul(n.add(a)?.sub(one)?)?
            .mul(n.add(b)?.sub(one)?)?
            .mul(twice)?
            .div(denominator)?,
    ))
}
