//! Concrete expression nodes. Structure is known to the compiler; arithmetic is
//! supplied by one backend and every executed node is charged at runtime.
use quest_numerics::arithmetic::{ArithmeticError, Backend, ExactConstant};
use std::{
	marker::Destruct,
	ops::{Add, Div, Mul, Neg, Sub},
};

pub(crate) mod sealed {
	pub trait Sealed {}
}
/// Library expressions are sealed: an arbitrary implementation cannot manufacture
/// structural consistency evidence.
/// ```compile_fail
/// use quest_polynomial::{Expression, ExpressionMetadata};
/// use quest_numerics::arithmetic::Backend;
/// struct Forged;
/// impl Expression for Forged {
///     const METADATA: ExpressionMetadata = ExpressionMetadata::leaf();
///     fn eval<B: Backend>(&self, _: &mut B, inputs: &[B::Scalar]) -> Result<B::Scalar, B::Error> {
///         Ok(inputs[0].clone())
///     }
/// }
/// ```
pub trait Expression: sealed::Sealed {
	const METADATA: ExpressionMetadata;
	/// Evaluate using exactly the operations of the selected backend.
	/// # Errors
	/// Propagates arithmetic, domain, input-shape and budget failures.
	fn eval<B: Backend>(
		&self,
		backend: &mut B,
		inputs: &[B::Scalar],
	) -> Result<B::Scalar, B::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpressionMetadata {
	pub nodes: usize,
	pub depth: usize,
	pub operations: usize,
	pub positive_jet_arguments: usize,
	pub nonzero_denominators: usize,
	pub inputs: usize,
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
			inputs: 0,
		}
	}
	#[must_use]
	pub const fn variable(index: usize) -> Self {
		Self {
			inputs: index.saturating_add(1),
			..Self::leaf()
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
			depth: (if a.depth > b.depth { a.depth } else { b.depth }).saturating_add(1),
			operations: a.operations.saturating_add(b.operations).saturating_add(1),
			positive_jet_arguments: a
				.positive_jet_arguments
				.saturating_add(b.positive_jet_arguments),
			nonzero_denominators: a
				.nonzero_denominators
				.saturating_add(b.nonzero_denominators),
			inputs: if a.inputs > b.inputs {
				a.inputs
			} else {
				b.inputs
			},
		}
	}
}
/// Const-capable structural introspection; this does not execute arithmetic or
/// remove numerical resource obligations.
pub const trait StaticExpression: Expression {
	fn static_metadata(&self) -> ExpressionMetadata {
		Self::METADATA
	}
}
#[derive(Clone, Copy, Debug)]
pub struct Typed<E>(pub(crate) E);
#[derive(Clone, Copy, Debug)]
pub struct VariableNode<const I: usize = 0>;
#[derive(Clone, Copy, Debug)]
pub struct ConstantNode(f64);
#[derive(Clone, Debug)]
pub struct ExactNode(ExactConstant);
#[must_use]
pub const fn variable<const I: usize>() -> Typed<VariableNode<I>> {
	Typed(VariableNode)
}
#[must_use]
pub const fn constant(value: f64) -> Typed<ConstantNode> {
	Typed(ConstantNode(value))
}
#[must_use]
pub const fn exact(value: ExactConstant) -> Typed<ExactNode> {
	Typed(ExactNode(value))
}
impl<E: Expression> sealed::Sealed for Typed<E> {}
impl<E: Expression> Expression for Typed<E> {
	const METADATA: ExpressionMetadata = E::METADATA;
	#[inline]
	fn eval<B: Backend>(&self, b: &mut B, inputs: &[B::Scalar]) -> Result<B::Scalar, B::Error> {
		self.0.eval(b, inputs)
	}
}
const impl<E: Expression> StaticExpression for Typed<E> {}
impl<const I: usize> sealed::Sealed for VariableNode<I> {}
impl sealed::Sealed for ConstantNode {}
impl sealed::Sealed for ExactNode {}
const impl<const I: usize> StaticExpression for VariableNode<I> {}
const impl StaticExpression for ConstantNode {}
const impl StaticExpression for ExactNode {}
impl<const I: usize> Expression for VariableNode<I> {
	const METADATA: ExpressionMetadata = ExpressionMetadata::variable(I);
	fn eval<B: Backend>(&self, b: &mut B, inputs: &[B::Scalar]) -> Result<B::Scalar, B::Error> {
		b.visit()?;
		let input = inputs
			.get(I)
			.ok_or_else(|| B::Error::from(ArithmeticError::Domain("expression input shape")))?;
		b.validate(input)?;
		Ok(input.clone())
	}
}
impl Expression for ConstantNode {
	const METADATA: ExpressionMetadata = ExpressionMetadata::leaf();
	fn eval<B: Backend>(&self, b: &mut B, _: &[B::Scalar]) -> Result<B::Scalar, B::Error> {
		b.visit()?;
		b.point(self.0)
	}
}
impl Expression for ExactNode {
	const METADATA: ExpressionMetadata = ExpressionMetadata::leaf();
	fn eval<B: Backend>(&self, b: &mut B, _: &[B::Scalar]) -> Result<B::Scalar, B::Error> {
		b.visit()?;
		b.constant(&self.0)
	}
}
macro_rules! binary_node {
    ($node:ident,$trait:ident,$method:ident $(,$requirement:ident)?) => {
        #[derive(Clone,Copy,Debug)] pub struct $node<L,R>(L,R);
        impl<L:Expression,R:Expression> sealed::Sealed for $node<L,R>{}
        const impl<L:Expression,R:Expression> StaticExpression for $node<L,R>{}
        impl<L:Expression,R:Expression> Expression for $node<L,R>{
            const METADATA:ExpressionMetadata=ExpressionMetadata::binary(L::METADATA,R::METADATA)$(.$requirement())?;
            fn eval<B:Backend>(&self,b:&mut B,inputs:&[B::Scalar])->Result<B::Scalar,B::Error>{
                b.visit()?; let left=self.0.eval(b,inputs)?; let right=self.1.eval(b,inputs)?; b.$method(left,right)
            }
        }
        const impl<L:[const] Destruct,R:[const] Destruct> $trait<Typed<R>> for Typed<L>{
            type Output=Typed<$node<L,R>>;
            fn $method(self,r:Typed<R>)->Self::Output{Typed($node(self.0,r.0))}
        }
        const impl<L:[const] Destruct> $trait<f64> for Typed<L>{
            type Output=Typed<$node<L,ConstantNode>>;
            fn $method(self,r:f64)->Self::Output{Typed($node(self.0,ConstantNode(r)))}
        }
        const impl<R:[const] Destruct> $trait<Typed<R>> for f64{
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
        #[derive(Clone,Copy,Debug)] pub struct $node<E>(E);
        impl<E:Expression> sealed::Sealed for $node<E>{}
        const impl<E:Expression> StaticExpression for $node<E>{}
        impl<E:Expression> Expression for $node<E>{
            const METADATA:ExpressionMetadata=ExpressionMetadata::unary(E::METADATA)$(.$requirement())?;
            fn eval<B:Backend>(&self,b:&mut B,inputs:&[B::Scalar])->Result<B::Scalar,B::Error>{
                b.visit()?; let value=self.0.eval(b,inputs)?; b.$method(value)
            }
        }
        impl<E> Typed<E>{
            #[must_use] pub const fn $method(self)->Typed<$node<E>> where E:[const] Destruct {Typed($node(self.0))}
        }
    };
}
unary_node!(Negative, neg);
unary_node!(Exponential, exp);
unary_node!(Logarithm, ln, positive_jet);
unary_node!(Sine, sin);
unary_node!(Cosine, cos);
unary_node!(SquareRoot, sqrt, positive_jet);
const impl<E: [const] Destruct> Neg for Typed<E> {
	type Output = Typed<Negative<E>>;
	fn neg(self) -> Self::Output {
		Typed(Negative(self.0))
	}
}
/// Constant-only and expression bodies share the same macro grammar.
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
	type Output = Typed<ConstantNode>;
	fn into_typed(self) -> Self::Output {
		constant(self)
	}
}
pub const fn capture<T: [const] IntoTyped>(value: T) -> T::Output {
	value.into_typed()
}

