use dashu_int::IBig;
use faer::Mat;
use googletest::{Result, prelude::*};
use num_complex::Complex64;
use quest_compile::dispatch_recipe::PreparedRecipeInventory;
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::{
    Angle, ApproximationMode, BudgetCategory, BudgetLedger, CliffordCost, CommunicationCost,
    Constructed, Control, ControlState, CostComparison, CostComponents, CostProfile,
    DependencyKind, DeploymentKind, DeploymentSnapshot, Gate, MatrixPolicy, NativeCost,
    NumericalOperator, OptimizationLimits, OptimizationOptions, OptimizationTarget, Optimizer,
    OptimizerInput, OptimizerInputKind, OptimizerSnapshot, OracleFragment, Program,
    QuantumRegionBuilder, StopReason, validate_mandatory_projection,
};
use std::sync::{Arc, Barrier};

fn ratio(n: i64, d: i64) -> quest_compile::RBig {
    quest_compile::RBig::from_parts_signed(IBig::from(n), IBig::from(d))
}

#[gtest]
fn embedded_fragment_cost_uses_actual_wider_deployment() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
    let plan = builder.finish()?.bind(&[])?.plan()?;
    let deployment =
        DeploymentSnapshot::new(DeploymentKind::StateVector, 3, false, false, false, 0, 1, 8)?;
    let ledger = BudgetLedger::new(OptimizationLimits::default());
    expect_true!(NativeCost::from_plan(&plan, deployment, &ledger, 0).is_err());
    let cost = NativeCost::from_embedded_plan(&plan, deployment, &ledger, 0)?;
    expect_eq!(cost.dispatches(), 1);
    expect_eq!(cost.state_passes(), 1);
    Ok(())
}

fn distributed_target(profile: CostProfile) -> quest_compile::Result<OptimizationTarget> {
    let deployment =
        DeploymentSnapshot::new(DeploymentKind::StateVector, 3, false, true, false, 0, 2, 4)?;
    OptimizationTarget::new(deployment, ratio(2, 1), profile)
}

#[gtest]
fn shared_ledger_admits_categories_under_one_limit_and_releases_bytes() -> Result<()> {
    let ledger = BudgetLedger::new(OptimizationLimits::new(6, 128)?);
    let candidate = ledger.reserve(BudgetCategory::Candidate, 2, 80)?;
    let verification = ledger.reserve(BudgetCategory::Verification, 2, 40)?;
    expect_eq!(ledger.usage().retained_bytes, 120);
    expect_true!(ledger.reserve(BudgetCategory::Frontier, 1, 9).is_err());
    drop(candidate);
    let provenance = ledger.reserve(BudgetCategory::Provenance, 2, 80)?;
    expect_eq!(ledger.usage().work, 6);
    expect_true!(ledger.reserve(BudgetCategory::Worker, 1, 1).is_err());
    drop(verification);
    drop(provenance);
    expect_eq!(ledger.usage().retained_bytes, 0);
    Ok(())
}

#[gtest]
fn concurrent_ledger_clones_admit_only_one_retained_allowance() -> Result<()> {
    let ledger = BudgetLedger::new(OptimizationLimits::new(10, 100)?);
    let barrier = Arc::new(Barrier::new(3));
    let (left, right, peak) = std::thread::scope(|scope| {
        let attempt = |ledger: BudgetLedger, barrier: Arc<Barrier>| {
            let lease = ledger.reserve(BudgetCategory::Candidate, 1, 80);
            barrier.wait();
            let admitted = lease.is_ok();
            barrier.wait();
            admitted
        };
        let left = scope.spawn({
            let barrier = Arc::clone(&barrier);
            let ledger = ledger.clone();
            move || attempt(ledger, barrier)
        });
        let right = scope.spawn({
            let barrier = Arc::clone(&barrier);
            let ledger = ledger.clone();
            move || attempt(ledger, barrier)
        });
        barrier.wait();
        let peak = ledger.usage().retained_bytes;
        barrier.wait();
        (left.join(), right.join(), peak)
    });
    expect_true!(left.is_ok());
    expect_true!(right.is_ok());
    expect_eq!(peak, 80);
    expect_ne!(left.unwrap_or(false), right.unwrap_or(false));
    expect_eq!(ledger.usage().retained_bytes, 0);
    Ok(())
}

