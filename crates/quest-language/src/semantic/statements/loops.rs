use crate::{
	classical::{ScalarType, Width},
	semantic::{
		ErrorKind, SemanticError,
		compile::{Binding, Compiler},
	},
	ssa::{self, InstructionKind as K, Type, ValueId},
	syntax::{self, BinaryOperator as B, Expression, Iterable, Statement},
};
use std::collections::BTreeMap;

impl Compiler {
	pub(in crate::semantic) fn while_statement(
		&mut self,
		condition: &Expression,
		body: &[Statement],
	) -> Result<(), SemanticError> {
		let header = self.new_block()?;
		let loop_body = self.new_block()?;
		let exit = self.new_block()?;
		self.jump(header)?;
		self.switch_block(header)?;
		let test = self.expr(condition)?;
		self.branch(test, loop_body, exit)?;
		self.switch_block(loop_body)?;
		self.loops.push((exit, header));
		self.body(body, true)?;
		self.loops.pop();
		if !self.terminated()? {
			self.jump(header)?;
		}
		self.switch_block(exit)
	}
	pub(in crate::semantic) fn for_statement(
		&mut self,
		name: &str,
		ty: &syntax::Type,
		iterable: &Iterable,
		body: &[Statement],
	) -> Result<(), SemanticError> {
		let ty = self.resolve_type(ty)?;
		if !matches!(ty, Type::Scalar(ScalarType::Int(_) | ScalarType::Uint(_))) {
			return Err(SemanticError::new(
				ErrorKind::Type,
				"for iteration type must be int or uint",
			));
		}
		self.scopes.push(BTreeMap::new());
		let result = match iterable {
			Iterable::Range { start, step, end } => {
				self.range_loop(name, &ty, start, step.as_ref(), end, body)
			}
			Iterable::Set(values) => {
				let expression = Expression {
					kind: syntax::ExpressionKind::Array(values.clone()),
					span: None,
				};
				self.collection_loop(name, &ty, &expression, body)
			}
			Iterable::Expression(value) => self.collection_loop(name, &ty, value, body),
		};
		self.scopes.pop();
		result
	}
	fn range_loop(
		&mut self,
		name: &str,
		ty: &Type,
		start: &Expression,
		step: Option<&Expression>,
		end: &Expression,
		body: &[Statement],
	) -> Result<(), SemanticError> {
		let start = self.expr(start)?;
		let start = self.coerce(start, ty, None)?;
		let end = self.expr(end)?;
		let end = self.coerce(end, ty, None)?;
		let step = self.range_step(step)?;
		let step = self.coerce(step, ty, None)?;
		self.require_nonzero_range_step(step, ty)?;
		let counter = self.slot(
			"<range-index>".into(),
			ty.clone(),
			true,
			ssa::Interface::Local,
			false,
		)?;
		self.effect(
			K::Store {
				place: counter.place.clone(),
				value: start,
				memory: self.memory,
				initializing: true,
			},
			None,
		)?;
		let iteration = self.slot(name.into(), ty.clone(), false, ssa::Interface::Local, false)?;
		self.bind(name.into(), iteration.clone())?;
		let header = self.new_block()?;
		let work = self.new_block()?;
		let advance = self.new_block()?;
		let exit = self.new_block()?;
		self.jump(header)?;
		self.switch_block(header)?;
		let current = self.emit_one(
			K::Load {
				place: counter.place.clone(),
				memory: self.memory,
			},
			ty.clone(),
			None,
		)?;
		let zero = self.integer_constant(0)?;
		let zero = self.coerce(zero, ty, None)?;
		let positive = self.binary(B::Greater, step, zero)?;
		let before = self.binary(B::LessEqual, current, end)?;
		let negative = self.binary(B::Less, step, zero)?;
		let after = self.binary(B::GreaterEqual, current, end)?;
		let forward = self.binary(B::And, positive, before)?;
		let backward = self.binary(B::And, negative, after)?;
		let condition = self.binary(B::Or, forward, backward)?;
		self.branch(condition, work, exit)?;
		self.switch_block(work)?;
		self.effect(
			K::Store {
				place: iteration.place,
				value: current,
				memory: self.memory,
				initializing: true,
			},
			None,
		)?;
		self.loops.push((exit, advance));
		self.body(body, true)?;
		self.loops.pop();
		if !self.terminated()? {
			self.jump(advance)?;
		}
		self.switch_block(advance)?;
		let current = self.emit_one(
			K::Load {
				place: counter.place.clone(),
				memory: self.memory,
			},
			ty.clone(),
			None,
		)?;
		let results = self.emit(
			K::RangeAdvance { current, step, end },
			vec![ty.clone(), Type::Scalar(ScalarType::Bool)],
			None,
		)?;
		let next = results
			.first()
			.copied()
			.ok_or_else(|| SemanticError::invalid("missing range successor"))?;
		let more = results
			.get(1)
			.copied()
			.ok_or_else(|| SemanticError::invalid("missing range continuation"))?;
		self.effect(
			K::Store {
				place: counter.place,
				value: next,
				memory: self.memory,
				initializing: false,
			},
			None,
		)?;
		self.branch(more, header, exit)?;
		self.switch_block(exit)
	}
	fn range_step(&mut self, step: Option<&Expression>) -> Result<ssa::ValueId, SemanticError> {
		if let Some(step) = step {
			self.expr(step)
		} else {
			self.integer_constant(1)
		}
	}
	fn require_nonzero_range_step(
		&mut self,
		step: ssa::ValueId,
		ty: &Type,
	) -> Result<(), SemanticError> {
		if self
			.constants
			.get(&step)
			.is_some_and(|value| value.to_i128() == Ok(0))
		{
			return Err(SemanticError::new(
				ErrorKind::Type,
				"range step cannot be zero",
			));
		}
		let zero = self.integer_constant(0)?;
		let zero = self.coerce(zero, ty, None)?;
		let nonzero = self.binary(B::NotEqual, step, zero)?;
		self.effect(
			K::Assert {
				condition: nonzero,
				message: "range step cannot be zero".into(),
				memory: self.memory,
			},
			None,
		)?;
		Ok(())
	}
	fn collection_loop(
		&mut self,
		name: &str,
		ty: &Type,
		expression: &Expression,
		body: &[Statement],
	) -> Result<(), SemanticError> {
		let collection = self.expr(expression)?;
		let collection_type = self.ty(collection)?.clone();
		let length = match &collection_type {
			Type::Array { dimensions, .. } => dimensions.first().copied(),
			Type::Scalar(ScalarType::Bit(width)) => Some(usize::from(width.value())),
			_ => None,
		}
		.ok_or_else(|| {
			SemanticError::new(
				ErrorKind::Type,
				"for collection must be a fixed array or bitstring",
			)
		})?;
		let element_type = collection_type.indexed().ok_or_else(|| {
			SemanticError::new(ErrorKind::Type, "for collection has no element type")
		})?;
		let index_type =
			Type::Scalar(ScalarType::Int(Width::new(64).map_err(|error| {
				SemanticError::new(ErrorKind::Numerical, error.to_string())
			})?));
		let counter = self.slot(
			"<collection-index>".into(),
			index_type.clone(),
			true,
			ssa::Interface::Local,
			false,
		)?;
		let zero = self.integer_constant(0)?;
		self.effect(
			K::Store {
				place: counter.place.clone(),
				value: zero,
				memory: self.memory,
				initializing: true,
			},
			None,
		)?;
		let iteration = self.slot(name.into(), ty.clone(), false, ssa::Interface::Local, false)?;
		self.bind(name.into(), iteration.clone())?;
		let header = self.new_block()?;
		let work = self.new_block()?;
		let advance = self.new_block()?;
		let exit = self.new_block()?;
		let bound = self.integer_constant(
			i128::try_from(length)
				.map_err(|_| SemanticError::budget("iteration length overflow"))?,
		)?;
		self.jump(header)?;
		self.switch_block(header)?;
		let index = self.emit_one(
			K::Load {
				place: counter.place.clone(),
				memory: self.memory,
			},
			index_type.clone(),
			None,
		)?;
		let condition = self.binary(B::Less, index, bound)?;
		self.branch(condition, work, exit)?;
		self.switch_block(work)?;
		let value = self.emit_one(
			K::Index {
				value: collection,
				index,
			},
			element_type,
			None,
		)?;
		let value = self.coerce(value, ty, None)?;
		self.effect(
			K::Store {
				place: iteration.place,
				value,
				memory: self.memory,
				initializing: true,
			},
			None,
		)?;
		self.loops.push((exit, advance));
		self.body(body, true)?;
		self.loops.pop();
		if !self.terminated()? {
			self.jump(advance)?;
		}
		self.switch_block(advance)?;
		self.increment(&counter, &index_type)?;
		self.jump(header)?;
		self.switch_block(exit)
	}
	fn increment(&mut self, counter: &Binding, ty: &Type) -> Result<(), SemanticError> {
		let current = self.emit_one(
			K::Load {
				place: counter.place.clone(),
				memory: self.memory,
			},
			ty.clone(),
			None,
		)?;
		let one = self.integer_constant(1)?;
		let next = self.binary(B::Add, current, one)?;
		self.effect(
			K::Store {
				place: counter.place.clone(),
				value: next,
				memory: self.memory,
				initializing: false,
			},
			None,
		)
	}
	fn binary(
		&mut self,
		operator: B,
		left: ValueId,
		right: ValueId,
	) -> Result<ValueId, SemanticError> {
		let ty = crate::semantic::binary_type(operator, self.ty(left)?, self.ty(right)?)?;
		self.emit_one(
			K::Binary {
				operator,
				left,
				right,
			},
			ty,
			None,
		)
	}
}