/// Construct a concrete expression without an interpreted tree or runtime
/// dispatch. Repeated variable leaves are Copy; owned constants are moved once.
#[macro_export]
macro_rules! function {
    (|$($x:ident),+| [$($expression:expr),+ $(,)?]) => {{
        $crate::function!(@bind 0usize; $($x),+);
        $crate::System::<_,{$crate::function!(@count $($x),+)},{$crate::function!(@outputs $($expression),+)}>::new(($($crate::typed::capture($expression),)+))
    }};
    (@outputs $head:expr $(,$tail:expr)*) => {1usize $(+ $crate::function!(@output $tail))*};
    (@output $x:expr) => {1usize};
    (|$x:ident| $expression:expr) => {{
        let $x=$crate::typed::variable::<0>();
        $crate::Function::<_,1>::new($crate::typed::capture($expression))
    }};
    (|$($x:ident),+| $expression:expr) => {{
        $crate::function!(@bind 0usize; $($x),+);
        $crate::Function::<_,{$crate::function!(@count $($x),+)}>::new($crate::typed::capture($expression))
    }};
    (@bind $index:expr; $head:ident $(,$tail:ident)*) => {
        let $head=$crate::typed::variable::<{$index}>();
        $crate::function!(@bind ($index+1usize); $($tail),*);
    };
    (@bind $index:expr;) => {};
    (@count $head:ident $(,$tail:ident)*) => {1usize $(+ $crate::function!(@one $tail))*};
    (@one $x:ident) => {1usize};
}
