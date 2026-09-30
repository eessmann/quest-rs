use super::*;
use googletest::prelude::*;
#[gtest]
fn interval_exact_dyadics_preserve_cancellation_and_complex_products() -> Result<()> {
    let a = MpComplex::exact(crate::Complex64::new(0.5, 0.25), 256)?;
    let b = MpComplex::exact(crate::Complex64::new(0.25, -0.5), 256)?;
    let product = a.mul(&b)?;
    expect_true!(product.real().contains_f64(0.25));
    expect_true!(product.imaginary().contains_f64(-0.1875));
    expect_true!(a.sub(&a)?.is_zero());
    Ok(())
}

fn analytic(degree: usize) -> CertificationResult<FrozenCandidate<UnitCircleResponse>> {
    let count = degree
        .checked_add(1)
        .ok_or(CertificationError::Budget("fixture"))?;
    let zero = crate::Complex64::new(0.0, 0.0);
    let one = crate::Complex64::new(1.0, 0.0);
    let mut target = vec![zero; count];
    let mut astar = target.clone();
    *target
        .first_mut()
        .ok_or(CertificationError::Export("fixture"))? = crate::Complex64::new(-0.8, 0.0);
    *astar
        .first_mut()
        .ok_or(CertificationError::Export("fixture"))? = crate::Complex64::new(0.6, 0.0);
    let mut controls = vec![[[one, zero], [zero, one]]; count];
    *controls
        .first_mut()
        .ok_or(CertificationError::Export("fixture"))? = [
        [
            crate::Complex64::new(0.6, 0.0),
            crate::Complex64::new(-0.8, 0.0),
        ],
        [
            crate::Complex64::new(0.8, 0.0),
            crate::Complex64::new(0.6, 0.0),
        ],
    ];
    *controls
        .last_mut()
        .ok_or(CertificationError::Export("fixture"))? =
        [[zero, crate::Complex64::new(-1.0, 0.0)], [one, zero]];
    let admitted = crate::AdmittedTarget {
        source_offset: 0,
        source_length: target.len(),
        source: Arc::new(target.clone()),
        target: Arc::new(target),
        norm_upper: 0.8,
        policy: crate::Policy::default(),
        _mode: std::marker::PhantomData,
    };
    Ok(FrozenCandidate {
        synthesis_precision: crate::SynthesisPrecision::Binary64,
        admitted,
        controls: Arc::new(controls),
        a_star: Arc::new(astar),
        phases: Arc::new(Vec::new()),
        completion_residual: f64::NAN,
        reconstruction_residual: Some(f64::NAN),
        completion_grid: 0,
    })
}
#[gtest]
fn interval_fft_contains_direct_rational_convolution_with_exact_support() -> Result<()> {
    let precision = 128;
    let left: Vec<_> = (0_i32..17)
        .map(|i| {
            MpComplex::exact(
                crate::Complex64::new(
                    f64::from(i) / 32.0,
                    f64::from(8_i32.saturating_sub(i)) / 16.0,
                ),
                precision,
            )
        })
        .collect::<CertificationResult<_>>()?;
    let right: Vec<_> = (0_i32..13)
        .map(|i| {
            MpComplex::exact(
                crate::Complex64::new(f64::from(i) / 16.0, f64::from(i) / 32.0),
                precision,
            )
        })
        .collect::<CertificationResult<_>>()?;
    let mut direct = Context::new(
        30,
        precision,
        CertificationPolicy {
            method: ConvolutionMethod::Direct,
            ..CertificationPolicy::default()
        },
    )?;
    let reference = product::convolve(&left, &right, &mut direct)?;
    let mut fft = Context::new(30, precision, CertificationPolicy::default())?;
    let actual = product::convolve(&left, &right, &mut fft)?;
    expect_that!(actual.len(), eq(29));
    expect_true!(!fft.roots.is_empty());
    for (actual, reference) in actual.iter().zip(&reference) {
        for (actual, reference) in [
            (actual.real(), reference.real()),
            (actual.imaginary(), reference.imaginary()),
        ] {
            expect_that!(actual.lower(), le(reference.lower()));
            expect_that!(actual.upper(), ge(reference.upper()));
        }
    }
    Ok(())
}
#[gtest]
fn all_four_matrix_entries_match_independent_analytic_sparse_product() -> Result<()> {
    let candidate = analytic(32)?;
    let certified = CertificationBuilder::new()
        .candidate(candidate.clone())
        .policy(CertificationPolicy::default())?
        .certify()?;
    let direct = CertificationBuilder::new()
        .candidate(candidate)
        .policy(CertificationPolicy {
            method: ConvolutionMethod::Direct,
            ..CertificationPolicy::default()
        })?
        .certify()?;
    expect_that!(certified.report().coefficients().len(), eq(33));
    for (index, (actual, reference)) in certified
        .report()
        .coefficients()
        .iter()
        .zip(direct.report().coefficients())
        .enumerate()
    {
        let [[a, b], [c, d]] = actual;
        expect_true!(a.real().contains_f64(if index == 0 { -0.8 } else { 0.0 }));
        expect_true!(b.real().contains_f64(if index == 32 { -0.6 } else { 0.0 }));
        expect_true!(c.real().contains_f64(if index == 0 { 0.6 } else { 0.0 }));
        expect_true!(d.real().contains_f64(if index == 32 { -0.8 } else { 0.0 }));
        for (actual, reference) in actual.iter().flatten().zip(reference.iter().flatten()) {
            expect_that!(actual.real().lower(), le(reference.real().upper()));
            expect_that!(actual.real().upper(), ge(reference.real().lower()));
        }
    }
    Ok(())
}
#[gtest]
fn exact_coefficient_violation_is_distinct_from_unestablished_bound() -> Result<()> {
    let mut candidate = analytic(2)?;
    let mut controls = candidate.controls().to_vec();
    let [[a, _], [_, _]] = controls
        .first_mut()
        .ok_or(CertificationError::Export("fixture"))?;
    a.re = 0.7;
    candidate.controls = Arc::new(controls);
    let result = CertificationBuilder::new()
        .candidate(candidate)
        .policy(CertificationPolicy::default())?
        .certify();
    let Err(CertificationError::Violation { metric, report }) = result else {
        return fail!("off-diagonal reconstruction error must be proved");
    };
    expect_that!(metric, eq("reconstruction"));
    expect_that!(report.response().upper_f64(), le(1e-11));
    let [_, upper_right, _, _] = report.entries();
    expect_that!(upper_right.lower_f64(), gt(0.09));
    Ok(())
}
#[gtest]
#[ignore = "degree-8192 cold interval-FFT scale fixture; run explicitly"]
fn degree_8192_analytic_product_is_certified_by_interval_fft() -> Result<()> {
    let certified = CertificationBuilder::new()
        .candidate(analytic(8192)?)
        .policy(CertificationPolicy::default())?
        .certify()?;
    eprintln!(
        "degree=8192 response_upper={:.6e} completion_upper={:.6e} reconstruction_upper={:.6e} unitarity_upper={:.6e} attempts={:?}",
        certified.report().response().upper_f64(),
        certified.report().completion().upper_f64(),
        certified.report().reconstruction().upper_f64(),
        certified.report().unitarity().upper_f64(),
        certified.report().attempts()
    );
    expect_that!(certified.report().coefficients().len(), eq(8193));
    expect_that!(certified.report().response().upper_f64(), le(1e-11));
    expect_that!(certified.report().unitarity().upper_f64(), le(1e-11));
    Ok(())
}

