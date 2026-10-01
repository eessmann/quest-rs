#![allow(
    clippy::float_cmp,
    reason = "Exact endpoint equality classifies an unchanged or exact-zero enclosure"
)]
//! Pure root-preserving interval contractors.
//!
//! Callbacks must enclose the same continuously differentiable function and its
//! derivative/Jacobian on every supplied box. This semantic premise is supplied
//! by the caller; finite intervals alone cannot validate arbitrary callbacks.
//! Empty images mean certified exclusion under that premise. Unchanged images
//! remain inconclusive. All returned branches are intersected with the input.
use crate::{Error, Interval, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractorOutcome {
    Rejected,
    Contracted,
    Split,
    Unchanged,
    CertifiedUnique,
    /// Branch admission prevented a sweep; the original box is retained.
    InconclusiveBudget,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractorMethod {
    ExtendedNewton,
    Krawczyk,
    ScalarHansenSengupta,
    VectorHansenSengupta,
}
/// An immutable contractor image tied to its input interval and algorithm.
#[derive(Debug, Clone)]
pub struct ContractorResult {
    input: Interval,
    images: Vec<Interval>,
    outcome: ContractorOutcome,
    method: ContractorMethod,
}
impl ContractorResult {
    #[must_use]
    pub const fn input(&self) -> Interval {
        self.input
    }
    #[must_use]
    pub fn images(&self) -> &[Interval] {
        &self.images
    }
    #[must_use]
    pub const fn outcome(&self) -> ContractorOutcome {
        self.outcome
    }
    #[must_use]
    pub const fn method(&self) -> ContractorMethod {
        self.method
    }
}
/// A single sequential Gauss--Seidel sweep, with explicit split-branch admission.
#[derive(Debug, Clone)]
pub struct VectorContractorResult {
    input: Vec<Interval>,
    images: Vec<Vec<Interval>>,
    outcome: ContractorOutcome,
}
impl VectorContractorResult {
    #[must_use]
    pub fn input(&self) -> &[Interval] {
        &self.input
    }
    #[must_use]
    pub fn images(&self) -> &[Vec<Interval>] {
        &self.images
    }
    #[must_use]
    pub const fn outcome(&self) -> ContractorOutcome {
        self.outcome
    }
    #[must_use]
    pub const fn method(&self) -> ContractorMethod {
        ContractorMethod::VectorHansenSengupta
    }
}
/// Bounds accounted wrapper-owned heap buffers and nested Vec metadata, and
/// modeled arithmetic work.
///
/// Fixed local stack values and allocator bookkeeping
/// are outside the byte model. Buffers reserve their admitted capacities exactly
/// and fallibly, without geometric growth. Callback allocations/work are the
/// caller's responsibility. No iteration or recursion is implicit.
#[derive(Debug, Clone, Copy)]
pub struct ContractorLimits {
    pub max_dimension: usize,
    pub max_boxes: usize,
    pub max_bytes: usize,
    pub max_work: usize,
}
impl Default for ContractorLimits {
    fn default() -> Self {
        Self {
            max_dimension: 128,
            max_boxes: 1024,
            max_bytes: 16_777_216,
            max_work: 67_108_864,
        }
    }
}
fn at<T>(values: &[T], index: usize) -> Result<&T> {
    values.get(index).ok_or(Error::Length("contractor shape"))
}
fn at_mut<T>(values: &mut [T], index: usize) -> Result<&mut T> {
    values
        .get_mut(index)
        .ok_or(Error::Length("contractor shape"))
}
fn exact_vec<T>(capacity: usize) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| Error::Allocation)?;
    Ok(values)
}
fn filled<T: Clone>(count: usize, value: T) -> Result<Vec<T>> {
    let mut values = exact_vec(count)?;
    values.resize(count, value);
    Ok(values)
}
fn copy_intervals(values: &[Interval]) -> Result<Vec<Interval>> {
    let mut output = exact_vec(values.len())?;
    output.extend_from_slice(values);
    Ok(output)
}
fn point(x: f64) -> Result<Interval> {
    Interval::point(x)
}
fn intersection(a: Interval, b: Interval) -> Result<Option<Interval>> {
    let lower = a.lower().max(b.lower());
    let upper = a.upper().min(b.upper());
    if lower > upper {
        Ok(None)
    } else {
        Ok(Some(Interval::new(lower, upper)?))
    }
}
fn same(a: Interval, b: Interval) -> bool {
    a.lower() == b.lower() && a.upper() == b.upper()
}
fn strict(a: Interval, b: Interval) -> bool {
    a.lower() > b.lower() && a.upper() < b.upper()
}
fn center_checked(input: Interval, center: f64) -> Result<Interval> {
    if !input.contains(center) {
        return Err(Error::Domain("contractor center"));
    }
    point(center)
}
/// Enclose N/D intersected with a finite displacement domain, including the
/// two disjoint branches when D straddles zero. No artificial tiny divisor is
/// substituted for zero, and no infinite endpoint enters the public interval.
fn extended_divide(mut n: Interval, mut d: Interval, domain: Interval) -> Result<Vec<Interval>> {
    let mut images = exact_vec(2)?;
    if !d.contains(0.0) {
        if let Some(image) = intersection(n.checked_div(d)?, domain)? {
            images.push(image);
        }
        return Ok(images);
    }
    if n.contains(0.0) {
        images.push(domain);
        return Ok(images);
    }
    if n.upper() < 0.0 {
        n = n.checked_neg()?;
        d = d.checked_neg()?;
    }
    if d.lower() < 0.0 {
        let upper = point(n.lower())?
            .checked_div(point(d.lower())?)?
            .upper()
            .min(domain.upper());
        if upper >= domain.lower() {
            images.push(Interval::new(domain.lower(), upper)?);
        }
    }
    if d.upper() > 0.0 {
        let lower = point(n.lower())?
            .checked_div(point(d.upper())?)?
            .lower()
            .max(domain.lower());
        if lower <= domain.upper() {
            images.push(Interval::new(lower, domain.upper())?);
        }
    }
    Ok(images)
}
#[allow(
    clippy::needless_pass_by_value,
    reason = "Own the immutable branch payload"
)]
fn scalar_result(
    input: Interval,
    images: Vec<Interval>,
    unique: bool,
    method: ContractorMethod,
) -> ContractorResult {
    let outcome = if images.is_empty() {
        ContractorOutcome::Rejected
    } else if unique {
        ContractorOutcome::CertifiedUnique
    } else if images.len() > 1 {
        ContractorOutcome::Split
    } else if images.first().is_some_and(|image| same(*image, input)) {
        ContractorOutcome::Unchanged
    } else {
        ContractorOutcome::Contracted
    };
    ContractorResult {
        input,
        images,
        outcome,
        method,
    }
}
fn newton_image(
    input: Interval,
    center: Interval,
    value: Interval,
    derivative: Interval,
) -> Result<Vec<Interval>> {
    let displacement = input.checked_sub(center)?;
    extended_divide(value.checked_neg()?, derivative, displacement)?
        .into_iter()
        .map(|v| intersection(v.checked_add(center)?, input))
        .collect::<Result<Vec<_>>>()
        .map(|images| images.into_iter().flatten().collect())
}
/// Extended interval Newton at a caller-chosen center. A derivative containing
/// zero may split into two images; this does not certify uniqueness.
/// # Errors
/// Propagates callback/domain/nonfinite arithmetic errors.
pub fn extended_newton(
    input: Interval,
    center: f64,
    function: impl Fn(Interval) -> Result<Interval>,
    derivative: impl Fn(Interval) -> Result<Interval>,
) -> Result<ContractorResult> {
    let center = center_checked(input, center)?;
    let value = function(center)?;
    let d = derivative(input)?;
    let images = newton_image(input, center, value, d)?;
    let unique = !d.contains(0.0)
        && images.len() == 1
        && (images.first().is_some_and(|image| strict(*image, input))
            || (value.lower() == 0.0 && value.upper() == 0.0));
    Ok(scalar_result(
        input,
        images,
        unique,
        ContractorMethod::ExtendedNewton,
    ))
}
/// Krawczyk image c-C*f(c)+(1-C*f'(X))*(X-c). Strict inclusion
/// certifies existence and uniqueness only with a contraction factor below one.
/// # Errors
/// Rejects an invalid center/preconditioner or failed enclosing arithmetic.
pub fn krawczyk(
    input: Interval,
    center: f64,
    function: impl Fn(Interval) -> Result<Interval>,
    derivative: impl Fn(Interval) -> Result<Interval>,
    preconditioner: f64,
) -> Result<ContractorResult> {
    let center = center_checked(input, center)?;
    let c = point(preconditioner)?;
    let value = function(center)?;
    let slope = point(1.0)?.checked_sub(c.checked_mul(derivative(input)?)?)?;
    let image = center
        .checked_sub(c.checked_mul(value)?)?
        .checked_add(slope.checked_mul(input.checked_sub(center)?)?)?;
    let images: Vec<_> = intersection(image, input)?.into_iter().collect();
    let unique = preconditioner != 0.0
        && slope.lower().abs().max(slope.upper().abs()) < 1.0
        && (strict(image, input) || (value.lower() == 0.0 && value.upper() == 0.0));
    Ok(scalar_result(
        input,
        images,
        unique,
        ContractorMethod::Krawczyk,
    ))
}
/// Scalar Hansen--Sengupta: extended division of -C*f(c) by C*f'(X).
/// A zero preconditioner deliberately returns an inconclusive unchanged image.
/// # Errors
/// Rejects an invalid center/preconditioner or failed enclosing arithmetic.
pub fn scalar_hansen_sengupta(
    input: Interval,
    center: f64,
    function: impl Fn(Interval) -> Result<Interval>,
    derivative: impl Fn(Interval) -> Result<Interval>,
    preconditioner: f64,
) -> Result<ContractorResult> {
    let center = center_checked(input, center)?;
    let c = point(preconditioner)?;
    let value = c.checked_mul(function(center)?)?;
    let d = c.checked_mul(derivative(input)?)?;
    let images = newton_image(input, center, value, d)?;
    let unique = preconditioner != 0.0
        && !d.contains(0.0)
        && images.len() == 1
        && (images.first().is_some_and(|image| strict(*image, input))
            || (value.lower() == 0.0 && value.upper() == 0.0));
    Ok(scalar_result(
        input,
        images,
        unique,
        ContractorMethod::ScalarHansenSengupta,
    ))
}
fn admission(n: usize, limits: ContractorLimits) -> Result<()> {
    if n == 0 {
        return Err(Error::Length("zero contractor dimension"));
    }
    let square = n.checked_mul(n).ok_or(Error::Overflow)?;
    let work = square
        .checked_mul(n)
        .and_then(|v| v.checked_mul(4))
        .and_then(|v| {
            square
                .checked_mul(limits.max_boxes)?
                .checked_mul(4)?
                .checked_add(v)
        })
        .ok_or(Error::Overflow)?;
    // c,b and two possible retained input copies coexist with two branch
    // generations. The two-element extended-division scratch also stays live
    // while a child is inserted. Nested headers include a's rows, both branch
    // generation lists and the one-image budget-fallback result.
    let interval_cells = square
        .checked_add(n.checked_mul(4).ok_or(Error::Overflow)?)
        .and_then(|v| {
            n.checked_mul(limits.max_boxes)?
                .checked_mul(2)?
                .checked_add(v)
        })
        .and_then(|v| v.checked_add(2))
        .ok_or(Error::Overflow)?;
    let headers = n
        .checked_add(limits.max_boxes.checked_mul(2).ok_or(Error::Overflow)?)
        .and_then(|v| v.checked_add(1))
        .ok_or(Error::Overflow)?;
    let bytes = interval_cells
        .checked_mul(size_of::<Interval>())
        .and_then(|v| {
            headers
                .checked_mul(size_of::<Vec<Interval>>())?
                .checked_add(v)
        })
        .ok_or(Error::Overflow)?;
    if isize::try_from(bytes).is_err() {
        return Err(Error::Overflow);
    }
    for (resource, requested, limit) in [
        ("contractor dimension", n, limits.max_dimension),
        ("contractor work", work, limits.max_work),
        ("contractor bytes", bytes, limits.max_bytes),
    ] {
        crate::policy::check_limit(resource, requested, limit)?;
    }
    if limits.max_boxes == 0 {
        return Err(Error::Length("zero contractor branches"));
    }
    Ok(())
}
/// Dynamic square vector Hansen--Sengupta using one root-preserving sequential
/// Gauss--Seidel sweep on R*J(X)*(x-c)=-R*f(c). Every extended-division branch
/// is retained.
///
/// If branch admission fails the original box is returned.
///
/// `CertifiedUnique` additionally requires an independently verified Krawczyk
/// strict inclusion and infinity-norm contraction bound below one. This avoids
/// interpreting a singular Gauss--Seidel contraction as existence evidence.
/// # Errors
/// Rejects zero/mismatched dimensions, resource requests, nonfinite
/// preconditioners, failed callbacks or enclosing arithmetic.
#[allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "Conventional names in one bounded interval linear algebra sweep"
)]
pub fn vector_hansen_sengupta(
    input: &[Interval],
    center: &[f64],
    function: impl Fn(&[Interval]) -> Result<Vec<Interval>>,
    jacobian: impl Fn(&[Interval]) -> Result<Vec<Vec<Interval>>>,
    preconditioner: &[Vec<f64>],
    limits: ContractorLimits,
) -> Result<VectorContractorResult> {
    let n = input.len();
    admission(n, limits)?;
    if center.len() != n
        || preconditioner.len() != n
        || preconditioner.iter().any(|row| row.len() != n)
    {
        return Err(Error::Length("contractor shape"));
    }
    let mut c = exact_vec(n)?;
    for (x, center) in input.iter().zip(center) {
        c.push(center_checked(*x, *center)?);
    }
    for row in preconditioner {
        for v in row {
            point(*v)?;
        }
    }
    let f = function(&c)?;
    let j = jacobian(input)?;
    if f.len() != n || j.len() != n || j.iter().any(|row| row.len() != n) {
        return Err(Error::Length("contractor callback shape"));
    }
    let zero = point(0.0)?;
    let mut a = exact_vec(n)?;
    for _ in 0..n {
        a.push(filled(n, zero)?);
    }
    let mut b = filled(n, zero)?;
    for ((row, bvalue), p_row) in a.iter_mut().zip(&mut b).zip(preconditioner) {
        for ((r, jrow), fvalue) in p_row.iter().zip(&j).zip(&f) {
            let r = point(*r)?;
            *bvalue = bvalue.checked_sub(r.checked_mul(*fvalue)?)?;
            for (out, jvalue) in row.iter_mut().zip(jrow) {
                *out = out.checked_add(r.checked_mul(*jvalue)?)?;
            }
        }
    }
    let mut displacement = exact_vec(n)?;
    for (x, c) in input.iter().zip(&c) {
        displacement.push(x.checked_sub(*c)?);
    }
    // Independent sufficient existence/uniqueness gate on the original box.
    let mut strict_inclusion = true;
    let mut contraction = true;
    for i in 0..n {
        let mut image = at(&c, i)?.checked_add(*at(&b, i)?)?;
        let mut row_norm = zero;
        for k in 0..n {
            let m = point(if i == k { 1.0 } else { 0.0 })?.checked_sub(*at(at(&a, i)?, k)?)?;
            image = image.checked_add(m.checked_mul(*at(&displacement, k)?)?)?;
            row_norm = row_norm.checked_add(point(m.lower().abs().max(m.upper().abs()))?)?;
        }
        strict_inclusion &= strict(image, *at(input, i)?);
        contraction &= row_norm.upper() < 1.0;
    }
    let mut branches = exact_vec(limits.max_boxes)?;
    branches.push(displacement);
    for i in 0..n {
        let mut next = exact_vec(limits.max_boxes)?;
        for branch in &branches {
            let mut residual = *at(&b, i)?;
            for k in 0..n {
                if k != i {
                    residual =
                        residual.checked_sub(at(at(&a, i)?, k)?.checked_mul(*at(branch, k)?)?)?;
                }
            }
            let images = extended_divide(residual, *at(at(&a, i)?, i)?, *at(branch, i)?)?;
            if next
                .len()
                .checked_add(images.len())
                .ok_or(Error::Overflow)?
                > limits.max_boxes
            {
                let mut images = exact_vec(1)?;
                images.push(copy_intervals(input)?);
                return Ok(VectorContractorResult {
                    input: copy_intervals(input)?,
                    images,
                    outcome: ContractorOutcome::InconclusiveBudget,
                });
            }
            for image in images {
                let mut child = copy_intervals(branch)?;
                *at_mut(&mut child, i)? = image;
                next.push(child);
            }
        }
        branches = next;
    }
    // Reuse the final branch buffers for absolute coordinates rather than
    // keeping a second complete output generation live during translation.
    for branch in &mut branches {
        let mut valid = true;
        for ((value, center), parent) in branch.iter_mut().zip(&c).zip(input) {
            if let Some(x) = intersection(value.checked_add(*center)?, *parent)? {
                *value = x;
            } else {
                valid = false;
                break;
            }
        }
        if !valid {
            branch.clear();
        }
    }
    branches.retain(|branch| !branch.is_empty());
    let images = branches;
    let outcome = if images.is_empty() {
        ContractorOutcome::Rejected
    } else if strict_inclusion && contraction {
        ContractorOutcome::CertifiedUnique
    } else if images.len() > 1 {
        ContractorOutcome::Split
    } else if images
        .first()
        .is_some_and(|image| image.iter().zip(input).all(|(a, b)| same(*a, *b)))
    {
        ContractorOutcome::Unchanged
    } else {
        ContractorOutcome::Contracted
    };
    Ok(VectorContractorResult {
        input: copy_intervals(input)?,
        images,
        outcome,
    })
}
