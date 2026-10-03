//! Consuming compiler stages for the shared structured language profile.
use quest_language::{
	SourceId, SourceMap, SourceSnapshot,
	classical::ScalarValue,
	semantic::{self, CompileLimits, TypedModule},
	ssa, syntax,
};
use std::sync::Arc;
mod artifact;
mod specialize;
pub use artifact::{
	AngleData, ArtifactError, ArtifactLimits, CompilationEvidence, EvidenceScope,
	FiniteSourceEvidence, HistoryEvidence, OccurrenceEvidence,
};
pub use specialize::{InputBinding, InputSpecialization};

/// One executable program family; transitions consume the preceding stage.
#[derive(Debug, Clone)]
pub struct Program<S> {
	state: S,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum LanguageError {
	#[error(transparent)]
	Circuit(Arc<crate::Error>),
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
impl From<crate::Error> for LanguageError {
	fn from(error: crate::Error) -> Self {
		Self::Circuit(Arc::new(error))
	}
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
			Self::Circuit(error) => language_failure(
				stage,
				sources,
				quest_language::LanguageFailureKind::RuntimeValue,
				error.to_string(),
			),
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
pub struct Constructed {
	typed: TypedModule,
	origin: Option<Arc<crate::BoundRegion>>,
	embedded_origins: Vec<Arc<crate::BoundRegion>>,
	artifact_sources: Vec<FiniteSourceEvidence>,
	artifact_publishers: Vec<ssa::SnapshotId>,
	compilation_evidence: Vec<CompilationEvidence>,
	specializations: Vec<InputSpecialization>,
	captures: Arc<[ScalarValue]>,
	exact_captures: Arc<std::collections::BTreeMap<usize, crate::Angle>>,
	payloads: Arc<std::collections::BTreeMap<usize, crate::QuantumPayload>>,
	oracles: Arc<std::collections::BTreeMap<usize, crate::OracleFragment>>,
	sources: SourceMap,
	locations: Arc<[MacroLocation]>,
}
/// Compiler coordinates retained when the enclosing Rust source text is unavailable.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MacroLocation {
	pub span: quest_language::SourceSpan,
	pub file: String,
	pub line: usize,
	pub column: usize,
}
impl Program<Constructed> {
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
			state: Constructed {
				typed,
				origin: None,
				embedded_origins: Vec::new(),
				artifact_sources: Vec::new(),
				artifact_publishers: Vec::new(),
				compilation_evidence: Vec::new(),
				specializations: Vec::new(),
				captures: Arc::from([]),
				exact_captures: Arc::default(),
				payloads: Arc::default(),
				oracles: Arc::default(),
				sources,
				locations: Arc::from([]),
			},
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
			state: Constructed {
				typed,
				origin: None,
				embedded_origins: Vec::new(),
				artifact_sources: Vec::new(),
				artifact_publishers: Vec::new(),
				compilation_evidence: Vec::new(),
				specializations: Vec::new(),
				captures: captures.into(),
				exact_captures: Arc::default(),
				payloads: Arc::default(),
				oracles: Arc::default(),
				sources,
				locations: locations.into(),
			},
		})
	}
	#[must_use]
	pub fn from_template(
		typed: TypedModule,
		captures: Vec<ScalarValue>,
		sources: SourceMap,
		locations: Vec<MacroLocation>,
	) -> Self {
		Self {
			state: Constructed {
				typed,
				origin: None,
				embedded_origins: Vec::new(),
				artifact_sources: Vec::new(),
				artifact_publishers: Vec::new(),
				compilation_evidence: Vec::new(),
				specializations: Vec::new(),
				captures: captures.into(),
				exact_captures: Arc::default(),
				payloads: Arc::default(),
				oracles: Arc::default(),
				sources,
				locations: locations.into(),
			},
		}
	}
	#[must_use]
	pub fn from_typed(typed: TypedModule, sources: SourceMap) -> Self {
		Self {
			state: Constructed {
				typed,
				origin: None,
				embedded_origins: Vec::new(),
				artifact_sources: Vec::new(),
				artifact_publishers: Vec::new(),
				compilation_evidence: Vec::new(),
				specializations: Vec::new(),
				captures: Arc::from([]),
				exact_captures: Arc::default(),
				payloads: Arc::default(),
				oracles: Arc::default(),
				sources,
				locations: Arc::from([]),
			},
		}
	}
	/// Attach explicit mathematical capture evidence without reinterpreting QASM floats.
	#[must_use]
	pub fn with_angle_captures(
		mut self,
		captures: std::collections::BTreeMap<usize, crate::Angle>,
	) -> Self {
		self.state.exact_captures = Arc::new(captures);
		self
	}
	/// Attach numerical effect payloads; verification checks every referenced interface.
	#[must_use]
	pub(crate) fn with_quantum_payloads(
		mut self,
		payloads: std::collections::BTreeMap<usize, crate::QuantumPayload>,
	) -> Self {
		self.state.payloads = Arc::new(payloads);
		self
	}
	/// Bind immutable oracle payloads to language-local capture identities.
	/// # Errors
	/// Rejects missing captures. The consuming `verify` stage checks declared arity
	/// against the bound payload before publishing an executable program.
	pub fn with_oracles(
		mut self,
		oracles: std::collections::BTreeMap<usize, crate::OracleFragment>,
	) -> Result<Self, LanguageError> {
		for statement in &self.state.typed.syntax().statements {
			if let syntax::StatementKind::Oracle { capture, .. } = statement.kind
				&& !oracles.contains_key(&capture)
			{
				return Err(LanguageError::Capture { index: capture });
			}
		}
		self.state.oracles = Arc::new(oracles);
		Ok(self)
	}
	#[must_use]
	pub const fn typed(&self) -> &TypedModule {
		&self.state.typed
	}
	#[must_use]
	pub fn locations(&self) -> &[MacroLocation] {
		&self.state.locations
	}
	/// Publish a verified executable SSA graph after checking bound captures.
	///
	/// # Errors
	/// Rejects malformed SSA and missing or incompatible capture values.
	#[expect(
		clippy::too_many_lines,
		reason = "Validate captures, evidence and SSA as one publication transaction"
	)]
	pub fn verify(self) -> Result<Program<Verified>, LanguageError> {
		let Constructed {
			typed,
			origin,
			embedded_origins,
			artifact_sources,
			artifact_publishers,
			compilation_evidence,
			specializations,
			captures,
			exact_captures,
			payloads,
			oracles,
			sources,
			locations,
		} = self.state;
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
		for (index, angle) in exact_captures.iter() {
			let expected = angle.evaluate(&std::collections::BTreeMap::new())?;
			let scalar = captures
				.get(*index)
				.ok_or(LanguageError::Capture { index: *index })?;
			let value = scalar.to_f64()?;
			if value.to_bits() != expected.to_bits() {
				return Err(LanguageError::Capture { index: *index });
			}
		}
		for instruction in program
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
		{
			if let ssa::InstructionKind::Payload {
				capture, places, ..
			} = &instruction.kind
			{
				let payload = payloads
					.get(capture)
					.ok_or(LanguageError::Capture { index: *capture })?;
				if payload.num_wires() != places.len() {
					return Err(LanguageError::Capture { index: *capture });
				}
			}
		}
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
		Ok(Program {
			state: Verified {
				origin_snapshot: program.snapshot(),
				program,
				origin,
				embedded_origins,
				artifact_sources,
				artifact_publishers,
				compilation_evidence,
				specializations,
				syntax,
				captures,
				exact_captures,
				payloads,
				oracles,
				sources,
				locations,
				retained_ir,
			},
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
pub struct Verified {
	program: ssa::VerifiedProgram,
	origin_snapshot: ssa::SnapshotId,
	origin: Option<Arc<crate::BoundRegion>>,
	embedded_origins: Vec<Arc<crate::BoundRegion>>,
	artifact_sources: Vec<FiniteSourceEvidence>,
	artifact_publishers: Vec<ssa::SnapshotId>,
	compilation_evidence: Vec<CompilationEvidence>,
	specializations: Vec<InputSpecialization>,
	syntax: Arc<syntax::Module>,
	captures: Arc<[ScalarValue]>,
	exact_captures: Arc<std::collections::BTreeMap<usize, crate::Angle>>,
	payloads: Arc<std::collections::BTreeMap<usize, crate::QuantumPayload>>,
	oracles: Arc<std::collections::BTreeMap<usize, crate::OracleFragment>>,
	sources: SourceMap,
	locations: Arc<[MacroLocation]>,
	retained_ir: usize,
}
impl Program<Verified> {
	/// Retained syntax, SSA, captures, oracle bodies and source metadata.
	/// Shared Arc allocations are counted once for this publication.
	/// # Errors
	/// Rejects accounting overflow.
	fn extra_storage(&self) -> Result<usize, LanguageError> {
		let mut storage = quest_language::quantum::RetainedStorage::default();
		let mut bytes = self
			.state
			.origin
			.as_ref()
			.map(|region| region.retained_bytes_with(&mut storage))
			.transpose()?
			.unwrap_or(0);
		bytes = bytes
			.checked_add(
				artifact::evidence_storage(&(
					&self.state.artifact_sources,
					&self.state.artifact_publishers,
					&self.state.compilation_evidence,
					&self.state.specializations,
				))
				.map_err(|_| LanguageError::Budget("artifact evidence storage"))?,
			)
			.ok_or(LanguageError::Budget("artifact evidence storage"))?;
		for region in &self.state.embedded_origins {
			bytes = bytes
				.checked_add(region.retained_bytes_with(&mut storage)?)
				.ok_or(LanguageError::Budget("embedded provenance storage"))?;
		}
		for angle in self.state.exact_captures.values() {
			bytes = bytes
				.checked_add(angle.retained_bytes()?)
				.and_then(|n| n.checked_add(64))
				.ok_or(LanguageError::Budget("exact capture storage"))?;
		}
		for payload in self.state.payloads.values() {
			bytes = bytes
				.checked_add(
					payload
						.retained_bytes_with(&mut storage)
						.ok_or(LanguageError::Budget("payload storage"))?,
				)
				.and_then(|n| n.checked_add(64))
				.ok_or(LanguageError::Budget("payload storage"))?;
		}
		for oracle in self.state.oracles.values() {
			bytes = bytes
				.checked_add(storage.oracle(oracle)?)
				.ok_or(LanguageError::Budget("oracle storage"))?;
		}
		Ok(bytes)
	}
	/// # Errors
	/// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
	pub fn retained_bytes(&self) -> Result<usize, LanguageError> {
		let mut bytes = self
			.state
			.retained_ir
			.checked_add(self.extra_storage()?)
			.and_then(|n| n.checked_add(source_storage(&self.state.sources).ok()?))
			.and_then(|n| n.checked_add(std::mem::size_of_val(self.state.captures.as_ref())))
			.and_then(|n| n.checked_add(std::mem::size_of_val(self.state.locations.as_ref())))
			.and_then(|n| n.checked_add(std::mem::size_of::<Self>()))
			.ok_or(LanguageError::Budget("structured retained storage"))?;
		// Bodies can be shared across bank keys, but every BTreeMap entry and
		// its node storage remains live. Triple the pair size to cover sparse
		// nodes/capacity and add pointer/header room per entry.
		let bank_pair = std::mem::size_of::<(usize, crate::OracleFragment)>();
		let bank_entry = bank_pair
			.checked_mul(3)
			.and_then(|n| n.checked_add(std::mem::size_of::<[usize; 8]>()))
			.ok_or(LanguageError::Budget("structured oracle bank"))?;
		let bank_bytes = self
			.state
			.oracles
			.len()
			.checked_mul(bank_entry)
			.and_then(|n| {
				n.checked_add(std::mem::size_of::<
					std::collections::BTreeMap<usize, crate::OracleFragment>,
				>())
			})
			.and_then(|n| n.checked_add(std::mem::size_of::<[usize; 2]>()))
			.ok_or(LanguageError::Budget("structured oracle bank"))?;
		bytes = bytes
			.checked_add(bank_bytes)
			.ok_or(LanguageError::Budget("structured retained storage"))?;
		for location in self.state.locations.iter() {
			bytes = bytes
				.checked_add(location.file.capacity())
				.ok_or(LanguageError::Budget("structured source locations"))?;
		}
		Ok(bytes)
	}
	pub(crate) fn transform_ssa<R, E>(
		mut self,
		transform: impl FnOnce(ssa::VerifiedProgram) -> Result<(ssa::VerifiedProgram, R), E>,
	) -> Result<(Self, R), E>
	where
		E: From<LanguageError>,
	{
		let old_bytes = self
			.state
			.program
			.retained_bytes()
			.map_err(LanguageError::from)?;
		let (program, report) = transform(self.state.program)?;
		let new_bytes = program.retained_bytes().map_err(LanguageError::from)?;
		self.state.retained_ir = self
			.state
			.retained_ir
			.checked_sub(old_bytes)
			.and_then(|bytes| bytes.checked_add(new_bytes))
			.ok_or_else(|| {
				LanguageError::Diagnostic(Box::new(
					LanguageError::Budget("optimized IR accounting").into_diagnostic(
						quest_language::Stage::Optimization,
						self.state.sources.clone(),
					),
				))
			})?;
		self.state.program = program;
		Ok((self, report))
	}
	/// Optimize executable classical SSA while retaining original structured export semantics.
	/// # Errors
	/// Rejects analysis budgets or failed independent verification.
	pub fn optimize_classical(
		mut self,
		limits: crate::classical::OptimizationLimits,
	) -> Result<(Self, crate::classical::OptimizationReport), LanguageError> {
		let old_ssa = self.state.program.retained_bytes().map_err(|error| {
			LanguageError::Diagnostic(Box::new(error.into_diagnostic(
				quest_language::Stage::Optimization,
				self.state.sources.clone(),
			)))
		})?;
		let sources = self.state.sources.clone();
		let (program, report) =
			crate::classical::optimize(self.state.program, limits).map_err(|error| {
				LanguageError::Diagnostic(Box::new(
					error.into_diagnostic(quest_language::Stage::Optimization, sources),
				))
			})?;
		self.state.retained_ir = self
			.state
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
					LanguageError::Budget("optimized IR accounting").into_diagnostic(
						quest_language::Stage::Optimization,
						self.state.sources.clone(),
					),
				))
			})?;
		self.state.program = program;
		Ok((self, report))
	}
	#[must_use]
	pub const fn ssa(&self) -> &ssa::VerifiedProgram {
		&self.state.program
	}
	#[must_use]
	pub fn exact_captures(&self) -> &std::collections::BTreeMap<usize, crate::Angle> {
		&self.state.exact_captures
	}
	pub(crate) fn captures(&self) -> &[ScalarValue] {
		&self.state.captures
	}
	pub(crate) fn oracle_bank(&self) -> &std::collections::BTreeMap<usize, crate::OracleFragment> {
		&self.state.oracles
	}
	/// Publish already reverified SSA and its matching bank in one transaction.
	pub(crate) fn publish_oracles(
		self,
		program: ssa::VerifiedProgram,
		oracles: std::collections::BTreeMap<usize, crate::OracleFragment>,
		max_bytes: usize,
	) -> Result<Self, LanguageError> {
		for region in program.regions() {
			if let Some(id) = &region.oracle {
				let fragment = oracles
					.get(&id.index())
					.ok_or(LanguageError::Unsupported("missing transformed oracle"))?;
				if fragment.num_qubits() != region.parameters.len() {
					return Err(LanguageError::Unsupported("transformed oracle arity"));
				}
			}
		}
		let (mut result, ()) = self.transform_ssa(|_| Ok::<_, LanguageError>((program, ())))?;
		result.state.oracles = Arc::new(oracles);
		if result.retained_bytes()? > max_bytes {
			return Err(LanguageError::Budget(
				"combined transformed SSA and oracle bank",
			));
		}
		Ok(result)
	}
	/// Admit native representability of every quantum interface.
	///
	/// # Errors
	/// Rejects qubit count overflow or an interface beyond the native index type.
	pub fn lower(self) -> Result<Program<Lowered>, LanguageError> {
		let sources = self.state.sources.clone();
		let result: Result<Program<Lowered>, LanguageError> = (|| {
			let qubits = self
				.state
				.program
				.slots()
				.iter()
				.filter(|slot| slot.region == self.state.program.program().entry)
				.try_fold(0usize, |total, slot| match slot.ty {
					ssa::Type::Qubit(count) => total
						.checked_add(count)
						.ok_or(LanguageError::Budget("qubit count")),
					_ => Ok(total),
				})?;
			i32::try_from(qubits).map_err(|_| LanguageError::Unsupported("qubit indices"))?;
			Ok(Program {
				state: Lowered {
					verified: self,
					qubits,
				},
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
pub struct Lowered {
	verified: Program<Verified>,
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
pub struct Executable {
	verified: Program<Verified>,
	qubits: usize,
	resources: StructuredResources,
	schedules: Vec<BlockSchedule>,
	dispatch: quest_language::vm::PreparedDispatch,
}
impl Program<Lowered> {
	/// Validate scheduling and account for source, IR and classical storage.
	///
	/// # Errors
	/// Rejects accounting overflow, budget exhaustion or allocation failure.
	pub fn plan(self) -> Result<Program<Executable>, LanguageError> {
		let sources = self.state.verified.state.sources.clone();
		self.plan_inner().map_err(|error| {
			LanguageError::Diagnostic(Box::new(
				error.into_diagnostic(quest_language::Stage::Preparation, sources),
			))
		})
	}

	#[expect(
		clippy::too_many_lines,
		reason = "Account IR, sources, schedules and reusable dispatch together"
	)]
	fn plan_inner(self) -> Result<Program<Executable>, LanguageError> {
		let limits = CompileLimits::default();
		let source_bytes = source_storage(&self.state.verified.state.sources)?;
		let classical_bytes =
			self.state
				.verified
				.state
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
		let extra_storage = self.state.verified.extra_storage()?;
		let mut ir_bytes = self
			.state
			.verified
			.state
			.retained_ir
			.checked_add(extra_storage)
			.and_then(|n| {
				n.checked_add(std::mem::size_of_val(
					self.state.verified.state.captures.as_ref(),
				))
			})
			.and_then(|n| {
				n.checked_add(std::mem::size_of_val(
					self.state.verified.state.locations.as_ref(),
				))
			})
			.ok_or(LanguageError::Budget("frontend storage"))?;
		for location in self.state.verified.state.locations.iter() {
			ir_bytes = ir_bytes
				.checked_add(location.file.capacity())
				.ok_or(LanguageError::Budget("source locations"))?;
		}
		let schedule_bytes = self.state.verified.state.program.blocks().iter().try_fold(
			0usize,
			|total, block| {
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
			},
		)?;
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
			.try_reserve(self.state.verified.state.program.blocks().len())
			.map_err(|_| LanguageError::Budget("schedule allocation"))?;
		for block in self.state.verified.state.program.blocks() {
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
		let dispatch = quest_language::vm::prepare_dispatch(
			&self.state.verified.state.program,
			&self.state.verified.state.captures,
			quest_language::vm::InterpreterLimits::default(),
		)
		.map_err(|error| {
			LanguageError::Diagnostic(Box::new(language_failure(
				quest_language::Stage::Preparation,
				self.state.verified.state.sources.clone(),
				quest_language::LanguageFailureKind::RuntimeValue,
				error.to_string(),
			)))
		})?;
		ir_bytes = ir_bytes
			.checked_add(dispatch.retained_bytes())
			.filter(|bytes| *bytes <= limits.storage_bytes)
			.ok_or(LanguageError::Budget("prepared dispatch storage"))?;
		Ok(Program {
			state: Executable {
				qubits: self.state.qubits,
				verified: self.state.verified,
				resources: StructuredResources {
					classical_bytes,
					ir_bytes,
					source_bytes,
				},
				schedules,
				dispatch,
			},
		})
	}
}
impl Program<Executable> {
	#[must_use]
	pub const fn num_qubits(&self) -> usize {
		self.state.qubits
	}
	#[must_use]
	pub const fn ssa(&self) -> &ssa::VerifiedProgram {
		&self.state.verified.state.program
	}
	#[must_use]
	pub fn syntax(&self) -> &syntax::Module {
		&self.state.verified.state.syntax
	}
	#[must_use]
	pub fn exact_captures(&self) -> &std::collections::BTreeMap<usize, crate::Angle> {
		&self.state.verified.state.exact_captures
	}
	#[must_use]
	pub fn quantum_payloads(&self) -> &std::collections::BTreeMap<usize, crate::QuantumPayload> {
		&self.state.verified.state.payloads
	}
	#[must_use]
	pub fn oracle_captures(&self) -> &std::collections::BTreeMap<usize, crate::OracleFragment> {
		&self.state.verified.state.oracles
	}
	#[must_use]
	pub fn captures(&self) -> &[ScalarValue] {
		&self.state.verified.state.captures
	}
	#[must_use]
	pub const fn sources(&self) -> &SourceMap {
		&self.state.verified.state.sources
	}
	#[must_use]
	pub fn locations(&self) -> &[MacroLocation] {
		&self.state.verified.state.locations
	}
	#[must_use]
	pub const fn resources(&self) -> StructuredResources {
		self.state.resources
	}
	#[must_use]
	pub const fn dispatch(&self) -> &quest_language::vm::PreparedDispatch {
		&self.state.dispatch
	}
	#[must_use]
	pub fn schedules(&self) -> &[BlockSchedule] {
		&self.state.schedules
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

impl From<quest_language::angle::Error> for LanguageError {
	fn from(error: quest_language::angle::Error) -> Self {
		Self::from(crate::Error::from(error))
	}
}

impl From<quest_language::matrix::Error> for LanguageError {
	fn from(error: quest_language::matrix::Error) -> Self {
		Self::from(crate::Error::from(error))
	}
}

impl Program<Constructed> {
	pub(crate) fn with_embedded_origins(mut self, regions: Vec<Arc<crate::BoundRegion>>) -> Self {
		self.state.embedded_origins = regions;
		self
	}
	pub(crate) fn with_origin(mut self, region: crate::BoundRegion) -> Self {
		self.state.origin = Some(Arc::new(region));
		self
	}
}
impl Program<Executable> {
	/// Source capabilities retained solely for provenance and exact binding history.
	#[must_use]
	pub fn embedded_regions(&self) -> &[Arc<crate::BoundRegion>] {
		&self.state.verified.state.embedded_origins
	}
	pub(crate) fn current_finite_origin(&self) -> Option<&crate::BoundRegion> {
		(self.ssa().snapshot() == self.state.verified.state.origin_snapshot)
			.then(|| self.finite_origin())
			.flatten()
	}
	/// Retained finite source, including binding obligations, occurrence spans and provenance.
	#[must_use]
	pub fn finite_origin(&self) -> Option<&crate::BoundRegion> {
		self.state.verified.state.origin.as_deref()
	}
}

impl Program<Verified> {
	#[cfg(any(feature = "workers", feature = "synthesis"))]
	pub(crate) fn retain_compilation_evidence(
		mut self,
		evidence: Vec<CompilationEvidence>,
	) -> Self {
		self.state.compilation_evidence.extend(evidence);
		self
	}
}

#[cfg(test)]
mod optimizer_accounting_tests {
	use super::*;
	use googletest::{Result, prelude::*};

	#[gtest]
	fn repeated_shared_oracle_bank_entries_are_charged_separately() -> Result<()> {
		let mut verified = Program::<Constructed>::parse("qubit q; h q;", "bank.qasm")?.verify()?;
		let before = verified.retained_bytes()?;
		let mut builder = crate::QuantumRegionBuilder::new(1, 0)?;
		builder.gate(crate::Gate::H, &[builder.qubit(0)?], &[])?;
		let fragment = crate::OracleFragment::from_program(
			builder.finish()?.bind(&[])?,
			0.0,
			crate::MatrixPolicy::default(),
		)?;
		let body = crate::OracleFragment::shared_storage_bytes([&fragment])?;
		let mut bank = std::collections::BTreeMap::new();
		for key in 0..64 {
			bank.insert(key, fragment.clone());
		}
		verified.state.oracles = Arc::new(bank);
		let added = verified
			.retained_bytes()?
			.checked_sub(before)
			.ok_or(crate::Error::Budget("oracle bank accounting"))?;
		let entry_payload = std::mem::size_of::<(usize, crate::OracleFragment)>()
			.checked_mul(64)
			.ok_or(crate::Error::Budget("oracle bank accounting"))?;
		expect_true!(
			added
				> body
					.checked_add(entry_payload)
					.ok_or(crate::Error::Budget("oracle bank accounting"))?
		);
		Ok(())
	}
}
