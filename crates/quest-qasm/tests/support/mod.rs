use quest_language::syntax::{
    Expression, ExpressionKind, Iterable, Modifier, Module, Statement, StatementKind, Type,
};
pub fn normalize(mut module: Module) -> Module {
    statements(&mut module.statements);
    module
}
fn expressions(values: &mut [Expression]) {
    for value in values {
        expression(value);
    }
}
fn ty(value: &mut Type) {
    match value {
        Type::Scalar(_, width) | Type::Qubit(width) => {
            if let Some(width) = width {
                expression(width);
            }
        }
        Type::Array {
            element,
            dimensions,
            ..
        } => {
            ty(element);
            expressions(dimensions);
        }
    }
}
fn expression(value: &mut Expression) {
    value.span = None;
    match &mut value.kind {
        ExpressionKind::Unary(_, inner) | ExpressionKind::Measure(inner) => expression(inner),
        ExpressionKind::Binary(_, left, right) | ExpressionKind::Index(left, right) => {
            expression(left);
            expression(right);
        }
        ExpressionKind::Call(_, arguments) | ExpressionKind::Array(arguments) => {
            expressions(arguments);
        }
        ExpressionKind::Cast(target, inner) => {
            ty(target);
            expression(inner);
        }
        ExpressionKind::Number(_)
        | ExpressionKind::BitString(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Name(_)
        | ExpressionKind::Capture(_) => {}
    }
}
#[expect(
    clippy::too_many_lines,
    reason = "Independent exhaustive AST normalization must visit every statement field"
)]
fn statements(body: &mut [Statement]) {
    for statement in body {
        statement.span = None;
        match &mut statement.kind {
            StatementKind::Include(_)
            | StatementKind::Break
            | StatementKind::Continue
            | StatementKind::End => {}
            StatementKind::Oracle { arity: value, .. }
            | StatementKind::Alias { value, .. }
            | StatementKind::Reset(value)
            | StatementKind::Expression(value) => expression(value),
            StatementKind::Qubit { size, .. } | StatementKind::Return(size) => {
                if let Some(value) = size {
                    expression(value);
                }
            }
            StatementKind::Declare {
                ty: target,
                initializer,
                ..
            } => {
                ty(target);
                if let Some(value) = initializer {
                    expression(value);
                }
            }
            StatementKind::Assign { target, value, .. } => {
                expression(target);
                expression(value);
            }
            StatementKind::Gate {
                arguments,
                operands,
                modifiers,
                ..
            } => {
                expressions(arguments);
                expressions(operands);
                for modifier in modifiers {
                    match modifier {
                        Modifier::Inverse | Modifier::Adjoint => {}
                        Modifier::Power(value) => expression(value),
                        Modifier::Control { count, .. } => {
                            if let Some(value) = count {
                                expression(value);
                            }
                        }
                    }
                }
            }
            StatementKind::GateDeclaration { body, .. } => statements(body),
            StatementKind::Subroutine {
                parameters,
                result,
                body,
                ..
            } => {
                for parameter in parameters {
                    ty(&mut parameter.ty);
                }
                if let Some(result) = result {
                    ty(result);
                }
                statements(body);
            }
            StatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                expression(condition);
                statements(then_body);
                statements(else_body);
            }
            StatementKind::Switch {
                selector,
                cases,
                default,
            } => {
                expression(selector);
                for (labels, body) in cases {
                    expressions(labels);
                    statements(body);
                }
                statements(default);
            }
            StatementKind::For {
                ty: target,
                iterable,
                body,
                ..
            } => {
                ty(target);
                match iterable {
                    Iterable::Range { start, step, end } => {
                        expression(start);
                        if let Some(step) = step {
                            expression(step);
                        }
                        expression(end);
                    }
                    Iterable::Set(values) => expressions(values),
                    Iterable::Expression(value) => expression(value),
                }
                statements(body);
            }
            StatementKind::While { condition, body } => {
                expression(condition);
                statements(body);
            }
            StatementKind::Barrier(values)
            | StatementKind::Payload {
                operands: values, ..
            } => expressions(values),
        }
    }
}
