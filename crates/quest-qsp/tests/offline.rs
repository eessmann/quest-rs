#![cfg(feature = "offline-synthesis")]
use dashu_base::Abs;
use googletest::prelude::*;
use quest_polynomial::{Chebyshev, Laurent, Limits, Polynomial};
use quest_qsp::Complex64;
use quest_qsp::offline::{OfflineBuilder, OfflineError, OfflinePolicy};
#[gtest]
fn empty_offline_generalized_laurent_preserves_zero_and_positive_offset() -> Result<()> {
    for offset in [0, 3] {
        let target = Polynomial::new(Laurent::new(offset), vec![], Limits::default())?;
        let original = OfflineBuilder::new().unit_circle_response(&target)?;
        expect_true!(original.policy(OfflinePolicy::default()).is_ok());
    }
    Ok(())
}
#[gtest]
fn explicit_offline_complex_synthesis_exports_then_independently_certifies() -> Result<()> {
    let target = Polynomial::new(
        Laurent::new(0),
        vec![Complex64::new(0.1, 0.2), Complex64::new(-0.3, 0.1)],
        Limits::default(),
    )?;
    let solved = OfflineBuilder::new()
        .unit_circle_response(&target)?
        .policy(OfflinePolicy::default())?
        .solve()?;
    expect_that!(
        solved.certified().report().response().upper_f64(),
        le(1e-11)
    );
    expect_that!(solved.report().attempts().len(), eq(1));
    expect_that!(
        solved
            .report()
            .attempts()
            .first()
            .map(quest_qsp::offline::OfflineAttempt::precision),
        some(eq(128))
    );
    expect_that!(
        solved.report().source_coefficients(),
        eq(target.coefficients())
    );
    Ok(())
}
#[gtest]
fn offline_canonical_phases_keep_the_wx_convention() -> Result<()> {
    let target = Polynomial::new(
        Chebyshev,
        vec![Complex64::new(0.0, 0.0), Complex64::new(0.6, 0.0)],
        Limits::default(),
    )?;
    let solved = OfflineBuilder::new()
        .real_parity_wx(&target)?
        .policy(OfflinePolicy::default())?
        .solve()?;
    for x in [-1.0, -0.3, 0.0, 0.4, 1.0] {
        expect_that!(
            solved.certified().candidate().response(x)?,
            near(0.6 * x, 1e-11)
        );
    }
    Ok(())
}
#[gtest]
fn offline_rejects_work_budgets_and_does_not_hide_export_rounding_failure() -> Result<()> {
    let target = Polynomial::new(
        Laurent::new(0),
        vec![Complex64::new(0.3, 0.4)],
        Limits::default(),
    )?;
    let result = OfflineBuilder::new()
        .unit_circle_response(&target)?
        .policy(OfflinePolicy {
            max_work: 1,
            ..OfflinePolicy::default()
        })?
        .solve();
    expect_true!(matches!(result, Err(OfflineError::Budget(_))));
    let mut policy = OfflinePolicy {
        max_precision: 128,
        ..OfflinePolicy::default()
    };
    policy.certification.response_tolerance = 1e-50;
    policy.certification.reconstruction_tolerance = 1e-50;
    policy.certification.unitarity_tolerance = 1e-50;
    let result = OfflineBuilder::new()
        .unit_circle_response(&target)?
        .policy(policy)?
        .solve();
    expect_true!(matches!(result, Err(OfflineError::Certification { .. })));
    Ok(())
}
#[gtest]
fn explicit_offline_near_boundary_retains_original_input() -> Result<()> {
    let value = 1.0 - 2.0_f64.powi(-40);
    let target = Polynomial::new(
        Laurent::new(0),
        vec![Complex64::new(value, 0.0)],
        Limits::default(),
    )?;
    let solved = OfflineBuilder::new()
        .unit_circle_response(&target)?
        .policy(OfflinePolicy {
            contractivity_margin: 1e-14,
            ..OfflinePolicy::default()
        })?
        .solve()?;
    expect_that!(
        solved.certified().report().response().upper_f64(),
        le(1e-11)
    );
    expect_that!(
        solved.certified().candidate().target(),
        eq(target.coefficients())
    );
    Ok(())
}
#[gtest]
#[ignore = "explicit dense catalog scale fixture; run in release and record timing"]
fn offline_original_degree_8105_catalog_exports_and_certifies() -> Result<()> {
    let bytes = include_bytes!("data/inverse-degree-8105.bin");
    let coefficients = bytes
        .as_chunks::<8>()
        .0
        .iter()
        .map(|chunk| Complex64::new(f64::from_le_bytes(*chunk), 0.0))
        .collect::<Vec<_>>();
    let target = Polynomial::new(Chebyshev, coefficients, Limits::default())?;
    let solved = OfflineBuilder::new()
        .real_parity_wx(&target)?
        .policy(OfflinePolicy::default())?
        .solve()?;
    expect_that!(solved.certified().candidate().target().len(), eq(8106));
    expect_that!(
        solved.certified().report().response().upper_f64(),
        le(1e-11)
    );
    eprintln!(
        "OFFLINE_DEGREE_8105 attempts={} response={:.17e} reconstruction={:.17e}",
        solved.report().attempts().len(),
        solved.certified().report().response().upper_f64(),
        solved.certified().report().reconstruction().upper_f64()
    );
    for attempt in solved.report().attempts() {
        eprintln!("OFFLINE_ATTEMPT {attempt:?}");
    }
    Ok(())
}
#[gtest]
fn offline_divide_inverse_handles_dense_complex_degree_sixteen() -> Result<()> {
    let coefficients = (0_i32..17)
        .map(|k| {
            Complex64::new(
                f64::from(k.rem_euclid(3).saturating_sub(1)) / 128.0,
                f64::from(k.rem_euclid(5).saturating_sub(2)) / 256.0,
            )
        })
        .collect();
    let target = Polynomial::new(Laurent::new(0), coefficients, Limits::default())?;
    let solved = OfflineBuilder::new()
        .unit_circle_response(&target)?
        .policy(OfflinePolicy {
            algorithm: quest_qsp::SynthesisAlgorithm::InverseNlftDivideConquer,
            ..OfflinePolicy::default()
        })?
        .solve()?;
    expect_that!(
        solved.certified().report().reconstruction().upper_f64(),
        le(1e-11)
    );
    expect_that!(solved.certified().report().coefficients().len(), eq(17));
    Ok(())
}
#[gtest]
fn offline_interval_fft_admits_contractivity_beyond_coefficient_l1() -> Result<()> {
    let target = Polynomial::new(
        Chebyshev,
        vec![
            Complex64::new(0.0, 0.0),
            Complex64::new(0.8, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(-0.25, 0.0),
        ],
        Limits::default(),
    )?;
    let solved = OfflineBuilder::new()
        .real_parity_wx(&target)?
        .policy(OfflinePolicy::default())?
        .solve()?;
    expect_that!(
        solved.certified().report().response().upper_f64(),
        le(1e-11)
    );
    expect_true!(
        solved
            .report()
            .contractivity_upper()
            .is_some_and(|bound| *bound < quest_qsp::precision::Binary::from(1))
    );
    Ok(())
}
#[gtest]
fn offline_subnormal_canonical_conversion_retains_original_and_certifies_rounding() -> Result<()> {
    let smallest = f64::from_bits(1);
    let target = Polynomial::new(
        Chebyshev,
        vec![Complex64::new(0.0, 0.0), Complex64::new(smallest, 0.0)],
        Limits::default(),
    )?;
    let solved = OfflineBuilder::new()
        .real_parity_wx(&target)?
        .policy(OfflinePolicy::default())?
        .solve()?;
    expect_that!(
        solved.report().source_coefficients(),
        eq(target.coefficients())
    );
    expect_true!(
        solved.certified().report().conversion().upper() > &quest_qsp::precision::Binary::ZERO
    );
    expect_that!(
        solved.certified().report().conversion().upper_f64(),
        le(smallest)
    );
    expect_that!(
        solved.certified().report().response().upper_f64(),
        le(1e-11)
    );
    Ok(())
}
#[gtest]
fn offline_supports_bit_granular_precision_without_silent_word_rounding() -> Result<()> {
    let target = Polynomial::new(
        Laurent::new(0),
        vec![Complex64::new(0.1, 0.0)],
        Limits::default(),
    )?;
    let solved = OfflineBuilder::new()
        .unit_circle_response(&target)?
        .policy(OfflinePolicy {
            initial_precision: 65,
            max_precision: 65,
            ..OfflinePolicy::default()
        })?
        .solve()?;
    expect_that!(solved.report().attempts().len(), eq(1));
    expect_that!(
        solved
            .report()
            .attempts()
            .first()
            .map(quest_qsp::offline::OfflineAttempt::precision),
        some(eq(65))
    );
    expect_true!(
        solved
            .report()
            .contractivity_upper()
            .is_some_and(|bound| bound.precision() == 65)
    );
    Ok(())
}

use quest_numerics::arithmetic::{
    Backend, BinaryRounding, ExactConstant, F64Backend, MpBackend, MpIntervalBackend, Precision,
    to_f64,
};
use quest_polynomial::{
    Accuracy, AdmittedFunction, DynamicShape, ExactDomain, GenericFunction, LinearSolver,
    MpHouseholder, PrecisionAttempts, PrecisionPair, RemezOptions, RemezRequest, function,
};

fn mp_request<F: AdmittedFunction>(
    f: F,
    degree: usize,
) -> std::result::Result<
    RemezRequest<F, MpBackend, MpIntervalBackend, DynamicShape, MpHouseholder>,
    quest_numerics::arithmetic::ArithmeticError,
> {
    let precision = Precision {
        bits: 128,
        ..Precision::default()
    };
    Ok(RemezRequest::new(
        f,
        ExactDomain::binary64(-1.0, 1.0),
        DynamicShape(degree.saturating_add(1)),
        MpBackend::new(precision)?,
        MpIntervalBackend::new(precision)?,
        MpHouseholder,
    ))
}

#[gtest]
fn shared_remez_enforces_work_and_storage_before_evaluation() -> Result<()> {
    for limits in [
        Limits {
            max_work: 0,
            ..Limits::default()
        },
        Limits {
            max_work: 1,
            ..Limits::default()
        },
        Limits {
            max_bytes: 1,
            ..Limits::default()
        },
    ] {
        let result = mp_request(function!(|x| x.exp().sin()), 1)?
            .options(RemezOptions {
                limits,
                ..RemezOptions::default()
            })
            .run();
        let Err(failure) = result else {
            return fail!("resource limit must reject the request");
        };
        expect_that!(
            failure.request().target().evaluate(&mut F64Backend, 0.0)?,
            eq(1.0_f64.sin())
        );
        expect_that!(
            failure.request().configuration().limits.max_work,
            eq(limits.max_work)
        );
        expect_that!(
            failure.request().configuration().limits.max_bytes,
            eq(limits.max_bytes)
        );
    }
    Ok(())
}

#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Operators construct the typed expression; backends perform checked numerical arithmetic"
)]
fn shared_mp_remez_preserves_quadratic_target_and_explicit_export() -> Result<()> {
    let solved = mp_request(function!(|x| x * x), 1)?
        .accuracy(Accuracy::UniformError(ExactConstant::Binary64(0.500_001)))
        .export_binary64()
        .run()?;
    expect_that!(
        solved.binary64_polynomial()?.evaluate_real(0.3)?,
        near(0.5, 1e-12)
    );
    expect_that!(
        to_f64(
            solved.uniform_error().unconditional_bound().upper(),
            BinaryRounding::Up
        )?,
        le(0.500_001)
    );
    expect_that!(
        solved.attempts().first().map(|a| a.precision_bits),
        some(eq(128))
    );
    expect_that!(
        solved.target().evaluate(&mut F64Backend, 0.3)?,
        near(0.09, 1e-16)
    );
    Ok(())
}

