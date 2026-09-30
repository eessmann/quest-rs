use quest_math::{AngleTarget, Axis, Limits, Target};
use quest_synthesis::*;
fn target() -> Target {
    Target {
        axis: Axis::Y,
        angle: AngleTarget::AffinePi {
            radians_numerator: 1.into(),
            radians_denominator: 3.into(),
            pi_numerator: 1.into(),
            pi_denominator: 5.into(),
        },
    }
}

#[test]
fn live_grid_storage_is_unavailable_to_candidate_callbacks() {
    let limits = Limits {
        bytes: 8_388_608,
        ..Limits::default()
    };
    // This is exactly the conservative grid reservation at the default
    // 16,384 coefficient bits. Candidate/certificate storage must be additional.
    let result = approximate_rotation(
        &target(),
        0.2_f64.to_bits(),
        SynthesisOptions {
            limits,
            ..SynthesisOptions::default()
        },
    );
    assert!(matches!(
        result,
        Err(SynthesisError::Budget { .. }
            | SynthesisError::Math(
                quest_math::Error::Budget { .. } | quest_math::Error::Resource(_)
            ))
    ));
    let accepted =
        approximate_rotation(&target(), 0.2_f64.to_bits(), SynthesisOptions::default()).unwrap();
    assert_eq!(accepted.limits(), Limits::default());
    assert_eq!(accepted.certificate().target(), &target());
    assert_eq!(accepted.certificate().epsilon_bits(), 0.2_f64.to_bits());
}

#[test]
fn exact_work_limits_are_never_reported_as_rejected_evidence() {
    let matrix = quest_math::ExactMatrix::identity(1, Limits::default()).unwrap();
    for max_work in 0..64 {
        let result = synthesize_matrix(
            &matrix,
            SynthesisOptions {
                max_work,
                ..SynthesisOptions::default()
            },
        );
        assert!(
            matches!(&result, Ok(_) | Err(SynthesisError::WorkExhausted { .. })),
            "valid identity must succeed or exhaust work, got {result:?}"
        );
    }
}
#[test]
fn cancellation_precision_and_resource_exhaustion_are_distinct() {
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    assert!(matches!(
        approximate_rotation(
            &target(),
            0.2_f64.to_bits(),
            SynthesisOptions {
                cancellation: Some(cancellation),
                ..SynthesisOptions::default()
            }
        ),
        Err(SynthesisError::Cancelled)
    ));
    assert!(matches!(
        approximate_rotation(
            &target(),
            1.0e-12_f64.to_bits(),
            SynthesisOptions {
                limits: Limits {
                    precision_bits: 32,
                    ..Limits::default()
                },
                ..SynthesisOptions::default()
            }
        ),
        Err(SynthesisError::PrecisionUnresolved { .. })
    ));
    assert!(
        approximate_rotation(
            &target(),
            0.2_f64.to_bits(),
            SynthesisOptions {
                limits: Limits {
                    bytes: 100,
                    ..Limits::default()
                },
                ..SynthesisOptions::default()
            }
        )
        .is_err()
    );
}
#[test]
#[expect(
    clippy::needless_collect,
    reason = "Spawn every request before joining to exercise concurrent request-owned precision and RNG"
)]
fn concurrent_requests_keep_rng_and_precision_owned_by_the_request() {
    let jobs = (0..3)
        .map(|index| {
            std::thread::spawn(move || {
                let options = SynthesisOptions {
                    seed: 23,
                    limits: Limits {
                        precision_bits: if index == 1 { 192 } else { 256 },
                        ..Limits::default()
                    },
                    ..SynthesisOptions::default()
                };
                approximate_rotation(&target(), 0.2_f64.to_bits(), options).unwrap()
            })
        })
        .collect::<Vec<_>>();
    let mut outputs = jobs.into_iter().map(|job| job.join().unwrap());
    let first = outputs.next().unwrap();
    let middle = outputs.next().unwrap();
    let last = outputs.next().unwrap();
    assert_eq!(first.sequence(), last.sequence());
    assert_eq!(first.working_precision_bits(), 256);
    assert_eq!(middle.working_precision_bits(), 192);
}

#[test]
fn normalization_accounts_for_retained_clifford_search_storage() {
    let input = quest_math::Sequence {
        qubits: 1,
        operations: vec![quest_math::Operation {
            gate: quest_math::Gate::W,
            targets: vec![],
            controls: vec![],
        }],
    };
    let base = quest_math::admit_synthesis_storage(1, 0, 1, Limits::default()).unwrap();
    let options = SynthesisOptions {
        limits: Limits {
            bytes: usize::try_from(base).unwrap() + 1024,
            ..Limits::default()
        },
        ..SynthesisOptions::default()
    };
    assert!(matches!(
        normalize_one_qubit(&input, options),
        Err(SynthesisError::Budget {
            resource: "normal-form storage"
        })
    ));
}
