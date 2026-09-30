use googletest::prelude::*;
use quest_circuit::classical::OptimizationLimits;
#[allow(unused_imports)]
use quest_circuit::prelude::*;
use quest_circuit::{
    Angle, Constructed, FusionOptions, Gate, LinearOptions, ParityOptions, Program,
    QuantumRegionBuilder,
    language::{
        GateKind, SourceId, SourceSnapshot,
        classical::{ScalarType, ScalarValue, Width},
        semantic::{CompileLimits, builder::Builder},
        syntax::BinaryOperator,
    },
};
#[cfg(feature = "macros")]
use quest_circuit::{circuit, circuit_file};

// ANCHOR: typed_builder
#[gtest]
fn typed_builder_keeps_classical_categories_and_loop_values() -> Result<()> {
    let mut builder = Builder::new()?;
    let zero = builder.integer::<32>(0)?;
    let one = builder.integer::<32>(1)?;
    let three = builder.integer::<32>(3)?;
    let counter = builder.local("counter", &zero)?;
    let q = builder.qubit("q", 1)?;
    let condition = builder.read(&counter)?.less(&three)?;
    builder.while_loop(&condition, |body| {
        body.gate(GateKind::X, &[], std::slice::from_ref(&q))?;
        body.assign(&counter, &body.read(&counter)?.add(&one)?)
    })?;
    let verified = builder.finish(CompileLimits::default())?.into_ssa()?;
    verify_that!(
        verified
            .blocks()
            .iter()
            .any(|block| block.arguments.len() > 1),
        eq(true)
    )?;
    Ok(())
}
// ANCHOR_END: typed_builder

// ANCHOR: import_export
#[gtest]
fn import_export_preserves_explicit_includes_and_owned_sources() -> Result<()> {
    let source = SourceSnapshot::new(
        SourceId::new(1),
        "bell.qasm",
        "OPENQASM 3.1; include \"stdgates.inc\"; qubit[2] q; h q[0]; cx q[0], q[1];",
    );
    let mut resolver = quest_qasm::StandardLibrary::new(SourceId::new(2));
    let imported = quest_qasm::import(
        source,
        &mut resolver,
        quest_qasm::ImportLimits::default(),
        CompileLimits::default(),
    )
    .or_fail()?;
    let text = quest_qasm::export(&imported, quest_qasm::ExportLimits::default()).or_fail()?;
    verify_that!(text.contains("include \"stdgates.inc\";"), eq(true))?;
    verify_that!(imported.sources().iter().count(), eq(2))?;
    let reimported = quest_qasm::import(
        SourceSnapshot::new(SourceId::new(3), "roundtrip.qasm", text.clone()),
        &mut resolver,
        quest_qasm::ImportLimits::default(),
        CompileLimits::default(),
    )
    .or_fail()?;
    verify_eq!(
        quest_qasm::export(&reimported, quest_qasm::ExportLimits::default()).or_fail()?,
        text
    )?;
    Ok(())
}
// ANCHOR_END: import_export

// ANCHOR: compiler_file
#[cfg(feature = "macros")]
#[gtest]
fn compiler_tracked_source_uses_the_same_structured_pipeline() -> Result<()> {
    let program = circuit_file!("../../../docs/book/src/examples/bell.qasm")?;
    verify_that!(program.verify()?.lower()?.plan()?.num_qubits(), eq(2))?;
    Ok(())
}
// ANCHOR_END: compiler_file

// ANCHOR: numeric_rules
#[gtest]
fn numeric_widths_and_integer_division_are_explicit() -> Result<()> {
    let one = ScalarValue::parse_number("1")?;
    let two = ScalarValue::parse_number("2")?;
    verify_eq!(one.binary(BinaryOperator::Divide, &two)?.to_i128()?, 0)?;
    let narrowed = ScalarValue::parse_number("128")?.cast(ScalarType::Int(Width::new(8)?))?;
    verify_eq!(narrowed.to_i128()?, -128)?;
    verify_that!(
        Program::<Constructed>::parse("bit result = true;", "implicit.qasm"),
        err(anything())
    )?;
    Program::<Constructed>::parse("bit result = bit(true);", "explicit.qasm")?;
    Ok(())
}
// ANCHOR_END: numeric_rules

