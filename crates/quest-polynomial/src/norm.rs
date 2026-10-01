#![allow(
    clippy::float_cmp,
    reason = "Only exact zero coefficients and exact singleton boundaries are compared"
)]
//! Certified supremum norms of original coefficients and original basis
//! parameters. Branch bounds use directed interval arithmetic; samples only
//! improve a lower bound. A sample maximum is never an upper certificate.
use crate::{Basis, Complex64, Error, Interval, Polynomial, Result};
use std::ops::Mul;

/// A closed real segment or closed complex disc, including its interior.
#[derive(Debug, Clone, Copy)]
pub enum NormDomain {
    RealInterval(Interval),
    Disc { center: Complex64, radius: f64 },
}
#[derive(Debug, Clone, Copy)]
pub struct NormOptions {
    pub absolute_tolerance: f64,
    pub max_cells: usize,
}
impl Default for NormOptions {
    fn default() -> Self {
        Self {
            absolute_tolerance: 1e-7,
            max_cells: 4096,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormStatus {
    Converged,
    /// Valid bounds remain, but the requested tolerance was not reached.
    Budget,
    /// Binary64 endpoints cannot be subdivided further.
    Resolution,
}
/// Source-bound finite supremum enclosure. Construction is private so evidence
/// cannot be retargeted to different coefficients or another domain.
#[derive(Debug, Clone)]
pub struct NormEvidence<B: Basis> {
    source: Polynomial<B>,
    domain: NormDomain,
    bounds: Interval,
    status: NormStatus,
    cells: usize,
}
impl<B: Basis> NormEvidence<B> {
    #[must_use]
    pub const fn source(&self) -> &Polynomial<B> {
        &self.source
    }
    #[must_use]
    pub const fn domain(&self) -> NormDomain {
        self.domain
    }
    #[must_use]
    pub const fn bounds(&self) -> Interval {
        self.bounds
    }
    #[must_use]
    pub const fn status(&self) -> NormStatus {
        self.status
    }
    #[must_use]
    pub const fn cells(&self) -> usize {
        self.cells
    }
}
#[derive(Debug, Clone)]
pub enum NormOutcome<B: Basis> {
    Bounded(NormEvidence<B>),
    /// A nonzero negative Laurent coefficient gives a genuine pole at zero.
    UnboundedPole,
    /// Directed pole-location arithmetic could not resolve a boundary case.
    InconclusivePoleLocation,
}
#[derive(Debug, Clone, Copy)]
struct Rect {
    re: Interval,
    im: Interval,
}
impl Rect {
    fn point(z: Complex64) -> Result<Self> {
        Ok(Self {
            re: Interval::point(z.re)?,
            im: Interval::point(z.im)?,
        })
    }
    fn real(x: Interval) -> Result<Self> {
        Ok(Self {
            re: x,
            im: Interval::point(0.0)?,
        })
    }
    fn add(self, b: Self) -> Result<Self> {
        Ok(Self {
            re: self.re.checked_add(b.re)?,
            im: self.im.checked_add(b.im)?,
        })
    }
    fn sub(self, b: Self) -> Result<Self> {
        Ok(Self {
            re: self.re.checked_sub(b.re)?,
            im: self.im.checked_sub(b.im)?,
        })
    }
    fn mul(self, b: Self) -> Result<Self> {
        Ok(Self {
            re: self
                .re
                .checked_mul(b.re)?
                .checked_sub(self.im.checked_mul(b.im)?)?,
            im: self
                .re
                .checked_mul(b.im)?
                .checked_add(self.im.checked_mul(b.re)?)?,
        })
    }
    fn scale(self, x: Interval) -> Result<Self> {
        Ok(Self {
            re: self.re.checked_mul(x)?,
            im: self.im.checked_mul(x)?,
        })
    }
    fn recip(self) -> Result<Self> {
        let denominator = self.re.square()?.checked_add(self.im.square()?)?;
        Ok(Self {
            re: self.re.checked_div(denominator)?,
            im: self.im.checked_neg()?.checked_div(denominator)?,
        })
    }
    fn modulus(self) -> Result<Interval> {
        // Scale before squaring so a representable modulus of a large or
        // subnormal real/complex payload does not overflow/underflow internally.
        let scale = self
            .re
            .lower()
            .abs()
            .max(self.re.upper().abs())
            .max(self.im.lower().abs())
            .max(self.im.upper().abs());
        if scale == 0.0 {
            return Ok(Interval::point(0.0)?);
        }
        let scale = Interval::point(scale)?;
        let re = self.re.checked_div(scale)?;
        let im = self.im.checked_div(scale)?;
        Ok(re
            .square()?
            .checked_add(im.square()?)?
            .sqrt()?
            .checked_mul(scale)?)
    }
}
fn power(mut x: Rect, mut exponent: u32) -> Result<Rect> {
    let mut y = Rect::point(Complex64::new(1.0, 0.0))?;
    while exponent > 0 {
        if exponent & 1 != 0 {
            y = y.mul(x)?;
        }
        exponent >>= 1;
        if exponent > 0 {
            x = x.mul(x)?;
        }
    }
    Ok(y)
}
fn real_power(mut x: Interval, mut exponent: u32) -> Result<Interval> {
    let mut y = Interval::point(1.0)?;
    while exponent > 0 {
        if exponent & 1 != 0 {
            y = y.checked_mul(x)?;
        }
        exponent >>= 1;
        if exponent > 0 {
            x = x.checked_mul(x)?;
        }
    }
    Ok(y)
}
#[allow(
    clippy::many_single_char_names,
    reason = "Conventional three-term basis recurrence coefficients"
)]
fn evaluate<B: Basis>(p: &Polynomial<B>, x: Rect) -> Result<Rect> {
    let zero = Rect::point(Complex64::new(0.0, 0.0))?;
    let Some((first, last)) = p.effective_support() else {
        return Ok(zero);
    };
    // Trim vanished negative Laurent entries before any reciprocal is taken.
    let skip = if p.basis().offset() < 0 && first >= 0 {
        usize::try_from(p.basis().offset().unsigned_abs()).map_err(|_| Error::SupportOverflow)?
    } else {
        0
    };
    let offset = p
        .basis()
        .offset()
        .checked_add(i32::try_from(skip).map_err(|_| Error::SupportOverflow)?)
        .ok_or(Error::SupportOverflow)?;
    let end = usize::try_from(
        i64::from(last)
            .checked_sub(i64::from(p.basis().offset()))
            .and_then(|v| v.checked_add(1))
            .ok_or(Error::SupportOverflow)?,
    )
    .map_err(|_| Error::SupportOverflow)?;
    let mut previous = zero;
    let mut current = Rect::point(Complex64::new(1.0, 0.0))?;
    let mut sum = zero;
    for (i, coefficient) in p
        .coefficients()
        .get(skip..end)
        .ok_or(Error::SupportOverflow)?
        .iter()
        .enumerate()
    {
        if i > 0 {
            let (a, b, d) = p
                .basis()
                .interval_recurrence(u32::try_from(i).map_err(|_| Error::SupportOverflow)?)?;
            let next = x
                .scale(a)?
                .add(Rect::real(b)?)?
                .mul(current)?
                .sub(previous.scale(d)?)?;
            previous = current;
            current = next;
        }
        if *coefficient != Complex64::new(0.0, 0.0) {
            sum = sum.add(current.mul(Rect::point(*coefficient)?)?)?;
        }
    }
    if offset == 0 {
        return Ok(sum);
    }
    let factor = power(
        if offset < 0 { x.recip()? } else { x },
        offset.unsigned_abs(),
    )?;
    sum.mul(factor)
}
#[derive(Debug, Clone, Copy)]
struct Cell {
    parameter: Interval,
    upper: f64,
}
fn parameter_rect(domain: NormDomain, t: Interval) -> Result<Rect> {
    match domain {
        NormDomain::RealInterval(_) => Rect::real(t),
        NormDomain::Disc { center, radius } => {
            let radius = Interval::point(radius)?;
            Rect::point(center)?.add(Rect {
                re: radius.checked_mul(t.cos()?)?,
                im: radius.checked_mul(t.sin()?)?,
            })
        }
    }
}
fn sample<B: Basis>(p: &Polynomial<B>, domain: NormDomain, t: f64) -> Result<f64> {
    Ok(evaluate(p, parameter_rect(domain, Interval::point(t)?)?)?
        .modulus()?
        .lower())
}
/// Pole-safe whole-domain coefficient bound for a Laurent polynomial. It is
/// also a fallback when a boundary arc's rectangular enclosure touches zero.
fn laurent_bound<B: Basis>(p: &Polynomial<B>, domain: NormDomain) -> Result<f64> {
    let magnitude = match domain {
        NormDomain::Disc { center, radius } => {
            let norm = Rect::point(center)?.modulus()?;
            let r = Interval::point(radius)?;
            Interval::new(
                norm.checked_sub(r)?.lower().max(0.0),
                norm.checked_add(r)?.upper(),
            )?
        }
        NormDomain::RealInterval(x) => Interval::new(
            if x.contains(0.0) {
                0.0
            } else {
                x.lower().abs().min(x.upper().abs())
            },
            x.lower().abs().max(x.upper().abs()),
        )?,
    };
    let mut bound = Interval::point(0.0)?;
    for (i, c) in p.coefficients().iter().enumerate() {
        if *c == Complex64::new(0.0, 0.0) {
            continue;
        }
        let exponent = p
            .basis()
            .offset()
            .checked_add(i32::try_from(i).map_err(|_| Error::SupportOverflow)?)
            .ok_or(Error::SupportOverflow)?;
        let m = if exponent < 0 {
            Interval::point(1.0)?.checked_div(magnitude)?
        } else {
            magnitude
        };
        bound = bound.checked_add(
            Rect::point(*c)?
                .modulus()?
                .checked_mul(real_power(m, exponent.unsigned_abs())?)?,
        )?;
    }
    Ok(bound.upper())
}
impl<B: Basis> Polynomial<B> {
    /// Bound sup |p| on a real interval or an entire closed complex disc.
    /// Disc bounds use the maximum-modulus principle only after excluding all
    /// Laurent poles from the disc; a unit circle Laurent norm is a different
    /// problem from the norm on the unit disc and is not silently substituted.
    ///
    /// All original basis parameters are enclosed. Work and storage admission
    /// uses the polynomial's Limits plus `max_cells`. Budget/resolution exit
    /// retains sound finite bounds with an explicit inconclusive status.
    /// # Errors
    /// Rejects invalid options/discs, exhausted admission or arithmetic for
    /// which a finite enclosing binary64 bound cannot be represented.
    #[allow(
        clippy::too_many_lines,
        reason = "Keep admission, pole evidence and one bounded refinement loop together"
    )]
    pub fn certify_norm(&self, domain: NormDomain, options: NormOptions) -> Result<NormOutcome<B>> {
        if !options.absolute_tolerance.is_finite()
            || options.absolute_tolerance < 0.0
            || options.max_cells == 0
        {
            return Err(Error::Domain);
        }
        let pole = self.effective_support().is_some_and(|(first, _)| first < 0);
        let parameter = match domain {
            NormDomain::RealInterval(x) => {
                if pole && x.contains(0.0) {
                    return Ok(NormOutcome::UnboundedPole);
                }
                x
            }
            NormDomain::Disc { center, radius } => {
                if !radius.is_finite() || radius < 0.0 {
                    return Err(Error::Domain);
                }
                let center_rect = Rect::point(center)?;
                if pole {
                    let distance = center_rect.modulus()?;
                    if distance.upper() <= radius {
                        return Ok(NormOutcome::UnboundedPole);
                    }
                    if distance.lower() <= radius {
                        return Ok(NormOutcome::InconclusivePoleLocation);
                    }
                }
                // 2*pi lies between the adjacent binary64 values of TAU.
                Interval::new(0.0, std::f64::consts::TAU.next_up())?
            }
        };
        let count = self.coefficients().len();
        let work = options
            .max_cells
            .checked_mul(options.max_cells)
            .and_then(|v| {
                options
                    .max_cells
                    .checked_mul(count)?
                    .checked_mul(64)?
                    .checked_add(v)
            })
            .ok_or(Error::Budget("norm work"))?;
        let bytes = options
            .max_cells
            .checked_mul(size_of::<Cell>())
            .and_then(|v| {
                count
                    .checked_mul(size_of::<Complex64>())?
                    .checked_mul(3)?
                    .checked_add(v)
            })
            .ok_or(Error::Budget("norm storage"))?;
        if work > self.limits().max_work
            || bytes > self.limits().max_bytes
            || isize::try_from(bytes).is_err()
        {
            return Err(Error::Budget("norm workspace"));
        }
        let fallback = if pole {
            Some(laurent_bound(self, domain)?)
        } else {
            None
        };
        let cell_upper = |t: Interval| -> Result<f64> {
            match evaluate(self, parameter_rect(domain, t)?).and_then(Rect::modulus) {
                Ok(value) => Ok(value.upper()),
                Err(Error::Interval(quest_numerics::Error::Domain("division")))
                    if fallback.is_some() =>
                {
                    Ok(fallback.unwrap_or(f64::MAX))
                }
                Err(error) => Err(error),
            }
        };
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(options.max_cells)
            .map_err(|_| Error::Budget("norm allocation"))?;
        cells.push(Cell {
            parameter,
            upper: cell_upper(parameter)?,
        });
        let mut lower =
            sample(self, domain, parameter.lower())?.max(sample(self, domain, parameter.upper())?);
        let (upper, status) = loop {
            let (index, cell) = cells
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.upper.total_cmp(&b.upper))
                .map(|(i, c)| (i, *c))
                .ok_or(Error::NotEstablished("empty norm cover"))?;
            let gap = Interval::point(cell.upper)?
                .checked_sub(Interval::point(lower)?)?
                .upper();
            if gap <= options.absolute_tolerance {
                break (cell.upper, NormStatus::Converged);
            }
            if cells.len() >= options.max_cells {
                break (cell.upper, NormStatus::Budget);
            }
            let midpoint = 0.5_f64.mul_add(cell.parameter.lower(), cell.parameter.upper().mul(0.5));
            if midpoint <= cell.parameter.lower() || midpoint >= cell.parameter.upper() {
                break (cell.upper, NormStatus::Resolution);
            }
            lower = lower.max(sample(self, domain, midpoint)?);
            let left = Interval::new(cell.parameter.lower(), midpoint)?;
            let right = Interval::new(midpoint, cell.parameter.upper())?;
            // Clamping to the parent upper cannot lose soundness.
            *cells.get_mut(index).ok_or(Error::SupportOverflow)? = Cell {
                parameter: left,
                upper: cell_upper(left)?.min(cell.upper),
            };
            cells.push(Cell {
                parameter: right,
                upper: cell_upper(right)?.min(cell.upper),
            });
        };
        Ok(NormOutcome::Bounded(NormEvidence {
            source: self.clone(),
            domain,
            bounds: Interval::new(lower.max(0.0), upper)?,
            status,
            cells: cells.len(),
        }))
    }
}
