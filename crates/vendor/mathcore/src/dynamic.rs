//! Bounded ordered dynamic expressions.
//!
//! Formal algebra and numerical evaluation
//! remain separate: simplification keeps its source and differentiation evaluates
//! the source before its derivative, preserving bindings and domain obligations.
use crate::{
	arithmetic::{ArithmeticError as Error, Backend, ExactConstant},
	exact::Symbol,
	multivariate::{PolynomialLimits, SparsePolynomial, rational_constant},
	scalar::{BinaryOperation, UnaryOperation},
	scope::{Scope, ScopePlan},
	typed::Expression,
};
use dashu_base::BitTest;
use std::{collections::BTreeMap, sync::Arc};
type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug)]
pub struct ExpressionLimits {
	pub max_nodes: usize,
	pub max_depth: usize,
	pub max_bytes: usize,
	pub max_work: usize,
	pub polynomial: PolynomialLimits,
}
impl Default for ExpressionLimits {
	fn default() -> Self {
		Self {
			max_nodes: 65536,
			max_depth: 128,
			max_bytes: 64 * 1024 * 1024,
			max_work: 16 * 1024 * 1024,
			polynomial: PolynomialLimits::default(),
		}
	}
}
#[derive(Clone, Debug)]
enum Node {
	Constant(ExactConstant),
	Variable(Symbol),
	Unary(UnaryOperation, DynamicExpression),
	Binary(BinaryOperation, DynamicExpression, DynamicExpression),
	Guard(DynamicExpression, DynamicExpression),
}
#[derive(Clone, Debug)]
pub struct DynamicExpression {
	node: Arc<Node>,
	limits: ExpressionLimits,
	nodes: usize,
	depth: usize,
	bytes: usize,
	coefficient_bits: usize,
	work: usize,
}
impl DynamicExpression {
	fn node_storage() -> Option<usize> {
		size_of::<Node>()
			.checked_add(size_of::<Self>())
			.and_then(|n| n.checked_add(const { 2 * size_of::<usize>() }))
	}
	fn constant_storage(value: &ExactConstant) -> Result<usize> {
		match value {
			ExactConstant::Decimal(text) => Ok(text.capacity()),
			ExactConstant::Ratio {
				numerator,
				denominator,
			} => numerator
				.capacity()
				.checked_add(denominator.capacity())
				.ok_or(Error::Budget("constant storage")),
			_ => Ok(0),
		}
	}
	fn build(node: Node, limits: ExpressionLimits) -> Result<Self> {
		let children = match &node {
			Node::Unary(_, a) => [Some(a), None],
			Node::Binary(_, a, b) | Node::Guard(a, b) => [Some(a), Some(b)],
			_ => [None, None],
		};

		let mut nodes = 1_usize;
		let mut work = 1_usize;
		let mut depth = 1_usize;
		let mut coefficient_bits = 0;
		let mut bytes = Self::node_storage().ok_or(Error::Budget("expression storage"))?;
		for child in children.into_iter().flatten() {
			work = work
				.checked_add(child.work)
				.ok_or(Error::Budget("expression work"))?;
			coefficient_bits = coefficient_bits.max(child.coefficient_bits);
			nodes = nodes
				.checked_add(child.nodes)
				.ok_or(Error::Budget("expression nodes"))?;
			depth = depth.max(
				child
					.depth
					.checked_add(1)
					.ok_or(Error::Budget("expression depth"))?,
			);
			bytes = bytes
				.checked_add(child.bytes)
				.ok_or(Error::Budget("expression storage"))?;
		}
		if let Node::Constant(value) = &node {
			let digits = match value {
				ExactConstant::Decimal(s) => s.capacity(),
				ExactConstant::Ratio {
					numerator,
					denominator,
				} => numerator
					.capacity()
					.checked_add(denominator.capacity())
					.ok_or(Error::Budget("constant storage"))?,
				_ => 0,
			};
			bytes = bytes
				.checked_add(digits)
				.ok_or(Error::Budget("constant storage"))?;
		}
		if coefficient_bits > limits.polynomial.max_coefficient_bits
			|| nodes > limits.max_nodes
			|| work > limits.max_work
			|| depth > limits.max_depth
			|| depth > 256
			|| bytes > limits.max_bytes
		{
			return Err(Error::Budget("expression structure"));
		}
		Ok(Self {
			node: Arc::new(node),
			limits,
			nodes,
			depth,
			bytes,
			coefficient_bits,
			work,
		})
	}
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn constant(value: ExactConstant, limits: ExpressionLimits) -> Result<Self> {
		let mut output = Self::build(Node::Constant(value), limits)?;
		if let Node::Constant(value) = output.node.as_ref()
			&& !matches!(value, ExactConstant::Pi)
		{
			let rational = rational_constant(value, limits.polynomial)?;
			output.coefficient_bits = rational
				.numerator()
				.bit_len()
				.max(rational.denominator().bit_len());
		}

		Ok(output)
	}

	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn variable(symbol: Symbol, limits: ExpressionLimits) -> Result<Self> {
		Self::build(Node::Variable(symbol), limits)
	}
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn unary(&self, operation: UnaryOperation) -> Result<Self> {
		Self::build(Node::Unary(operation, self.clone()), self.limits)
	}
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn binary(&self, operation: BinaryOperation, rhs: &Self) -> Result<Self> {
		if operation == BinaryOperation::Power {
			return Err(Error::Domain(
				"dynamic power requires explicit multiplication",
			));
		}
		Self::build(
			Node::Binary(operation, self.clone(), rhs.clone()),
			self.limits,
		)
	}
	#[must_use]
	pub const fn logical_nodes(&self) -> usize {
		self.nodes
	}
	#[must_use]
	pub const fn logical_work(&self) -> usize {
		self.work
	}
	#[must_use]
	pub const fn depth(&self) -> usize {
		self.depth
	}
	#[must_use]
	pub const fn retained_bytes(&self) -> usize {
		self.bytes
	}
	/// Capture a sealed typed expression once, preserving its operand order.
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn from_typed<E: Expression>(
		expression: &E,
		symbols: &[Symbol],
		limits: ExpressionLimits,
	) -> Result<Self> {
		if E::METADATA.nodes > limits.max_nodes
			|| E::METADATA.depth > limits.max_depth
			|| E::METADATA.depth > 256
			|| E::METADATA.inputs > symbols.len()
		{
			return Err(Error::Budget("typed dynamic capture"));
		}
		let unit = Self::node_storage().ok_or(Error::Budget("typed dynamic capture storage"))?;
		let input_bytes = symbols
			.len()
			.checked_mul(unit)
			.and_then(|n| n.checked_add(size_of::<Vec<Self>>()))
			.ok_or(Error::Budget("typed dynamic capture storage"))?;
		let output_bytes = E::METADATA
			.nodes
			.checked_mul(unit)
			.ok_or(Error::Budget("typed dynamic capture storage"))?;
		let fixed_bytes = input_bytes
			.checked_add(output_bytes)
			.ok_or(Error::Budget("typed dynamic capture storage"))?;
		let plan = ScopePlan::new(
			symbols.len(),
			limits.polynomial.max_variables,
			limits.max_bytes,
			limits.max_work,
			0,
		)?;
		let work = plan
			.work
			.checked_add(symbols.len())
			.and_then(|n| n.checked_add(E::METADATA.nodes))
			.ok_or(Error::Budget("typed dynamic capture work"))?;
		if work > limits.max_work
			|| symbols
				.len()
				.checked_add(E::METADATA.nodes)
				.is_none_or(|n| n > limits.max_nodes)
			|| fixed_bytes > limits.max_bytes
		{
			return Err(Error::Budget("typed dynamic capture admission"));
		}
		// The uniqueness buffer is dropped before capture inputs/output coexist.
		drop(plan.validate(symbols)?);
		let mut inputs = Vec::new();
		inputs
			.try_reserve_exact(symbols.len())
			.map_err(|_| Error::Budget("typed dynamic capture allocation"))?;
		for symbol in symbols {
			inputs.push(Self::variable(*symbol, limits)?);
		}
		let mut backend = ConstructionBackend {
			limits,
			work: plan
				.work
				.checked_add(symbols.len())
				.ok_or(Error::Budget("typed dynamic capture work"))?,
			fixed_bytes,
			constant_bytes: 0,
		};
		let mut output = expression.eval(&mut backend, &inputs)?;
		output.work = backend.work;
		Ok(output)
	}
	/// Simultaneous substitution traverses the source, including retained guards.
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn substitute(&self, replacements: &BTreeMap<Symbol, Self>) -> Result<Self> {
		let mut work = 0;
		self.substitute_inner(replacements, &mut work)
	}
	fn tick(&self, work: &mut usize) -> Result<()> {
		*work = work
			.checked_add(1)
			.ok_or(Error::Budget("expression work"))?;
		if *work > self.limits.max_work {
			return Err(Error::Budget("expression work"));
		}
		Ok(())
	}
	fn substitute_inner(
		&self,
		replacements: &BTreeMap<Symbol, Self>,
		work: &mut usize,
	) -> Result<Self> {
		self.tick(work)?;
		match self.node.as_ref() {
			Node::Variable(s) => replacements.get(s).map_or_else(
				|| Ok(self.clone()),
				|value| {
					if value.coefficient_bits > self.limits.polynomial.max_coefficient_bits
						|| value.work > self.limits.max_work
					{
						return Err(Error::Budget("expression coefficient bits"));
					}
					let mut output = Self::build(value.node.as_ref().clone(), self.limits)?;
					output.coefficient_bits = value.coefficient_bits;
					output.work = value.work;
					Ok(output)
				},
			),
			Node::Constant(_) => Ok(self.clone()),
			Node::Unary(op, a) => a.substitute_inner(replacements, work)?.unary(*op),
			Node::Binary(op, a, b) => a
				.substitute_inner(replacements, work)?
				.binary(*op, &b.substitute_inner(replacements, work)?),
			Node::Guard(a, b) => Self::build(
				Node::Guard(
					a.substitute_inner(replacements, work)?,
					b.substitute_inner(replacements, work)?,
				),
				self.limits,
			),
		}
	}
	/// The derivative retains the original source's domain and binding obligations.
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn differentiate(&self, variable: Symbol) -> Result<Self> {
		let mut work = 0;
		let derivative = self.derivative_inner(variable, &mut work)?;
		Self::build(Node::Guard(self.clone(), derivative), self.limits)
	}
	fn derivative_inner(&self, s: Symbol, work: &mut usize) -> Result<Self> {
		self.tick(work)?;
		let c = |v| Self::constant(ExactConstant::Integer(v), self.limits);
		match self.node.as_ref() {
			Node::Constant(_) => c(0),
			Node::Variable(v) => c(i64::from(*v == s)),
			Node::Guard(a, b) => Self::build(
				Node::Guard(a.clone(), b.derivative_inner(s, work)?),
				self.limits,
			),
			Node::Binary(op, a, b) => {
				let da = a.derivative_inner(s, work)?;
				let db = b.derivative_inner(s, work)?;
				match op {
					BinaryOperation::Add | BinaryOperation::Subtract => da.binary(*op, &db),
					BinaryOperation::Multiply => da.binary(BinaryOperation::Multiply, b)?.binary(
						BinaryOperation::Add,
						&a.binary(BinaryOperation::Multiply, &db)?,
					),
					BinaryOperation::Divide => da
						.binary(BinaryOperation::Multiply, b)?
						.binary(
							BinaryOperation::Subtract,
							&a.binary(BinaryOperation::Multiply, &db)?,
						)?
						.binary(
							BinaryOperation::Divide,
							&b.binary(BinaryOperation::Multiply, b)?,
						),
					BinaryOperation::Power => Err(Error::Domain("dynamic power derivative")),
				}
			}
			Node::Unary(op, a) => {
				let da = a.derivative_inner(s, work)?;
				match op {
					UnaryOperation::Negate => da.unary(UnaryOperation::Negate),
					UnaryOperation::Exp => a
						.unary(UnaryOperation::Exp)?
						.binary(BinaryOperation::Multiply, &da),
					UnaryOperation::Ln => da.binary(BinaryOperation::Divide, a),
					UnaryOperation::Sin => a
						.unary(UnaryOperation::Cos)?
						.binary(BinaryOperation::Multiply, &da),
					UnaryOperation::Cos => a
						.unary(UnaryOperation::Sin)?
						.unary(UnaryOperation::Negate)?
						.binary(BinaryOperation::Multiply, &da),
					UnaryOperation::Sqrt => da.binary(
						BinaryOperation::Divide,
						&c(2)?
							.binary(BinaryOperation::Multiply, &a.unary(UnaryOperation::Sqrt)?)?,
					),
				}
			}
		}
	}
	/// Canonical exact polynomial construction. Floating constants, pi and
	/// domain-sensitive non-polynomial operations require an explicit lowering.
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn polynomial(
		&self,
		symbols: &[Symbol],
		limits: PolynomialLimits,
	) -> Result<SparsePolynomial> {
		// Extraction borrows the caller's scope; reject the eventual owned scope,
		// exponent vector and uniqueness scratch before cloning any of them.
		SparsePolynomial::admit_variables(symbols, symbols.len(), limits)?;
		let mut work = 0;
		self.polynomial_inner(symbols, limits, &mut work)
	}
	fn polynomial_inner(
		&self,
		symbols: &[Symbol],
		limits: PolynomialLimits,
		work: &mut usize,
	) -> Result<SparsePolynomial> {
		self.tick(work)?;
		match self.node.as_ref() {
			Node::Constant(value) => {
				if matches!(value, ExactConstant::Binary64(_) | ExactConstant::Pi) {
					return Err(Error::Domain(
						"explicit floating or pi polynomial lowering required",
					));
				}
				SparsePolynomial::constant(
					symbols.to_vec(),
					rational_constant(value, limits)?,
					limits,
				)
			}
			Node::Variable(s) => SparsePolynomial::variable(
				symbols.to_vec(),
				symbols
					.iter()
					.position(|v| v == s)
					.ok_or(Error::Domain("polynomial variable binding"))?,
				limits,
			),
			Node::Unary(UnaryOperation::Negate, a) => {
				a.polynomial_inner(symbols, limits, work)?.negate()
			}
			Node::Binary(op, a, b) => {
				let a = a.polynomial_inner(symbols, limits, work)?;
				let b = b.polynomial_inner(symbols, limits, work)?;
				match op {
					BinaryOperation::Add => a.add(&b),
					BinaryOperation::Subtract => a.subtract(&b),
					BinaryOperation::Multiply => a.multiply(&b),
					_ => Err(Error::Domain("non-polynomial operation")),
				}
			}
			_ => Err(Error::Domain("non-polynomial operation or source guard")),
		}
	}
	/// Exact simplification returns its canonical polynomial alongside the
	/// ordered source; evaluation continues to execute that source.
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn simplify(
		&self,
		symbols: &[Symbol],
		limits: PolynomialLimits,
	) -> Result<PolynomialSimplification> {
		Ok(PolynomialSimplification {
			source: self.clone(),
			polynomial: self.polynomial(symbols, limits)?,
		})
	}
	/// Convert constants and symbol identities once to a bounded execution plan.
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn lower<B: Backend>(
		&self,
		backend: &mut B,
		symbols: &[Symbol],
	) -> std::result::Result<ExpressionKernel<B::Scalar>, B::Error> {
		let capacity = self
			.nodes
			.checked_mul(2)
			.ok_or(Error::Budget("expression kernel"))?;
		let projected = capacity
			.checked_mul(size_of::<Instruction<B::Scalar>>())
			.and_then(|b| b.checked_add(self.nodes.checked_mul(backend.working_scalar_bytes())?))
			.and_then(|b| b.checked_add(self.depth.checked_mul(size_of::<Stored<B::Scalar>>())?))
			.and_then(|b| b.checked_add(self.bytes))
			.ok_or(Error::Budget("expression kernel storage"))?;
		if projected > self.limits.max_bytes {
			return Err(Error::Budget("expression kernel storage").into());
		}
		let plan = ScopePlan::new(
			symbols.len(),
			self.limits.polynomial.max_variables,
			self.limits.max_bytes,
			self.limits.max_work,
			projected,
		)?;
		let work = self
			.work
			.checked_add(plan.work)
			.and_then(|n| n.checked_add(self.nodes.checked_mul(plan.search_work)?))
			.ok_or(Error::Budget("expression lowering work"))?;
		if work > self.limits.max_work {
			return Err(Error::Budget("expression lowering work").into());
		}
		backend.charge(work)?;
		let scope = plan.validate(symbols)?;
		let mut instructions = Vec::new();
		instructions
			.try_reserve_exact(capacity)
			.map_err(|_| Error::Budget("expression kernel"))?;

		self.emit(backend, &scope, &mut instructions)?;
		let mut bytes = instructions
			.capacity()
			.checked_mul(size_of::<Instruction<B::Scalar>>())
			.ok_or(Error::Budget("expression kernel"))?;
		for i in &instructions {
			if let Instruction::Constant(s) = i {
				bytes = bytes
					.checked_add(backend.storage_bytes(s)?)
					.ok_or(Error::Budget("expression kernel"))?;
			}
		}
		let stack_bytes = self
			.nodes
			.checked_mul(backend.working_scalar_bytes())
			.ok_or(Error::Budget("expression stack"))?;
		if bytes
			.checked_add(stack_bytes)
			.and_then(|n| n.checked_add(self.bytes))
			.and_then(|n| n.checked_add(plan.bytes))
			.is_none_or(|b| b > self.limits.max_bytes)
		{
			return Err(Error::Budget("expression kernel storage").into());
		}
		Ok(ExpressionKernel {
			instructions,
			inputs: symbols.len(),
			stack_capacity: self.depth,
			max_bytes: self.limits.max_bytes,
			retained: bytes,
		})
	}
	fn emit<B: Backend>(
		&self,
		backend: &mut B,
		scope: &Scope,
		output: &mut Vec<Instruction<B::Scalar>>,
	) -> std::result::Result<(), B::Error> {
		output.push(Instruction::Visit);
		match self.node.as_ref() {
			Node::Constant(c) => output.push(Instruction::Constant(backend.constant(c)?)),
			Node::Variable(s) => output.push(Instruction::Variable(scope.resolve(*s)?)),
			Node::Unary(op, a) => {
				a.emit(backend, scope, output)?;
				output.push(Instruction::Unary(*op));
			}
			Node::Binary(op, a, b) => {
				a.emit(backend, scope, output)?;
				b.emit(backend, scope, output)?;
				output.push(Instruction::Binary(*op));
			}
			Node::Guard(a, b) => {
				a.emit(backend, scope, output)?;
				output.push(Instruction::Discard);
				b.emit(backend, scope, output)?;
			}
		}
		Ok(())
	}
}
#[derive(Clone, Debug)]
pub struct PolynomialSimplification {
	pub source: DynamicExpression,
	pub polynomial: SparsePolynomial,
}
#[derive(Clone, Debug)]
enum Instruction<S> {
	Visit,
	Constant(S),
	Variable(usize),
	Unary(UnaryOperation),
	Binary(BinaryOperation),
	Discard,
}
#[derive(Clone, Debug)]
pub struct ExpressionKernel<S> {
	instructions: Vec<Instruction<S>>,
	inputs: usize,
	stack_capacity: usize,
	max_bytes: usize,
	retained: usize,
}
impl<S: Clone> ExpressionKernel<S> {
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn evaluate<B: Backend<Scalar = S>>(
		&self,
		backend: &mut B,
		inputs: &[S],
	) -> std::result::Result<S, B::Error> {
		if inputs.len() != self.inputs {
			return Err(Error::Domain("expression input shape").into());
		}
		for input in inputs {
			backend.validate(input)?;
		}
		let input_bytes = inputs.iter().try_fold(
			0_usize,
			|total, value| -> std::result::Result<usize, B::Error> {
				Ok(total
					.checked_add(backend.storage_bytes(value)?)
					.ok_or(Error::Budget("expression inputs"))?)
			},
		)?;
		let base = self
			.retained
			.checked_add(input_bytes)
			.and_then(|b| b.checked_add(self.stack_capacity.checked_mul(size_of::<Stored<S>>())?))
			.ok_or(Error::Budget("expression stack"))?;
		let mut stack = ValueStack::new(self.stack_capacity, base, self.max_bytes)?;
		for instruction in &self.instructions {
			match instruction {
				Instruction::Visit => backend.visit()?,
				Instruction::Constant(value) => {
					backend.validate(value)?;
					stack.admit(backend.storage_bytes(value)?)?;
					stack.push(backend, value.clone())?;
				}
				Instruction::Variable(index) => {
					let value = inputs
						.get(*index)
						.ok_or(Error::Domain("expression input shape"))?;
					backend.validate(value)?;
					stack.admit(backend.storage_bytes(value)?)?;
					stack.push(backend, value.clone())?;
				}
				Instruction::Unary(op) => {
					let a = stack.pop()?;
					stack.admit(
						a.bytes
							.checked_add(backend.working_scalar_bytes())
							.ok_or(Error::Budget("expression stack"))?,
					)?;
					let value = unary(backend, *op, a.value)?;
					stack.push(backend, value)?;
				}
				Instruction::Binary(op) => {
					let b = stack.pop()?;
					let a = stack.pop()?;
					stack.admit(
						a.bytes
							.checked_add(b.bytes)
							.and_then(|bytes| bytes.checked_add(backend.working_scalar_bytes()))
							.ok_or(Error::Budget("expression stack"))?,
					)?;
					let value = binary(backend, *op, a.value, b.value)?;
					stack.push(backend, value)?;
				}
				Instruction::Discard => {
					stack.pop()?;
				}
			}
		}
		let result = stack.pop()?.value;

		backend.validate(&result)?;
		Ok(result)
	}
}
struct Stored<S> {
	value: S,
	bytes: usize,
}
struct ValueStack<S> {
	values: Vec<Stored<S>>,
	bytes: usize,
	base: usize,
	limit: usize,
}
impl<S> ValueStack<S> {
	fn new(capacity: usize, base: usize, limit: usize) -> Result<Self> {
		if base > limit {
			return Err(Error::Budget("expression stack"));
		}
		let mut values = Vec::new();
		values
			.try_reserve_exact(capacity)
			.map_err(|_| Error::Budget("expression stack"))?;
		Ok(Self {
			values,
			bytes: 0,
			base,
			limit,
		})
	}
	fn admit(&self, extra: usize) -> Result<()> {
		if self
			.base
			.checked_add(self.bytes)
			.and_then(|b| b.checked_add(extra))
			.is_none_or(|b| b > self.limit)
		{
			return Err(Error::Budget("expression stack"));
		}
		Ok(())
	}
	fn push<B: Backend<Scalar = S>>(
		&mut self,
		backend: &B,
		value: S,
	) -> std::result::Result<(), B::Error> {
		let bytes = backend.storage_bytes(&value)?;
		self.admit(bytes)?;
		self.bytes = self
			.bytes
			.checked_add(bytes)
			.ok_or(Error::Budget("expression stack"))?;
		if self.values.len() == self.values.capacity() {
			return Err(Error::Budget("expression stack").into());
		}
		self.values.push(Stored { value, bytes });
		Ok(())
	}
	fn pop(&mut self) -> Result<Stored<S>> {
		let value = self.values.pop().ok_or(Error::Domain("expression stack"))?;
		self.bytes = self
			.bytes
			.checked_sub(value.bytes)
			.ok_or(Error::Budget("expression stack"))?;
		Ok(value)
	}
}
fn unary<B: Backend>(
	b: &mut B,
	op: UnaryOperation,
	a: B::Scalar,
) -> std::result::Result<B::Scalar, B::Error> {
	match op {
		UnaryOperation::Negate => b.neg(a),
		UnaryOperation::Exp => b.exp(a),
		UnaryOperation::Ln => b.ln(a),
		UnaryOperation::Sin => b.sin(a),
		UnaryOperation::Cos => b.cos(a),
		UnaryOperation::Sqrt => b.sqrt(a),
	}
}
fn binary<B: Backend>(
	b: &mut B,
	op: BinaryOperation,
	a: B::Scalar,
	c: B::Scalar,
) -> std::result::Result<B::Scalar, B::Error> {
	match op {
		BinaryOperation::Add => b.add(a, c),
		BinaryOperation::Subtract => b.sub(a, c),
		BinaryOperation::Multiply => b.mul(a, c),
		BinaryOperation::Divide => b.div(a, c),
		BinaryOperation::Power => Err(Error::Domain("dynamic power").into()),
	}
}
struct ConstructionBackend {
	limits: ExpressionLimits,
	work: usize,
	fixed_bytes: usize,
	constant_bytes: usize,
}
impl Backend for ConstructionBackend {
	type Scalar = DynamicExpression;
	type Error = Error;
	fn visit(&mut self) -> Result<()> {
		self.work = self
			.work
			.checked_add(1)
			.ok_or(Error::Budget("expression work"))?;
		if self.work > self.limits.max_work {
			return Err(Error::Budget("expression work"));
		}
		Ok(())
	}
	fn constant(&mut self, value: &ExactConstant) -> Result<Self::Scalar> {
		let next = self
			.constant_bytes
			.checked_add(DynamicExpression::constant_storage(value)?)
			.ok_or(Error::Budget("typed dynamic capture constants"))?;
		let live = self
			.fixed_bytes
			.checked_add(next)
			.ok_or(Error::Budget("typed dynamic capture constants"))?;
		let remaining = self
			.limits
			.max_bytes
			.checked_sub(live)
			.ok_or(Error::Budget("typed dynamic capture constants"))?;
		let mut receiving = self.limits;
		receiving.polynomial.max_bytes = receiving.polynomial.max_bytes.min(remaining);
		// Validate parse/growth budget before cloning a captured heap constant.
		if !matches!(value, ExactConstant::Pi) {
			rational_constant(value, receiving.polynomial)?;
		}
		self.constant_bytes = next;
		let mut output = DynamicExpression::constant(value.clone(), receiving)?;
		output.limits = self.limits;
		Ok(output)
	}
	fn add(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar> {
		a.binary(BinaryOperation::Add, &b)
	}
	fn sub(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar> {
		a.binary(BinaryOperation::Subtract, &b)
	}
	fn mul(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar> {
		a.binary(BinaryOperation::Multiply, &b)
	}
	fn div(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar> {
		a.binary(BinaryOperation::Divide, &b)
	}
	fn neg(&mut self, a: Self::Scalar) -> Result<Self::Scalar> {
		a.unary(UnaryOperation::Negate)
	}
	fn exp(&mut self, a: Self::Scalar) -> Result<Self::Scalar> {
		a.unary(UnaryOperation::Exp)
	}
	fn ln(&mut self, a: Self::Scalar) -> Result<Self::Scalar> {
		a.unary(UnaryOperation::Ln)
	}
	fn sin(&mut self, a: Self::Scalar) -> Result<Self::Scalar> {
		a.unary(UnaryOperation::Sin)
	}
	fn cos(&mut self, a: Self::Scalar) -> Result<Self::Scalar> {
		a.unary(UnaryOperation::Cos)
	}
	fn sqrt(&mut self, a: Self::Scalar) -> Result<Self::Scalar> {
		a.unary(UnaryOperation::Sqrt)
	}
}
