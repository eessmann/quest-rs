use clap::Parser;
use quest_qsvt_cli::Cli;
#[test]
fn both_algorithms_have_explicit_sequence_exports_without_precision_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.json");
    let output = dir.path().join("sequence.json");
    std::fs::write(&input, r#"{"basis":"Chebyshev","coefficients":[0,0.3]}"#).unwrap();
    for algorithm in ["rhw", "inverse-nlft"] {
        let report = Cli::try_parse_from([
            "qsvt",
            "synthesize",
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--algorithm",
            algorithm,
            "--export",
            "sequence",
        ])
        .unwrap()
        .run()
        .unwrap();
        assert_eq!(report["algorithm"], algorithm);
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        assert!(value.get("angles").is_some());
    }
}
#[cfg(feature = "certification")]
#[test]
fn certified_cli_export_retains_receipt_and_reloads_both_conventions() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.json");
    let output = dir.path().join("compiled.json");
    for (mode, source) in [
        (
            "real-parity-wx",
            r#"{"basis":"Chebyshev","coefficients":[0,0.3]}"#,
        ),
        (
            "unit-circle-response",
            r#"{"basis":"Laurent","coefficients":[[0.2,0.1],[-0.03,0.02]]}"#,
        ),
    ] {
        for algorithm in ["rhw", "inverse-nlft"] {
            std::fs::write(&input, source).unwrap();
            let report = Cli::try_parse_from([
                "qsvt",
                "synthesize",
                "--input",
                input.to_str().unwrap(),
                "--output",
                output.to_str().unwrap(),
                "--mode",
                mode,
                "--algorithm",
                algorithm,
                "--certify",
            ])
            .unwrap()
            .run()
            .unwrap();
            assert_eq!(report["export"], "compiled");
            let text = std::fs::read_to_string(&output).unwrap();
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert!(value["payload"]["historical_receipt"].is_object());
            assert!(matches!(
                quest_qsvt_io::read_qsp_json(&text, quest_qsvt_io::IoPolicy::default()).unwrap(),
                quest_qsvt_io::QspInput::Compiled(_)
            ));
        }
    }
}
#[cfg(not(feature = "certification"))]
#[test]
fn compiled_export_requires_explicit_feature_without_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.json");
    let output = dir.path().join("compiled.json");
    std::fs::write(&input, r#"{"coefficients":[0.2]}"#).unwrap();
    let result = Cli::try_parse_from([
        "qsvt",
        "synthesize",
        "--input",
        input.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ])
    .unwrap()
    .run();
    assert!(result.unwrap_err().to_string().contains("certification"));
    assert!(!output.exists());
}
#[cfg(feature = "offline-synthesis")]
#[test]
fn offline_algorithm_selection_retains_arbitrary_precision_for_both_modes() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.json");
    let output = dir.path().join("compiled.json");
    for (mode, basis) in [
        ("real-parity-wx", "Chebyshev"),
        ("unit-circle-response", "Laurent"),
    ] {
        std::fs::write(
            &input,
            format!(r#"{{"basis":"{basis}","coefficients":[0.2]}}"#),
        )
        .unwrap();
        for algorithm in ["rhw", "inverse-nlft"] {
            let report = Cli::try_parse_from([
                "qsvt",
                "offline-synthesize",
                "--input",
                input.to_str().unwrap(),
                "--output",
                output.to_str().unwrap(),
                "--mode",
                mode,
                "--algorithm",
                algorithm,
            ])
            .unwrap()
            .run()
            .unwrap();
            assert_eq!(report["algorithm"], algorithm);
            let value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
            assert_eq!(value["payload"]["precision_kind"], "arbitrary");
            assert!(value["payload"]["historical_receipt"].is_object());
        }
    }
}
#[test]
fn certification_request_cannot_silently_discard_evidence_in_sequence_export() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.json");
    let output = dir.path().join("sequence.json");
    std::fs::write(&input, r#"{"coefficients":[0.2]}"#).unwrap();
    let result = Cli::try_parse_from([
        "qsvt",
        "synthesize",
        "--input",
        input.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--export",
        "sequence",
        "--certify",
    ])
    .unwrap()
    .run();
    assert!(result.unwrap_err().to_string().contains("retain evidence"));
    assert!(!output.exists());
}
