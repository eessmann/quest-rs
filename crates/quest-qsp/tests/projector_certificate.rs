#![cfg_attr(
    feature = "certification",
    expect(
        clippy::panic_in_result_fn,
        reason = "Test assertions validate independently reconstructed bounds"
    )
)]
#![cfg(feature = "certification")]
use quest_polynomial::{Chebyshev, Limits, Polynomial};
use quest_qsp::certification::{CertificationBuilder, CertificationPolicy};
use quest_qsp::{Complex64, SynthesisBuilder};
#[test]
fn actual_projector_export_is_independently_certified_for_both_parities()
-> Result<(), Box<dyn std::error::Error>> {
    for coefficients in [
        vec![0.31],
        vec![0.0, 0.7],
        vec![-0.1, 0.0, 0.2],
        vec![0.0, 0.14, 0.0, -0.13],
    ] {
        let target = Polynomial::new(
            Chebyshev,
            coefficients
                .iter()
                .map(|&v| Complex64::new(v, 0.0))
                .collect(),
            Limits::default(),
        )?;
        let frozen = SynthesisBuilder::new()
            .real_parity_wx(&target)?
            .admit()?
            .complete()?
            .synthesize()?;
        let source = CertificationBuilder::new()
            .candidate(frozen)
            .policy(CertificationPolicy::default())?
            .certify()?;
        let converted = source.certify_projector_phases(CertificationPolicy::default())?;
        assert!(converted.response_bound().upper_f64() < 1e-11);
        assert!(converted.unitarity_bound().upper_f64() < 1e-11);
        assert_eq!(converted.values().len(), coefficients.len());
        assert_eq!(converted.source_coefficients(), target.coefficients());
    }
    Ok(())
}

#[test]
fn projector_certificate_applies_its_own_tolerance_and_budgets()
-> Result<(), Box<dyn std::error::Error>> {
    use quest_qsp::certification::CertificationError;
    let target = Polynomial::new(
        Chebyshev,
        vec![Complex64::new(0.0, 0.0), Complex64::new(0.7, 0.0)],
        Limits::default(),
    )?;
    let frozen = SynthesisBuilder::new()
        .real_parity_wx(&target)?
        .admit()?
        .complete()?
        .synthesize()?;
    let source = CertificationBuilder::new()
        .candidate(frozen)
        .policy(CertificationPolicy::default())?
        .certify()?;
    assert!(matches!(
        source.certify_projector_phases(CertificationPolicy {
            max_work: 1,
            ..CertificationPolicy::default()
        }),
        Err(CertificationError::Budget(_))
    ));
    assert!(matches!(
        source.certify_projector_phases(CertificationPolicy {
            response_tolerance: 1e-30,
            ..CertificationPolicy::default()
        }),
        Err(CertificationError::ProjectorViolation { .. })
    ));
    Ok(())
}
