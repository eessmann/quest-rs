//! Typed interfaces, quantum effects, and call modifiers.
use super::{
    Bit, Builder, Classical, E, ErrorKind, Expr, Expression, Float, GateKind, Int, Local,
    PhantomData, ProgramId, Qubit, S, SemanticError, SharedExpression, expression, syntax,
};

/// Checked operation modifier; powers retain their classical expression and traps.
#[derive(Debug, Clone)]
pub enum Modifier {
    Adjoint,
    Inverse,
    Control { positive: bool, count: usize },
    Power(Expr<Int<64>>),
}

/// Builder-owned gate definition. Calls retain checked arity and effects.
#[derive(Debug, Clone)]
pub struct GateDefinition {
    owner: ProgramId,
    name: String,
    parameters: usize,
    qubits: usize,
}

impl Builder {
    /// Declare an external typed scalar input.
    /// # Errors
    /// Rejects local scope, invalid widths, or invalid symbols.
    pub fn input<T: Classical>(&mut self, name: &str) -> Result<Local<T>, SemanticError> {
        self.interface(name, None, syntax::Qualifier::Input)
    }
    /// Declare an initialized typed scalar output.
    /// # Errors
    /// Rejects local scope, invalid widths, or foreign expressions.
    pub fn output<T: Classical>(
        &mut self,
        name: &str,
        initial: &Expr<T>,
    ) -> Result<Local<T>, SemanticError> {
        self.check(initial.owner)?;
        self.interface(
            name,
            Some(initial.expression.materialize(self.expression_limits)?),
            syntax::Qualifier::Output,
        )
    }
    fn interface<T: Classical>(
        &mut self,
        name: &str,
        initializer: Option<Expression>,
        qualifier: syntax::Qualifier,
    ) -> Result<Local<T>, SemanticError> {
        if self.depth != 0 {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "interfaces require module scope",
            ));
        }
        // Interface names are externally visible, unlike lexical local symbols.
        self.symbol(name)?;
        self.push(S::Declare {
            name: name.into(),
            ty: T::syntax_type()?,
            initializer,
            qualifier,
        });
        Ok(Local {
            owner: self.owner,
            name: name.into(),
            marker: PhantomData,
        })
    }
    /// Store a single-qubit measurement in typed bit storage.
    /// # Errors
    /// Rejects foreign handles; admission checks scalar width and effects.
    pub fn measure(&mut self, qubit: &Qubit, result: &Local<Bit<1>>) -> Result<(), SemanticError> {
        self.check(qubit.owner)?;
        self.check(result.owner)?;
        self.push(S::Assign {
            target: expression(E::Name(result.name.clone())),
            operator: None,
            value: expression(E::Measure(Box::new(qubit.expression.clone()))),
        });
        Ok(())
    }
    /// # Errors
    /// Rejects foreign quantum references.
    pub fn reset(&mut self, qubit: &Qubit) -> Result<(), SemanticError> {
        self.check(qubit.owner)?;
        self.push(S::Reset(qubit.expression.clone()));
        Ok(())
    }
    /// # Errors
    /// Rejects foreign quantum references; admission checks aliasing.
    pub fn barrier(&mut self, qubits: &[Qubit]) -> Result<(), SemanticError> {
        for qubit in qubits {
            self.check(qubit.owner)?;
        }
        self.push(S::Barrier(
            qubits.iter().map(|q| q.expression.clone()).collect(),
        ));
        Ok(())
    }
    fn modifiers(&self, modifiers: &[Modifier]) -> Result<Vec<syntax::Modifier>, SemanticError> {
        modifiers
            .iter()
            .map(|modifier| {
                Ok(match modifier {
                    Modifier::Adjoint => syntax::Modifier::Adjoint,
                    Modifier::Inverse => syntax::Modifier::Inverse,
                    Modifier::Control { positive, count } => {
                        if *count == 0 {
                            return Err(SemanticError::new(
                                ErrorKind::Type,
                                "control count must be positive",
                            ));
                        }
                        syntax::Modifier::Control {
                            positive: *positive,
                            count: Some(expression(E::Number(count.to_string()))),
                        }
                    }
                    Modifier::Power(power) => {
                        self.check(power.owner)?;
                        syntax::Modifier::Power(
                            power.expression.materialize(self.expression_limits)?,
                        )
                    }
                })
            })
            .collect()
    }
    /// Emit a gate with signed controls, adjoints, and checked integral powers.
    /// # Errors
    /// Rejects foreign handles; shared admission verifies aliases, arity, and effects.
    pub fn gate_with_modifiers(
        &mut self,
        gate: GateKind,
        arguments: &[Expr<Float<64>>],
        operands: &[Qubit],
        modifiers: &[Modifier],
    ) -> Result<(), SemanticError> {
        self.named_gate(gate.definition().name, arguments, operands, modifiers)
    }
    fn named_gate(
        &mut self,
        name: &str,
        arguments: &[Expr<Float<64>>],
        operands: &[Qubit],
        modifiers: &[Modifier],
    ) -> Result<(), SemanticError> {
        for arg in arguments {
            self.check(arg.owner)?;
        }
        for q in operands {
            self.check(q.owner)?;
        }
        let modifiers = self.modifiers(modifiers)?;
        self.push(S::Gate {
            name: name.into(),
            arguments: arguments
                .iter()
                .map(|v| v.expression.materialize(self.expression_limits))
                .collect::<Result<_, _>>()?,
            operands: operands.iter().map(|q| q.expression.clone()).collect(),
            modifiers,
        });
        Ok(())
    }
    /// Define a unitary gate using the same typed builder and scoped handles.
    /// # Errors
    /// Rejects invalid declarations and any nonunitary effect during admission.
    pub fn define_gate(
        &mut self,
        name: &str,
        parameters: usize,
        qubits: usize,
        body: impl FnOnce(&mut Self, &[Expr<Float<64>>], &[Qubit]) -> Result<(), SemanticError>,
    ) -> Result<GateDefinition, SemanticError> {
        if self.depth != 0 || qubits == 0 {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "gate definition requires module scope and qubits",
            ));
        }
        let name = self.symbol(name)?;
        let parameter_names = (0..parameters)
            .map(|i| self.symbol(&format!("angle_{i}")))
            .collect::<Result<Vec<_>, _>>()?;
        let qubit_names = (0..qubits)
            .map(|i| self.symbol(&format!("qubit_{i}")))
            .collect::<Result<Vec<_>, _>>()?;
        let args = parameter_names
            .iter()
            .map(|name| {
                Ok(self.wrap(SharedExpression::leaf(
                    expression(E::Name(name.clone())),
                    self.expression_limits,
                )?))
            })
            .collect::<Result<Vec<_>, SemanticError>>()?;
        let qs = qubit_names
            .iter()
            .map(|name| Qubit {
                owner: self.owner,
                expression: expression(E::Name(name.clone())),
            })
            .collect::<Vec<_>>();
        let body = self.body(|b| body(b, &args, &qs))?;
        self.push(S::GateDeclaration {
            name: name.clone(),
            parameters: parameter_names,
            qubits: qubit_names,
            body,
        });
        Ok(GateDefinition {
            owner: self.owner,
            name,
            parameters,
            qubits,
        })
    }
    /// Invoke a checked gate definition, including signed controls and powers.
    /// # Errors
    /// Rejects foreign definitions and parameter count; admission checks operand arity.
    pub fn call_gate(
        &mut self,
        definition: &GateDefinition,
        arguments: &[Expr<Float<64>>],
        operands: &[Qubit],
        modifiers: &[Modifier],
    ) -> Result<(), SemanticError> {
        self.check(definition.owner)?;
        if arguments.len() != definition.parameters || operands.len() < definition.qubits {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "gate call arity mismatch",
            ));
        }
        self.named_gate(&definition.name, arguments, operands, modifiers)
    }
    /// Declare an immutable payload identity supplied by the compilation frontend.
    /// # Errors
    /// Rejects empty interfaces or local declarations; payload arity is rechecked at verification.
    pub fn oracle(
        &mut self,
        name: &str,
        qubits: usize,
        capture: usize,
    ) -> Result<GateDefinition, SemanticError> {
        if self.depth != 0 || qubits == 0 {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "oracle requires module scope and qubits",
            ));
        }
        let name = self.symbol(name)?;
        self.push(S::Oracle {
            name: name.clone(),
            arity: expression(E::Number(qubits.to_string())),
            capture,
        });
        Ok(GateDefinition {
            owner: self.owner,
            name,
            parameters: 0,
            qubits,
        })
    }
    /// Construct a typed capture reference without converting its runtime payload.
    /// # Errors
    /// Rejects expression budget exhaustion.
    pub fn capture(&self, index: usize) -> Result<Expr<Float<64>>, SemanticError> {
        Ok(self.wrap(SharedExpression::leaf(
            expression(E::Capture(index)),
            self.expression_limits,
        )?))
    }
}
