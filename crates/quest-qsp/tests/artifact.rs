#![cfg(feature = "artifact")]
use quest_polynomial::{Chebyshev, Laurent, Limits, Polynomial};
use quest_qsp::artifact::{
    ArtifactLimits, LoadPolicy, LoadedCertified, LoadedCompiled, export_compiled, load_certified,
    load_compiled,
};
use quest_qsp::certification::CertificationPolicy;
use quest_qsp::{Complex64, SynthesisBuilder};
#[test]
fn exact_payload_roundtrip_and_recertification() {
    let target = Polynomial::new(
        Chebyshev,
        vec![Complex64::new(0.0, -0.0), Complex64::new(0.3, 0.0)],
        Limits::default(),
    )
    .unwrap();
    let frozen = SynthesisBuilder::new()
        .real_parity_wx(&target)
        .unwrap()
        .admit()
        .unwrap()
        .complete()
        .unwrap()
        .synthesize()
        .unwrap();
    let bytes = export_compiled(&frozen, ArtifactLimits::default()).unwrap();
    let LoadedCompiled::RealParityWx(loaded) =
        load_compiled(&bytes, LoadPolicy::default()).unwrap()
    else {
        panic!("wrong mode")
    };
    assert_eq!(
        frozen
            .phases()
            .iter()
            .map(|x| x.to_bits())
            .collect::<Vec<_>>(),
        loaded
            .phases()
            .iter()
            .map(|x| x.to_bits())
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        load_certified(
            &bytes,
            LoadPolicy::default(),
            CertificationPolicy::default()
        )
        .unwrap(),
        LoadedCertified::RealParityWx(_)
    ));
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        parsed["payload"]["source"][0][1],
        serde_json::json!((-0.0f64).to_bits())
    );
}
#[test]
fn complex_export_binds_every_control_and_rejects_corruption_and_budgets() {
    let target = Polynomial::new(
        Laurent::new(2),
        vec![Complex64::new(0.2, 0.1), Complex64::new(-0.03, 0.02)],
        Limits::default(),
    )
    .unwrap();
    let frozen = SynthesisBuilder::new()
        .unit_circle_response(&target)
        .unwrap()
        .admit()
        .unwrap()
        .complete()
        .unwrap()
        .synthesize()
        .unwrap();
    let bytes = export_compiled(&frozen, ArtifactLimits::default()).unwrap();
    let LoadedCertified::UnitCircleResponse(loaded) = load_certified(
        &bytes,
        LoadPolicy::default(),
        CertificationPolicy::default(),
    )
    .unwrap() else {
        panic!("wrong mode")
    };
    assert_eq!(frozen.controls(), loaded.candidate().controls());
    let mut damaged: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    damaged["payload"]["controls"][0][0][0][0] = serde_json::json!(0u64);
    assert!(
        load_compiled(
            &serde_json::to_vec(&damaged).unwrap(),
            LoadPolicy::default()
        )
        .is_err()
    );
    assert!(
        load_compiled(
            &bytes,
            LoadPolicy {
                storage: ArtifactLimits {
                    max_bytes: bytes.len() - 1,
                    ..ArtifactLimits::default()
                },
                ..LoadPolicy::default()
            }
        )
        .is_err()
    );
    assert!(
        load_compiled(
            &bytes,
            LoadPolicy {
                storage: ArtifactLimits {
                    max_coefficients: 1,
                    ..ArtifactLimits::default()
                },
                ..LoadPolicy::default()
            }
        )
        .is_err()
    );
}
#[cfg(feature = "offline-synthesis")]
#[test]
fn offline_solver_65_bit_precision_and_original_support_are_retained() {
    use quest_qsp::offline::{OfflineBuilder, OfflinePolicy};
    use quest_qsp::{SynthesisAlgorithm, SynthesisPrecision};
    for algorithm in [
        SynthesisAlgorithm::RhwHalfCholesky,
        SynthesisAlgorithm::InverseNlftDivideConquer,
    ] {
        let target = Polynomial::new(
            Laurent::new(2),
            vec![Complex64::new(0.2, -0.0)],
            Limits::default(),
        )
        .unwrap();
        let solved = OfflineBuilder::new()
            .unit_circle_response(&target)
            .unwrap()
            .policy(OfflinePolicy {
                algorithm,
                initial_precision: 65,
                max_precision: 65,
                certification: CertificationPolicy {
                    initial_precision: 65,
                    max_precision: 65,
                    ..CertificationPolicy::default()
                },
                ..OfflinePolicy::default()
            })
            .unwrap()
            .solve()
            .unwrap();
        let bytes =
            quest_qsp::artifact::export_certified(solved.certified(), ArtifactLimits::default())
                .unwrap();
        let LoadedCertified::UnitCircleResponse(loaded) = load_certified(
            &bytes,
            LoadPolicy::default(),
            CertificationPolicy::default(),
        )
        .unwrap() else {
            panic!("mode")
        };
        assert_eq!(loaded.candidate().source_storage(), (2, 1));
        assert_eq!(loaded.candidate().algorithm(), algorithm);
        assert!(matches!(
            loaded.candidate().synthesis_precision(),
            SynthesisPrecision::Arbitrary { bits: 65 }
        ));
    }
}
