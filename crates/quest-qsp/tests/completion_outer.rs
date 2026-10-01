use googletest::prelude::*;
use quest_polynomial::{Laurent, Limits, Polynomial};
use quest_qsp::{Complex64, Policy, SynthesisAlgorithm, SynthesisBuilder};

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