#[gtest]
fn deployment_rejects_inconsistent_local_state_and_density_sizes() {
    expect_true!(
        DeploymentSnapshot::new(DeploymentKind::StateVector, 1, false, false, false, 0, 1, 1,)
            .is_err()
    );
    expect_true!(
        DeploymentSnapshot::new(
            DeploymentKind::DensityMatrix,
            2,
            false,
            true,
            false,
            0,
            2,
            4,
        )
        .is_err()
    );
    expect_true!(
        DeploymentSnapshot::new(
            DeploymentKind::DensityMatrix,
            2,
            false,
            true,
            false,
            0,
            2,
            8,
        )
        .is_ok()
    );
}

#[gtest]
fn failed_opaque_trace_spends_bounded_work_in_shared_ledger() -> Result<()> {
    let ledger = BudgetLedger::new(OptimizationLimits::new(4, 128)?);
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
    let plan = builder.finish()?.bind(&[])?.plan()?;
    expect_true!(CommunicationCost::from_plan(&plan, &ledger).is_err());
    expect_eq!(ledger.usage().work, 4);
    expect_true!(CommunicationCost::from_plan(&plan, &ledger).is_err());
    Ok(())
}

#[gtest]
fn shared_nested_oracle_trace_stops_at_work_limit_before_expansion() -> Result<()> {
    let mut base = QuantumRegionBuilder::new(1, 0)?;
    base.gate(Gate::H, &[base.qubit(0)?], &[])?;
    let mut fragment =
        OracleFragment::from_program(base.finish()?.bind(&[])?, 0.0, MatrixPolicy::default())?;
    for _ in 0..18 {
        let mut wrapper = QuantumRegionBuilder::new(1, 0)?;
        let target = wrapper.qubit(0)?;
        wrapper.oracle(&fragment, &[target], &[])?;
        wrapper.oracle(&fragment, &[target], &[])?;
        fragment = OracleFragment::from_program(
            wrapper.finish()?.bind(&[])?,
            0.0,
            MatrixPolicy::default(),
        )?;
    }
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.oracle(&fragment, &[builder.qubit(0)?], &[])?;
    let plan = builder.finish()?.bind(&[])?.plan()?;
    let ledger = BudgetLedger::new(OptimizationLimits::new(32, 1024)?);
    expect_true!(CommunicationCost::from_plan(&plan, &ledger).is_err());
    expect_eq!(ledger.usage().work, 32);
    Ok(())
}

#[gtest]
fn native_v1_score_is_exact_and_density_multiplier_is_single_sided() -> Result<()> {
    let cost = CostComponents::new(
        NativeCost::new(512, 3, 1, 4, 5, CommunicationCost::known(8)),
        CliffordCost::new(2, 1, 3, 4),
    );
    let state = distributed_target(CostProfile::NativeV1)?;
    expect_eq!(state.score(&cost)?.known(), &ratio(337, 2));
    let density = OptimizationTarget::new(
        DeploymentSnapshot::new(
            DeploymentKind::DensityMatrix,
            2,
            false,
            true,
            false,
            0,
            2,
            8,
        )?,
        ratio(2, 1),
        CostProfile::NativeV1,
    )?;
    expect_eq!(density.score(&cost)?.known(), &ratio(593, 2));
    Ok(())
}

