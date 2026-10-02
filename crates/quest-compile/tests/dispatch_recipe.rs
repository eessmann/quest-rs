use faer::Mat;
use num_complex::Complex64;
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::{
    BoundGate, Control, ControlState, MatrixPolicy, NumericalOperator, OracleFragment,
    QuantumRegionBuilder,
    dispatch_recipe::{
        DispatchStep, MatrixRecipe, PreparedRecipeInventory, RecipeLimits,
        discover_oracle_profiles, discover_oracle_profiles_with_limits, gate_recipe,
        scalar_phase_recipe,
    },
};

use googletest::prelude::*;
#[gtest]
fn gate_recipe_preserves_u_order_and_signed_phase_cost() -> googletest::Result<()> {
    let recipe = gate_recipe(
        &BoundGate::U {
            theta: 0.4,
            phi: 0.2,
            lambda: 0.3,
        },
        2,
    )?;
    expect_eq!(recipe.native_calls(), 16);
    expect_eq!(
        gate_recipe(
            &BoundGate::U {
                theta: 0.4,
                phi: 0.2,
                lambda: 0.3
            },
            0
        )?
        .logical_state_passes(true, false),
        3
    );
    expect_eq!(
        scalar_phase_recipe(0.1, 0)?.logical_state_passes(true, false),
        0
    );
    expect_eq!(
        recipe.steps().collect::<Vec<_>>(),
        vec![
            DispatchStep::PhaseGate(0.3),
            DispatchStep::Native(quest_compile::dispatch_recipe::PrimitiveGate::Ry(0.4)),
            DispatchStep::PhaseGate(0.2),
            DispatchStep::ScalarPhase(0.2),
        ]
    );
    expect_eq!(gate_recipe(&BoundGate::Id, 2)?.native_calls(), 0);
    expect_eq!(gate_recipe(&BoundGate::Sx, 2)?.native_calls(), 6);
    expect_eq!(scalar_phase_recipe(0.1, 2)?.native_calls(), 5);
    Ok(())
}

#[gtest]
fn matrix_recipe_embeds_ordered_signed_controls_and_both_variants() -> googletest::Result<()> {
    let source = Mat::from_fn(2, 2, |r, c| {
        Complex64::new(
            match (r, c) {
                (0, 0) => 1.0,
                (1, 1) => 2.0,
                _ => 0.0,
            },
            0.0,
        )
    });
    let matrix = NumericalOperator::from_view(source.as_ref(), MatrixPolicy::default())?;
    let recipe = MatrixRecipe::new(&matrix, &[false, true])?;
    expect_eq!(recipe.dimension(), 8);
    expect_true!(recipe.is_diagonal());
    expect_eq!(recipe.native_variant_count(), 2);
    expect_eq!(recipe.native_apply_calls(false), 1);
    expect_eq!(recipe.native_apply_calls(true), 2);
    expect_eq!(recipe.value(4, 4), Complex64::new(1.0, 0.0));
    expect_eq!(recipe.value(2, 2), Complex64::new(1.0, 0.0));
    expect_eq!(recipe.value(4 + 1, 4 + 1), Complex64::new(2.0, 0.0));
    expect_eq!(recipe.value(4, 5), Complex64::new(0.0, 0.0));
    expect_eq!(recipe.value(2 + 1, 2 + 1), Complex64::new(1.0, 0.0));
    Ok(())
}

#[gtest]
fn scalar_numerical_promotes_to_one_native_qubit_and_prepares_adjoint() -> googletest::Result<()> {
    let source = Mat::from_fn(1, 1, |_, _| Complex64::new(0.0, 1.0));
    let matrix = NumericalOperator::from_view(source.as_ref(), MatrixPolicy::default())?;
    let local = MatrixRecipe::new(&matrix, &[])?;
    expect_eq!(local.dimension(), 2);
    expect_eq!(local.value(1, 1), Complex64::new(0.0, 1.0));
    expect_eq!(local.adjoint_value(1, 1), Complex64::new(0.0, -1.0));
    let negative = MatrixRecipe::new(&matrix, &[false])?;
    expect_eq!(negative.value(0, 0), Complex64::new(0.0, 1.0));
    expect_eq!(negative.value(1, 1), Complex64::new(1.0, 0.0));
    expect_eq!(negative.adjoint_value(0, 0), Complex64::new(0.0, -1.0));
    Ok(())
}