fn tiny_phase() -> FrozenCandidate<RealParityWx> {
    let tiny = 2.0_f64.powi(-100);
    let value = crate::Complex64::new(tiny, 0.0);
    let zero = crate::Complex64::new(0.0, 0.0);
    FrozenCandidate {
        synthesis_precision: crate::SynthesisPrecision::Binary64,
        admitted: crate::AdmittedTarget {
            source_offset: 0,
            source_length: 1,
            target: Arc::new(vec![value]),
            source: Arc::new(vec![value]),
            norm_upper: tiny,
            policy: crate::Policy::default(),
            _mode: std::marker::PhantomData,
        },
        // Deliberately useless production diagnostic controls: phases are authority.
        controls: Arc::new(vec![[[zero, zero], [zero, zero]]]),
        phases: Arc::new(vec![tiny]),
        a_star: Arc::new(vec![crate::Complex64::new(1.0, 0.0)]),
        completion_residual: f64::NAN,
        reconstruction_residual: Some(f64::NAN),
        completion_grid: 0,
    }
}
#[gtest]
fn precision_retries_verify_same_phase_export_and_separate_insufficient_bounds() -> Result<()> {
    let candidate = tiny_phase();
    let original = candidate.phases().to_vec();
    let limited = CertificationPolicy {
        initial_precision: 64,
        max_precision: 64,
        response_tolerance: 1e-80,
        method: ConvolutionMethod::Direct,
        ..CertificationPolicy::default()
    };
    let result = CertificationBuilder::new()
        .candidate(candidate.clone())
        .policy(limited)?
        .certify();
    let Err(CertificationError::NotEstablished { report }) = result else {
        return fail!("low verifier precision must produce an insufficient bound");
    };
    expect_that!(report.attempts().len(), eq(1));
    expect_that!(report.response().lower_f64(), le(1e-80));
    expect_that!(report.response().upper_f64(), gt(1e-80));
    let certified = CertificationBuilder::new()
        .candidate(candidate)
        .policy(CertificationPolicy {
            max_precision: 256,
            ..limited
        })?
        .certify()?;
    expect_that!(certified.report().attempts().len(), eq(3));
    expect_that!(certified.report().response().upper_f64(), le(1e-80));
    expect_that!(certified.candidate().phases(), eq(original.as_slice()));
    expect_that!(
        certified.candidate().controls().first(),
        some(eq(&[[crate::Complex64::new(0.0, 0.0); 2]; 2]))
    );
    Ok(())
}
#[gtest]
fn source_precision_memory_and_work_budgets_fail_explicitly() -> Result<()> {
    for policy in [
        CertificationPolicy {
            initial_precision: 32,
            ..CertificationPolicy::default()
        },
        CertificationPolicy {
            max_coefficients: 1,
            ..CertificationPolicy::default()
        },
        CertificationPolicy {
            max_bytes: 1,
            ..CertificationPolicy::default()
        },
    ] {
        expect_true!(
            CertificationBuilder::new()
                .candidate(analytic(2)?)
                .policy(policy)
                .is_err()
        );
    }
    let result = CertificationBuilder::new()
        .candidate(analytic(2)?)
        .policy(CertificationPolicy {
            max_work: 1,
            ..CertificationPolicy::default()
        })?
        .certify();
    expect_true!(matches!(result, Err(CertificationError::Budget(_))));
    Ok(())
}

