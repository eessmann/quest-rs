//! Finite-region admission remaps operands into typed lexical storage.
use super::{Bit, Builder, E, Expression, Local, Qubit, S, SemanticError, Statement, expression};
use std::collections::BTreeMap;
impl Builder {
    /// Append a finite quantum fragment produced by the shared finite adapter.
    /// Candidate syntax is still independently admitted by `finish`; these handles
    /// only grant lexical operand mapping and never grant an executable witness.
    #[doc(hidden)]
    #[expect(
        clippy::items_after_statements,
        reason = "Local rewriting helpers are scoped to the checked fragment admission transaction"
    )]
    pub fn append_finite_fragment(
        &mut self,
        statements: Vec<Statement>,
        qubits: &[Qubit],
        bits: &[Local<Bit<1>>],
    ) -> Result<(), SemanticError> {
        let mut names = BTreeMap::new();
        for (i, qubit) in qubits.iter().enumerate() {
            self.check(qubit.owner)?;
            names.insert(format!("q{i}"), qubit.expression.clone());
        }
        for (i, bit) in bits.iter().enumerate() {
            self.check(bit.owner)?;
            names.insert(format!("c{i}"), expression(E::Name(bit.name.clone())));
        }
        let mut oracle_names = BTreeMap::new();
        for statement in &statements {
            if let S::Oracle { name, .. } = &statement.kind {
                oracle_names.insert(name.clone(), self.symbol(name)?);
            }
        }
        fn rewrite_expr(
            expr: &mut Expression,
            names: &BTreeMap<String, Expression>,
        ) -> Result<(), SemanticError> {
            match &mut expr.kind {
                E::Name(name) => {
                    *expr = names.get(name).cloned().ok_or_else(|| {
                        SemanticError::invalid("unmapped finite fragment operand")
                    })?;
                }
                E::Capture(_) | E::Number(_) | E::BitString(_) => {}
                E::Measure(value) => rewrite_expr(value, names)?,
                E::Binary(_, left, right) => {
                    rewrite_expr(left, names)?;
                    rewrite_expr(right, names)?;
                }
                _ => {
                    return Err(SemanticError::invalid(
                        "unexpected finite fragment expression",
                    ));
                }
            }
            Ok(())
        }
        fn rewrite(
            statement: &mut Statement,
            names: &BTreeMap<String, Expression>,
            oracles: &BTreeMap<String, String>,
        ) -> Result<(), SemanticError> {
            match &mut statement.kind {
                S::Gate { name, operands, .. } => {
                    if let Some(mapped) = oracles.get(name) {
                        *name = mapped.clone();
                    }
                    for operand in operands {
                        rewrite_expr(operand, names)?;
                    }
                }
                S::Oracle { name, .. } => {
                    *name = oracles
                        .get(name)
                        .cloned()
                        .ok_or_else(|| SemanticError::invalid("finite oracle declaration"))?;
                }
                S::Payload { operands, .. } | S::Barrier(operands) => {
                    for operand in operands {
                        rewrite_expr(operand, names)?;
                    }
                }
                S::Reset(value) => rewrite_expr(value, names)?,
                S::Assign { target, value, .. } => {
                    rewrite_expr(target, names)?;
                    rewrite_expr(value, names)?;
                }
                S::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    rewrite_expr(condition, names)?;
                    for item in then_body.iter_mut().chain(else_body) {
                        rewrite(item, names, oracles)?;
                    }
                }
                _ => {
                    return Err(SemanticError::invalid(
                        "unexpected finite fragment statement",
                    ));
                }
            }
            Ok(())
        }
        let mut statements = statements;
        for statement in &mut statements {
            rewrite(statement, &names, &oracle_names)?;
        }
        for statement in statements {
            self.statements.push(statement);
        }
        Ok(())
    }
}