#[gtest]
fn shared_remez_distinguishes_uniform_error_from_minimax_gap() -> Result<()> {
    let solved =
        RemezRequest::binary64(function!(|x| x.exp()), ExactDomain::binary64(-1.0, 1.0), 3)
            .accuracy(Accuracy::MinimaxGap(ExactConstant::Binary64(1e-8)))
            .export_binary64()
            .run()?;
    expect_that!(solved.minimax_gap().gap().upper(), le(1e-8));
    expect_that!(
        solved.uniform_error().unconditional_bound().upper(),
        gt(1e-3)
    );
    expect_that!(
        solved.uniform_error().unconditional_bound().upper(),
        le(0.006)
    );
    expect_true!(solved.attempts().iter().any(|a| a.iterations > 1));
    let exported = solved.binary64_polynomial()?;
    for x in [-1.0_f64, -0.5, 0.0, 0.5, 1.0] {
        expect_that!(exported.evaluate_real(x)?, near(x.exp(), 0.006));
    }
    Ok(())
}

#[gtest]
fn shared_remez_rejects_domains_and_retains_unestablished_candidate() -> Result<()> {
    let invalid = mp_request(function!(|x| x.ln()), 1)?.run();
    expect_true!(invalid.is_err());
    let result = mp_request(function!(|x| x.exp()), 1)?
        .options(RemezOptions {
            accuracy: Accuracy::UniformError(ExactConstant::Binary64(1e-6)),
            max_iterations: 2,
            max_subdivisions: 16,
            ..RemezOptions::default()
        })
        .export_binary64()
        .run();
    let Err(failure) = result else {
        return fail!("degree one cannot approximate exp this accurately");
    };
    let candidate = failure
        .attempts()
        .last()
        .and_then(|a| a.candidate.as_ref())
        .ok_or_else(|| std::io::Error::other("missing retained candidate"))?;
    expect_that!(candidate.coefficients().len(), eq(2));
    expect_true!(
        candidate
            .coefficients()
            .iter()
            .all(|x| !x.repr().is_infinite())
    );
    expect_that!(
        failure.request().target().evaluate(&mut F64Backend, 0.0)?,
        eq(1.0)
    );
    Ok(())
}

