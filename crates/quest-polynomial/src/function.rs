use crate::{Error, Interval, Result};
use std::{
    ops::{Add, Div, Mul, Neg, Sub},
    sync::Arc,
};

/// One mathematical expression shared by scalar and interval evaluation.
#[derive(Clone, Debug)]
pub struct Expr(Arc<Node>);
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
        Self(Arc::new(Node::Variable))
    }
    #[must_use]
    pub fn exp(self) -> Self {
        Self(Arc::new(Node::Exp(self)))
    }
    #[must_use]
    pub fn ln(self) -> Self {
        Self(Arc::new(Node::Ln(self)))
    }
    #[must_use]
    pub fn sin(self) -> Self {
        Self(Arc::new(Node::Sin(self)))
    }
    #[must_use]
    pub fn cos(self) -> Self {
        Self(Arc::new(Node::Cos(self)))
    }
    #[must_use]
    pub fn sqrt(self) -> Self {
        Self(Arc::new(Node::Sqrt(self)))
    }
    fn jet<T: Real>(&self, x: T, depth: u16) -> Result<Jet<T>> {
        let next = depth
            .checked_sub(1)
            .ok_or(Error::Budget("expression depth"))?;
        match self.0.as_ref() {
            Node::Variable => Ok(Jet {
                value: x,
                first: T::point(1.0)?,
                second: T::point(0.0)?,
            }),
            Node::Constant(c) => Jet::constant(T::point(*c)?),
            Node::Add(a, b) => a.jet(x, next)?.add(b.jet(x, next)?),
            Node::Sub(a, b) => a.jet(x, next)?.sub(b.jet(x, next)?),
            Node::Mul(a, b) => a.jet(x, next)?.mul(b.jet(x, next)?),
            Node::Div(a, b) => a.jet(x, next)?.mul(b.jet(x, next)?.recip()?),
            Node::Neg(a) => a.jet(x, next)?.neg(),
            Node::Exp(a) => {
                let a = a.jet(x, next)?;
                let v = a.value.exp()?;
                a.chain(v, v, v)
            }
            Node::Ln(a) => {
                let a = a.jet(x, next)?;
                let inv = T::point(1.0)?.div(a.value)?;
                a.chain(a.value.ln()?, inv, inv.mul(inv)?.neg()?)
            }
            Node::Sin(a) => {
                let a = a.jet(x, next)?;
                let v = a.value.sin()?;
                a.chain(v, a.value.cos()?, v.neg()?)
            }
            Node::Cos(a) => {
                let a = a.jet(x, next)?;
                let v = a.value.cos()?;
                a.chain(v, a.value.sin()?.neg()?, v.neg()?)
            }
            Node::Sqrt(a) => {
                let a = a.jet(x, next)?;
                let v = a.value.sqrt()?;
                let d = T::point(0.5)?.div(v)?;
                a.chain(v, d, T::point(-0.25)?.div(v.mul(v)?.mul(v)?)?)
            }
        }
    }
    fn value<T: Real>(&self, x: T, depth: u16) -> Result<T> {
        let next = depth
            .checked_sub(1)
            .ok_or(Error::Budget("expression depth"))?;
        match self.0.as_ref() {
            Node::Variable => Ok(x),
            Node::Constant(v) => T::point(*v),
            Node::Add(a, b) => a.value(x, next)?.add(b.value(x, next)?),
            Node::Sub(a, b) => a.value(x, next)?.sub(b.value(x, next)?),
            Node::Mul(a, b) => a.value(x, next)?.mul(b.value(x, next)?),
            Node::Div(a, b) => a.value(x, next)?.div(b.value(x, next)?),
            Node::Neg(a) => a.value(x, next)?.neg(),
            Node::Exp(a) => a.value(x, next)?.exp(),
            Node::Ln(a) => a.value(x, next)?.ln(),
            Node::Sin(a) => a.value(x, next)?.sin(),
            Node::Cos(a) => a.value(x, next)?.cos(),
            Node::Sqrt(a) => a.value(x, next)?.sqrt(),
        }
    }
}
impl From<f64> for Expr {
    fn from(x: f64) -> Self {
        Self(Arc::new(Node::Constant(x)))
    }
}
macro_rules! binary {
    ($trait:ident,$method:ident,$node:ident) => {
        impl<T: Into<Expr>> $trait<T> for Expr {
            type Output = Self;
            fn $method(self, rhs: T) -> Self {
                Self(Arc::new(Node::$node(self, rhs.into())))
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
binary!(Div, div, Div);
impl Neg for Expr {
    type Output = Self;
    fn neg(self) -> Self {
        Self(Arc::new(Node::Neg(self)))
    }
}
/// Immutable expression. Evaluators allocate no heap memory.
#[derive(Clone, Debug)]
pub struct Function {
    expression: Expr,
}
impl Function {
    #[must_use]
    pub const fn expression(&self) -> &Expr {
        &self.expression
    }
    #[must_use]
    pub const fn new(expression: Expr) -> Self {
        Self { expression }
    }
    /// # Errors
    /// Rejects invalid domains, nonfinite arithmetic, or expression depth above 256.
    pub fn evaluate(&self, x: f64) -> Result<f64> {
        self.expression.value(checked(x)?, 256)
    }
    /// # Errors
    /// Rejects any undefined point in the entire interval or unbounded arithmetic.
    pub fn evaluate_interval(&self, x: Interval) -> Result<Interval> {
        self.expression.value(x, 256)
    }
    /// # Errors
    /// Rejects domains where either derivative cannot be enclosed finitely.
    pub fn jet_interval(&self, x: Interval) -> Result<Jet<Interval>> {
        self.expression.jet(x, 256)
    }
    /// # Errors
    /// Rejects domains where the expression or its derivatives are undefined.
    pub fn derivative_interval(&self, x: Interval) -> Result<Interval> {
        Ok(self.jet_interval(x)?.first)
    }
    /// # Errors
    /// Rejects undefined or nonfinite value or derivative arithmetic.
    pub fn jet(&self, x: f64) -> Result<Jet<f64>> {
        self.expression.jet(checked(x)?, 256)
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
        $crate::Function::new(($expression).into())
    }};
}
/// Value and its first two mathematical derivatives.
#[derive(Clone, Copy, Debug)]
pub struct Jet<T> {
    pub value: T,
    pub first: T,
    pub second: T,
}
pub trait Real: Copy {
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
        Ok(Self {
            value: self.value.add(rhs.value)?,
            first: self.first.add(rhs.first)?,
            second: self.second.add(rhs.second)?,
        })
    }
    pub(crate) fn sub(self, rhs: Self) -> Result<Self> {
        self.add(rhs.neg()?)
    }
    pub(crate) fn neg(self) -> Result<Self> {
        Ok(Self {
            value: self.value.neg()?,
            first: self.first.neg()?,
            second: self.second.neg()?,
        })
    }
    pub(crate) fn mul(self, rhs: Self) -> Result<Self> {
        Ok(Self {
            value: self.value.mul(rhs.value)?,
            first: self.first.mul(rhs.value)?.add(self.value.mul(rhs.first)?)?,
            second: self
                .second
                .mul(rhs.value)?
                .add(T::point(2.0)?.mul(self.first)?.mul(rhs.first)?)?
                .add(self.value.mul(rhs.second)?)?,
        })
    }
    fn chain(self, value: T, first: T, second: T) -> Result<Self> {
        Ok(Self {
            value,
            first: first.mul(self.first)?,
            second: second
                .mul(self.first)?
                .mul(self.first)?
                .add(first.mul(self.second)?)?,
        })
    }
    pub(crate) fn recip(self) -> Result<Self> {
        let v = T::point(1.0)?.div(self.value)?;
        self.chain(v, v.mul(v)?.neg()?, T::point(2.0)?.mul(v)?.mul(v)?.mul(v)?)
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
