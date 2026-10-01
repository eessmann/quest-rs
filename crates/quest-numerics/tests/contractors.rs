#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::float_cmp,
    clippy::panic,
    clippy::panic_in_result_fn,
    reason = "Small analytic fixtures deliberately use direct indices, exact expectations and failing assertions"
)]
use quest_numerics::{
    ContractorLimits, ContractorOutcome, Interval, Result, extended_newton, krawczyk,
    scalar_hansen_sengupta, vector_hansen_sengupta,
};
fn p(x: f64) -> Result<Interval> {
    Interval::point(x)
}
fn linear(x: Interval) -> Result<Interval> {
    x.checked_sub(p(0.25)?)
}
fn one(_: Interval) -> Result<Interval> {
    p(1.0)
}
#[test]
fn scalar_contractors_isolate_linear_root_and_reject_empty_image() -> Result<()> {
    let x = Interval::new(-1.0, 1.0)?;
    for r in [
        extended_newton(x, 0.0, linear, one)?,
        krawczyk(x, 0.0, linear, one, 1.0)?,
        scalar_hansen_sengupta(x, 0.0, linear, one, 1.0)?,
    ] {
        assert_eq!(r.outcome(), ContractorOutcome::CertifiedUnique);
        assert_eq!(r.images().len(), 1);
        assert!(r.images()[0].contains(0.25));
        assert!(r.images()[0].upper() - r.images()[0].lower() < 1e-14);
    }
    let r = extended_newton(Interval::new(1.0, 2.0)?, 1.5, linear, one)?;
    assert_eq!(r.outcome(), ContractorOutcome::Rejected);
    assert!(r.images().is_empty());
    Ok(())
}
#[test]
fn extended_newton_splits_and_preserves_both_quadratic_roots() -> Result<()> {
    let r = extended_newton(
        Interval::new(-2.0, 2.0)?,
        0.0,
        |x| x.square()?.checked_sub(p(1.0)?),
        |x| x.checked_mul(p(2.0)?),
    )?;
    assert_eq!(r.outcome(), ContractorOutcome::Split);
    assert_eq!(r.images().len(), 2);
    assert!(r.images()[0].contains(-1.0));
    assert!(r.images()[1].contains(1.0));
    assert!(r.images()[0].upper() < 0.0 && r.images()[1].lower() > 0.0);
    let r = extended_newton(Interval::new(-1.0, 1.0)?, 0.0, |_| p(0.0), |_| p(0.0))?;
    assert_eq!(r.outcome(), ContractorOutcome::Unchanged);
    Ok(())
}
#[test]
fn degenerate_preconditioner_is_inconclusive_and_center_is_checked() -> Result<()> {
    let x = Interval::new(-1.0, 1.0)?;
    assert_eq!(
        krawczyk(x, 0.0, linear, one, 0.0)?.outcome(),
        ContractorOutcome::Unchanged
    );
    assert_eq!(
        scalar_hansen_sengupta(x, 0.0, linear, one, 0.0)?.outcome(),
        ContractorOutcome::Unchanged
    );
    assert!(extended_newton(x, 2.0, linear, one).is_err());
    assert!(krawczyk(x, 0.0, linear, one, f64::NAN).is_err());
    Ok(())
}
#[test]
fn vector_hansen_sengupta_contracts_coupled_system_and_checks_shapes_budgets() -> Result<()> {
    // A=[[2,1],[1,2]], root=(1,-1), inverse=[[2/3,-1/3],[-1/3,2/3]].
    let x = vec![Interval::new(-2.0, 2.0)?, Interval::new(-2.0, 2.0)?];
    let f = |x: &[Interval]| {
        Ok(vec![
            x[0].checked_mul(p(2.0)?)?
                .checked_add(x[1])?
                .checked_sub(p(1.0)?)?,
            x[0].checked_add(x[1].checked_mul(p(2.0)?)?)?
                .checked_add(p(1.0)?)?,
        ])
    };
    let j = |_: &[Interval]| Ok(vec![vec![p(2.0)?, p(1.0)?], vec![p(1.0)?, p(2.0)?]]);
    let r = vec![vec![2.0 / 3.0, -1.0 / 3.0], vec![-1.0 / 3.0, 2.0 / 3.0]];
    let out = vector_hansen_sengupta(&x, &[0.0, 0.0], f, j, &r, ContractorLimits::default())?;
    assert_eq!(out.outcome(), ContractorOutcome::CertifiedUnique);
    assert_eq!(out.images().len(), 1);
    assert!(out.images()[0][0].contains(1.0));
    assert!(out.images()[0][1].contains(-1.0));
    assert!(out.images()[0][0].upper() - out.images()[0][0].lower() < 1e-12);
    assert!(vector_hansen_sengupta(&x, &[0.0], f, j, &r, ContractorLimits::default()).is_err());
    assert!(
        vector_hansen_sengupta(
            &x,
            &[0.0, 0.0],
            f,
            j,
            &r,
            ContractorLimits {
                max_work: 0,
                ..ContractorLimits::default()
            }
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn vector_singular_system_never_certifies_unique_and_zero_dimension_rejected() -> Result<()> {
    let x = [Interval::new(-1.0, 1.0)?];
    let out = vector_hansen_sengupta(
        &x,
        &[0.0],
        |_| Ok(vec![p(0.0)?]),
        |_| Ok(vec![vec![p(0.0)?]]),
        &[vec![1.0]],
        ContractorLimits::default(),
    )?;
    assert_eq!(out.outcome(), ContractorOutcome::Unchanged);
    assert!(
        vector_hansen_sengupta(
            &[],
            &[],
            |_| Ok(vec![]),
            |_| Ok(vec![]),
            &[],
            ContractorLimits::default()
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn vector_extended_branches_are_retained_or_explicitly_budget_inconclusive() -> Result<()> {
    let x = [Interval::new(-2.0, 2.0)?];
    let f = |x: &[Interval]| Ok(vec![x[0].square()?.checked_sub(p(1.0)?)?]);
    let j = |x: &[Interval]| Ok(vec![vec![x[0].checked_mul(p(2.0)?)?]]);
    let r = [vec![1.0]];
    let out = vector_hansen_sengupta(&x, &[0.0], f, j, &r, ContractorLimits::default())?;
    assert_eq!(out.outcome(), ContractorOutcome::Split);
    assert_eq!(out.images().len(), 2);
    assert!(out.images().iter().any(|b| b[0].contains(-1.0)));
    assert!(out.images().iter().any(|b| b[0].contains(1.0)));
    let out = vector_hansen_sengupta(
        &x,
        &[0.0],
        f,
        j,
        &r,
        ContractorLimits {
            max_boxes: 1,
            ..ContractorLimits::default()
        },
    )?;
    assert_eq!(out.outcome(), ContractorOutcome::InconclusiveBudget);
    assert_eq!(out.images().len(), 1);
    assert!(out.images()[0][0].contains(-1.0) && out.images()[0][0].contains(1.0));
    Ok(())
}
#[test]
fn vector_storage_admission_accounts_for_live_headers_and_exact_reserved_buffers() -> Result<()> {
    let x = [Interval::new(-1.0, 1.0)?];
    let f = |x: &[Interval]| Ok(vec![x[0].checked_sub(p(0.25)?)?]);
    let j = |_: &[Interval]| Ok(vec![vec![p(1.0)?]]);
    // One dimension/branch: nine live interval cells (144 bytes), and four
    // nested row/list Vec metadata entries (96 bytes), excluding callbacks,
    // fixed local stack values and allocator bookkeeping.
    let limits = ContractorLimits {
        max_boxes: 1,
        max_bytes: 239,
        ..ContractorLimits::default()
    };
    assert!(vector_hansen_sengupta(&x, &[0.0], f, j, &[vec![1.0]], limits).is_err());
    let result = vector_hansen_sengupta(
        &x,
        &[0.0],
        f,
        j,
        &[vec![1.0]],
        ContractorLimits {
            max_bytes: 240,
            ..limits
        },
    )?;
    assert_eq!(result.outcome(), ContractorOutcome::CertifiedUnique);
    assert!(result.images()[0][0].contains(0.25));
    Ok(())
}