// ANCHOR: classical_optimization
#[gtest]
fn classical_optimization_reports_changes_and_retains_export_authority() -> Result<()> {
    let source = "qubit q; int n = 1 + 2; if (n == 3) { x q; } else { z q; }";
    let program = Program::<Constructed>::parse(source, "optimize.qasm")?;
    let (optimized, report) = program
        .verify()?
        .optimize_classical(OptimizationLimits::default())?;
    verify_that!(report.constants_folded, gt(0))?;
    verify_that!(report.branches_simplified, gt(0))?;
    let plan = optimized.lower()?.plan()?;
    let text =
        quest_qasm::export_syntax(plan.syntax(), quest_qasm::ExportLimits::default()).or_fail()?;
    verify_that!(text.contains("else"), eq(true))?;
    Ok(())
}
// ANCHOR_END: classical_optimization

// ANCHOR: ideal_optimization
#[gtest]
fn ideal_builder_separates_exact_rewrites_binding_and_numerical_fusion() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let q = builder.qubit(0)?;
    let theta = builder.parameter("theta")?;
    builder.gate(Gate::H, &[q], &[])?;
    builder.gate(Gate::H, &[q], &[])?;
    builder.gate(Gate::Rx(Angle::parameter(theta)?), &[q], &[])?;
    builder.gate(Gate::Z, &[q], &[])?;
    let (exact, exact_report) = builder.finish()?.optimize_exact()?;
    verify_eq!(exact_report.removed.len(), 2)?;
    let (linear, linear_report) = exact.optimize_linear(LinearOptions::default())?;
    let (parity, parity_report) = linear.optimize_parity(ParityOptions::default())?;
    let (fused, fusion_report) = parity
        .bind(&[(theta, 0.25)])?
        .fuse(FusionOptions::default())?;
    verify_that!(
        linear_report.after_operations,
        le(linear_report.before_operations)
    )?;
    verify_that!(
        parity_report.after_operations,
        le(parity_report.before_operations)
    )?;
    verify_that!(fusion_report.numerical_rounding_changed, eq(true))?;
    verify_eq!(fused.plan()?.num_qubits(), 1)?;
    Ok(())
}
// ANCHOR_END: ideal_optimization

// ANCHOR: macro_bell
#[cfg(feature = "macros")]
#[gtest]
fn structured_macro_builds_bell_program() -> Result<()> {
    let bell = circuit! {
        qubit[2] q;
        h q[0];
        cx q[0], q[1];
    }?;
    verify_eq!(bell.verify()?.lower()?.plan()?.num_qubits(), 2)?;
    Ok(())
}
// ANCHOR_END: macro_bell

// ANCHOR: owned_diagnostic
#[gtest]
fn owned_diagnostic_retains_source_and_typed_cause() -> Result<()> {
    use quest_circuit::language::{Diagnostic, DiagnosticCause, Label, LabelStyle, Stage};
    let source = SourceSnapshot::new(SourceId::new(9), "example.qasm", "x missing;");
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
        message: "declare this qubit before use".into(),
    });
    diagnostic.sources.insert(source)?;
    diagnostic
        .notes
        .push("Source text remains owned by this diagnostic.".into());
    diagnostic.validate_sources()?;
    verify_eq!(diagnostic.code().as_str(), "QL0002")?;
    verify_eq!(
        diagnostic
            .sources
            .slice(diagnostic.labels.first().or_fail()?.span)?,
        "missing"
    )?;
    Ok(())
}
// ANCHOR_END: owned_diagnostic

// ANCHOR: structured_quantum_optimization
#[gtest]
fn structured_quantum_optimization_keeps_syntax_and_reports_inverse_pairs() -> Result<()> {
    let program = Program::<Constructed>::parse("qubit q; h q; h q;", "quantum.qasm")?;
    let (optimized, report) = program
        .verify()?
        .optimize_quantum(quest_circuit::StructuredQuantumOptions::default())?;
    verify_eq!(report.before_gates, 2)?;
    verify_eq!(report.after_gates, 0)?;
    verify_eq!(report.inverse_pairs, 1)?;
    verify_eq!(optimized.lower()?.plan()?.syntax().statements.len(), 3)?;
    Ok(())
}
// ANCHOR_END: structured_quantum_optimization
