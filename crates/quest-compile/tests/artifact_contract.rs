use googletest::prelude::*;
use quest_compile::{
    Angle, ArtifactLimits, Constructed, Executable, Program, ProgramBuilder, circuit,
};
#[gtest]
fn source_and_compiled_exports_remain_distinct_and_load_retains_optimized_ssa() -> Result<()> {
    let source = Program::<Constructed>::parse("qubit q;h q;h q;", "artifact.qasm")?.verify()?;
    let optimized = source
        .optimize_quantum(quest_compile::StructuredQuantumOptions::default())?
        .0
        .lower()?
        .plan()?;
    let source_text = optimized.export_source(quest_qasm::ExportLimits::default())?;
    expect_eq!(
        optimized
            .syntax()
            .statements
            .iter()
            .filter(|s| matches!(
                s.kind,
                quest_compile::language::syntax::StatementKind::Gate { .. }
            ))
            .count(),
        2
    );
    let encoded = optimized.export_compiled(ArtifactLimits::default())?;
    let loaded = Program::<Executable>::load_compiled(&encoded, ArtifactLimits::default())?;
    expect_eq!(
        loaded
            .ssa()
            .blocks()
            .iter()
            .flat_map(|b| &b.instructions)
            .filter(|i| matches!(
                i.kind,
                quest_compile::language::ssa::InstructionKind::Gate { .. }
            ))
            .count(),
        0
    );
    expect_eq!(
        loaded.export_source(quest_qasm::ExportLimits::default())?,
        source_text
    );
    Ok(())
}
#[gtest]
fn macro_and_builder_exact_captures_roundtrip_and_bad_envelopes_fail() -> Result<()> {
    let macro_program = circuit! {qubit q;rz(${Angle::pi(1,4)?}) q;}?
        .verify()?
        .lower()?
        .plan()?;
    let mut builder = ProgramBuilder::new()?;
    let q = builder.qubit("q", 1)?;
    let theta = builder.angle(Angle::pi(1, 4)?)?;
    builder.gate(quest_compile::language::GateKind::Rz, &[theta], &[q])?;
    for program in [macro_program, builder.finish()?.verify()?.lower()?.plan()?] {
        let encoded = program.export_compiled(ArtifactLimits::default())?;
        let loaded = Program::<Executable>::load_compiled(&encoded, ArtifactLimits::default())?;
        expect_eq!(loaded.exact_captures().len(), 1);
        expect_eq!(loaded.captures(), program.captures());
        expect_true!(
            loaded
                .export_source(quest_qasm::ExportLimits::default())
                .is_err()
        );
        let incompatible = encoded.replacen("\"version\":3", "\"version\":1", 1);
        expect_true!(
            Program::<Executable>::load_compiled(&incompatible, ArtifactLimits::default()).is_err()
        );
        let corrupted = encoded.replacen("quest-ssa-v1", "quest-ssa-v2", 1);
        expect_true!(
            Program::<Executable>::load_compiled(&corrupted, ArtifactLimits::default()).is_err()
        );
    }
    Ok(())
}

