//! Statically dispatched, allocation-free expression trees with const construction.
use crate::{Backend, Error, Expr};
use std::ops::{Add, Div, Mul, Neg, Sub};
pub(crate) mod sealed {
    pub trait Sealed {}
}
/// Only library expression nodes implement this trait. Custom backends are open;
/// custom callables must explicitly use conditional consistency evidence.
///
/// ```compile_fail
/// use quest_polynomial::{Backend, Expr, Expression, ExpressionMetadata};
/// #[derive(Clone, Debug)]
/// struct Forged;
/// impl Expression for Forged {
///     fn eval<B: Backend>(&self, _: &mut B, x: &B::Scalar, _: u16)
///         -> Result<B::Scalar, B::Error> { Ok(x.clone()) }
///     fn to_expr(&self) -> Expr { Expr::variable() }
///     fn metadata(&self) -> ExpressionMetadata { ExpressionMetadata::leaf() }
/// }
/// ```
pub trait Expression: sealed::Sealed + Clone + std::fmt::Debug {
    /// # Errors
    /// Propagates backend errors or an exhausted expression-depth budget.
    fn eval<B: Backend>(
        &self,
        backend: &mut B,
        x: &B::Scalar,
        depth: u16,
    ) -> Result<B::Scalar, B::Error>;
    fn to_expr(&self) -> Expr;
    fn metadata(&self) -> ExpressionMetadata;
}
/// Structural counts, independent of values, numerical precision and budgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpressionMetadata {
    pub nodes: usize,
    pub depth: usize,
    /// Mathematical operations in the value expression (not backend instruction count).
    pub operations: usize,
    /// Number of ln/sqrt nodes whose second-order jet requires positive arguments.
    pub positive_jet_arguments: usize,
    /// Number of division nodes requiring a nonzero denominator.
    pub nonzero_denominators: usize,
}
impl ExpressionMetadata {
    #[must_use]
    pub const fn leaf() -> Self {
        Self {
            nodes: 1,
            depth: 1,
            operations: 0,
            positive_jet_arguments: 0,
            nonzero_denominators: 0,
        }
    }
    #[must_use]
    pub const fn unary(a: Self) -> Self {
        Self {
            nodes: a.nodes.saturating_add(1),
            depth: a.depth.saturating_add(1),
            operations: a.operations.saturating_add(1),
            ..a
        }
    }
    #[must_use]
    pub const fn positive_jet(mut self) -> Self {
        self.positive_jet_arguments = self.positive_jet_arguments.saturating_add(1);
        self
    }
    #[must_use]
    pub const fn denominator(mut self) -> Self {
        self.nonzero_denominators = self.nonzero_denominators.saturating_add(1);
        self
    }
    #[must_use]
    pub const fn binary(a: Self, b: Self) -> Self {
        Self {
            nodes: a.nodes.saturating_add(b.nodes).saturating_add(1),
            depth: if a.depth > b.depth {
                a.depth.saturating_add(1)
            } else {
                b.depth.saturating_add(1)
            },
            operations: a.operations.saturating_add(b.operations).saturating_add(1),
            positive_jet_arguments: a
                .positive_jet_arguments
                .saturating_add(b.positive_jet_arguments),
            nonzero_denominators: a
                .nonzero_denominators
                .saturating_add(b.nonzero_denominators),
        }
    }
}
pub const trait StaticExpression: Expression + Copy {
    const METADATA: ExpressionMetadata;
    fn static_metadata(&self) -> ExpressionMetadata {
        Self::METADATA
    }
}
pub(crate) fn enter<B: Backend>(backend: &mut B, depth: u16) -> Result<u16, B::Error> {
    let next = depth
        .checked_sub(1)
        .ok_or_else(|| B::Error::from(Error::Budget("expression depth")))?;
    backend.visit()?;
    Ok(next)
}
#[derive(Clone, Copy, Debug)]
pub struct Typed<E>(pub(crate) E);
#[derive(Clone, Copy, Debug)]
pub struct VariableNode;
#[derive(Clone, Copy, Debug)]
pub struct ConstantNode(f64);
pub type Variable = Typed<VariableNode>;
pub type Constant = Typed<ConstantNode>;
#[must_use]
pub const fn variable() -> Variable {
    Typed(VariableNode)
}
#[must_use]
pub const fn constant(value: f64) -> Constant {
    Typed(ConstantNode(value))
}
impl<E: Expression> sealed::Sealed for Typed<E> {}
impl<E: Expression> Expression for Typed<E> {
    fn eval<B: Backend>(&self, b: &mut B, x: &B::Scalar, d: u16) -> Result<B::Scalar, B::Error> {
        self.0.eval(b, x, d)
    }
    fn to_expr(&self) -> Expr {
        self.0.to_expr()
    }
    fn metadata(&self) -> ExpressionMetadata {
        self.0.metadata()
    }
}
const impl<E: StaticExpression> StaticExpression for Typed<E> {
    const METADATA: ExpressionMetadata = E::METADATA;
}
impl sealed::Sealed for VariableNode {}
impl sealed::Sealed for ConstantNode {}
const impl StaticExpression for VariableNode {
    const METADATA: ExpressionMetadata = ExpressionMetadata::leaf();
}
const impl StaticExpression for ConstantNode {
    const METADATA: ExpressionMetadata = ExpressionMetadata::leaf();
}
impl Expression for VariableNode {
    fn eval<B: Backend>(&self, b: &mut B, x: &B::Scalar, d: u16) -> Result<B::Scalar, B::Error> {
        enter(b, d)?;
        Ok(x.clone())
    }
    fn to_expr(&self) -> Expr {
        Expr::variable()
    }
    fn metadata(&self) -> ExpressionMetadata {
        Self::METADATA
    }
}
impl Expression for ConstantNode {
    fn eval<B: Backend>(&self, b: &mut B, _x: &B::Scalar, d: u16) -> Result<B::Scalar, B::Error> {
        enter(b, d)?;
        b.point(self.0)
    }
    fn to_expr(&self) -> Expr {
        Expr::from(self.0)
    }
    fn metadata(&self) -> ExpressionMetadata {
        Self::METADATA
    }
}

