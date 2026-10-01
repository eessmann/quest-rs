use crate::{Error, Interval, Result};
use std::{
    ops::{Add, Div, Mul, Neg, Sub},
    sync::Arc,
};

/// One mathematical expression shared by scalar and interval evaluation.
#[derive(Clone, Debug)]
pub struct Expr(Arc<Node>, crate::ExpressionMetadata);
#[derive(Debug)]
enum Node {
    Variable,
    Constant(f64),
    Add(Expr, Expr),
    Sub(Expr, Expr),
    Mul(Expr, Expr),
    Div(Expr, Expr),
    Neg(Expr),
    Exp(Expr),
    Ln(Expr),
    Sin(Expr),
    Cos(Expr),
    Sqrt(Expr),
}
/// Borrowed read-only view of the original mathematical expression.
/// Cold external evaluators can select a numerical backend without altering the AST.
#[derive(Clone, Copy, Debug)]
pub enum ExprNode<'a> {
    Variable,
    Constant(f64),
    Add(&'a Expr, &'a Expr),
    Sub(&'a Expr, &'a Expr),
    Mul(&'a Expr, &'a Expr),
    Div(&'a Expr, &'a Expr),
    Neg(&'a Expr),
    Exp(&'a Expr),
    Ln(&'a Expr),
    Sin(&'a Expr),
    Cos(&'a Expr),
    Sqrt(&'a Expr),
}
impl Expr {
    #[must_use]
    pub fn node(&self) -> ExprNode<'_> {
        match self.0.as_ref() {
            Node::Variable => ExprNode::Variable,
            Node::Constant(v) => ExprNode::Constant(*v),
            Node::Add(a, b) => ExprNode::Add(a, b),
            Node::Sub(a, b) => ExprNode::Sub(a, b),
            Node::Mul(a, b) => ExprNode::Mul(a, b),
            Node::Div(a, b) => ExprNode::Div(a, b),
            Node::Neg(a) => ExprNode::Neg(a),
            Node::Exp(a) => ExprNode::Exp(a),
            Node::Ln(a) => ExprNode::Ln(a),
            Node::Sin(a) => ExprNode::Sin(a),
            Node::Cos(a) => ExprNode::Cos(a),
            Node::Sqrt(a) => ExprNode::Sqrt(a),
        }
    }

    #[must_use]
    pub fn variable() -> Self {
        Self(Arc::new(Node::Variable), crate::ExpressionMetadata::leaf())
    }
    #[must_use]
    pub fn exp(self) -> Self {
        let metadata = crate::ExpressionMetadata::unary(self.1);
        Self(Arc::new(Node::Exp(self)), metadata)
    }
    #[must_use]
    pub fn ln(self) -> Self {
        let metadata = crate::ExpressionMetadata::unary(self.1).positive_jet();
        Self(Arc::new(Node::Ln(self)), metadata)
    }
    #[must_use]
    pub fn sin(self) -> Self {
        let metadata = crate::ExpressionMetadata::unary(self.1);
        Self(Arc::new(Node::Sin(self)), metadata)
    }
    #[must_use]
    pub fn cos(self) -> Self {
        let metadata = crate::ExpressionMetadata::unary(self.1);
        Self(Arc::new(Node::Cos(self)), metadata)
    }
    #[must_use]
    pub fn sqrt(self) -> Self {
        let metadata = crate::ExpressionMetadata::unary(self.1).positive_jet();
        Self(Arc::new(Node::Sqrt(self)), metadata)
    }
}
impl From<f64> for Expr {
    fn from(x: f64) -> Self {
        Self(
            Arc::new(Node::Constant(x)),
            crate::ExpressionMetadata::leaf(),
        )
    }
}
macro_rules! binary {
    ($trait:ident,$method:ident,$node:ident $(,$requirement:ident)?) => {
        impl<T: Into<Expr>> $trait<T> for Expr {
            type Output = Self;
            fn $method(self, rhs: T) -> Self {
                let rhs=rhs.into();
                let metadata=crate::ExpressionMetadata::binary(self.1,rhs.1)$(.$requirement())?;
                Self(Arc::new(Node::$node(self, rhs)),metadata)
            }
        }
        impl $trait<Expr> for f64 {
            type Output = Expr;
            fn $method(self, rhs: Expr) -> Expr {
                Expr::from(self).$method(rhs)
            }
        }
    };
}
binary!(Add, add, Add);
binary!(Sub, sub, Sub);
binary!(Mul, mul, Mul);
binary!(Div, div, Div, denominator);
impl Neg for Expr {
    type Output = Self;
    fn neg(self) -> Self {
        let metadata = crate::ExpressionMetadata::unary(self.1);
        Self(Arc::new(Node::Neg(self)), metadata)
    }
}
/// Immutable expression. Built-in binary64 and interval evaluators allocate no
/// heap memory; external backend allocation behavior follows its own policy.
#[derive(Clone, Debug)]
pub struct Function<E = Expr> {
    expression: E,
}
impl Function {
    /// Construct a compatibility AST function, preserving `value.into()` inference.
    #[must_use]
    pub const fn new(expression: Expr) -> Self {
        Self { expression }
    }
}
impl<E: crate::Expression> Function<E> {
    #[must_use]
    pub const fn expression(&self) -> &E {
        &self.expression
    }
    #[must_use]
    pub const fn from_expression(expression: E) -> Self {
        Self { expression }
    }
    /// Expression shape available during const evaluation for typed expressions.
    pub const fn static_metadata(&self) -> crate::ExpressionMetadata
    where
        E: [const] crate::StaticExpression,
    {
        self.expression.static_metadata()
    }
    /// Evaluate this exact expression using the caller's arithmetic policy.
    /// # Errors
    /// Propagates backend arithmetic, domain and work errors.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Owned scalar inputs match all backend evaluator entry points"
    )]
    pub fn evaluate_backend<B: crate::Backend>(
        &self,
        backend: &mut B,
        x: B::Scalar,
    ) -> std::result::Result<B::Scalar, B::Error> {
        self.expression.eval(backend, &x, 256)
    }
    /// # Errors
    /// Propagates backend arithmetic, derivative-domain and work errors.
    pub fn jet_backend<B: crate::Backend>(
        &self,
        backend: &mut B,
        x: B::Scalar,
    ) -> std::result::Result<Jet<B::Scalar>, B::Error> {
        let mut jet = crate::JetBackend(backend);
        let x = jet.variable(x)?;
        self.expression.eval(&mut jet, &x, 256)
    }
    /// Exact structural conversion, retaining captured binary64 constant bits.
    #[must_use]
    pub fn to_dynamic(&self) -> Function {
        Function::new(self.expression.to_expr())
    }
    #[must_use]
    pub fn metadata(&self) -> crate::ExpressionMetadata {
        self.expression.metadata()
    }
    /// # Errors
    /// Rejects invalid domains, nonfinite arithmetic, or expression depth above 256.
    pub fn evaluate(&self, x: f64) -> Result<f64> {
        self.evaluate_backend(&mut crate::ScalarBackend::<f64>::new(), checked(x)?)
    }
    /// # Errors
    /// Rejects any undefined point in the entire interval or unbounded arithmetic.
    pub fn evaluate_interval(&self, x: Interval) -> Result<Interval> {
        self.evaluate_backend(&mut crate::ScalarBackend::<Interval>::new(), x)
    }
    /// # Errors
    /// Rejects domains where either derivative cannot be enclosed finitely.
    pub fn jet_interval(&self, x: Interval) -> Result<Jet<Interval>> {
        self.jet_backend(&mut crate::ScalarBackend::<Interval>::new(), x)
    }
    /// # Errors
    /// Rejects domains where the expression or its derivatives are undefined.
    pub fn derivative_interval(&self, x: Interval) -> Result<Interval> {
        Ok(self.jet_interval(x)?.first)
    }
    /// # Errors
    /// Rejects undefined or nonfinite value or derivative arithmetic.
    pub fn jet(&self, x: f64) -> Result<Jet<f64>> {
        self.jet_backend(&mut crate::ScalarBackend::<f64>::new(), checked(x)?)
    }
}
/// Build one expression, including arithmetic and exp/ln/sin/cos/sqrt methods.
///
/// ```
/// use quest_polynomial::function;
/// let f = function!(|x| (1.0 + x.clone()*x).ln());
/// assert!(f.evaluate(0.0).is_ok());
/// ```
#[macro_export]
macro_rules! function {
    (|$x:ident| $expression:expr) => {{
        let $x = $crate::Expr::variable();
        $crate::Function::<$crate::Expr>::new(($expression).into())
    }};
}
/// Value and its first two mathematical derivatives.
#[derive(Clone, Copy, Debug)]
pub struct Jet<T> {
    pub value: T,
    pub first: T,
    pub second: T,
}
pub trait Real: Clone {
    fn point(x: f64) -> Result<Self>;
    fn add(self, rhs: Self) -> Result<Self>;
    fn sub(self, rhs: Self) -> Result<Self>;
    fn mul(self, rhs: Self) -> Result<Self>;
    fn div(self, rhs: Self) -> Result<Self>;
    fn neg(self) -> Result<Self>;
    fn exp(self) -> Result<Self>;
    fn ln(self) -> Result<Self>;
    fn sin(self) -> Result<Self>;
    fn cos(self) -> Result<Self>;
    fn sqrt(self) -> Result<Self>;
}
const fn checked(x: f64) -> Result<f64> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(Error::NonFinite)
    }
}
impl Real for f64 {
    fn point(x: f64) -> Result<Self> {
        checked(x)
    }
    fn add(self, rhs: Self) -> Result<Self> {
        checked(self + rhs)
    }
    fn sub(self, rhs: Self) -> Result<Self> {
        checked(self - rhs)
    }
    fn mul(self, rhs: Self) -> Result<Self> {
        checked(self * rhs)
    }
    fn div(self, rhs: Self) -> Result<Self> {
        if rhs == 0.0 {
            return Err(Error::Domain);
        }
        checked(self / rhs)
    }
    fn neg(self) -> Result<Self> {
        checked(-self)
    }
    fn exp(self) -> Result<Self> {
        checked(self.exp())
    }
    fn ln(self) -> Result<Self> {
        checked(self.ln())
    }
    fn sin(self) -> Result<Self> {
        checked(self.sin())
    }
    fn cos(self) -> Result<Self> {
        checked(self.cos())
    }
    fn sqrt(self) -> Result<Self> {
        checked(self.sqrt())
    }
}
impl Real for Interval {
    fn point(x: f64) -> Result<Self> {
        Ok(Self::point(x)?)
    }
    fn add(self, rhs: Self) -> Result<Self> {
        Ok(self.checked_add(rhs)?)
    }
    fn sub(self, rhs: Self) -> Result<Self> {
        Ok(self.checked_sub(rhs)?)
    }
    fn mul(self, rhs: Self) -> Result<Self> {
        Ok(self.checked_mul(rhs)?)
    }
    fn div(self, rhs: Self) -> Result<Self> {
        Ok(self.checked_div(rhs)?)
    }
    fn neg(self) -> Result<Self> {
        Ok(self.checked_neg()?)
    }
    fn exp(self) -> Result<Self> {
        Ok(self.exp()?)
    }
    fn ln(self) -> Result<Self> {
        Ok(self.ln()?)
    }
    fn sin(self) -> Result<Self> {
        Ok(self.sin()?)
    }
    fn cos(self) -> Result<Self> {
        Ok(self.cos()?)
    }
    fn sqrt(self) -> Result<Self> {
        Ok(self.sqrt()?)
    }
}
impl<T: Real> Jet<T> {
    pub(crate) fn constant(value: T) -> Result<Self> {
        Ok(Self {
            value,
            first: T::point(0.0)?,
            second: T::point(0.0)?,
        })
    }
    pub(crate) fn add(self, rhs: Self) -> Result<Self> {
        crate::Backend::add(
            &mut crate::JetBackend(&mut crate::ScalarBackend::<T>::new()),
            self,
            rhs,
        )
    }
    pub(crate) fn sub(self, rhs: Self) -> Result<Self> {
        crate::Backend::sub(
            &mut crate::JetBackend(&mut crate::ScalarBackend::<T>::new()),
            self,
            rhs,
        )
    }
    pub(crate) fn mul(self, rhs: Self) -> Result<Self> {
        crate::Backend::mul(
            &mut crate::JetBackend(&mut crate::ScalarBackend::<T>::new()),
            self,
            rhs,
        )
    }
    pub(crate) fn recip(self) -> Result<Self> {
        crate::JetBackend(&mut crate::ScalarBackend::<T>::new()).recip(self)
    }
}

