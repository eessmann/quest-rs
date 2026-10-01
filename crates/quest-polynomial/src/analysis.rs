use crate::{
    Basis, Chebyshev, Complex64, Error, Hermite, Interval, Jacobi, Jet, Laguerre, Laurent,
    Monomial, Polynomial, Result, finite, zeros,
};
use std::{
    marker::PhantomData,
    ops::{Add, Mul},
};

/// A binary64 conversion and an outward upper bound on the coefficient l1 error.
///
/// The bound is in the output basis. For monomials it bounds response on |x|<=1.
/// Both immutable payloads are retained so evidence cannot be reassigned to another target.
///
/// ```compile_fail
/// use quest_polynomial::{Conversion, Monomial, Polynomial};
/// fn forge(source: Polynomial<Monomial>, polynomial: Polynomial<Monomial>) {
///     let _ = Conversion { source, polynomial, coefficient_error_bound: 0.0 };
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Conversion<B: Basis = Monomial, S: Basis = Monomial> {
    source: Polynomial<S>,
    polynomial: Polynomial<B>,
    coefficient_error_bound: f64,
}
impl<B: Basis, S: Basis> Conversion<B, S> {
    #[must_use]
    pub const fn source(&self) -> &Polynomial<S> {
        &self.source
    }
    #[must_use]
    pub const fn polynomial(&self) -> &Polynomial<B> {
        &self.polynomial
    }
    #[must_use]
    pub const fn coefficient_error_bound(&self) -> f64 {
        self.coefficient_error_bound
    }
    /// Discard conversion evidence and retain the rounded destination coefficients.
    #[must_use]
    pub fn into_polynomial(self) -> Polynomial<B> {
        self.polynomial
    }
}
mod sealed {
    pub trait Sealed {}
}
pub trait Parity: sealed::Sealed {
    const ODD: bool;
}
#[derive(Debug, Clone, Copy)]
pub struct Even;
#[derive(Debug, Clone, Copy)]
pub struct Odd;
impl sealed::Sealed for Even {}
impl sealed::Sealed for Odd {}
impl Parity for Even {
    const ODD: bool = false;
}
impl Parity for Odd {
    const ODD: bool = true;
}
/// Polynomial whose forbidden parity coefficients are exactly zero.
#[derive(Debug, Clone)]
pub struct ParityPolynomial<B: Basis, P: Parity> {
    polynomial: Polynomial<B>,
    parity: PhantomData<P>,
}
impl<B: Basis, P: Parity> ParityPolynomial<B, P> {
    #[must_use]
    pub const fn polynomial(&self) -> &Polynomial<B> {
        &self.polynomial
    }
    #[must_use]
    pub fn into_polynomial(self) -> Polynomial<B> {
        self.polynomial
    }
}
impl<B: Basis> Polynomial<B> {
    /// # Errors
    /// Rejects nonsymmetric bases or any nonzero forbidden parity coefficient.
    pub fn admit_parity<P: Parity>(self) -> Result<ParityPolynomial<B, P>> {
        if !self.basis().symmetric() {
            return Err(Error::Parity);
        }
        for (i, c) in self.coefficients().iter().enumerate() {
            let k = i32::try_from(i)
                .map_err(|_| Error::SupportOverflow)?
                .checked_add(self.basis().offset())
                .ok_or(Error::SupportOverflow)?;
            if (k.rem_euclid(2) == 1) != P::ODD && *c != Complex64::new(0.0, 0.0) {
                return Err(Error::Parity);
            }
        }
        Ok(ParityPolynomial {
            polynomial: self,
            parity: PhantomData,
        })
    }
    /// Enclose original real coefficients and original basis parameters.
    /// # Errors
    /// Rejects complex coefficients, poles or unbounded interval arithmetic.
    pub fn evaluate_interval(&self, x: Interval) -> Result<Interval> {
        Ok(self.jet_interval(x)?.value)
    }
    /// Enclose value and mathematical derivatives without rounding derivative coefficients.
    /// # Errors
    /// Rejects complex coefficients, poles or unbounded interval arithmetic.
    pub fn jet_interval(&self, x: Interval) -> Result<Jet<Interval>> {
        if self.coefficients().iter().any(|c| c.im != 0.0) {
            return Err(Error::NotReal);
        }
        let zero = Jet::constant(Interval::point(0.0)?)?;
        if self.is_zero() {
            return Ok(zero);
        }
        // A Laurent storage offset is not a pole when all negative powers vanish.
        if self.basis().offset() < 0
            && self
                .effective_support()
                .is_some_and(|(first, _)| first >= 0)
        {
            let skip = usize::try_from(self.basis().offset().unsigned_abs())
                .map_err(|_| Error::SupportOverflow)?;
            let coefficients = self
                .coefficients()
                .get(skip..)
                .ok_or(Error::SupportOverflow)?;
            return Polynomial::new(Laurent::new(0), coefficients.to_vec(), self.limits())?
                .jet_interval(x);
        }
        let variable = Jet {
            value: x,
            first: Interval::point(1.0)?,
            second: Interval::point(0.0)?,
        };
        let mut previous = zero;
        let mut current = Jet::constant(Interval::point(1.0)?)?;
        let mut sum = zero;
        for (i, c) in self.coefficients().iter().enumerate() {
            if i > 0 {
                let (a, b, d) = self
                    .basis()
                    .interval_recurrence(u32::try_from(i).map_err(|_| Error::SupportOverflow)?)?;
                let next = variable
                    .mul(Jet::constant(a)?)?
                    .add(Jet::constant(b)?)?
                    .mul(current)?
                    .sub(previous.mul(Jet::constant(d)?)?)?;
                previous = current;
                current = next;
            }
            sum = sum.add(current.mul(Jet::constant(Interval::point(c.re)?)?)?)?;
        }
        let mut shift = self.basis().offset().unsigned_abs();
        let mut factor = variable;
        let mut power = Jet::constant(Interval::point(1.0)?)?;
        while shift > 0 {
            if shift & 1 != 0 {
                power = power.mul(factor)?;
            }
            shift >>= 1;
            if shift > 0 {
                factor = factor.mul(factor)?;
            }
        }
        if self.basis().offset() < 0 {
            power = power.recip()?;
        }
        sum.mul(power)
    }
    /// Explicit conversion into an unshifted basis, with outward coefficient evidence.
    ///
    /// This cold quadratic operation uses an interval triangular solve. Negative
    /// Laurent support and a shifted target require a separate support transform.
    /// # Errors
    /// Rejects unsupported support, nonfinite arithmetic, unresolved leading
    /// coefficients, or a caller storage/work limit.
    pub fn to_basis<C: Basis>(&self, target: C) -> Result<Conversion<C, B>> {
        if target.offset() != 0 {
            return Err(Error::UnsupportedConversion);
        }
        if self.basis().offset() < 0 {
            return Err(Error::UnsupportedConversion);
        }
        let offset = usize::try_from(self.basis().offset()).map_err(|_| Error::SupportOverflow)?;
        let count = self
            .coefficients()
            .len()
            .checked_add(offset)
            .ok_or(Error::SupportOverflow)?;
        self.limits().check(count, 12)?;
        let square = count
            .checked_mul(count)
            .ok_or(Error::Budget("conversion storage"))?;
        // Admit the whole transformation before its first allocation. The dense
        // interval rows coexist with their Vec headers, the converted source,
        // two interval residuals and the destination coefficients. Twelve linear
        // arrays conservatively cover those buffers and the monomial stage.
        let linear_bytes = count
            .checked_mul(const { 12 * size_of::<Complex64>() + size_of::<Vec<Interval>>() })
            .ok_or(Error::Budget("conversion storage"))?;
        let bytes = square
            .checked_mul(size_of::<Interval>())
            .and_then(|dense| dense.checked_add(linear_bytes))
            .ok_or(Error::Budget("conversion storage"))?;
        // Logical coefficient work includes the monomial recurrence (n²),
        // basis construction and both triangular residual solves (4n²).
        let work = square
            .checked_mul(5)
            .ok_or(Error::Budget("conversion work"))?;
        if bytes > self.limits().max_bytes
            || isize::try_from(bytes).is_err()
            || work > self.limits().max_work
        {
            return Err(Error::Budget("conversion workspace"));
        }
        let source = self.to_monomial()?;
        let rows = basis_rows(&target, count)?;
        let zero = Interval::point(0.0)?;
        let radius = Interval::new(
            -source.coefficient_error_bound,
            source.coefficient_error_bound,
        )?;
        let mut real = Vec::with_capacity(count);
        let mut imag = Vec::with_capacity(count);
        for c in source.polynomial.coefficients() {
            real.push(Interval::point(c.re)?.checked_add(radius)?);
            imag.push(Interval::point(c.im)?.checked_add(radius)?);
        }
        let mut coefficients = zeros(count, self.limits())?;
        let mut error = zero;
        for index in (0..count).rev() {
            let row = rows.get(index).ok_or(Error::SupportOverflow)?;
            let leading = *row.get(index).ok_or(Error::SupportOverflow)?;
            let re = real
                .get(index)
                .copied()
                .ok_or(Error::SupportOverflow)?
                .checked_div(leading)?;
            let im = imag
                .get(index)
                .copied()
                .ok_or(Error::SupportOverflow)?
                .checked_div(leading)?;
            let value = coefficients.get_mut(index).ok_or(Error::SupportOverflow)?;
            value.re = 0.5_f64.mul_add(re.lower(), 0.5 * re.upper());
            value.im = 0.5_f64.mul_add(im.lower(), 0.5 * im.upper());
            for (interval, rounded) in [(re, value.re), (im, value.im)] {
                let delta = interval.checked_sub(Interval::point(rounded)?)?;
                error = error.checked_add(Interval::point(
                    delta.lower().abs().max(delta.upper().abs()),
                )?)?;
            }
            for (k, basis) in row.iter().take(index).enumerate() {
                let r = real.get_mut(k).ok_or(Error::SupportOverflow)?;
                *r = r.checked_sub(re.checked_mul(*basis)?)?;
                let i = imag.get_mut(k).ok_or(Error::SupportOverflow)?;
                *i = i.checked_sub(im.checked_mul(*basis)?)?;
            }
        }
        Ok(Conversion {
            source: self.clone(),
            polynomial: Polynomial::new(target, coefficients, self.limits())?,
            coefficient_error_bound: error.upper(),
        })
    }
    /// Explicit quadratic conversion with interval coefficient error evidence.
    /// # Errors
    /// Rejects negative Laurent powers, excessive work/storage or nonfinite arithmetic.
    pub fn to_monomial(&self) -> Result<Conversion<Monomial, B>> {
        if self.basis().offset() < 0 {
            return Err(Error::UnsupportedConversion);
        }
        let offset = usize::try_from(self.basis().offset()).map_err(|_| Error::SupportOverflow)?;
        let count = self
            .coefficients()
            .len()
            .checked_add(offset)
            .ok_or(Error::SupportOverflow)?;
        self.limits().check(count, 10)?;
        if count
            .checked_mul(count)
            .ok_or(Error::Budget("conversion work"))?
            > self.limits().max_work
        {
            return Err(Error::Budget("conversion work"));
        }
        let z = Interval::point(0.0)?;
        let mut previous = vec![z; count];
        let mut current = vec![z; count];
        let mut next = vec![z; count];
        let mut real = vec![z; count];
        let mut imag = vec![z; count];
        if let Some(first) = current.first_mut() {
            *first = Interval::point(1.0)?;
        }
        for (i, c) in self.coefficients().iter().enumerate() {
            if i > 0 {
                let (a, b, d) = self
                    .basis()
                    .interval_recurrence(u32::try_from(i).map_err(|_| Error::SupportOverflow)?)?;
                for (k, n) in next.iter_mut().enumerate() {
                    *n = b
                        .checked_mul(*current.get(k).ok_or(Error::SupportOverflow)?)?
                        .checked_sub(
                            d.checked_mul(*previous.get(k).ok_or(Error::SupportOverflow)?)?,
                        )?;
                    if let Some(left) = k.checked_sub(1).and_then(|j| current.get(j)) {
                        *n = n.checked_add(a.checked_mul(*left)?)?;
                    }
                }
                std::mem::swap(&mut previous, &mut current);
                std::mem::swap(&mut current, &mut next);
            }
            for (k, v) in current
                .iter()
                .take(count.saturating_sub(offset))
                .enumerate()
            {
                let j = k.checked_add(offset).ok_or(Error::SupportOverflow)?;
                let r = real.get_mut(j).ok_or(Error::SupportOverflow)?;
                *r = r.checked_add(v.checked_mul(Interval::point(c.re)?)?)?;
                let r = imag.get_mut(j).ok_or(Error::SupportOverflow)?;
                *r = r.checked_add(v.checked_mul(Interval::point(c.im)?)?)?;
            }
        }
        let mut values = zeros(count, self.limits())?;
        let mut error = z;
        for ((v, r), i) in values.iter_mut().zip(real).zip(imag) {
            v.re = 0.5_f64.mul_add(r.lower(), 0.5 * r.upper());
            v.im = 0.5_f64.mul_add(i.lower(), 0.5 * i.upper());
            let dr = r.checked_sub(Interval::point(v.re)?)?;
            let di = i.checked_sub(Interval::point(v.im)?)?;
            error = error
                .checked_add(Interval::point(dr.lower().abs().max(dr.upper().abs()))?)?
                .checked_add(Interval::point(di.lower().abs().max(di.upper().abs()))?)?;
        }
        Ok(Conversion {
            source: self.clone(),
            polynomial: Polynomial::new(Monomial, values, self.limits())?,
            coefficient_error_bound: error.upper(),
        })
    }
}
impl Polynomial<Chebyshev> {
    /// # Errors
    /// Rejects nonfinite derivative coefficients or budget overflow.
    pub fn derivative(&self) -> Result<Self> {
        let mut c = zeros(self.stored_order(), self.limits())?;
        let mut next = Complex64::new(0.0, 0.0);
        let mut after = next;
        for (k, v) in c.iter_mut().enumerate().rev() {
            let n = k.checked_add(1).ok_or(Error::SupportOverflow)?;
            *v = finite(
                after.add(
                    self.coefficients()
                        .get(n)
                        .copied()
                        .ok_or(Error::SupportOverflow)?
                        .mul(
                            2.0 * f64::from(u32::try_from(n).map_err(|_| Error::SupportOverflow)?),
                        ),
                ),
            )?;
            after = next;
            next = *v;
        }
        if let Some(first) = c.first_mut() {
            *first = first.mul(0.5);
        }
        Self::new(Chebyshev, c, self.limits())
    }
}
impl Polynomial<Monomial> {
    /// # Errors
    /// Rejects nonfinite derivative coefficients.
    pub fn derivative(&self) -> Result<Self> {
        Self::new(Monomial, lowered(self, Ok)?, self.limits())
    }
}
impl Polynomial<Hermite> {
    /// # Errors
    /// Rejects nonfinite derivative coefficients.
    pub fn derivative(&self) -> Result<Self> {
        Self::new(
            *self.basis(),
            lowered(self, |n| Ok(n * self.basis().scale()))?,
            self.limits(),
        )
    }
}
impl Polynomial<Laguerre> {
    /// Differentiate in the same basis with the original alpha parameter.
    /// # Errors
    /// Rejects nonfinite coefficient arithmetic or exceeded storage limits.
    pub fn derivative(&self) -> Result<Self> {
        let mut coefficients = zeros(self.stored_order(), self.limits())?;
        let mut tail = Complex64::new(0.0, 0.0);
        for (index, value) in coefficients.iter_mut().enumerate().rev() {
            tail = finite(
                tail.add(
                    self.coefficients()
                        .get(index.saturating_add(1))
                        .copied()
                        .ok_or(Error::SupportOverflow)?,
                ),
            )?;
            *value = tail.mul(-1.0);
        }
        Self::new(*self.basis(), coefficients, self.limits())
    }
}
impl Polynomial<Jacobi> {
    /// Differentiate in the same basis with both original parameters.
    ///
    /// The cold quadratic recurrence rounds coefficients to binary64. For a
    /// rigorous original-polynomial derivative use `jet_interval` directly.
    /// # Errors
    /// Rejects nonfinite recurrence arithmetic or exceeded resource limits.
    pub fn derivative(&self) -> Result<Self> {
        let degree = self.stored_order();
        self.limits().check(degree, 5)?;
        if degree
            .checked_mul(degree)
            .ok_or(Error::Budget("derivative work"))?
            > self.limits().max_work
        {
            return Err(Error::Budget("derivative work"));
        }
        let mut previous = zeros(degree, self.limits())?;
        let mut current = previous.clone();
        let mut next = previous.clone();
        let mut result = previous.clone();
        for order in 1..self.coefficients().len() {
            let (a, b, c) = self
                .basis()
                .recurrence(u32::try_from(order).map_err(|_| Error::SupportOverflow)?)?;
            for ((out, cur), prev) in next.iter_mut().zip(&current).zip(&previous) {
                *out = finite(cur.mul(b).add(prev.mul(-c)))?;
            }
            for (k, value) in current.iter().take(order.saturating_sub(1)).enumerate() {
                let (scale, shift, back) = self.basis().recurrence(
                    u32::try_from(k.saturating_add(1)).map_err(|_| Error::SupportOverflow)?,
                )?;
                let scaled = finite(value.mul(a / scale))?;
                add_coefficient(&mut next, k.saturating_add(1), scaled)?;
                add_coefficient(&mut next, k, scaled.mul(-shift))?;
                if let Some(k) = k.checked_sub(1) {
                    add_coefficient(&mut next, k, scaled.mul(back))?;
                }
            }
            add_coefficient(&mut next, order.saturating_sub(1), Complex64::new(a, 0.0))?;
            let coefficient = self
                .coefficients()
                .get(order)
                .copied()
                .ok_or(Error::SupportOverflow)?;
            for (out, value) in result.iter_mut().zip(&next) {
                *out = finite(out.add(value.mul(coefficient)))?;
            }
            std::mem::swap(&mut previous, &mut current);
            std::mem::swap(&mut current, &mut next);
        }
        Self::new(*self.basis(), result, self.limits())
    }
}
fn add_coefficient(values: &mut [Complex64], index: usize, value: Complex64) -> Result<()> {
    let out = values.get_mut(index).ok_or(Error::SupportOverflow)?;
    *out = finite(out.add(value))?;
    Ok(())
}
impl Polynomial<Laurent> {
    /// Convert an exactly inversion-symmetric Laurent polynomial to Chebyshev
    /// coefficients in `x=(z+z^-1)/2`, using `z^k+z^-k=2*T_k(x)`.
    ///
    /// Absent support entries are exact zeros. Complex coefficients are allowed;
    /// inversion symmetry means `a_k=a_-k`, without complex conjugation. This is
    /// a change of variable, not evaluation of the Laurent polynomial at x.
    /// The returned l1 coefficient error bounds binary64 rounding in the output.
    /// # Errors
    /// Rejects asymmetry, support/storage/work overflow or nonfinite scaling.
    pub fn to_chebyshev_symmetric(&self) -> Result<Conversion<Chebyshev, Laurent>> {
        let degree = self
            .effective_support()
            .map_or(0, |(a, b)| a.unsigned_abs().max(b.unsigned_abs()));
        let count = usize::try_from(degree)
            .map_err(|_| Error::SupportOverflow)?
            .checked_add(1)
            .ok_or(Error::SupportOverflow)?;
        self.limits().check(count, 4)?;
        if count
            .checked_mul(4)
            .ok_or(Error::Budget("symmetric conversion work"))?
            > self.limits().max_work
        {
            return Err(Error::Budget("symmetric conversion work"));
        }
        let coefficient = |exponent: i64| -> Complex64 {
            exponent
                .checked_sub(i64::from(self.basis().offset()))
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| self.coefficients().get(i))
                .copied()
                .unwrap_or(Complex64::new(0.0, 0.0))
        };
        let mut output = zeros(count, self.limits())?;
        *output.first_mut().ok_or(Error::SupportOverflow)? = coefficient(0);
        let mut error = Interval::point(0.0)?;
        for (k, value) in output.iter_mut().enumerate().skip(1) {
            let exponent = i64::try_from(k).map_err(|_| Error::SupportOverflow)?;
            let a = coefficient(exponent);
            if a != coefficient(exponent.checked_neg().ok_or(Error::SupportOverflow)?) {
                return Err(Error::UnsupportedConversion);
            }
            *value = finite(a.mul(2.0))?;
            for (original, rounded) in [(a.re, value.re), (a.im, value.im)] {
                let delta = Interval::point(original)?
                    .checked_mul(Interval::point(2.0)?)?
                    .checked_sub(Interval::point(rounded)?)?;
                error = error.checked_add(Interval::point(
                    delta.lower().abs().max(delta.upper().abs()),
                )?)?;
            }
        }
        Ok(Conversion {
            source: self.clone(),
            polynomial: Polynomial::new(Chebyshev, output, self.limits())?,
            coefficient_error_bound: error.upper(),
        })
    }
    /// # Errors
    /// Rejects signed support overflow or nonfinite derivative coefficients.
    pub fn derivative(&self) -> Result<Self> {
        let offset = self
            .basis()
            .offset()
            .checked_sub(1)
            .ok_or(Error::SupportOverflow)?;
        let mut c = zeros(self.coefficients().len(), self.limits())?;
        for (i, (out, v)) in c.iter_mut().zip(self.coefficients()).enumerate() {
            let n = self
                .basis()
                .offset()
                .checked_add(i32::try_from(i).map_err(|_| Error::SupportOverflow)?)
                .ok_or(Error::SupportOverflow)?;
            *out = finite(v.mul(f64::from(n)))?;
        }
        Self::new(Laurent::new(offset), c, self.limits())
    }
}
fn lowered<B: Basis>(
    p: &Polynomial<B>,
    scale: impl Fn(f64) -> Result<f64>,
) -> Result<Vec<Complex64>> {
    let mut c = zeros(p.stored_order(), p.limits())?;
    for (i, v) in c.iter_mut().enumerate() {
        let n = i.checked_add(1).ok_or(Error::SupportOverflow)?;
        *v = finite(
            p.coefficients()
                .get(n)
                .copied()
                .ok_or(Error::SupportOverflow)?
                .mul(scale(f64::from(
                    u32::try_from(n).map_err(|_| Error::SupportOverflow)?,
                ))?),
        )?;
    }
    Ok(c)
}