#[gtest]
fn shared_remez_restarts_exact_original_at_bounded_precisions() -> Result<()> {
    let exact = quest_polynomial::typed::exact(ExactConstant::Rational(1, 3));
    let solved = mp_request(function!(|x| exact), 0)?
        .options(RemezOptions {
            accuracy: Accuracy::UniformError(ExactConstant::Decimal("1e-50".into())),
            root_width: ExactConstant::Decimal("1e-55".into()),
            ..RemezOptions::default()
        })
        .run_with_precisions(PrecisionAttempts {
            attempts: vec![
                PrecisionPair {
                    candidate_bits: 64,
                    proof_bits: 64,
                },
                PrecisionPair {
                    candidate_bits: 256,
                    proof_bits: 256,
                },
            ],
            max_total_work: 1_000_000,
        })?;
    expect_that!(solved.attempts().len(), eq(2));
    expect_that!(
        solved.attempts().last().map(|a| a.precision_bits),
        some(eq(256))
    );
    // A pure MP certificate cannot authorize an implicit binary64 conversion.
    expect_true!(solved.binary64_polynomial().is_err());
    let mut p = MpBackend::new(Precision::default())?;
    let zero = p.point(0.0)?;
    let actual = solved.target().evaluate(&mut p, zero)?;
    expect_eq!(actual, p.constant(&ExactConstant::Rational(1, 3))?);
    Ok(())
}

