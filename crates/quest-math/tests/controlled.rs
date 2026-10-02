use googletest::{Result, prelude::*};
use quest_math::{
    AngleTarget, Axis, Control, Cyclotomic, Gate, Limits, Operation, Sequence, Target,
    certify_rotation, lift_controlled_rotation, reconstruct,
};

fn operation(gate: Gate) -> Operation {
    Operation {
        gate,
        targets: if gate == Gate::W { vec![] } else { vec![0] },
        controls: vec![],
    }
}

fn exact_rz_pi_over_two() -> quest_math::Result<quest_math::ApproxCertificate> {
    let mut operations = vec![operation(Gate::W); 7];
    operations.push(operation(Gate::S));
    certify_rotation(
        &Sequence {
            qubits: 1,
            operations,
        },
        &Target {
            axis: Axis::Z,
            angle: AngleTarget::RationalPi {
                numerator: "1"
                    .parse()
                    .map_err(|_| quest_math::Error::Invalid("fixture numerator parse".into()))?,
                denominator: "2"
                    .parse()
                    .map_err(|_| quest_math::Error::Invalid("fixture denominator parse".into()))?,
            },
        },
        1.0e-20_f64.to_bits(),
        Limits::default(),
    )
}

#[gtest]
fn exact_controlled_rotation_matches_signed_nonsorted_full_matrix() -> Result<()> {
    let base = exact_rz_pi_over_two()?;
    let controls = [
        Control {
            qubit: 1,
            positive: false,
        },
        Control {
            qubit: 0,
            positive: true,
        },
    ];
    let lifted = lift_controlled_rotation(&base, 3, 2, &controls, Limits::default())?;
    expect_eq!(lifted.base(), &base);
    expect_eq!(lifted.target_qubit(), 2);
    expect_eq!(lifted.controls(), controls.as_slice());
    expect_eq!(lifted.bound_squared(), base.bound_squared());

    let matrix = reconstruct(lifted.sequence(), Limits::default())?;
    for row in 0..8usize {
        for column in 0..8usize {
            let expected = if row != column {
                Cyclotomic::zero()
            } else if column == 1 {
                Cyclotomic::omega(7)
            } else if column == 5 {
                Cyclotomic::omega(1)
            } else {
                Cyclotomic::one()
            };
            let offset = row
                .checked_mul(8)
                .and_then(|value| value.checked_add(column))
                .ok_or_else(|| std::io::Error::other("fixture matrix offset"))?;
            expect_eq!(matrix.entries().get(offset), Some(&expected));
        }
    }
    Ok(())
}

#[gtest]
fn controlled_lift_rejects_incomplete_overlapping_and_over_budget_interfaces() -> Result<()> {
    let base = exact_rz_pi_over_two()?;
    let valid = [
        Control {
            qubit: 0,
            positive: true,
        },
        Control {
            qubit: 1,
            positive: false,
        },
    ];
    expect_true!(lift_controlled_rotation(&base, 4, 2, &valid, Limits::default()).is_err());
    let overlap = [
        Control {
            qubit: 2,
            positive: true,
        },
        Control {
            qubit: 0,
            positive: true,
        },
    ];
    expect_true!(lift_controlled_rotation(&base, 3, 2, &overlap, Limits::default()).is_err());
    let duplicate = [
        Control {
            qubit: 0,
            positive: true,
        },
        Control {
            qubit: 0,
            positive: false,
        },
    ];
    expect_true!(lift_controlled_rotation(&base, 3, 2, &duplicate, Limits::default()).is_err());
    expect_true!(
        lift_controlled_rotation(
            &base,
            3,
            2,
            &valid,
            Limits {
                gates: 7,
                ..Limits::default()
            },
        )
        .is_err()
    );
    expect_true!(
        lift_controlled_rotation(
            &base,
            3,
            2,
            &valid,
            Limits {
                coefficient_bits: 8,
                ..Limits::default()
            },
        )
        .is_err()
    );
    Ok(())
}

