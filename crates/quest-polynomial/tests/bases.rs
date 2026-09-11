use googletest::prelude::*;
use num_complex::Complex64;
use quest_polynomial::{
    Chebyshev, Hermite, Jacobi, Laguerre, Laurent, Limits, Monomial, Polynomial,
};

const fn c(value: f64) -> Complex64 {
    Complex64::new(value, 0.0)
}

#[gtest]
fn named_bases_match_independent_low_degree_formulas() -> Result<()> {
    let limits = Limits::default();
    let x = c(0.3);
    let coefficients = vec![c(0.0), c(0.0), c(1.0)];
    let cheb = Polynomial::new(Chebyshev, coefficients.clone(), limits)?;
    let hermite = Polynomial::new(Hermite::physicists(), coefficients.clone(), limits)?;
    let probabilist = Polynomial::new(Hermite::probabilists(), coefficients.clone(), limits)?;
    let laguerre = Polynomial::new(Laguerre::new(0.0)?, coefficients.clone(), limits)?;
    let legendre = Polynomial::new(Jacobi::new(0.0, 0.0)?, coefficients, limits)?;
    expect_that!(cheb.evaluate(x)?.re, near(-0.82, 1e-14));
    expect_that!(hermite.evaluate(x)?.re, near(-1.64, 1e-14));
    expect_that!(probabilist.evaluate(x)?.re, near(-0.91, 1e-14));
    expect_that!(laguerre.evaluate(x)?.re, near(0.445, 1e-14));
    expect_that!(legendre.evaluate(x)?.re, near(-0.365, 1e-14));
    Ok(())
}

#[gtest]
fn complex_laurent_support_is_preserved_and_zero_poles_are_errors() -> Result<()> {
    let p = Polynomial::new(
        Laurent::new(-1),
        vec![c(1.0), Complex64::new(0.0, 2.0), c(3.0)],
        Limits::default(),
    )?;
    let value = p.evaluate(Complex64::new(0.0, 1.0))?;
    expect_that!(value, eq(Complex64::new(0.0, 4.0)));
    expect_that!(p.support(), eq((-1, 1)));
    expect_true!(p.evaluate(c(0.0)).is_err());
    Ok(())
}

#[gtest]
fn cosine_laurent_conversion_preserves_complex_response_and_signed_support() -> Result<()> {
    let p = Polynomial::new(
        Chebyshev,
        vec![Complex64::new(0.2, -0.1), c(0.0), Complex64::new(-0.3, 0.2)],
        Limits::default(),
    )?;
    let laurent = p.on_cosine_circle()?;
    let angle: f64 = 0.73;
    let z = Complex64::new(angle.cos(), angle.sin());
    let actual = laurent.evaluate(z)?;
    let expected = p.evaluate(c(angle.cos()))?;
    expect_that!(std::ops::Sub::sub(actual, expected).norm(), lt(1e-14));
    expect_that!(laurent.support(), eq((-2, 2)));
    Ok(())
}

#[gtest]
fn admission_rejects_bad_parameters_nonfinite_payloads_and_storage_limits() {
    expect_true!(Laguerre::new(-1.0).is_err());
    expect_true!(Jacobi::new(0.0, f64::NAN).is_err());
    expect_true!(Polynomial::new(Monomial, vec![c(f64::INFINITY)], Limits::default()).is_err());
    expect_true!(
        Polynomial::new(
            Monomial,
            vec![c(1.0), c(2.0)],
            Limits {
                max_coefficients: 1,
                ..Limits::default()
            }
        )
        .is_err()
    );
    expect_true!(
        Polynomial::new(
            Laurent::new(i32::MAX),
            vec![c(1.0), c(2.0)],
            Limits::default()
        )
        .is_err()
    );
}
