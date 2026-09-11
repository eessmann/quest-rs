use super::{
    BinaryOperator as Bin, Expression, ExpressionKind, Iterable, Modifier, Module, Parameter,
    ParseError, ParseErrorKind, ParseLimits, Qualifier, ScalarKind, Statement, StatementKind,
    Token, TokenKind, Type, UnaryOperator, lex,
};
use crate::SourceSnapshot;

/// Parse the simulator profile's structured syntax.
///
/// # Errors
/// Returns a located syntax, capability or resource diagnostic.
pub fn parse_source(source: &SourceSnapshot) -> Result<Module, ParseError> {
    let limits = ParseLimits::default();
    parse_tokens(&lex(source, limits)?, limits)
}
/// Shared entrypoint for source and procedural-macro token adapters.
///
/// # Errors
/// Rejects malformed syntax and resource-limit violations.
pub fn parse_tokens(tokens: &[Token], limits: ParseLimits) -> Result<Module, ParseError> {
    if tokens.len() > limits.tokens {
        return Err(ParseError::budget_limit(
            "token budget exceeded",
            crate::ResourceKind::SyntaxTokens,
            tokens.len(),
            limits.tokens,
        ));
    }
    if let Some(token) = tokens.iter().find(|token| token.kind.spelling() == "++") {
        return Err(ParseError {
            kind: ParseErrorKind::Capability,
            span: token.span,
            resource: None,
            message: "register concatenation is outside the simulator profile".into(),
        });
    }
    let mut parser = Parser {
        tokens,
        position: 0,
        depth: 0,
        limits,
    };
    if parser.eat("OPENQASM") {
        let version = parser.next()?;
        if version.kind != TokenKind::Number("3.1".into()) {
            return Err(ParseError {
                resource: None,
                kind: ParseErrorKind::Capability,
                span: version.span,
                message: "this profile requires OPENQASM 3.1".into(),
            });
        }
        parser.require(";")?;
    }
    let mut statements = Vec::new();
    while parser.peek().is_some() {
        statements.push(parser.statement()?);
    }
    Ok(Module { statements })
}
struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    depth: usize,
    limits: ParseLimits,
}
impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }
    fn is(&self, text: &str) -> bool {
        self.peek().is_some_and(|t| t.kind.spelling() == text)
    }
    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError::syntax(self.peek().and_then(|t| t.span), message)
    }
    fn next(&mut self) -> Result<Token, ParseError> {
        let token = self
            .peek()
            .cloned()
            .ok_or_else(|| self.error("unexpected end of source"))?;
        self.position = self
            .position
            .checked_add(1)
            .ok_or_else(|| ParseError::budget("token cursor overflow"))?;
        Ok(token)
    }
    fn eat(&mut self, text: &str) -> bool {
        if !self.is(text) {
            return false;
        }
        if let Some(position) = self.position.checked_add(1) {
            self.position = position;
            true
        } else {
            false
        }
    }
    fn require(&mut self, text: &str) -> Result<(), ParseError> {
        if self.eat(text) {
            Ok(())
        } else {
            Err(self.error(format!("expected {text:?}")))
        }
    }
    fn name(&mut self) -> Result<String, ParseError> {
        let token = self.next()?;
        if let TokenKind::Identifier(name) = token.kind {
            Ok(name)
        } else {
            Err(ParseError::syntax(token.span, "expected an identifier"))
        }
    }
    fn enter(&mut self) -> Result<(), ParseError> {
        if self.depth >= self.limits.nesting {
            return Err(ParseError::budget_limit(
                "syntax nesting budget exceeded",
                crate::ResourceKind::SyntaxNesting,
                self.depth.saturating_add(1),
                self.limits.nesting,
            ));
        }
        self.depth = self
            .depth
            .checked_add(1)
            .ok_or_else(|| ParseError::budget("nesting overflow"))?;
        Ok(())
    }
    const fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
    fn block(&mut self) -> Result<Vec<Statement>, ParseError> {
        self.enter()?;
        self.require("{")?;
        let mut body = Vec::new();
        while !self.eat("}") {
            body.push(self.statement()?);
        }
        self.leave();
        Ok(body)
    }
    fn condition(&mut self) -> Result<Expression, ParseError> {
        self.require("(")?;
        let result = self.expression(0)?;
        self.require(")")?;
        Ok(result)
    }
    fn oracle_declaration(&mut self) -> Result<StatementKind, ParseError> {
        self.next()?;
        let name = self.name()?;
        self.require("[")?;
        let arity = self.expression(0)?;
        self.require("]")?;
        self.require("=")?;
        let token = self.next()?;
        let TokenKind::Capture(capture) = token.kind else {
            return Err(ParseError::syntax(
                token.span,
                "oracle requires a Rust fragment capture",
            ));
        };
        self.require(";")?;
        Ok(StatementKind::Oracle {
            name,
            arity,
            capture,
        })
    }
    fn statement(&mut self) -> Result<Statement, ParseError> {
        let span = self.peek().and_then(|t| t.span);
        let keyword = self
            .peek()
            .map(|t| t.kind.spelling().to_owned())
            .ok_or_else(|| self.error("expected a statement"))?;
        let kind = match keyword.as_str() {
            "include" => {
                self.next()?;
                let token = self.next()?;
                let TokenKind::String(path) = token.kind else {
                    return Err(ParseError::syntax(token.span, "expected include filename"));
                };
                self.require(";")?;
                StatementKind::Include(path)
            }
            "defcal" | "cal" | "defcalgrammar" | "delay" | "box" | "extern" | "duration"
            | "stretch" | "complex" => {
                return Err(ParseError {
                    resource: None,
                    kind: ParseErrorKind::Capability,
                    span,
                    message: format!("{keyword} is outside the simulator profile"),
                });
            }
            "let" => {
                self.next()?;
                let name = self.name()?;
                self.require("=")?;
                let value = self.expression(0)?;
                self.require(";")?;
                StatementKind::Alias { name, value }
            }
            "oracle" => self.oracle_declaration()?,
            "gate" => self.gate_declaration()?,
            "def" => self.subroutine()?,
            "qubit" => {
                self.next()?;
                let size = self.optional_width()?;
                let name = self.name()?;
                self.require(";")?;
                StatementKind::Qubit { name, size }
            }
            "input" | "output" | "const" | "bool" | "bit" | "int" | "uint" | "angle" | "float"
            | "array" => self.declaration()?,
            "if" => self.if_statement()?,
            "while" => {
                self.next()?;
                let condition = self.condition()?;
                let body = self.block()?;
                StatementKind::While { condition, body }
            }
            "for" => self.for_loop()?,
            "switch" => self.switch()?,
            "break" | "continue" | "end" => {
                self.next()?;
                self.require(";")?;
                match keyword.as_str() {
                    "break" => StatementKind::Break,
                    "continue" => StatementKind::Continue,
                    _ => StatementKind::End,
                }
            }
            "return" => {
                self.next()?;
                let value = if self.is(";") {
                    None
                } else {
                    Some(self.expression(0)?)
                };
                self.require(";")?;
                StatementKind::Return(value)
            }
            "reset" => {
                self.next()?;
                let target = self.expression(0)?;
                self.require(";")?;
                StatementKind::Reset(target)
            }
            "barrier" => {
                self.next()?;
                StatementKind::Barrier(self.expression_list(";")?)
            }
            "measure" => {
                let value = self.expression(0)?;
                let kind = if self.eat("->") {
                    StatementKind::Assign {
                        target: self.expression(0)?,
                        operator: None,
                        value,
                    }
                } else {
                    StatementKind::Expression(value)
                };
                self.require(";")?;
                kind
            }
            _ => self.operation()?,
        };
        Ok(Statement { kind, span })
    }
    fn if_statement(&mut self) -> Result<StatementKind, ParseError> {
        self.next()?;
        let condition = self.condition()?;
        let then_body = self.block()?;
        let else_body = if self.eat("else") {
            if self.is("if") {
                vec![self.statement()?]
            } else {
                self.block()?
            }
        } else {
            Vec::new()
        };
        Ok(StatementKind::If {
            condition,
            then_body,
            else_body,
        })
    }
    fn declaration(&mut self) -> Result<StatementKind, ParseError> {
        let qualifier = if self.eat("input") {
            Qualifier::Input
        } else if self.eat("output") {
            Qualifier::Output
        } else if self.eat("const") {
            Qualifier::Const
        } else {
            Qualifier::Local
        };
        let ty = self.ty()?;
        let name = self.name()?;
        let initializer = if self.eat("=") {
            Some(self.expression(0)?)
        } else {
            None
        };
        self.require(";")?;
        Ok(StatementKind::Declare {
            name,
            ty,
            initializer,
            qualifier,
        })
    }
    fn optional_width(&mut self) -> Result<Option<Expression>, ParseError> {
        if !self.eat("[") {
            return Ok(None);
        }
        let width = self.expression(0)?;
        self.require("]")?;
        Ok(Some(width))
    }
    fn ty(&mut self) -> Result<Type, ParseError> {
        let name = self.name()?;
        if name == "array" {
            self.require("[")?;
            let element = Box::new(self.ty()?);
            self.require(",")?;
            let dimensions = self.expression_list("]")?;
            if dimensions.is_empty() {
                return Err(self.error("arrays require fixed dimensions"));
            }
            return Ok(Type::Array {
                element,
                dimensions,
                reference: false,
            });
        }
        if name == "qubit" {
            return Ok(Type::Qubit(self.optional_width()?.map(Box::new)));
        }
        let kind = scalar_kind(&name)
            .ok_or_else(|| self.error(format!("unsupported classical type {name}")))?;
        Ok(Type::Scalar(kind, self.optional_width()?.map(Box::new)))
    }
    fn gate_declaration(&mut self) -> Result<StatementKind, ParseError> {
        self.require("gate")?;
        let name = self.name()?;
        let mut parameters = Vec::new();
        if self.eat("(") {
            while !self.eat(")") {
                parameters.push(self.name()?);
                if !self.is(")") {
                    self.require(",")?;
                }
            }
        }
        let mut qubits = Vec::new();
        while !self.is("{") {
            qubits.push(self.name()?);
            if !self.is("{") {
                self.require(",")?;
            }
        }
        let body = self.block()?;
        Ok(StatementKind::GateDeclaration {
            name,
            parameters,
            qubits,
            body,
        })
    }
    fn subroutine(&mut self) -> Result<StatementKind, ParseError> {
        self.require("def")?;
        let name = self.name()?;
        self.require("(")?;
        let mut parameters = Vec::new();
        while !self.eat(")") {
            let mutable = if self.eat("readonly") {
                false
            } else {
                self.eat("mutable")
            };
            let mut ty = self.ty()?;
            if let Type::Array { reference, .. } = &mut ty {
                *reference = true;
            }
            let name = self.name()?;
            parameters.push(Parameter { name, ty, mutable });
            if !self.is(")") {
                self.require(",")?;
            }
        }
        let result = if self.eat("->") {
            Some(self.ty()?)
        } else {
            None
        };
        let body = self.block()?;
        Ok(StatementKind::Subroutine {
            name,
            parameters,
            result,
            body,
        })
    }
    fn for_loop(&mut self) -> Result<StatementKind, ParseError> {
        self.require("for")?;
        let ty = self.ty()?;
        let name = self.name()?;
        self.require("in")?;
        let iterable = if self.eat("[") {
            let start = self.expression(0)?;
            self.require(":")?;
            let middle = self.expression(0)?;
            let (step, end) = if self.eat(":") {
                (Some(middle), self.expression(0)?)
            } else {
                (None, middle)
            };
            self.require("]")?;
            Iterable::Range { start, step, end }
        } else if self.eat("{") {
            Iterable::Set(self.expression_list("}")?)
        } else {
            Iterable::Expression(self.expression(0)?)
        };
        let body = self.block()?;
        Ok(StatementKind::For {
            name,
            ty,
            iterable,
            body,
        })
    }
    fn switch(&mut self) -> Result<StatementKind, ParseError> {
        self.require("switch")?;
        let selector = self.condition()?;
        self.require("{")?;
        let mut cases = Vec::new();
        let mut default = Vec::new();
        let mut default_seen = false;
        while !self.eat("}") {
            if self.eat("case") {
                let mut labels = vec![self.expression(0)?];
                while self.eat(",") {
                    labels.push(self.expression(0)?);
                }
                cases.push((labels, self.block()?));
            } else {
                self.require("default")?;
                if default_seen {
                    return Err(self.error("duplicate default case"));
                }
                default_seen = true;
                default = self.block()?;
            }
        }
        Ok(StatementKind::Switch {
            selector,
            cases,
            default,
        })
    }
    fn operation(&mut self) -> Result<StatementKind, ParseError> {
        let mut modifiers = Vec::new();
        loop {
            if self.eat("adjoint") {
                modifiers.push(Modifier::Adjoint);
                self.require("@")?;
            } else if self.eat("inv") {
                modifiers.push(Modifier::Inverse);
                self.require("@")?;
            } else if self.is("ctrl") || self.is("negctrl") {
                let positive = self.next()?.kind.spelling() == "ctrl";
                let count = if self.is("(") {
                    Some(self.condition()?)
                } else {
                    None
                };
                modifiers.push(Modifier::Control { positive, count });
                self.require("@")?;
            } else if self.eat("pow") {
                modifiers.push(Modifier::Power(self.condition()?));
                self.require("@")?;
            } else {
                break;
            }
        }
        let span = self.peek().and_then(|t| t.span);
        let name = self.name()?;
        if modifiers.is_empty()
            && (self.is("[") || assignment(self.peek().map(|t| t.kind.spelling())).is_some())
        {
            let base = Expression {
                kind: ExpressionKind::Name(name),
                span,
            };
            let target = self.indices(base)?;
            let operator = assignment(self.peek().map(|t| t.kind.spelling()))
                .ok_or_else(|| self.error("expected assignment operator"))?;
            let operator = match operator {
                Assignment::Replace => None,
                Assignment::Compound(operator) => Some(operator),
            };
            self.next()?;
            let value = self.expression(0)?;
            self.require(";")?;
            return Ok(StatementKind::Assign {
                target,
                operator,
                value,
            });
        }
        let arguments = if self.eat("(") {
            self.expression_list(")")?
        } else {
            Vec::new()
        };
        let terminated = self.eat(";");
        if terminated && name != "gphase" && modifiers.is_empty() {
            return Ok(StatementKind::Expression(Expression {
                kind: ExpressionKind::Call(name, arguments),
                span,
            }));
        }
        // gphase has no quantum operands; other gate interfaces are checked during admission.
        let operands = if terminated {
            Vec::new()
        } else {
            self.expression_list(";")?
        };
        Ok(StatementKind::Gate {
            name,
            arguments,
            operands,
            modifiers,
        })
    }
    fn expression_list(&mut self, end: &str) -> Result<Vec<Expression>, ParseError> {
        let mut values = Vec::new();
        if self.eat(end) {
            return Ok(values);
        }
        loop {
            values.push(self.expression(0)?);
            if self.eat(end) {
                return Ok(values);
            }
            self.require(",")?;
        }
    }
    fn expression(&mut self, minimum: u8) -> Result<Expression, ParseError> {
        self.enter()?;
        let mut lhs = self.prefix()?;
        let mut operations = 0usize;
        while let Some((operator, left, right)) =
            self.peek().and_then(|t| binary(t.kind.spelling()))
        {
            if left < minimum {
                break;
            }
            operations = operations
                .checked_add(1)
                .ok_or_else(|| ParseError::budget("expression size overflow"))?;
            if operations >= self.limits.nesting {
                return Err(ParseError::budget_limit(
                    "expression depth budget exceeded",
                    crate::ResourceKind::SyntaxNesting,
                    operations,
                    self.limits.nesting,
                ));
            }
            self.next()?;
            let rhs = self.expression(right)?;
            let span = lhs.span;
            lhs = Expression {
                kind: ExpressionKind::Binary(operator, Box::new(lhs), Box::new(rhs)),
                span,
            };
        }
        self.leave();
        Ok(lhs)
    }
    fn prefix(&mut self) -> Result<Expression, ParseError> {
        let token = self.next()?;
        let span = token.span;
        let kind = match token.kind {
            TokenKind::Number(number) => ExpressionKind::Number(number),
            TokenKind::String(bits) => ExpressionKind::BitString(bits),
            TokenKind::Capture(id) => ExpressionKind::Capture(id),
            TokenKind::Identifier(name) if name == "true" || name == "false" => {
                ExpressionKind::Bool(name == "true")
            }
            TokenKind::Identifier(name) if name == "measure" => {
                ExpressionKind::Measure(Box::new(self.expression(24)?))
            }
            TokenKind::Identifier(name) => {
                if let Some(kind) = scalar_kind(&name) {
                    let ty = Type::Scalar(kind, self.optional_width()?.map(Box::new));
                    ExpressionKind::Cast(ty, Box::new(self.condition()?))
                } else if self.eat("(") {
                    ExpressionKind::Call(name, self.expression_list(")")?)
                } else {
                    ExpressionKind::Name(name)
                }
            }
            TokenKind::Symbol(symbol) if symbol == "(" => {
                let value = self.expression(0)?;
                self.require(")")?;
                return self.indices(value);
            }
            TokenKind::Symbol(symbol) if symbol == "{" => {
                ExpressionKind::Array(self.expression_list("}")?)
            }
            TokenKind::Symbol(symbol) => {
                let operator = match symbol.as_str() {
                    "-" => UnaryOperator::Negate,
                    "+" => UnaryOperator::Positive,
                    "!" => UnaryOperator::Not,
                    "~" => UnaryOperator::Complement,
                    _ => return Err(ParseError::syntax(span, "expected expression")),
                };
                ExpressionKind::Unary(operator, Box::new(self.expression(23)?))
            }
        };
        self.indices(Expression { kind, span })
    }
    fn indices(&mut self, mut value: Expression) -> Result<Expression, ParseError> {
        let mut count = 0usize;
        while self.eat("[") {
            loop {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| ParseError::budget("index depth overflow"))?;
                if count >= self.limits.nesting {
                    return Err(ParseError::budget_limit(
                        "index depth exceeded",
                        crate::ResourceKind::SyntaxNesting,
                        count,
                        self.limits.nesting,
                    ));
                }
                let index = self.expression(0)?;
                if self.is(":") {
                    return Err(ParseError {
                        kind: ParseErrorKind::Capability,
                        span: self.peek().and_then(|token| token.span),
                        resource: None,
                        message: "range slices are outside the simulator profile".into(),
                    });
                }
                let span = value.span;
                value = Expression {
                    kind: ExpressionKind::Index(Box::new(value), Box::new(index)),
                    span,
                };
                if self.eat("]") {
                    break;
                }
                self.require(",")?;
            }
        }
        Ok(value)
    }
}
fn scalar_kind(name: &str) -> Option<ScalarKind> {
    Some(match name {
        "bool" => ScalarKind::Bool,
        "bit" => ScalarKind::Bit,
        "int" => ScalarKind::Int,
        "uint" => ScalarKind::Uint,
        "angle" => ScalarKind::Angle,
        "float" => ScalarKind::Float,
        _ => return None,
    })
}
enum Assignment {
    Replace,
    Compound(Bin),
}
fn assignment(token: Option<&str>) -> Option<Assignment> {
    Some(match token? {
        "=" => Assignment::Replace,
        "+=" => Assignment::Compound(Bin::Add),
        "-=" => Assignment::Compound(Bin::Subtract),
        "*=" => Assignment::Compound(Bin::Multiply),
        "/=" => Assignment::Compound(Bin::Divide),
        "%=" => Assignment::Compound(Bin::Remainder),
        "&=" => Assignment::Compound(Bin::BitAnd),
        "|=" => Assignment::Compound(Bin::BitOr),
        "^=" => Assignment::Compound(Bin::BitXor),
        "<<=" => Assignment::Compound(Bin::ShiftLeft),
        ">>=" => Assignment::Compound(Bin::ShiftRight),
        _ => return None,
    })
}
fn binary(token: &str) -> Option<(Bin, u8, u8)> {
    Some(match token {
        "||" => (Bin::Or, 1, 2),
        "&&" => (Bin::And, 3, 4),
        "|" => (Bin::BitOr, 5, 6),
        "^" => (Bin::BitXor, 7, 8),
        "&" => (Bin::BitAnd, 9, 10),
        "==" => (Bin::Equal, 11, 12),
        "!=" => (Bin::NotEqual, 11, 12),
        "<" => (Bin::Less, 13, 14),
        "<=" => (Bin::LessEqual, 13, 14),
        ">" => (Bin::Greater, 13, 14),
        ">=" => (Bin::GreaterEqual, 13, 14),
        "<<" => (Bin::ShiftLeft, 15, 16),
        ">>" => (Bin::ShiftRight, 15, 16),
        "+" => (Bin::Add, 17, 18),
        "-" => (Bin::Subtract, 17, 18),
        "*" => (Bin::Multiply, 19, 20),
        "/" => (Bin::Divide, 19, 20),
        "%" => (Bin::Remainder, 19, 20),
        "**" => (Bin::Power, 25, 24),
        _ => return None,
    })
}
