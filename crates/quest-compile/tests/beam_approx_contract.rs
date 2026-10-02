#![cfg(all(feature = "workers", target_os = "linux"))]
use googletest::{Result, prelude::*};
use quest_compile::optimizer::{Client, MitmResult, WorkerLimits};
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::{
    Angle, BoundAngleTarget, Control, ControlState, Gate, QuantumRegionBuilder, WorkerError,
};
use quest_math::Limits;
use quest_optimizer_protocol::MitmLimits;
use std::os::unix::fs::PermissionsExt;

struct Remove(std::path::PathBuf);
impl Drop for Remove {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn client_for(label: &str, outcome: &str) -> Result<(Remove, Client)> {
    let path = std::env::temp_dir().join(format!(
        "quest-approx-adapter-{label}-{}",
        std::process::id()
    ));
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{{\"version\":3,\"seed\":9,\"outcome\":{outcome}}}'\n"
        ),
    )?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    let client = Client::new(path.clone(), WorkerLimits::default())?;
    Ok((Remove(path), client))
}
fn controlled_rotation() -> Result<quest_compile::QuantumRegion> {
    let mut builder = QuantumRegionBuilder::new(2, 1)?;
    let target = builder.qubit(0)?;
    let control = builder.qubit(1)?;
    let bit = builder.bit(0)?;
    builder.gate(Gate::H, &[target], &[])?;
    builder.measure(control, bit)?;
    builder.gate(
        Gate::Rz(Angle::pi(-1, 2)?),
        &[target],
        &[Control::new(control, ControlState::Zero)],
    )?;
    Ok(builder.finish()?)
}

#[gtest]
fn approximate_mitm_lifts_full_phase_through_negative_control_after_effect_fence() -> Result<()> {
    let (_remove, client) = client_for(
        "candidate",
        r#"{"Candidate":{"sequence":{"qubits":1,"operations":[{"gate":"W","targets":[],"controls":[]},{"gate":"Sdg","targets":[0],"controls":[]}]},"engine":"phase-fixture","precision_bits":256}}"#,
    )?;
    let original = controlled_rotation()?;
    let before = original.schedule()[0..2].to_vec();
    let (candidate, report) = original.approx_mitm_candidate_from(
        2,
        &client,
        1e-10f64.to_bits(),
        9,
        MitmLimits::for_qubits(1)?,
        Limits::default(),
        5,
    )?;
    expect_eq!(report.candidate_window, Some((2, 3)));
    expect_eq!(
        report.target_identity,
        Some(BoundAngleTarget::RationalPi {
            numerator: (-1).into(),
            denominator: 2.into()
        })
    );
    expect_true!(report.local_work > 0);
    expect_eq!(&candidate.schedule()[0..2], before.as_slice());
    expect_eq!(candidate.schedule().len(), 4);
    let Some(MitmResult::Candidate(certificate)) = report.outcome else {
        return fail!("expected parent-certified approximate candidate");
    };
    expect_eq!(certificate.certificate.sequence().qubits, 2);
    for op in &certificate.certificate.sequence().operations {
        expect_eq!(op.controls.len(), 1);
        expect_eq!(op.controls[0].qubit, 1);
        expect_false!(op.controls[0].positive);
    }
    candidate.bind(&[])?.plan()?;
    Ok(())
}

#[gtest]
fn approximate_mitm_large_output_ceiling_reserves_only_reachable_depth() -> Result<()> {
    let (_remove, client) = client_for(
        "reachable-output",
        r#"{"Candidate":{"sequence":{"qubits":1,"operations":[{"gate":"W","targets":[],"controls":[]},{"gate":"Sdg","targets":[0],"controls":[]}]},"engine":"reachable-output-fixture","precision_bits":256}}"#,
    )?;
    let original = controlled_rotation()?;
    let (candidate, report) = original.approx_mitm_candidate_from(
        2,
        &client,
        1e-10f64.to_bits(),
        9,
        MitmLimits::for_qubits(1)?,
        Limits::default(),
        16_384,
    )?;
    expect_eq!(candidate.schedule().len(), 4);
    expect_true!(matches!(report.outcome, Some(MitmResult::Candidate(_))));
    Ok(())
}

#[gtest]
fn approximate_mitm_classifies_certified_overlimit_output_as_worker_rejection() -> Result<()> {
    let (_remove, client) = client_for(
        "overlimit-output",
        r#"{"Candidate":{"sequence":{"qubits":1,"operations":[{"gate":"W","targets":[],"controls":[]},{"gate":"Sdg","targets":[0],"controls":[]}]},"engine":"overlimit-output-fixture","precision_bits":256}}"#,
    )?;
    let original = controlled_rotation()?;
    let mut shallow = MitmLimits::for_qubits(1)?;
    shallow.max_depth = 1;
    expect_true!(matches!(
        original.clone().approx_mitm_candidate_from(
            2,
            &client,
            1e-10f64.to_bits(),
            9,
            shallow,
            Limits::default(),
            16_384,
        ),
        Err(WorkerError::RejectedOutput("approx MITM candidate depth"))
    ));
    expect_true!(matches!(
        original.approx_mitm_candidate_from(
            2,
            &client,
            1e-10f64.to_bits(),
            9,
            MitmLimits::for_qubits(1)?,
            Limits::default(),
            3,
        ),
        Err(WorkerError::RejectedOutput("approx MITM output operations"))
    ));
    Ok(())
}

#[gtest]
fn approximate_mitm_rejects_forged_worker_and_preserves_terminal_status() -> Result<()> {
    let original = controlled_rotation()?;
    let (_remove, forged) = client_for(
        "forged",
        r#"{"Candidate":{"sequence":{"qubits":1,"operations":[]},"engine":"forged","precision_bits":4096}}"#,
    )?;
    expect_true!(
        original
            .clone()
            .approx_mitm_candidate_from(
                2,
                &forged,
                1e-12f64.to_bits(),
                9,
                MitmLimits::for_qubits(1)?,
                Limits::default(),
                5,
            )
            .is_err()
    );
    let (_remove, incomplete) = client_for(
        "incomplete",
        r#"{"Incomplete":{"reason":"states","explored":17}}"#,
    )?;
    let prior = original.schedule().to_vec();
    let (unchanged, report) = original.approx_mitm_candidate_from(
        2,
        &incomplete,
        1e-12f64.to_bits(),
        9,
        MitmLimits::for_qubits(1)?,
        Limits::default(),
        5,
    )?;
    expect_eq!(unchanged.schedule(), prior.as_slice());
    expect_eq!(report.candidate_window, Some((2, 3)));
    expect_true!(matches!(
        report.outcome,
        Some(MitmResult::Incomplete { explored: 17, .. })
    ));
    Ok(())
}

#[gtest]
fn approximate_mitm_retains_other_typed_terminal_outcomes_without_publication() -> Result<()> {
    let original = controlled_rotation()?;
    let prior = original.schedule().to_vec();
    for (label, wire) in [
        ("none", r#"{"NoCandidate":{"explored":3}}"#),
        ("exhausted", r#"{"Exhausted":{"explored":4}}"#),
        (
            "unresolved",
            r#"{"Unresolved":{"precision_bits":512,"explored":5}}"#,
        ),
    ] {
        let (_remove, client) = client_for(label, wire)?;
        let (unchanged, report) = original.clone().approx_mitm_candidate_from(
            2,
            &client,
            1e-12f64.to_bits(),
            9,
            MitmLimits::for_qubits(1)?,
            Limits::default(),
            5,
        )?;
        expect_eq!(unchanged.schedule(), prior.as_slice());
        expect_eq!(report.candidate_window, Some((2, 3)));
        match label {
            "none" => expect_true!(matches!(
                report.outcome,
                Some(MitmResult::NoCandidate { explored: 3 })
            )),
            "exhausted" => expect_true!(matches!(
                report.outcome,
                Some(MitmResult::Exhausted { explored: 4 })
            )),
            _ => expect_true!(matches!(
                report.outcome,
                Some(MitmResult::Unresolved {
                    precision_bits: 512,
                    explored: 5
                })
            )),
        }
    }
    Ok(())
}

#[gtest]
fn approximate_mitm_admits_request_limits_before_any_worker_call() -> Result<()> {
    let original = controlled_rotation()?;
    let client = Client::new("/nonexistent-worker", WorkerLimits::default())?;
    let mut low_work = MitmLimits::for_qubits(1)?;
    low_work.max_work = 1_000;
    expect_true!(
        original
            .clone()
            .approx_mitm_candidate_from(
                2,
                &client,
                1e-12f64.to_bits(),
                9,
                low_work,
                Limits::default(),
                5,
            )
            .is_err()
    );
    expect_true!(
        original
            .clone()
            .approx_mitm_candidate_from(
                3,
                &client,
                f64::NAN.to_bits(),
                9,
                MitmLimits::for_qubits(1)?,
                Limits::default(),
                5,
            )
            .is_err()
    );
    expect_true!(
        original
            .clone()
            .approx_mitm_candidate_from(
                3,
                &client,
                1e-12f64.to_bits(),
                9,
                MitmLimits::for_qubits(1)?,
                Limits::default(),
                2,
            )
            .is_err()
    );
    expect_true!(
        original
            .approx_mitm_candidate_from(
                4,
                &client,
                1e-12f64.to_bits(),
                9,
                MitmLimits::for_qubits(1)?,
                Limits::default(),
                5,
            )
            .is_err()
    );
    Ok(())
}
