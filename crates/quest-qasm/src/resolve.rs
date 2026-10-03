use crate::Result;
use quest_language::{
	SourceId, SourceMap, SourceSnapshot, SourceSpan,
	syntax::{Module, ParseLimits},
};
#[derive(Debug, Clone, Copy)]
pub struct ImportLimits {
	pub parse: ParseLimits,
	pub include_count: usize,
	pub include_depth: usize,
	pub source_bytes: usize,
}
impl Default for ImportLimits {
	fn default() -> Self {
		Self {
			parse: ParseLimits::default(),
			include_count: 128,
			include_depth: 32,
			source_bytes: 8_388_608,
		}
	}
}
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct ResolveError {
	pub message: String,
}
impl ResolveError {
	pub fn new(message: impl Into<String>) -> Self {
		Self {
			message: message.into(),
		}
	}
}
/// Caller-controlled snapshot lookup; no filesystem access is performed by this crate.
pub trait IncludeResolver {
	/// Resolve one include relative to its immutable requesting source.
	///
	/// # Errors
	/// Return an owned failure when the requested snapshot is unavailable.
	fn resolve(
		&mut self,
		from: &SourceSnapshot,
		path: &str,
	) -> std::result::Result<SourceSnapshot, ResolveError>;
}
#[derive(Debug, Clone)]
pub struct IncludeEdge {
	pub span: SourceSpan,
	pub target: SourceId,
	pub path: String,
}
#[derive(Debug, Clone)]
pub struct ParsedModule {
	pub(crate) root: Module,
	pub(crate) expanded: Module,
	pub(crate) sources: SourceMap,
	pub(crate) includes: Vec<IncludeEdge>,
}
impl ParsedModule {
	#[must_use]
	pub const fn syntax(&self) -> &Module {
		&self.root
	}
	#[must_use]
	pub const fn expanded(&self) -> &Module {
		&self.expanded
	}
	#[must_use]
	pub const fn sources(&self) -> &SourceMap {
		&self.sources
	}
	#[must_use]
	pub fn includes(&self) -> &[IncludeEdge] {
		&self.includes
	}
}
/// Resolve includes from explicit immutable snapshots and retain the root syntax.
///
/// # Errors
/// Rejects malformed syntax, missing includes, cycles, identity conflicts and budgets.
#[expect(
	clippy::needless_pass_by_value,
	reason = "The frontend boundary transfers ownership of the caller snapshot into retained provenance"
)]
pub fn parse(
	source: SourceSnapshot,
	resolver: &mut impl IncludeResolver,
	limits: ImportLimits,
) -> Result<ParsedModule> {
	let mut loader = Loader {
		resolver,
		limits,
		sources: SourceMap::default(),
		includes: Vec::new(),
		active: Vec::new(),
		trace: Vec::new(),
		total_bytes: 0,
		include_count: 0,
	};
	let (root, expanded) = loader.expand(&source, 0)?;
	Ok(ParsedModule {
		root,
		expanded: Module {
			statements: expanded,
		},
		sources: loader.sources,
		includes: loader.includes,
	})
}
struct Loader<'a, R> {
	resolver: &'a mut R,
	limits: ImportLimits,
	sources: SourceMap,
	includes: Vec<IncludeEdge>,
	active: Vec<(SourceId, String)>,
	trace: Vec<quest_language::TraceFrame>,
	total_bytes: usize,
	include_count: usize,
}
impl<R: IncludeResolver> Loader<'_, R> {
	fn diagnostic(
		&self,
		cause: quest_language::DiagnosticCause,
		message: &str,
		span: Option<SourceSpan>,
	) -> Box<quest_language::Diagnostic> {
		let mut diagnostic = crate::failure(quest_language::Stage::Parsing, cause, message);
		diagnostic.sources = self.sources.clone();
		diagnostic.provenance.trace.clone_from(&self.trace);
		if let Some(span) = span {
			diagnostic.labels.push(quest_language::Label {
				span,
				style: quest_language::LabelStyle::Primary,
				message: message.into(),
			});
		}
		diagnostic
	}
	fn include_error(
		&self,
		path: &str,
		message: &str,
		span: Option<SourceSpan>,
	) -> Box<quest_language::Diagnostic> {
		self.diagnostic(
			quest_language::DiagnosticCause::IncludeFailure {
				include: path.into(),
				reason: message.into(),
			},
			message,
			span,
		)
	}
	fn budget(
		&self,
		resource: quest_language::ResourceKind,
		requested: usize,
		limit: usize,
		span: Option<SourceSpan>,
	) -> Box<quest_language::Diagnostic> {
		let cause = match (u64::try_from(requested), u64::try_from(limit)) {
			(Ok(requested), Ok(limit)) => {
				quest_language::DiagnosticCause::ResourceLimit(quest_language::ResourceUsage {
					resource,
					requested,
					limit,
				})
			}
			_ => quest_language::DiagnosticCause::ResourceOverflow(resource),
		};
		self.diagnostic(cause, "OpenQASM import resource limit exceeded", span)
	}
	fn overflow(
		&self,
		resource: quest_language::ResourceKind,
		span: Option<SourceSpan>,
	) -> Box<quest_language::Diagnostic> {
		self.diagnostic(
			quest_language::DiagnosticCause::ResourceOverflow(resource),
			"OpenQASM import resource arithmetic overflow",
			span,
		)
	}

	#[expect(
		clippy::too_many_lines,
		reason = "Keep source admission and recursive include accounting in one transaction"
	)]
	fn expand(
		&mut self,
		source: &SourceSnapshot,
		depth: usize,
	) -> Result<(Module, Vec<quest_language::syntax::Statement>)> {
		if self
			.active
			.iter()
			.any(|(id, name)| *id == source.id() || name == source.name())
		{
			return Err(self.include_error(
				source.name(),
				"include cycle detected",
				self.trace.last().map(|frame| frame.span),
			));
		}
		if let Some(previous) = self.sources.get(source.id()) {
			if previous != source {
				return Err(self.include_error(
					source.name(),
					"source identity reused for a different snapshot",
					self.trace.last().map(|frame| frame.span),
				));
			}
		} else {
			self.sources.insert(source.clone()).map_err(|error| {
				self.diagnostic(
					quest_language::DiagnosticCause::InvalidSource(error),
					"invalid source identity",
					None,
				)
			})?;
		}
		self.total_bytes = self
			.total_bytes
			.checked_add(source.text().len())
			.ok_or_else(|| self.overflow(quest_language::ResourceKind::SourceBytes, None))?;
		if self.total_bytes > self.limits.source_bytes {
			return Err(self.budget(
				quest_language::ResourceKind::SourceBytes,
				self.total_bytes,
				self.limits.source_bytes,
				None,
			));
		}
		let module = quest_language::syntax::lex(source, self.limits.parse)
			.and_then(|tokens| quest_language::syntax::parse_tokens(&tokens, self.limits.parse))
			.map_err(|error| {
				let mut diagnostic =
					error.into_diagnostic(quest_language::Stage::Parsing, self.sources.clone());
				diagnostic.provenance.trace.clone_from(&self.trace);
				Box::new(diagnostic)
			})?;
		self.active.push((source.id(), source.name().into()));
		let mut expanded = Vec::new();
		for statement in &module.statements {
			if let quest_language::syntax::StatementKind::Include(path) = &statement.kind {
				self.include_count = self.include_count.checked_add(1).ok_or_else(|| {
					self.overflow(quest_language::ResourceKind::IncludeCount, statement.span)
				})?;
				if self.include_count > self.limits.include_count {
					return Err(self.budget(
						quest_language::ResourceKind::IncludeCount,
						self.include_count,
						self.limits.include_count,
						statement.span,
					));
				}
				let next_depth = depth.checked_add(1).ok_or_else(|| {
					self.overflow(quest_language::ResourceKind::IncludeDepth, statement.span)
				})?;
				if next_depth > self.limits.include_depth {
					return Err(self.budget(
						quest_language::ResourceKind::IncludeDepth,
						next_depth,
						self.limits.include_depth,
						statement.span,
					));
				}
				let target = self
					.resolver
					.resolve(source, path)
					.map_err(|error| self.include_error(path, &error.message, statement.span))?;
				let span = statement.span.ok_or_else(|| {
					self.include_error(path, "included statement is missing its source span", None)
				})?;
				self.includes.push(IncludeEdge {
					span,
					target: target.id(),
					path: path.clone(),
				});
				self.trace.push(quest_language::TraceFrame {
					kind: quest_language::TraceKind::Include,
					span,
					message: path.clone(),
				});
				let (_, body) = self.expand(&target, next_depth)?;
				self.trace.pop();
				expanded.try_reserve(body.len()).map_err(|_| {
					self.include_error(path, "include expansion allocation failed", Some(span))
				})?;
				expanded.extend(body);
			} else {
				if let Some(span) = nested_include(statement) {
					return Err(self.include_error(
						"",
						"includes are only legal at global scope",
						Some(span),
					));
				}
				expanded.try_reserve(1).map_err(|_| {
					self.include_error(
						source.name(),
						"include expansion allocation failed",
						statement.span,
					)
				})?;
				expanded.push(statement.clone());
			}
		}
		self.active.pop();
		Ok((module, expanded))
	}
}
fn nested_include(statement: &quest_language::syntax::Statement) -> Option<SourceSpan> {
	use quest_language::syntax::StatementKind;
	match &statement.kind {
		StatementKind::Include(_) => statement.span,
		StatementKind::GateDeclaration { body, .. }
		| StatementKind::Subroutine { body, .. }
		| StatementKind::For { body, .. }
		| StatementKind::While { body, .. } => body.iter().find_map(nested_include),
		StatementKind::If {
			then_body,
			else_body,
			..
		} => then_body.iter().chain(else_body).find_map(nested_include),
		StatementKind::Switch { cases, default, .. } => cases
			.iter()
			.flat_map(|(_, body)| body)
			.chain(default)
			.find_map(nested_include),
		_ => None,
	}
}
/// Resolver that deliberately provides no include sources.
#[derive(Debug, Default)]
pub struct NoIncludes;
impl IncludeResolver for NoIncludes {
	fn resolve(
		&mut self,
		_: &SourceSnapshot,
		_: &str,
	) -> std::result::Result<SourceSnapshot, ResolveError> {
		Err(ResolveError::new("no include resolver supplied"))
	}
}