#[gtest]
fn shared_remez_failure_keeps_all_attempts_and_exact_target() -> Result<()> {
    let exact = quest_polynomial::typed::exact(ExactConstant::Decimal("0.1".into()));
    let result = mp_request(function!(|x| exact), 0)?
        .options(RemezOptions {
            accuracy: Accuracy::UniformError(ExactConstant::Decimal("1e-50".into())),
            root_width: ExactConstant::Decimal("1e-55".into()),
            ..RemezOptions::default()
        })
        .run_with_precisions(PrecisionAttempts {
            attempts: vec![
                PrecisionPair {
                    candidate_bits: 64,
                    proof_bits: 64,
                },
                PrecisionPair {
                    candidate_bits: 128,
                    proof_bits: 128,
                },
            ],
            max_total_work: 1_000_000,
        });
    let Err(failure) = result else {
        return fail!("precision limit cannot establish requested error");
    };
    expect_that!(failure.attempts().len(), eq(2));
    expect_true!(failure.request().precision_policy().is_some());
    let mut p = MpBackend::new(Precision::default())?;
    let zero = p.point(0.0)?;
    let actual = failure.request().target().evaluate(&mut p, zero)?;
    expect_eq!(actual, p.constant(&ExactConstant::Decimal("0.1".into()))?);
    Ok(())
}

