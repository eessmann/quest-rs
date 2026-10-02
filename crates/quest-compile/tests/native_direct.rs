#![cfg(feature = "synthesis")]
use quest_math::{AngleTarget, Axis, Limits, Target, certify_rotation};
use quest_synthesis::SynthesisOptions;
#[test]
fn direct_engine_works_without_any_process_capability() {
    let target = Target {
        axis: Axis::X,
        angle: AngleTarget::DyadicRadians {
            bits: 1.1_f64.to_bits(),
        },
    };
    let candidate = quest_synthesis::approximate_rotation(
        &target,
        0.2_f64.to_bits(),
        SynthesisOptions::default(),
    )
    .unwrap();
    certify_rotation(
        candidate.sequence(),
        &target,
        0.2_f64.to_bits(),
        Limits::default(),
    )
    .unwrap();
}
