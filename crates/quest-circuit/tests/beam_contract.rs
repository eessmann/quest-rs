use googletest::{Result, prelude::*};
#[allow(unused_imports)]
use quest_circuit::prelude::*;
use quest_circuit::{
    ApproximationMode, BeamOptions, CostProfile, DeploymentKind, DeploymentSnapshot, Gate,
    OptimizationEvidence, OptimizationLimits, OptimizationOptions, OptimizationTarget, Optimizer,
    OptimizerInput, QuantumRegionBuilder,
};

fn options(width: usize) -> quest_circuit::Result<OptimizationOptions> {
    let amplitudes = 1u64
        .checked_shl(u32::try_from(width).map_err(|_| quest_circuit::Error::Budget("test width"))?)
        .ok_or(quest_circuit::Error::Budget("test width"))?;
    let deployment = DeploymentSnapshot::new(
        DeploymentKind::StateVector,
        width,
        false,
        false,
        false,
        0,
        1,
        amplitudes,
    )?;
    OptimizationOptions::new(
        OptimizationTarget::once(deployment, CostProfile::CliffordTV1)?,
        OptimizationLimits::default(),
        ApproximationMode::Disabled,
    )
}

#[gtest]
fn deterministic_beam_publishes_verified_exact_improvement() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let q = builder.qubit(0)?;
    builder.gate(Gate::H, &[q], &[])?;
    builder.gate(Gate::H, &[q], &[])?;
    let optimizer = Optimizer::from_region(builder.finish()?, &[], options(1)?)?;
    let outcome = optimizer.search(BeamOptions::default())?;
    expect_eq!(outcome.evidence(), OptimizationEvidence::ExactVerified);
    expect_gt!(
        outcome
            .search_report()
            .ok_or(quest_circuit::Error::InvalidId)?
            .generated(),
        0
    );
    let OptimizerInput::Region { bound, .. } = outcome.into_input() else {
        return Err(quest_circuit::Error::InvalidId.into());
    };
    expect_eq!(bound.instructions().len(), 0);
    Ok(())
}

#[gtest]
fn beam_admits_replayed_longer_linear_candidate_before_scoring() -> Result<()> {
    use quest_circuit::{Control, ControlState};
    let mut builder = QuantumRegionBuilder::new(3, 0)?;
    for (control, target) in [(0, 1), (1, 2), (2, 0)] {
        builder.gate(
            Gate::X,
            &[builder.qubit(target)?],
            &[Control::new(builder.qubit(control)?, ControlState::One)],
        )?;
    }
    let outcome = Optimizer::from_region(builder.finish()?, &[], options(3)?)?
        .search(BeamOptions::default())?;
    let report = outcome
        .search_report()
        .ok_or(quest_circuit::Error::InvalidId)?;
    expect_gt!(report.maximum_frontier_operations(), 3);
    expect_gt!(report.admitted(), 0);
    Ok(())
}

#[gtest]
fn ideal_source_remains_original_binding_authority() -> Result<()> {
    use quest_circuit::Angle;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let p = builder.parameter("p")?;
    let angle = Angle::parameter(p)?;
    builder.gate(Gate::Rx(angle.clone()), &[builder.qubit(0)?], &[])?;
    builder.gate(Gate::Rx(angle.negated()?), &[builder.qubit(0)?], &[])?;
    let original = builder.finish()?;
    let original_snapshot = original.snapshot_id();
    let outcome = Optimizer::from_region(original, &[(p, 0.0)], options(1)?)?
        .search(BeamOptions::default())?;
    let OptimizerInput::Region { source, bound } = outcome.into_input() else {
        return Err(quest_circuit::Error::InvalidId.into());
    };
    expect_eq!(source.snapshot_id(), original_snapshot);
    expect_eq!(source.schedule().len(), 2);
    expect_eq!(bound.instructions().len(), 0);
    Ok(())
}

#[gtest]
fn bound_entry_rewrites_discrete_clifford_window() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
    builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
    let original = builder.finish()?.bind(&[])?;
    let source_snapshot = original.source_snapshot_id();
    let outcome = Optimizer::from_bound(original, options(1)?)?.search(BeamOptions::default())?;
    expect_eq!(outcome.evidence(), OptimizationEvidence::ExactVerified);
    let OptimizerInput::Bound(bound) = outcome.into_input() else {
        return Err(quest_circuit::Error::InvalidId.into());
    };
    expect_eq!(bound.source_snapshot_id(), source_snapshot);
    expect_eq!(bound.instructions().len(), 0);
    Ok(())
}

