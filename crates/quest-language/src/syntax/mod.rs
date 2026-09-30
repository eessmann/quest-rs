//! Shared spanned syntax for the text and Rust-token frontends.
mod lexer;
mod parser;
use crate::SourceSpan;
pub use lexer::lex;
pub use parser::{parse_source, parse_tokens};

/// Limits checked before parsing a source or expanding recursive syntax.
#[derive(Debug, Clone, Copy)]
pub struct ParseLimits {
    pub source_bytes: usize,
    pub tokens: usize,
    pub nesting: usize,
}
impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            source_bytes: 4_194_304,
            tokens: 1_000_000,
            nesting: 256,
        }
    }
}
/// The token adapter preserves literal spelling and compiler/source locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Option<SourceSpan>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Identifier(String),
    Number(String),
    String(String),
    Symbol(String),
    Capture(usize),
}
impl TokenKind {
    #[must_use]
    pub fn spelling(&self) -> &str {
        match self {
            Self::Identifier(s) | Self::Number(s) | Self::String(s) | Self::Symbol(s) => s,
            Self::Capture(_) => "${capture}",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ParseError {
    pub resource: Option<crate::ResourceUsage>,
    pub kind: ParseErrorKind,
    pub span: Option<SourceSpan>,
    pub message: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    Syntax,
    Capability,
    Budget,
}
impl ParseError {
    /// Convert this parser failure into a self-contained diagnostic snapshot.
    #[must_use]
    pub fn into_diagnostic(
        self,
        stage: crate::Stage,
        sources: crate::SourceMap,
    ) -> crate::Diagnostic {
        let cause = match self.kind {
            ParseErrorKind::Syntax => crate::DiagnosticCause::LanguageFailure {
                kind: crate::LanguageFailureKind::Syntax,
                reason: self.message.clone(),
            },
            ParseErrorKind::Capability => crate::DiagnosticCause::UnsupportedCapability {
                capability: self.message.clone(),
            },
            ParseErrorKind::Budget => self.resource.map_or_else(
                || crate::DiagnosticCause::ResourceFailure {
                    reason: self.message.clone(),
                },
                crate::DiagnosticCause::ResourceLimit,
            ),
        };
        let mut diagnostic = crate::Diagnostic::new(stage, cause, &self.message);
        diagnostic.occurrence = self.span;
        diagnostic.sources = sources;
        if let Some(span) = self
            .span
            .filter(|span| diagnostic.sources.slice(*span).is_ok())
        {
            diagnostic.labels.push(crate::Label {
                span,
                style: crate::LabelStyle::Primary,
                message: self.message,
            });
        }
        diagnostic
    }

    pub(super) fn syntax(span: Option<SourceSpan>, message: impl Into<String>) -> Self {
        Self {
            resource: None,
            kind: ParseErrorKind::Syntax,
            span,
            message: message.into(),
        }
    }
    pub(super) fn budget_limit(
        message: &str,
        resource: crate::ResourceKind,
        requested: usize,
        limit: usize,
    ) -> Self {
        let mut error = Self::budget(message);
        if let (Ok(requested), Ok(limit)) = (u64::try_from(requested), u64::try_from(limit)) {
            error.resource = Some(crate::ResourceUsage {
                resource,
                requested,
                limit,
            });
        }
        error
    }
    pub(super) fn budget(message: &str) -> Self {
        Self {
            resource: None,
            kind: ParseErrorKind::Budget,
            span: None,
            message: message.into(),
        }
    }
}
/// Structured syntax remains available after admission for canonical export.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Module {
    pub statements: Vec<Statement>,
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Option<SourceSpan>,
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StatementKind {
    Include(String),
    /// Rust payload identity and checked local arity; no captured data lives here.
    Oracle {
        name: String,
        arity: Expression,
        capture: usize,
    },
    Alias {
        name: String,
        value: Expression,
    },
    Qubit {
        name: String,
        size: Option<Expression>,
    },
    Declare {
        name: String,
        ty: Type,
        initializer: Option<Expression>,
        qualifier: Qualifier,
    },
    Assign {
        target: Expression,
        operator: Option<BinaryOperator>,
        value: Expression,
    },
    Gate {
        name: String,
        arguments: Vec<Expression>,
        operands: Vec<Expression>,
        modifiers: Vec<Modifier>,
    },
    GateDeclaration {
        name: String,
        parameters: Vec<String>,
        qubits: Vec<String>,
        body: Vec<Statement>,
    },
    Subroutine {
        name: String,
        parameters: Vec<Parameter>,
        result: Option<Type>,
        body: Vec<Statement>,
    },
    If {
        condition: Expression,
        then_body: Vec<Statement>,
        else_body: Vec<Statement>,
    },
    Switch {
        selector: Expression,
        cases: Vec<(Vec<Expression>, Vec<Statement>)>,
        default: Vec<Statement>,
    },
    For {
        name: String,
        ty: Type,
        iterable: Iterable,
        body: Vec<Statement>,
    },
    While {
        condition: Expression,
        body: Vec<Statement>,
    },
    /// Immutable channel bank identity; constructed by typed frontends, never inferred from QASM.
    Payload {
        capture: usize,
        operands: Vec<Expression>,
    },
    Reset(Expression),
    Barrier(Vec<Expression>),
    Expression(Expression),
    Return(Option<Expression>),
    Break,
    Continue,
    End,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Qualifier {
    Local,
    Const,
    Input,
    Output,
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Parameter {
    pub name: String,
    pub ty: Type,
    pub mutable: bool,
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Type {
    Scalar(ScalarKind, Option<Box<Expression>>),
    Qubit(Option<Box<Expression>>),
    Array {
        element: Box<Self>,
        dimensions: Vec<Expression>,
        reference: bool,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ScalarKind {
    Bool,
    Bit,
    Int,
    Uint,
    Angle,
    Float,
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Iterable {
    Range {
        start: Expression,
        step: Option<Expression>,
        end: Expression,
    },
    Set(Vec<Expression>),
    Expression(Expression),
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Modifier {
    Inverse,
    Adjoint,
    Control {
        positive: bool,
        count: Option<Expression>,
    },
    Power(Expression),
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Option<SourceSpan>,
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ExpressionKind {
    Number(String),
    BitString(String),
    Bool(bool),
    Name(String),
    Capture(usize),
    Unary(UnaryOperator, Box<Expression>),
    Binary(BinaryOperator, Box<Expression>, Box<Expression>),
    Index(Box<Expression>, Box<Expression>),
    Call(String, Vec<Expression>),
    Cast(Type, Box<Expression>),
    Measure(Box<Expression>),
    Array(Vec<Expression>),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum UnaryOperator {
    Negate,
    Not,
    Complement,
    Positive,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Power,
    ShiftLeft,
    ShiftRight,
    BitAnd,
    BitOr,
    BitXor,
    And,
    Or,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}
