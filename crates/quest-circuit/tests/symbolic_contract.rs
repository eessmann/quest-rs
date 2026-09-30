use googletest::{Result, prelude::*};
use num_bigint::BigInt;
#[allow(unused_imports)]
use quest_circuit::prelude::*;
use quest_circuit::{
    Angle, BigRational, BoundAngleTarget, Error, Gate, ParityOptions, QuantumRegionBuilder,
};

#[gtest]
fn bound_affine_angle_retains_exact_target_after_parameter_substitution() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let parameter = builder.parameter("theta")?;
    let angle = Angle::parameter(parameter)?.added(&Angle::pi(1, 3)?)?;
    builder.gate(Gate::Rz(angle), &[builder.qubit(0)?], &[])?;
    let program = builder.finish()?.bind(&[(parameter, 0.25)])?;
    let target = program.instructions()[0].angle_targets()[0].as_ref();
    expect_eq!(
        target,
        Some(&BoundAngleTarget::AffinePi {
            radians_numerator: BigInt::from(1),
            radians_denominator: BigInt::from(4),
            pi_numerator: BigInt::from(1),
            pi_denominator: BigInt::from(3),
        })
    );
    Ok(())
}

#[gtest]
fn ideal_binding_work_forecast_is_stable_after_shared_angle_conversion() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::Rz(Angle::pi(1, 7)?), &[builder.qubit(0)?], &[])?;
    let ideal = builder.finish()?;
    let cold = ideal.binding_work_estimate()?;
    expect_true!(cold > 16_000);
    ideal.clone().bind(&[])?.plan()?;
    expect_eq!(ideal.binding_work_estimate()?, cold);
    Ok(())
}

#[gtest]
fn binding_with_unrelated_declared_parameter_preserves_angle_target() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let used = builder.parameter("used")?;
    let unused = builder.parameter("unused")?;
    builder.gate(Gate::Rz(Angle::parameter(used)?), &[builder.qubit(0)?], &[])?;
    let bound = builder.finish()?.bind(&[(used, 0.25), (unused, 0.5)])?;
    expect_eq!(
        bound.instructions()[0].angle_targets()[0].as_ref(),
        Some(&BoundAngleTarget::DyadicRadians {
            bits: 0.25f64.to_bits()
        })
    );
    Ok(())
}

#[gtest]
fn finite_exact_zero_merge_can_be_removed() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let positive = Angle::pi(1, 3)?;
    let negative = Angle::pi(-1, 3)?;
    builder.gate(Gate::Rz(positive), &[builder.qubit(0)?], &[])?;
    builder.gate(Gate::Rz(negative), &[builder.qubit(0)?], &[])?;
    let (optimized, _) = builder.finish()?.optimize_exact()?;
    expect_eq!(optimized.schedule().len(), 0);
    Ok(())
}

#[gtest]
fn inverse_parameter_leaf_pair_is_removed_with_retained_bindings() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let parameter = builder.parameter("theta")?;
    let angle = Angle::parameter(parameter)?;
    builder.gate(Gate::Rx(angle.clone()), &[builder.qubit(0)?], &[])?;
    builder.gate(Gate::Rx(angle.negated()?), &[builder.qubit(0)?], &[])?;
    let original = builder.finish()?;
    let (optimized, _) = original.optimize_exact()?;
    expect_eq!(optimized.schedule().len(), 0);
    expect_true!(optimized.bind(&[(parameter, -0.0)]).is_ok());
    Ok(())
}

#[gtest]
fn cancelled_foreign_parameter_is_still_rejected() -> Result<()> {
    let mut foreign = QuantumRegionBuilder::new(1, 0)?;
    let p = foreign.parameter("foreign")?;
    let angle = Angle::parameter(p)?.added(&Angle::parameter(p)?.negated()?)?;
    let mut local = QuantumRegionBuilder::new(1, 0)?;
    expect_true!(
        local
            .gate(Gate::Rz(angle), &[local.qubit(0)?], &[])
            .is_err()
    );
    Ok(())
}

#[gtest]
fn bound_parameter_leaf_preserves_negative_zero_bits() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let p = builder.parameter("p")?;
    builder.gate(Gate::Rx(Angle::parameter(p)?), &[builder.qubit(0)?], &[])?;
    let program = builder.finish()?.bind(&[(p, -0.0)])?;
    expect_eq!(
        program.instructions()[0].angle_targets()[0].as_ref(),
        Some(&BoundAngleTarget::DyadicRadians {
            bits: (-0.0f64).to_bits(),
        })
    );
    Ok(())
}

