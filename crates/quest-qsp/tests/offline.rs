#![cfg(feature = "offline-synthesis")]
use googletest::prelude::*;
use quest_polynomial::{Chebyshev, Laurent, Limits, Polynomial};
use quest_qsp::Complex64;
use quest_qsp::offline::{OfflineBuilder, OfflineError, OfflinePolicy};
#[gtest]
fn explicit_offline_complex_synthesis_exports_then_independently_certifies() -> Result<()> {
    let target = Polynomial::new(
        Laurent::new(0),
        vec![Complex64::new(0.1, 0.2), Complex64::new(-0.3, 0.1)],
        Limits::default(),
    )?;
    let solved = OfflineBuilder::new()
        .generalized(&target)?
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
        .canonical(&target)?
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
        .generalized(&target)?
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
        .generalized(&target)?
        .policy(policy)?
        .solve();
    expect_true!(matches!(result, Err(OfflineError::Certification { .. })));
    Ok(())
}
#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Operators construct the single expression before offline evaluation"
)]
fn offline_arbitrary_remez_uses_original_expression_and_pivoted_qr() -> Result<()> {
    use quest_polynomial::{Interval, function};
    use quest_qsp::offline::{OfflineRemezBuilder, OfflineRemezPolicy};
    let f = function!(|x| x.clone() * x);
    let result = OfflineRemezBuilder::new()
        .function(f)
        .domain(Interval::new(-1.0, 1.0)?)?
        .degree(1)
        .policy(OfflineRemezPolicy {
            error_tolerance: 0.500_001,
            ..OfflineRemezPolicy::default()
        })?
        .solve()?;
    expect_that!(result.polynomial().evaluate_real(0.3)?, near(0.5, 1e-12));
    expect_that!(result.error_bound().upper(), le(0.500_001));
    expect_that!(result.precision(), eq(128));
    Ok(())
}
#[gtest]
fn offline_nonpolynomial_remez_exchanges_and_proves_export_error() -> Result<()> {
    use quest_polynomial::{Interval, function};
    use quest_qsp::offline::{OfflineRemezBuilder, OfflineRemezPolicy};
    let solved = OfflineRemezBuilder::new()
        .function(function!(|x| x.exp()))
        .domain(Interval::new(-1.0, 1.0)?)?
        .degree(3)
        .policy(OfflineRemezPolicy {
            error_tolerance: 0.006,
            ..OfflineRemezPolicy::default()
        })?
        .solve()?;
    expect_that!(solved.iterations(), gt(1));
    expect_that!(solved.error_bound().upper(), le(0.006));
    for x in [-1.0_f64, -0.5, 0.0, 0.5, 1.0] {
        expect_that!(solved.polynomial().evaluate_real(x)?, near(x.exp(), 0.006));
    }
    Ok(())
}
#[gtest]
fn offline_remez_rejects_domain_and_insufficient_error_budget() -> Result<()> {
    use quest_polynomial::{Interval, function};
    use quest_qsp::offline::{OfflineRemezBuilder, OfflineRemezPolicy};
    expect_true!(
        OfflineRemezBuilder::new()
            .function(function!(|x| x.ln()))
            .domain(Interval::new(-1.0, 1.0)?)
            .is_err()
    );
    let result = OfflineRemezBuilder::new()
        .function(function!(|x| x.exp()))
        .domain(Interval::new(-1.0, 1.0)?)?
        .degree(1)
        .policy(OfflineRemezPolicy {
            error_tolerance: 1e-6,
            max_subdivisions: 16,
            offline: OfflinePolicy {
                max_precision: 128,
                ..OfflinePolicy::default()
            },
            ..OfflineRemezPolicy::default()
        })?
        .solve();
    let Err(OfflineError::ApproximationNotEstablished { report }) = result else {
        return Err(std::io::Error::other("expected retained approximation failure").into());
    };
    let coefficients: &[astro_float::BigFloat] = report
        .arbitrary_coefficients()
        .ok_or_else(|| std::io::Error::other("missing retained arbitrary coefficients"))?;
    expect_that!(coefficients.len(), eq(2));
    expect_true!(
        coefficients
            .iter()
            .all(|value| !value.is_nan() && !value.is_inf())
    );
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
        .generalized(&target)?
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
        .canonical(&target)?
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
        .generalized(&target)?
        .policy(OfflinePolicy::default())?
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
        .canonical(&target)?
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
            .is_some_and(|bound| *bound < astro_float::BigFloat::from_u64(1, 64))
    );
    Ok(())
}
#[gtest]
fn offline_remez_restarts_original_expression_at_higher_precision() -> Result<()> {
    use quest_polynomial::{Interval, function};
    use quest_qsp::offline::{OfflineRemezBuilder, OfflineRemezPolicy};
    let solved = OfflineRemezBuilder::new()
        .function(function!(|x| x.exp()))
        .domain(Interval::new(-1.0, 1.0)?)?
        .degree(3)
        .policy(OfflineRemezPolicy {
            error_tolerance: 0.006,
            exchange_tolerance: 1e-30,
            max_iterations: 16,
            offline: OfflinePolicy {
                initial_precision: 64,
                max_precision: 128,
                ..OfflinePolicy::default()
            },
            ..OfflineRemezPolicy::default()
        })?
        .solve()?;
    expect_that!(solved.attempts(), eq(2));
    expect_that!(solved.precision(), eq(128));
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
        .canonical(&target)?
        .policy(OfflinePolicy::default())?
        .solve()?;
    expect_that!(
        solved.report().source_coefficients(),
        eq(target.coefficients())
    );
    expect_true!(
        solved.certified().report().conversion().upper() > &astro_float::BigFloat::new(256)
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
fn offline_insufficient_precision_preserves_original_function_and_attempt_count() -> Result<()> {
    use quest_polynomial::{Interval, function};
    use quest_qsp::offline::{OfflineRemezBuilder, OfflineRemezPolicy};
    let original = function!(|x| x.exp());
    let result = OfflineRemezBuilder::new()
        .function(original.clone())
        .domain(Interval::new(-1.0, 1.0)?)?
        .degree(3)
        .policy(OfflineRemezPolicy {
            error_tolerance: 0.006,
            exchange_tolerance: 1e-30,
            max_iterations: 16,
            offline: OfflinePolicy {
                initial_precision: 64,
                max_precision: 64,
                ..OfflinePolicy::default()
            },
            ..OfflineRemezPolicy::default()
        })?
        .solve();
    let Err(OfflineError::ApproximationNotEstablished { report }) = result else {
        return fail!("64 bits cannot establish this numerical exchange tolerance");
    };
    expect_that!(report.precision(), eq(64));
    expect_that!(report.attempts(), eq(1));
    expect_that!(
        report.function().evaluate(0.3)?,
        eq(original.evaluate(0.3)?)
    );
    Ok(())
}

#[gtest]
fn offline_rejects_precision_that_would_be_silently_rounded_to_a_backend_word() -> Result<()> {
    let target = Polynomial::new(
        Laurent::new(0),
        vec![Complex64::new(0.1, 0.0)],
        Limits::default(),
    )?;
    let result = OfflineBuilder::new()
        .generalized(&target)?
        .policy(OfflinePolicy {
            initial_precision: 65,
            ..OfflinePolicy::default()
        });
    expect_true!(matches!(
        result,
        Err(OfflineError::Policy(
            "precision must be a whole backend word"
        ))
    ));
    Ok(())
}
