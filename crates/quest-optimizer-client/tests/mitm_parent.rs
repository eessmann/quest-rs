#![cfg(target_os = "linux")]
use googletest::{Result, prelude::*};
use quest_math::{AngleTarget, Axis, Gate, Limits, Operation, Sequence, Target};
use quest_optimizer_client::{Client, Error, MitmResult, WorkerLimits};
use quest_optimizer_protocol::MitmLimits;
use std::{fs, os::unix::fs::PermissionsExt};

fn client_for(outcome: &str) -> Result<(tempfile::TempDir, Client)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("worker");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{{\"version\":2,\"seed\":42,\"outcome\":{outcome}}}'\n"
        ),
    )?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok((dir, Client::new(path, WorkerLimits::default())?))
}
fn x() -> Sequence {
    Sequence {
        qubits: 1,
        operations: vec![Operation {
            gate: Gate::X,
            targets: vec![0],
            controls: vec![],
        }],
    }
}

#[gtest]
fn parent_rejects_forged_exact_candidate_and_preserves_incomplete() -> Result<()> {
    let (_dir, client) = client_for(
        r#"{"Candidate":{"sequence":{"qubits":1,"operations":[]},"engine":"forged","precision_bits":0}}"#,
    )?;
    let limits = MitmLimits::for_qubits(1)?;
    expect_true!(matches!(
        client.exact_mitm(&x(), 42, limits, Limits::default()),
        Err(Error::Verification(_))
    ));
    let (_dir, client) = client_for(r#"{"Incomplete":{"reason":"states","explored":17}}"#)?;
    expect_true!(matches!(
        client.exact_mitm(&x(), 42, limits, Limits::default())?,
        MitmResult::Incomplete { explored: 17, .. }
    ));
    Ok(())
}

#[gtest]
fn parent_approx_certificate_uses_original_target() -> Result<()> {
    let (_dir, client) = client_for(
        r#"{"Candidate":{"sequence":{"qubits":1,"operations":[]},"engine":"forged","precision_bits":4096}}"#,
    )?;
    let target = Target {
        axis: Axis::Z,
        angle: AngleTarget::RationalPi {
            numerator: 1.into(),
            denominator: 1.into(),
        },
    };
    let limits = MitmLimits::for_qubits(1)?;
    expect_true!(matches!(
        client.approx_mitm(&target, 1e-12f64.to_bits(), 42, limits, Limits::default()),
        Err(Error::Verification(_))
    ));
    Ok(())
}