#[gtest]
fn bound_entry_uses_exact_phase_targets() -> Result<()> {
    use quest_circuit::Angle;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::Rz(Angle::pi(1, 2)?), &[builder.qubit(0)?], &[])?;
    builder.gate(Gate::Rz(Angle::pi(-1, 2)?), &[builder.qubit(0)?], &[])?;
    let original = builder.finish()?.bind(&[])?;
    let outcome = Optimizer::from_bound(original, options(1)?)?.search(BeamOptions::default())?;
    let OptimizerInput::Bound(bound) = outcome.into_input() else {
        return Err(quest_circuit::Error::InvalidId.into());
    };
    expect_eq!(bound.instructions().len(), 0);
    Ok(())
}

#[gtest]
fn bound_work_exhaustion_keeps_original_snapshot() -> Result<()> {
    use quest_circuit::StopReason;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
    builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
    let original = builder.finish()?.bind(&[])?;
    let snapshot = original.snapshot_id();
    let base = options(1)?;
    let limited = OptimizationOptions::new(
        base.target().clone(),
        OptimizationLimits::new(5_000, 256 * 1024 * 1024)?,
        ApproximationMode::Disabled,
    )?;
    let outcome = Optimizer::from_bound(original, limited)?.search(BeamOptions::default())?;
    expect_eq!(outcome.stop_reason(), StopReason::WorkLimit);
    expect_eq!(
        outcome.snapshot_id(),
        quest_circuit::OptimizerSnapshot::Bound(snapshot)
    );
    Ok(())
}

#[gtest]
fn beam_reports_round_and_candidate_caps_without_losing_best() -> Result<()> {
    use quest_circuit::StopReason;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
    builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
    let ideal = builder.finish()?;
    let round = Optimizer::from_region(ideal.clone(), &[], options(1)?)?
        .search(BeamOptions::new(1, 1, 128, 1)?)?;
    expect_eq!(round.stop_reason(), StopReason::RoundLimit);
    expect_eq!(round.evidence(), OptimizationEvidence::ExactVerified);
    let candidate =
        Optimizer::from_region(ideal, &[], options(1)?)?.search(BeamOptions::new(1, 8, 1, 1)?)?;
    expect_eq!(candidate.stop_reason(), StopReason::CandidateLimit);
    expect_eq!(candidate.evidence(), OptimizationEvidence::ExactVerified);
    Ok(())
}

#[gtest]
fn tied_rewrites_deduplicate_and_search_terminates_deterministically() -> Result<()> {
    use quest_circuit::StopReason;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let q = builder.qubit(0)?;
    builder.gate(Gate::H, &[q], &[])?;
    builder.gate(Gate::H, &[q], &[])?;
    let ideal = builder.finish()?;
    let first =
        Optimizer::from_region(ideal.clone(), &[], options(1)?)?.search(BeamOptions::default())?;
    let second = Optimizer::from_region(ideal, &[], options(1)?)?.search(BeamOptions::default())?;
    let first_report = first
        .search_report()
        .ok_or(quest_circuit::Error::InvalidId)?;
    let second_report = second
        .search_report()
        .ok_or(quest_circuit::Error::InvalidId)?;
    expect_eq!(first.stop_reason(), StopReason::Complete);
    expect_eq!(second.stop_reason(), StopReason::Complete);
    expect_eq!(first_report.rounds(), second_report.rounds());
    expect_eq!(first_report.generated(), second_report.generated());
    expect_lt!(first_report.rounds(), BeamOptions::default().rounds());
    let OptimizerInput::Region {
        bound: first_bound, ..
    } = first.into_input()
    else {
        return Err(quest_circuit::Error::InvalidId.into());
    };
    let OptimizerInput::Region {
        bound: second_bound,
        ..
    } = second.into_input()
    else {
        return Err(quest_circuit::Error::InvalidId.into());
    };
    expect_eq!(first_bound.instructions().len(), 0);
    expect_eq!(second_bound.instructions().len(), 0);
    Ok(())
}

#[cfg(all(feature = "workers", target_os = "linux"))]
fn worker_script(body: &str) -> Result<(std::path::PathBuf, quest_circuit::optimizer::Client)> {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT_SCRIPT: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "quest-beam-worker-{}-{}-{}",
        std::process::id(),
        body.len(),
        NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::write(&path, body)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    let client = quest_circuit::optimizer::Client::new(
        path.clone(),
        quest_circuit::optimizer::WorkerLimits::default(),
    )?;
    Ok((path, client))
}

