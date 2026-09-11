use googletest::prelude::*;
use quest_qsp::{ControlSequence, PhaseSequence, WxSymmetric};
use quest_qsvt::{
    Complex64, DenseEncodingBuilder, NumericalPolicy, TransformBuilder,
    analysis::{
        Assumption, BlockEncodingBound, ErrorBudget, Reference, ReferenceParity, StandardPremises,
        Status,
    },
};
use std::ops::{Mul, Sub};
fn claim() -> quest_qsvt::analysis::Result<Assumption> {
    Assumption::stated(
        "Explicit mathematical fixture premise; numerical residuals are not its proof",
    )
}
fn bound(alpha: f64, error: f64) -> quest_qsvt::analysis::Result<BlockEncodingBound> {
    BlockEncodingBound::builder()
        .normalization(alpha)?
        .absolute_error(error)?
        .assume_contract(claim()?)
        .full_oracle_error(0.001, claim()?)?
        .build()
}
fn transform(phase: f64) -> quest_qsvt::Result<quest_qsvt::ValidatedTransform> {
    let matrix = faer::Mat::from_fn(1, 1, |_, _| Complex64::new(0.3, 0.4));
    let encoding = DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
        .normalization(1.)?
        .build()?;
    TransformBuilder::new()
        .encoding(encoding)
        .standard(PhaseSequence::<WxSymmetric>::builder(vec![phase, phase]).build()?)
        .build()
}
fn premises(
    transform: &quest_qsvt::ValidatedTransform,
) -> quest_qsvt::analysis::Result<StandardPremises<'_>> {
    Ok(StandardPremises::for_transform(transform)?
        .assume_projected_unitary_subspaces(claim()?)
        .assume_actual_phase_response(claim()?)
        .assume_parity_and_completion(claim()?)
        .build())
}
#[gtest]
fn standard_bounds_are_conditional_outward_and_bound_to_actual_transform() -> googletest::Result<()>
{
    let transform = transform(0.2)?;
    let report = transform
        .analysis()
        .encoding_bound(bound(1., 0.01)?)?
        .standard_premises(premises(&transform)?)?
        .budget(ErrorBudget::stated(0.01, 0.02, 0.03, claim()?)?)
        .build()?;
    expect_eq!(report.status(), Status::ConditionalOnExplicitPremises);
    expect_true!(report.robustness().unwrap().contains(0.4));
    // Standard extracted response currently retains two physical source calls.
    expect_true!(report.oracle_telescoping().unwrap().contains(0.002));
    expect_true!(report.total().unwrap().contains(0.462));
    expect_eq!(report.theorem_assumptions().len(), 3);
    expect_eq!(report.encoding().assumptions().len(), 2);
    expect_true!(transform.theorem_error_bound().is_none());
    expect_true!(report.observations().unitarity_residual().is_some());
    let changed_phase = self::transform(0.3)?;
    expect_true!(
        changed_phase
            .analysis()
            .encoding_bound(bound(1., 0.01)?)?
            .standard_premises(premises(&transform)?)
            .is_err()
    );
    let other = transform.clone();
    expect_true!(
        other
            .analysis()
            .encoding_bound(bound(1., 0.01)?)?
            .standard_premises(premises(&transform)?)
            .is_err()
    );
    let missing = transform
        .analysis()
        .encoding_bound(bound(1., 0.01)?)?
        .uncertified()?;
    expect_eq!(missing.status(), Status::Uncertified);
    expect_true!(missing.robustness().is_none());
    expect_true!(missing.total().is_none());
    Ok(())
}
#[gtest]
fn normalization_composition_preserves_enclosures_and_separate_oracle_errors()
-> googletest::Result<()> {
    let product = bound(2., 0.2)?.product(bound(3., 0.6)?)?;
    expect_true!(product.normalization().contains(6.));
    expect_true!(product.normalized_error().contains(0.3));
    expect_true!(product.absolute_error()?.contains(1.8));
    expect_true!(product.full_oracle_error().unwrap().contains(0.002));
    let lifted = product.clone().hermitianize();
    expect_eq!(
        lifted.normalization().lower(),
        product.normalization().lower()
    );
    expect_eq!(
        lifted.normalized_error().upper(),
        product.normalized_error().upper()
    );
    expect_eq!(lifted.assumptions().len(), product.assumptions().len());
    let without_oracle = BlockEncodingBound::builder()
        .normalization(2.)?
        .absolute_error(0.)?
        .assume_contract(claim()?)
        .build()?;
    expect_true!(
        product
            .product(without_oracle)?
            .full_oracle_error()
            .is_none()
    );
    let alpha = 1.1_f64.mul(1.1);
    let rounded = bound(1.1, 0.)?.product(bound(1.1, 0.)?)?;
    expect_true!(rounded.normalization().lower() < rounded.normalization().upper());
    let matrix = faer::Mat::from_fn(1, 1, |_, _| Complex64::new(0.3, 0.));
    let encoding = DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
        .normalization(alpha)?
        .build()?;
    let t = TransformBuilder::new()
        .encoding(encoding)
        .standard(PhaseSequence::<WxSymmetric>::builder(vec![0.2]).build()?)
        .build()?;
    expect_true!(t.analysis().encoding_bound(rounded).is_err());
    expect_true!(bound(0., 0.).is_err());
    expect_true!(bound(1., -0.01).is_err());
    expect_true!(bound(f64::MAX, 0.)?.product(bound(2., 0.)?).is_err());
    Ok(())
}
#[gtest]
fn generalized_reports_use_physical_queries_without_standard_theorem_privileges()
-> googletest::Result<()> {
    let source = transform(0.2)?;
    let t = TransformBuilder::new()
        .encoding(source.encoding().clone())
        .hermitianized_full(
            ControlSequence::builder()
                .angles(&[0.2, 0.3], &[0.4, 0.5])?
                .build()?,
        )
        .build()?;
    expect_true!(StandardPremises::for_transform(&t).is_err());
    let report = t
        .analysis()
        .encoding_bound(bound(1., 0.01)?)?
        .uncertified()?;
    expect_eq!(report.status(), Status::Uncertified);
    let count = t
        .query_counts()
        .source_forward
        .checked_add(t.query_counts().source_adjoint)
        .unwrap();
    let expected = f64::from(u32::try_from(count)?).mul(0.001);
    expect_true!(report.oracle_telescoping().unwrap().contains(expected));
    expect_true!(report.total().is_none());
    Ok(())
}
#[gtest]
fn svd_reference_keeps_rectangular_odd_map_and_even_right_nullspace() -> googletest::Result<()> {
    let source = faer::Mat::from_fn(2, 3, |r, c| {
        if r == c {
            Complex64::new(if r == 0 { 0.3 } else { 0.7 }, 0.)
        } else {
            Complex64::new(0., 0.)
        }
    });
    let odd = Reference::svd_polynomial(
        source.as_ref(),
        ReferenceParity::Odd,
        |x| Complex64::new(0., x),
        NumericalPolicy::default(),
    )?;
    expect_eq!((odd.matrix().nrows(), odd.matrix().ncols()), (2, 3));
    for r in 0..2 {
        for c in 0..3 {
            expect_that!(
                odd.matrix()[(r, c)]
                    .sub(Complex64::new(0., 1.).mul(source[(r, c)]))
                    .norm(),
                lt(1e-13)
            );
        }
    }
    let even = Reference::svd_polynomial(
        source.as_ref(),
        ReferenceParity::Even,
        |x| Complex64::new(x.mul_add(x, 0.2), 0.),
        NumericalPolicy::default(),
    )?;
    expect_eq!((even.matrix().nrows(), even.matrix().ncols()), (3, 3));
    expect_that!(even.matrix()[(2, 2)].re, near(0.2, 1e-13));
    expect_that!(even.matrix()[(0, 0)].re, near(0.29, 1e-13));
    expect_true!(
        Reference::svd_polynomial(
            source.as_ref(),
            ReferenceParity::Odd,
            |_| Complex64::new(1., 0.),
            NumericalPolicy::default()
        )
        .is_err()
    );
    Ok(())
}
#[gtest]
fn diagnostics_preserve_global_phase_and_repeat_seeded_observations() -> googletest::Result<()> {
    let t = transform(0.2)?;
    let actual = t.materialize_block()?;
    let wrong = faer::Mat::from_fn(actual.nrows(), actual.ncols(), |r, c| {
        Complex64::new(0., 1.).mul(actual[(r, c)])
    });
    let observed = t
        .diagnostics()
        .reference(Reference::dense(
            wrong.as_ref(),
            NumericalPolicy::default(),
        )?)
        .run()?;
    expect_false!(observed.is_certified_upper_bound());
    expect_that!(
        observed.observed_error(),
        near(actual[(0, 0)].norm().mul(std::f64::consts::SQRT_2), 1e-13)
    );
    let first = t
        .diagnostics()
        .reference(Reference::dense(
            wrong.as_ref(),
            NumericalPolicy::default(),
        )?)
        .sampled(8, 3, 42)?
        .run()?;
    let second = t
        .diagnostics()
        .reference(Reference::dense(
            wrong.as_ref(),
            NumericalPolicy::default(),
        )?)
        .sampled(8, 3, 42)?
        .run()?;
    expect_eq!(
        first.observed_error().to_bits(),
        second.observed_error().to_bits()
    );
    expect_that!(
        first.observed_error(),
        near(observed.observed_error(), 1e-13)
    );
    expect_eq!(first.seed(), Some(42));
    expect_false!(first.is_certified_upper_bound());
    let budget = t
        .diagnostics()
        .reference(Reference::dense(
            wrong.as_ref(),
            NumericalPolicy::default(),
        )?)
        .policy(NumericalPolicy { max_bytes: 1 })
        .run();
    expect_true!(budget.is_err());
    expect_true!(t.diagnostics().sampled(0, 3, 42).is_err());
    Ok(())
}