#[gtest]
fn matrix_recipe_rejects_shifted_dimension_overflow() -> googletest::Result<()> {
    let source = Mat::from_fn(2, 2, |r, c| Complex64::new(f64::from(r == c), 0.0));
    let matrix = NumericalOperator::from_view(source.as_ref(), MatrixPolicy::default())?;
    expect_true!(MatrixRecipe::new(&matrix, &[false; 63]).is_err());
    Ok(())
}

#[gtest]
fn preparation_shares_storage_and_ordered_profile_but_counts_occurrences() -> googletest::Result<()>
{
    let source = Mat::from_fn(2, 2, |r, c| Complex64::new(f64::from(r == c), 0.0));
    let shared = NumericalOperator::from_view(source.as_ref(), MatrixPolicy::default())?;
    let distinct = NumericalOperator::from_view(source.as_ref(), MatrixPolicy::default())?;
    let mut builder = QuantumRegionBuilder::new(3, 0)?;
    let target = builder.qubit(0)?;
    let first = Control::new(builder.qubit(1)?, ControlState::Zero);
    let second = Control::new(builder.qubit(2)?, ControlState::One);
    builder.numerical(shared.clone(), &[target], &[first])?;
    builder.numerical(shared.clone(), &[target], &[first])?;
    builder.numerical(shared, &[target], &[second])?;
    builder.numerical(distinct, &[target], &[first])?;
    let plan = builder.finish()?.bind(&[])?.plan()?;
    let inventory = PreparedRecipeInventory::from_plan(&plan, false)?;
    expect_eq!(inventory.native_apply_calls(), 4);
    expect_eq!(inventory.unique_matrices(), 3);
    expect_eq!(inventory.native_variants(), 6);
    expect_eq!(inventory.payload_bytes(), 3 * 4 * 16 * 2);
    expect_true!(
        PreparedRecipeInventory::from_plan_with_limits(
            &plan,
            false,
            RecipeLimits::new(3, 3, 1024)?
        )
        .is_err()
    );
    expect_true!(
        PreparedRecipeInventory::from_plan_with_limits(&plan, false, RecipeLimits::new(10, 10, 1)?)
            .is_err()
    );
    Ok(())
}

#[gtest]
fn nested_adjoint_oracles_share_payload_per_full_signed_profile() -> googletest::Result<()> {
    let source = Mat::from_fn(2, 2, |r, c| {
        Complex64::new(
            if r == c {
                if r == 0 { 1.0 } else { -1.0 }
            } else {
                0.0
            },
            0.0,
        )
    });
    let matrix = NumericalOperator::from_view(source.as_ref(), MatrixPolicy::default())?;
    let mut leaf = QuantumRegionBuilder::new(1, 0)?;
    leaf.numerical(matrix, &[leaf.qubit(0)?], &[])?;
    let leaf = OracleFragment::builder(leaf.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let mut middle = QuantumRegionBuilder::new(1, 0)?;
    middle.oracle(&leaf, &[middle.qubit(0)?], &[])?;
    let middle = OracleFragment::builder(middle.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let profiles = discover_oracle_profiles(&middle, &[false], 1, 2)?;
    expect_eq!(profiles.len(), 2);
    expect_true!(profiles.iter().all(|p| p.signed_controls() == [false]));
    expect_true!(
        discover_oracle_profiles_with_limits(
            &middle,
            &[false],
            1,
            2,
            RecipeLimits::new(10, 1, 1024)?
        )
        .is_err()
    );
    let mut caller = QuantumRegionBuilder::new(2, 0)?;
    let t = caller.qubit(0)?;
    let c = caller.qubit(1)?;
    caller.oracle(&middle, &[t], &[Control::new(c, ControlState::Zero)])?;
    caller.oracle(
        &middle.adjoint(),
        &[t],
        &[Control::new(c, ControlState::Zero)],
    )?;
    caller.oracle(&middle, &[t], &[Control::new(c, ControlState::One)])?;
    let plan = caller.finish()?.bind(&[])?.plan()?;
    let inventory = PreparedRecipeInventory::from_plan(&plan, true)?;
    expect_eq!(inventory.native_apply_calls(), 6);
    expect_eq!(inventory.logical_state_passes(), 3);
    expect_eq!(inventory.unique_matrices(), 2);
    expect_eq!(inventory.native_variants(), 4);
    Ok(())
}
