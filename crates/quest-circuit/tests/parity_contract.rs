use googletest::{Result, prelude::*};
use quest_circuit::{AffinePhaseOperation as A, BigRational, Cnot, ParityOptions, fold_parity};

#[gtest]
fn complemented_rz_keeps_the_exact_scalar_phase() -> Result<()> {
    let one = BigRational::from_integer(1.into());
    let source = [
        A::X { target: 0 },
        A::Rz {
            target: 0,
            coefficient: one.clone(),
        },
        A::X { target: 0 },
        A::Rz {
            target: 0,
            coefficient: one,
        },
    ];
    expect_true!(
        fold_parity(1, &source, ParityOptions::default())?
            .operations()
            .is_empty()
    );
    let source = [A::Rz {
        target: 0,
        coefficient: BigRational::from_integer(2.into()),
    }; 1];
    let result = fold_parity(1, &source, ParityOptions::default())?;
    expect_eq!(result.operations(), &source);
    Ok(())
}

#[gtest]
fn repeated_parity_phase_and_affine_x_network_fold_exactly() -> Result<()> {
    let phase = A::Phase {
        target: 0,
        coefficient: BigRational::new(1.into(), 4.into()),
    };
    let cx = A::Cnot(Cnot {
        control: 1,
        target: 0,
    });
    let source = [cx.clone(), phase.clone(), cx.clone(), cx.clone(), phase, cx];
    let result = fold_parity(2, &source, ParityOptions::default())?;
    expect_eq!(result.operations().len(), 3);
    expect_true!(
        fold_parity(
            2,
            &source,
            ParityOptions {
                max_coefficient_bits: 0,
                ..ParityOptions::default()
            }
        )
        .is_err()
    );
    Ok(())
}

fn evaluate(operations: &[A], mut basis: u64) -> (u64, BigRational) {
    let mut phase = BigRational::from_integer(0.into());
    for operation in operations {
        let addition = match operation {
            A::X { target } => {
                basis ^= 1u64 << target;
                continue;
            }
            A::Cnot(gate) => {
                if basis & (1u64 << gate.control) != 0 {
                    basis ^= 1u64 << gate.target;
                }
                continue;
            }
            A::Phase {
                target,
                coefficient,
            } => {
                if basis & (1u64 << target) != 0 {
                    coefficient.clone()
                } else {
                    BigRational::from_integer(0.into())
                }
            }
            A::Rz {
                target,
                coefficient,
            } => {
                let sign = if basis & (1u64 << target) != 0 { 1 } else { -1 };
                std::ops::Mul::mul(coefficient, BigRational::new(sign.into(), 2.into()))
            }
            A::GlobalPhase { coefficient } => coefficient.clone(),
        };
        phase = std::ops::Add::add(phase, addition);
    }
    let period = std::ops::Mul::mul(phase.denom(), num_bigint::BigInt::from(2));
    let remainder = std::ops::Rem::rem(phase.numer(), &period);
    let numerator = std::ops::Rem::rem(std::ops::Add::add(remainder, &period), period);
    (basis, BigRational::new(numerator, phase.denom().clone()))
}
const fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}
#[gtest]
fn seeded_affine_phase_windows_preserve_every_basis_column_and_scalar_phase() -> Result<()> {
    let mut state = 0xcafe_5eedu64;
    let mut accepted = false;
    for width in 1..=4 {
        for _ in 0..40 {
            let mut source = vec![];
            for _ in 0..80 {
                let target = usize::try_from(
                    random(&mut state)
                        .checked_rem(u64::try_from(width)?)
                        .ok_or_else(|| std::io::Error::other("fixture width"))?,
                )?;
                let coefficient = BigRational::new(
                    i64::try_from(random(&mut state) % 17)?
                        .checked_sub(8)
                        .ok_or_else(|| std::io::Error::other("fixture numerator"))?
                        .into(),
                    4.into(),
                );
                source.push(match random(&mut state) % 5 {
                    0 => A::X { target },
                    1 if width > 1 => {
                        let control = target
                            .checked_add(1)
                            .and_then(|value| value.checked_rem(width))
                            .ok_or_else(|| std::io::Error::other("fixture target"))?;
                        A::Cnot(Cnot { control, target })
                    }
                    2 => A::Rz {
                        target,
                        coefficient,
                    },
                    3 => A::GlobalPhase { coefficient },
                    _ => A::Phase {
                        target,
                        coefficient,
                    },
                });
            }
            let result = fold_parity(width, &source, ParityOptions::default())?;
            accepted |= result.operations().len() < source.len();
            for basis in 0..(1u64 << width) {
                expect_eq!(
                    evaluate(&source, basis),
                    evaluate(result.operations(), basis)
                );
            }
        }
    }
    expect_true!(accepted);
    Ok(())
}

