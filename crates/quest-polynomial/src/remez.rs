use crate::{Chebyshev, Complex64, Error, Function, Interval, Jet, Limits, Polynomial, Result};
use faer::{
    Mat, Par,
    dyn_stack::{MemBuffer, MemStack},
    linalg::qr::col_pivoting::{factor, solve},
};

/// Cold approximation resource and accuracy controls.
#[derive(Clone, Copy, Debug)]
pub struct RemezOptions {
    pub degree: usize,
    pub max_iterations: usize,
    pub max_subdivisions: usize,
    pub root_width: f64,
    pub tolerance: f64,
    pub limits: Limits,
}
impl Default for RemezOptions {
    fn default() -> Self {
        Self {
            degree: 8,
            max_iterations: 30,
            max_subdivisions: 32768,
            root_width: 1e-10,
            tolerance: 1e-8,
            limits: Limits::default(),
        }
    }
}
#[derive(Debug)]
pub struct MissingTarget;
#[derive(Debug)]
pub struct HasTarget {
    target: Function,
}
#[derive(Debug)]
pub struct ReadyRemez {
    target: Function,
    domain: Interval,
}
/// Required target and domain are supplied through owning state transitions.
///
/// The polynomial is expressed in the original real variable, using `T_k(x)`.
/// Its bound applies only on the supplied finite closed interval. The target
/// must have finite interval enclosures for its first two derivatives there.
///
/// ```
/// use quest_polynomial::{function, Interval, RemezBuilder};
/// # fn example() -> quest_polynomial::Result<()> {
/// let result = RemezBuilder::new().target(function!(|x| x.exp()))
///     .degree(3).tolerance(1e-8)
///     .domain(Interval::new(-1.0, 1.0)?)?.run()?;
/// assert!(result.error_bound().upper() < 0.006);
/// # Ok(())
/// # }
/// # example().unwrap();
/// ```
///
/// ```compile_fail
/// use quest_polynomial::RemezBuilder;
/// let result = RemezBuilder::new().degree(3).run(); // target/domain missing
/// ```
#[derive(Debug)]
pub struct RemezBuilder<S = MissingTarget> {
    state: S,
    options: RemezOptions,
}
impl Default for RemezBuilder {
    fn default() -> Self {
        Self::new()
    }
}
impl RemezBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: MissingTarget,
            options: RemezOptions::default(),
        }
    }
    #[must_use]
    pub const fn target(self, target: Function) -> RemezBuilder<HasTarget> {
        RemezBuilder {
            state: HasTarget { target },
            options: self.options,
        }
    }
}
impl RemezBuilder<HasTarget> {
    /// # Errors
    /// Rejects an empty-width domain or a target undefined anywhere in the interval.
    pub fn domain(self, domain: Interval) -> Result<RemezBuilder<ReadyRemez>> {
        if domain.lower() >= domain.upper() {
            return Err(Error::Domain);
        }
        self.state.target.evaluate_interval(domain)?;
        Ok(RemezBuilder {
            state: ReadyRemez {
                target: self.state.target,
                domain,
            },
            options: self.options,
        })
    }
}
impl<S> RemezBuilder<S> {
    #[must_use]
    pub const fn degree(mut self, degree: usize) -> Self {
        self.options.degree = degree;
        self
    }
    #[must_use]
    pub const fn tolerance(mut self, tolerance: f64) -> Self {
        self.options.tolerance = tolerance;
        self
    }
    #[must_use]
    pub const fn limits(mut self, limits: Limits) -> Self {
        self.options.limits = limits;
        self
    }
    #[must_use]
    pub const fn options(mut self, options: RemezOptions) -> Self {
        self.options = options;
        self
    }
}
impl RemezBuilder<ReadyRemez> {
    /// # Errors
    /// Rejects invalid configuration, failed solves/exchanges, exhausted budgets, or insufficient enclosures.
    pub fn run(self) -> Result<RemezResult> {
        remez_impl(&self.state.target, self.state.domain, self.options)
    }
}
/// An admitted approximation with an interval upper bound for the actual fixed
/// binary64 polynomial and an alternation lower bound for the minimax error.
///
/// Successful construction establishes their gap is at most the requested tolerance.
#[derive(Debug, Clone)]
pub struct RemezResult {
    polynomial: Polynomial<Chebyshev>,
    error_bound: Interval,
    minimax_lower_bound: f64,
    iterations: usize,
    domain: Interval,
}
impl RemezResult {
    #[must_use]
    pub const fn polynomial(&self) -> &Polynomial<Chebyshev> {
        &self.polynomial
    }
    #[must_use]
    pub const fn error_bound(&self) -> Interval {
        self.error_bound
    }
    #[must_use]
    pub const fn minimax_lower_bound(&self) -> f64 {
        self.minimax_lower_bound
    }
    #[must_use]
    pub const fn iterations(&self) -> usize {
        self.iterations
    }
    #[must_use]
    pub const fn domain(&self) -> Interval {
        self.domain
    }
    #[must_use]
    pub fn into_polynomial(self) -> Polynomial<Chebyshev> {
        self.polynomial
    }
}
/// A stationary-point box with separately established existence and uniqueness.
#[derive(Debug, Clone, Copy)]
/// `unique` establishes at most one root; `exists` establishes at least one.
pub struct CriticalPoint {
    pub interval: Interval,
    pub exists: bool,
    pub unique: bool,
}
/// Every stationary point lies in one of these boxes. Boxes can overlap at
/// subdivision endpoints; unresolved boxes are not asserted to contain roots.
#[derive(Debug, Clone)]
pub struct CriticalPoints {
    pub boxes: Vec<CriticalPoint>,
    pub examined: usize,
}
/// # Errors
/// Rejects invalid configuration, exhausted subdivision budgets or undefined derivatives.
pub fn isolate_critical_points(
    function: &Function,
    domain: Interval,
    width: f64,
    max_subdivisions: usize,
) -> Result<CriticalPoints> {
    isolate(
        |x| function.jet_interval(x),
        domain,
        width,
        max_subdivisions,
    )
}
fn midpoint(x: Interval) -> f64 {
    0.5_f64.mul_add(x.lower(), 0.5 * x.upper())
}
fn opposite(a: Interval, b: Interval) -> bool {
    (a.upper() < 0.0 && b.lower() > 0.0) || (a.lower() > 0.0 && b.upper() < 0.0)
}
fn isolate(
    jet: impl Fn(Interval) -> Result<Jet<Interval>>,
    domain: Interval,
    width: f64,
    budget: usize,
) -> Result<CriticalPoints> {
    if !width.is_finite() || width <= 0.0 || budget == 0 {
        return Err(Error::Domain);
    }
    let mut pending = vec![domain];
    let mut boxes = Vec::new();
    let mut examined = 0_usize;
    while let Some(interval) = pending.pop() {
        examined = examined.checked_add(1).ok_or(Error::Budget("root work"))?;
        if examined > budget {
            return Err(Error::NotEstablished("stationary-point subdivision budget"));
        }
        let j = jet(interval)?;
        if !j.first.contains(0.0) {
            continue;
        }
        let middle = midpoint(interval);
        if interval.upper() - interval.lower() <= width
            || middle <= interval.lower()
            || middle >= interval.upper()
        {
            let a = jet(Interval::point(interval.lower())?)?.first;
            let b = jet(Interval::point(interval.upper())?)?.first;
            let exists = opposite(a, b)
                || (a.lower() == 0.0 && a.upper() == 0.0)
                || (b.lower() == 0.0 && b.upper() == 0.0);
            boxes.push(CriticalPoint {
                interval,
                exists,
                unique: !j.second.contains(0.0),
            });
            continue;
        }
        if j.first.lower() == 0.0 && j.first.upper() == 0.0 {
            boxes.push(CriticalPoint {
                interval,
                exists: true,
                unique: false,
            });
            continue;
        }
        pending.push(Interval::new(middle, interval.upper())?);
        pending.push(Interval::new(interval.lower(), middle)?);
    }
    Ok(CriticalPoints { boxes, examined })
}
fn residual_jet(f: &Function, p: &Polynomial<Chebyshev>, x: Interval) -> Result<Jet<Interval>> {
    let f = f.jet_interval(x)?;
    let p = p.jet_interval(x)?;
    Ok(Jet {
        value: f.value.checked_sub(p.value)?,
        first: f.first.checked_sub(p.first)?,
        second: f.second.checked_sub(p.second)?,
    })
}
fn residual(f: &Function, p: &Polynomial<Chebyshev>, x: f64) -> Result<f64> {
    let v = f.evaluate(x)? - p.evaluate_real(x)?;
    if !v.is_finite() {
        return Err(Error::NonFinite);
    }
    Ok(v)
}
fn magnitude(x: Interval) -> f64 {
    x.lower().abs().max(x.upper().abs())
}
fn bound(
    f: &Function,
    p: &Polynomial<Chebyshev>,
    domain: Interval,
    roots: &CriticalPoints,
) -> Result<f64> {
    let mut bound = 0.0_f64;
    for x in [domain.lower(), domain.upper()] {
        bound = bound.max(magnitude(residual_jet(f, p, Interval::point(x)?)?.value));
    }
    for root in &roots.boxes {
        // Both the direct enclosure and centered mean-value enclosure are valid.
        let center = midpoint(root.interval);
        let j = residual_jet(f, p, root.interval)?;
        let at_center = residual_jet(f, p, Interval::point(center)?)?.value;
        let delta = root.interval.checked_sub(Interval::point(center)?)?;
        let centered = at_center.checked_add(j.first.checked_mul(delta)?)?;
        bound = bound.max(magnitude(j.value).min(magnitude(centered)));
    }
    Ok(bound)
}
fn cheb(x: f64, degree: usize) -> f64 {
    let mut previous = 1.0;
    let mut current = x;
    if degree == 0 {
        return previous;
    }
    for _ in 1..degree {
        let next = (2.0 * x).mul_add(current, -previous);
        previous = current;
        current = next;
    }
    current
}
fn solve_reference(
    f: &Function,
    reference: &[f64],
    options: RemezOptions,
) -> Result<Polynomial<Chebyshev>> {
    let n = reference.len();
    let degree = options.degree;
    let par = Par::Seq;
    let mut matrix = Mat::from_fn(n, n, |i, j| {
        if j > degree {
            if i % 2 == 0 { 1.0 } else { -1.0 }
        } else {
            cheb(reference.get(i).copied().unwrap_or(f64::NAN), j)
        }
    });
    let mut rhs = Mat::zeros(n, 1);
    for (i, x) in reference.iter().enumerate() {
        *rhs.get_mut(i, 0) = f.evaluate(*x)?;
    }
    let mut forward = vec![0_usize; n];
    let mut backward = vec![0_usize; n];
    let mut coefficients = Mat::zeros(1, n);
    let req = factor::qr_in_place_scratch::<usize, f64>(n, n, 1, par, faer::Spec::default());
    let mut scratch = MemBuffer::new(req);
    let (_, permutation) = factor::qr_in_place(
        matrix.as_mut(),
        coefficients.as_mut(),
        &mut forward,
        &mut backward,
        par,
        MemStack::new(&mut scratch),
        faer::Spec::default(),
    );
    let scale = matrix.get(0, 0).abs();
    for i in 0..n {
        let diagonal = *matrix.get(i, i);
        if !diagonal.is_finite() || diagonal.abs() <= scale * 1e-14 {
            return Err(Error::NotEstablished("rank deficient alternation system"));
        }
    }
    let mut scratch = MemBuffer::new(solve::solve_in_place_scratch::<usize, f64>(n, 1, 1, par));
    solve::solve_in_place(
        matrix.as_ref(),
        coefficients.as_ref(),
        matrix.as_ref(),
        permutation,
        rhs.as_mut(),
        par,
        MemStack::new(&mut scratch),
    );
    let count = degree.checked_add(1).ok_or(Error::SupportOverflow)?;
    let values = (0..count)
        .map(|i| Complex64::new(*rhs.get(i, 0), 0.0))
        .collect();
    let p = Polynomial::new(Chebyshev, values, options.limits)?;
    for (i, x) in reference.iter().enumerate() {
        let sign: f64 = if i % 2 == 0 { 1.0 } else { -1.0 };
        let r = sign.mul_add(*rhs.get(n.saturating_sub(1), 0), p.evaluate_real(*x)?)
            - f.evaluate(*x)?;
        if !r.is_finite() || r.abs() > options.tolerance * 0.1 {
            return Err(Error::NotEstablished("alternation solve residual"));
        }
    }
    Ok(p)
}
/// Convenience adapter to the stateful builder.
/// # Errors
/// Same mathematical, numerical and resource failures as `RemezBuilder::run`.
pub fn remez(target: &Function, domain: Interval, options: RemezOptions) -> Result<RemezResult> {
    RemezBuilder::new()
        .target(target.clone())
        .options(options)
        .domain(domain)?
        .run()
}
fn remez_impl(f: &Function, domain: Interval, options: RemezOptions) -> Result<RemezResult> {
    let count = options
        .degree
        .checked_add(2)
        .ok_or(Error::Budget("degree"))?;
    options.limits.check(count, 10)?;
    let work = count
        .checked_mul(count)
        .and_then(|n| n.checked_mul(count))
        .and_then(|n| n.checked_mul(options.max_iterations))
        .ok_or(Error::Budget("Remez work"))?;
    if work > options.limits.max_work
        || count
            .checked_mul(count)
            .and_then(|n| n.checked_mul(32))
            .ok_or(Error::Budget("Remez storage"))?
            > options.limits.max_bytes
    {
        return Err(Error::Budget("Remez workspace"));
    }
    if !options.tolerance.is_finite() || options.tolerance <= 0.0 || options.max_iterations == 0 {
        return Err(Error::Domain);
    }
    let divisor =
        f64::from(u32::try_from(count.saturating_sub(1)).map_err(|_| Error::Budget("degree"))?);
    let center = midpoint(domain);
    let radius = 0.5_f64.mul_add(-domain.lower(), 0.5 * domain.upper());
    let mut reference = Vec::with_capacity(count);
    for i in 0..count {
        let theta = std::f64::consts::PI
            * f64::from(u32::try_from(i).map_err(|_| Error::SupportOverflow)?)
            / divisor;
        reference.push((-radius).mul_add(theta.cos(), center));
    }
    if let Some(x) = reference.first_mut() {
        *x = domain.lower();
    }
    if let Some(x) = reference.last_mut() {
        *x = domain.upper();
    }
    for iteration in 0..options.max_iterations {
        let p = solve_reference(f, &reference, options)?;
        let roots = isolate(
            |x| residual_jet(f, &p, x),
            domain,
            options.root_width,
            options.max_subdivisions,
        )?;
        let upper = bound(f, &p, domain, &roots)?;
        let mut points = vec![domain.lower()];
        points.extend(roots.boxes.iter().map(|root| midpoint(root.interval)));
        points.push(domain.upper());
        points.sort_by(f64::total_cmp);
        points.dedup();
        let mut extrema: Vec<(f64, f64)> = Vec::new();
        for x in points {
            let value = residual(f, &p, x)?;
            if let Some(last) = extrema.last_mut()
                && last.1.is_sign_positive() == value.is_sign_positive()
            {
                if value.abs() > last.1.abs() {
                    *last = (x, value);
                }
                continue;
            }
            extrema.push((x, value));
        }
        let (lower, best) = alternation_bound(f, &p, &extrema, count)?;
        if Interval::point(upper)?
            .checked_sub(Interval::point(lower)?)?
            .upper()
            <= options.tolerance
        {
            return Ok(RemezResult {
                polynomial: p,
                error_bound: Interval::new(0.0, upper)?,
                minimax_lower_bound: lower,
                iterations: iteration.saturating_add(1),
                domain,
            });
        }
        let start = best.ok_or(Error::NotEstablished("insufficient strict alternation"))?;
        reference.clear();
        reference.extend(extrema.iter().skip(start).take(count).map(|(x, _)| *x));
    }
    Err(Error::NotEstablished("Remez iteration budget"))
}

fn alternation_bound(
    f: &Function,
    p: &Polynomial<Chebyshev>,
    extrema: &[(f64, f64)],
    count: usize,
) -> Result<(f64, Option<usize>)> {
    let mut lower = 0.0_f64;
    let mut best = None;
    for (start, window) in extrema.windows(count).enumerate() {
        let mut candidate = f64::INFINITY;
        let mut previous = None;
        for (x, _) in window {
            let enclosure = residual_jet(f, p, Interval::point(*x)?)?.value;
            let (sign, amplitude) = if enclosure.lower() > 0.0 {
                (true, enclosure.lower())
            } else if enclosure.upper() < 0.0 {
                (false, -enclosure.upper())
            } else {
                candidate = 0.0;
                break;
            };
            if previous == Some(sign) {
                candidate = 0.0;
                break;
            }
            previous = Some(sign);
            candidate = candidate.min(amplitude);
        }
        if candidate > lower {
            lower = candidate;
            best = Some(start);
        }
    }

    Ok((lower, best))
}