/// Explicit caller claim required by independent value/derivative callbacks.
#[derive(Clone, Copy, Debug)]
pub enum ConsistencyAssumption {
    /// The scalar callback evaluates the same mathematical function whose value
    /// and first two derivatives are enclosed throughout each interval callback.
    SameFunctionAndDerivatives,
}
/// Callbacks with conditional mathematical semantics.
///
/// This distinct type records a caller assumption. It cannot enter the
/// expression-only Remez builder or acquire its unconditional expression evidence.
#[derive(Debug)]
pub struct CallbackFunction<S, I> {
    scalar: S,
    interval: I,
    assumption: ConsistencyAssumption,
}
impl<S, I> CallbackFunction<S, I>
where
    S: Fn(f64) -> Result<f64>,
    I: Fn(Interval) -> Result<Jet<Interval>>,
{
    #[must_use]
    pub const fn with_assumed_consistency(
        assumption: ConsistencyAssumption,
        scalar: S,
        interval: I,
    ) -> Self {
        Self {
            scalar,
            interval,
            assumption,
        }
    }
    #[must_use]
    pub const fn assumption(&self) -> ConsistencyAssumption {
        self.assumption
    }
    /// # Errors
    /// Propagates callback failures and rejects nonfinite input or output.
    pub fn evaluate(&self, x: f64) -> Result<f64> {
        checked((self.scalar)(checked(x)?)?)
    }
    /// # Errors
    /// Propagates callback domain and arithmetic failures. The caller is
    /// responsible for the explicitly assumed enclosure/consistency contract.
    pub fn jet_interval(&self, x: Interval) -> Result<Jet<Interval>> {
        (self.interval)(x)
    }
}

