//! Consuming compiler stages for the shared structured language profile.
use quest_language::{
    SourceId, SourceMap, SourceSnapshot,
    classical::ScalarValue,
    semantic::{self, CompileLimits, TypedModule},
    ssa, syntax,
};
use std::sync::Arc;

#[derive(Debug, Clone, thiserror::Error)]
pub enum LanguageError {
    #[error(transparent)]
    Parse(#[from] syntax::ParseError),
    #[error(transparent)]
    Semantic(#[from] semantic::SemanticError),
    #[error(transparent)]
    Source(#[from] quest_language::SourceError),
    #[error(transparent)]
    Value(#[from] quest_language::classical::ValueError),
    #[error(transparent)]
    Diagnostic(#[from] Box<quest_language::Diagnostic>),
    #[error("capture {index} is missing or has an incompatible type")]
    Capture { index: usize },
    #[error("structured compilation budget exceeded: {0}")]
    Budget(&'static str),
    #[error("structured native lowering cannot represent {0}")]
    Unsupported(&'static str),
}
impl LanguageError {
    /// Borrow an already-owned frontend diagnostic, when this error carries one.
    #[must_use]
    pub fn diagnostic(&self) -> Option<&quest_language::Diagnostic> {
        if let Self::Diagnostic(diagnostic) = self {
            Some(diagnostic)
        } else {
            None
        }
    }

    /// Produce an owned diagnostic for callers that need a uniform reporting value.
    #[must_use]
    pub fn report(&self, stage: quest_language::Stage) -> quest_language::Diagnostic {
        self.clone()
            .into_diagnostic(stage, quest_language::SourceMap::default())
    }

    /// Convert every language-stage error into a self-contained diagnostic.
    #[must_use]
    pub fn into_diagnostic(
        self,
        stage: quest_language::Stage,
        sources: SourceMap,
    ) -> quest_language::Diagnostic {
        match self {
            Self::Parse(error) => error.into_diagnostic(stage, sources),
            Self::Semantic(error) => error.into_diagnostic(stage, sources),
            Self::Diagnostic(diagnostic) => *diagnostic,
            Self::Source(error) => {
                let mut diagnostic = quest_language::Diagnostic::new(
                    stage,
                    quest_language::DiagnosticCause::InvalidSource(error),
                    error.to_string(),
                );
                diagnostic.sources = sources;
                diagnostic
            }
            Self::Value(error) => language_failure(
                stage,
                sources,
                quest_language::LanguageFailureKind::RuntimeValue,
                error.to_string(),
            ),
            Self::Capture { index } => language_failure(
                stage,
                sources,
                quest_language::LanguageFailureKind::RuntimeCapture,
                format!("capture {index} is missing or has an incompatible type"),
            ),
            Self::Budget(resource) => {
                let mut diagnostic = quest_language::Diagnostic::new(
                    stage,
                    quest_language::DiagnosticCause::ResourceFailure {
                        reason: resource.into(),
                    },
                    format!("structured compilation budget exceeded: {resource}"),
                );
                diagnostic.sources = sources;
                diagnostic
            }
            Self::Unsupported(capability) => {
                let mut diagnostic = quest_language::Diagnostic::new(
                    stage,
                    quest_language::DiagnosticCause::UnsupportedCapability {
                        capability: capability.into(),
                    },
                    format!("structured native lowering cannot represent {capability}"),
                );
                diagnostic.sources = sources;
                diagnostic
            }
        }
    }
}
fn language_failure(
    stage: quest_language::Stage,
    sources: SourceMap,
    kind: quest_language::LanguageFailureKind,
    reason: String,
) -> quest_language::Diagnostic {
    let mut diagnostic = quest_language::Diagnostic::new(
        stage,
        quest_language::DiagnosticCause::LanguageFailure {
            kind,
            reason: reason.clone(),
        },
        reason,
    );
    diagnostic.sources = sources;
    diagnostic
}
/// Admitted structured source with captures evaluated during Rust construction.
#[derive(Debug, Clone)]
pub struct StructuredProgram {
    typed: TypedModule,
    captures: Arc<[ScalarValue]>,
    oracles: Arc<std::collections::BTreeMap<usize, crate::OracleFragment>>,
    sources: SourceMap,
    locations: Arc<[MacroLocation]>,
}
/// Compiler coordinates retained when the enclosing Rust source text is unavailable.
#[derive(Debug, Clone)]
pub struct MacroLocation {
    pub span: quest_language::SourceSpan,
    pub file: String,
    pub line: usize,
    pub column: usize,
}
impl StructuredProgram {
    /// Parse a self-contained source. Includes use the explicit `quest-qasm` resolver.
    ///
    /// # Errors
    /// Rejects syntax, semantic and source-budget violations.
    pub fn parse(source: &str, name: impl Into<String>) -> Result<Self, LanguageError> {
        if source.len() > syntax::ParseLimits::default().source_bytes {
            return Err(LanguageError::Budget("source bytes"));
        }
        let snapshot = SourceSnapshot::new(SourceId::new(1), name, source);
        let mut sources = SourceMap::default();
        sources.insert(snapshot.clone())?;
        let module = syntax::parse_source(&snapshot).map_err(|error| {
            LanguageError::Diagnostic(Box::new(
                error.into_diagnostic(quest_language::Stage::Parsing, sources.clone()),
            ))
        })?;
        let typed = semantic::admit(module, CompileLimits::default()).map_err(|error| {
            LanguageError::Diagnostic(Box::new(
                error.into_diagnostic(quest_language::Stage::Admission, sources.clone()),
            ))
        })?;
        Ok(Self {
            typed,
            captures: Arc::from([]),
            oracles: Arc::default(),
            sources,
            locations: Arc::from([]),
        })
    }
    /// Construct from shared tokens emitted by the Rust frontend.
    ///
    /// # Errors
    /// Rejects invalid tokens, types, captures or resource limits.
    pub fn from_tokens(
        tokens: &[syntax::Token],
        captures: Vec<ScalarValue>,
    ) -> Result<Self, LanguageError> {
        Self::from_frontend(tokens, captures, SourceMap::default(), Vec::new())
    }
    /// Admit shared frontend tokens and their immutable source metadata.
    ///
    /// # Errors
    /// Rejects invalid grammar, semantics and source or capture budgets.
    pub fn from_frontend(
        tokens: &[syntax::Token],
        captures: Vec<ScalarValue>,
        sources: SourceMap,
        locations: Vec<MacroLocation>,
    ) -> Result<Self, LanguageError> {
        let module =
            syntax::parse_tokens(tokens, syntax::ParseLimits::default()).map_err(|error| {
                let diagnostic =
                    error.into_diagnostic(quest_language::Stage::Parsing, sources.clone());
                LanguageError::Diagnostic(Box::new(attach_macro_location(diagnostic, &locations)))
            })?;
        let typed = quest_qasm::admit_expanded(module, &sources, CompileLimits::default())
            .map_err(|diagnostic| {
                LanguageError::Diagnostic(Box::new(attach_macro_location(*diagnostic, &locations)))
            })?;
        Ok(Self {
            typed,
            captures: captures.into(),
            oracles: Arc::default(),
            sources,
            locations: locations.into(),
        })
    }
    #[must_use]
    pub fn from_typed(typed: TypedModule, sources: SourceMap) -> Self {
        Self {
            typed,
            captures: Arc::from([]),
            oracles: Arc::default(),
            sources,
            locations: Arc::from([]),
        }
    }
    /// Bind immutable oracle payloads to language-local capture identities.
    /// # Errors
    /// Rejects missing captures. The consuming `verify` stage checks declared arity
    /// against the bound payload before publishing an executable program.
    pub fn with_oracles(
        mut self,
        oracles: std::collections::BTreeMap<usize, crate::OracleFragment>,
    ) -> Result<Self, LanguageError> {
        for statement in &self.typed.syntax().statements {
            if let syntax::StatementKind::Oracle { capture, .. } = statement.kind
                && !oracles.contains_key(&capture)
            {
                return Err(LanguageError::Capture { index: capture });
            }
        }
        self.oracles = Arc::new(oracles);
        Ok(self)
    }
    #[must_use]
    pub const fn typed(&self) -> &TypedModule {
        &self.typed
    }
    #[must_use]
    pub fn locations(&self) -> &[MacroLocation] {
        &self.locations
    }
    /// Publish a verified executable SSA graph after checking bound captures.
    ///
    /// # Errors
    /// Rejects malformed SSA and missing or incompatible capture values.
    pub fn verify(self) -> Result<VerifiedStructuredProgram, LanguageError> {
        let Self {
            typed,
            captures,
            oracles,
            sources,
            locations,
        } = self;
        let retained_ir = typed.retained_bytes().map_err(|error| {
            LanguageError::Diagnostic(Box::new(
                error.into_diagnostic(quest_language::Stage::Verification, sources.clone()),
            ))
        })?;
        if retained_ir > CompileLimits::default().storage_bytes {
            return Err(LanguageError::Diagnostic(Box::new(
                LanguageError::Budget("retained semantic IR")
                    .into_diagnostic(quest_language::Stage::Verification, sources),
            )));
        }
        let (syntax, program) = typed.into_verified_parts().map_err(|error| {
            LanguageError::Diagnostic(Box::new(
                error.into_diagnostic(quest_language::Stage::Verification, sources.clone()),
            ))
        })?;
        for region in &program.program().regions {
            if let Some(id) = &region.oracle {
                let fragment = oracles
                    .get(&id.index())
                    .ok_or_else(|| LanguageError::Capture { index: id.index() })?;
                if fragment.num_qubits() != region.parameters.len() {
                    return Err(LanguageError::Capture { index: id.index() });
                }
            }
        }
        let syntax = Arc::new(syntax);
        for instruction in program
            .blocks()
            .iter()
            .flat_map(|block| &block.instructions)
        {
            if let ssa::InstructionKind::Capture { index, ty } = &instruction.kind {
                if oracles.contains_key(index) {
                    return Err(LanguageError::Capture { index: *index });
                }
                let capture = captures.get(*index).ok_or_else(|| {
                    let mut diagnostic = LanguageError::Capture { index: *index }
                        .into_diagnostic(quest_language::Stage::Verification, sources.clone());
                    diagnostic.occurrence = instruction.span;
                    LanguageError::Diagnostic(Box::new(attach_macro_location(
                        diagnostic, &locations,
                    )))
                })?;
                if ty != &ssa::Type::Scalar(capture.ty()) {
                    let mut diagnostic = LanguageError::Capture { index: *index }
                        .into_diagnostic(quest_language::Stage::Verification, sources.clone());
                    diagnostic.occurrence = instruction.span;
                    return Err(LanguageError::Diagnostic(Box::new(attach_macro_location(
                        diagnostic, &locations,
                    ))));
                }
            }
        }
        Ok(VerifiedStructuredProgram {
            program,
            syntax,
            captures,
            oracles,
            sources,
            locations,
            retained_ir,
        })
    }
}
fn attach_macro_location(
    mut diagnostic: quest_language::Diagnostic,
    locations: &[MacroLocation],
) -> quest_language::Diagnostic {
    if diagnostic.labels.is_empty()
        && let Some(span) = diagnostic.occurrence
    {
        if diagnostic.sources.slice(span).is_ok() {
            diagnostic.labels.push(quest_language::Label {
                span,
                style: quest_language::LabelStyle::Primary,
                message: diagnostic.message.clone(),
            });
        } else if let Some(location) = locations.iter().find(|location| location.span == span) {
            diagnostic.notes.push(format!(
                "Rust source location: {}:{}:{}",
                location.file, location.line, location.column
            ));
        }
    }
    diagnostic
}
/// SSA has independent ownership, dominance, signature and effect verification.
#[derive(Debug, Clone)]
pub struct VerifiedStructuredProgram {
    program: ssa::VerifiedProgram,
    syntax: Arc<syntax::Module>,
    captures: Arc<[ScalarValue]>,
    oracles: Arc<std::collections::BTreeMap<usize, crate::OracleFragment>>,
    sources: SourceMap,
    locations: Arc<[MacroLocation]>,
    retained_ir: usize,
}
impl VerifiedStructuredProgram {
    pub(crate) fn transform_ssa<R, E>(
        mut self,
        transform: impl FnOnce(ssa::VerifiedProgram) -> Result<(ssa::VerifiedProgram, R), E>,
    ) -> Result<(Self, R), E>
    where
        E: From<LanguageError>,
    {
        let old_bytes = self.program.retained_bytes().map_err(LanguageError::from)?;
        let (program, report) = transform(self.program)?;
        let new_bytes = program.retained_bytes().map_err(LanguageError::from)?;
        self.retained_ir = self
            .retained_ir
            .checked_sub(old_bytes)
            .and_then(|bytes| bytes.checked_add(new_bytes))
            .ok_or_else(|| {
                LanguageError::Diagnostic(Box::new(
                    LanguageError::Budget("optimized IR accounting")
                        .into_diagnostic(quest_language::Stage::Optimization, self.sources.clone()),
                ))
            })?;
        self.program = program;
        Ok((self, report))
    }
    /// Optimize executable classical SSA while retaining original structured export semantics.
    /// # Errors
    /// Rejects analysis budgets or failed independent verification.
    pub fn optimize_classical(
        mut self,
        limits: ssa::optimization::OptimizationLimits,
    ) -> Result<(Self, ssa::optimization::OptimizationReport), LanguageError> {
        let old_ssa = self.program.retained_bytes().map_err(|error| {
            LanguageError::Diagnostic(Box::new(
                error.into_diagnostic(quest_language::Stage::Optimization, self.sources.clone()),
            ))
        })?;
        let sources = self.sources.clone();
        let (program, report) =
            ssa::optimization::optimize(self.program, limits).map_err(|error| {
                LanguageError::Diagnostic(Box::new(
                    error.into_diagnostic(quest_language::Stage::Optimization, sources),
                ))
            })?;
        self.retained_ir = self
            .retained_ir
            .checked_sub(old_ssa)
            .and_then(|bytes| {
                program
                    .retained_bytes()
                    .ok()
                    .and_then(|ssa| bytes.checked_add(ssa))
            })
            .ok_or_else(|| {
                LanguageError::Diagnostic(Box::new(
                    LanguageError::Budget("optimized IR accounting")
                        .into_diagnostic(quest_language::Stage::Optimization, self.sources.clone()),
                ))
            })?;
        self.program = program;
        Ok((self, report))
    }
    #[must_use]
    pub const fn ssa(&self) -> &ssa::VerifiedProgram {
        &self.program
    }
    #[cfg(feature = "workers")]
    pub(crate) fn captures(&self) -> &[ScalarValue] {
        &self.captures
    }
    /// Admit native representability of every quantum interface.
    ///
    /// # Errors
    /// Rejects qubit count overflow or an interface beyond the native index type.
    pub fn lower(self) -> Result<LoweredStructuredProgram, LanguageError> {
        let sources = self.sources.clone();
        let result: Result<LoweredStructuredProgram, LanguageError> = (|| {
            let qubits = self
                .program
                .slots()
                .iter()
                .filter(|slot| slot.region == self.program.program().entry)
                .try_fold(0usize, |total, slot| match slot.ty {
                    ssa::Type::Qubit(count) => total
                        .checked_add(count)
                        .ok_or(LanguageError::Budget("qubit count")),
                    _ => Ok(total),
                })?;
            i32::try_from(qubits).map_err(|_| LanguageError::Unsupported("qubit indices"))?;
            Ok(LoweredStructuredProgram {
                verified: self,
                qubits,
            })
        })();
        result.map_err(|error| {
            LanguageError::Diagnostic(Box::new(
                error.into_diagnostic(quest_language::Stage::Lowering, sources),
            ))
        })
    }
}
/// Every admitted SSA instruction has a supported interpreter lowering.
#[derive(Debug, Clone)]
pub struct LoweredStructuredProgram {
    verified: VerifiedStructuredProgram,
    qubits: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructuredResources {
    pub classical_bytes: usize,
    pub ir_bytes: usize,
    pub source_bytes: usize,
}
/// A block's quantum/effect occurrences remain distinct and ordered.
#[derive(Debug, Clone)]
pub struct BlockSchedule {
    pub block: ssa::BlockId,
    pub occurrences: Vec<usize>,
    pub dependencies: Vec<(usize, usize)>,
}
#[derive(Debug, Clone)]
pub struct StructuredPlan {
    verified: VerifiedStructuredProgram,
    qubits: usize,
    resources: StructuredResources,
    schedules: Vec<BlockSchedule>,
}
impl LoweredStructuredProgram {
    /// Validate scheduling and account for source, IR and classical storage.
    ///
    /// # Errors
    /// Rejects accounting overflow, budget exhaustion or allocation failure.
    pub fn plan(self) -> Result<StructuredPlan, LanguageError> {
        let sources = self.verified.sources.clone();
        self.plan_inner().map_err(|error| {
            LanguageError::Diagnostic(Box::new(
                error.into_diagnostic(quest_language::Stage::Preparation, sources),
            ))
        })
    }

    fn plan_inner(self) -> Result<StructuredPlan, LanguageError> {
        let limits = CompileLimits::default();
        let source_bytes = source_storage(&self.verified.sources)?;
        let classical_bytes =
            self.verified
                .program
                .slots()
                .iter()
                .try_fold(0usize, |total, slot| {
                    total
                        .checked_add(storage_size(&slot.ty)?)
                        .ok_or(LanguageError::Budget("classical storage"))
                })?;
        if classical_bytes > limits.storage_bytes {
            return Err(LanguageError::Budget("classical storage"));
        }
        let mut ir_bytes = self
            .verified
            .retained_ir
            .checked_add(std::mem::size_of_val(self.verified.captures.as_ref()))
            .and_then(|n| n.checked_add(std::mem::size_of_val(self.verified.locations.as_ref())))
            .ok_or(LanguageError::Budget("frontend storage"))?;
        ir_bytes = ir_bytes
            .checked_add(
                crate::OracleFragment::shared_storage_bytes(self.verified.oracles.values())
                    .map_err(|_| LanguageError::Budget("oracle storage"))?,
            )
            .ok_or(LanguageError::Budget("oracle storage"))?;
        for location in self.verified.locations.iter() {
            ir_bytes = ir_bytes
                .checked_add(location.file.capacity())
                .ok_or(LanguageError::Budget("source locations"))?;
        }
        let schedule_bytes =
            self.verified
                .program
                .blocks()
                .iter()
                .try_fold(0usize, |total, block| {
                    block
                        .instructions
                        .iter()
                        .try_fold(total, |total, instruction| {
                            instruction
                                .accesses
                                .len()
                                .checked_add(1)
                                .and_then(|n| n.checked_mul(64))
                                .and_then(|n| n.checked_add(128))
                                .and_then(|n| total.checked_add(n))
                                .ok_or(LanguageError::Budget("quantum schedule"))
                        })
                })?;
        ir_bytes = ir_bytes
            .checked_add(schedule_bytes)
            .ok_or(LanguageError::Budget("quantum schedules"))?;
        if ir_bytes > limits.storage_bytes
            || source_bytes > syntax::ParseLimits::default().source_bytes
        {
            return Err(LanguageError::Budget("source and IR storage"));
        }
        let mut schedules = Vec::new();
        schedules
            .try_reserve(self.verified.program.blocks().len())
            .map_err(|_| LanguageError::Budget("schedule allocation"))?;
        for block in self.verified.program.blocks() {
            let dag = block.quantum_dag();
            let occurrences = dag.nodes.iter().map(|node| node.instruction).collect();
            let dependencies = dag
                .nodes
                .iter()
                .flat_map(|node| {
                    node.predecessors
                        .iter()
                        .map(move |predecessor| (*predecessor, node.instruction))
                })
                .collect();
            schedules.push(BlockSchedule {
                block: block.id,
                occurrences,
                dependencies,
            });
        }
        if ir_bytes > limits.storage_bytes {
            return Err(LanguageError::Budget("IR storage"));
        }
        Ok(StructuredPlan {
            qubits: self.qubits,
            verified: self.verified,
            resources: StructuredResources {
                classical_bytes,
                ir_bytes,
                source_bytes,
            },
            schedules,
        })
    }
}
impl StructuredPlan {
    #[must_use]
    pub const fn num_qubits(&self) -> usize {
        self.qubits
    }
    #[must_use]
    pub const fn ssa(&self) -> &ssa::VerifiedProgram {
        &self.verified.program
    }
    #[must_use]
    pub fn syntax(&self) -> &syntax::Module {
        &self.verified.syntax
    }
    #[must_use]
    pub fn oracle_captures(&self) -> &std::collections::BTreeMap<usize, crate::OracleFragment> {
        &self.verified.oracles
    }
    #[must_use]
    pub fn captures(&self) -> &[ScalarValue] {
        &self.verified.captures
    }
    #[must_use]
    pub const fn sources(&self) -> &SourceMap {
        &self.verified.sources
    }
    #[must_use]
    pub fn locations(&self) -> &[MacroLocation] {
        &self.verified.locations
    }
    #[must_use]
    pub const fn resources(&self) -> StructuredResources {
        self.resources
    }
    #[must_use]
    pub fn schedules(&self) -> &[BlockSchedule] {
        &self.schedules
    }
}
fn storage_size(ty: &ssa::Type) -> Result<usize, LanguageError> {
    match ty {
        ssa::Type::Scalar(scalar) => Ok(scalar.storage_bytes()),
        ssa::Type::Array {
            element,
            dimensions,
        } => dimensions
            .iter()
            .try_fold(element.storage_bytes(), |total, dimension| {
                total
                    .checked_mul(*dimension)
                    .ok_or(LanguageError::Budget("array storage"))
            }),
        ssa::Type::Qubit(count) => count
            .checked_mul(std::mem::size_of::<usize>())
            .ok_or(LanguageError::Budget("qubit storage")),
        ssa::Type::Memory | ssa::Type::Void => {
            Err(LanguageError::Unsupported("nonstorage slot type"))
        }
    }
}

fn source_storage(sources: &SourceMap) -> Result<usize, LanguageError> {
    sources.iter().try_fold(0usize, |total, source| {
        total
            .checked_add(source.text().len())
            .and_then(|n| n.checked_add(source.name().len()))
            .and_then(|n| n.checked_add(1024))
            .ok_or(LanguageError::Budget("source accounting"))
    })
}