#[gtest]
fn existing_base_scalar_control_is_remapped_to_the_new_target() -> Result<()> {
    let mut operations = vec![operation(Gate::W); 7];
    let conditional_phase = Operation {
        gate: Gate::W,
        targets: vec![],
        controls: vec![Control {
            qubit: 0,
            positive: true,
        }],
    };
    operations.push(conditional_phase.clone());
    operations.push(conditional_phase);
    let base = certify_rotation(
        &Sequence {
            qubits: 1,
            operations,
        },
        &Target {
            axis: Axis::Z,
            angle: AngleTarget::RationalPi {
                numerator: "1".parse()?,
                denominator: "2".parse()?,
            },
        },
        1.0e-20_f64.to_bits(),
        Limits::default(),
    )?;
    let external = [Control {
        qubit: 0,
        positive: false,
    }];
    let lifted = lift_controlled_rotation(&base, 2, 1, &external, Limits::default())?;
    let final_controls = lifted
        .sequence()
        .operations
        .last()
        .map(|operation| operation.controls.as_slice());
    expect_eq!(
        final_controls,
        Some(
            [
                Control {
                    qubit: 0,
                    positive: false,
                },
                Control {
                    qubit: 1,
                    positive: true,
                },
            ]
            .as_slice()
        )
    );
    Ok(())
}

#[gtest]
fn controlled_lift_rechecks_affine_target_identity_budget() -> Result<()> {
    let base = certify_rotation(
        &Sequence {
            qubits: 1,
            operations: vec![],
        },
        &Target {
            axis: Axis::Z,
            angle: AngleTarget::AffinePi {
                radians_numerator: 0.into(),
                radians_denominator: std::ops::Shl::shl(dashu_int::IBig::from(1), 1000usize),
                pi_numerator: 0.into(),
                pi_denominator: 1.into(),
            },
        },
        1e-12f64.to_bits(),
        Limits::default(),
    )?;
    let controls = [Control {
        qubit: 1,
        positive: true,
    }];
    expect_true!(
        lift_controlled_rotation(
            &base,
            2,
            0,
            &controls,
            Limits {
                coefficient_bits: 512,
                ..Limits::default()
            }
        )
        .is_err()
    );
    let lifted = lift_controlled_rotation(&base, 2, 0, &controls, Limits::default())?;
    expect_eq!(lifted.base().target(), base.target());
    Ok(())
}

#[gtest]
fn affine_two_pi_scalar_phase_survives_negative_control_lift() -> Result<()> {
    let base = certify_rotation(
        &Sequence {
            qubits: 1,
            operations: vec![operation(Gate::W); 4],
        },
        &Target {
            axis: Axis::Z,
            angle: AngleTarget::AffinePi {
                radians_numerator: 0.into(),
                radians_denominator: 1.into(),
                pi_numerator: 2.into(),
                pi_denominator: 1.into(),
            },
        },
        1e-12f64.to_bits(),
        Limits::default(),
    )?;
    let controls = [Control {
        qubit: 1,
        positive: false,
    }];
    let lifted = lift_controlled_rotation(&base, 2, 0, &controls, Limits::default())?;
    expect_eq!(lifted.base().target(), base.target());
    let matrix = reconstruct(lifted.sequence(), Limits::default())?;
    for row in 0..4usize {
        for column in 0..4usize {
            let expected = if row != column {
                Cyclotomic::zero()
            } else if column < 2 {
                Cyclotomic::omega(4)
            } else {
                Cyclotomic::one()
            };
            let offset = row
                .checked_mul(4)
                .and_then(|value| value.checked_add(column))
                .ok_or_else(|| std::io::Error::other("fixture matrix offset"))?;
            expect_eq!(matrix.entries().get(offset), Some(&expected));
        }
    }
    Ok(())
}
