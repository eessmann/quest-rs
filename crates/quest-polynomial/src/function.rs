//! Function ownership and the single open extension contract.
use crate::{Expression, ExpressionMetadata};
use quest_numerics::{
	ad::{First, FirstBackend, Jet, JetBackend},
	arithmetic::Backend,
};

const fn assert_dimension<E: Expression, const N: usize>() {
	assert!(N > 0, "function dimension must be positive");
	assert!(
		E::METADATA.inputs <= N,
		"expression input dimension mismatch"
	);
}

/// An owned mathematical expression. The type retains every operation and exact
/// captured source constant, including non-Copy decimal and rational values.
///
/// Const dimensions cannot admit an out-of-range structural variable:
/// ```compile_fail
/// use quest_polynomial::{Function, typed};
/// let invalid = Function::<_, 1>::new(typed::variable::<1>());
/// ```
/// First-order evaluation does not expose an uncomputed second derivative:
/// ```compile_fail
/// use quest_polynomial::{function, GenericFunction};
/// use quest_numerics::arithmetic::F64Backend;
/// let value = function!(|x| x*x).first(&mut F64Backend, 1.0).unwrap();
/// let second = value.second;
/// ```
#[derive(Clone, Debug)]
pub struct Function<E, const N: usize = 1> {
	expression: E,
}
impl<E: Expression, const N: usize> Function<E, N> {
	#[must_use]
	pub const fn new(expression: E) -> Self {
		const { assert_dimension::<E, N>() };
		Self { expression }
	}
	#[must_use]
	pub const fn expression(&self) -> &E {
		&self.expression
	}
	#[must_use]
	pub const fn metadata(&self) -> ExpressionMetadata {
		E::METADATA
	}
	#[must_use]
	pub const fn static_metadata(&self) -> ExpressionMetadata
	where
		E: [const] crate::StaticExpression,
	{
		self.expression.static_metadata()
	}
	/// Capture the shared ordered expression for bounded symbolic construction.
	/// # Errors
	/// Rejects duplicate scoped symbols and expression resource limits.
	pub fn dynamic(
		&self,
		symbols: &[mathcore::identity::Symbol; N],
		limits: mathcore::dynamic::ExpressionLimits,
	) -> Result<mathcore::dynamic::DynamicExpression, mathcore::arithmetic::ArithmeticError> {
		mathcore::dynamic::DynamicExpression::from_typed(&self.expression, symbols, limits)
	}
	/// # Errors
	/// Propagates domain, arithmetic and work-limit failures.
	pub fn evaluate_inputs<B: Backend>(
		&self,
		b: &mut B,
		inputs: &[B::Scalar; N],
	) -> Result<B::Scalar, B::Error> {
		for input in inputs {
			b.validate(input)?;
		}
		self.expression.eval(b, inputs)
	}
	/// Structurally derive the scalar output's first-order Jacobian.
	/// # Errors
	/// Propagates derivative-domain, workspace-admission and arithmetic failures.
	pub fn jacobian<B: Backend>(
		&self,
		b: &mut B,
		inputs: [B::Scalar; N],
		limits: quest_numerics::ad::JacobianLimits,
	) -> Result<quest_numerics::ad::Jacobian<B::Scalar, 1, N>, B::Error> {
		quest_numerics::ad::jacobian(b, inputs, limits, |ad, seeds| {
			Ok(vec![self.expression.eval(ad, seeds)?])
		})
	}
}
/// The only open function extension interface. Implementations must use backend
/// arithmetic consistently; their mathematical claims require explicit premises.
pub trait GenericFunction {
	/// # Errors
	/// Propagates arithmetic, domain, work-limit and user-function failures.
	fn evaluate<B: Backend>(&self, backend: &mut B, x: B::Scalar) -> Result<B::Scalar, B::Error>;
	/// # Errors
	/// Propagates first-derivative-domain and arithmetic failures.
	fn first<B: Backend>(
		&self,
		backend: &mut B,
		x: B::Scalar,
	) -> Result<First<B::Scalar>, B::Error> {
		let mut ad = FirstBackend(backend);
		let variable = ad.variable(x)?;
		self.evaluate(&mut ad, variable)
	}
	/// # Errors
	/// Propagates second-derivative-domain and arithmetic failures.
	fn jet<B: Backend>(&self, backend: &mut B, x: B::Scalar) -> Result<Jet<B::Scalar>, B::Error> {
		let mut ad = JetBackend(backend);
		let variable = ad.variable(x)?;
		self.evaluate(&mut ad, variable)
	}
}
impl<E: Expression> GenericFunction for Function<E> {
	fn evaluate<B: Backend>(&self, b: &mut B, x: B::Scalar) -> Result<B::Scalar, B::Error> {
		b.validate(&x)?;
		self.expression.eval(b, &[x])
	}
}
/// Mathematical premise, never inferred from static dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsistencyAssumption {
	/// One pure function is evaluated in each arithmetic policy, and backend
	/// operations preserve their documented mathematical meanings.
	SameFunctionAndValidEnclosures,
}
#[derive(Clone, Debug)]
pub struct AssumedFunction<C> {
	callable: C,
	assumption: ConsistencyAssumption,
}
impl<C: GenericFunction> AssumedFunction<C> {
	#[must_use]
	pub const fn new(callable: C, assumption: ConsistencyAssumption) -> Self {
		Self {
			callable,
			assumption,
		}
	}
	#[must_use]
	pub const fn callable(&self) -> &C {
		&self.callable
	}
	#[must_use]
	pub const fn assumption(&self) -> ConsistencyAssumption {
		self.assumption
	}
}
impl<C: GenericFunction> GenericFunction for AssumedFunction<C> {
	fn evaluate<B: Backend>(&self, b: &mut B, x: B::Scalar) -> Result<B::Scalar, B::Error> {
		self.callable.evaluate(b, x)
	}
}
mod sealed {
	pub trait Admitted {}
	pub trait Vector {}
}
/// Targets admitted to numerical proof orchestration. Library expressions carry
/// structural evidence; extensions retain their explicit consistency premise.
pub trait AdmittedFunction: GenericFunction + sealed::Admitted {
	type Evidence;
	fn premise(&self) -> Option<ConsistencyAssumption>;
	fn structure(&self) -> Option<ExpressionMetadata>;
}
impl<E: Expression> sealed::Admitted for Function<E> {}
impl<E: Expression> AdmittedFunction for Function<E> {
	type Evidence = Structural;
	fn premise(&self) -> Option<ConsistencyAssumption> {
		None
	}
	fn structure(&self) -> Option<ExpressionMetadata> {
		Some(E::METADATA)
	}
}
impl<C: GenericFunction> sealed::Admitted for AssumedFunction<C> {}
impl<C: GenericFunction> AdmittedFunction for AssumedFunction<C> {
	type Evidence = Assumed;
	fn premise(&self) -> Option<ConsistencyAssumption> {
		Some(self.assumption)
	}
	fn structure(&self) -> Option<ExpressionMetadata> {
		None
	}
}