#[gtest]
fn changed_opaque_communication_is_unscorable() -> Result<()> {
    let target = distributed_target(CostProfile::NativeV1)?;
    let old = CostComponents::new(
        NativeCost::new(0, 2, 1, 1, 0, CommunicationCost::opaque(vec![10, 20])?),
        CliffordCost::new(0, 0, 1, 2),
    );
    let same = CostComponents::new(
        NativeCost::new(0, 1, 1, 1, 0, CommunicationCost::opaque(vec![10, 20])?),
        CliffordCost::new(0, 0, 1, 1),
    );
    let changed = CostComponents::new(
        NativeCost::new(0, 1, 1, 1, 0, CommunicationCost::opaque(vec![10, 21])?),
        CliffordCost::new(0, 0, 1, 1),
    );
    expect_eq!(target.compare(&same, &old)?, CostComparison::Better);
    expect_eq!(target.compare(&changed, &old)?, CostComparison::Unscorable);
    Ok(())
}

#[gtest]
fn clifford_profile_orders_lexicographically() -> Result<()> {
    let target = distributed_target(CostProfile::CliffordTV1)?;
    let expensive_native = CostComponents::new(
        NativeCost::new(
            4096,
            100,
            100,
            100,
            100,
            CommunicationCost::opaque(vec![1])?,
        ),
        CliffordCost::new(1, 10, 20, 30),
    );
    let cheaper_native = CostComponents::new(
        NativeCost::new(0, 0, 0, 0, 0, CommunicationCost::opaque(vec![1])?),
        CliffordCost::new(2, 0, 0, 0),
    );
    expect_eq!(
        target.compare(&expensive_native, &cheaper_native)?,
        CostComparison::Better
    );
    let changed_communication = CostComponents::new(
        NativeCost::new(0, 0, 0, 0, 0, CommunicationCost::opaque(vec![2])?),
        CliffordCost::new(0, 0, 0, 0),
    );
    expect_eq!(
        target.compare(&changed_communication, &cheaper_native)?,
        CostComparison::Unscorable
    );
    Ok(())
}

#[gtest]
fn options_reject_invalid_global_epsilon_and_hard_limit_excess() -> Result<()> {
    let target = distributed_target(CostProfile::NativeV1)?;
    expect_true!(ApproximationMode::global(ratio(0, 1)).is_err());
    expect_true!(
        ApproximationMode::global(quest_compile::RBig::from_parts_signed(
            1.into(),
            (-2).into()
        ))
        .is_err()
    );
    expect_true!(
        OptimizationTarget::new(
            target.deployment(),
            quest_compile::RBig::from_parts_signed(1.into(), (-2).into()),
            CostProfile::NativeV1,
        )
        .is_err()
    );
    expect_true!(OptimizationLimits::new(10_000_001, 256 * 1024 * 1024).is_err());
    let options = OptimizationOptions::new(
        target,
        OptimizationLimits::default(),
        ApproximationMode::Disabled,
    )?;
    expect_eq!(options.limits().max_work(), 10_000_000);
    Ok(())
}

#[gtest]
fn recipe_inventory_counts_density_numerical_pass_once() -> Result<()> {
    let matrix = Mat::from_fn(2, 2, |row, col| Complex64::new(f64::from(row != col), 0.0));
    let numerical = NumericalOperator::from_view(matrix.as_ref(), MatrixPolicy::default())?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.numerical(numerical, &[builder.qubit(0)?], &[])?;
    let plan = builder.finish()?.bind(&[])?.plan()?;
    let inventory = PreparedRecipeInventory::from_plan(&plan, true)?;
    let deployment = DeploymentSnapshot::new(
        DeploymentKind::DensityMatrix,
        1,
        false,
        false,
        false,
        0,
        1,
        4,
    )?;
    let cost = NativeCost::from_recipe_inventory(
        inventory,
        deployment,
        3,
        0,
        CommunicationCost::known(0),
    )?;
    expect_eq!(cost.preparation_bytes(), 128);
    expect_eq!(cost.dispatches(), 2);
    expect_eq!(cost.state_passes(), 1);
    expect_eq!(cost.arithmetic(), 3);
    Ok(())
}

