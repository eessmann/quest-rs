//! Checked finite fragments enter SSA directly, without reconstructed gate syntax.
use super::{SemanticError, compile::Compiler};
use crate::{
	GateKind, SourceSpan,
	classical::{FloatWidth, ScalarType, Width},
	ssa::{self, InstructionKind as K, Type},
	syntax::Expression,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FiniteOperation {
	Gate {
		gate: GateKind,
		arguments: Vec<usize>,
		targets: Vec<usize>,
		controls: Vec<(usize, bool)>,
	},
	Oracle {
		capture: usize,
		targets: Vec<usize>,
		controls: Vec<(usize, bool)>,
	},
	Payload {
		capture: usize,
		operands: Vec<usize>,
	},
	Measure {
		qubit: usize,
		bit: usize,
	},
	Reset(usize),
	Barrier(Vec<usize>),
	Conditional {
		bit: usize,
		expected: bool,
		operation: Box<Self>,
	},
}
impl FiniteOperation {
	pub(crate) fn check_depth(&self, remaining: usize) -> Result<(), SemanticError> {
		if remaining == 0 {
			return Err(SemanticError::budget("finite fragment nesting"));
		}
		if let Self::Conditional { operation, .. } = self {
			operation.check_depth(remaining.saturating_sub(1))?;
		}
		Ok(())
	}
}
fn collect(
	op: &FiniteOperation,
	captures: &mut BTreeMap<usize, Option<ssa::ValueId>>,
	remaining: usize,
) -> Result<(), SemanticError> {
	op.check_depth(remaining)?;
	match op {
		FiniteOperation::Gate { arguments, .. } => {
			for index in arguments {
				captures.insert(*index, None);
			}
		}
		FiniteOperation::Conditional { operation, .. } => {
			collect(operation, captures, remaining.saturating_sub(1))?;
		}
		_ => {}
	}
	Ok(())
}
impl Compiler {
	pub(super) fn finite_fragment(
		&mut self,
		operations: &[FiniteOperation],
		qubits: &[Expression],
		bits: &[Expression],
		span: Option<SourceSpan>,
	) -> Result<(), SemanticError> {
		let qubits = qubits
			.iter()
			.map(|expr| {
				let binding = self.place(expr)?;
				if binding.ty != Type::Qubit(1) {
					return Err(SemanticError::invalid("finite operand requires one qubit"));
				}
				Ok(binding.place)
			})
			.collect::<Result<Vec<_>, _>>()?;
		let bit_type = Type::Scalar(ScalarType::Bit(
			Width::new(1).map_err(|_| SemanticError::invalid("bit width"))?,
		));
		let bits = bits
			.iter()
			.map(|expr| {
				let binding = self.place(expr)?;
				if binding.ty != bit_type || !binding.mutable {
					return Err(SemanticError::invalid(
						"finite operand requires mutable bit storage",
					));
				}
				Ok(binding.place)
			})
			.collect::<Result<Vec<_>, _>>()?;
		// Evaluate captures once before the fragment; every use has one dominating definition.
		let mut captures = BTreeMap::new();
		for operation in operations {
			collect(operation, &mut captures, self.limits.call_depth.min(256))?;
		}
		for (index, value) in &mut captures {
			let ty = Type::Scalar(ScalarType::Float(FloatWidth::F64));
			*value = Some(self.emit_one(
				K::Capture {
					index: *index,
					ty: ty.clone(),
				},
				ty,
				span,
			)?);
		}
		for operation in operations {
			self.finite_operation(operation, &qubits, &bits, &captures, span)?;
		}
		Ok(())
	}
	#[expect(
		clippy::too_many_lines,
		reason = "Exhaustive finite operation lowering keeps operand checks and SSA memory effects together"
	)]
	fn finite_operation(
		&mut self,
		op: &FiniteOperation,
		qubits: &[ssa::Place],
		bits: &[ssa::Place],
		captures: &BTreeMap<usize, Option<ssa::ValueId>>,
		span: Option<SourceSpan>,
	) -> Result<(), SemanticError> {
		use FiniteOperation as F;
		fn place(places: &[ssa::Place], index: usize) -> Result<ssa::Place, SemanticError> {
			places
				.get(index)
				.cloned()
				.ok_or_else(|| SemanticError::invalid("finite operand index out of bounds"))
		}
		let bit_type = Type::Scalar(ScalarType::Bit(
			Width::new(1).map_err(|_| SemanticError::invalid("bit width"))?,
		));
		match op {
			F::Gate {
				gate,
				arguments,
				targets,
				controls,
			} => {
				let arguments = arguments
					.iter()
					.map(|index| {
						captures
							.get(index)
							.copied()
							.flatten()
							.ok_or_else(|| SemanticError::invalid("finite capture missing"))
					})
					.collect::<Result<Vec<_>, _>>()?;
				let operands = controls
					.iter()
					.map(|(q, _)| *q)
					.chain(targets.iter().copied())
					.map(|q| place(qubits, q))
					.collect::<Result<Vec<_>, _>>()?;
				let modifiers = controls
					.iter()
					.map(|(_, positive)| ssa::GateModifier::Control {
						positive: *positive,
						count: 1,
					})
					.collect();
				self.effect(
					K::Gate {
						gate: *gate,
						arguments,
						operands,
						modifiers,
						memory: self.memory,
					},
					span,
				)
			}
			F::Oracle {
				capture,
				targets,
				controls,
			} => {
				if targets.is_empty() {
					return Err(SemanticError::invalid("empty finite oracle interface"));
				}
				let region =
					if let Some(region) = self
						.program
						.regions
						.iter()
						.find(|r| r.oracle.as_ref().is_some_and(|id| id.index() == *capture))
					{
						if region.parameters.len() != targets.len() {
							return Err(SemanticError::invalid(
								"incompatible finite oracle interface",
							));
						}
						region.id
					} else {
						let name = format!(
							"__quest_finite_oracle_{}_{}",
							capture,
							self.program.regions.len()
						);
						let params = (0..targets.len())
							.map(|i| (format!("q{i}"), Type::Qubit(1), true, true))
							.collect();
						self.declare_function(&name, params, Type::Void, true, &[])?;
						let region = self.program.regions.last_mut().ok_or_else(|| {
							SemanticError::invalid("missing finite oracle region")
						})?;
						region.oracle = Some(ssa::OracleId::new(*capture));
						let id = region.id;
						let entry = region.entry;
						self.functions.remove(&name);
						let block =
							self.program.blocks.get_mut(entry.index()).ok_or_else(|| {
								SemanticError::invalid("missing finite oracle entry")
							})?;
						let memory = block
							.arguments
							.first()
							.ok_or_else(|| SemanticError::invalid("missing finite oracle memory"))?
							.id;
						block.terminator = Some(ssa::Terminator::Return {
							value: None,
							memory,
						});
						block.sealed = true;
						id
					};
				let arguments = targets
					.iter()
					.map(|q| {
						Ok(ssa::CallArgument::Reference {
							place: place(qubits, *q)?,
							mutable: true,
						})
					})
					.collect::<Result<Vec<_>, SemanticError>>()?;
				let places = controls
					.iter()
					.map(|(q, _)| place(qubits, *q))
					.collect::<Result<Vec<_>, _>>()?;
				let modifiers = controls
					.iter()
					.map(|(_, positive)| ssa::GateModifier::Control {
						positive: *positive,
						count: 1,
					})
					.collect();
				self.effect(
					K::Call {
						region,
						arguments,
						controls: places,
						modifiers,
						memory: self.memory,
					},
					span,
				)
			}
			F::Payload { capture, operands } => self.effect(
				K::Payload {
					capture: *capture,
					places: operands
						.iter()
						.map(|q| place(qubits, *q))
						.collect::<Result<Vec<_>, _>>()?,
					memory: self.memory,
				},
				span,
			),
			F::Reset(q) => self.effect(
				K::Reset {
					place: place(qubits, *q)?,
					memory: self.memory,
				},
				span,
			),
			F::Barrier(operands) => self.effect(
				K::Barrier {
					places: operands
						.iter()
						.map(|q| place(qubits, *q))
						.collect::<Result<Vec<_>, _>>()?,
					memory: self.memory,
				},
				span,
			),
			F::Measure { qubit, bit } => {
				let measured = self
					.emit(
						K::Measure {
							place: place(qubits, *qubit)?,
							memory: self.memory,
						},
						vec![bit_type, Type::Memory],
						span,
					)?
					.first()
					.copied()
					.ok_or_else(|| SemanticError::invalid("missing finite measurement result"))?;
				self.effect(
					K::Store {
						place: place(bits, *bit)?,
						value: measured,
						memory: self.memory,
						initializing: false,
					},
					span,
				)
			}
			F::Conditional {
				bit,
				expected,
				operation,
			} => {
				let value = self.emit_one(
					K::Load {
						place: place(bits, *bit)?,
						memory: self.memory,
					},
					bit_type,
					span,
				)?;
				let condition = self.emit_one(
					K::Cast {
						value,
						ty: ScalarType::Bool,
					},
					Type::Scalar(ScalarType::Bool),
					span,
				)?;
				let then_block = self.new_block()?;
				let join = self.new_block()?;
				self.branch(
					condition,
					if *expected { then_block } else { join },
					if *expected { join } else { then_block },
				)?;
				self.switch_block(then_block)?;
				self.finite_operation(operation, qubits, bits, captures, span)?;
				self.jump(join)?;
				self.switch_block(join)
			}
		}
	}
}
