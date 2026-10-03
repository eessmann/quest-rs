use crate::MatrixPolicy;
use crate::{Control, Error, Operation, OracleFragment, QubitId, Result};
use std::sync::Arc;
/// Compiler extension over shared semantic capabilities.
pub trait OracleExport: Sized {
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	fn export_qasm(&self, limits: crate::qasm::ExportLimits) -> crate::qasm::Result<String>;
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	fn portable_operations(
		&self,
		targets: &[QubitId],
		controls: &[Control],
		budget: usize,
		output: &mut Vec<Operation>,
	) -> Result<()>;
}
impl OracleExport for OracleFragment {
	/// Export an explicit portable built-in decomposition. Numerical matrices
	/// require a caller-supplied decomposition and are never silently omitted.
	/// # Errors
	/// Rejects unsupported matrix payloads, remapping and export budgets.
	fn export_qasm(&self, limits: crate::qasm::ExportLimits) -> crate::qasm::Result<String> {
		use crate::language::syntax::{
			Expression, ExpressionKind, Module, Statement, StatementKind,
		};
		let failure = |message: &str| {
			Box::new(crate::language::Diagnostic::new(
				crate::language::Stage::Export,
				crate::language::DiagnosticCause::UnsupportedCapability {
					capability: message.into(),
				},
				message,
			))
		};
		let builder = crate::QuantumRegionBuilder::new(self.num_qubits(), 0)
			.map_err(|error| failure(&error.to_string()))?;
		let targets = (0..self.num_qubits())
			.map(|index| builder.qubit(index))
			.collect::<Result<Vec<_>>>()
			.map_err(|error| failure(&error.to_string()))?;
		let mut operations = Vec::new();
		self.portable_operations(&targets, &[], limits.bytes, &mut operations)
			.map_err(|error| failure(&error.to_string()))?;
		let mut statements = vec![Statement {
			span: None,
			kind: StatementKind::Qubit {
				name: "q".into(),
				size: Some(Expression {
					span: None,
					kind: ExpressionKind::Number(self.num_qubits().to_string()),
				}),
			},
		}];
		for operation in operations {
			statements
				.push(portable_statement(operation).map_err(|error| failure(&error.to_string()))?);
		}
		crate::qasm::export_syntax(&Module { statements }, limits)
	}
	fn portable_operations(
		&self,
		targets: &[QubitId],
		controls: &[Control],
		budget: usize,
		output: &mut Vec<Operation>,
	) -> Result<()> {
		if self.operations().len() > budget.saturating_sub(output.len()) {
			return Err(Error::Budget("oracle export operations"));
		}
		for operation in self.decompose(targets, controls, MatrixPolicy::default())? {
			match operation {
				Operation::Numerical { .. } => {
					return Err(Error::Unsupported(
						"portable numerical oracle decomposition",
					));
				}
				Operation::Oracle {
					fragment,
					targets,
					controls,
				} => fragment.portable_operations(&targets, &controls, budget, output)?,
				operation => {
					if output.len() >= budget {
						return Err(Error::Budget("oracle export operations"));
					}
					output.push(operation);
				}
			}
		}
		Ok(())
	}
}
fn portable_statement(operation: Operation) -> Result<crate::language::syntax::Statement> {
	use crate::language::syntax::{
		Expression, ExpressionKind, Modifier, Statement, StatementKind, UnaryOperator,
	};
	let number = |value: f64| {
		let positive = Expression {
			span: None,
			kind: ExpressionKind::Number(value.abs().to_string()),
		};
		if value.is_sign_negative() {
			Expression {
				span: None,
				kind: ExpressionKind::Unary(UnaryOperator::Negate, Box::new(positive)),
			}
		} else {
			positive
		}
	};
	let operand = |qubit: QubitId| Expression {
		span: None,
		kind: ExpressionKind::Index(
			Box::new(Expression {
				span: None,
				kind: ExpressionKind::Name("q".into()),
			}),
			Box::new(Expression {
				span: None,
				kind: ExpressionKind::Number(qubit.index().to_string()),
			}),
		),
	};
	let (name, arguments, targets, controls) = match operation {
		Operation::Gate {
			gate,
			targets,
			controls,
		} => (
			gate.kind().definition().name,
			gate.parameters().map(number).collect(),
			targets,
			controls,
		),

		Operation::GlobalPhase { radians, controls } => {
			("gphase", vec![number(radians)], Arc::from([]), controls)
		}
		Operation::Barrier { qubits } => {
			return Ok(Statement {
				span: None,
				kind: StatementKind::Barrier(qubits.iter().copied().map(operand).collect()),
			});
		}
		_ => return Err(Error::Unsupported("portable oracle operation")),
	};
	Ok(Statement {
		span: None,
		kind: StatementKind::Gate {
			name: name.into(),
			arguments,
			operands: controls
				.iter()
				.map(|control| operand(control.qubit()))
				.chain(targets.iter().copied().map(operand))
				.collect(),
			modifiers: controls
				.iter()
				.map(|control| Modifier::Control {
					positive: control.state() == crate::ControlState::One,
					count: None,
				})
				.collect(),
		},
	})
}
