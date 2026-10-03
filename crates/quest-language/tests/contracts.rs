use googletest::prelude::*;
use quest_language::{GateKind, SourceError, SourceId, SourceSnapshot};

#[gtest]
fn controlled_gate_signature_separates_controls_from_targets() -> Result<()> {
	let gate = GateKind::lookup("ccx")
		.ok_or_else(|| std::io::Error::other("missing ccx"))?
		.definition();
	verify_eq!(gate.target_count, 1)?;
	verify_eq!(gate.parameter_count, 0)?;
	verify_eq!(gate.intrinsic_controls, 2)?;
	verify_eq!(gate.rust_variant, "X")?;
	verify_eq!(GateKind::lookup("U"), GateKind::lookup("u"))?;
	verify_eq!(GateKind::lookup("unknown"), None)?;
	Ok(())
}

#[gtest]
fn source_spans_reject_reversal_bounds_and_split_utf8() -> Result<()> {
	let source = SourceSnapshot::new(SourceId::new(7), "same.qasm", "αx");
	verify_eq!(
		source.span(std::ops::Range { start: 2, end: 1 }),
		Err(SourceError::ReversedRange)
	)?;
	verify_eq!(source.span(0..4), Err(SourceError::OutOfBounds))?;
	verify_eq!(source.span(1..2), Err(SourceError::NotCharBoundary))?;
	verify_eq!(source.slice(source.span(0..2)?)?, "α")?;
	verify_eq!(source.slice(source.span(3..3)?)?, "")?;
	let other = SourceSnapshot::new(SourceId::new(8), "same.qasm", "αx");
	verify_eq!(
		other.slice(source.span(0..2)?),
		Err(SourceError::WrongSource)
	)?;
	Ok(())
}

#[gtest]
fn exact_u_decomposition_keeps_global_phase_and_execution_order() -> Result<()> {
	use quest_language::{Adjoint, Decomposition, ParameterTransform};
	let definition = GateKind::U.definition();
	verify_eq!(
		definition.adjoint,
		Adjoint::Parameters(&[
			ParameterTransform {
				input: 0,
				negate: true
			},
			ParameterTransform {
				input: 2,
				negate: true
			},
			ParameterTransform {
				input: 1,
				negate: true
			},
		])
	)?;
	let Decomposition::Sequence(steps) = definition.decomposition else {
		return Err(std::io::Error::other("U must expose its exact decomposition").into());
	};
	verify_eq!(
		steps.iter().map(|step| step.gate).collect::<Vec<_>>(),
		vec![
			GateKind::GlobalPhase,
			GateKind::Rz,
			GateKind::Ry,
			GateKind::Rz
		]
	)?;
	let phase = steps
		.first()
		.and_then(|step| step.parameters.first())
		.ok_or_else(|| std::io::Error::other("missing phase"))?;
	verify_eq!(
		phase
			.terms
			.iter()
			.map(|term| (term.input, term.numerator, term.denominator))
			.collect::<Vec<_>>(),
		vec![(0, 1, 2), (1, 1, 2), (2, 1, 2)]
	)?;
	Ok(())
}

#[gtest]
fn source_identity_cannot_replace_an_existing_snapshot() -> Result<()> {
	use quest_language::SourceMap;
	let mut map = SourceMap::default();
	let original = SourceSnapshot::new(SourceId::new(0), "input", "original");
	let span = original.span(0..8)?;
	map.insert(original)?;
	verify_eq!(
		map.insert(SourceSnapshot::new(SourceId::new(0), "input", "new")),
		Err(SourceError::DuplicateIdentity)
	)?;
	verify_eq!(map.slice(span)?, "original")?;
	Ok(())
}