#[gtest]
fn wx_conversion_is_checked_against_original_source_independently() -> Result<()> {
    let mut candidate = tiny_phase();
    candidate.admitted.target = Arc::new(vec![crate::Complex64::new(0.1, 0.0)]);
    let policy = CertificationPolicy {
        completion_tolerance: 1.0,
        reconstruction_tolerance: 1.0,
        ..CertificationPolicy::default()
    };
    let result = CertificationBuilder::new()
        .candidate(candidate)
        .policy(policy)?
        .certify();
    let Err(CertificationError::Violation { metric, report }) = result else {
        return fail!("changed target must fail original-source conversion");
    };
    expect_that!(metric, eq("conversion"));
    expect_that!(report.response().upper_f64(), le(1e-11));
    expect_that!(report.conversion().lower_f64(), gt(0.09));
    Ok(())
}

#[gtest]
fn multiprecision_acceptance_does_not_round_an_above_tolerance_bound_to_binary64() -> Result<()> {
    let mut candidate = analytic(1)?;
    let zero = crate::Complex64::new(0.0, 0.0);
    let one = crate::Complex64::new(1.0, 0.0);
    let tolerance = 2.0_f64.powi(-20);
    let tiny = crate::Complex64::new(2.0_f64.powi(-50), 0.0);
    // Actual top-left response is tolerance*z + 2^-100. Rounding the exact
    // l1 bound to nearest binary64 would erase the positive excess.
    candidate.controls = Arc::new(vec![
        [[crate::Complex64::new(tolerance, 0.0), tiny], [zero, one]],
        [[one, zero], [tiny, one]],
    ]);
    candidate.admitted.target = Arc::new(vec![zero; 2]);
    candidate.admitted.source = Arc::clone(&candidate.admitted.target);
    candidate.a_star = Arc::new(vec![zero; 2]);
    let result = CertificationBuilder::new()
        .candidate(candidate)
        .policy(CertificationPolicy {
            initial_precision: 128,
            max_precision: 128,
            response_tolerance: tolerance,
            completion_tolerance: 10.0,
            reconstruction_tolerance: 10.0,
            unitarity_tolerance: 10.0,
            method: ConvolutionMethod::Direct,
            ..CertificationPolicy::default()
        })?
        .certify();
    let Err(CertificationError::NotEstablished { report }) = result else {
        return fail!(
            "a multiprecision upper bound above tolerance cannot be accepted after rounding"
        );
    };
    expect_that!(
        to_f64(report.response().upper(), BinaryRounding::Nearest)?,
        eq(tolerance)
    );
    expect_that!(report.response().upper_f64(), gt(tolerance));
    expect_true!(report.response().upper() > &exact_from_f64(tolerance, 128)?);
    Ok(())
}

#[gtest]
fn exact_admission_and_outward_summary_keep_the_smallest_subnormal() -> Result<()> {
    let smallest = f64::from_bits(1);
    let original = MpInterval::exact(smallest, 64)?;
    expect_true!(original.lower() > &BigFloat::from_u64(0, 64));
    expect_that!(
        to_f64(original.lower(), BinaryRounding::Nearest)?.to_bits(),
        eq(1)
    );
    let half = original.divide_usize(2)?;
    expect_that!(to_f64(half.upper(), BinaryRounding::Up)?.to_bits(), eq(1));
    expect_that!(to_f64(half.lower(), BinaryRounding::Down)?.to_bits(), eq(0));
    let doubled = half.add(&half)?;
    expect_that!(doubled.lower(), eq(original.lower()));
    expect_that!(doubled.upper(), eq(original.upper()));
    Ok(())
}

