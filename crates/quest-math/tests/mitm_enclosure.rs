use googletest::{Result, prelude::*};
use quest_math::{
    AngleTarget, Axis, DyadicBox8, ExactMatrix, Limits, Target, adjoint_times_rotation_enclosure,
    rotation_enclosure,
};

#[gtest]
fn exact_identity_and_zero_rotation_boxes_overlap() -> Result<()> {
    let target = Target {
        axis: Axis::Z,
        angle: AngleTarget::RationalPi {
            numerator: 0.into(),
            denominator: 1.into(),
        },
    };
    let limits = Limits::default();
    let identity = ExactMatrix::identity(1, limits)?;
    let exact = DyadicBox8::from_exact(&identity, 128, limits)?;
    let rotation = rotation_enclosure(&target, 128, limits)?;
    expect_eq!(exact.gap_squared(&rotation)?, 0.into());
    let query = adjoint_times_rotation_enclosure(&identity, &target, 128, limits)?;
    expect_eq!(query.gap_squared(&exact)?, 0.into());
    Ok(())
}

#[gtest]
fn box_gap_is_closed_at_the_boundary() -> Result<()> {
    let mut left = DyadicBox8::point(
        4,
        [
            0.into(),
            0.into(),
            0.into(),
            0.into(),
            0.into(),
            0.into(),
            0.into(),
            0.into(),
        ],
    )?;
    let right = DyadicBox8::point(
        4,
        [
            16.into(),
            0.into(),
            0.into(),
            0.into(),
            0.into(),
            0.into(),
            0.into(),
            0.into(),
        ],
    )?;
    expect_eq!(left.gap_squared(&right)?, 256.into());
    left.include(&right)?;
    expect_eq!(left.gap_squared(&right)?, 0.into());
    Ok(())
}

#[gtest]
fn angle_input_kinds_share_certified_enclosure_path() -> Result<()> {
    let limits = Limits {
        precision_bits: 256,
        ..Limits::default()
    };
    let targets = [
        Target {
            axis: Axis::X,
            angle: AngleTarget::DyadicRadians {
                bits: f64::from_bits(1).to_bits(),
            },
        },
        Target {
            axis: Axis::Y,
            angle: AngleTarget::DyadicRadians {
                bits: (-f64::from_bits(1)).to_bits(),
            },
        },
        Target {
            axis: Axis::Z,
            angle: AngleTarget::RationalPi {
                numerator: 1.into(),
                denominator: 17.into(),
            },
        },
        Target {
            axis: Axis::X,
            angle: AngleTarget::AffinePi {
                radians_numerator: 1.into(),
                radians_denominator: 7.into(),
                pi_numerator: (-1).into(),
                pi_denominator: 11.into(),
            },
        },
    ];
    for target in targets {
        let box_ = rotation_enclosure(&target, 128, limits)?;
        expect_eq!(box_.bits(), 128);
        for index in 0..8 {
            let (lower, upper) = box_.coordinate(index)?;
            expect_true!(lower <= upper);
        }
    }
    Ok(())
}
