use googletest::prelude::*;
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::{Constructed, MacroLocation, Program};

#[gtest]
fn structured_stages_retain_source_and_verified_loop_arguments() -> Result<()> {
    let program = Program::<Constructed>::parse(
        "qubit q; int i = 0; while (i < 3) { h q; i += 1; }",
        "loop.qasm",
    )?;
    let verified = program.verify()?;
    expect_true!(
        verified
            .ssa()
            .blocks()
            .iter()
            .any(|block| block.arguments.len() > 1)
    );
    let plan = verified.lower()?.plan()?;
    expect_eq!(plan.num_qubits(), 1);
    expect_eq!(plan.syntax().statements.len(), 3);
    expect_true!(plan.resources().classical_bytes > 0);
    Ok(())
}

#[gtest]
fn capture_admission_rejects_missing_values() -> Result<()> {
    use quest_compile::language::{
        SourceId, SourceSnapshot,
        syntax::{ParseLimits, TokenKind, lex},
    };
    let source = SourceSnapshot::new(SourceId::new(55), "capture.qasm", "qubit q; rx(0.25) q;");
    let mut tokens = lex(&source, ParseLimits::default())?;
    for token in &mut tokens {
        if token.kind == TokenKind::Number("0.25".into()) {
            token.kind = TokenKind::Capture(0);
        }
    }
    let span = tokens
        .iter()
        .find(|token| token.kind == TokenKind::Capture(0))
        .and_then(|token| token.span)
        .or_fail()?;
    let program = Program::<Constructed>::from_frontend(
        &tokens,
        vec![],
        quest_compile::language::SourceMap::default(),
        vec![MacroLocation {
            span,
            file: "consumer.rs".into(),
            line: 17,
            column: 9,
        }],
    )?;
    let error = program.verify().unwrap_err();
    let diagnostic = error.diagnostic().or_fail()?;
    expect_true!(matches!(
        diagnostic.cause,
        quest_compile::language::DiagnosticCause::LanguageFailure {
            kind: quest_compile::language::LanguageFailureKind::RuntimeCapture,
            ..
        }
    ));
    expect_true!(diagnostic.labels.is_empty());
    expect_true!(diagnostic.notes[0].contains("consumer.rs:17:9"));
    expect_true!(
        Program::<Constructed>::parse("qubit q; float x; rx(x) q;", "invalid.qasm").is_err()
    );
    Ok(())
}

#[gtest]
fn parsed_language_error_owns_renderable_source() -> Result<()> {
    let error = Program::<Constructed>::parse("qubit q; missing q;", "bad.qasm").unwrap_err();
    let diagnostic = error.diagnostic().or_fail()?;
    expect_eq!(diagnostic.stage, quest_compile::language::Stage::Admission);
    expect_eq!(diagnostic.labels.len(), 1);
    diagnostic.validate_sources()?;
    expect_eq!(
        diagnostic.sources.slice(diagnostic.labels[0].span)?,
        "missing"
    );
    Ok(())
}