#[gtest]
fn seeded_matrix_diagnostic_observes_spectral_error_without_claiming_an_upper_bound()
-> googletest::Result<()> {
    let policy = NumericalPolicy::default();
    let source = faer::Mat::from_fn(2, 2, |r, c| {
        if r == c {
            Complex64::new(0.2, 0.1)
        } else {
            Complex64::new(0., 0.)
        }
    });
    let encoding = DenseEncodingBuilder::new(source.as_ref(), policy)?
        .normalization(1.)?
        .build()?;
    let t = TransformBuilder::new()
        .encoding(encoding)
        .standard(PhaseSequence::<WxSymmetric>::builder(vec![0.2, 0.2]).build()?)
        .build()?;
    let actual = t.materialize_block()?;
    let reference = faer::Mat::from_fn(2, 2, |r, c| {
        actual[(r, c)].sub(match (r, c) {
            (0, 0) => Complex64::new(0.5, 0.),
            (0, 1) => Complex64::new(0., 0.25),
            (1, 0) => Complex64::new(0.1, 0.15),
            _ => Complex64::new(0.2, 0.),
        })
    });
    let dense = t
        .diagnostics()
        .reference(Reference::dense(reference.as_ref(), policy)?)
        .run()?;
    let build = || {
        t.diagnostics()
            .reference(Reference::dense(reference.as_ref(), policy)?)
            .sampled(8, 12, 42)?
            .run()
    };
    let first = build()?;
    let second = build()?;
    expect_eq!(
        first.observed_error().to_bits(),
        second.observed_error().to_bits()
    );
    expect_that!(first.observed_error(), near(dense.observed_error(), 1e-10));
    expect_false!(first.is_certified_upper_bound());
    Ok(())
}

