//! Optional plain-text rendering of owned diagnostic snapshots.
use crate::{Diagnostic, LabelStyle, Severity, SourceError};
use codespan_reporting::{diagnostic as backend, files::SimpleFiles, term};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
	#[error(transparent)]
	Source(#[from] SourceError),
	#[error(transparent)]
	Backend(#[from] codespan_reporting::files::Error),
}

/// Render labels, expansion traces, notes and suggestions using owned text.
///
/// # Errors
/// Rejects invalid or missing source spans and reports backend rendering errors.
pub fn render_plain(diagnostic: &Diagnostic) -> Result<String, RenderError> {
	diagnostic.validate_sources()?;
	let mut files = SimpleFiles::new();
	let mut identifiers = BTreeMap::new();
	for source in diagnostic.sources.iter() {
		identifiers.insert(source.id(), files.add(source.name(), source.text()));
	}
	let file_id = |span: crate::SourceSpan| {
		identifiers
			.get(&span.source())
			.copied()
			.ok_or(SourceError::UnknownSource)
	};
	let mut labels = Vec::new();
	for label in &diagnostic.labels {
		let style = match label.style {
			LabelStyle::Primary => backend::LabelStyle::Primary,
			LabelStyle::Secondary => backend::LabelStyle::Secondary,
		};
		labels.push(
			backend::Label::new(style, file_id(label.span)?, label.span.range())
				.with_message(&label.message),
		);
	}
	for frame in &diagnostic.provenance.trace {
		labels.push(
			backend::Label::secondary(file_id(frame.span)?, frame.span.range())
				.with_message(format!("{:?}: {}", frame.kind, frame.message)),
		);
	}
	let mut notes = diagnostic.notes.clone();
	for suggestion in &diagnostic.suggestions {
		notes.push(format!("help: {}", suggestion.message));
		if let Some(replacement) = &suggestion.replacement {
			labels.push(
				backend::Label::secondary(file_id(replacement.span)?, replacement.span.range())
					.with_message(format!("replace with {:?}", replacement.text)),
			);
		}
	}
	let severity = match diagnostic.severity {
		Severity::Error => backend::Severity::Error,
		Severity::Warning => backend::Severity::Warning,
		Severity::Note => backend::Severity::Note,
	};
	let output = backend::Diagnostic::new(severity)
		.with_code(diagnostic.code().as_str())
		.with_message(&diagnostic.message)
		.with_labels(labels)
		.with_notes(notes);
	Ok(term::emit_into_string(
		&term::Config::default(),
		&files,
		&output,
	)?)
}