#[gtest]
fn consuming_entrypoints_preserve_input_kind_and_expose_no_search() -> Result<()> {
    let deployment =
        DeploymentSnapshot::new(DeploymentKind::StateVector, 1, false, false, false, 0, 1, 2)?;
    let options = OptimizationOptions::new(
        OptimizationTarget::once(deployment, CostProfile::NativeV1)?,
        OptimizationLimits::default(),
        ApproximationMode::Disabled,
    )?;
    let ideal = QuantumRegionBuilder::new(1, 0)?.finish()?;
    let snapshot = ideal.snapshot_id();
    let result = Optimizer::from_region(ideal, &[], options.clone())?.finish_without_search();
    expect_eq!(result.input_kind(), OptimizerInputKind::Region);
    expect_eq!(result.stop_reason(), StopReason::NoSearchConfigured);
    expect_eq!(result.snapshot_id(), OptimizerSnapshot::Region(snapshot));
    expect_true!(result.budget().work > 0);
    match result.into_input() {
        OptimizerInput::Region { source, bound } => {
            expect_eq!(source.schedule().len(), 0);
            expect_eq!(bound.instructions().len(), 0);
        }
        _ => {
            expect_true!(false, "ideal input was discarded");
        }
    }
    let bound = QuantumRegionBuilder::new(1, 0)?.finish()?.bind(&[])?;
    let bound_snapshot = bound.snapshot_id();
    let result = Optimizer::from_bound(bound, options.clone())?.finish_without_search();
    expect_eq!(result.input_kind(), OptimizerInputKind::Bound);
    expect_eq!(
        result.snapshot_id(),
        OptimizerSnapshot::Bound(bound_snapshot)
    );
    let structured = Program::<Constructed>::parse("qubit q; h q;", "optimizer.qasm")?.verify()?;
    let result = Optimizer::from_verified_structured(structured, options)?.finish_without_search();
    expect_eq!(result.input_kind(), OptimizerInputKind::VerifiedStructured);
    Ok(())
}

#[gtest]
fn mandatory_order_guard_rejects_deleted_endpoint_and_contraction_cycle() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let q = builder.qubit(0)?;
    let a = builder.gate(Gate::H, &[q], &[])?;
    let middle = builder.gate(Gate::Id, &[q], &[])?;
    let end = builder.gate(Gate::X, &[q], &[])?;
    builder.depend(a, middle)?;
    builder.depend(middle, end)?;
    let source = builder
        .finish()?
        .dependencies()
        .into_iter()
        .filter(|edge| edge.kind == DependencyKind::Explicit)
        .collect::<Vec<_>>();
    let ledger = BudgetLedger::new(OptimizationLimits::default());
    expect_true!(
        validate_mandatory_projection(
            &source,
            &[(a, Some(a)), (middle, None), (end, Some(end))],
            &[],
            &ledger,
        )
        .is_err()
    );
    expect_true!(
        validate_mandatory_projection(
            &source,
            &[(a, Some(a)), (middle, Some(middle)), (end, Some(a))],
            &[
                quest_compile::DependencyEdge {
                    before: a,
                    after: middle,
                    kind: DependencyKind::Explicit
                },
                quest_compile::DependencyEdge {
                    before: middle,
                    after: a,
                    kind: DependencyKind::Explicit
                },
            ],
            &ledger,
        )
        .is_err()
    );
    validate_mandatory_projection(
        &source,
        &[(a, Some(a)), (middle, Some(middle)), (end, Some(end))],
        &source,
        &ledger,
    )?;
    Ok(())
}

#[gtest]
fn required_global_rejects_uncertified_numerical_composition() -> Result<()> {
    let matrix = Mat::from_fn(2, 2, |row, col| Complex64::new(f64::from(row == col), 0.0));
    let numerical = NumericalOperator::from_view(matrix.as_ref(), MatrixPolicy::default())?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.numerical(numerical, &[builder.qubit(0)?], &[])?;
    let deployment =
        DeploymentSnapshot::new(DeploymentKind::StateVector, 1, false, false, false, 0, 1, 2)?;
    let options = OptimizationOptions::new(
        OptimizationTarget::once(deployment, CostProfile::NativeV1)?,
        OptimizationLimits::default(),
        ApproximationMode::global(ratio(1, 10))?,
    )?;
    expect_true!(Optimizer::from_region(builder.finish()?, &[], options).is_err());
    Ok(())
}