#[cfg(all(feature = "workers", target_os = "linux"))]
fn worker_input() -> quest_circuit::Result<quest_circuit::QuantumRegion> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let q = builder.qubit(0)?;
    builder.gate(Gate::H, &[q], &[])?;
    builder.gate(Gate::X, &[q], &[])?;
    builder.gate(Gate::H, &[q], &[])?;
    builder.finish()
}

#[cfg(all(feature = "workers", target_os = "linux"))]
#[gtest]
fn worker_search_retains_exact_certificate_and_original_authority() -> Result<()> {
    let (path, client) = worker_script(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":2,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"Z\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"beam-fixture\",\"precision_bits\":256}}}'\n",
    )?;
    let original = worker_input()?;
    let snapshot = original.snapshot_id();
    let result = Optimizer::from_region(original, &[], options(1)?)?.search_with_workers(
        BeamOptions::new(1, 1, 16, 1)?,
        &client,
        0,
        quest_math::Limits::default(),
    );
    let _ = std::fs::remove_file(path);
    let outcome = result?;
    expect_ge!(
        outcome.budget().retained_bytes,
        u64::try_from(quest_math::Limits::default().bytes)?,
    );
    let report = outcome
        .search_report()
        .ok_or(quest_circuit::Error::InvalidId)?;
    expect_eq!(report.worker_requests(), 1);
    expect_eq!(report.exact_regions().len(), 1);
    let OptimizerInput::Region { source, bound } = outcome.into_input() else {
        return Err(quest_circuit::Error::InvalidId.into());
    };
    expect_eq!(source.snapshot_id(), snapshot);
    expect_eq!(bound.instructions().len(), 1);
    Ok(())
}

#[cfg(all(feature = "workers", target_os = "linux"))]
#[gtest]
fn bound_worker_search_uses_discrete_source_and_preserves_bound_authority() -> Result<()> {
    let (path, client) = worker_script(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":2,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"Z\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"beam-bound-fixture\",\"precision_bits\":256}}}'\n",
    )?;
    let original = worker_input()?.bind(&[])?;
    let source_snapshot = original.source_snapshot_id();
    let result = Optimizer::from_bound(original, options(1)?)?.search_with_workers(
        BeamOptions::new(1, 1, 16, 1)?,
        &client,
        0,
        quest_math::Limits::default(),
    );
    let _ = std::fs::remove_file(path);
    let outcome = result?;
    let report = outcome
        .search_report()
        .ok_or(quest_circuit::Error::InvalidId)?;
    expect_eq!(report.worker_requests(), 1);
    expect_eq!(report.exact_regions().len(), 1);
    let OptimizerInput::Bound(bound) = outcome.into_input() else {
        return Err(quest_circuit::Error::InvalidId.into());
    };
    expect_eq!(bound.source_snapshot_id(), source_snapshot);
    expect_eq!(bound.instructions().len(), 1);
    Ok(())
}

#[cfg(all(feature = "workers", target_os = "linux"))]
#[gtest]
fn synthesis_keeps_local_certificate_and_exact_cumulative_allowance() -> Result<()> {
    use quest_circuit::{Angle, BigRational};
    let (path, client) = worker_script(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":2,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[]},\"engine\":\"beam-approx-fixture\",\"precision_bits\":256}}}'\n",
    )?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::Rx(Angle::radians(0.1)?), &[builder.qubit(0)?], &[])?;
    let base = options(1)?;
    let budget = BigRational::new(1.into(), 2.into());
    let configured = OptimizationOptions::new(
        base.target().clone(),
        OptimizationLimits::default(),
        ApproximationMode::local(budget.clone())?,
    )?;
    let result = Optimizer::from_region(builder.finish()?, &[], configured)?.search_with_workers(
        BeamOptions::new(1, 1, 16, 1)?,
        &client,
        0,
        quest_math::Limits::default(),
    );
    let _ = std::fs::remove_file(path);
    let outcome = result?;
    let report = outcome
        .search_report()
        .ok_or(quest_circuit::Error::InvalidId)?;
    expect_eq!(outcome.evidence(), OptimizationEvidence::LocalCertified);
    expect_eq!(report.local_rotations().len(), 1);
    expect_true!(
        report
            .cumulative_error()
            .is_some_and(|value| value <= &budget)
    );
    let OptimizerInput::Region { bound, .. } = outcome.into_input() else {
        return Err(quest_circuit::Error::InvalidId.into());
    };
    expect_eq!(bound.instructions().len(), 0);
    Ok(())
}

