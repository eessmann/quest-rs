use googletest::{Result, prelude::*};
use quest_language::{SourceId, SourceSnapshot, semantic::CompileLimits};
use quest_qasm::{ExportLimits, ImportLimits, StandardLibrary, export, import};
use std::collections::BTreeMap;
mod support;

#[gtest]
fn admitted_import_export_import_preserves_definitions_and_control() -> Result<()> {
    let source = SourceSnapshot::new(
        SourceId::new(1),
        "root.qasm",
        r#"
OPENQASM 3.1;
include "stdgates.inc";
qubit[2] q;
output bit result;
gate local(a) t { gphase(a/2); ry(a) t; }
int[8] value = 0;
for int[8] i in [0:2] { value += 1; }
while (value > 0) { value -= 1; }
if (value == 0) { local(pi/3) q[0]; } else { reset q[0]; }
switch (value) { case 0 { CX q[0], q[1]; } default { barrier q; } }
result = measure q[1];
"#,
    );
    let first = import(
        source,
        &mut StandardLibrary::new(SourceId::new(10)),
        ImportLimits::default(),
        CompileLimits::default(),
    )?;
    let text = export(&first, ExportLimits::default())?;
    expect_true!(text.contains("include \"stdgates.inc\";"));
    expect_true!(text.contains("gate local"));
    let second = import(
        SourceSnapshot::new(SourceId::new(2), "again.qasm", text.clone()),
        &mut StandardLibrary::new(SourceId::new(20)),
        ImportLimits::default(),
        CompileLimits::default(),
    )?;
    expect_eq!(
        support::normalize(first.typed().syntax().clone()),
        support::normalize(second.typed().syntax().clone())
    );
    expect_eq!(export(&second, ExportLimits::default())?, text);
    first.into_typed().into_ssa()?;
    second.into_typed().into_ssa()?;
    Ok(())
}

#[gtest]
fn semantic_errors_own_their_source_after_resolver_drops() -> Result<()> {
    let error = import(
        SourceSnapshot::new(SourceId::new(1), "bad.qasm", "qubit q; unknown q;"),
        &mut quest_qasm::NoIncludes,
        ImportLimits::default(),
        CompileLimits::default(),
    )
    .err()
    .ok_or_else(|| std::io::Error::other("unknown gate must fail admission"))?;
    expect_eq!(error.stage, quest_language::Stage::Admission);
    expect_true!(matches!(
        error.cause,
        quest_language::DiagnosticCause::LanguageFailure {
            kind: quest_language::LanguageFailureKind::UnknownSymbol,
            ..
        }
    ));
    expect_eq!(error.sources.iter().count(), 1);
    error.validate_sources()?;
    Ok(())
}

#[gtest]
fn semantic_resource_diagnostics_preserve_exact_usage() -> Result<()> {
    let error = import(
        SourceSnapshot::new(SourceId::new(1), "budget.qasm", "qubit[2] q;"),
        &mut quest_qasm::NoIncludes,
        ImportLimits::default(),
        CompileLimits {
            qubits: 1,
            ..CompileLimits::default()
        },
    )
    .err()
    .ok_or_else(|| std::io::Error::other("qubit budget must fail"))?;
    expect_eq!(
        error.cause,
        quest_language::DiagnosticCause::ResourceLimit(quest_language::ResourceUsage {
            resource: quest_language::ResourceKind::Qubits,
            requested: 2,
            limit: 1
        })
    );
    error.validate_sources()?;
    Ok(())
}

struct Resolver(BTreeMap<String, SourceSnapshot>);
impl quest_qasm::IncludeResolver for Resolver {
    fn resolve(
        &mut self,
        _: &SourceSnapshot,
        path: &str,
    ) -> std::result::Result<SourceSnapshot, quest_qasm::ResolveError> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| quest_qasm::ResolveError::new("not supplied"))
    }
}

#[gtest]
fn semantic_include_failure_owns_label_and_expansion_trace() -> Result<()> {
    let root = SourceSnapshot::new(SourceId::new(1), "root.qasm", "include \"bad.inc\";");
    let included = SourceSnapshot::new(SourceId::new(2), "bad.inc", "qubit q; missing q;");
    let mut resolver = Resolver(BTreeMap::from([("bad.inc".into(), included)]));
    let error = import(
        root,
        &mut resolver,
        ImportLimits::default(),
        CompileLimits::default(),
    )
    .unwrap_err();
    drop(resolver);
    expect_eq!(error.sources.iter().count(), 2);
    expect_eq!(error.labels.len(), 1);
    expect_eq!(error.provenance.trace.len(), 1);
    expect_eq!(
        error.provenance.trace[0].kind,
        quest_language::TraceKind::Include
    );
    error.validate_sources()?;
    #[cfg(feature = "codespan-reporting")]
    {
        let rendered = quest_language::render_plain(&error)?;
        expect_true!(rendered.contains("root.qasm"));
        expect_true!(rendered.contains("bad.inc"));
    }
    Ok(())
}
