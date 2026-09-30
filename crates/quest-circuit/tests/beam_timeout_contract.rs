#![cfg(all(feature = "workers", target_os = "linux"))]
use googletest::{Result, prelude::*};
#[allow(unused_imports)]
use quest_circuit::prelude::*;
use quest_circuit::*;
use std::{os::unix::fs::PermissionsExt, time::Duration};

struct Script(std::path::PathBuf);
impl Drop for Script {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[gtest]
fn worker_timeout_discards_request_and_preserves_prior_exact_improvement() -> Result<()> {
    let path = std::env::temp_dir().join(format!("quest-beam-timeout-{}.sh", std::process::id()));
    std::fs::write(&path, "#!/bin/sh\ncat >/dev/null\nsleep 2\n")?;
    let script = Script(path);
    std::fs::set_permissions(&script.0, std::fs::Permissions::from_mode(0o700))?;
    let client = optimizer::Client::new(
        script.0.clone(),
        optimizer::WorkerLimits {
            wall_time: Duration::from_millis(100),
            ..optimizer::WorkerLimits::default()
        },
    )?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let q = builder.qubit(0)?;
    builder.gate(Gate::H, &[q], &[])?;
    builder.gate(Gate::H, &[q], &[])?;
    let program = builder.finish()?;
    let original = program.snapshot_id();
    let options = OptimizationOptions::new(
        OptimizationTarget::once(
            DeploymentSnapshot::new(DeploymentKind::StateVector, 1, false, false, false, 0, 1, 2)?,
            CostProfile::CliffordTV1,
        )?,
        OptimizationLimits::default(),
        ApproximationMode::Disabled,
    )?;
    let outcome = Optimizer::from_region(program, &[], options)?.search_with_workers(
        BeamOptions::new(1, 1, 16, 1)?,
        &client,
        0,
        certified::Limits::default(),
    )?;
    expect_eq!(outcome.stop_reason(), StopReason::Timeout);
    expect_eq!(outcome.evidence(), OptimizationEvidence::ExactVerified);
    let report = outcome.search_report().ok_or(Error::InvalidId)?;
    expect_true!(report.exact_regions().is_empty());
    let OptimizerInput::Region { source, bound } = outcome.into_input() else {
        return fail!("expected ideal input");
    };
    expect_eq!(source.snapshot_id(), original);
    expect_true!(bound.instructions().is_empty());
    bound.plan()?;
    Ok(())
}