#[gtest]
fn program_folding_retains_rz_scalar_phase_and_stops_at_effects_and_opaque_angles() -> Result<()> {
    use quest_circuit::{Angle, Gate, Operation, ProgramBuilder};
    let mut builder = ProgramBuilder::new(1, 0)?;
    let q = builder.qubit(0)?;
    for _ in 0..2 {
        builder.gate(Gate::Rz(Angle::pi(1, 1)?), &[q], &[])?;
    }
    let (optimized, report) = builder
        .finish()?
        .optimize_parity(ParityOptions::default())?;
    expect_eq!(report.accepted_windows, 1);
    let bound = optimized.bind(&[])?;
    let [instruction] = bound.instructions() else {
        return Err(std::io::Error::other("expected retained scalar phase").into());
    };
    expect_true!(
        matches!(instruction.operation(), Operation::GlobalPhase { radians, controls } if radians.to_bits() == std::f64::consts::PI.to_bits() && controls.is_empty())
    );
    let mut builder = ProgramBuilder::new(1, 1)?;
    let q = builder.qubit(0)?;
    let b = builder.bit(0)?;
    builder.gate(Gate::Rz(Angle::pi(1, 1)?), &[q], &[])?;
    builder.measure(q, b)?;
    builder.gate(Gate::Rz(Angle::pi(1, 1)?), &[q], &[])?;
    builder.reset(q)?;
    builder.gate(Gate::Phase(Angle::radians(0.3)?), &[q], &[])?;
    builder.gate(Gate::Phase(Angle::radians(-0.3)?), &[q], &[])?;
    let (unchanged, report) = builder
        .finish()?
        .optimize_parity(ParityOptions::default())?;
    expect_eq!(unchanged.schedule().len(), 6);
    expect_eq!(report.accepted_windows, 0);
    Ok(())
}

#[gtest]
fn negative_controls_and_explicit_edges_guard_parity_replacements() -> Result<()> {
    use quest_circuit::{Control, ControlState, Gate, ProgramBuilder};
    let mut builder = ProgramBuilder::new(2, 0)?;
    let target = builder.qubit(0)?;
    let controls = [Control::new(builder.qubit(1)?, ControlState::Zero)];
    for _ in 0..2 {
        builder.gate(Gate::X, &[target], &controls)?;
    }
    let (unchanged, report) = builder
        .finish()?
        .optimize_parity(ParityOptions::default())?;
    expect_eq!(unchanged.schedule().len(), 2);
    expect_eq!(report.considered_windows, 0);
    let mut builder = ProgramBuilder::new(1, 0)?;
    let target = builder.qubit(0)?;
    let first = builder.gate(Gate::T, &[target], &[])?;
    let second = builder.gate(Gate::Tdg, &[target], &[])?;
    builder.depend(first, second)?;
    let (unchanged, report) = builder
        .finish()?
        .optimize_parity(ParityOptions::default())?;
    expect_eq!(unchanged.schedule().len(), 2);
    expect_eq!(report.considered_windows, 0);
    expect_true!(
        fold_parity(
            1,
            &[A::GlobalPhase {
                coefficient: BigRational::new_raw(1.into(), 0.into())
            }],
            ParityOptions::default()
        )
        .is_err()
    );
    Ok(())
}
