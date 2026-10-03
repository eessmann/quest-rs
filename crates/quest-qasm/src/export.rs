use crate::{Result, failure};
use quest_language::{
	DiagnosticCause, ResourceKind, ResourceUsage, Stage,
	syntax::{
		BinaryOperator, Expression, ExpressionKind, Iterable, Modifier, Module, Parameter,
		Qualifier, ScalarKind, Statement, StatementKind, Type, UnaryOperator,
	},
};

/// Bounds canonical output allocation and recursive AST traversal.
#[derive(Debug, Clone, Copy)]
pub struct ExportLimits {
	pub bytes: usize,
	pub nesting: usize,
}
impl Default for ExportLimits {
	fn default() -> Self {
		Self {
			bytes: 4_194_304,
			nesting: 256,
		}
	}
}
/// Canonical serialization of structured syntax, without flattening definitions.
///
/// This does not grant semantic admission. Use `import` for checked executable
/// programs. Captures and AST forms without an equivalent textual representation
/// are rejected rather than silently changing their meaning.
///
/// # Errors
/// Rejects unsupported representations, malformed leaves, or exhausted limits.
pub fn export_syntax(module: &Module, limits: ExportLimits) -> Result<String> {
	let mut writer = Writer {
		text: String::new(),
		limits,
		depth: 0,
		indent: 0,
	};
	writer.push("OPENQASM 3.1;\n")?;
	writer.statements(&module.statements)?;
	Ok(writer.text)
}
fn unsupported(message: &str) -> Box<quest_language::Diagnostic> {
	failure(
		Stage::Export,
		DiagnosticCause::UnsupportedCapability {
			capability: message.into(),
		},
		message,
	)
}
fn overflow(resource: ResourceKind) -> Box<quest_language::Diagnostic> {
	failure(
		Stage::Export,
		DiagnosticCause::ResourceOverflow(resource),
		"canonical export resource arithmetic overflow",
	)
}
fn budget(
	resource: ResourceKind,
	requested: usize,
	limit: usize,
) -> Box<quest_language::Diagnostic> {
	match (u64::try_from(requested), u64::try_from(limit)) {
		(Ok(requested), Ok(limit)) => failure(
			Stage::Export,
			DiagnosticCause::ResourceLimit(ResourceUsage {
				resource,
				requested,
				limit,
			}),
			"canonical export resource limit exceeded",
		),
		_ => overflow(resource),
	}
}
struct Writer {
	text: String,
	limits: ExportLimits,
	depth: usize,
	indent: usize,
}
impl Writer {
	fn push(&mut self, text: &str) -> Result<()> {
		let size = self
			.text
			.len()
			.checked_add(text.len())
			.ok_or_else(|| overflow(ResourceKind::ExportBytes))?;
		if size > self.limits.bytes {
			return Err(budget(ResourceKind::ExportBytes, size, self.limits.bytes));
		}
		self.text.try_reserve(text.len()).map_err(|_| {
			failure(
				Stage::Export,
				DiagnosticCause::ResourceFailure {
					reason: "canonical export allocation failed".into(),
				},
				"canonical export allocation failed",
			)
		})?;
		self.text.push_str(text);
		Ok(())
	}
	fn enter(&mut self) -> Result<()> {
		let next = self
			.depth
			.checked_add(1)
			.ok_or_else(|| overflow(ResourceKind::ExportNesting))?;
		if next > self.limits.nesting {
			return Err(budget(
				ResourceKind::ExportNesting,
				next,
				self.limits.nesting,
			));
		}
		self.depth = next;
		Ok(())
	}
	fn name(&mut self, name: &str) -> Result<()> {
		let mut chars = name.chars();
		if matches!(
			name,
			"true"
				| "false"
				| "measure"
				| "bool"
				| "bit"
				| "int"
				| "uint"
				| "angle"
				| "float"
				| "OPENQASM"
				| "include"
				| "let"
				| "gate"
				| "def"
				| "qubit"
				| "input"
				| "output"
				| "const"
				| "array"
				| "if"
				| "else"
				| "while"
				| "for"
				| "in"
				| "switch"
				| "case"
				| "default"
				| "break"
				| "continue"
				| "end"
				| "return"
				| "reset"
				| "barrier"
				| "inv"
				| "ctrl"
				| "negctrl"
				| "pow"
				| "mutable"
				| "readonly"
				| "defcal"
				| "cal"
				| "defcalgrammar"
				| "delay"
				| "box"
				| "extern"
				| "duration"
				| "stretch"
				| "complex"
		) || !chars
			.next()
			.is_some_and(|ch| ch.is_alphabetic() || ch == '_')
			|| !chars.all(|ch| ch.is_alphanumeric() || ch == '_')
		{
			return Err(unsupported("identifier has no textual representation"));
		}
		self.push(name)
	}
	fn finite_operation(
		&mut self,
		operation: &quest_language::semantic::finite::FiniteOperation,
		qubits: &[Expression],
		bits: &[Expression],
	) -> Result<()> {
		use quest_language::semantic::finite::FiniteOperation as F;
		let qubit = |index: usize| {
			qubits
				.get(index)
				.ok_or_else(|| unsupported("finite qubit index"))
		};
		let bit = |index: usize| {
			bits.get(index)
				.ok_or_else(|| unsupported("finite bit index"))
		};
		self.enter()?;
		match operation {
			F::Gate {
				gate,
				arguments,
				targets,
				controls,
			} => {
				if !arguments.is_empty() {
					return Err(unsupported(
						"captured finite angle requires explicit portable binding",
					));
				}
				for (_, positive) in controls {
					self.push(if *positive { "ctrl @ " } else { "negctrl @ " })?;
				}
				self.push(gate.definition().name)?;
				self.push("() ")?;
				for (i, target) in controls.iter().map(|(q, _)| q).chain(targets).enumerate() {
					if i != 0 {
						self.push(", ")?;
					}
					self.expression(qubit(*target)?)?;
				}
				self.push(";")?;
			}
			F::Measure { qubit: q, bit: b } => {
				self.expression(bit(*b)?)?;
				self.push(" = measure ")?;
				self.expression(qubit(*q)?)?;
				self.push(";")?;
			}
			F::Reset(q) => {
				self.push("reset ")?;
				self.expression(qubit(*q)?)?;
				self.push(";")?;
			}
			F::Barrier(targets) => {
				self.push("barrier ")?;
				for (i, q) in targets.iter().enumerate() {
					if i != 0 {
						self.push(", ")?;
					}
					self.expression(qubit(*q)?)?;
				}
				self.push(";")?;
			}
			F::Conditional {
				bit: b,
				expected,
				operation,
			} => {
				self.push("if (")?;
				self.expression(bit(*b)?)?;
				self.push(if *expected { " == 1) { " } else { " == 0) { " })?;
				self.finite_operation(operation, qubits, bits)?;
				self.push(" }")?;
			}
			F::Oracle { .. } | F::Payload { .. } => {
				return Err(unsupported(
					"finite oracle or payload requires an explicit portable decomposition",
				));
			}
		}
		self.depth = self.depth.saturating_sub(1);
		Ok(())
	}
	fn quoted(&mut self, text: &str) -> Result<()> {
		if text.contains(['"', '\\', '\n', '\r']) {
			return Err(unsupported("string requires unsupported escapes"));
		}
		self.push("\"")?;
		self.push(text)?;
		self.push("\"")
	}
	fn statements(&mut self, statements: &[Statement]) -> Result<()> {
		for statement in statements {
			for _ in 0..self.indent {
				self.push("    ")?;
			}
			self.enter()?;
			self.statement(&statement.kind)?;
			self.depth = self.depth.saturating_sub(1);
			self.push("\n")?;
		}
		Ok(())
	}
	fn block(&mut self, body: &[Statement]) -> Result<()> {
		self.push("{\n")?;
		self.indent = self
			.indent
			.checked_add(1)
			.ok_or_else(|| overflow(ResourceKind::ExportNesting))?;
		self.statements(body)?;
		self.indent = self.indent.saturating_sub(1);
		for _ in 0..self.indent {
			self.push("    ")?;
		}
		self.push("}")
	}
	#[expect(
		clippy::too_many_lines,
		reason = "Exhaustive syntax dispatch makes unsupported representations visible"
	)]
	fn statement(&mut self, statement: &StatementKind) -> Result<()> {
		match statement {
			StatementKind::Finite {
				operations,
				qubits,
				bits,
			} => {
				for operation in operations {
					self.finite_operation(operation, qubits, bits)?;
					self.push("\n")?;
				}
				Ok(())
			}
			StatementKind::Oracle { .. } | StatementKind::Payload { .. } => Err(unsupported(
				"oracle capture requires an explicit portable decomposition",
			)),
			StatementKind::Include(path) => {
				self.push("include ")?;
				self.quoted(path)?;
				self.push(";")
			}
			StatementKind::Alias { name, value } => {
				self.push("let ")?;
				self.name(name)?;
				self.push(" = ")?;
				self.expression(value)?;
				self.push(";")
			}
			StatementKind::Qubit { name, size } => {
				self.push("qubit")?;
				self.width(size.as_ref())?;
				self.push(" ")?;
				self.name(name)?;
				self.push(";")
			}
			StatementKind::Declare {
				name,
				ty,
				initializer,
				qualifier,
			} => {
				self.push(match qualifier {
					Qualifier::Local => "",
					Qualifier::Const => "const ",
					Qualifier::Input => "input ",
					Qualifier::Output => "output ",
				})?;
				if matches!(ty, Type::Qubit(_)) {
					return Err(unsupported(
						"qubit declarations require the dedicated qubit statement",
					));
				}
				self.ty(ty, false)?;
				self.push(" ")?;
				self.name(name)?;
				if let Some(initializer) = initializer {
					self.push(" = ")?;
					self.expression(initializer)?;
				}
				self.push(";")
			}
			StatementKind::Assign {
				target,
				operator,
				value,
			} => {
				self.place(target)?;
				self.push(" ")?;
				if let Some(operator) = operator {
					if !matches!(
						operator,
						BinaryOperator::Add
							| BinaryOperator::Subtract
							| BinaryOperator::Multiply
							| BinaryOperator::Divide
							| BinaryOperator::Remainder
							| BinaryOperator::BitAnd
							| BinaryOperator::BitOr
							| BinaryOperator::BitXor
							| BinaryOperator::ShiftLeft
							| BinaryOperator::ShiftRight
					) {
						return Err(unsupported(
							"compound assignment operator has no representation",
						));
					}
					self.push(binary(*operator))?;
				}
				self.push("= ")?;
				self.expression(value)?;
				self.push(";")
			}
			StatementKind::Gate {
				name,
				arguments,
				operands,
				modifiers,
			} => {
				for modifier in modifiers {
					self.modifier(modifier)?;
					self.push(" @ ")?;
				}
				self.name(name)?;
				// Explicit empty parameter lists disambiguate operands beginning with `(`.
				self.push("(")?;
				self.expressions(arguments)?;
				self.push(")")?;
				if !operands.is_empty() {
					self.push(" ")?;
					self.expressions(operands)?;
				}
				if operands.is_empty() && name != "gphase" && modifiers.is_empty() {
					return Err(unsupported(
						"zero-operand user gates are not represented by this grammar",
					));
				}
				self.push(";")
			}
			StatementKind::GateDeclaration {
				name,
				parameters,
				qubits,
				body,
			} => {
				self.push("gate ")?;
				self.name(name)?;
				if !parameters.is_empty() {
					self.push("(")?;
					self.names(parameters)?;
					self.push(")")?;
				}
				self.push(" ")?;
				self.names(qubits)?;
				self.push(" ")?;
				self.block(body)
			}
			StatementKind::Subroutine {
				name,
				parameters,
				result,
				body,
			} => {
				self.push("def ")?;
				self.name(name)?;
				self.push("(")?;
				for (index, parameter) in parameters.iter().enumerate() {
					if index != 0 {
						self.push(", ")?;
					}
					self.parameter(parameter)?;
				}
				self.push(")")?;
				if let Some(result) = result {
					self.push(" -> ")?;
					self.ty(result, false)?;
				}
				self.push(" ")?;
				self.block(body)
			}
			StatementKind::If {
				condition,
				then_body,
				else_body,
			} => {
				self.push("if (")?;
				self.expression(condition)?;
				self.push(") ")?;
				self.block(then_body)?;
				if !else_body.is_empty() {
					self.push(" else ")?;
					self.block(else_body)?;
				}
				Ok(())
			}
			StatementKind::Switch {
				selector,
				cases,
				default,
			} => {
				self.push("switch (")?;
				self.expression(selector)?;
				self.push(") {\n")?;
				for (labels, body) in cases {
					if labels.is_empty() {
						return Err(unsupported("switch case requires at least one label"));
					}
					for _ in 0..self.indent {
						self.push("    ")?;
					}
					self.push("case ")?;
					self.expressions(labels)?;
					self.push(" ")?;
					self.block(body)?;
					self.push("\n")?;
				}
				if !default.is_empty() {
					self.push("default ")?;
					self.block(default)?;
					self.push("\n")?;
				}
				for _ in 0..self.indent {
					self.push("    ")?;
				}
				self.push("}")
			}
			StatementKind::For {
				name,
				ty,
				iterable,
				body,
			} => {
				self.push("for ")?;
				self.ty(ty, false)?;
				self.push(" ")?;
				self.name(name)?;
				self.push(" in ")?;
				self.iterable(iterable)?;
				self.push(" ")?;
				self.block(body)
			}
			StatementKind::While { condition, body } => {
				self.push("while (")?;
				self.expression(condition)?;
				self.push(") ")?;
				self.block(body)
			}
			StatementKind::Reset(target) => {
				self.push("reset ")?;
				self.expression(target)?;
				self.push(";")
			}
			StatementKind::Barrier(operands) => {
				self.push("barrier ")?;
				self.expressions(operands)?;
				self.push(";")
			}
			StatementKind::Expression(value) => {
				if !matches!(
					value.kind,
					ExpressionKind::Call(..) | ExpressionKind::Measure(_)
				) {
					return Err(unsupported(
						"standalone expression has no statement representation",
					));
				}
				self.expression(value)?;
				self.push(";")
			}
			StatementKind::Return(value) => {
				self.push("return")?;
				if let Some(value) = value {
					self.push(" ")?;
					self.expression(value)?;
				}
				self.push(";")
			}
			StatementKind::Break => self.push("break;"),
			StatementKind::Continue => self.push("continue;"),
			StatementKind::End => self.push("end;"),
		}
	}
	fn names(&mut self, names: &[String]) -> Result<()> {
		for (index, name) in names.iter().enumerate() {
			if index != 0 {
				self.push(", ")?;
			}
			self.name(name)?;
		}
		Ok(())
	}
	fn expressions(&mut self, values: &[Expression]) -> Result<()> {
		for (index, value) in values.iter().enumerate() {
			if index != 0 {
				self.push(", ")?;
			}
			self.expression(value)?;
		}
		Ok(())
	}
	fn width(&mut self, width: Option<&Expression>) -> Result<()> {
		if let Some(width) = width {
			self.push("[")?;
			self.expression(width)?;
			self.push("]")?;
		}
		Ok(())
	}
	fn ty(&mut self, ty: &Type, parameter: bool) -> Result<()> {
		self.enter()?;
		match ty {
			Type::Scalar(kind, width) => {
				self.push(match kind {
					ScalarKind::Bool => "bool",
					ScalarKind::Bit => "bit",
					ScalarKind::Int => "int",
					ScalarKind::Uint => "uint",
					ScalarKind::Angle => "angle",
					ScalarKind::Float => "float",
				})?;
				self.width(width.as_deref())?;
			}
			Type::Qubit(width) => {
				self.push("qubit")?;
				self.width(width.as_deref())?;
			}
			Type::Array {
				element,
				dimensions,
				reference,
			} => {
				if *reference != parameter || dimensions.is_empty() {
					return Err(unsupported(
						"array reference or dimensions have no representation in this context",
					));
				}
				self.push("array[")?;
				self.ty(element, false)?;
				self.push(", ")?;
				self.expressions(dimensions)?;
				self.push("]")?;
			}
		}
		self.depth = self.depth.saturating_sub(1);
		Ok(())
	}
	fn parameter(&mut self, parameter: &Parameter) -> Result<()> {
		if parameter.mutable {
			self.push("mutable ")?;
		} else if matches!(parameter.ty, Type::Array { .. }) {
			self.push("readonly ")?;
		}
		self.ty(&parameter.ty, true)?;
		self.push(" ")?;
		self.name(&parameter.name)
	}
	fn modifier(&mut self, modifier: &Modifier) -> Result<()> {
		match modifier {
			Modifier::Inverse | Modifier::Adjoint => self.push("inv"),
			Modifier::Control { positive, count } => {
				self.push(if *positive { "ctrl" } else { "negctrl" })?;
				if let Some(count) = count {
					self.push("(")?;
					self.expression(count)?;
					self.push(")")?;
				}
				Ok(())
			}
			Modifier::Power(value) => {
				self.push("pow(")?;
				self.expression(value)?;
				self.push(")")
			}
		}
	}
	fn iterable(&mut self, iterable: &Iterable) -> Result<()> {
		match iterable {
			Iterable::Range { start, step, end } => {
				self.push("[")?;
				self.expression(start)?;
				self.push(":")?;
				if let Some(step) = step {
					self.expression(step)?;
					self.push(":")?;
				}
				self.expression(end)?;
				self.push("]")
			}
			Iterable::Set(values) => {
				self.push("{")?;
				self.expressions(values)?;
				self.push("}")
			}
			Iterable::Expression(value) => self.expression(value),
		}
	}
	fn place(&mut self, target: &Expression) -> Result<()> {
		self.enter()?;
		match &target.kind {
			ExpressionKind::Name(name) => self.name(name)?,
			ExpressionKind::Index(base, index) => {
				self.place(base)?;
				self.push("[")?;
				self.expression(index)?;
				self.push("]")?;
			}
			_ => {
				return Err(unsupported(
					"assignment target is not a textual storage place",
				));
			}
		}
		self.depth = self.depth.saturating_sub(1);
		Ok(())
	}
	fn number(&mut self, number: &str) -> Result<()> {
		let source = quest_language::SourceSnapshot::new(
			quest_language::SourceId::new(0),
			"literal",
			number,
		);
		let tokens = quest_language::syntax::lex(
			&source,
			quest_language::syntax::ParseLimits {
				source_bytes: self.limits.bytes,
				tokens: 1,
				nesting: self.limits.nesting,
			},
		)
		.map_err(|_| unsupported("number has no textual literal representation"))?;
		if tokens.len() != 1
			|| !tokens.first().is_some_and(|token| {
				token.kind == quest_language::syntax::TokenKind::Number(number.into())
			}) {
			return Err(unsupported("number has no textual literal representation"));
		}
		self.push(number)?;
		Ok(())
	}
	fn expression(&mut self, expression: &Expression) -> Result<()> {
		self.enter()?;
		match &expression.kind {
			ExpressionKind::Number(number) => self.number(number)?,
			ExpressionKind::BitString(bits) => self.quoted(bits)?,
			ExpressionKind::Bool(value) => self.push(if *value { "true" } else { "false" })?,
			ExpressionKind::Name(name) => self.name(name)?,
			ExpressionKind::Capture(_) => {
				return Err(unsupported(
					"unresolved Rust capture cannot be exported as OpenQASM text",
				));
			}
			ExpressionKind::Unary(operator, value) => {
				self.push("(")?;
				self.push(match operator {
					UnaryOperator::Negate => "-",
					UnaryOperator::Positive => "+",
					UnaryOperator::Not => "!",
					UnaryOperator::Complement => "~",
				})?;
				self.push("(")?;
				self.expression(value)?;
				self.push("))")?;
			}
			ExpressionKind::Binary(operator, left, right) => {
				self.push("(")?;
				self.expression(left)?;
				self.push(" ")?;
				self.push(binary(*operator))?;
				self.push(" ")?;
				self.expression(right)?;
				self.push(")")?;
			}
			ExpressionKind::Index(base, index) => {
				let parentheses = matches!(base.kind, ExpressionKind::Measure(_));
				if parentheses {
					self.push("(")?;
				}
				self.expression(base)?;
				if parentheses {
					self.push(")")?;
				}
				self.push("[")?;
				self.expression(index)?;
				self.push("]")?;
			}
			ExpressionKind::Call(name, arguments) => {
				if name == "gphase" {
					return Err(unsupported(
						"gphase is a gate rather than a callable expression",
					));
				}
				self.name(name)?;
				self.push("(")?;
				self.expressions(arguments)?;
				self.push(")")?;
			}
			ExpressionKind::Cast(ty, value) => {
				if !matches!(ty, Type::Scalar(..)) {
					return Err(unsupported("only scalar casts have a text representation"));
				}
				self.ty(ty, false)?;
				self.push("(")?;
				self.expression(value)?;
				self.push(")")?;
			}
			ExpressionKind::Measure(target) => {
				self.push("measure (")?;
				self.expression(target)?;
				self.push(")")?;
			}
			ExpressionKind::Array(values) => {
				self.push("{")?;
				self.expressions(values)?;
				self.push("}")?;
			}
		}
		self.depth = self.depth.saturating_sub(1);
		Ok(())
	}
}
const fn binary(operator: BinaryOperator) -> &'static str {
	match operator {
		BinaryOperator::Add => "+",
		BinaryOperator::Subtract => "-",
		BinaryOperator::Multiply => "*",
		BinaryOperator::Divide => "/",
		BinaryOperator::Remainder => "%",
		BinaryOperator::Power => "**",
		BinaryOperator::ShiftLeft => "<<",
		BinaryOperator::ShiftRight => ">>",
		BinaryOperator::BitAnd => "&",
		BinaryOperator::BitOr => "|",
		BinaryOperator::BitXor => "^",
		BinaryOperator::And => "&&",
		BinaryOperator::Or => "||",
		BinaryOperator::Equal => "==",
		BinaryOperator::NotEqual => "!=",
		BinaryOperator::Less => "<",
		BinaryOperator::LessEqual => "<=",
		BinaryOperator::Greater => ">",
		BinaryOperator::GreaterEqual => ">=",
	}
}