#[cfg(all(feature = "workers", target_os = "linux"))]
#[gtest]
fn rational_pi_rotation_does_not_spend_exact_mitm_slot_before_synthesis() -> Result<()> {
    use quest_circuit::{Angle, BigRational};
    let (path, client) = worker_script(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":2,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[]},\"engine\":\"beam-rational-synthesis\",\"precision_bits\":256}}}'\n",
    )?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::Rx(Angle::pi(1, 100)?), &[builder.qubit(0)?], &[])?;
    let base = options(1)?;
    let configured = OptimizationOptions::new(
        base.target().clone(),
        OptimizationLimits::default(),
        ApproximationMode::local(BigRational::new(1.into(), 2.into()))?,
    )?;
    let result = Optimizer::from_region(builder.finish()?, &[], configured)?.search_with_workers(
        BeamOptions::new(1, 1, 16, 1)?,
        &client,
        0,
        quest_math::Limits::default(),
    );
    let _ = std::fs::remove_file(path);
    let outcome = result?;
    let report = outcome
        .search_report()
        .ok_or(quest_circuit::Error::InvalidId)?;
    expect_eq!(report.worker_requests(), 1);
    expect_eq!(report.local_rotations().len(), 1);
    expect_eq!(outcome.evidence(), OptimizationEvidence::LocalCertified);
    Ok(())
}

#[cfg(all(feature = "workers", target_os = "linux"))]
#[gtest]
fn beam_keeps_parent_certified_exact_mitm_region() -> Result<()> {
    use quest_circuit::BeamMitmStatus;
    let (path, client) = worker_script(
        "#!/bin/sh\ninput=$(cat)\ncase \"$input\" in *ExactMitm*) printf '%s' '{\"version\":2,\"seed\":1,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"Z\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"beam-mitm\",\"precision_bits\":256}}}' ;; *) printf '%s' '{\"version\":2,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"H\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]},{\"gate\":\"H\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"beam-zx-nochange\",\"precision_bits\":256}}}' ;; esac\n",
    )?;
    let result = Optimizer::from_region(worker_input()?, &[], options(1)?)?.search_with_workers(
        BeamOptions::new(1, 1, 16, 2)?,
        &client,
        0,
        quest_math::Limits::default(),
    );
    let _ = std::fs::remove_file(path);
    let outcome = result?;
    let report = outcome
        .search_report()
        .ok_or(quest_circuit::Error::InvalidId)?;
    expect_eq!(report.worker_requests(), 2);
    expect_true!(matches!(
        report.exact_mitm_statuses(),
        [BeamMitmStatus::Candidate { .. }]
    ));
    expect_eq!(report.exact_regions().len(), 1);
    let OptimizerInput::Region { bound, .. } = outcome.into_input() else {
        return Err(quest_circuit::Error::InvalidId.into());
    };
    expect_eq!(bound.instructions().len(), 1);
    Ok(())
}

#[cfg(all(feature = "workers", target_os = "linux"))]
#[gtest]
fn pure_phase_window_reaches_exact_mitm_after_zx() -> Result<()> {
    use quest_circuit::{Angle, BeamMitmStatus};
    let (path, client) = worker_script(
        "#!/bin/sh\ninput=$(cat)\ncase \"$input\" in *ExactMitm*) seed=1 ;; *) seed=0 ;; esac\nprintf '{\"version\":2,\"seed\":%s,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"T\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"beam-phase\",\"precision_bits\":256}}}' \"$seed\"\n",
    )?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::Phase(Angle::pi(1, 4)?), &[builder.qubit(0)?], &[])?;
    let result = Optimizer::from_region(builder.finish()?, &[], options(1)?)?.search_with_workers(
        BeamOptions::new(1, 1, 16, 2)?,
        &client,
        0,
        quest_math::Limits::default(),
    );
    let _ = std::fs::remove_file(path);
    let outcome = result?;
    expect_true!(matches!(
        outcome
            .search_report()
            .ok_or(quest_circuit::Error::InvalidId)?
            .exact_mitm_statuses(),
        [BeamMitmStatus::Candidate { .. }]
    ));
    Ok(())
}