#[gtest]
fn diagnostic_keeps_typed_budget_and_cross_source_expansion_context() -> Result<()> {
	use quest_language::{
		Diagnostic, DiagnosticCause, DiagnosticCode, Entity, Label, LabelStyle, Provenance,
		ResourceKind, ResourceUsage, Stage, TraceFrame, TraceKind,
	};
	let definition = SourceSnapshot::new(SourceId::new(1), "library", "gate example");
	let call = SourceSnapshot::new(SourceId::new(2), "main", "example q;");
	let mut diagnostic = Diagnostic::new(
		Stage::Admission,
		DiagnosticCause::ResourceLimit(ResourceUsage {
			resource: ResourceKind::SourceBytes,
			requested: 11,
			limit: 10,
		}),
		"source budget exceeded",
	);
	diagnostic.sources.insert(definition.clone())?;
	diagnostic.sources.insert(call.clone())?;
	diagnostic.labels.push(Label {
		span: call.span(0..7)?,
		style: LabelStyle::Primary,
		message: "expanded here".into(),
	});
	diagnostic.labels.push(Label {
		span: definition.span(5..12)?,
		style: LabelStyle::Secondary,
		message: "defined here".into(),
	});
	diagnostic.provenance = Provenance {
		entity: Some(Entity::Gate("example".into())),
		trace: vec![
			TraceFrame {
				kind: TraceKind::Definition,
				span: definition.span(5..12)?,
				message: "definition".into(),
			},
			TraceFrame {
				kind: TraceKind::Call,
				span: call.span(0..7)?,
				message: "call".into(),
			},
		],
		execution: Vec::new(),
	};
	verify_eq!(diagnostic.code(), DiagnosticCode::ResourceLimit)?;
	verify_eq!(diagnostic.code().as_str(), "QL0005")?;
	diagnostic.validate_sources()?;
	verify_eq!(diagnostic.provenance.trace.len(), 2)?;
	Ok(())
}

#[gtest]
fn source_errors_convert_without_inventing_structured_fields() -> Result<()> {
	use quest_language::{
		DiagnosticCause, LanguageFailureKind, ResourceKind, SourceMap, Stage,
		semantic::{ErrorKind, SemanticError},
		syntax::{ParseError, ParseErrorKind},
	};
	let source = SourceSnapshot::new(SourceId::new(73), "owned.qasm", "bad");
	let span = source.span(0..3)?;
	let mut sources = SourceMap::default();
	sources.insert(source)?;

	let parsed = ParseError {
		resource: None,
		kind: ParseErrorKind::Syntax,
		span: Some(span),
		message: "generic parser failure".into(),
	}
	.into_diagnostic(Stage::Parsing, sources.clone());
	verify_eq!(
		parsed.cause,
		DiagnosticCause::LanguageFailure {
			kind: LanguageFailureKind::Syntax,
			reason: "generic parser failure".into(),
		}
	)?;
	parsed.validate_sources()?;

	let semantic = SemanticError {
		kind: ErrorKind::Resource,
		span: Some(span),
		message: "allocation failed".into(),
		resource: None,
		overflow_resource: None,
	}
	.into_diagnostic(Stage::Admission, sources);
	verify_eq!(
		semantic.cause,
		DiagnosticCause::ResourceFailure {
			reason: "allocation failed".into(),
		}
	)?;
	verify_false!(matches!(
		semantic.cause,
		DiagnosticCause::ResourceOverflow(ResourceKind::CompileNodes)
	))?;
	semantic.validate_sources()?;
	Ok(())
}

#[cfg(feature = "codespan-reporting")]
#[gtest]
fn renderer_uses_snapshots_and_preserves_multiple_labels() -> Result<()> {
	use quest_language::{Diagnostic, DiagnosticCause, Label, LabelStyle, Stage};
	let source = SourceSnapshot::new(SourceId::new(5), "nonexistent.qasm", "x missing;\n");
	let mut diagnostic = Diagnostic::new(
		Stage::Admission,
		DiagnosticCause::UnknownSymbol {
			name: "missing".into(),
		},
		"unknown qubit",
	);
	diagnostic.labels.push(Label {
		span: source.span(2..9)?,
		style: LabelStyle::Primary,
		message: "not declared".into(),
	});
	diagnostic.labels.push(Label {
		span: source.span(0..1)?,
		style: LabelStyle::Secondary,
		message: "gate invocation".into(),
	});
	diagnostic.sources.insert(source)?;
	let rendered = quest_language::render_plain(&diagnostic)?;
	verify_that!(rendered, contains_substring("nonexistent.qasm"))?;
	verify_that!(rendered, contains_substring("not declared"))?;
	verify_that!(rendered, contains_substring("gate invocation"))?;
	verify_that!(rendered, contains_substring("QL0002"))?;
	Ok(())
}