#[gtest]
fn bound_input_accounts_for_retained_numerical_payload_before_publication() -> Result<()> {
    let matrix = Mat::from_fn(16, 16, |row, col| {
        Complex64::new(f64::from(row == col.wrapping_add(1) % 16), 0.0)
    });
    let numerical = NumericalOperator::from_view(matrix.as_ref(), MatrixPolicy::default())?;
    let mut builder = QuantumRegionBuilder::new(4, 0)?;
    let targets = (0..4)
        .map(|index| builder.qubit(index))
        .collect::<quest_compile::Result<Vec<_>>>()?;
    builder.numerical(numerical, &targets, &[])?;
    let ideal = builder.finish()?;
    let deployment = DeploymentSnapshot::new(
        DeploymentKind::StateVector,
        4,
        false,
        false,
        false,
        0,
        1,
        16,
    )?;
    let options = OptimizationOptions::new(
        OptimizationTarget::once(deployment, CostProfile::NativeV1)?,
        OptimizationLimits::new(10_000_000, 1_024)?,
        ApproximationMode::Disabled,
    )?;
    expect_true!(Optimizer::from_region(ideal.clone(), &[], options.clone()).is_err());
    expect_true!(Optimizer::from_bound(ideal.bind(&[])?, options).is_err());
    Ok(())
}

#[gtest]
fn ideal_binding_reserves_symbolic_replay_work_before_bind() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::Rz(Angle::pi(1, 2)?), &[builder.qubit(0)?], &[])?;
    let deployment =
        DeploymentSnapshot::new(DeploymentKind::StateVector, 1, false, false, false, 0, 1, 2)?;
    let options = OptimizationOptions::new(
        OptimizationTarget::once(deployment, CostProfile::NativeV1)?,
        OptimizationLimits::new(10_000, 256 * 1024 * 1024)?,
        ApproximationMode::Disabled,
    )?;
    expect_true!(Optimizer::from_region(builder.finish()?, &[], options).is_err());
    Ok(())
}

#[gtest]
fn warmed_ideal_clones_keep_the_same_work_admission_limit() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::Rz(Angle::pi(1, 7)?), &[builder.qubit(0)?], &[])?;
    let ideal = builder.finish()?;
    let deployment =
        DeploymentSnapshot::new(DeploymentKind::StateVector, 1, false, false, false, 0, 1, 2)?;
    let options = OptimizationOptions::new(
        OptimizationTarget::once(deployment, CostProfile::NativeV1)?,
        OptimizationLimits::new(10_000, 256 * 1024 * 1024)?,
        ApproximationMode::Disabled,
    )?;
    expect_true!(Optimizer::from_region(ideal.clone(), &[], options.clone()).is_err());
    ideal.clone().bind(&[])?.plan()?;
    expect_true!(Optimizer::from_region(ideal, &[], options).is_err());
    Ok(())
}

#[gtest]
fn ordinary_128_angle_ideal_program_fits_default_shared_work_limit() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let target = builder.qubit(0)?;
    let angle = Angle::pi(1, 2)?;
    for _ in 0..128 {
        builder.gate(Gate::Rz(angle.clone()), &[target], &[])?;
    }
    let deployment =
        DeploymentSnapshot::new(DeploymentKind::StateVector, 1, false, false, false, 0, 1, 2)?;
    let options = OptimizationOptions::new(
        OptimizationTarget::once(deployment, CostProfile::NativeV1)?,
        OptimizationLimits::default(),
        ApproximationMode::Disabled,
    )?;
    let result = Optimizer::from_region(builder.finish()?, &[], options)?.finish_without_search();
    expect_eq!(result.input_kind(), OptimizerInputKind::Region);
    expect_true!(result.budget().work < 10_000_000);
    Ok(())
}

