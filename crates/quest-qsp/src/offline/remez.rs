//! arbitrary-precision exchange candidates followed by a proof for the frozen binary64 polynomial.
//! The error enclosure proves uniform approximation on the supplied real interval;
//! finite derivative sampling used during exchange is not a minimax certificate.
use super::{
    Context, OfflineError, OfflinePolicy, OfflineResult, modeled_scalar_bytes,
    number::{add, bits, div, mul, neg, negative, positive, sqrt, sub, validate},
};
use crate::precision::{BinaryRounding, checked, exact_from_f64, to_f64};
use astro_float::BigFloat;
use quest_polynomial::{
    Chebyshev, Complex64, Expr, ExprNode, Function, Interval, Limits, Polynomial,
};
use std::time::{Duration, Instant};

/// Precision, numerical exchange controls and final uniform-error requirements.
#[derive(Clone, Copy, Debug)]
pub struct OfflineRemezPolicy {
    /// Arbitrary-precision computation policy used for approximation attempts.
    pub offline: OfflinePolicy,
    /// Maximum proved total uniform error of the exported binary64 polynomial.
    pub error_tolerance: f64,
    /// Numerical exchange stopping tolerance; this is not a certified minimax gap.
    pub exchange_tolerance: f64,
    /// Maximum alternation iterations in one precision attempt.
    pub max_iterations: usize,
    /// Maximum subdivisions for the final full-domain error enclosure.
    pub max_subdivisions: usize,
}
impl Default for OfflineRemezPolicy {
    fn default() -> Self {
        Self {
            offline: OfflinePolicy::default(),
            error_tolerance: 1e-8,
            exchange_tolerance: 1e-24,
            max_iterations: 64,
            max_subdivisions: 262_144,
        }
    }
}
/// Approximation builder state before an original function is supplied.
#[derive(Debug)]
pub struct MissingFunction;
/// Original expression retained before choosing its real approximation domain.
#[derive(Debug)]
pub struct OriginalFunction {
    function: Function,
}
/// Defined real interval and original function, with configurable polynomial degree.
#[derive(Debug)]
pub struct FunctionDomain {
    function: Function,
    domain: Interval,
    degree: usize,
}
/// Approximation inputs with admitted precision and resource policy.
#[derive(Debug)]
pub struct ReadyRemez {
    input: FunctionDomain,
    policy: OfflineRemezPolicy,
}
/// Explicit arbitrary-precision exchange followed by a binary64 export error proof.
///
/// Supply a function, a positive-width subinterval of `[-1,1]`, the degree and
/// an admitted policy before calling `solve`. The final enclosure establishes
/// total uniform error, independently of the empirical exchange gap.
///
/// ```
/// use quest_polynomial::{Interval, function};
/// use quest_qsp::offline::{OfflineRemezBuilder, OfflineRemezPolicy};
/// let approximation = OfflineRemezBuilder::new()
///     .function(function!(|x| x.exp()))
///     .domain(Interval::new(-1.0, 1.0)?)?
///     .degree(3)
///     .policy(OfflineRemezPolicy {
///         error_tolerance: 0.006,
///         ..OfflineRemezPolicy::default()
///     })?
///     .solve()?;
/// assert!(approximation.error_bound().upper() <= 0.006);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct OfflineRemezBuilder<S = MissingFunction> {
    state: S,
}
impl Default for OfflineRemezBuilder {
    fn default() -> Self {
        Self::new()
    }
}
impl OfflineRemezBuilder {
    /// Start an explicit approximation request without a function.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: MissingFunction,
        }
    }
    /// Retain the original function expression for all precision attempts.
    #[must_use]
    pub const fn function(self, function: Function) -> OfflineRemezBuilder<OriginalFunction> {
        OfflineRemezBuilder {
            state: OriginalFunction { function },
        }
    }
}
impl OfflineRemezBuilder<OriginalFunction> {
    /// Admit the real approximation interval and interval derivative domain.
    /// # Errors
    /// Requires a nondegenerate domain within [-1,1] and defined interval derivatives.
    pub fn domain(self, domain: Interval) -> OfflineResult<OfflineRemezBuilder<FunctionDomain>> {
        if domain.lower() >= domain.upper() || domain.lower() < -1.0 || domain.upper() > 1.0 {
            return Err(OfflineError::Domain(
                "Remez requires a positive-width subinterval of [-1,1]",
            ));
        }
        self.state.function.jet_interval(domain)?;
        Ok(OfflineRemezBuilder {
            state: FunctionDomain {
                function: self.state.function,
                domain,
                degree: 16,
            },
        })
    }
}
impl OfflineRemezBuilder<FunctionDomain> {
    /// Set the Chebyshev degree (default 16), validated with the policy.
    #[must_use]
    pub const fn degree(mut self, degree: usize) -> Self {
        self.state.degree = degree;
        self
    }
    /// Admit precision, stopping criteria and modeled approximation resources.
    /// # Errors
    /// Rejects invalid numerical policies and allocations beyond the stated budgets.
    pub fn policy(
        self,
        policy: OfflineRemezPolicy,
    ) -> OfflineResult<OfflineRemezBuilder<ReadyRemez>> {
        policy.offline.validate()?;
        if !policy.error_tolerance.is_finite()
            || policy.error_tolerance <= 0.0
            || !policy.exchange_tolerance.is_finite()
            || policy.exchange_tolerance <= 0.0
            || policy.max_iterations == 0
            || policy.max_subdivisions == 0
        {
            return Err(OfflineError::Policy(
                "finite positive Remez tolerances and budgets required",
            ));
        }
        let n = self
            .state
            .degree
            .checked_add(2)
            .ok_or(OfflineError::Budget("Remez degree"))?;
        if n > policy.offline.max_coefficients {
            return Err(OfflineError::Budget("Remez coefficients"));
        }
        let cells = n.checked_mul(n).ok_or(OfflineError::Budget("QR cells"))?;
        let bytes = cells
            .checked_mul(4)
            .ok_or(OfflineError::Budget("QR temporaries"))?
            .checked_mul(modeled_scalar_bytes(policy.offline.max_precision)?)
            .and_then(|bytes| {
                policy
                    .max_subdivisions
                    .checked_mul(size_of::<Interval>())
                    .and_then(|stack| bytes.checked_add(stack))
            })
            .and_then(|bytes| bytes.checked_add(65_536))
            .ok_or(OfflineError::Budget("QR and interval-stack bytes"))?;
        if bytes > policy.offline.max_bytes {
            return Err(OfflineError::Budget("QR bytes"));
        }
        Ok(OfflineRemezBuilder {
            state: ReadyRemez {
                input: self.state,
                policy,
            },
        })
    }
}
/// Frozen Chebyshev approximation with an independently enclosed uniform error.
#[derive(Debug)]
pub struct OfflineApproximation {
    function: Function,
    domain: Interval,
    polynomial: Polynomial<Chebyshev>,
    error_bound: Interval,
    precision: u32,
    iterations: usize,
    attempts: usize,
    computation_elapsed: Duration,
    enclosure_elapsed: Duration,
    exchange_gap: BigFloat,
}
impl OfflineApproximation {
    /// Original expression against which the exported polynomial was checked.
    #[must_use]
    pub const fn function(&self) -> &Function {
        &self.function
    }
    /// Real interval covered by the uniform-error enclosure.
    #[must_use]
    pub const fn domain(&self) -> Interval {
        self.domain
    }
    /// Frozen binary64 Chebyshev polynomial in the original real coordinate.
    #[must_use]
    pub const fn polynomial(&self) -> &Polynomial<Chebyshev> {
        &self.polynomial
    }
    /// Rigorous total uniform-error enclosure on the complete admitted interval.
    #[must_use]
    pub const fn error_bound(&self) -> Interval {
        self.error_bound
    }
    /// Working precision in bits of the successful computation attempt.
    #[must_use]
    pub const fn precision(&self) -> u32 {
        self.precision
    }
    /// Exchange iterations used by the successful attempt.
    #[must_use]
    pub const fn iterations(&self) -> usize {
        self.iterations
    }
    /// Number of computation precision attempts.
    #[must_use]
    pub const fn attempts(&self) -> usize {
        self.attempts
    }
    /// Accumulated computation wall time, excluding final-error enclosure work.
    #[must_use]
    pub const fn computation_elapsed(&self) -> Duration {
        self.computation_elapsed
    }
    /// Accumulated wall time establishing the final polynomial's uniform error.
    #[must_use]
    pub const fn enclosure_elapsed(&self) -> Duration {
        self.enclosure_elapsed
    }
    /// Empirical exchange gap at located extrema, not a minimax enclosure.
    #[must_use]
    pub const fn exchange_gap(&self) -> &BigFloat {
        &self.exchange_gap
    }
}
/// Original expression and last export retained when the approximation is not established.
#[derive(Debug)]
pub struct ApproximationFailure {
    function: Function,
    domain: Interval,
    degree: usize,
    precision: u32,
    attempts: usize,
    reason: &'static str,
    polynomial: Option<Polynomial<Chebyshev>>,
    coefficients: Option<Vec<BigFloat>>,
}
impl ApproximationFailure {
    /// Original expression retained for diagnosis or a new explicit request.
    #[must_use]
    pub const fn function(&self) -> &Function {
        &self.function
    }
    /// Original real approximation interval.
    #[must_use]
    pub const fn domain(&self) -> Interval {
        self.domain
    }
    /// Requested polynomial degree.
    #[must_use]
    pub const fn degree(&self) -> usize {
        self.degree
    }
    /// Final attempted computation precision in bits.
    #[must_use]
    pub const fn precision(&self) -> u32 {
        self.precision
    }
    /// Number of attempted precision levels.
    #[must_use]
    pub const fn attempts(&self) -> usize {
        self.attempts
    }
    /// Why the last attempt did not establish the requested result.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        self.reason
    }
    /// Last exported binary64 candidate, if an export was reached.
    #[must_use]
    pub const fn polynomial(&self) -> Option<&Polynomial<Chebyshev>> {
        self.polynomial.as_ref()
    }
    /// Last arbitrary-precision candidate coefficients, when available.
    #[must_use]
    pub fn arbitrary_coefficients(&self) -> Option<&[BigFloat]> {
        self.coefficients.as_deref()
    }
}
impl OfflineRemezBuilder<ReadyRemez> {
    /// Compute exchange candidates and enclose the final binary64 uniform error.
    /// # Errors
    /// Returns typed budget, domain, or inability-to-establish errors; never calls
    /// the production binary64 approximation or synthesis kernels.
    #[expect(
        clippy::too_many_lines,
        reason = "precision retries retain the original source and each typed failure report"
    )]
    pub fn solve(self) -> OfflineResult<OfflineApproximation> {
        let ReadyRemez { input, policy } = self.state;
        let mut precision = policy.offline.initial_precision;
        let mut attempts = 0usize;
        let mut work = 0;
        let mut computation_elapsed = Duration::ZERO;
        let mut enclosure_elapsed = Duration::ZERO;
        loop {
            attempts = attempts
                .checked_add(1)
                .ok_or(OfflineError::Budget("Remez attempts"))?;
            let mut context = Context::new(
                input
                    .degree
                    .checked_add(2)
                    .ok_or(OfflineError::Budget("Remez count"))?,
                precision,
                policy.offline,
            )?;
            context.work = work;
            let start = Instant::now();
            let candidate = exchange(&input, policy, &mut context);
            computation_elapsed = computation_elapsed.saturating_add(start.elapsed());
            work = context.work;
            match candidate {
                Ok((coefficients, iterations, exchange_gap)) => {
                    let polynomial = Polynomial::new(
                        Chebyshev,
                        coefficients
                            .iter()
                            .map(|x| Ok(Complex64::new(to_f64(x, BinaryRounding::Nearest)?, 0.0)))
                            .collect::<OfflineResult<_>>()?,
                        Limits {
                            max_coefficients: policy.offline.max_coefficients,
                            ..Limits::default()
                        },
                    )?;
                    let start = Instant::now();
                    let enclosure = enclose(
                        &input.function,
                        &polynomial,
                        input.domain,
                        policy,
                        &mut work,
                    );
                    enclosure_elapsed = enclosure_elapsed.saturating_add(start.elapsed());
                    match enclosure {
                        Ok(error_bound) => {
                            return Ok(OfflineApproximation {
                                function: input.function,
                                domain: input.domain,
                                polynomial,
                                error_bound,
                                precision,
                                iterations,
                                attempts,
                                computation_elapsed,
                                enclosure_elapsed,
                                exchange_gap,
                            });
                        }
                        Err(OfflineError::Numerical(reason)) => {
                            if precision == policy.offline.max_precision {
                                return Err(OfflineError::ApproximationNotEstablished {
                                    report: Box::new(ApproximationFailure {
                                        function: input.function,
                                        domain: input.domain,
                                        degree: input.degree,
                                        precision,
                                        attempts,
                                        reason,
                                        polynomial: Some(polynomial),
                                        coefficients: Some(coefficients),
                                    }),
                                });
                            }
                        }
                        Err(e) => return Err(e),
                    }
                }
                Err(OfflineError::Numerical(reason)) => {
                    if precision == policy.offline.max_precision {
                        return Err(OfflineError::ApproximationNotEstablished {
                            report: Box::new(ApproximationFailure {
                                function: input.function,
                                domain: input.domain,
                                degree: input.degree,
                                precision,
                                attempts,
                                reason,
                                polynomial: None,
                                coefficients: None,
                            }),
                        });
                    }
                }
                Err(e) => return Err(e),
            }
            precision = precision
                .checked_mul(2)
                .ok_or(OfflineError::Budget("Remez precision"))?
                .min(policy.offline.max_precision);
        }
    }
}
#[derive(Clone)]
struct Jet {
    value: BigFloat,
    first: BigFloat,
}
fn evaluate(expr: &Expr, x: &BigFloat, depth: u16, context: &mut Context) -> OfflineResult<Jet> {
    context.charge(16)?;
    let next = depth
        .checked_sub(1)
        .ok_or(OfflineError::Budget("expression depth"))?;
    let zero = BigFloat::from_i64(0, crate::offline::number::precision_bits(context.precision));
    let one = BigFloat::from_i64(1, crate::offline::number::precision_bits(context.precision));
    let result = match expr.node() {
        ExprNode::Variable => Jet {
            value: x.clone(),
            first: one,
        },
        ExprNode::Constant(v) => Jet {
            value: exact_from_f64(v, context.precision)?,
            first: zero,
        },
        ExprNode::Add(a, b) | ExprNode::Sub(a, b) | ExprNode::Mul(a, b) | ExprNode::Div(a, b) => {
            let a = evaluate(a, x, next, context)?;
            let b = evaluate(b, x, next, context)?;
            match expr.node() {
                ExprNode::Add(..) => Jet {
                    value: add(&a.value, &b.value),
                    first: add(&a.first, &b.first),
                },
                ExprNode::Sub(..) => Jet {
                    value: sub(&a.value, &b.value),
                    first: sub(&a.first, &b.first),
                },
                ExprNode::Mul(..) => Jet {
                    value: mul(&a.value, &b.value),
                    first: add(&mul(&a.first, &b.value), &mul(&a.value, &b.first)),
                },
                _ => Jet {
                    value: div(&a.value, &b.value),
                    first: div(
                        &sub(&mul(&a.first, &b.value), &mul(&a.value, &b.first)),
                        &mul(&b.value, &b.value),
                    ),
                },
            }
        }
        ExprNode::Neg(a) => {
            let a = evaluate(a, x, next, context)?;
            Jet {
                value: neg(&a.value),
                first: neg(&a.first),
            }
        }
        ExprNode::Exp(a) => {
            let a = evaluate(a, x, next, context)?;
            let value = context.exp(&a.value)?;
            let first = mul(&value, &a.first);
            Jet { value, first }
        }
        ExprNode::Ln(a) => {
            let a = evaluate(a, x, next, context)?;
            let first = div(&a.first, &a.value);
            Jet {
                value: context.ln(&a.value)?,
                first,
            }
        }
        ExprNode::Sin(a) => {
            let a = evaluate(a, x, next, context)?;
            let first = mul(&context.cos(&a.value)?, &a.first);
            Jet {
                value: context.sin(&a.value)?,
                first,
            }
        }
        ExprNode::Cos(a) => {
            let a = evaluate(a, x, next, context)?;
            let first = neg(&mul(&context.sin(&a.value)?, &a.first));
            Jet {
                value: context.cos(&a.value)?,
                first,
            }
        }
        ExprNode::Sqrt(a) => {
            let a = evaluate(a, x, next, context)?;
            let value = sqrt(&a.value);
            let first = div(
                &a.first,
                &mul(
                    &BigFloat::from_i64(
                        2,
                        crate::offline::number::precision_bits(context.precision),
                    ),
                    &value,
                ),
            );
            Jet { value, first }
        }
    };
    validate(&result.value)?;
    validate(&result.first)?;
    Ok(result)
}
fn scratch<T>(count: usize, label: &'static str) -> OfflineResult<Vec<T>> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| OfflineError::Budget(label))?;
    Ok(result)
}
fn basis(x: &BigFloat, count: usize) -> OfflineResult<Vec<Jet>> {
    let p = bits(x);
    let two = BigFloat::from_i64(2, p);
    let mut previous = Jet {
        value: BigFloat::from_i64(0, p),
        first: BigFloat::from_i64(0, p),
    };
    let mut current = Jet {
        value: BigFloat::from_i64(1, p),
        first: BigFloat::from_i64(0, p),
    };
    let mut out = scratch(count, "Remez basis scratch")?;
    for k in 0..count {
        out.push(current.clone());
        let next = if k == 0 {
            Jet {
                value: x.clone(),
                first: BigFloat::from_i64(1, p),
            }
        } else {
            Jet {
                value: sub(&mul(&mul(&two, x), &current.value), &previous.value),
                first: sub(
                    &mul(&two, &add(&current.value, &mul(x, &current.first))),
                    &previous.first,
                ),
            }
        };
        previous = current;
        current = next;
    }
    Ok(out)
}
fn residual(
    input: &FunctionDomain,
    c: &[BigFloat],
    x: &BigFloat,
    context: &mut Context,
) -> OfflineResult<Jet> {
    let mut out = evaluate(input.function.expression(), x, 256, context)?;
    context.charge(
        c.len()
            .checked_mul(16)
            .ok_or(OfflineError::Budget("evaluation work"))?,
    )?;
    for (coefficient, b) in c.iter().zip(basis(x, c.len())?) {
        out.value = sub(&out.value, &mul(coefficient, &b.value));
        out.first = sub(&out.first, &mul(coefficient, &b.first));
    }
    validate(&out.value)?;
    validate(&out.first)?;
    Ok(out)
}
fn point(domain: Interval, index: usize, count: usize, p: u32) -> OfflineResult<BigFloat> {
    let low = exact_from_f64(domain.lower(), p)?;
    Ok(checked(add(
        &low,
        &mul(
            &sub(&exact_from_f64(domain.upper(), p)?, &low),
            &div(
                &BigFloat::from_u64(
                    u64::try_from(index)
                        .map_err(|_| super::OfflineError::Budget("integer interchange"))?,
                    crate::offline::number::precision_bits(p),
                ),
                &BigFloat::from_u64(
                    u64::try_from(count)
                        .map_err(|_| super::OfflineError::Budget("integer interchange"))?,
                    crate::offline::number::precision_bits(p),
                ),
            ),
        ),
    ))?)
}
fn extrema(
    input: &FunctionDomain,
    c: &[BigFloat],
    policy: OfflineRemezPolicy,
    context: &mut Context,
) -> OfflineResult<Vec<(BigFloat, BigFloat)>> {
    let count = c
        .len()
        .checked_mul(16)
        .ok_or(OfflineError::Budget("extrema grid"))?
        .max(64);
    if count > policy.offline.max_grid {
        return Err(OfflineError::Budget("extrema grid"));
    }
    let mut points = scratch::<BigFloat>(
        count
            .checked_add(2)
            .ok_or(OfflineError::Budget("extrema points"))?,
        "extrema points",
    )?;
    points.push(exact_from_f64(input.domain.lower(), context.precision)?);
    let mut left = points
        .first()
        .ok_or(OfflineError::Numerical("initial point"))?
        .clone();
    let mut derivative = residual(input, c, &left, context)?.first;
    for i in 1..=count {
        let right = point(input.domain, i, count, context.precision)?;
        let right_derivative = residual(input, c, &right, context)?.first;
        if right_derivative.is_zero() && i < count {
            points.push(right.clone());
        }
        if (negative(&derivative) && positive(&right_derivative))
            || (positive(&derivative) && negative(&right_derivative))
        {
            let mut a = left.clone();
            let mut b = right.clone();
            let mut da = derivative.clone();
            for _ in 0..context.precision {
                let mid = div(
                    &add(&a, &b),
                    &BigFloat::from_i64(
                        2,
                        crate::offline::number::precision_bits(context.precision),
                    ),
                );
                if mid == a || mid == b {
                    break;
                }
                let dm = residual(input, c, &mid, context)?.first;
                if dm.is_zero() {
                    a.clone_from(&mid);
                    b = mid;
                    break;
                }
                if (negative(&da)) == (negative(&dm)) {
                    a = mid;
                    da = dm;
                } else {
                    b = mid;
                }
            }
            points.push(div(
                &add(&a, &b),
                &BigFloat::from_i64(2, crate::offline::number::precision_bits(context.precision)),
            ));
        }
        left = right;
        derivative = right_derivative;
    }
    points.push(exact_from_f64(input.domain.upper(), context.precision)?);
    let mut result = scratch::<(BigFloat, BigFloat)>(points.len(), "extrema results")?;
    for x in points {
        let r = residual(input, c, &x, context)?.value;
        if let Some(last) = result.last_mut()
            && (negative(&last.1)) == (negative(&r))
        {
            if r.abs() > last.1.abs() {
                *last = (x, r);
            }
            continue;
        }
        result.push((x, r));
    }
    Ok(result)
}
/// Pivoted Householder QR. Shape is validated before the indexed square algorithm.
#[expect(
    clippy::indexing_slicing,
    reason = "The square dimensions are checked and each loop uses the validated dimension"
)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Loop indices are bounded by the admitted square matrix dimensions"
)]
#[expect(
    clippy::too_many_lines,
    reason = "pivoted QR keeps its numerical operation order explicit"
)]
fn qr(
    mut a: Vec<Vec<BigFloat>>,
    mut rhs: Vec<BigFloat>,
    context: &mut Context,
) -> OfflineResult<Vec<BigFloat>> {
    let n = rhs.len();
    if n == 0 || a.len() != n || a.iter().any(|r| r.len() != n) {
        return Err(OfflineError::Numerical("QR dimensions"));
    }
    context.charge(
        n.checked_mul(n)
            .and_then(|v| v.checked_mul(n))
            .and_then(|v| v.checked_mul(16))
            .ok_or(OfflineError::Budget("QR work"))?,
    )?;
    let p = context.precision;
    let mut permutation = scratch(n, "QR permutation")?;
    permutation.extend(0..n);
    for k in 0..n {
        let mut pivot = k;
        let mut largest = BigFloat::from_i64(-1, crate::offline::number::precision_bits(p));
        for j in k..n {
            let mut norm = BigFloat::from_i64(0, crate::offline::number::precision_bits(p));
            for row in a.iter().skip(k) {
                norm = add(&norm, &mul(&row[j], &row[j]));
            }
            validate(&norm)?;
            if norm > largest {
                largest = norm;
                pivot = j;
            }
        }
        validate(&largest)?;
        if !positive(&largest) {
            return Err(OfflineError::Numerical(
                "singular arbitrary-precision alternation system",
            ));
        }
        for row in &mut a {
            row.swap(k, pivot);
        }
        permutation.swap(k, pivot);
        let norm = sqrt(&largest);
        let alpha = if negative(&a[k][k]) { norm } else { neg(&norm) };
        let mut v = scratch(n - k, "QR reflector")?;
        v.extend(a.iter().skip(k).map(|row| row[k].clone()));
        v[0] = sub(&v[0], &alpha);
        let denominator = v.iter().fold(
            BigFloat::from_i64(0, crate::offline::number::precision_bits(p)),
            |s, x| add(&s, &mul(x, x)),
        );
        validate(&denominator)?;
        if denominator.is_zero() {
            return Err(OfflineError::Numerical(
                "degenerate arbitrary-precision QR reflector",
            ));
        }
        let beta = div(
            &BigFloat::from_i64(2, crate::offline::number::precision_bits(p)),
            &denominator,
        );
        for j in k..n {
            let dot = v.iter().enumerate().fold(
                BigFloat::from_i64(0, crate::offline::number::precision_bits(p)),
                |s, (i, x)| add(&s, &mul(x, &a[k + i][j])),
            );
            let factor = mul(&beta, &dot);
            for (i, x) in v.iter().enumerate() {
                a[k + i][j] = sub(&a[k + i][j], &mul(x, &factor));
            }
        }
        let dot = v.iter().enumerate().fold(
            BigFloat::from_i64(0, crate::offline::number::precision_bits(p)),
            |s, (i, x)| add(&s, &mul(x, &rhs[k + i])),
        );
        let factor = mul(&beta, &dot);
        for (i, x) in v.iter().enumerate() {
            rhs[k + i] = sub(&rhs[k + i], &mul(x, &factor));
        }
        a[k][k] = alpha;
        for row in a.iter_mut().skip(k + 1) {
            row[k] = BigFloat::from_i64(0, crate::offline::number::precision_bits(p));
        }
    }
    let mut solution = scratch(n, "QR solution")?;
    solution.resize_with(n, || {
        BigFloat::from_i64(0, crate::offline::number::precision_bits(p))
    });
    for i in (0..n).rev() {
        let mut value = rhs[i].clone();
        for (j, x) in solution.iter().enumerate().skip(i + 1) {
            value = sub(&value, &mul(&a[i][j], x));
        }
        solution[i] = div(&value, &a[i][i]);
    }
    let mut result = scratch(n, "QR ordered solution")?;
    result.resize_with(n, || {
        BigFloat::from_i64(0, crate::offline::number::precision_bits(p))
    });
    for (i, x) in solution.into_iter().enumerate() {
        result[permutation[i]] = x;
    }
    for value in &result {
        validate(value)?;
    }
    Ok(result)
}
fn exchange(
    input: &FunctionDomain,
    policy: OfflineRemezPolicy,
    context: &mut Context,
) -> OfflineResult<(Vec<BigFloat>, usize, BigFloat)> {
    let n = input
        .degree
        .checked_add(2)
        .ok_or(OfflineError::Budget("alternation count"))?;
    let count = n
        .checked_sub(1)
        .ok_or(OfflineError::Budget("alternation count"))?;
    let mut nodes = scratch(n, "Remez nodes")?;
    for i in 0..n {
        nodes.push(point(input.domain, i, count, context.precision)?);
    }
    let tolerance = exact_from_f64(policy.exchange_tolerance, context.precision)?;
    for iteration in 1..=policy.max_iterations {
        let mut matrix = scratch(n, "Remez matrix")?;
        let mut rhs = scratch(n, "Remez right-hand side")?;
        for (i, x) in nodes.iter().enumerate() {
            let mut row = scratch(n, "Remez matrix row")?;
            row.extend(basis(x, count)?.into_iter().map(|j| j.value));
            row.push(BigFloat::from_i64(
                if i & 1 == 0 { 1 } else { -1 },
                crate::offline::number::precision_bits(context.precision),
            ));
            matrix.push(row);
            rhs.push(evaluate(input.function.expression(), x, 256, context)?.value);
        }
        let mut coefficients = qr(matrix, rhs, context)?;
        coefficients.pop();
        let extrema = extrema(input, &coefficients, policy, context)?;
        let maximum = extrema
            .iter()
            .map(|(_, y)| y.abs())
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or(OfflineError::Numerical("no extrema"))?;
        if maximum <= tolerance {
            return Ok((coefficients, iteration, maximum));
        }
        let mut selected = None;
        for window in extrema.windows(n) {
            let minimum = window
                .iter()
                .map(|(_, r)| r.abs())
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .ok_or(OfflineError::Numerical("empty alternation"))?;
            if selected
                .as_ref()
                .is_none_or(|(_, old): &(Vec<BigFloat>, BigFloat)| minimum > *old)
            {
                let mut chosen = scratch(n, "Remez selected nodes")?;
                chosen.extend(window.iter().map(|(x, _)| x.clone()));
                selected = Some((chosen, minimum));
            }
        }
        let (new_nodes, minimum) = selected.ok_or(OfflineError::Numerical(
            "insufficient located alternating extrema",
        ))?;
        let gap = sub(&maximum, &minimum);
        validate(&gap)?;
        if gap <= tolerance {
            return Ok((coefficients, iteration, gap));
        }
        nodes = new_nodes;
    }
    Err(OfflineError::Numerical(
        "arbitrary-precision exchange iteration budget exhausted",
    ))
}
fn residual_interval(
    function: &Function,
    polynomial: &Polynomial<Chebyshev>,
    x: Interval,
) -> OfflineResult<(Interval, Interval)> {
    let a = function.jet_interval(x)?;
    let b = polynomial.jet_interval(x)?;
    Ok((a.value.checked_sub(b.value)?, a.first.checked_sub(b.first)?))
}
fn expression_nodes(expr: &Expr, depth: u16) -> OfflineResult<usize> {
    let next = depth
        .checked_sub(1)
        .ok_or(OfflineError::Budget("Remez expression depth"))?;
    let children = match expr.node() {
        ExprNode::Variable | ExprNode::Constant(_) => 0,
        ExprNode::Add(a, b) | ExprNode::Sub(a, b) | ExprNode::Mul(a, b) | ExprNode::Div(a, b) => {
            expression_nodes(a, next)?
                .checked_add(expression_nodes(b, next)?)
                .ok_or(OfflineError::Budget("Remez expression work"))?
        }
        ExprNode::Neg(a)
        | ExprNode::Exp(a)
        | ExprNode::Ln(a)
        | ExprNode::Sin(a)
        | ExprNode::Cos(a)
        | ExprNode::Sqrt(a) => expression_nodes(a, next)?,
    };
    children
        .checked_add(1)
        .ok_or(OfflineError::Budget("Remez expression work"))
}
fn magnitude(x: Interval) -> f64 {
    x.lower().abs().max(x.upper().abs())
}
fn enclose(
    function: &Function,
    polynomial: &Polynomial<Chebyshev>,
    domain: Interval,
    policy: OfflineRemezPolicy,
    work: &mut usize,
) -> OfflineResult<Interval> {
    let mut pending = scratch(1, "Remez enclosure stack")?;
    pending.push(domain);
    // A visit evaluates both jets at least twice and may evaluate both endpoints.
    // Charge the worst case before doing the interval arithmetic.
    let visit_work = expression_nodes(function.expression(), 256)?
        .checked_add(polynomial.coefficients().len())
        .and_then(|units| units.checked_mul(4))
        .ok_or(OfflineError::Budget("Remez enclosure work"))?;
    let mut upper = 0.0_f64;
    let mut visited = 0usize;
    while let Some(x) = pending.pop() {
        visited = visited
            .checked_add(1)
            .ok_or(OfflineError::Budget("enclosure count"))?;
        if visited > policy.max_subdivisions {
            return Err(OfflineError::Numerical(
                "uniform exported error not established within interval subdivision budget",
            ));
        }
        *work = work
            .checked_add(visit_work)
            .ok_or(OfflineError::Budget("Remez enclosure work"))?;
        if *work > policy.offline.max_work {
            return Err(OfflineError::Budget("Remez enclosure work"));
        }
        let (value, derivative) = residual_interval(function, polynomial, x)?;
        let mid = x.lower().mul_add(0.5, x.upper() * 0.5);
        let center = Interval::point(mid)?;
        let center_residual = residual_interval(function, polynomial, center)?.0;
        let mean_value =
            center_residual.checked_add(derivative.checked_mul(x.checked_sub(center)?)?)?;
        let mut local = magnitude(value).min(magnitude(mean_value));
        if derivative.lower() > 0.0 || derivative.upper() < 0.0 {
            local = local.min(
                magnitude(residual_interval(function, polynomial, Interval::point(x.lower())?)?.0)
                    .max(magnitude(
                        residual_interval(function, polynomial, Interval::point(x.upper())?)?.0,
                    )),
            );
        }
        if local <= policy.error_tolerance {
            upper = upper.max(local);
            continue;
        }
        if mid <= x.lower() || mid >= x.upper() {
            return Err(OfflineError::Numerical(
                "binary64 export error enclosure cannot establish requested tolerance",
            ));
        }
        let scheduled = visited
            .checked_add(pending.len())
            .and_then(|count| count.checked_add(2))
            .ok_or(OfflineError::Budget("Remez enclosure count"))?;
        if scheduled > policy.max_subdivisions {
            return Err(OfflineError::Numerical(
                "uniform exported error not established within interval subdivision budget",
            ));
        }
        pending
            .try_reserve_exact(2)
            .map_err(|_| OfflineError::Budget("Remez enclosure stack"))?;
        pending.push(Interval::new(mid, x.upper())?);
        pending.push(Interval::new(x.lower(), mid)?);
    }
    Ok(Interval::new(0.0, upper)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;
    #[gtest]
    fn exported_error_enclosure_charges_the_offline_work_budget() -> Result<()> {
        let function = quest_polynomial::function!(|x| x);
        let polynomial = Polynomial::new(
            Chebyshev,
            vec![Complex64::new(0.0, 0.0), Complex64::new(1.0, 0.0)],
            Limits::default(),
        )?;
        let policy = OfflineRemezPolicy {
            offline: OfflinePolicy {
                max_work: 0,
                ..OfflinePolicy::default()
            },
            ..OfflineRemezPolicy::default()
        };
        expect_true!(matches!(
            enclose(
                &function,
                &polynomial,
                Interval::new(-1.0, 1.0)?,
                policy,
                &mut 0
            ),
            Err(OfflineError::Budget(_))
        ));
        Ok(())
    }
    #[gtest]
    fn exported_error_enclosure_charges_evaluation_size_not_only_visits() -> Result<()> {
        let function = quest_polynomial::function!(|x| x);
        let polynomial = Polynomial::new(
            Chebyshev,
            vec![Complex64::new(0.0, 0.0), Complex64::new(1.0, 0.0)],
            Limits::default(),
        )?;
        let policy = OfflineRemezPolicy {
            offline: OfflinePolicy {
                max_work: 1,
                ..OfflinePolicy::default()
            },
            ..OfflineRemezPolicy::default()
        };
        expect_true!(matches!(
            enclose(
                &function,
                &polynomial,
                Interval::new(-1.0, 1.0)?,
                policy,
                &mut 0
            ),
            Err(OfflineError::Budget(_))
        ));
        Ok(())
    }
    #[gtest]
    fn pivoted_arbitrary_qr_solves_independent_integer_reference() -> Result<()> {
        let p = 128;
        let mut context = Context::new(3, p, OfflinePolicy::default())?;
        let a = vec![vec![1, 10, 0], vec![0, 0, 2], vec![1, 1, 1]];
        let a = a
            .into_iter()
            .map(|r| {
                r.into_iter()
                    .map(|x| {
                        BigFloat::from_i64(i64::from(x), crate::offline::number::precision_bits(p))
                    })
                    .collect()
            })
            .collect();
        let rhs = vec![
            BigFloat::from_i64(-18, crate::offline::number::precision_bits(p)),
            BigFloat::from_i64(6, crate::offline::number::precision_bits(p)),
            BigFloat::from_i64(3, crate::offline::number::precision_bits(p)),
        ];
        let solution = qr(a, rhs, &mut context)?;
        for (actual, expected) in solution.iter().zip([2, -2, 3]) {
            expect_true!(
                sub(
                    actual,
                    &BigFloat::from_i64(
                        i64::from(expected),
                        crate::offline::number::precision_bits(p)
                    )
                )
                .abs()
                    < exact_from_f64(1e-34, p)?
            );
        }
        Ok(())
    }
}
