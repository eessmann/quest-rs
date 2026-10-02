use dashu_int::IBig;
use googletest::{Result, prelude::*};
use quest_math::{AngleTarget, Axis, Gate, Limits, Operation, Sequence, Target, certify_rotation};
fn operation(gate: Gate) -> Operation {
    Operation {
        gate,
        targets: if gate == Gate::W { vec![] } else { vec![0] },
        controls: vec![],
    }
}
fn target(axis: Axis, numerator: i64, denominator: i64) -> Target {
    Target {
        axis,
        angle: AngleTarget::RationalPi {
            numerator: IBig::from(numerator),
            denominator: IBig::from(denominator),
        },
    }
}
fn sequence(gates: &[Gate]) -> Sequence {
    Sequence {
        qubits: 1,
        operations: gates.iter().copied().map(operation).collect(),
    }
}
fn affine(axis: Axis, r_num: i64, r_den: i64, s_num: i64, s_den: i64) -> Target {
    Target {
        axis,
        angle: AngleTarget::AffinePi {
            radians_numerator: r_num.into(),
            radians_denominator: r_den.into(),
            pi_numerator: s_num.into(),
            pi_denominator: s_den.into(),
        },
    }
}
#[gtest]
fn affine_pi_certifies_both_exact_terms_and_retains_target() -> Result<()> {
    // 1/3 - pi/10 is about 0.01917 radians, so identity passes at 0.03
    // and fails at 0.01. Neither rational coefficient is dyadic.
    let target = affine(Axis::Z, 1, 3, -1, 10);
    let candidate = sequence(&[]);
    let proof = certify_rotation(&candidate, &target, 0.03f64.to_bits(), Limits::default())?;
    expect_eq!(proof.target(), &target);
    expect_true!(
        certify_rotation(&candidate, &target, 0.01f64.to_bits(), Limits::default()).is_err()
    );
    Ok(())
}
#[gtest]
fn affine_pi_reduction_keeps_full_four_pi_phase() -> Result<()> {
    let identity = sequence(&[]);
    let negative_identity = sequence(&[Gate::W, Gate::W, Gate::W, Gate::W]);
    let two_pi = affine(Axis::Z, 0, 1, 2, 1);
    let four_pi = affine(Axis::Z, 0, 1, 4, 1);
    certify_rotation(
        &negative_identity,
        &two_pi,
        1e-12f64.to_bits(),
        Limits::default(),
    )?;
    expect_true!(
        certify_rotation(&identity, &two_pi, 1e-12f64.to_bits(), Limits::default()).is_err()
    );
    certify_rotation(&identity, &four_pi, 1e-12f64.to_bits(), Limits::default())?;
    expect_true!(
        certify_rotation(
            &negative_identity,
            &four_pi,
            1e-12f64.to_bits(),
            Limits::default()
        )
        .is_err()
    );
    Ok(())
}
#[gtest]
fn affine_pi_rejects_bad_denominators_and_input_budget() {
    let id = sequence(&[]);
    for target in [affine(Axis::Z, 1, 0, 0, 1), affine(Axis::Z, 0, 1, 1, 0)] {
        expect_true!(certify_rotation(&id, &target, 0.5f64.to_bits(), Limits::default()).is_err());
    }
    let target = affine(Axis::Z, 17, 1, 0, 1);
    expect_true!(
        certify_rotation(
            &id,
            &target,
            0.5f64.to_bits(),
            Limits {
                coefficient_bits: 4,
                ..Limits::default()
            }
        )
        .is_err()
    );
}
#[gtest]
fn large_nondyadic_affine_radians_require_refinement_and_respect_budget() -> Result<()> {
    let huge = Target {
        axis: Axis::X,
        angle: AngleTarget::AffinePi {
            radians_numerator: std::ops::Add::add(std::ops::Shl::shl(IBig::from(1), 400usize), 1),
            radians_denominator: 3.into(),
            pi_numerator: 0.into(),
            pi_denominator: 1.into(),
        },
    };
    let candidate = sequence(&[]);
    expect_true!(certify_rotation(&candidate, &huge, 3.0f64.to_bits(), Limits::default()).is_err());
    let proof = certify_rotation(
        &candidate,
        &huge,
        3.0f64.to_bits(),
        Limits {
            precision_bits: 1024,
            ..Limits::default()
        },
    )?;
    expect_true!(proof.precision_bits() >= 512);
    expect_true!(
        certify_rotation(
            &candidate,
            &huge,
            3.0f64.to_bits(),
            Limits {
                coefficient_bits: 400,
                ..Limits::default()
            }
        )
        .is_err()
    );
    let negative_huge = Target {
        axis: Axis::Y,
        angle: AngleTarget::AffinePi {
            radians_numerator: std::ops::Add::add(std::ops::Shl::shl(IBig::from(1), 400usize), 1),
            radians_denominator: (-3).into(),
            pi_numerator: (-1).into(),
            pi_denominator: (-10).into(),
        },
    };
    expect_true!(
        certify_rotation(
            &candidate,
            &negative_huge,
            3.0f64.to_bits(),
            Limits::default()
        )
        .is_err()
    );
    let negative_proof = certify_rotation(
        &candidate,
        &negative_huge,
        3.0f64.to_bits(),
        Limits {
            precision_bits: 1024,
            ..Limits::default()
        },
    )?;
    expect_true!(negative_proof.precision_bits() >= 512);
    expect_eq!(negative_proof.target(), &negative_huge);
    Ok(())
}
#[gtest]
fn mathematical_rotation_certificates_keep_phase_and_input_identity() -> Result<()> {
    let limits = Limits::default();
    let epsilon = 1e-12f64.to_bits();
    let rz = sequence(&[
        Gate::W,
        Gate::W,
        Gate::W,
        Gate::W,
        Gate::W,
        Gate::W,
        Gate::Z,
    ]);
    let pi = target(Axis::Z, 1, 1);
    let exact = certify_rotation(&rz, &pi, epsilon, limits)?;
    expect_eq!(exact.target(), &pi);
    expect_eq!(exact.candidate(), &rz);
    expect_true!(certify_rotation(&sequence(&[Gate::Z]), &pi, epsilon, limits).is_err());
    let dyadic = Target {
        axis: Axis::Z,
        angle: AngleTarget::DyadicRadians {
            bits: std::f64::consts::PI.to_bits(),
        },
    };
    expect_true!(certify_rotation(&rz, &dyadic, 1e-20f64.to_bits(), limits).is_err());
    certify_rotation(&rz, &pi, 1e-20f64.to_bits(), limits)?;
    Ok(())
}
#[gtest]
fn basis_conjugation_orientations_are_certified_independently() -> Result<()> {
    let middle = [
        Gate::W,
        Gate::W,
        Gate::W,
        Gate::W,
        Gate::W,
        Gate::W,
        Gate::W,
        Gate::S,
    ];
    let mut rx = vec![Gate::H];
    rx.extend(middle);
    rx.push(Gate::H);
    certify_rotation(
        &sequence(&rx),
        &target(Axis::X, 1, 2),
        1e-12f64.to_bits(),
        Limits::default(),
    )?;
    let mut ry = vec![Gate::Sdg, Gate::H];
    ry.extend(middle);
    ry.extend([Gate::H, Gate::S]);
    certify_rotation(
        &sequence(&ry),
        &target(Axis::Y, 1, 2),
        1e-12f64.to_bits(),
        Limits::default(),
    )?;
    let mut reversed = vec![Gate::S, Gate::H];
    reversed.extend(middle);
    reversed.extend([Gate::H, Gate::Sdg]);
    expect_true!(
        certify_rotation(
            &sequence(&reversed),
            &target(Axis::Y, 1, 2),
            1e-12f64.to_bits(),
            Limits::default()
        )
        .is_err()
    );
    Ok(())
}
#[gtest]
fn tiny_dyadic_angles_have_proved_pass_fail_tolerance_boundaries() -> Result<()> {
    let id = sequence(&[]);
    let tiny = Target {
        axis: Axis::Z,
        angle: AngleTarget::DyadicRadians {
            bits: 1e-13f64.to_bits(),
        },
    };
    let certificate = certify_rotation(&id, &tiny, 1e-12f64.to_bits(), Limits::default())?;
    expect_true!(certificate.precision_bits() > 0);
    expect_true!(certify_rotation(&id, &tiny, 1e-14f64.to_bits(), Limits::default()).is_err());
    for epsilon in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        expect_true!(certify_rotation(&id, &tiny, epsilon.to_bits(), Limits::default()).is_err());
    }
    Ok(())
}