#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Operators construct the typed expression; backends perform checked numerical arithmetic"
)]
fn shared_mp_derivatives_and_export_keep_captured_binary64_bits() -> Result<()> {
    let mut p = MpBackend::new(Precision::default())?;
    let x = p.point(0.5)?;
    let jet = function!(|x| (1.0 + x * x).ln()).jet(&mut p, x)?;
    let expected_first = p.constant(&ExactConstant::Rational(4, 5))?;
    let expected_second = p.constant(&ExactConstant::Rational(24, 25))?;
    let tolerance = p.constant(&ExactConstant::Decimal("1e-70".into()))?;
    expect_true!((&jet.first - &expected_first).abs() < tolerance);
    expect_true!((&jet.second - &expected_second).abs() < tolerance);
    let captured = 0.1;
    let solved = mp_request(function!(|x| x * 0.25 + captured), 1)?
        .accuracy(Accuracy::UniformError(ExactConstant::Binary64(1e-12)))
        .export_binary64()
        .run()?;
    let exported = solved.binary64_polynomial()?;
    expect_that!(
        exported.coefficients().first().map(|v| v.re.to_bits()),
        some(eq(captured.to_bits()))
    );
    expect_that!(exported.coefficients().get(1).map(|v| v.re), some(eq(0.25)));
    let zero = p.point(0.0)?;
    expect_eq!(solved.target().evaluate(&mut p, zero)?, p.point(captured)?);
    Ok(())
}

#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "The independent native dyadic residual compares finite QR outputs with bounded integer references"
)]
fn shared_mp_qr_solves_independent_integer_reference() -> Result<()> {
    let mut p = MpBackend::new(Precision {
        bits: 128,
        ..Precision::default()
    })?;
    let matrix = [1, 10, 0, 0, 0, 2, 1, 1, 1]
        .into_iter()
        .map(|n| p.constant(&ExactConstant::Integer(n)))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let rhs = [-18, 6, 3]
        .into_iter()
        .map(|n| p.constant(&ExactConstant::Integer(n)))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let solution = MpHouseholder.solve(&mut p, &matrix, &rhs, 3, Limits::default())?;
    let tolerance = p.constant(&ExactConstant::Decimal("1e-34".into()))?;
    for (actual, expected) in solution.values.iter().zip([2, -2, 3]) {
        let expected = p.constant(&ExactConstant::Integer(expected))?;
        expect_true!((actual - &expected).abs() < tolerance);
    }
    Ok(())
}