impl crate::typed::sealed::Sealed for Expr {}
impl crate::Expression for Expr {
    fn eval<B: crate::Backend>(
        &self,
        backend: &mut B,
        x: &B::Scalar,
        depth: u16,
    ) -> std::result::Result<B::Scalar, B::Error> {
        let next = crate::typed::enter(backend, depth)?;
        match self.node() {
            ExprNode::Variable => Ok(x.clone()),
            ExprNode::Constant(v) => backend.point(v),
            ExprNode::Add(a, b) => {
                let a = a.eval(backend, x, next)?;
                let b = b.eval(backend, x, next)?;
                backend.add(a, b)
            }
            ExprNode::Sub(a, b) => {
                let a = a.eval(backend, x, next)?;
                let b = b.eval(backend, x, next)?;
                backend.sub(a, b)
            }
            ExprNode::Mul(a, b) => {
                let a = a.eval(backend, x, next)?;
                let b = b.eval(backend, x, next)?;
                backend.mul(a, b)
            }
            ExprNode::Div(a, b) => {
                let a = a.eval(backend, x, next)?;
                let b = b.eval(backend, x, next)?;
                backend.div(a, b)
            }
            ExprNode::Neg(a) => {
                let a = a.eval(backend, x, next)?;
                backend.neg(a)
            }
            ExprNode::Exp(a) => {
                let a = a.eval(backend, x, next)?;
                backend.exp(a)
            }
            ExprNode::Ln(a) => {
                let a = a.eval(backend, x, next)?;
                backend.ln(a)
            }
            ExprNode::Sin(a) => {
                let a = a.eval(backend, x, next)?;
                backend.sin(a)
            }
            ExprNode::Cos(a) => {
                let a = a.eval(backend, x, next)?;
                backend.cos(a)
            }
            ExprNode::Sqrt(a) => {
                let a = a.eval(backend, x, next)?;
                backend.sqrt(a)
            }
        }
    }
    fn to_expr(&self) -> Expr {
        self.clone()
    }
    fn metadata(&self) -> crate::ExpressionMetadata {
        self.1
    }
}

