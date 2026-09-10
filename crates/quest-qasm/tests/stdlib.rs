use googletest::{Result, prelude::*};
use quest_language::{
    SourceId, SourceSnapshot,
    semantic::CompileLimits,
    syntax::{self, StatementKind},
};
use quest_qasm::{
    ImportLimits, IncludeResolver, ResolveError, STANDARD_GATES, StandardLibrary,
    UPSTREAM_STANDARD_GATES, import,
};
use sha2::{Digest, Sha256};
use std::fmt::Write;
fn digest(text: &str) -> std::result::Result<String, std::fmt::Error> {
    let mut hex = String::new();
    for byte in Sha256::digest(text) {
        write!(hex, "{byte:02x}")?;
    }
    Ok(hex)
}
mod support;

#[gtest]
fn pinned_sources_and_only_documented_phase_corrections_are_exact() -> Result<()> {
    expect_eq!(
        digest(UPSTREAM_STANDARD_GATES)?,
        "b2b60afcbc0c2195bd3cb3ec0347c4c9f7447fd22efb28b36b5ab7ee721c11c0"
    );
    expect_eq!(
        digest(STANDARD_GATES)?,
        "c27f21a4c72cfe36dfceee47d9812041c9cbaf309568173507b581bf7d74b534"
    );
    let expected = format!(
        "// quest-qasm: corrected cu and CX to match the OpenQASM 3.1 mathematical specification.\n// Original and provenance are bundled alongside this file.\n{}",
        UPSTREAM_STANDARD_GATES
            .replace("p(γ-θ/2) a;", "p(γ) a;")
            .replace(
                "gate CX a, b { ctrl @ U(π, 0, π) a, b; }",
                "gate CX a, b { cx a, b; }"
            )
    );
    expect_eq!(STANDARD_GATES, expected);
    let parsed = syntax::parse_source(&SourceSnapshot::new(
        SourceId::new(1),
        "stdlib",
        STANDARD_GATES,
    ))?;
    let corrected = syntax::Module { statements: parsed.statements.into_iter().filter(|s| matches!(&s.kind, StatementKind::GateDeclaration { name, .. } if name == "cu" || name == "CX")).collect() };
    let expected = syntax::parse_source(&SourceSnapshot::new(
        SourceId::new(2),
        "spec definitions",
        "gate cu(θ, φ, λ, γ) a, b { p(γ) a; ctrl @ U(θ, φ, λ) a, b; } gate CX a, b { cx a, b; }",
    ))?;
    expect_eq!(support::normalize(corrected), support::normalize(expected));
    Ok(())
}

struct AlteredLibrary;
impl IncludeResolver for AlteredLibrary {
    fn resolve(
        &mut self,
        _: &SourceSnapshot,
        _: &str,
    ) -> std::result::Result<SourceSnapshot, ResolveError> {
        Ok(SourceSnapshot::new(
            SourceId::new(2),
            "stdgates.inc",
            STANDARD_GATES.replace(
                "gate x a { U(π, 0, π) a; gphase(-π/2);}",
                "gate x a { U(0, 0, 0) a; }",
            ),
        ))
    }
}
#[gtest]
fn only_exact_pinned_library_may_use_registry_intrinsics() -> Result<()> {
    let source = SourceSnapshot::new(
        SourceId::new(1),
        "root.qasm",
        "include \"stdgates.inc\"; qubit[3] q; cu(0.3, 0.5, 0.7, 0.9) q[0], q[1]; CX q[1], q[2];",
    );
    let admitted = import(
        source.clone(),
        &mut StandardLibrary::new(SourceId::new(2)),
        ImportLimits::default(),
        CompileLimits::default(),
    )?;
    expect_true!(
        admitted.typed().syntax().statements.iter().any(
            |s| matches!(&s.kind, StatementKind::GateDeclaration { name, .. } if name == "cu")
        )
    );
    expect_false!(
        admitted
            .typed()
            .syntax()
            .statements
            .iter()
            .any(|s| matches!(&s.kind, StatementKind::GateDeclaration { name, .. } if name == "x"))
    );
    let error = import(
        source,
        &mut AlteredLibrary,
        ImportLimits::default(),
        CompileLimits::default(),
    )
    .err()
    .ok_or_else(|| {
        std::io::Error::other("altered standard declarations must not be silently elided")
    })?;
    expect_eq!(error.stage, quest_language::Stage::Admission);
    error.validate_sources()?;
    Ok(())
}

#[gtest]
fn shared_expanded_admission_validates_the_pinned_statement_body() -> Result<()> {
    let source = SourceSnapshot::new(SourceId::new(1), "stdgates.inc", STANDARD_GATES);
    let mut sources = quest_language::SourceMap::default();
    sources.insert(source.clone())?;
    let module = syntax::parse_source(&source)?;
    let admitted = quest_qasm::admit_expanded(module.clone(), &sources, CompileLimits::default())?;
    expect_false!(admitted.syntax().statements.iter().any(|statement| matches!(&statement.kind, StatementKind::GateDeclaration { name, .. } if name == "x")));
    let mut forged = module;
    for statement in &mut forged.statements {
        if let StatementKind::GateDeclaration { name, body, .. } = &mut statement.kind
            && name == "x"
        {
            body.clear();
        }
    }
    let error = quest_qasm::admit_expanded(forged, &sources, CompileLimits::default())
        .err()
        .ok_or_else(|| std::io::Error::other("forged pinned declaration must fail"))?;
    error.validate_sources()?;
    Ok(())
}