/// Marker for consistency derived from sealed library expressions.
#[derive(Debug)]
pub struct Structural;
/// Marker for an explicitly assumed mathematical contract.
#[derive(Debug)]
pub struct Assumed;

/// Statically shaped vector expression; each output owns its concrete tree.
#[derive(Clone, Debug)]
pub struct System<E, const N: usize, const M: usize> {
	expression: E,
}
pub trait VectorExpression<const M: usize>: sealed::Vector {
	const INPUTS: usize;
	/// # Errors
	/// Propagates input-shape, domain and backend failures.
	fn evaluate<B: Backend>(
		&self,
		b: &mut B,
		inputs: &[B::Scalar],
	) -> Result<[B::Scalar; M], B::Error>;
	/// # Errors
	/// Propagates storage, input-shape, domain and backend failures.
	fn evaluate_vec<B: Backend>(
		&self,
		b: &mut B,
		inputs: &[B::Scalar],
	) -> Result<Vec<B::Scalar>, B::Error>;
}
const fn assert_system<E: VectorExpression<M>, const N: usize, const M: usize>() {
	assert!(N > 0 && M > 0, "system dimensions must be positive");
	assert!(E::INPUTS <= N, "system input dimension mismatch");
}
impl<E: VectorExpression<M>, const N: usize, const M: usize> System<E, N, M> {
	#[must_use]
	pub const fn new(expression: E) -> Self {
		const { assert_system::<E, N, M>() };
		Self { expression }
	}
	/// # Errors
	/// Propagates arithmetic, domain and work-limit failures.
	pub fn evaluate<B: Backend>(
		&self,
		b: &mut B,
		inputs: &[B::Scalar; N],
	) -> Result<[B::Scalar; M], B::Error> {
		for input in inputs {
			b.validate(input)?;
		}
		self.expression.evaluate(b, inputs)
	}
	/// Structurally derive the complete first-order Jacobian.
	/// # Errors
	/// Propagates derivative-domain, storage and arithmetic failures.
	pub fn jacobian<B: Backend>(
		&self,
		b: &mut B,
		inputs: [B::Scalar; N],
		limits: quest_numerics::ad::JacobianLimits,
	) -> Result<quest_numerics::ad::Jacobian<B::Scalar, M, N>, B::Error> {
		quest_numerics::ad::jacobian(b, inputs, limits, |ad, seeds| {
			self.expression.evaluate_vec(ad, seeds)
		})
	}
}
macro_rules! vector_expression {
    ($count:literal;$($name:ident:$index:tt),+)=>{
        impl<$($name:Expression),+> sealed::Vector for ($($name,)+){}
        impl<$($name:Expression),+> VectorExpression<$count> for ($($name,)+){
            const INPUTS:usize={let mut n=0;$(if $name::METADATA.inputs>n{n=$name::METADATA.inputs;})+n};
            fn evaluate<B:Backend>(&self,b:&mut B,inputs:&[B::Scalar])->Result<[B::Scalar;$count],B::Error>{Ok([$(self.$index.eval(b,inputs)?),+])}
            fn evaluate_vec<B:Backend>(&self,b:&mut B,inputs:&[B::Scalar])->Result<Vec<B::Scalar>,B::Error>{
                let mut values=Vec::new();values.try_reserve_exact($count).map_err(|_|B::Error::from(mathcore::arithmetic::ArithmeticError::Budget("system values")))?;
                $(values.push(self.$index.eval(b,inputs)?);)+Ok(values)
            }
        }
    }
}
vector_expression!(1;A:0);
vector_expression!(2;A:0,C:1);
vector_expression!(3;A:0,C:1,D:2);
vector_expression!(4;A:0,C:1,D:2,E:3);
vector_expression!(5;A:0,C:1,D:2,E:3,F:4);
vector_expression!(6;A:0,C:1,D:2,E:3,F:4,G:5);
vector_expression!(7;A:0,C:1,D:2,E:3,F:4,G:5,H:6);
vector_expression!(8;A:0,C:1,D:2,E:3,F:4,G:5,H:6,J:7);
