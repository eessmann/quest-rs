use quest_qsvt_io::{IoPolicy, read_qsp_json};
#[cfg(not(feature = "certification"))]
#[test]
fn compiled_json_is_never_silently_read_as_source_without_feature() {
    assert!(
        read_qsp_json(r#"{"payload":{},"sha256":[]}"#, IoPolicy::default())
            .unwrap_err()
            .to_string()
            .contains("certification")
    );
}
#[cfg(feature = "certification")]
#[test]
fn compiled_evidence_survives_execution_wire_and_rejects_payload_changes() {
    use quest_polynomial::{Chebyshev, Limits, Polynomial};
    use quest_qsp::artifact::{ArtifactLimits, export_compiled};
    use quest_qsp::{Complex64, SynthesisBuilder};
    use quest_qsvt_io::{QspInput, read_qsp_execution_json, write_qsp_execution_json};
    let p = Polynomial::new(Chebyshev, vec![Complex64::new(0.3, 0.0)], Limits::default()).unwrap();
    let candidate = SynthesisBuilder::new()
        .real_parity_wx(&p)
        .unwrap()
        .admit()
        .unwrap()
        .complete()
        .unwrap()
        .synthesize()
        .unwrap();
    let bytes = export_compiled(&candidate, ArtifactLimits::default()).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let frozen = read_qsp_json(&text, IoPolicy::default()).unwrap();
    assert!(matches!(&frozen, QspInput::Compiled(_)));
    let wire = write_qsp_execution_json(&frozen).unwrap();
    assert_eq!(wire, text);
    assert!(matches!(
        read_qsp_execution_json(&wire, IoPolicy::default()).unwrap(),
        QspInput::Compiled(_)
    ));
    let mut value: serde_json::Value = serde_json::from_str(&wire).unwrap();
    value["payload"]["phases"][0] = serde_json::json!(0u64);
    assert!(read_qsp_json(&value.to_string(), IoPolicy::default()).is_err());
    assert!(
        quest_qsvt_io::read_compiled_qsp_json(
            &text,
            IoPolicy::default(),
            quest_qsp::certification::CertificationPolicy {
                response_tolerance: 1e-30,
                ..quest_qsp::certification::CertificationPolicy::default()
            }
        )
        .is_err()
    );
}