#[cfg(all(feature = "workers", target_os = "linux"))]
#[gtest]
fn beam_keeps_parent_certified_approx_mitm_region_and_rational_budget() -> Result<()> {
    use quest_circuit::{Angle, BeamMitmStatus, BigRational};
    let (path, client) = worker_script(
        "#!/bin/sh\ninput=$(cat)\ncase \"$input\" in *ApproxMitm*) printf '%s' '{\"version\":2,\"seed\":1,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[]},\"engine\":\"beam-approx-mitm\",\"precision_bits\":256}}}' ;; *) printf '%s' '{\"version\":2,\"seed\":0,\"outcome\":{\"Failure\":{\"code\":\"declined\",\"message\":\"use MITM\"}}}' ;; esac\n",
    )?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::Rx(Angle::radians(0.1)?), &[builder.qubit(0)?], &[])?;
    let budget = BigRational::new(1.into(), 2.into());
    let base = options(1)?;
    let configured = OptimizationOptions::new(
        base.target().clone(),
        OptimizationLimits::default(),
        ApproximationMode::local(budget.clone())?,
    )?;
    let result = Optimizer::from_region(builder.finish()?, &[], configured)?.search_with_workers(
        BeamOptions::new(1, 1, 16, 2)?,
        &client,
        0,
        quest_math::Limits::default(),
    );
    let _ = std::fs::remove_file(path);
    let outcome = result?;
    let report = outcome
        .search_report()
        .ok_or(quest_circuit::Error::InvalidId)?;
    expect_eq!(outcome.evidence(), OptimizationEvidence::LocalCertified);
    expect_true!(matches!(
        report.approx_mitm_statuses(),
        [BeamMitmStatus::Candidate { .. }]
    ));
    expect_eq!(report.approx_regions().len(), 1);
    expect_true!(
        report
            .cumulative_error()
            .is_some_and(|value| value <= &budget)
    );
    let OptimizerInput::Region { bound, .. } = outcome.into_input() else {
        return Err(quest_circuit::Error::InvalidId.into());
    };
    expect_eq!(bound.instructions().len(), 0);
    Ok(())
}

#[cfg(all(feature = "workers", target_os = "linux"))]
#[gtest]
fn approximation_across_rounds_spends_one_exact_budget_against_original() -> Result<()> {
    use quest_circuit::{Angle, BigRational};
    let (path, client) = worker_script(
        "#!/bin/sh\ninput=$(cat)\nseed=$(printf '%s' \"$input\" | sed -n 's/.*\"seed\":\\([0-9]*\\).*/\\1/p')\ncase \"$input\" in *ApproxMitm*) printf '{\"version\":2,\"seed\":%s,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[]},\"engine\":\"beam-two-round\",\"precision_bits\":256}}}' \"$seed\" ;; *) printf '{\"version\":2,\"seed\":%s,\"outcome\":{\"Failure\":{\"code\":\"declined\",\"message\":\"use MITM\"}}}' \"$seed\" ;; esac\n",
    )?;
    let mut builder = QuantumRegionBuilder::new(1, 1)?;
    let q = builder.qubit(0)?;
    let bit = builder.bit(0)?;
    builder.gate(Gate::Rx(Angle::radians(0.1)?), &[q], &[])?;
    builder.measure(q, bit)?;
    builder.gate(Gate::Rx(Angle::radians(0.1)?), &[q], &[])?;
    let budget = BigRational::from_integer(1.into());
    let base = options(1)?;
    let configured = OptimizationOptions::new(
        base.target().clone(),
        OptimizationLimits::default(),
        ApproximationMode::local(budget.clone())?,
    )?;
    let result = Optimizer::from_region(builder.finish()?, &[], configured)?.search_with_workers(
        BeamOptions::new(1, 3, 32, 4)?,
        &client,
        0,
        quest_math::Limits::default(),
    );
    let _ = std::fs::remove_file(path);
    let outcome = result?;
    let report = outcome
        .search_report()
        .ok_or(quest_circuit::Error::InvalidId)?;
    expect_eq!(outcome.evidence(), OptimizationEvidence::LocalCertified);
    expect_eq!(report.approx_regions().len(), 2);
    expect_true!(
        report
            .cumulative_error()
            .is_some_and(|value| value <= &budget)
    );
    let OptimizerInput::Region { bound, .. } = outcome.into_input() else {
        return Err(quest_circuit::Error::InvalidId.into());
    };
    expect_eq!(bound.instructions().len(), 1);
    Ok(())
}

