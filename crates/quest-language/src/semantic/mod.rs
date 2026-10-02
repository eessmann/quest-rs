//! Scoped semantic admission and typed lowering into executable SSA.
mod allocation;
pub mod builder;
#[doc(hidden)]
pub mod cfg;
mod compile;
mod expressions;
pub mod finite;
mod preflight;
#[doc(hidden)]
pub mod promote;
pub(crate) mod retained;
mod statements;
#[cfg(feature = "templates")]
pub mod template;
mod types;
use crate::{SourceSpan, ssa, syntax};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CompileLimits {
    pub nodes: usize,
    pub blocks: usize,
    pub slots: usize,
    pub storage_bytes: usize,
    pub qubits: usize,
    pub call_depth: usize,
}
impl Default for CompileLimits {
    fn default() -> Self {
        Self {
            nodes: 1_000_000,
            blocks: 100_000,
            slots: 100_000,
            storage_bytes: 64 * 1024 * 1024,
            qubits: 1_000_000,
            call_depth: 64,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Type,
    UnknownSymbol,
    DuplicateSymbol,
    DefiniteAssignment,
    ControlFlow,
    Alias,
    Capability,
    Resource,
    InvalidIr,
    Numerical,
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct SemanticError {
    pub kind: ErrorKind,
    pub span: Option<SourceSpan>,
    pub message: String,
    pub resource: Option<crate::ResourceUsage>,
    pub overflow_resource: Option<crate::ResourceKind>,
}
impl SemanticError {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            span: None,
            message: message.into(),
            resource: None,
            overflow_resource: None,
        }
    }
    #[doc(hidden)]
    #[must_use]
    pub fn budget(message: &str) -> Self {
        Self::new(ErrorKind::Resource, message)
    }
    #[doc(hidden)]
    #[must_use]
    pub fn limit(
        resource: crate::ResourceKind,
        requested: usize,
        limit: usize,
        message: &str,
    ) -> Self {
        let mut error = Self::new(ErrorKind::Resource, message);
        match (u64::try_from(requested), u64::try_from(limit)) {
            (Ok(requested), Ok(limit)) => {
                error.resource = Some(crate::ResourceUsage {
                    resource,
                    requested,
                    limit,
                });
            }
            _ => {
                error.overflow_resource = Some(resource);
            }
        }
        error
    }
    pub(crate) const fn at(mut self, span: Option<SourceSpan>) -> Self {
        if self.span.is_none() {
            self.span = span;
        }
        self
    }
    #[doc(hidden)]
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidIr, message)
    }
    /// Convert this semantic failure into a self-contained diagnostic snapshot.
    #[must_use]
    pub fn into_diagnostic(
        self,
        stage: crate::Stage,
        sources: crate::SourceMap,
    ) -> crate::Diagnostic {
        let cause = match self.kind {
            ErrorKind::UnknownSymbol => crate::DiagnosticCause::LanguageFailure {
                kind: crate::LanguageFailureKind::UnknownSymbol,
                reason: self.message.clone(),
            },
            ErrorKind::Numerical => crate::DiagnosticCause::NumericalFailure {
                reason: self.message.clone(),
            },
            ErrorKind::Type => crate::DiagnosticCause::LanguageFailure {
                kind: crate::LanguageFailureKind::Type,
                reason: self.message.clone(),
            },
            ErrorKind::DuplicateSymbol => crate::DiagnosticCause::LanguageFailure {
                kind: crate::LanguageFailureKind::DuplicateSymbol,
                reason: self.message.clone(),
            },
            ErrorKind::DefiniteAssignment => crate::DiagnosticCause::LanguageFailure {
                kind: crate::LanguageFailureKind::DefiniteAssignment,
                reason: self.message.clone(),
            },
            ErrorKind::Alias => crate::DiagnosticCause::LanguageFailure {
                kind: crate::LanguageFailureKind::Alias,
                reason: self.message.clone(),
            },
            ErrorKind::InvalidIr => crate::DiagnosticCause::LanguageFailure {
                kind: crate::LanguageFailureKind::InvalidIr,
                reason: self.message.clone(),
            },
            ErrorKind::ControlFlow => crate::DiagnosticCause::InvalidControlFlow {
                reason: self.message.clone(),
            },
            ErrorKind::Capability => crate::DiagnosticCause::UnsupportedCapability {
                capability: self.message.clone(),
            },
            ErrorKind::Resource => self
                .resource
                .map(crate::DiagnosticCause::ResourceLimit)
                .or_else(|| {
                    self.overflow_resource
                        .map(crate::DiagnosticCause::ResourceOverflow)
                })
                .unwrap_or_else(|| crate::DiagnosticCause::ResourceFailure {
                    reason: self.message.clone(),
                }),
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
}
/// Admitted structured syntax plus a typed candidate; construction is private.
///
/// ```compile_fail
/// use quest_language::{semantic::{TypedModule, CompileLimits}, ssa::Program, syntax::Module};
/// fn forge(syntax: Module, program: Program) -> TypedModule {
///     TypedModule { syntax, program, limits: CompileLimits::default() }
/// }
/// ```
///
/// Admission is consumed when transferring to verified execution.
///
/// ```compile_fail
/// use quest_language::semantic::TypedModule;
/// fn reuse(module: TypedModule) {
///     let _ = module.into_ssa();
///     let _ = module.syntax();
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TypedModule {
    syntax: syntax::Module,
    program: ssa::VerifiedProgram,
}
impl TypedModule {
    /// Admit a compiled executable separately from its immutable source-export syntax.
    /// Source is independently admitted for well-formedness; executable SSA is independently
    /// verified and retained verbatim, since optimization may change it.
    /// # Errors
    /// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
    pub fn from_compiled_parts(
        source: syntax::Module,
        program: ssa::Program,
        limits: CompileLimits,
    ) -> Result<Self, SemanticError> {
        let source = admit(source, limits)?.syntax;
        let program = program.verify(limits)?;
        let module = Self {
            syntax: source,
            program,
        };
        if module.retained_bytes()? > limits.storage_bytes {
            return Err(SemanticError::budget("compiled artifact storage"));
        }
        Ok(module)
    }

    #[must_use]
    pub const fn syntax(&self) -> &syntax::Module {
        &self.syntax
    }
    /// Owned bytes including nested allocation capacities; excludes allocator overhead and source snapshots.
    ///
    /// # Errors
    /// Returns a resource error if byte accounting overflows.
    pub fn retained_bytes(&self) -> Result<usize, SemanticError> {
        use retained::Heap as _;
        retained::sum([
            std::mem::size_of::<Self>(),
            self.syntax.heap()?,
            self.program.program().heap()?,
        ])
    }
    /// Move the export authority and verified executable representation without cloning.
    ///
    /// # Errors
    /// This transfer is infallible; admission has already verified this immutable SSA.
    pub fn into_verified_parts(
        self,
    ) -> Result<(syntax::Module, ssa::VerifiedProgram), SemanticError> {
        Ok((self.syntax, self.program))
    }
    /// Consume admission and transfer the already verified executable SSA.
    ///
    /// # Errors
    /// This transfer is infallible; mutation requires leaving the verified representation.
    pub fn into_ssa(self) -> Result<ssa::VerifiedProgram, SemanticError> {
        Ok(self.program)
    }
}
/// Admit scoped, typed structured syntax and construct its executable representation.
///
/// # Errors
/// Rejects unsupported constructs, invalid types, invalid control flow and budget excesses.
pub fn admit(module: syntax::Module, limits: CompileLimits) -> Result<TypedModule, SemanticError> {
    use retained::Heap as _;
    preflight::check(&module, limits)?;
    let syntax_bytes = retained::sum([std::mem::size_of::<syntax::Module>(), module.heap()?])?;
    if syntax_bytes > limits.storage_bytes {
        return Err(SemanticError::limit(
            crate::ResourceKind::StorageBytes,
            syntax_bytes,
            limits.storage_bytes,
            "retained syntax exceeds storage budget",
        ));
    }
    let program = compile::compile(&module, limits)?;
    // Admission itself must reject definite-assignment and verifier errors.
    let program = program.verify(limits)?;
    Ok(TypedModule {
        syntax: module,
        program,
    })
}
pub(crate) use types::{binary_type, place_type, storage_size};