/// Open binary64 callable interface. Independent implementations carry no
/// expression proof; approximation requires an explicit consistency assumption.
pub trait Callable {
    /// # Errors
    /// Reports invalid input or failed caller evaluation.
    fn evaluate(&self, x: f64) -> Result<f64>;
    /// # Errors
    /// Reports invalid derivative domains or failed caller enclosures.
    fn jet_interval(&self, x: Interval) -> Result<Jet<Interval>>;
}
impl<E: crate::Expression> Callable for Function<E> {
    fn evaluate(&self, x: f64) -> Result<f64> {
        self.evaluate(x)
    }
    fn jet_interval(&self, x: Interval) -> Result<Jet<Interval>> {
        self.jet_interval(x)
    }
}
impl<S, I> Callable for CallbackFunction<S, I>
where
    S: Fn(f64) -> Result<f64>,
    I: Fn(Interval) -> Result<Jet<Interval>>,
{
    fn evaluate(&self, x: f64) -> Result<f64> {
        self.evaluate(x)
    }
    fn jet_interval(&self, x: Interval) -> Result<Jet<Interval>> {
        self.jet_interval(x)
    }
}

/// Open, statically dispatched evaluator for user-defined mathematical functions.
///
/// Backend-generic code can run with non-Copy arbitrary-precision numbers. This
/// trait has no trusted expression evidence and no bounded-work metadata.
pub trait GenericCallable {
    /// # Errors
    /// Propagates backend arithmetic, domain and caller evaluation failures.
    fn evaluate<B: crate::Backend>(
        &self,
        backend: &mut B,
        x: B::Scalar,
    ) -> std::result::Result<B::Scalar, B::Error>;
}
/// Caller-defined evaluator with its retained mathematical premise.
///
/// Automatic
/// differentiation is conditional on the evaluator honoring backend operations
/// and evaluating the same function in every arithmetic policy.
///
/// ```compile_fail
/// use quest_polynomial::{AssumedFunction, GenericCallable, RemezBuilder};
/// fn forge<C: GenericCallable>(custom: AssumedFunction<C>) {
///     RemezBuilder::new().target(custom);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AssumedFunction<C> {
    callable: C,
    assumption: ConsistencyAssumption,
}
impl<C: GenericCallable> AssumedFunction<C> {
    #[must_use]
    pub const fn new(callable: C, assumption: ConsistencyAssumption) -> Self {
        Self {
            callable,
            assumption,
        }
    }
    #[must_use]
    pub const fn assumption(&self) -> ConsistencyAssumption {
        self.assumption
    }
    /// # Errors
    /// Propagates failures from the user evaluator and arithmetic backend.
    pub fn evaluate_backend<B: crate::Backend>(
        &self,
        backend: &mut B,
        x: B::Scalar,
    ) -> std::result::Result<B::Scalar, B::Error> {
        self.callable.evaluate(backend, x)
    }
    /// # Errors
    /// Propagates failures in value or derivative arithmetic. Derivative meaning
    /// requires the retained caller consistency assumption.
    pub fn jet_backend<B: crate::Backend>(
        &self,
        backend: &mut B,
        x: B::Scalar,
    ) -> std::result::Result<Jet<B::Scalar>, B::Error> {
        let mut jet = crate::JetBackend(backend);
        let x = jet.variable(x)?;
        self.callable.evaluate(&mut jet, x)
    }
}
impl<C: GenericCallable> Callable for AssumedFunction<C> {
    fn evaluate(&self, x: f64) -> Result<f64> {
        self.evaluate_backend(&mut crate::ScalarBackend::new(), checked(x)?)
    }
    fn jet_interval(&self, x: Interval) -> Result<Jet<Interval>> {
        self.jet_backend(&mut crate::ScalarBackend::new(), x)
    }
}
