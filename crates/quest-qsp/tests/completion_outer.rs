use googletest::prelude::*;
use quest_polynomial::{Laurent, Limits, Polynomial};
use quest_qsp::{Complex64, Policy, SynthesisAlgorithm, SynthesisBuilder};

#[gtest]
fn completed_payload_tracks_algorithm_and_preserves_common_completion_bits() -> Result<()> {
    let target = Polynomial::new(
        Laurent::new(2),
        vec![Complex64::new(0.2, 0.1), Complex64::new(-0.1, 0.07)],
        Limits::default(),
    )?;
    let default = SynthesisBuilder::new()
        .unit_circle_response(&target)?
        .admit()?
        .complete()?;
    expect_eq!(
        default.algorithm(),
        SynthesisAlgorithm::InverseNlftDivideConquer
    );
    expect_true!(default.weiss_ratio().is_none());
    for algorithm in [
        SynthesisAlgorithm::InverseNlftDivideConquer,
        SynthesisAlgorithm::RhwHalfCholesky,
    ] {
        let completed = SynthesisBuilder::new()
            .policy(Policy {
                algorithm,
                ..Policy::default()
            })
            .unit_circle_response(&target)?
            .admit()?
            .complete()?;
        expect_eq!(completed.algorithm(), algorithm);
        expect_eq!(completed.completion_grid(), default.completion_grid());
        expect_eq!(
            completed.completion_residual().to_bits(),
            default.completion_residual().to_bits()
        );
        for (actual, expected) in completed
            .conjugate_complement_coefficients()
            .iter()
            .zip(default.conjugate_complement_coefficients())
        {
            expect_eq!(actual.re.to_bits(), expected.re.to_bits());
            expect_eq!(actual.im.to_bits(), expected.im.to_bits());
        }
        match algorithm {
            SynthesisAlgorithm::InverseNlftDivideConquer => {
                expect_true!(completed.weiss_ratio().is_none());
            }
            SynthesisAlgorithm::RhwHalfCholesky => {
                let ratio = completed
                    .weiss_ratio()
                    .ok_or(quest_qsp::Error::Target("missing RHW ratio"))?;
                expect_eq!(ratio.grid(), completed.completion_grid());
                expect_eq!(
                    ratio.target(),
                    &[
                        Complex64::new(0.0, 0.0),
                        Complex64::new(0.0, 0.0),
                        Complex64::new(0.2, 0.1),
                        Complex64::new(-0.1, 0.07)
                    ]
                );
                expect_that!(ratio.contractivity_upper_bound(), lt(1.0));
                expect_eq!(ratio.gauge(), quest_qsp::OuterGauge::PositiveRealConstant);
            }
        }
        expect_eq!(completed.synthesize()?.algorithm(), algorithm);
    }
    Ok(())
}

#[gtest]
#[allow(
    clippy::arithmetic_side_effects,
    reason = "Small bounded fixtures keep the independent work formula visible"
)]
fn completion_charges_four_nlft_transforms_and_five_rhw_transforms() -> Result<()> {
    let target = Polynomial::new(
        Laurent::new(0),
        vec![Complex64::new(0.3, 0.2)],
        Limits::default(),
    )?;
    // Grid 32 costs 1280 units per transform; two length-one residual
    // convolutions cost 25 units each, including their pointwise product.
    for (algorithm, budget) in [
        (SynthesisAlgorithm::InverseNlftDivideConquer, 5_170),
        (SynthesisAlgorithm::RhwHalfCholesky, 6_450),
    ] {
        let complete = |max_work| {
            SynthesisBuilder::new()
                .policy(Policy {
                    algorithm,
                    limits: quest_numerics::Limits {
                        max_work,
                        ..quest_numerics::Limits::default()
                    },
                    ..Policy::default()
                })
                .unit_circle_response(&target)?
                .admit()?
                .complete()
        };
        expect_true!(complete(budget).is_ok());
        expect_true!(complete(budget - 1).is_err());
    }
    Ok(())
}

#[gtest]
#[allow(
    clippy::arithmetic_side_effects,
    reason = "Small bounded fixtures keep the independent work formula visible"
)]
fn completion_refinement_charges_all_attempts_and_residuals() -> Result<()> {
    let target = Polynomial::new(
        Laurent::new(0),
        vec![Complex64::new(0.49, 0.0); 2],
        Limits::default(),
    )?;
    for (algorithm, transforms) in [
        (SynthesisAlgorithm::InverseNlftDivideConquer, 4),
        (SynthesisAlgorithm::RhwHalfCholesky, 5),
    ] {
        let complete = |max_work| {
            SynthesisBuilder::new()
                .policy(Policy {
                    algorithm,
                    limits: quest_numerics::Limits {
                        max_work,
                        ..quest_numerics::Limits::default()
                    },
                    ..Policy::default()
                })
                .unit_circle_response(&target)?
                .admit()?
                .complete()
        };
        let completed = complete(quest_numerics::Limits::default().max_work)?;
        expect_that!(completed.completion_grid(), gt(32));
        let mut budget = 0;
        let mut grid: usize = 32;
        while grid <= completed.completion_grid() {
            // A length-two pair uses a size-four convolution plan twice.
            budget += grid * usize::try_from(grid.ilog2())? * 8 * transforms + 392;
            grid *= 2;
        }
        expect_true!(complete(budget).is_ok());
        expect_true!(complete(budget - 1).is_err());
    }
    Ok(())
}

#[gtest]
#[allow(
    clippy::manual_midpoint,
    clippy::suboptimal_flops,
    reason = "Keep the independent quadratic formula visible in this analytic oracle."
)]
fn linear_weiss_completion_selects_the_zero_outside_the_closed_disc() -> Result<()> {
    // For b(z)=.3+.4z, |a|²=.75-.12(z+z^-1). Both reciprocal
    // root choices give the same boundary norm; only the larger a0 is outer.
    let expected_a0 = ((0.75_f64 + (0.75_f64 * 0.75 - 4.0 * 0.12 * 0.12).sqrt()) / 2.0).sqrt();
    let target = Polynomial::new(
        Laurent::new(0),
        vec![Complex64::new(0.3, 0.0), Complex64::new(0.4, 0.0)],
        Limits::default(),
    )?;
    for algorithm in [
        SynthesisAlgorithm::InverseNlftDivideConquer,
        SynthesisAlgorithm::RhwHalfCholesky,
    ] {
        let completed = SynthesisBuilder::new()
            .unit_circle_response(&target)?
            .policy(Policy {
                algorithm,
                ..Policy::default()
            })
            .admit()?
            .complete()?;
        let [constant, linear] = completed.conjugate_complement_coefficients() else {
            return fail!("expected a linear complementary factor");
        };
        expect_that!(constant.re, near(expected_a0, 1e-12));
        expect_that!(linear.re, near(-0.12 / expected_a0, 1e-12));
        expect_that!(constant.im.abs(), le(1e-12));
        expect_that!(linear.im.abs(), le(1e-12));
        expect_that!(constant.norm() / linear.norm(), gt(1.0));
        // Reflection preserves both autocorrelation coefficients but moves the
        // root inside. Completion residual alone cannot establish outerness.
        expect_that!(linear.norm() / constant.norm(), lt(1.0));
        expect_that!(constant.norm_sqr() + linear.norm_sqr(), near(0.75, 1e-12));
        expect_that!(constant.re * linear.re, near(-0.12, 1e-12));
    }
    Ok(())
}