#[gtest]
fn diagnostic_budget_covers_oracle_decomposition_metadata_before_materializing()
-> googletest::Result<()> {
    use quest_qsvt::{EncodingBuilder, Left, LogicalSpace, Right};
    let policy = NumericalPolicy::default();
    let mut body = quest_circuit::ProgramBuilder::new(1, 0)?;
    for _ in 0..200 {
        body.global_phase(quest_circuit::Angle::radians(0.)?, &[])?;
    }
    let oracle = quest_circuit::OracleFragment::builder(body.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let source = EncodingBuilder::new()
        .oracle(oracle)
        .left(LogicalSpace::<Left>::coordinates(2, &[0], policy)?)
        .right(LogicalSpace::<Right>::coordinates(2, &[0], policy)?)
        .normalization(1.)?
        .build()?;
    let t = TransformBuilder::new()
        .encoding(source)
        .standard(PhaseSequence::<WxSymmetric>::builder(vec![0.2, 0.2]).build()?)
        .build()?;
    let reference = Reference::dense(
        faer::Mat::from_fn(1, 1, |_, _| Complex64::new(0., 0.)).as_ref(),
        policy,
    )?;
    let result = t
        .diagnostics()
        .reference(reference)
        .policy(NumericalPolicy { max_bytes: 8192 })
        .run();
    expect_true!(matches!(
        result,
        Err(quest_qsvt::analysis::Error::Budget(_))
    ));
    Ok(())
}