fn basis_rows<B: Basis>(basis: &B, count: usize) -> Result<Vec<Vec<Interval>>> {
    let zero = Interval::point(0.0)?;
    let mut rows: Vec<Vec<Interval>> = Vec::with_capacity(count);
    for degree in 0..count {
        let mut row = vec![zero; count];
        if degree == 0 {
            if let Some(value) = row.first_mut() {
                *value = Interval::point(1.0)?;
            }
        } else {
            let (a, b, c) = basis
                .interval_recurrence(u32::try_from(degree).map_err(|_| Error::SupportOverflow)?)?;
            let previous = rows.last().ok_or(Error::SupportOverflow)?;
            let before = degree.checked_sub(2).and_then(|index| rows.get(index));
            for (index, out) in row.iter_mut().enumerate() {
                let current = previous.get(index).copied().ok_or(Error::SupportOverflow)?;
                *out = b.checked_mul(current)?;
                if let Some(left) = index.checked_sub(1).and_then(|k| previous.get(k)) {
                    *out = out.checked_add(a.checked_mul(*left)?)?;
                }
                if let Some(back) = before.and_then(|r| r.get(index)) {
                    *out = out.checked_sub(c.checked_mul(*back)?)?;
                }
            }
        }
        rows.push(row);
    }
    Ok(rows)
}
