//! Finite semantic insertion checks lexical handle ownership before publication.
use super::{Bit, Builder, E, Local, Qubit, S, SemanticError, expression};
use crate::semantic::finite::FiniteOperation;
impl Builder {
	/// Append typed finite operations. Full SSA type, effect and alias checks run at finish.
	/// # Errors
	/// Rejects foreign handles and excessive operation nesting.
	pub fn append_finite_fragment(
		&mut self,
		operations: Vec<FiniteOperation>,
		qubits: &[Qubit],
		bits: &[Local<Bit<1>>],
	) -> Result<(), SemanticError> {
		for q in qubits {
			self.check(q.owner)?;
		}
		for b in bits {
			self.check(b.owner)?;
		}
		for operation in &operations {
			operation.check_depth(self.expression_limits.call_depth.min(256))?;
		}
		self.push(S::Finite {
			operations,
			qubits: qubits.iter().map(|q| q.expression.clone()).collect(),
			bits: bits
				.iter()
				.map(|b| expression(E::Name(b.name.clone())))
				.collect(),
		});
		Ok(())
	}
}
