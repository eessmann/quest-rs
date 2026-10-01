#![cfg(feature = "synthesis")]
use googletest::prelude::*;
use quest_compile::{NativeSynthesis, RotationGenerator};
use quest_math::{AngleTarget, Axis, Target};

#[gtest]
fn native_compiler_records_the_algorithm_that_produced_the_candidate() -> Result<()> {
    let target = Target {
        axis: Axis::Z,
        angle: AngleTarget::RationalPi {
            numerator: 0.into(),
            denominator: 1.into(),
        },
    };
    let approximation = quest_synthesis::approximate_rotation(
        &target,
        0.01_f64.to_bits(),
        quest_synthesis::SynthesisOptions::default(),
    )?;
    expect_eq!(
        NativeSynthesis::default().algorithm(),
        approximation.algorithm()
    );
    Ok(())
}