macro_rules! binary_node {
    ($node:ident,$trait:ident,$method:ident $(,$requirement:ident)?) => {
        #[derive(Clone,Copy,Debug)]
        pub struct $node<L,R>(L,R);
        impl<L:Expression,R:Expression> sealed::Sealed for $node<L,R>{}
        const impl<L:StaticExpression,R:StaticExpression> StaticExpression for $node<L,R>{
            const METADATA:ExpressionMetadata=ExpressionMetadata::binary(L::METADATA,R::METADATA)$(.$requirement())?;
        }
        impl<L:Expression,R:Expression> Expression for $node<L,R>{
            fn eval<B:Backend>(&self,b:&mut B,x:&B::Scalar,d:u16)->Result<B::Scalar,B::Error>{
                let next=enter(b,d)?;let a=self.0.eval(b,x,next)?;let c=self.1.eval(b,x,next)?;b.$method(a,c)
            }
            fn to_expr(&self)->Expr{self.0.to_expr().$method(self.1.to_expr())}
            fn metadata(&self)->ExpressionMetadata{ExpressionMetadata::binary(self.0.metadata(),self.1.metadata())$(.$requirement())?}
        }
        const impl<L:Copy,R:Copy> $trait<Typed<R>> for Typed<L>{
            type Output=Typed<$node<L,R>>;
            fn $method(self,r:Typed<R>)->Self::Output{Typed($node(self.0,r.0))}
        }
        const impl<L:Copy> $trait<f64> for Typed<L>{
            type Output=Typed<$node<L,ConstantNode>>;
            fn $method(self,r:f64)->Self::Output{Typed($node(self.0,ConstantNode(r)))}
        }
        const impl<R:Copy> $trait<Typed<R>> for f64{
            type Output=Typed<$node<ConstantNode,R>>;
            fn $method(self,r:Typed<R>)->Self::Output{Typed($node(ConstantNode(self),r.0))}
        }
    };
}
binary_node!(Sum, Add, add);
binary_node!(Difference, Sub, sub);
binary_node!(Product, Mul, mul);
binary_node!(Quotient, Div, div, denominator);
macro_rules! unary_node {
    ($node:ident,$method:ident $(,$requirement:ident)?) => {
        #[derive(Clone,Copy,Debug)]
        pub struct $node<E>(E);
        impl<E:Expression> sealed::Sealed for $node<E>{}
        const impl<E:StaticExpression> StaticExpression for $node<E>{
            const METADATA:ExpressionMetadata=ExpressionMetadata::unary(E::METADATA)$(.$requirement())?;
        }
        impl<E:Expression> Expression for $node<E>{
            fn eval<B:Backend>(&self,b:&mut B,x:&B::Scalar,d:u16)->Result<B::Scalar,B::Error>{
                let next=enter(b,d)?;let a=self.0.eval(b,x,next)?;b.$method(a)
            }
            fn to_expr(&self)->Expr{self.0.to_expr().$method()}
            fn metadata(&self)->ExpressionMetadata{ExpressionMetadata::unary(self.0.metadata())$(.$requirement())?}
        }
        impl<E:Copy> Typed<E>{#[must_use]
        pub const fn $method(self)->Typed<$node<E>>{Typed($node(self.0))}}
    };
}
unary_node!(Negative, neg);
unary_node!(Exponential, exp);
unary_node!(Logarithm, ln, positive_jet);
unary_node!(Sine, sin);
unary_node!(Cosine, cos);
unary_node!(SquareRoot, sqrt, positive_jet);
const impl<E: Copy> Neg for Typed<E> {
    type Output = Typed<Negative<E>>;
    fn neg(self) -> Self::Output {
        Typed(Negative(self.0))
    }
}

/// Const conversion used by the macro, including constant-only functions.
pub const trait IntoTyped {
    type Output: Expression;
    fn into_typed(self) -> Self::Output;
}
const impl<E: Expression> IntoTyped for Typed<E> {
    type Output = Self;
    fn into_typed(self) -> Self {
        self
    }
}
const impl IntoTyped for f64 {
    type Output = Constant;
    fn into_typed(self) -> Constant {
        constant(self)
    }
}
pub const fn capture<T: [const] IntoTyped>(value: T) -> T::Output {
    value.into_typed()
}

/// Const-friendly counterpart of `function!`. Typed nodes are Copy, so `x*x`
/// requires no cloning and every operation retains its concrete expression type.
#[macro_export]
macro_rules! typed_function {
    (|$x:ident| $expression:expr) => {{
        #[allow(unused_variables)]
        let $x = $crate::typed::variable();
        $crate::Function::from_expression($crate::typed::capture($expression))
    }};
}