#[gtest]
fn affine_constant_binds_without_rounding_terms_separately() -> Result<()> {
    let angle = Angle::affine(
        BigRational::new(BigInt::from(1), BigInt::from(3)),
        BigRational::new(BigInt::from(-1), BigInt::from(10)),
    )?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(Gate::Rz(angle), &[builder.qubit(0)?], &[])?;
    let program = builder.finish()?.bind(&[])?;
    expect_eq!(
        program.instructions()[0].angle_targets()[0].as_ref(),
        Some(&BoundAngleTarget::AffinePi {
            radians_numerator: BigInt::from(1),
            radians_denominator: BigInt::from(3),
            pi_numerator: BigInt::from(-1),
            pi_denominator: BigInt::from(10),
        })
    );
    Ok(())
}

#[gtest]
fn cancelled_pi_leaves_keep_original_finite_conversion_obligations() -> Result<()> {
    let huge = BigInt::from(10).pow(400);
    let positive = Angle::rational_pi(BigRational::from_integer(huge.clone()))?;
    let negative = Angle::rational_pi(BigRational::from_integer(std::ops::Neg::neg(huge)))?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(
        Gate::Rz(positive.added(&negative)?),
        &[builder.qubit(0)?],
        &[],
    )?;
    let original = builder.finish()?;
    expect_true!(matches!(original.clone().bind(&[]), Err(Error::NonFinite)));
    let (optimized, _) = original.optimize_exact()?;
    expect_true!(matches!(optimized.bind(&[]), Err(Error::NonFinite)));
    Ok(())
}

#[gtest]
fn parity_rewrite_preserves_unbindable_pi_leaf() -> Result<()> {
    let huge = BigInt::from(10).pow(400);
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(
        Gate::Rz(Angle::rational_pi(BigRational::from_integer(huge))?),
        &[builder.qubit(0)?],
        &[],
    )?;
    let (optimized, report) = builder
        .finish()?
        .optimize_parity(ParityOptions::default())?;
    expect_eq!(report.accepted_windows, 0);
    expect_true!(matches!(optimized.bind(&[]), Err(Error::NonFinite)));
    Ok(())
}

#[gtest]
fn direct_affine_leaf_and_derived_scaling_keep_original_conversion_obligations() -> Result<()> {
    let huge = BigRational::from_integer(BigInt::from(10).pow(400));
    let bad = Angle::affine(huge, BigRational::from_integer(0.into()))?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(
        Gate::Rz(bad.added(&bad.negated()?)?),
        &[builder.qubit(0)?],
        &[],
    )?;
    expect_true!(matches!(builder.finish()?.bind(&[]), Err(Error::NonFinite)));

    let scaled = Angle::pi(1, 1)?.scaled_ratio(BigInt::from(10).pow(400), 1.into())?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.gate(
        Gate::Rz(scaled.added(&scaled.negated()?)?),
        &[builder.qubit(0)?],
        &[],
    )?;
    expect_true!(matches!(builder.finish()?.bind(&[]), Err(Error::NonFinite)));
    Ok(())
}

#[gtest]
fn definition_parameter_substitution_preserves_opaque_signed_zero() -> Result<()> {
    let mut body = QuantumRegionBuilder::new(1, 0)?;
    let p = body.parameter("formal")?;
    body.gate(
        Gate::Rx(Angle::parameter(p)?.negated()?),
        &[body.qubit(0)?],
        &[],
    )?;
    let body = body.finish()?.into_unitary()?;
    let mut caller = QuantumRegionBuilder::new(1, 0)?;
    let definition = caller.define("negative", body)?;
    caller.call(
        definition,
        &[caller.qubit(0)?],
        &[Angle::radians(0.0)?],
        &[],
    )?;
    let bound = caller.finish()?.bind(&[])?;
    expect_eq!(
        bound.instructions()[0].angle_targets()[0].as_ref(),
        Some(&BoundAngleTarget::DyadicRadians {
            bits: (-0.0f64).to_bits()
        })
    );
    Ok(())
}

#[gtest]
fn compound_exact_definition_rejects_opaque_argument() -> Result<()> {
    let mut body = QuantumRegionBuilder::new(1, 0)?;
    let p = body.parameter("formal")?;
    let angle = Angle::parameter(p)?.added(&Angle::pi(1, 3)?)?;
    body.gate(Gate::Rx(angle), &[body.qubit(0)?], &[])?;
    let mut caller = QuantumRegionBuilder::new(1, 0)?;
    let definition = caller.define("compound", body.finish()?.into_unitary()?)?;
    expect_true!(matches!(
        caller.call(
            definition,
            &[caller.qubit(0)?],
            &[Angle::radians(0.1)?],
            &[]
        ),
        Err(Error::Unsupported("opaque compound angle substitution"))
    ));
    Ok(())
}
