use googletest::Result;
use googletest::prelude::*;
use quest_circuit::*;

#[gtest]
fn rejects_foreign_operands_without_consuming_an_occurrence() -> Result<()> {
    let mut a = ProgramBuilder::new(2, 1)?;
    let b = ProgramBuilder::new(2, 1)?;
    expect_true!(a.gate(Gate::X, &[b.qubit(0)?], &[]).is_err());
    let id = a.gate(Gate::X, &[a.qubit(0)?], &[])?;
    expect_eq!(id.index(), 0);
    Ok(())
}

#[gtest]
fn controlled_full_turn_rotation_keeps_relative_phase() -> Result<()> {
    let mut b = ProgramBuilder::new(2, 0)?;
    let q0 = b.qubit(0)?;
    let q1 = b.qubit(1)?;
    b.gate(
        Gate::Rz(Angle::pi(2, 1)?),
        &[q1],
        &[Control::new(q0, ControlState::One)],
    )?;
    let (program, report) = b.finish()?.optimize_exact()?;
    expect_eq!(report.removed.len(), 0);
    let plan = program.bind(&[])?.lower()?.plan()?;
    expect_eq!(plan.instructions().len(), 1);
    Ok(())
}

#[gtest]
fn stochastic_and_classical_hazards_preserve_order() -> Result<()> {
    let mut b = ProgramBuilder::new(2, 1)?;
    let q0 = b.qubit(0)?;
    let q1 = b.qubit(1)?;
    let c = b.bit(0)?;
    let first = b.measure(q0, c)?;
    let second = b.measure(q1, c)?;
    let p = b.finish()?;
    expect_true!(p.has_dependency(first, second)?);
    expect_eq!(p.schedule(), &[first, second]);
    Ok(())
}

#[gtest]
fn missing_nonfinite_and_duplicate_bindings_are_rejected() -> Result<()> {
    let mut b = ProgramBuilder::new(1, 0)?;
    let t = b.parameter("theta")?;
    b.gate(Gate::Rx(Angle::parameter(t)), &[b.qubit(0)?], &[])?;
    let p = b.finish()?;
    expect_true!(p.clone().bind(&[]).is_err());
    expect_true!(p.clone().bind(&[(t, f64::INFINITY)]).is_err());
    expect_true!(p.clone().bind(&[(t, 1.0), (t, 2.0)]).is_err());
    expect_eq!(
        p.bind(&[(t, 0.5)])?.lower()?.plan()?.instructions().len(),
        1
    );
    Ok(())
}

#[gtest]
fn exact_cancellation_stops_at_measurement() -> Result<()> {
    let mut b = ProgramBuilder::new(1, 1)?;
    let q = b.qubit(0)?;
    b.gate(Gate::X, &[q], &[])?;
    b.measure(q, b.bit(0)?)?;
    b.gate(Gate::X, &[q], &[])?;
    let (p, r) = b.finish()?.optimize_exact()?;
    expect_eq!(p.schedule().len(), 3);
    expect_eq!(r.removed.len(), 0);
    expect_true!(p.into_unitary().is_err());
    Ok(())
}

#[gtest]
fn arbitrary_rational_angles_validate_and_normalize_raw_ratios() -> Result<()> {
    use quest_circuit::BigRational;
    for numerator in [0, 1] {
        expect_true!(matches!(
            Angle::rational_pi(BigRational::new_raw(numerator.into(), 0.into())),
            Err(Error::ZeroDenominator)
        ));
    }
    expect_eq!(
        Angle::rational_pi(BigRational::new_raw(2.into(), (-4).into()))?,
        Angle::pi(-1, 2)?
    );
    Ok(())
}

#[gtest]
fn channel_admission_counts_existing_payloads_and_residual_scratch() -> Result<()> {
    let matrix = BoundGate::Id.matrix(MatrixPolicy::default())?;
    let mut b = ProgramBuilder::with_limits(
        1,
        0,
        ProgramLimits {
            max_matrix_bytes: 256,
            ..ProgramLimits::default()
        },
    )?;
    let q = b.qubit(0)?;
    b.numerical(matrix.clone(), &[q], &[])?;
    // The resident 128-byte payload, new payload and completeness sum require
    // 384 bytes during admission, although the final two payloads would fit.
    expect_true!(matches!(
        b.channel(vec![matrix], &[q], 1e-12),
        Err(Error::Budget(_))
    ));
    expect_eq!(b.gate(Gate::X, &[q], &[])?.index(), 1);
    Ok(())
}

#[gtest]
fn upgraded_bigint_rationals_keep_large_ratio_and_subnormal_conversion() -> Result<()> {
    use num_bigint::BigInt;
    use quest_circuit::BigRational;
    let denominator = std::ops::Shl::shl(BigInt::from(1), 1200usize);
    let numerator = std::ops::Add::add(&denominator, BigInt::from(1));
    let angle = Angle::rational_pi(BigRational::new(numerator, denominator))?;
    let mut builder = ProgramBuilder::new(1, 0)?;
    builder.gate(Gate::Rz(angle), &[builder.qubit(0)?], &[])?;
    let bound = builder.finish()?.bind(&[])?;
    if let Operation::Gate {
        gate: BoundGate::Rz(value),
        ..
    } = bound.instructions()[0].operation()
    {
        expect_eq!(*value, std::f64::consts::PI);
    } else {
        fail!("expected Rz")?;
    }
    Ok(())
}