#[expect(
    clippy::indexing_slicing,
    reason = "The fixture deliberately mutates known JSON envelope fields"
)]
#[expect(
    clippy::format_collect,
    reason = "The fixture recomputes the published hexadecimal digest"
)]
fn mutate_artifact(encoded: &str, change: impl FnOnce(&mut serde_json::Value)) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut envelope: serde_json::Value = serde_json::from_str(encoded)?;
    let mut publication: serde_json::Value = serde_json::from_str(
        envelope["payload"]
            .as_str()
            .ok_or_else(|| std::io::Error::other("payload"))?,
    )?;
    change(&mut publication);
    let payload = serde_json::to_string(&publication)?;
    let digest: String = Sha256::digest(payload.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    envelope["payload"] = payload.into();
    envelope["digest"] = digest.into();
    Ok(serde_json::to_string(&envelope)?)
}
#[gtest]
fn artifact_load_rechecks_ir_and_capture_evidence_even_with_valid_digest() -> Result<()> {
    let program = circuit! {qubit q;rz(${Angle::pi(1,4)?}) q;}?
        .verify()?
        .lower()?
        .plan()?;
    let encoded = program.export_compiled(ArtifactLimits::default())?;
    let broken = mutate_artifact(&encoded, |p| p["program"]["blocks"] = serde_json::json!([]))?;
    expect_true!(Program::<Executable>::load_compiled(&broken, ArtifactLimits::default()).is_err());
    let changed = mutate_artifact(&encoded, |p| {
        p["exact"]["0"]["Affine"]["pi_numerator"] = "2".into();
    })?;
    expect_true!(
        Program::<Executable>::load_compiled(&changed, ArtifactLimits::default()).is_err()
    );
    let mut envelope: serde_json::Value = serde_json::from_str(&encoded)?;
    envelope["digest"] = "00".into();
    expect_true!(
        Program::<Executable>::load_compiled(
            &serde_json::to_string(&envelope)?,
            ArtifactLimits::default()
        )
        .is_err()
    );
    Ok(())
}
#[gtest]
fn matrix_channel_oracle_and_finite_provenance_roundtrip() -> Result<()> {
    use quest_compile::{BoundGate, Gate, MatrixPolicy, OracleFragment, QuantumRegionBuilder};
    let mut finite = QuantumRegionBuilder::new(1, 0)?;
    let q = finite.qubit(0)?;
    let theta = finite.parameter("theta")?;
    finite.gate(Gate::Rz(Angle::parameter(theta)?), &[q], &[])?;
    let p = Program::<Constructed>::from_region(finite.finish()?, &[(theta, 0.25)])?
        .verify()?
        .lower()?
        .plan()?;
    let loaded = Program::<Executable>::load_compiled(
        &p.export_compiled(ArtifactLimits::default())?,
        ArtifactLimits::default(),
    )?;
    expect_eq!(loaded.artifact_sources().len(), 1);
    expect_eq!(loaded.artifact_sources()[0].bindings.len(), 1);
    expect_eq!(loaded.publication_history(), &[p.ssa().snapshot()]);
    let mut body = QuantumRegionBuilder::new(1, 0)?;
    body.gate(Gate::H, &[body.qubit(0)?], &[])?;
    let oracle =
        OracleFragment::from_program(body.finish()?.bind(&[])?, 1e-10, MatrixPolicy::default())?;
    let mut b = ProgramBuilder::new()?;
    let q = b.qubit("q", 1)?;
    let matrix = BoundGate::X.matrix(MatrixPolicy::default())?;
    b.matrix(matrix.clone(), std::slice::from_ref(&q), &[])?;
    b.channel(
        vec![matrix],
        std::slice::from_ref(&q),
        1e-10,
        MatrixPolicy::default(),
    )?;
    let oracle = b.oracle("oracle", oracle)?;
    b.call_gate(&oracle, &[], &[q], &[])?;
    let p = b.finish()?.verify()?.lower()?.plan()?;
    let loaded = Program::<Executable>::load_compiled(
        &p.export_compiled(ArtifactLimits::default())?,
        ArtifactLimits::default(),
    )?;
    expect_eq!(loaded.quantum_payloads().len(), 2);
    expect_eq!(loaded.oracle_captures().len(), 1);
    Ok(())
}

#[cfg(feature = "synthesis")]
#[gtest]
fn synthesis_artifact_retains_and_rechecks_certificate_evidence() -> Result<()> {
    let program = circuit! {qubit q;rz(${Angle::pi(0,1)?}) q;}?.verify()?;
    let (compiled, _) = program.synthesize_rotations(
        &quest_compile::NativeSynthesis::default(),
        0.1,
        17,
        quest_compile::certified::Limits::default(),
    )?;
    let executable = compiled.lower()?.plan()?;
    let encoded = executable.export_compiled(ArtifactLimits::default())?;
    let loaded = Program::<Executable>::load_compiled(&encoded, ArtifactLimits::default())?;
    expect_eq!(loaded.compilation_evidence().len(), 1);
    expect_eq!(loaded.compilation_evidence()[0].seed, 17);
    let previous_evidence = mutate_artifact(&encoded, |p| {
        p["evidence"][0]["version"] = 1.into();
    })?;
    expect_true!(
        Program::<Executable>::load_compiled(&previous_evidence, ArtifactLimits::default())
            .is_err()
    );

    let changed = mutate_artifact(&encoded, |p| {
        p["evidence"][0]["target"] = serde_json::json!({"invalid":true});
    })?;
    expect_true!(
        Program::<Executable>::load_compiled(&changed, ArtifactLimits::default()).is_err()
    );
    Ok(())
}

