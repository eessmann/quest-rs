use super::{
	ErrorKind, SemanticError,
	compile::{Binding, Compiler},
};
use crate::{
	classical::{FloatWidth, ScalarType, ScalarValue, Width},
	ssa::{self, InstructionKind as K, Type, ValueId},
	syntax::{self, Expression, ExpressionKind as E},
};

impl Compiler {
	pub fn expr(&mut self, expression: &Expression) -> Result<ValueId, SemanticError> {
		self.budget()?;
		if self.depth >= self.limits.call_depth.saturating_mul(4) {
			return Err(SemanticError::limit(
				crate::ResourceKind::SyntaxNesting,
				self.depth.saturating_add(1),
				self.limits.call_depth.saturating_mul(4),
				"expression nesting limit exceeded",
			));
		}
		self.depth = self
			.depth
			.checked_add(1)
			.ok_or_else(|| SemanticError::budget("expression depth overflow"))?;
		let result = self.expression_inner(expression);
		self.depth = self.depth.saturating_sub(1);
		result.map_err(|mut error| {
			if error.span.is_none() {
				error.span = expression.span;
			}
			error
		})
	}
	fn expression_inner(&mut self, expression: &Expression) -> Result<ValueId, SemanticError> {
		let span = expression.span;
		match &expression.kind {
			E::Number(_) | E::BitString(_) | E::Bool(_) => {
				let value = self.const_eval(expression)?;
				self.emit_one(K::Constant(value), Type::Scalar(value.ty()), span)
			}
			E::Name(name) => self.name_expression(name, expression),
			E::Capture(index) => self.emit_one(
				K::Capture {
					index: *index,
					ty: Type::Scalar(ScalarType::Float(FloatWidth::F64)),
				},
				Type::Scalar(ScalarType::Float(FloatWidth::F64)),
				span,
			),
			E::Unary(operator, value) => {
				let value = self.expr(value)?;
				let Type::Scalar(ty) = self.ty(value)? else {
					return Err(SemanticError::new(
						ErrorKind::Type,
						"unary operand must be scalar",
					));
				};
				let result = ty.unary_result(*operator).map_err(SemanticError::from)?;
				self.emit_one(
					K::Unary {
						operator: *operator,
						value,
					},
					Type::Scalar(result),
					span,
				)
			}
			E::Binary(operator, left, right) => {
				if matches!(
					operator,
					syntax::BinaryOperator::And | syntax::BinaryOperator::Or
				) {
					return self.short_circuit(*operator, left, right, span);
				}
				let left = self.expr(left)?;
				let right = self.expr(right)?;
				let ty = super::binary_type(*operator, self.ty(left)?, self.ty(right)?)?;
				self.emit_one(
					K::Binary {
						operator: *operator,
						left,
						right,
					},
					ty,
					span,
				)
			}
			E::Index(_, _) if self.is_place(expression) => {
				let binding = self.place(expression)?;
				self.emit_one(
					K::Load {
						place: binding.place,
						memory: self.memory,
					},
					binding.ty,
					span,
				)
			}
			E::Index(value, index) => {
				let value = self.expr(value)?;
				let index = self.expr(index)?;
				let ty = self
					.ty(value)?
					.indexed()
					.ok_or_else(|| SemanticError::new(ErrorKind::Type, "value is not indexable"))?;
				self.emit_one(K::Index { value, index }, ty, span)
			}
			E::Call(name, arguments) => self
				.call(name, arguments, Vec::new(), Vec::new(), span)?
				.ok_or_else(|| {
					SemanticError::new(ErrorKind::Type, "void subroutine used as expression")
				}),
			E::Cast(ty, value) => {
				let Type::Scalar(ty) = self.resolve_type(ty)? else {
					return Err(SemanticError::new(
						ErrorKind::Type,
						"cast target must be scalar",
					));
				};
				let value = self.expr(value)?;
				if !matches!(self.ty(value)?, Type::Scalar(source) if source.can_explicitly_cast_to(ty))
				{
					return Err(SemanticError::new(
						ErrorKind::Type,
						"explicit cast is not defined for these scalar categories or widths",
					));
				}
				self.emit_one(K::Cast { value, ty }, Type::Scalar(ty), span)
			}
			E::Measure(value) => self.measure_expression(value, span),
			E::Array(values) => self.array(values, span),
		}
	}
	fn name_expression(
		&mut self,
		name: &str,
		expression: &Expression,
	) -> Result<ValueId, SemanticError> {
		let span = expression.span;

		if matches!(name, "pi" | "π" | "tau" | "τ" | "euler" | "ℇ") {
			let value = self.const_eval(expression)?;
			return self.emit_one(K::Constant(value), Type::Scalar(value.ty()), span);
		}
		let binding = self.binding(name)?;
		if let Some(value) = binding.constant {
			self.emit_one(K::Constant(value), Type::Scalar(value.ty()), span)
		} else {
			self.emit_one(
				K::Load {
					place: binding.place,
					memory: self.memory,
				},
				binding.ty,
				span,
			)
		}
	}
	fn measure_expression(
		&mut self,
		value: &Expression,
		span: Option<crate::SourceSpan>,
	) -> Result<ValueId, SemanticError> {
		let binding = self.place(value)?;
		let Type::Qubit(size) = binding.ty else {
			return Err(SemanticError::new(
				ErrorKind::Type,
				"measurement requires quantum operand",
			));
		};
		let width = Width::new(
			u8::try_from(size)
				.map_err(|_| SemanticError::new(ErrorKind::Type, "measurement exceeds 64 bits"))?,
		)
		.map_err(SemanticError::from)?;
		self.emit(
			K::Measure {
				place: binding.place,
				memory: self.memory,
			},
			vec![Type::Scalar(ScalarType::Bit(width)), Type::Memory],
			span,
		)?
		.first()
		.copied()
		.ok_or_else(|| SemanticError::invalid("missing measurement result"))
	}
	fn array(
		&mut self,
		expressions: &[Expression],
		span: Option<crate::SourceSpan>,
	) -> Result<ValueId, SemanticError> {
		let mut values = expressions
			.iter()
			.map(|expression| self.expr(expression))
			.collect::<Result<Vec<_>, _>>()?;
		let first = values
			.first()
			.copied()
			.ok_or_else(|| SemanticError::new(ErrorKind::Type, "empty array literal"))?;
		let element = self.ty(first)?.clone();
		for value in &mut values {
			*value = self.coerce(*value, &element, span)?;
		}
		let (element, dimensions) = match element {
			Type::Scalar(element) => (element, vec![values.len()]),
			Type::Array {
				element,
				dimensions,
			} => {
				let mut shape = vec![values.len()];
				shape.extend(dimensions);
				(element, shape)
			}
			_ => {
				return Err(SemanticError::new(
					ErrorKind::Type,
					"array elements must be classical",
				));
			}
		};
		self.emit_one(
			K::Array { values },
			Type::Array {
				element,
				dimensions,
			},
			span,
		)
	}
	pub fn expr_as(
		&mut self,
		expression: &Expression,
		ty: &Type,
	) -> Result<ValueId, SemanticError> {
		if let (E::Array(elements), Type::Array { dimensions, .. }) = (&expression.kind, ty) {
			if dimensions.first().copied() != Some(elements.len()) {
				return Err(SemanticError::new(
					ErrorKind::Type,
					"array literal shape mismatch",
				));
			}
			let element_type = ty.indexed().ok_or_else(|| {
				SemanticError::new(ErrorKind::Type, "array literal requires element type")
			})?;
			let values = elements
				.iter()
				.map(|element| self.expr_as(element, &element_type))
				.collect::<Result<Vec<_>, _>>()?;
			self.emit_one(K::Array { values }, ty.clone(), expression.span)
		} else {
			let value = self.expr(expression)?;
			self.coerce(value, ty, expression.span)
		}
	}
	pub fn require_constant(&self, expression: &Expression) -> Result<(), SemanticError> {
		if let E::Array(elements) = &expression.kind {
			for element in elements {
				self.require_constant(element)?;
			}
		} else {
			self.const_eval(expression)?;
		}
		Ok(())
	}
	pub fn coerce(
		&mut self,
		value: ValueId,
		ty: &Type,
		span: Option<crate::SourceSpan>,
	) -> Result<ValueId, SemanticError> {
		if self.ty(value)? == ty {
			return Ok(value);
		}
		if let (Type::Scalar(source), Type::Scalar(target)) = (self.ty(value)?, ty) {
			if !source.can_implicitly_cast_to(*target) {
				return Err(SemanticError::new(
					ErrorKind::Type,
					"this scalar conversion requires an explicit cast",
				));
			}
			if let Some(constant) = self.constants.get(&value) {
				let converted = constant.cast(*target).map_err(SemanticError::from)?;
				return self.emit_one(K::Constant(converted), ty.clone(), span);
			}
			return self.emit_one(K::Cast { value, ty: *target }, ty.clone(), span);
		}
		Err(SemanticError::new(
			ErrorKind::Type,
			"incompatible assignment or argument type",
		))
	}
	pub fn is_place(&self, expression: &Expression) -> bool {
		match &expression.kind {
			E::Name(name) => self.binding(name).is_ok(),
			E::Index(value, _) => self.is_place(value),
			_ => false,
		}
	}
	pub fn place(&mut self, expression: &Expression) -> Result<Binding, SemanticError> {
		match &expression.kind {
			E::Name(name) => self.binding(name),
			E::Index(value, index) => {
				let mut binding = self.place(value)?;
				let id = self.expr(index)?;
				if !matches!(
					self.ty(id)?,
					Type::Scalar(ScalarType::Int(_) | ScalarType::Uint(_) | ScalarType::Bit(_))
				) {
					return Err(SemanticError::new(ErrorKind::Type, "index must be integer"));
				}
				if let Some(constant) = self.constants.get(&id) {
					let length = match &binding.ty {
						Type::Qubit(count) => Some(*count),
						Type::Array { dimensions, .. } => dimensions.first().copied(),
						Type::Scalar(ScalarType::Bit(width)) => Some(usize::from(width.value())),
						_ => None,
					};
					if let Some(length) = length {
						constant.to_index(length).map_err(SemanticError::from)?;
					}
				}
				binding.ty = binding.ty.indexed().ok_or_else(|| {
					SemanticError::new(ErrorKind::Type, "storage is not indexable")
				})?;
				binding.place.indices.push(id);
				binding.constant = None;
				Ok(binding)
			}
			_ => Err(SemanticError::new(
				ErrorKind::Type,
				"assignable reference required",
			)),
		}
	}
	pub fn call(
		&mut self,
		name: &str,
		arguments: &[Expression],
		controls: Vec<ssa::Place>,
		modifiers: Vec<ssa::GateModifier>,
		span: Option<crate::SourceSpan>,
	) -> Result<Option<ValueId>, SemanticError> {
		let Some(function) = self.functions.get(name) else {
			return self.builtin(name, arguments, span).map(Some);
		};
		let id = function.region;
		let region = self
			.program
			.regions
			.get(id.index())
			.cloned()
			.ok_or_else(|| SemanticError::invalid("missing callee region"))?;
		if arguments.len() != region.parameters.len() {
			return Err(SemanticError::new(
				ErrorKind::Type,
				"subroutine argument count mismatch",
			));
		}
		let mut actual = Vec::new();
		for (argument, parameter) in arguments.iter().zip(&region.parameters) {
			let slot = self
				.program
				.slots
				.get(parameter.index())
				.cloned()
				.ok_or_else(|| SemanticError::invalid("missing parameter slot"))?;
			if slot.reference {
				let binding = self.place(argument)?;
				let compatible = binding.ty == slot.ty
					|| (region.gate
						&& matches!((&slot.ty, &binding.ty), (Type::Qubit(1), Type::Qubit(count)) if *count > 0));
				if !compatible
					|| (slot.mutable && !binding.mutable && !matches!(binding.ty, Type::Qubit(_)))
				{
					return Err(SemanticError::new(
						ErrorKind::Alias,
						"reference type or mutability mismatch",
					));
				}
				actual.push(ssa::CallArgument::Reference {
					place: binding.place,
					mutable: slot.mutable,
				});
			} else {
				let value = if region.gate {
					self.gate_parameter(argument)?
				} else {
					self.expr_as(argument, &slot.ty)?
				};
				actual.push(ssa::CallArgument::Value(value));
			}
		}
		let mut results = Vec::new();
		if region.result != Type::Void {
			results.push(region.result);
		}
		results.push(Type::Memory);
		let values = self.emit(
			K::Call {
				region: id,
				arguments: actual,
				controls,
				modifiers,
				memory: self.memory,
			},
			results,
			span,
		)?;
		Ok(if values.len() > 1 {
			values.first().copied()
		} else {
			None
		})
	}
	fn gate_parameter(&mut self, argument: &Expression) -> Result<ValueId, SemanticError> {
		let value = self.expr(argument)?;
		if !matches!(
			self.ty(value)?,
			Type::Scalar(
				ScalarType::Bool
					| ScalarType::Int(_)
					| ScalarType::Uint(_)
					| ScalarType::Angle(_)
					| ScalarType::Float(_)
			)
		) {
			return Err(SemanticError::new(
				ErrorKind::Type,
				"gate parameters require real scalar values",
			));
		}
		let ty = ScalarType::Float(FloatWidth::F64);
		if self.ty(value)? == &Type::Scalar(ty) {
			Ok(value)
		} else {
			self.emit_one(K::GateParameter { value }, Type::Scalar(ty), argument.span)
		}
	}
	fn builtin(
		&mut self,
		name: &str,
		arguments: &[Expression],
		span: Option<crate::SourceSpan>,
	) -> Result<ValueId, SemanticError> {
		let values = arguments
			.iter()
			.map(|argument| self.expr(argument))
			.collect::<Result<Vec<_>, _>>()?;
		let types = values
			.iter()
			.map(|id| {
				if let Type::Scalar(ty) = self.ty(*id)? {
					Ok(*ty)
				} else {
					Err(SemanticError::new(
						ErrorKind::Type,
						"builtin requires scalar arguments",
					))
				}
			})
			.collect::<Result<Vec<_>, _>>()?;
		let ty = ScalarType::function_result(name, &types).map_err(SemanticError::from)?;
		self.emit_one(
			K::Builtin {
				name: name.into(),
				arguments: values,
			},
			Type::Scalar(ty),
			span,
		)
	}
	fn short_circuit(
		&mut self,
		operator: syntax::BinaryOperator,
		left: &Expression,
		right: &Expression,
		span: Option<crate::SourceSpan>,
	) -> Result<ValueId, SemanticError> {
		let left = self.expr(left)?;
		if self.ty(left)? != &Type::Scalar(ScalarType::Bool) {
			return Err(SemanticError::new(
				ErrorKind::Type,
				"logical operand must be bool",
			));
		}
		let binding = self.slot(
			"<short-circuit>".into(),
			Type::Scalar(ScalarType::Bool),
			true,
			ssa::Interface::Local,
			false,
		)?;
		self.effect(
			K::Store {
				place: binding.place.clone(),
				value: left,
				memory: self.memory,
				initializing: true,
			},
			span,
		)?;
		let rhs = self.new_block()?;
		let merge = self.new_block()?;
		if operator == syntax::BinaryOperator::And {
			self.branch(left, rhs, merge)?;
		} else {
			self.branch(left, merge, rhs)?;
		}
		self.switch_block(rhs)?;
		let value = self.expr(right)?;
		if self.ty(value)? != &binding.ty {
			return Err(SemanticError::new(
				ErrorKind::Type,
				"logical operand must be bool",
			));
		}
		self.effect(
			K::Store {
				place: binding.place.clone(),
				value,
				memory: self.memory,
				initializing: false,
			},
			span,
		)?;
		self.jump(merge)?;
		self.switch_block(merge)?;
		self.emit_one(
			K::Load {
				place: binding.place,
				memory: self.memory,
			},
			binding.ty,
			span,
		)
	}
	// Check skipped operands without executing their domain-sensitive arithmetic.
	fn constant_type(&self, expression: &Expression) -> Result<ScalarType, SemanticError> {
		match &expression.kind {
			E::Number(text) => Ok(ScalarValue::parse_number(text)
				.map_err(SemanticError::from)?
				.ty()),
			E::BitString(text) => Ok(ScalarValue::bitstring(text)
				.map_err(SemanticError::from)?
				.ty()),
			E::Bool(_) => Ok(ScalarType::Bool),
			E::Name(_) => Ok(self.const_eval(expression)?.ty()),
			E::Unary(operator, value) => self
				.constant_type(value)?
				.unary_result(*operator)
				.map_err(SemanticError::from),
			E::Binary(operator, left, right) => self
				.constant_type(left)?
				.binary_result(*operator, self.constant_type(right)?)
				.map_err(SemanticError::from),
			E::Call(name, arguments) => {
				let types = arguments
					.iter()
					.map(|arg| self.constant_type(arg))
					.collect::<Result<Vec<_>, _>>()?;
				ScalarType::function_result(name, &types).map_err(SemanticError::from)
			}
			E::Cast(ty, value) => {
				let Type::Scalar(target) = self.resolve_type(ty)? else {
					return Err(SemanticError::new(
						ErrorKind::Type,
						"scalar constant cast required",
					));
				};
				if !self.constant_type(value)?.can_explicitly_cast_to(target) {
					return Err(SemanticError::new(ErrorKind::Type, "invalid scalar cast"));
				}
				Ok(target)
			}
			_ => Err(SemanticError::new(
				ErrorKind::Type,
				"compile-time scalar constant required",
			)),
		}
	}
	pub fn const_eval(&self, expression: &Expression) -> Result<ScalarValue, SemanticError> {
		match &expression.kind {
			E::Number(text) => ScalarValue::parse_number(text).map_err(SemanticError::from),
			E::BitString(text) => ScalarValue::bitstring(text).map_err(SemanticError::from),
			E::Bool(value) => Ok(ScalarValue::boolean(*value)),
			E::Name(name) => {
				let special = match name.as_str() {
					"pi" | "π" => Some(std::f64::consts::PI),
					"tau" | "τ" => Some(std::f64::consts::TAU),
					"euler" | "ℇ" => Some(std::f64::consts::E),
					_ => None,
				};
				special.map_or_else(
					|| {
						match self.binding(name) {
							Ok(binding) => binding.constant,
							Err(_) => self.type_constants.get(name).copied(),
						}
						.ok_or_else(|| {
							SemanticError::new(ErrorKind::Type, "compile-time constant required")
						})
					},
					|value| {
						ScalarValue::floating(FloatWidth::F64, value).map_err(SemanticError::from)
					},
				)
			}
			E::Unary(operator, value) => self
				.const_eval(value)?
				.unary(*operator)
				.map_err(SemanticError::from),
			E::Binary(operator, left, right) => {
				let left = self.const_eval(left)?;
				if matches!(
					operator,
					syntax::BinaryOperator::And | syntax::BinaryOperator::Or
				) {
					left.ty()
						.binary_result(*operator, self.constant_type(right)?)
						.map_err(SemanticError::from)?;
					let operation = if *operator == syntax::BinaryOperator::And {
						mathcore::scalar::BooleanOperation::And
					} else {
						mathcore::scalar::BooleanOperation::Or
					};
					let value = mathcore::scalar::boolean(
						left.to_bool().map_err(SemanticError::from)?,
						operation,
						|| {
							self.const_eval(right)?
								.to_bool()
								.map_err(SemanticError::from)
						},
					)?;
					return Ok(ScalarValue::boolean(value));
				}
				left.binary(*operator, &self.const_eval(right)?)
					.map_err(SemanticError::from)
			}
			E::Call(name, arguments) => {
				let values = arguments
					.iter()
					.map(|argument| self.const_eval(argument))
					.collect::<Result<Vec<_>, _>>()?;
				ScalarValue::function(name, &values).map_err(SemanticError::from)
			}
			E::Cast(ty, value) => {
				let Type::Scalar(ty) = self.resolve_type(ty)? else {
					return Err(SemanticError::new(
						ErrorKind::Type,
						"scalar constant cast required",
					));
				};
				self.const_eval(value)?
					.cast(ty)
					.map_err(SemanticError::from)
			}
			_ => Err(SemanticError::new(
				ErrorKind::Type,
				"compile-time scalar constant required",
			)),
		}
	}
	pub fn positive_size(&self, expression: &Expression) -> Result<usize, SemanticError> {
		let value = self
			.const_eval(expression)?
			.to_i128()
			.map_err(SemanticError::from)?;
		let size = usize::try_from(value)
			.map_err(|_| SemanticError::new(ErrorKind::Type, "positive size required"))?;
		if size == 0 {
			Err(SemanticError::new(
				ErrorKind::Type,
				"positive size required",
			))
		} else {
			Ok(size)
		}
	}
	pub fn resolve_type(&self, ty: &syntax::Type) -> Result<Type, SemanticError> {
		match ty {
			syntax::Type::Scalar(kind, width) => {
				let size = width.as_ref().map_or_else(
					|| {
						Ok(if *kind == syntax::ScalarKind::Bit {
							1
						} else {
							64
						})
					},
					|expression| self.positive_size(expression),
				)?;
				if *kind == syntax::ScalarKind::Bool {
					if width.is_some() {
						return Err(SemanticError::new(ErrorKind::Type, "bool has no width"));
					}
					return Ok(Type::Scalar(ScalarType::Bool));
				}
				if *kind == syntax::ScalarKind::Float {
					return Ok(Type::Scalar(ScalarType::Float(match size {
						32 => FloatWidth::F32,
						64 => FloatWidth::F64,
						_ => {
							return Err(SemanticError::new(
								ErrorKind::Type,
								"float width must be 32 or 64",
							));
						}
					})));
				}
				let width =
					Width::new(u8::try_from(size).map_err(|_| {
						SemanticError::new(ErrorKind::Type, "scalar width exceeds 64")
					})?)
					.map_err(SemanticError::from)?;
				let scalar = match kind {
					syntax::ScalarKind::Bit => ScalarType::Bit(width),
					syntax::ScalarKind::Int => ScalarType::Int(width),
					syntax::ScalarKind::Uint => ScalarType::Uint(width),
					syntax::ScalarKind::Angle => ScalarType::Angle(width),
					_ => return Err(SemanticError::invalid("invalid scalar kind")),
				};
				Ok(Type::Scalar(scalar))
			}
			syntax::Type::Qubit(size) => {
				Ok(Type::Qubit(size.as_ref().map_or(Ok(1), |expression| {
					self.positive_size(expression)
				})?))
			}
			syntax::Type::Array {
				element,
				dimensions,
				..
			} => {
				let Type::Scalar(element) = self.resolve_type(element)? else {
					return Err(SemanticError::new(
						ErrorKind::Type,
						"array element type must be scalar",
					));
				};
				let dimensions = dimensions
					.iter()
					.map(|dimension| self.positive_size(dimension))
					.collect::<Result<Vec<_>, _>>()?;
				if dimensions.is_empty() {
					return Err(SemanticError::new(
						ErrorKind::Type,
						"array requires dimensions",
					));
				}
				Ok(Type::Array {
					element,
					dimensions,
				})
			}
		}
	}
}
impl From<crate::classical::ValueError> for SemanticError {
	fn from(error: crate::classical::ValueError) -> Self {
		Self::new(ErrorKind::Numerical, error.to_string())
	}
}