#[cfg(all(feature = "workers", target_os = "linux"))]
#[gtest]
fn mitm_shortlist_exhaustion_is_incomplete_even_with_original_best() -> Result<()> {
    use quest_circuit::{Angle, BeamMitmStatus, BigRational, StopReason};
    let (path, client) = worker_script(
        "#!/bin/sh\ninput=$(cat)\ncase \"$input\" in *ApproxMitm*) printf '%s' '{\"version\":2,\"seed\":1,\"outcome\":{\"Exhausted\":{\"explored\":4}}}' ;; *) printf '%s' '{\"version\":2,\"seed\":0,\"outcome\":{\"Failure\":{\"code\":\"declined\",\"message\":\"use MITM\"}}}' ;; esac\n",
    )?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::Rx(Angle::radians(0.1)?), &[builder.qubit(0)?], &[])?;
    let base = options(1)?;
    let configured = OptimizationOptions::new(
        base.target().clone(),
        OptimizationLimits::default(),
        ApproximationMode::local(BigRational::from_integer(1.into()))?,
    )?;
    let result = Optimizer::from_region(builder.finish()?, &[], configured)?.search_with_workers(
        BeamOptions::new(1, 1, 16, 2)?,
        &client,
        0,
        quest_math::Limits::default(),
    );
    let _ = std::fs::remove_file(path);
    let outcome = result?;
    expect_eq!(outcome.stop_reason(), StopReason::GeneratorLimit);
    expect_true!(matches!(
        outcome
            .search_report()
            .ok_or(quest_circuit::Error::InvalidId)?
            .approx_mitm_statuses(),
        [BeamMitmStatus::Exhausted { explored: 4 }]
    ));
    Ok(())
}

#[cfg(all(feature = "workers", target_os = "linux"))]
#[gtest]
fn budget_after_zx_worker_proof_returns_a_complete_prior_publication() -> Result<()> {
    use quest_circuit::{Error, StopReason};
    let marker = std::env::temp_dir().join(format!("quest-beam-proof-call-{}", std::process::id()));
    let mut script = format!(
        "#!/bin/sh\ncat >/dev/null\nprintf 'called' > '{}'\n",
        marker.display()
    );
    script.push_str("printf '%s' '{\"version\":2,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"Z\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"beam-budget-fixture\",\"precision_bits\":256}}}'\n");
    let (path, client) = worker_script(&script)?;
    let base = options(1)?;
    let mut witnessed = false;
    for max_work in (1_000_000..=1_400_000).step_by(5_000) {
        let _ = std::fs::remove_file(&marker);
        let configured = OptimizationOptions::new(
            base.target().clone(),
            OptimizationLimits::new(max_work, 256 * 1024 * 1024)?,
            ApproximationMode::Disabled,
        )?;
        let optimizer = match Optimizer::from_region(worker_input()?, &[], configured) {
            Ok(value) => value,
            Err(Error::Budget(_)) => continue,
            Err(error) => return Err(error.into()),
        };
        let outcome = optimizer.search_with_workers(
            BeamOptions::new(1, 1, 16, 1)?,
            &client,
            0,
            quest_math::Limits::default(),
        )?;
        let requests = outcome
            .search_report()
            .ok_or(Error::InvalidId)?
            .worker_requests();
        if requests > 0 && marker.exists() && outcome.stop_reason() == StopReason::WorkLimit {
            let OptimizerInput::Region { bound, .. } = outcome.into_input() else {
                return Err(Error::InvalidId.into());
            };
            bound.plan()?;
            witnessed = true;
            break;
        }
    }
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(marker);
    expect_true!(witnessed);
    Ok(())
}

#[cfg(all(feature = "workers", target_os = "linux"))]
#[gtest]
fn forged_worker_envelope_is_a_typed_error() -> Result<()> {
    let (path, client) = worker_script(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":2,\"seed\":999,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"Z\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"forged-fixture\",\"precision_bits\":256}}}'\n",
    )?;
    let result = Optimizer::from_region(worker_input()?, &[], options(1)?)?.search_with_workers(
        BeamOptions::new(1, 1, 16, 1)?,
        &client,
        0,
        quest_math::Limits::default(),
    );
    let _ = std::fs::remove_file(path);
    expect_true!(matches!(
        result,
        Err(quest_circuit::Error::Worker(error))
            if matches!(error.downcast_ref::<quest_circuit::WorkerError>(), Some(quest_circuit::WorkerError::Worker(quest_circuit::optimizer::Error::Envelope)))
    ));
    Ok(())
}
