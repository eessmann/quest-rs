use googletest::prelude::*;
use quest_polynomial::{
    Chebyshev, Complex64, Hermite, Interval, Jacobi, Laguerre, Limits, Monomial, Polynomial,
    RemezOptions, function, remez,
};
const fn c(x: f64) -> Complex64 {
    Complex64::new(x, 0.0)
}
#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Expression operators build syntax; numerical evaluation is checked"
)]
fn one_expression_evaluates_values_and_encloses_derivatives() -> Result<()> {
    let f = function!(|x| (x.clone() * x + 1.0).ln());
    expect_that!(f.evaluate(0.5)?, near(1.25_f64.ln(), 1e-14));
    let j = f.jet_interval(Interval::point(0.5)?)?;
    expect_true!(j.value.contains(1.25_f64.ln()));
    expect_true!(j.first.contains(0.8));
    expect_true!(j.second.contains(0.96));
    expect_true!(
        function!(|x| x.ln())
            .evaluate_interval(Interval::new(-1.0, 1.0)?)
            .is_err()
    );
    Ok(())
}
#[gtest]
fn derivatives_keep_basis_families_and_intervals_enclose_original_parameters() -> Result<()> {
    let l = Limits::default();
    let a = vec![c(0.0), c(0.0), c(1.0)];
    let p = Polynomial::new(Chebyshev, a.clone(), l)?;
    expect_that!(p.derivative()?.evaluate_real(0.3)?, near(1.2, 1e-14));
    let p = Polynomial::new(Hermite::physicists(), a.clone(), l)?;
    expect_that!(p.derivative()?.evaluate_real(0.3)?, near(2.4, 1e-14));
    let p = Polynomial::new(Laguerre::new(0.3)?, a.clone(), l)?;
    expect_that!(p.derivative()?.evaluate_real(0.3)?, near(-2.0, 1e-14));
    expect_true!(
        p.evaluate_interval(Interval::new(0.2, 0.4)?)?
            .contains(p.evaluate_real(0.3)?)
    );
    let p = Polynomial::new(Jacobi::new(0.3, 0.4)?, a, l)?;
    expect_true!(
        p.evaluate_interval(Interval::point(0.3)?)?
            .contains(p.evaluate_real(0.3)?)
    );
    let p = Polynomial::new(Jacobi::new(f64::MAX, f64::MAX)?, vec![c(2.0)], l)?;
    expect_that!(p.evaluate_real(0.3)?, eq(2.0));
    Ok(())
}
#[gtest]
fn conversions_return_enclosed_error_and_parity_is_admitted() -> Result<()> {
    let p = Polynomial::new(Chebyshev, vec![c(1.0), c(0.0), c(2.0)], Limits::default())?;
    let converted = p.to_monomial()?;
    expect_that!(converted.polynomial.evaluate_real(0.3)?, near(-0.64, 1e-14));
    expect_true!(converted.coefficient_error_bound >= 0.0);
    expect_true!(p.clone().admit_parity::<quest_polynomial::Even>().is_ok());
    expect_true!(p.admit_parity::<quest_polynomial::Odd>().is_err());
    let p = Polynomial::new(Monomial, vec![c(0.0), c(0.0), c(1.0)], Limits::default())?;
    expect_that!(p.derivative()?.evaluate_real(0.5)?, eq(1.0));
    Ok(())
}
#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Expression operators build syntax; numerical evaluation is checked"
)]
fn remez_linear_square_matches_known_minimax_and_certifies_domain() -> Result<()> {
    let f = function!(|x| x.clone() * x);
    let result = remez(
        &f,
        Interval::new(-1.0, 1.0)?,
        RemezOptions {
            degree: 1,
            ..RemezOptions::default()
        },
    )?;
    expect_that!(result.polynomial().evaluate_real(0.3)?, near(0.5, 1e-8));
    expect_true!(result.error_bound().upper() >= 0.5);
    expect_that!(result.error_bound().upper(), lt(0.50001));
    Ok(())
}
#[gtest]
fn basis_derivatives_preserve_parameters_and_match_exact_legendre_identity() -> Result<()> {
    let limits = Limits::default();
    let laguerre = Polynomial::new(Laguerre::new(0.3)?, vec![c(0.0), c(0.0), c(1.0)], limits)?;
    expect_that!(laguerre.derivative()?.basis().alpha(), eq(0.3));
    let p = Polynomial::new(
        Jacobi::new(0.0, 0.0)?,
        vec![c(0.0), c(0.0), c(0.0), c(1.0)],
        limits,
    )?;
    expect_that!(p.derivative()?.basis().parameters(), eq((0.0, 0.0)));
    expect_that!(p.derivative()?.evaluate_real(0.3)?, near(-0.825, 1e-14));
    Ok(())
}
#[gtest]
fn remez_exp_converges_with_domain_enclosure_and_error_lower_bound() -> Result<()> {
    let f = function!(|x| x.exp());
    let result = quest_polynomial::RemezBuilder::new()
        .target(f.clone())
        .domain(Interval::new(-1.0, 1.0)?)?
        .degree(3)
        .tolerance(1e-8)
        .run()?;
    expect_that!(result.iterations(), gt(1));
    expect_that!(result.error_bound().upper(), near(0.005_528_37, 1e-6));
    expect_that!(
        result.error_bound().upper() - result.minimax_lower_bound(),
        le(1e-8)
    );
    for x in [-1.0, -0.9, -0.6, 0.0, 0.4, 0.9, 1.0] {
        expect_that!(
            (f.evaluate(x)? - result.polynomial().evaluate_real(x)?).abs(),
            le(result.error_bound().upper())
        );
    }
    Ok(())
}
#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Expression operators build a checked mathematical AST"
)]
fn root_isolation_keeps_tangent_roots_unresolved_and_rejects_budget_exhaustion() -> Result<()> {
    let f = function!(|x| x.sin());
    let roots =
        quest_polynomial::isolate_critical_points(&f, Interval::new(0.0, 3.0)?, 1e-9, 4096)?;
    expect_true!(
        roots
            .boxes
            .iter()
            .any(|r| r.interval.contains(std::f64::consts::FRAC_PI_2) && r.exists && r.unique)
    );
    let tangent = function!(|x| x.clone() * x.clone() * x);
    let tangent_roots =
        quest_polynomial::isolate_critical_points(&tangent, Interval::new(-1.0, 1.0)?, 1e-9, 4096)?;
    expect_true!(
        tangent_roots
            .boxes
            .iter()
            .any(|r| r.interval.contains(0.0) && r.exists && !r.unique)
    );
    expect_true!(
        quest_polynomial::isolate_critical_points(&f, Interval::new(0.0, 3.0)?, 1e-9, 1).is_err()
    );
    expect_true!(
        quest_polynomial::RemezBuilder::new()
            .target(function!(|x| x.ln()))
            .domain(Interval::new(-1.0, 1.0)?)
            .is_err()
    );
    Ok(())
}
#[gtest]
fn explicit_conversion_preserves_complex_response_across_parameterized_bases() -> Result<()> {
    let limits = Limits::default();
    let p = Polynomial::new(
        Hermite::physicists(),
        vec![Complex64::new(0.2, 0.1), Complex64::new(-0.3, 0.2), c(0.4)],
        limits,
    )?;
    let cheb = p.to_basis(Chebyshev)?;
    let lag = p.to_basis(Laguerre::new(0.3)?)?;
    let jac = p.to_basis(Jacobi::new(0.3, 0.7)?)?;
    let x = Complex64::new(0.3, 0.2);
    let expected = p.evaluate(x)?;
    for actual in [
        cheb.polynomial.evaluate(x)?,
        lag.polynomial.evaluate(x)?,
        jac.polynomial.evaluate(x)?,
    ] {
        expect_that!(std::ops::Sub::sub(actual, expected).norm(), lt(1e-13));
    }
    expect_true!(cheb.coefficient_error_bound >= 0.0);
    expect_true!(lag.coefficient_error_bound >= 0.0);
    expect_true!(jac.coefficient_error_bound >= 0.0);
    Ok(())
}
#[gtest]
fn callback_functions_require_an_explicit_consistency_assumption() -> Result<()> {
    use quest_polynomial::{CallbackFunction, ConsistencyAssumption, Jet};
    let f = CallbackFunction::with_assumed_consistency(
        ConsistencyAssumption::SameFunctionAndDerivatives,
        |x: f64| Ok(x),
        |x: Interval| {
            Ok(Jet {
                value: x,
                first: Interval::point(1.0)?,
                second: Interval::point(0.0)?,
            })
        },
    );
    expect_that!(f.evaluate(0.3)?, eq(0.3));
    expect_true!(
        f.jet_interval(Interval::new(0.2, 0.4)?)?
            .value
            .contains(0.3)
    );
    Ok(())
}
#[gtest]
fn exact_cosine_conversion_rejects_underflow_that_changes_the_polynomial() -> Result<()> {
    let p = Polynomial::new(
        Chebyshev,
        vec![c(0.0), c(f64::from_bits(1))],
        Limits::default(),
    )?;
    expect_true!(p.on_cosine_circle().is_err());
    Ok(())
}
#[gtest]
fn expression_views_preserve_the_original_single_expression() -> Result<()> {
    use quest_polynomial::ExprNode;
    let function = function!(|x| x.exp());
    let ExprNode::Exp(argument) = function.expression().node() else {
        return fail!("expected original exponential node");
    };
    expect_true!(matches!(argument.node(), ExprNode::Variable));
    Ok(())
}