#[cfg(feature = "synthesis")]
#[gtest]
fn finite_synthesis_evidence_survives_normal_import_and_compiled_export() -> Result<()> {
    use quest_compile::{Gate, NativeSynthesis, QuantumRegionBuilder, prelude::*};
    let mut b = QuantumRegionBuilder::new(1, 0)?;
    b.gate(Gate::Rz(Angle::pi(0, 1)?), &[b.qubit(0)?], &[])?;
    let (region, report) = b.finish()?.synthesize_rotations(
        &NativeSynthesis::default(),
        0.1,
        23,
        quest_compile::certified::Limits::default(),
    )?;
    expect_eq!(report.rotations.len(), 1);
    let program = Program::<Constructed>::from_region(region, &[])?
        .verify()?
        .lower()?
        .plan()?;
    let encoded = program.export_compiled(ArtifactLimits::default())?;
    let loaded = Program::<Executable>::load_compiled(&encoded, ArtifactLimits::default())?;
    expect_eq!(loaded.artifact_sources()[0].certificates.len(), 1);
    let changed = mutate_artifact(&encoded, |p| {
        p["finite_sources"][0]["certificates"][0]["target"] = serde_json::json!({"invalid":true});
    })?;
    expect_true!(
        Program::<Executable>::load_compiled(&changed, ArtifactLimits::default()).is_err()
    );
    Ok(())
}

#[gtest]
fn artifact_admission_bounds_deserialization_and_aggregate_matrix_reconstruction() -> Result<()> {
    use quest_compile::{ArtifactError, BoundGate, MatrixPolicy};
    let mut b = ProgramBuilder::new()?;
    let q = b.qubit("q", 1)?;
    let m = BoundGate::X.matrix(MatrixPolicy::default())?;
    b.matrix(m.clone(), std::slice::from_ref(&q), &[])?;
    b.matrix(m, std::slice::from_ref(&q), &[])?;
    let p = b.finish()?.verify()?.lower()?.plan()?;
    let encoded = p.export_compiled(ArtifactLimits::default())?;
    let mut limits = ArtifactLimits::default();
    limits.compile.storage_bytes = 1;
    expect_true!(matches!(
        Program::<Executable>::load_compiled(&encoded, limits),
        Err(ArtifactError::Budget)
    ));
    // Each 2x2 payload fits individually; simultaneous DTO/native/admission scratch does not.
    let limits = ArtifactLimits {
        matrix_bytes: 400,
        ..ArtifactLimits::default()
    };
    expect_true!(matches!(
        Program::<Executable>::load_compiled(&encoded, limits),
        Err(ArtifactError::Budget)
    ));
    expect_true!(matches!(
        p.export_compiled(ArtifactLimits {
            bytes: 1,
            ..ArtifactLimits::default()
        }),
        Err(ArtifactError::Budget)
    ));
    Ok(())
}

#[cfg(feature = "synthesis")]
#[gtest]
fn historical_local_evidence_never_grants_current_executable_equivalence() -> Result<()> {
    let p = circuit! {qubit q;rz(${Angle::pi(0,1)?}) q;}?.verify()?;
    let (p, _) = p.synthesize_rotations(
        &quest_compile::NativeSynthesis::default(),
        0.1,
        3,
        quest_compile::certified::Limits::default(),
    )?;
    let encoded = p
        .lower()?
        .plan()?
        .export_compiled(ArtifactLimits::default())?;
    let other = circuit! {qubit q;x q;}?
        .verify()?
        .lower()?
        .plan()?
        .export_compiled(ArtifactLimits::default())?;
    let envelope: serde_json::Value = serde_json::from_str(&other)?;
    let replacement: serde_json::Value = serde_json::from_str(
        envelope
            .get("payload")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| std::io::Error::other("payload"))?,
    )?;
    let changed = mutate_artifact(&encoded, |p| p["program"] = replacement["program"].clone())?;
    let loaded = Program::<Executable>::load_compiled(&changed, ArtifactLimits::default())?;
    expect_eq!(
        loaded.compilation_evidence()[0].scope,
        quest_compile::EvidenceScope::HistoricalLocalCertificate
    );
    expect_true!(
        loaded
            .ssa()
            .blocks()
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(
                i.kind,
                quest_compile::language::ssa::InstructionKind::Gate {
                    gate: quest_compile::language::GateKind::X,
                    ..
                }
            ))
    );
    Ok(())
}

#[gtest]
fn artifact_unknown_fields_are_incompatible_even_with_recomputed_digest() -> Result<()> {
    let p = circuit! {qubit q;h q;}?.verify()?.lower()?.plan()?;
    let encoded = p.export_compiled(ArtifactLimits::default())?;
    let changed = mutate_artifact(&encoded, |p| {
        p["new_evidence_semantics"] = serde_json::json!(true);
    })?;
    expect_true!(
        Program::<Executable>::load_compiled(&changed, ArtifactLimits::default()).is_err()
    );
    Ok(())
}
