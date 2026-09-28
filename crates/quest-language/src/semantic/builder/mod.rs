//! Typed Rust syntax construction followed by the same checked semantic admission.
//!
//! Classical category mismatches are rejected by Rust.
//! ```compile_fail
//! use quest_language::semantic::builder::Builder;
//! let mut b=Builder::new().unwrap();
//! let n=b.integer::<32>(0).unwrap(); let local=b.local("n",&n).unwrap();
//! let flag=b.boolean(true).unwrap(); b.assign(&local,&flag).unwrap();
//! ```
//! ```compile_fail
//! use quest_language::semantic::builder::Builder;
//! let mut b=Builder::new().unwrap(); let q=b.qubit("q",1).unwrap();
//! b.local("classical",&q).unwrap();
//! ```
//! ```compile_fail
//! use quest_language::semantic::builder::Builder;
//! let mut b=Builder::new().unwrap(); let n=b.integer::<32>(1).unwrap();
//! b.while_loop(&n,|_|Ok(())).unwrap();
//! ```
mod expressions;
mod shared;
mod types;
use super::{CompileLimits, ErrorKind, SemanticError, TypedModule};
use crate::{
    GateKind,
    ssa::ProgramId,
    syntax::{self, Expression, ExpressionKind as E, Statement, StatementKind as S},
};
use shared::SharedExpression;
use std::marker::PhantomData;
pub use types::{Angle, Arithmetic, Bit, Bool, Classical, Float, Int, Numeric, Uint};
/// An owned expression carrying a checked builder identity and classical category.
#[derive(Debug, Clone)]
pub struct Expr<T: Classical> {
    owner: ProgramId,
    expression: SharedExpression,
    limits: CompileLimits,
    marker: PhantomData<T>,
}
/// A scoped classical storage handle, distinct from a value expression.
#[derive(Debug, Clone)]
pub struct Local<T: Classical> {
    owner: ProgramId,
    name: String,
    marker: PhantomData<T>,
}
/// A quantum reference cannot be used as a classical value.
#[derive(Debug, Clone)]
pub struct Qubit {
    owner: ProgramId,
    expression: Expression,
}
#[derive(Debug)]
pub struct Builder {
    owner: ProgramId,
    statements: Vec<Statement>,
    depth: usize,
    next_symbol: usize,
    expression_limits: CompileLimits,
}
const fn expression(kind: E) -> Expression {
    Expression { kind, span: None }
}
fn foreign() -> SemanticError {
    SemanticError::new(
        ErrorKind::InvalidIr,
        "handle belongs to a different typed builder",
    )
}
impl Builder {
    /// # Errors
    /// Reports exhaustion of program identities.
    pub fn new() -> Result<Self, SemanticError> {
        Self::with_limits(CompileLimits::default())
    }
    /// Bound each shared expression before construction or syntax materialization.
    /// `finish` separately admits the complete program under its supplied limits.
    /// Expression depth is additionally bounded to 256 to keep recursive drop safe.
    /// # Errors
    /// Reports exhaustion of program identities.
    pub fn with_limits(expression_limits: CompileLimits) -> Result<Self, SemanticError> {
        Ok(Self {
            owner: ProgramId::fresh()?,
            statements: Vec::new(),
            depth: 0,
            next_symbol: 0,
            expression_limits,
        })
    }
    fn check(&self, owner: ProgramId) -> Result<(), SemanticError> {
        if self.owner == owner {
            Ok(())
        } else {
            Err(foreign())
        }
    }
    fn symbol(&mut self, name: &str) -> Result<String, SemanticError> {
        if name.is_empty()
            || !name.chars().enumerate().all(|(index, c)| {
                c == '_'
                    || if index == 0 {
                        c.is_alphabetic()
                    } else {
                        c.is_alphanumeric()
                    }
            })
        {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "builder symbol requires an identifier",
            ));
        }
        let index = self.next_symbol;
        self.next_symbol = self
            .next_symbol
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("builder symbol count overflow"))?;
        Ok(format!("{name}__quest_{index}"))
    }
    fn push(&mut self, kind: S) {
        self.statements.push(Statement { kind, span: None });
    }
    /// Declare initialized mutable local storage.
    /// # Errors
    /// Rejects invalid scalar widths or a foreign expression handle.
    pub fn local<T: Classical>(
        &mut self,
        name: &str,
        initial: &Expr<T>,
    ) -> Result<Local<T>, SemanticError> {
        self.check(initial.owner)?;
        let name = self.symbol(name)?;
        self.push(S::Declare {
            name: name.clone(),
            ty: T::syntax_type()?,
            initializer: Some(initial.expression.materialize(self.expression_limits)?),
            qualifier: syntax::Qualifier::Local,
        });
        Ok(Local {
            owner: self.owner,
            name,
            marker: PhantomData,
        })
    }
    /// # Errors
    /// Rejects a storage handle from another builder.
    pub fn read<T: Classical>(&self, local: &Local<T>) -> Result<Expr<T>, SemanticError> {
        self.check(local.owner)?;
        Ok(self.wrap(SharedExpression::leaf(
            expression(E::Name(local.name.clone())),
            self.expression_limits,
        )?))
    }
    /// # Errors
    /// Rejects foreign expression or storage handles.
    pub fn assign<T: Classical>(
        &mut self,
        local: &Local<T>,
        value: &Expr<T>,
    ) -> Result<(), SemanticError> {
        self.check(local.owner)?;
        self.check(value.owner)?;
        self.push(S::Assign {
            target: expression(E::Name(local.name.clone())),
            operator: None,
            value: value.expression.materialize(self.expression_limits)?,
        });
        Ok(())
    }
    /// # Errors
    /// Rejects a foreign condition or an error returned by either body closure.
    pub fn if_else(
        &mut self,
        condition: &Expr<Bool>,
        then_body: impl FnOnce(&mut Self) -> Result<(), SemanticError>,
        else_body: impl FnOnce(&mut Self) -> Result<(), SemanticError>,
    ) -> Result<(), SemanticError> {
        self.check(condition.owner)?;
        let then_body = self.body(then_body)?;
        let else_body = self.body(else_body)?;
        self.push(S::If {
            condition: condition.expression.materialize(self.expression_limits)?,
            then_body,
            else_body,
        });
        Ok(())
    }
    /// # Errors
    /// Rejects a foreign condition or an error returned by the body closure.
    pub fn while_loop(
        &mut self,
        condition: &Expr<Bool>,
        body: impl FnOnce(&mut Self) -> Result<(), SemanticError>,
    ) -> Result<(), SemanticError> {
        self.check(condition.owner)?;
        let body = self.body(body)?;
        self.push(S::While {
            condition: condition.expression.materialize(self.expression_limits)?,
            body,
        });
        Ok(())
    }
    fn body(
        &mut self,
        body: impl FnOnce(&mut Self) -> Result<(), SemanticError>,
    ) -> Result<Vec<Statement>, SemanticError> {
        let outer = std::mem::take(&mut self.statements);
        self.depth = self
            .depth
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("builder scope depth overflow"))?;
        let result = body(self);
        self.depth = self.depth.saturating_sub(1);
        let inner = std::mem::replace(&mut self.statements, outer);
        result.map(|()| inner)
    }
    /// # Errors
    /// Rejects empty quantum registers.
    pub fn qubit(&mut self, name: &str, count: usize) -> Result<Qubit, SemanticError> {
        if self.depth != 0 {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "quantum declarations require global module scope",
            ));
        }
        if count == 0 {
            return Err(SemanticError::new(
                ErrorKind::Type,
                "qubit register must be nonempty",
            ));
        }
        let name = self.symbol(name)?;
        self.push(S::Qubit {
            name: name.clone(),
            size: Some(expression(E::Number(count.to_string()))),
        });
        Ok(Qubit {
            owner: self.owner,
            expression: expression(E::Name(name)),
        })
    }
    /// # Errors
    /// Rejects foreign quantum or index handles. Bounds are checked by execution.
    pub fn index<const W: u8>(
        &self,
        qubit: &Qubit,
        index: &Expr<Int<W>>,
    ) -> Result<Qubit, SemanticError> {
        self.check(qubit.owner)?;
        self.check(index.owner)?;
        Ok(Qubit {
            owner: self.owner,
            expression: expression(E::Index(
                Box::new(qubit.expression.clone()),
                Box::new(index.expression.materialize(self.expression_limits)?),
            )),
        })
    }
    /// Emit a registry gate occurrence with typed dynamic parameters.
    /// # Errors
    /// Rejects foreign handles; semantic admission checks signature, widths and aliases.
    pub fn gate(
        &mut self,
        gate: GateKind,
        arguments: &[Expr<Float<64>>],
        operands: &[Qubit],
    ) -> Result<(), SemanticError> {
        for argument in arguments {
            self.check(argument.owner)?;
        }
        for operand in operands {
            self.check(operand.owner)?;
        }
        self.push(S::Gate {
            name: gate.definition().name.into(),
            arguments: arguments
                .iter()
                .map(|value| value.expression.materialize(self.expression_limits))
                .collect::<Result<_, _>>()?,
            operands: operands
                .iter()
                .map(|value| value.expression.clone())
                .collect(),
            modifiers: Vec::new(),
        });
        Ok(())
    }
    /// Consume the builder through shared semantic admission and resource limits.
    /// # Errors
    /// Rejects scope, type, control flow, alias, resource, or executable invariant violations.
    pub fn finish(self, limits: CompileLimits) -> Result<TypedModule, SemanticError> {
        super::admit(
            syntax::Module {
                statements: self.statements,
            },
            limits,
        )
    }
    const fn wrap<T: Classical>(&self, expression: SharedExpression) -> Expr<T> {
        Expr {
            owner: self.owner,
            expression,
            limits: self.expression_limits,
            marker: PhantomData,
        }
    }
}
