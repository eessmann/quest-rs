#![cfg(target_os = "linux")]
use googletest::prelude::*;
use quest_optimizer_client::{Client, Error, WorkerLimits};
use quest_optimizer_protocol::{Outcome, Request};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

fn script(body: &str) -> Result<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("worker");
    fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok((dir, path))
}

#[gtest]
fn rejects_malformed_output_and_wrong_seed() -> Result<()> {
    let (_dir, path) = script("cat >/dev/null; printf broken")?;
    let client = Client::new(path, WorkerLimits::default())?;
    verify_that!(client.request(Request::Capabilities, 42), err(anything()))?;
    let (_dir, path) = script(
        "cat >/dev/null; printf '%s' '{\"version\":1,\"seed\":0,\"outcome\":{\"Capabilities\":{\"synthesis\":false,\"zx\":false}}}'",
    )?;
    let client = Client::new(path, WorkerLimits::default())?;
    verify_that!(
        matches!(
            client.request(Request::Capabilities, 42),
            Err(Error::Envelope)
        ),
        eq(true)
    )?;
    Ok(())
}

#[gtest]
fn timeout_and_output_limit_kill_the_process_group() -> Result<()> {
    let (_dir, path) = script("exec sleep 10")?;
    let client = Client::new(
        path,
        WorkerLimits {
            wall_time: Duration::from_millis(80),
            ..WorkerLimits::default()
        },
    )?;
    verify_that!(
        matches!(
            client.request(Request::Capabilities, 0),
            Err(Error::Timeout)
        ),
        eq(true)
    )?;
    let (_dir, path) = script("exec yes X")?;
    let client = Client::new(path, WorkerLimits::default())?;
    verify_that!(
        matches!(
            client.request(Request::Capabilities, 0),
            Err(Error::OutputLimit)
        ),
        eq(true)
    )?;
    Ok(())
}

#[gtest]
fn memory_limit_is_applied_before_worker_execution() -> Result<()> {
    let (_dir, path) = script("exec /usr/bin/python3 -c 'a=bytearray(700*1024*1024)'")?;
    let client = Client::new(path, WorkerLimits::default())?;
    verify_that!(
        matches!(
            client.request(Request::Capabilities, 0),
            Err(Error::Failed { .. })
        ),
        eq(true)
    )?;
    Ok(())
}

#[gtest]
fn bounded_valid_response_and_capability_rejection() -> Result<()> {
    let (_dir, path) = script(
        "cat >/dev/null; printf '%s' '{\"version\":1,\"seed\":42,\"outcome\":{\"Capabilities\":{\"synthesis\":false,\"zx\":false}}}'",
    )?;
    let client = Client::new(path, WorkerLimits::default())?;
    verify_that!(
        matches!(
            client.request(Request::Capabilities, 42)?,
            Outcome::Capabilities {
                synthesis: false,
                zx: false
            }
        ),
        eq(true)
    )?;
    verify_that!(
        Client::new(
            "missing",
            WorkerLimits {
                memory_bytes: usize::MAX,
                ..WorkerLimits::default()
            }
        ),
        err(anything())
    )?;
    Ok(())
}

#[gtest]
fn well_formed_candidates_still_need_independent_mathematical_admission() -> Result<()> {
    let body = "cat >/dev/null; printf '%s' '{\"version\":1,\"seed\":42,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[]},\"engine\":\"untrusted\",\"precision_bits\":1024}}}'";
    let (_dir, path) = script(body)?;
    let client = Client::new(path, WorkerLimits::default())?;
    let target = quest_math::Target {
        axis: quest_math::Axis::Z,
        angle: quest_math::AngleTarget::RationalPi {
            numerator: 1.into(),
            denominator: 1.into(),
        },
    };
    verify_that!(
        matches!(
            client.synthesize(&target, 1e-12, 42, quest_math::Limits::default()),
            Err(Error::Verification(_))
        ),
        eq(true)
    )?;
    let original = quest_math::Sequence {
        qubits: 1,
        operations: vec![quest_math::Operation {
            gate: quest_math::Gate::X,
            targets: vec![0],
            controls: vec![],
        }],
    };
    verify_that!(
        matches!(
            client.optimize_zx(&original, 42, quest_math::Limits::default()),
            Err(Error::Verification(_))
        ),
        eq(true)
    )?;
    Ok(())
}

#[gtest]
fn exited_leader_with_inherited_pipes_keeps_cleanup_ownership() -> Result<()> {
    let (dir, path) = script("cat >/dev/null; (sleep 0.3; touch \"$0.marker\") & exit 0")?;
    let marker = dir.path().join("worker.marker");
    let client = Client::new(
        path,
        WorkerLimits {
            wall_time: Duration::from_millis(80),
            ..WorkerLimits::default()
        },
    )?;
    verify_that!(
        matches!(
            client.request(Request::Capabilities, 0),
            Err(Error::Timeout)
        ),
        eq(true)
    )?;
    std::thread::sleep(Duration::from_millis(400));
    verify_that!(marker.exists(), eq(false))?;
    Ok(())
}