#[gtest]
fn diagnostic_checks_trace_and_replacement_spans_without_labels() -> Result<()> {
	use quest_language::{
		Diagnostic, DiagnosticCause, Replacement, Stage, Suggestion, TraceFrame, TraceKind,
	};
	let source = SourceSnapshot::new(SourceId::new(99), "input", "include x;");
	let mut diagnostic = Diagnostic::new(
		Stage::Parsing,
		DiagnosticCause::IncludeFailure {
			include: "x".into(),
			reason: "cycle".into(),
		},
		"include cycle",
	);
	diagnostic.provenance.trace.push(TraceFrame {
		kind: TraceKind::Include,
		span: source.span(0..9)?,
		message: "included here".into(),
	});
	verify_eq!(
		diagnostic.validate_sources(),
		Err(SourceError::UnknownSource)
	)?;
	diagnostic.sources.insert(source)?;
	diagnostic.validate_sources()?;
	let other = SourceSnapshot::new(SourceId::new(100), "input", "other");
	diagnostic.suggestions.push(Suggestion {
		message: "remove include".into(),
		replacement: Some(Replacement {
			span: other.span(0..5)?,
			text: String::new(),
		}),
	});
	verify_eq!(
		diagnostic.validate_sources(),
		Err(SourceError::UnknownSource)
	)?;
	Ok(())
}

#[gtest]
fn location_only_occurrences_allow_missing_text_but_validate_present_snapshots() -> Result<()> {
	use quest_language::{Diagnostic, DiagnosticCause, SourceSpan, Stage};
	let missing = SourceSpan::location(SourceId::new(201), 4..9)?;
	let mut diagnostic = Diagnostic::new(
		Stage::Execution,
		DiagnosticCause::Lifecycle {
			reason: "backend".into(),
		},
		"backend",
	);
	diagnostic.occurrence = Some(missing);
	diagnostic.validate_sources()?;

	let source = SourceSnapshot::new(SourceId::new(201), "macro.rs", "x");
	diagnostic.sources.insert(source)?;
	verify_eq!(diagnostic.validate_sources(), Err(SourceError::OutOfBounds))?;
	Ok(())
}