#[gtest]
fn structured_input_accounts_for_retained_source_before_publication() -> Result<()> {
    let source = format!("qubit q; h q; {}", " ".repeat(4096));
    let structured = Program::<Constructed>::parse(&source, "large.qasm")?.verify()?;
    let deployment =
        DeploymentSnapshot::new(DeploymentKind::StateVector, 1, false, false, false, 0, 1, 2)?;
    let options = OptimizationOptions::new(
        OptimizationTarget::once(deployment, CostProfile::NativeV1)?,
        OptimizationLimits::new(10_000_000, 1024)?,
        ApproximationMode::Disabled,
    )?;
    expect_true!(Optimizer::from_verified_structured(structured, options).is_err());
    Ok(())
}

#[gtest]
fn structured_input_rejects_target_width_mismatch() -> Result<()> {
    let structured = Program::<Constructed>::parse("qubit q; h q;", "width.qasm")?.verify()?;
    let deployment =
        DeploymentSnapshot::new(DeploymentKind::StateVector, 2, false, false, false, 0, 1, 4)?;
    let options = OptimizationOptions::new(
        OptimizationTarget::once(deployment, CostProfile::NativeV1)?,
        OptimizationLimits::default(),
        ApproximationMode::Disabled,
    )?;
    expect_true!(Optimizer::from_verified_structured(structured, options).is_err());
    Ok(())
}

#[gtest]
fn distributed_opaque_trace_uses_operation_content_not_fresh_occurrence_ids() -> Result<()> {
    let ledger = BudgetLedger::new(OptimizationLimits::default());
    let make_plan = |gate| -> quest_compile::Result<_> {
        let mut builder = QuantumRegionBuilder::new(1, 0)?;
        builder.gate(gate, &[builder.qubit(0)?], &[])?;
        builder.finish()?.bind(&[])?.plan()
    };
    let first = make_plan(Gate::H)?;
    let same = make_plan(Gate::H)?;
    let changed = make_plan(Gate::X)?;
    let a = CommunicationCost::from_plan(&first, &ledger)?;
    let b = CommunicationCost::from_plan(&same, &ledger)?;
    let c = CommunicationCost::from_plan(&changed, &ledger)?;
    expect_eq!(&a, &b);
    expect_ne!(&a, &c);
    expect_true!(ledger.usage().retained_bytes > 0);
    drop((a, b, c));
    expect_eq!(ledger.usage().retained_bytes, 0);
    Ok(())
}

#[gtest]
fn plan_cost_derives_u_signed_flips_and_density_scalar_noop() -> Result<()> {
    let ledger = BudgetLedger::new(OptimizationLimits::default());
    let mut builder = QuantumRegionBuilder::new(2, 0)?;
    let target = builder.qubit(0)?;
    let control = Control::new(builder.qubit(1)?, ControlState::Zero);
    builder.gate(Gate::Sx, &[target], &[])?;
    builder.gate(
        Gate::U {
            theta: Angle::radians(0.4)?,
            phi: Angle::radians(0.2)?,
            lambda: Angle::radians(0.3)?,
        },
        &[target],
        &[control],
    )?;
    let plan = builder.finish()?.bind(&[])?.plan()?;
    let state =
        DeploymentSnapshot::new(DeploymentKind::StateVector, 2, false, false, false, 0, 1, 4)?;
    let cost = NativeCost::from_plan(&plan, state, &ledger, 0)?;
    expect_eq!(cost.dispatches(), 12);
    expect_eq!(cost.state_passes(), 12);
    expect_eq!(cost.arithmetic(), 20);
    let density = DeploymentSnapshot::new(
        DeploymentKind::DensityMatrix,
        2,
        false,
        false,
        false,
        0,
        1,
        16,
    )?;
    let cost = NativeCost::from_plan(&plan, density, &ledger, 0)?;
    expect_eq!(cost.dispatches(), 12);
    expect_eq!(cost.state_passes(), 11);
    expect_eq!(cost.arithmetic(), 19);
    Ok(())
}