#[gtest]
fn wx_large_phase_uses_the_exact_export_without_period_reduction() -> Result<()> {
    let phase = 1e20_f64;
    // Fixed independent binary64 reference; do not obtain it from the verifier.
    let response = -0.645_251_285_265_780_8;
    let complement = 0.763_970_404_441_728_3;
    let mut candidate = tiny_phase();
    candidate.phases = Arc::new(vec![phase]);
    candidate.admitted.target = Arc::new(vec![crate::Complex64::new(response, 0.0)]);
    candidate.admitted.source = Arc::clone(&candidate.admitted.target);
    candidate.a_star = Arc::new(vec![crate::Complex64::new(complement, 0.0)]);
    let certified = CertificationBuilder::new()
        .candidate(candidate)
        .policy(CertificationPolicy::default())?
        .certify()?;
    expect_that!(certified.report().response().upper_f64(), le(1e-11));
    expect_that!(certified.candidate().phases(), eq(&[phase]));
    let reduced = phase % std::f64::consts::TAU;
    expect_true!((reduced.sin() - response).abs() > 0.1);
    Ok(())
}

#[gtest]
fn dyadic_twiddle_axes_octants_and_periodicity_use_exact_integer_reduction() -> Result<()> {
    let mut constants = Consts::new()?;
    for length in [1, 2, 4, 8, 16, 64] {
        for k in 0..length {
            let (s, c) = MpInterval::twiddle(k, length, 128, &mut constants)?;
            let (period_s, period_c) = MpInterval::twiddle(
                k.checked_add(length)
                    .ok_or(CertificationError::Budget("fixture index"))?,
                length,
                128,
                &mut constants,
            )?;
            expect_that!(s.lower(), eq(period_s.lower()));
            expect_that!(s.upper(), eq(period_s.upper()));
            expect_that!(c.lower(), eq(period_c.lower()));
            expect_that!(c.upper(), eq(period_c.upper()));
            expect_true!(s.square()?.add(&c.square()?)?.contains_f64(1.0));
            let scaled = k
                .checked_mul(4)
                .ok_or(CertificationError::Budget("fixture index"))?;
            if scaled.is_multiple_of(length) {
                let (sin, cos) = match scaled
                    .checked_div(length)
                    .ok_or(CertificationError::Budget("fixture length"))?
                {
                    0 => (0.0, 1.0),
                    1 => (1.0, 0.0),
                    2 => (0.0, -1.0),
                    _ => (-1.0, 0.0),
                };
                expect_that!(s.lower(), eq(s.upper()));
                expect_that!(c.lower(), eq(c.upper()));
                expect_true!(s.contains_f64(sin));
                expect_true!(c.contains_f64(cos));
            }
        }
    }
    let diagonal = MpInterval::exact(0.5, 128)?.sqrt()?;
    for k in [1, 3, 5, 7] {
        let (s, c) = MpInterval::twiddle(k, 8, 128, &mut constants)?;
        let expected_s = if k < 4 {
            diagonal.clone()
        } else {
            diagonal.neg()
        };
        let expected_c = if k == 1 || k == 7 {
            diagonal.clone()
        } else {
            diagonal.neg()
        };
        for (actual, expected) in [(s, expected_s), (c, expected_c)] {
            expect_that!(actual.lower(), le(expected.upper()));
            expect_that!(actual.upper(), ge(expected.lower()));
        }
    }
    Ok(())
}

#[gtest]
fn directed_interval_non_ties_and_nonfinite_endpoints_are_checked() -> Result<()> {
    let third = MpInterval::integer(1, 64).divide_usize(3)?;
    let one = BigFloat::from_u64(1, 256);
    let three = BigFloat::from_u64(3, 256);
    expect_true!(third.lower().mul(&three, 256, Round::ToEven) < one);
    expect_true!(third.upper().mul(&three, 256, Round::ToEven) > one);
    let negative = third.neg();
    expect_that!(negative.lower(), eq(&third.upper().neg()));
    expect_that!(negative.upper(), eq(&third.lower().neg()));
    expect_true!(MpInterval::bounds(BigFloat::nan(None), one, 64).is_err());
    expect_true!(MpInterval::exact(f64::INFINITY, 64).is_err());
    expect_true!(
        CertificationPolicy {
            initial_precision: 65,
            ..CertificationPolicy::default()
        }
        .validate()
        .is_err()
    );
    Ok(())
}
