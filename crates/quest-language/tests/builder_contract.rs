use googletest::prelude::*;
use quest_language::{
    GateKind,
    semantic::{
        CompileLimits,
        builder::{Builder, Int},
    },
};

#[gtest]
fn typed_builder_bounds_shared_expression_expansion_before_materialization() -> Result<()> {
    let mut builder = Builder::with_limits(CompileLimits {
        nodes: 64,
        ..CompileLimits::default()
    })?;
    let mut value = builder.integer::<32>(1)?;
    let mut rejected = false;
    for _ in 0..10 {
        match value.add(&value) {
            Ok(next) => value = next,
            Err(error) => {
                expect_eq!(error.kind, quest_language::semantic::ErrorKind::Resource);
                rejected = true;
                break;
            }
        }
    }
    expect_true!(rejected);
    // Rejecting the next node leaves the accepted expression and builder usable.
    builder.local("sum", &value)?;
    builder.finish(CompileLimits::default())?.into_ssa()?;
    Ok(())
}

#[gtest]
fn typed_builder_constructs_control_flow_through_shared_admission() -> Result<()> {
    let mut builder = Builder::new()?;
    let zero = builder.integer::<32>(0)?;
    let one = builder.integer::<32>(1)?;
    let three = builder.integer::<32>(3)?;
    let counter = builder.local("counter", &zero)?;
    let q = builder.qubit("q", 1)?;
    let condition = builder.read(&counter)?.less(&three)?;
    builder.while_loop(&condition, |body| {
        body.gate(GateKind::X, &[], std::slice::from_ref(&q))?;
        body.assign(&counter, &body.read(&counter)?.add(&one)?)
    })?;
    let typed = builder.finish(CompileLimits::default())?;
    let verified = typed.into_ssa()?;
    verify_that!(verified.blocks().len(), gt(1))?;
    Ok(())
}
#[gtest]
fn typed_builder_rejects_foreign_handles_and_invalid_widths() -> Result<()> {
    let mut left = Builder::new()?;
    let mut right = Builder::new()?;
    let value = left.integer::<32>(1)?;
    let local = left.local("local", &value)?;
    verify_that!(right.local("foreign", &value), err(anything()))?;
    verify_that!(right.read(&local), err(anything()))?;
    verify_that!(right.integer::<0>(0), err(anything()))?;
    verify_that!(right.integer::<65>(0), err(anything()))?;
    verify_that!(right.floating::<16>(0.0), err(anything()))?;
    let own = right.integer::<32>(2)?;
    verify_that!(value.add(&own), err(anything()))?;
    let _: quest_language::semantic::builder::Expr<Int<32>> = value;
    Ok(())
}

#[gtest]
fn typed_builder_uses_hygienic_names_and_rejects_nested_qubit_declarations() -> Result<()> {
    let mut builder = Builder::new()?;
    let zero = builder.integer::<32>(0)?;
    let local = builder.local("value", &zero)?;
    let condition = builder.boolean(true)?;
    builder.if_else(
        &condition,
        |body| {
            body.local("value", &body.boolean(false)?)?;
            body.assign(&local, &zero)
        },
        |_| Ok(()),
    )?;
    verify_that!(
        builder.while_loop(&condition, |body| body.qubit("nested", 1).map(|_| ())),
        err(anything())
    )?;
    builder.finish(CompileLimits::default())?.into_ssa()?;
    Ok(())
}

#[gtest]
fn typed_builder_gate_parameters_do_not_bypass_explicit_cast_rules() -> Result<()> {
    use quest_language::semantic::builder::Float;
    let mut valid = Builder::new()?;
    let q = valid.qubit("q", 1)?;
    let theta = valid.floating::<64>(0.25)?;
    valid.gate(GateKind::Rx, &[theta], &[q])?;
    valid.finish(CompileLimits::default())?.into_ssa()?;
    let mut invalid = Builder::new()?;
    let q = invalid.qubit("q", 1)?;
    let theta = invalid.angle_bits::<8>(128)?.cast::<Float<64>>()?;
    invalid.gate(GateKind::Rx, &[theta], &[q])?;
    verify_that!(invalid.finish(CompileLimits::default()).is_err(), eq(true))?;
    Ok(())
}

#[gtest]
fn boolean_construction_obeys_zero_storage_and_node_budgets() -> Result<()> {
    for limits in [
        CompileLimits {
            storage_bytes: 0,
            ..CompileLimits::default()
        },
        CompileLimits {
            nodes: 0,
            ..CompileLimits::default()
        },
    ] {
        let builder = Builder::with_limits(limits)?;
        expect_true!(builder.boolean(true).is_err());
    }
    Ok(())
}

#[gtest]
fn array_initializers_admit_aggregate_shared_expression_expansion() -> Result<()> {
    for limits in [
        CompileLimits {
            nodes: 80,
            ..CompileLimits::default()
        },
        CompileLimits {
            storage_bytes: 32_768,
            ..CompileLimits::default()
        },
    ] {
        let mut b = Builder::with_limits(limits)?;
        let mut large = b.integer::<32>(1)?;
        for _ in 0..4 {
            large = large.add(&large)?;
        }
        let repeated = vec![large.clone(); 16];
        expect_true!(b.array("repeated", &repeated).is_err());
        expect_true!(b.ranked_array("ranked", [4, 4], &repeated).is_err());
        // Rejected aggregate materialization leaves the cheap shared value usable.
        b.local("accepted", &large)?;
        b.finish(CompileLimits::default())?.into_ssa()?;
    }
    Ok(())
}

#[gtest]
fn ranked_indices_admit_combined_base_and_shared_index_expansions() -> Result<()> {
    let mut b = Builder::with_limits(CompileLimits {
        nodes: 80,
        ..CompileLimits::default()
    })?;
    let array = b.input_array::<Int<32>, 2>("values", [2, 2])?;
    let mut index = b.integer::<32>(0)?;
    for _ in 0..4 {
        index = index.add(&index)?;
    }
    expect_true!(b.ranked_read(&array, &[index.clone(), index]).is_err());
    let zero = b.integer::<32>(0)?;
    b.ranked_read(&array, &[zero.clone(), zero])?;
    Ok(())
}
