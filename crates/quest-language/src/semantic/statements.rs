use super::{ErrorKind, SemanticError, compile::Compiler};
use crate::{
    classical::ScalarType,
    ssa::{self, InstructionKind as K, Type},
    syntax::{self, BinaryOperator as B, Expression, Statement, StatementKind as S},
};
use std::collections::BTreeMap;
mod loops;

impl Compiler {
    pub fn body(&mut self, statements: &[Statement], scoped: bool) -> Result<(), SemanticError> {
        if self.depth >= self.limits.call_depth.saturating_mul(4) {
            return Err(SemanticError::limit(
                crate::ResourceKind::SyntaxNesting,
                self.depth.saturating_add(1),
                self.limits.call_depth.saturating_mul(4),
                "statement nesting budget exceeded",
            ));
        }
        self.depth = self
            .depth
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("statement nesting overflow"))?;
        if scoped {
            self.scopes.push(BTreeMap::new());
        }
        let result = statements
            .iter()
            .try_for_each(|statement| self.statement(statement));
        if scoped {
            self.scopes.pop();
        }
        self.depth = self.depth.saturating_sub(1);
        result
    }
    fn statement(&mut self, statement: &Statement) -> Result<(), SemanticError> {
        self.budget()?;
        if self.terminated()? {
            return Err(SemanticError::new(
                ErrorKind::ControlFlow,
                "unreachable statement after control transfer",
            ));
        }
        self.statement_inner(statement).map_err(|mut error| {
            if error.span.is_none() {
                error.span = statement.span;
            }
            error
        })
    }
    fn declare_qubit(
        &mut self,
        name: &str,
        size: Option<&Expression>,
        span: Option<crate::SourceSpan>,
    ) -> Result<(), SemanticError> {
        if self.region != self.program.entry || self.depth != 1 {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "quantum declarations require global module scope",
            ));
        }
        let size = size.map_or(Ok(1), |value| self.positive_size(value))?;
        let binding = self.slot(
            name.into(),
            Type::Qubit(size),
            true,
            ssa::Interface::Local,
            false,
        )?;
        self.effect(
            K::Allocate {
                slot: binding.place.slot,
                memory: self.memory,
            },
            span,
        )?;
        self.bind(name.into(), binding)
    }
    fn statement_inner(&mut self, statement: &Statement) -> Result<(), SemanticError> {
        let span = statement.span;
        match &statement.kind {
            S::Include(_) => Err(SemanticError::new(
                ErrorKind::Capability,
                "includes must be resolved explicitly before admission",
            )),
            S::Qubit { name, size } => self.declare_qubit(name, size.as_ref(), span),
            S::Declare {
                name,
                ty,
                initializer,
                qualifier,
            } => self.declare(name, ty, initializer.as_ref(), *qualifier, span),
            S::Alias { name, value } => {
                if !self.is_place(value) {
                    return Err(SemanticError::new(
                        ErrorKind::Capability,
                        "this alias profile requires a named or indexed reference",
                    ));
                }
                let binding = self.place(value)?;
                self.bind(name.clone(), binding)
            }
            S::Assign {
                target,
                operator,
                value,
            } => self.assign(target, *operator, value, span),
            S::Gate {
                name,
                arguments,
                operands,
                modifiers,
            } => self.gate(name, arguments, operands, modifiers, span),
            S::Oracle { .. } | S::GateDeclaration { .. } | S::Subroutine { .. } => {
                if self.region != self.program.entry || self.scopes.len() != 1 {
                    Err(SemanticError::new(
                        ErrorKind::ControlFlow,
                        "function declarations must be global",
                    ))
                } else {
                    Ok(())
                }
            }
            S::Reset(value) => {
                let place = self.place(value)?.place;
                self.effect(
                    K::Reset {
                        place,
                        memory: self.memory,
                    },
                    span,
                )
            }
            S::Barrier(values) => {
                let places = values
                    .iter()
                    .map(|value| self.place(value).map(|binding| binding.place))
                    .collect::<Result<Vec<_>, _>>()?;
                self.effect(
                    K::Barrier {
                        places,
                        memory: self.memory,
                    },
                    span,
                )
            }
            S::Expression(expression) => {
                if let syntax::ExpressionKind::Call(name, arguments) = &expression.kind {
                    self.call(name, arguments, Vec::new(), Vec::new(), span)?;
                } else {
                    self.expr(expression)?;
                }
                Ok(())
            }
            _ => self.control_statement(statement),
        }
    }
    fn control_statement(&mut self, statement: &Statement) -> Result<(), SemanticError> {
        match &statement.kind {
            S::If {
                condition,
                then_body,
                else_body,
            } => self.if_statement(condition, then_body, else_body),
            S::Switch {
                selector,
                cases,
                default,
            } => self.switch_statement(selector, cases, default),
            S::While { condition, body } => self.while_statement(condition, body),
            S::For {
                name,
                ty,
                iterable,
                body,
            } => self.for_statement(name, ty, iterable, body),
            S::Return(value) => self.return_statement(value.as_ref()),
            S::Break => {
                let target = self
                    .loops
                    .last()
                    .ok_or_else(|| {
                        SemanticError::new(ErrorKind::ControlFlow, "break outside loop")
                    })?
                    .0;
                self.jump(target)
            }
            S::Continue => {
                let target = self
                    .loops
                    .last()
                    .ok_or_else(|| {
                        SemanticError::new(ErrorKind::ControlFlow, "continue outside loop")
                    })?
                    .1;
                self.jump(target)
            }
            S::End => {
                if self
                    .program
                    .regions
                    .get(self.region.index())
                    .is_some_and(|region| region.gate)
                {
                    return Err(SemanticError::new(
                        ErrorKind::Type,
                        "end is not a unitary gate effect",
                    ));
                }
                self.terminate(ssa::Terminator::End {
                    memory: self.memory,
                })
            }
            _ => Err(SemanticError::invalid("unexpected control statement")),
        }
    }
    fn declare(
        &mut self,
        name: &str,
        ty: &syntax::Type,
        initializer: Option<&Expression>,
        qualifier: syntax::Qualifier,
        span: Option<crate::SourceSpan>,
    ) -> Result<(), SemanticError> {
        let ty = self.resolve_type(ty)?;
        let interface = match qualifier {
            syntax::Qualifier::Input => ssa::Interface::Input,
            syntax::Qualifier::Output => ssa::Interface::Output,
            _ => ssa::Interface::Local,
        };
        if interface != ssa::Interface::Local
            && (self.region != self.program.entry || self.scopes.len() != 1)
        {
            return Err(SemanticError::new(
                ErrorKind::ControlFlow,
                "typed I/O declarations must be global",
            ));
        }
        if interface == ssa::Interface::Input && initializer.is_some() {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "input cannot have initializer",
            ));
        }
        if qualifier == syntax::Qualifier::Const && initializer.is_none() {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "const declaration requires initializer",
            ));
        }
        let mut binding = self.slot(
            name.into(),
            ty.clone(),
            qualifier != syntax::Qualifier::Const && qualifier != syntax::Qualifier::Input,
            interface,
            false,
        )?;
        if let Some(expression) = initializer {
            if qualifier == syntax::Qualifier::Const {
                self.require_constant(expression)?;
            }
            if qualifier == syntax::Qualifier::Const && matches!(ty, Type::Scalar(_)) {
                let Type::Scalar(target) = ty else {
                    return Err(SemanticError::invalid("missing scalar type"));
                };
                binding.constant =
                    Some(self.const_eval(expression)?.cast(target).map_err(|error| {
                        SemanticError::new(ErrorKind::Numerical, error.to_string())
                    })?);
            }
            let value = self.expr_as(expression, &ty)?;
            self.effect(
                K::Store {
                    place: binding.place.clone(),
                    value,
                    memory: self.memory,
                    initializing: true,
                },
                span,
            )?;
        } else if interface == ssa::Interface::Input {
            self.effect(
                K::Input {
                    slot: binding.place.slot,
                    memory: self.memory,
                },
                span,
            )?;
        }
        if initializer.is_none()
            && interface != ssa::Interface::Input
            && matches!(ty, Type::Array { .. } | Type::Scalar(ScalarType::Bit(_)))
        {
            self.effect(
                K::AllocateArray {
                    slot: binding.place.slot,
                    memory: self.memory,
                },
                span,
            )?;
        }
        self.bind(name.into(), binding)
    }
    fn assign(
        &mut self,
        target: &Expression,
        operator: Option<B>,
        expression: &Expression,
        span: Option<crate::SourceSpan>,
    ) -> Result<(), SemanticError> {
        let binding = self.place(target)?;
        if !binding.mutable {
            return Err(SemanticError::new(
                ErrorKind::Alias,
                "assignment to immutable binding",
            ));
        }
        let old = if operator.is_some() {
            Some(self.emit_one(
                K::Load {
                    place: binding.place.clone(),
                    memory: self.memory,
                },
                binding.ty.clone(),
                span,
            )?)
        } else {
            None
        };
        let mut value = if operator.is_none() {
            self.expr_as(expression, &binding.ty)?
        } else {
            self.expr(expression)?
        };
        if let (Some(operator), Some(left)) = (operator, old) {
            let ty = super::binary_type(operator, self.ty(left)?, self.ty(value)?)?;
            value = self.emit_one(
                K::Binary {
                    operator,
                    left,
                    right: value,
                },
                ty,
                span,
            )?;
        }
        let value = self.coerce(value, &binding.ty, span)?;
        self.effect(
            K::Store {
                place: binding.place,
                value,
                memory: self.memory,
                initializing: false,
            },
            span,
        )
    }
    fn gate(
        &mut self,
        name: &str,
        arguments: &[Expression],
        operands: &[Expression],
        modifiers: &[syntax::Modifier],
        span: Option<crate::SourceSpan>,
    ) -> Result<(), SemanticError> {
        let modifiers = self.modifiers(modifiers)?;
        if let Some(gate) = crate::GateKind::lookup(name) {
            let arguments = arguments
                .iter()
                .map(|argument| self.expr(argument))
                .collect::<Result<Vec<_>, _>>()?;
            let operands = operands
                .iter()
                .map(|operand| self.place(operand).map(|binding| binding.place))
                .collect::<Result<Vec<_>, _>>()?;
            self.effect(
                K::Gate {
                    gate,
                    arguments,
                    operands,
                    modifiers,
                    memory: self.memory,
                },
                span,
            )
        } else {
            let region = self
                .functions
                .get(name)
                .and_then(|function| self.program.regions.get(function.region.index()))
                .ok_or_else(|| {
                    SemanticError::new(ErrorKind::UnknownSymbol, format!("unknown gate {name}"))
                })?;
            if region.oracle.is_some()
                && modifiers.iter().any(|modifier| match modifier {
                    ssa::GateModifier::Inverse => true,
                    ssa::GateModifier::Power(value) => self
                        .constants
                        .get(value)
                        .and_then(|value| value.to_i128().ok())
                        .is_some_and(|value| value < 0),
                    _ => false,
                })
            {
                return Err(SemanticError::new(
                    ErrorKind::Capability,
                    "numerical oracle requires adjoint; inverse and negative powers require exact semantics",
                ));
            }
            if !region.gate {
                return Err(SemanticError::new(
                    ErrorKind::Type,
                    "mixed subroutine requires call syntax",
                ));
            }
            let count = modifiers
                .iter()
                .try_fold(0usize, |total, modifier| match modifier {
                    ssa::GateModifier::Control { count, .. } => total
                        .checked_add(*count)
                        .ok_or_else(|| SemanticError::budget("control count overflow")),
                    _ => Ok(total),
                })?;
            let control_expressions = operands.get(..count).ok_or_else(|| {
                SemanticError::new(ErrorKind::Type, "missing controlled gate operands")
            })?;
            let controls = control_expressions
                .iter()
                .map(|operand| self.place(operand).map(|binding| binding.place))
                .collect::<Result<Vec<_>, _>>()?;
            let remaining = operands
                .get(count..)
                .ok_or_else(|| SemanticError::new(ErrorKind::Type, "missing target operands"))?;
            let actual = arguments
                .iter()
                .chain(remaining)
                .cloned()
                .collect::<Vec<_>>();
            self.call(name, &actual, controls, modifiers, span)?;
            Ok(())
        }
    }
    fn modifiers(
        &mut self,
        modifiers: &[syntax::Modifier],
    ) -> Result<Vec<ssa::GateModifier>, SemanticError> {
        modifiers
            .iter()
            .map(|modifier| match modifier {
                syntax::Modifier::Inverse => Ok(ssa::GateModifier::Inverse),
                syntax::Modifier::Adjoint => Ok(ssa::GateModifier::Adjoint),
                syntax::Modifier::Control { positive, count } => Ok(ssa::GateModifier::Control {
                    positive: *positive,
                    count: count
                        .as_ref()
                        .map_or(Ok(1), |value| self.positive_size(value))?,
                }),
                syntax::Modifier::Power(value) => {
                    let power = self.expr(value)?;
                    if !matches!(self.ty(power)?, Type::Scalar(ScalarType::Int(_) | ScalarType::Uint(_))) {
                        return Err(SemanticError::new(ErrorKind::Type, "gate power requires int or uint; other categories require an explicit integer cast").at(value.span));
                    }
                    Ok(ssa::GateModifier::Power(power))
                },
            })
            .collect()
    }
    fn if_statement(
        &mut self,
        condition: &Expression,
        then_body: &[Statement],
        else_body: &[Statement],
    ) -> Result<(), SemanticError> {
        let condition = self.expr(condition)?;
        let yes = self.new_block()?;
        let no = self.new_block()?;
        let merge = self.new_block()?;
        self.branch(condition, yes, no)?;
        self.switch_block(yes)?;
        self.body(then_body, true)?;
        if !self.terminated()? {
            self.jump(merge)?;
        }
        self.switch_block(no)?;
        self.body(else_body, true)?;
        if !self.terminated()? {
            self.jump(merge)?;
        }
        self.switch_block(merge)?;
        if self
            .program
            .blocks
            .get(merge.index())
            .is_some_and(|block| block.predecessors.is_empty())
        {
            self.terminate(ssa::Terminator::End {
                memory: self.memory,
            })?;
        }
        Ok(())
    }
    fn return_statement(&mut self, expression: Option<&Expression>) -> Result<(), SemanticError> {
        if self.region == self.program.entry {
            return Err(SemanticError::new(
                ErrorKind::ControlFlow,
                "return outside subroutine",
            ));
        }
        let ty = self
            .program
            .regions
            .get(self.region.index())
            .ok_or_else(|| SemanticError::invalid("missing return region"))?
            .result
            .clone();
        let value = expression
            .map(|value| {
                self.expr(value).and_then(|value| {
                    self.coerce(
                        value,
                        &ty,
                        expression.and_then(|expression| expression.span),
                    )
                })
            })
            .transpose()?;
        if (ty == Type::Void) != value.is_none() {
            return Err(SemanticError::new(ErrorKind::Type, "return value mismatch"));
        }
        self.terminate(ssa::Terminator::Return {
            value,
            memory: self.memory,
        })
    }
    fn switch_statement(
        &mut self,
        expression: &Expression,
        cases: &[(Vec<Expression>, Vec<Statement>)],
        default: &[Statement],
    ) -> Result<(), SemanticError> {
        let selector = self.expr(expression)?;
        let ty = self.ty(selector)?.clone();
        if !matches!(
            ty,
            Type::Scalar(ScalarType::Int(_) | ScalarType::Uint(_) | ScalarType::Bit(_))
        ) {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "switch selector must be integer",
            ));
        }
        let merge = self.new_block()?;
        let mut seen = Vec::new();
        for (labels, body) in cases {
            if labels.is_empty() {
                return Err(SemanticError::new(ErrorKind::Type, "empty switch case"));
            }
            let mut condition = None;
            for label in labels {
                let Type::Scalar(selector_type) = ty else {
                    return Err(SemanticError::invalid("switch scalar type missing"));
                };
                let value = self.const_eval(label)?.cast(selector_type)?;
                if seen.contains(&value) {
                    return Err(SemanticError::new(
                        ErrorKind::DuplicateSymbol,
                        "duplicate switch case",
                    ));
                }
                seen.push(value);
                let value =
                    self.emit_one(K::Constant(value), Type::Scalar(value.ty()), label.span)?;
                let value = self.coerce(value, &ty, label.span)?;
                let equal = self.emit_one(
                    K::Binary {
                        operator: B::Equal,
                        left: selector,
                        right: value,
                    },
                    Type::Scalar(ScalarType::Bool),
                    label.span,
                )?;
                condition = Some(if let Some(left) = condition {
                    self.emit_one(
                        K::Binary {
                            operator: B::Or,
                            left,
                            right: equal,
                        },
                        Type::Scalar(ScalarType::Bool),
                        label.span,
                    )?
                } else {
                    equal
                });
            }
            let matched = self.new_block()?;
            let next = self.new_block()?;
            self.branch(
                condition.ok_or_else(|| SemanticError::invalid("missing case condition"))?,
                matched,
                next,
            )?;
            self.switch_block(matched)?;
            self.body(body, true)?;
            if !self.terminated()? {
                self.jump(merge)?;
            }
            self.switch_block(next)?;
        }
        self.body(default, true)?;
        if !self.terminated()? {
            self.jump(merge)?;
        }
        self.switch_block(merge)?;
        if self
            .program
            .blocks
            .get(merge.index())
            .is_some_and(|block| block.predecessors.is_empty())
        {
            self.terminate(ssa::Terminator::End {
                memory: self.memory,
            })?;
        }
        Ok(())
    }
}