#[gtest]
fn golden_frobenius_bound_for_t_minus_identity_is_two_minus_sqrt_two() -> Result<()> {
    let certificate = certify_rotation(
        &sequence(&[Gate::T]),
        &target(Axis::Z, 0, 1),
        0.766f64.to_bits(),
        Limits::default(),
    )?;
    let denominator = IBig::from(100_000_000_000_000_000u64);
    let lower = quest_math::RBig::from_parts_signed(
        IBig::from(58_578_643_762_690_494u64),
        denominator.clone(),
    );
    let upper =
        quest_math::RBig::from_parts_signed(IBig::from(58_578_643_762_690_496u64), denominator);
    expect_true!(certificate.bound_squared() >= &lower);
    expect_true!(certificate.bound_squared() <= &upper);
    expect_true!(
        certify_rotation(
            &sequence(&[Gate::T]),
            &target(Axis::Z, 0, 1),
            0.765f64.to_bits(),
            Limits::default()
        )
        .is_err()
    );
    Ok(())
}
#[gtest]
fn huge_finite_dyadic_angles_use_certified_range_reduction() -> Result<()> {
    let huge = Target {
        axis: Axis::Z,
        angle: AngleTarget::DyadicRadians {
            bits: f64::MAX.to_bits(),
        },
    };
    expect_true!(
        certify_rotation(&sequence(&[]), &huge, 3.0f64.to_bits(), Limits::default()).is_err()
    );
    let proof = certify_rotation(
        &sequence(&[]),
        &huge,
        3.0f64.to_bits(),
        Limits {
            precision_bits: 2048,
            ..Limits::default()
        },
    )?;
    expect_true!(proof.precision_bits() >= 1024);
    Ok(())
}
#[gtest]
fn exact_dyadic_decoding_preserves_subnormals_and_rejects_nonfinite_payloads() -> Result<()> {
    let limits = Limits::default();
    let smallest = quest_math::dyadic_from_bits(1, limits)?;
    expect_eq!(
        smallest,
        quest_math::RBig::from_parts_signed(
            IBig::from(1),
            std::ops::Shl::shl(IBig::from(1), 1074usize)
        )
    );
    expect_eq!(
        quest_math::dyadic_from_bits(0x8000_0000_0000_0001, limits)?,
        std::ops::Neg::neg(smallest)
    );
    for bits in [
        f64::NAN.to_bits(),
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
    ] {
        expect_true!(quest_math::dyadic_from_bits(bits, limits).is_err());
    }
    let negative_zero = Target {
        axis: Axis::X,
        angle: AngleTarget::DyadicRadians {
            bits: 0x8000_0000_0000_0000,
        },
    };
    let certificate = certify_rotation(&sequence(&[]), &negative_zero, 1e-12f64.to_bits(), limits)?;
    expect_eq!(certificate.target(), &negative_zero);
    expect_true!(
        certify_rotation(
            &sequence(&[]),
            &target(Axis::Z, 0, 1),
            1e-12f64.to_bits(),
            Limits {
                taylor_terms: 0,
                ..limits
            }
        )
        .is_err()
    );
    Ok(())
}

#[gtest]
fn rotation_two_pi_phase_is_not_reduced_away() -> Result<()> {
    let negative_identity = sequence(&[Gate::W, Gate::W, Gate::W, Gate::W]);
    certify_rotation(
        &negative_identity,
        &target(Axis::Z, 2, 1),
        1e-12f64.to_bits(),
        Limits::default(),
    )?;
    expect_true!(
        certify_rotation(
            &sequence(&[]),
            &target(Axis::Z, 2, 1),
            1e-12f64.to_bits(),
            Limits::default()
        )
        .is_err()
    );
    certify_rotation(
        &sequence(&[]),
        &target(Axis::Z, 4, 1),
        1e-12f64.to_bits(),
        Limits::default(),
    )?;
    Ok(())
}