#[cfg(feature = "serde")]
#[gtest]
fn serde_rejects_reversed_spans_and_duplicate_snapshot_identities() -> Result<()> {
	use quest_language::{SourceMap, SourceSpan};
	verify_that!(
		serde_json::from_str::<SourceSpan>(r#"{"source":0,"start":9,"end":1}"#),
		err(anything())
	)?;
	verify_that!(
		serde_json::from_str::<SourceMap>(
			r#"[{"id":0,"name":"one","text":"x"},{"id":0,"name":"two","text":"y"}]"#
		),
		err(anything())
	)?;
	// A wire span cannot establish bounds until paired with its snapshot.
	let span = serde_json::from_str::<SourceSpan>(r#"{"source":0,"start":0,"end":99}"#)?;
	let source = SourceSnapshot::new(SourceId::new(0), "input", "x");
	verify_eq!(source.slice(span), Err(SourceError::OutOfBounds))?;
	Ok(())
}

#[cfg(feature = "serde")]
#[gtest]
fn diagnostic_roundtrip_preserves_sources_causes_and_suggestions() -> Result<()> {
	use quest_language::{
		Diagnostic, DiagnosticCause, Entity, ExecutionContext, InstructionOccurrence, IrOccurrence,
		Replacement, Stage, Suggestion,
	};
	let source = SourceSnapshot::new(SourceId::new(42), "memory", "bad");
	let mut diagnostic = Diagnostic::new(
		Stage::Admission,
		DiagnosticCause::UnsupportedCapability {
			capability: "pulse timing".into(),
		},
		"unsupported timing",
	);
	diagnostic.suggestions.push(Suggestion {
		message: "remove timing".into(),
		replacement: Some(Replacement {
			span: source.span(0..3)?,
			text: String::new(),
		}),
	});
	diagnostic.sources.insert(source)?;
	diagnostic.occurrence = Some(
		diagnostic
			.sources
			.get(SourceId::new(42))
			.or_fail()?
			.span(0..3)?,
	);
	diagnostic.provenance.entity = Some(Entity::Operation(InstructionOccurrence {
		program: 91,
		block: 4,
		instruction: 7,
	}));
	diagnostic.provenance.execution.push(ExecutionContext {
		region: IrOccurrence {
			program: 91,
			index: 2,
		},
		block: IrOccurrence {
			program: 91,
			index: 4,
		},
	});
	diagnostic.notes.push("simulator profile".into());
	let encoded = serde_json::to_string(&diagnostic)?;
	let decoded: Diagnostic = serde_json::from_str(&encoded)?;
	verify_eq!(&decoded, &diagnostic)?;
	decoded.validate_sources()?;
	Ok(())
}

#[gtest]
fn registry_adjoint_and_decomposition_references_are_well_formed() -> Result<()> {
	use quest_language::{Adjoint, Decomposition};
	for &kind in GateKind::ALL {
		let definition = kind.definition();
		verify_eq!(GateKind::lookup(definition.name), Some(kind))?;
		match definition.adjoint {
			Adjoint::Gate(inverse) => {
				verify_eq!(inverse.definition().adjoint, Adjoint::Gate(kind))?;
			}
			Adjoint::Parameters(parameters) => {
				verify_eq!(parameters.len(), definition.parameter_count)?;
				for parameter in parameters {
					verify_that!(parameter.input, lt(definition.parameter_count))?;
				}
			}
			Adjoint::SelfInverse => {}
		}
		match definition.decomposition {
			Decomposition::Controlled { base, controls } => {
				verify_eq!(controls, definition.intrinsic_controls)?;
				verify_eq!(base.definition().target_count, definition.target_count)?;
			}
			Decomposition::Sequence(steps) => {
				for step in steps {
					verify_that!(step.gate, not(eq(kind)))?;
					verify_eq!(
						step.parameters.len(),
						step.gate.definition().parameter_count
					)?;
					for angle in step.parameters {
						verify_that!(angle.pi_denominator, gt(0))?;
						for term in angle.terms {
							verify_that!(term.denominator, gt(0))?;
							verify_that!(term.input, lt(definition.parameter_count))?;
						}
					}
				}
			}
			Decomposition::Primitive(_) => {}
		}
	}
	Ok(())
}

#[cfg(all(feature = "serde", feature = "codespan-reporting"))]
#[gtest]
fn renderer_rejects_deserialized_spans_splitting_utf8() -> Result<()> {
	use quest_language::{Diagnostic, DiagnosticCause, Label, LabelStyle, Stage};
	let source = SourceSnapshot::new(SourceId::new(0), "input", "α");
	let span = serde_json::from_str(r#"{"source":0,"start":1,"end":2}"#)?;
	let mut diagnostic = Diagnostic::new(
		Stage::Parsing,
		DiagnosticCause::InvalidSource(SourceError::NotCharBoundary),
		"invalid source range",
	);
	diagnostic.sources.insert(source)?;
	diagnostic.labels.push(Label {
		span,
		style: LabelStyle::Primary,
		message: "invalid".into(),
	});
	verify_that!(quest_language::render_plain(&diagnostic), err(anything()))?;
	Ok(())
}
