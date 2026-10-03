use crate::{ExportLimits, ImportLimits, IncludeEdge, IncludeResolver, ParsedModule, Result};
use quest_language::{
	SourceMap, SourceSnapshot, Stage, TraceFrame, TraceKind,
	semantic::{CompileLimits, SemanticError, TypedModule},
};

/// Admitted structured syntax with the original include authority and snapshots.
#[derive(Debug, Clone)]
pub struct ImportedModule {
	root: quest_language::syntax::Module,
	sources: SourceMap,
	includes: Vec<IncludeEdge>,
	typed: TypedModule,
}
impl ImportedModule {
	#[must_use]
	pub const fn typed(&self) -> &TypedModule {
		&self.typed
	}
	#[must_use]
	pub const fn sources(&self) -> &SourceMap {
		&self.sources
	}
	#[must_use]
	pub fn includes(&self) -> &[IncludeEdge] {
		&self.includes
	}
	#[must_use]
	pub const fn syntax(&self) -> &quest_language::syntax::Module {
		&self.root
	}
	#[must_use]
	pub fn into_typed(self) -> TypedModule {
		self.typed
	}
}
impl ParsedModule {
	/// Admit resolved syntax while preserving the original source authority.
	///
	/// # Errors
	/// Returns an owned diagnostic for invalid types, effects, control flow or budgets.
	pub fn admit(self, limits: CompileLimits) -> Result<ImportedModule> {
		let Self {
			root,
			expanded,
			sources,
			includes,
		} = self;
		let expanded = crate::registry::expand(expanded, &sources)?;
		let typed = quest_language::semantic::admit(expanded, limits)
			.map_err(|error| semantic_error(error, &sources, &includes))?;
		Ok(ImportedModule {
			root,
			sources,
			includes,
			typed,
		})
	}
}
fn semantic_error(
	error: SemanticError,
	sources: &SourceMap,
	includes: &[IncludeEdge],
) -> Box<quest_language::Diagnostic> {
	let mut diagnostic = error.into_diagnostic(Stage::Admission, sources.clone());
	if let Some(span) = diagnostic.occurrence {
		let mut current = span.source();
		// Expansion rejects cycles; this bound also protects future graph changes.
		for _ in 0..includes.len() {
			let Some(edge) = includes.iter().find(|edge| edge.target == current) else {
				break;
			};
			diagnostic.provenance.trace.push(TraceFrame {
				kind: TraceKind::Include,
				span: edge.span,
				message: edge.path.clone(),
			});
			current = edge.span.source();
		}
		diagnostic.provenance.trace.reverse();
	}
	Box::new(diagnostic)
}
/// Parse explicit sources, expand includes and admit the resulting structured module.
///
/// # Errors
/// Returns owned syntax, include, resource or semantic diagnostics.
pub fn import(
	source: SourceSnapshot,
	resolver: &mut impl IncludeResolver,
	limits: ImportLimits,
	compile_limits: CompileLimits,
) -> Result<ImportedModule> {
	crate::parse(source, resolver, limits)?.admit(compile_limits)
}
/// Export the canonical root while retaining its explicit include directives.
///
/// Reimport requires the retained snapshots through the caller's resolver.
///
/// # Errors
/// Rejects output budgets and syntax with no text representation.
pub fn export(module: &ImportedModule, limits: ExportLimits) -> Result<String> {
	crate::export_syntax(module.syntax(), limits).map_err(|mut error| {
		error.sources = module.sources().clone();
		error
	})
}
/// Export an admitted structured module, including expanded user definitions.
///
/// Registry standard gates remain implicit, as they are in semantic admission.
///
/// # Errors
/// Rejects output budgets and syntax with no text representation.
pub fn export_typed(module: &TypedModule, limits: ExportLimits) -> Result<String> {
	crate::export_syntax(module.syntax(), limits)
}

/// Admit already-expanded frontend syntax using the pinned standard-library authority.
///
/// # Errors
/// Rejects untrusted registry overrides, unresolved includes, invalid semantics and budgets.
pub fn admit_expanded(
	module: quest_language::syntax::Module,
	sources: &SourceMap,
	limits: CompileLimits,
) -> Result<TypedModule> {
	let module = crate::registry::expand(module, sources)?;
	quest_language::semantic::admit(module, limits)
		.map_err(|error| semantic_error(error, sources, &[]))
}
