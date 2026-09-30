use super::{Affine, Context, ExactError, Limits, Owner, Rational, Symbol};
use googletest::{Result, prelude::*};
use num_bigint::BigInt;
use std::ops::Add;

fn r(n: i64, d: i64) -> Rational {
    Rational::new(BigInt::from(n), BigInt::from(d))
}

#[gtest]
fn thirds_are_canonical_and_large_integers_stay_exact() -> Result<()> {
    let ctx = Context::new(Owner::new(7));
    let third = ctx.ratio(1.into(), 3.into())?;
    let two_thirds = third.add(&third)?;
    expect_eq!(two_thirds.constant(), &r(2, 3));
    expect_eq!(third.add(&two_thirds)?, ctx.one()?);
    let huge = ctx.ratio(BigInt::from(1u64 << 54).add(1), 1.into())?;
    expect_eq!(huge.constant().numer(), &(BigInt::from(1u64 << 54).add(1)));
    expect_true!(!ctx.ratio(1.into(), BigInt::from(1u64 << 60))?.is_zero());
    Ok(())
}

#[gtest]
fn denominator_sign_is_normalized_and_zero_is_rejected() -> Result<()> {
    let ctx = Context::new(Owner::new(1));
    expect_eq!(ctx.ratio(2.into(), (-4).into())?.constant(), &r(-1, 2));
    expect_eq!(
        ctx.ratio(1.into(), 0.into()),
        Err(ExactError::ZeroDenominator)
    );
    expect_eq!(
        ctx.one()?.divide_ratio(0.into(), 1.into()),
        Err(ExactError::ZeroDenominator)
    );
    Ok(())
}

#[gtest]
fn pi_is_symbolic_and_terms_have_canonical_order() -> Result<()> {
    let ctx = Context::new(Owner::new(4));
    let x = ctx.symbol(Symbol::new(ctx.owner(), 9))?;
    let y = ctx.symbol(Symbol::new(ctx.owner(), 2))?;
    let expr = ctx.pi()?.add(&x)?.add(&y)?.sub(&x)?;
    expect_eq!(expr.pi_coefficient(), &r(1, 1));
    expect_eq!(
        expr.terms().map(|(s, _)| s.index()).collect::<Vec<_>>(),
        vec![2]
    );
    expect_eq!(
        expr.export()?.as_str(),
        "owner=4;r=0/1;pi=1/1;terms=[2=1/1]"
    );
    expect_ne!(ctx.pi()?, ctx.zero()?);
    Ok(())
}

#[gtest]
fn substitution_is_simultaneous_and_rehomes_only_explicit_symbols() -> Result<()> {
    let source = Context::new(Owner::new(11));
    let target = Context::new(Owner::new(12));
    let x = Symbol::new(source.owner(), 0);
    let y = Symbol::new(source.owner(), 1);
    let a = Symbol::new(target.owner(), 5);
    let body = source.symbol(x)?.add(&source.symbol(y)?)?;
    let replacement_x = target.symbol(a)?.add(&target.one()?)?;
    let replacement_y = target.symbol(a)?.neg()?;
    let mapped = body.substitute_into(&target, &[(x, replacement_x), (y, replacement_y)])?;
    expect_eq!(mapped, target.one()?);
    expect_eq!(
        body.substitute_into(&target, &[(x, target.one()?)]),
        Err(ExactError::UnmappedSymbol)
    );
    expect_eq!(
        body.substitute_into(&target, &[(Symbol::new(target.owner(), 0), target.one()?)]),
        Err(ExactError::ForeignSymbol)
    );
    Ok(())
}

#[gtest]
fn from_parts_canonicalizes_duplicate_terms_and_rejects_foreign_symbols() -> Result<()> {
    let ctx = Context::new(Owner::new(2));
    let x = Symbol::new(ctx.owner(), 3);
    let value = ctx.assemble(r(1, 3), r(-1, 2), vec![(x, r(2, 3)), (x, r(1, 3))])?;
    expect_eq!(
        value.terms().map(|(_, c)| c.clone()).collect::<Vec<_>>(),
        vec![r(1, 1)]
    );
    expect_eq!(
        ctx.assemble(
            r(0, 1),
            r(0, 1),
            vec![(Symbol::new(Owner::new(3), 0), r(1, 1))]
        ),
        Err(ExactError::ForeignSymbol)
    );
    Ok(())
}

#[gtest]
fn term_and_coefficient_budgets_are_enforced_before_publication() -> Result<()> {
    let limits = Limits {
        max_terms: 1,
        max_coefficient_bits: 4,
        max_bytes: 1024,
        max_work: 1000,
    };
    let ctx = Context::with_limits(Owner::new(3), limits)?;
    let x = ctx.symbol(Symbol::new(ctx.owner(), 1))?;
    let y = ctx.symbol(Symbol::new(ctx.owner(), 2))?;
    expect_eq!(x.add(&y), Err(ExactError::TermLimit));
    expect_eq!(
        ctx.ratio(16.into(), 1.into()),
        Err(ExactError::CoefficientLimit)
    );
    expect_eq!(
        ctx.ratio(8.into(), 1.into())?
            .scale_ratio(2.into(), 1.into()),
        Err(ExactError::CoefficientLimit)
    );
    Ok(())
}

#[gtest]
fn context_usage_tracks_retained_storage_and_work_across_clones() -> Result<()> {
    let ctx = Context::new(Owner::new(6));
    let peer = ctx.clone();
    let before = peer.usage();
    let value = ctx.symbol(Symbol::new(ctx.owner(), 0))?;
    let during = peer.usage();
    expect_true!(during.retained_bytes > before.retained_bytes);
    expect_true!(during.work > before.work);
    expect_eq!(during.staged_bytes, 0);
    drop(value);
    expect_eq!(peer.usage().retained_bytes, before.retained_bytes);
    Ok(())
}

#[gtest]
fn exact_equality_and_zero_survive_cancellation() -> Result<()> {
    let ctx = Context::new(Owner::new(8));
    let x = ctx.symbol(Symbol::new(ctx.owner(), 0))?;
    let expr = x.add(&ctx.pi()?)?;
    expect_true!(expr.sub(&expr)?.is_zero());
    expect_true!(ctx.one()?.is_one());
    expect_ne!(x, ctx.zero()?);
    Ok(())
}

#[gtest]
fn affine_type_is_cloneable_and_immutable() -> Result<()> {
    let ctx = Context::new(Owner::new(10));
    let original: Affine = ctx.pi()?;
    let shared = original.clone();
    let changed = shared.add(&ctx.one()?)?;
    expect_eq!(original.pi_coefficient(), &r(1, 1));
    expect_eq!(original.constant(), &r(0, 1));
    expect_eq!(changed.constant(), &r(1, 1));
    Ok(())
}

#[gtest]
fn coefficient_limit_allows_negation_and_self_cancellation_at_boundary() -> Result<()> {
    let ctx = Context::with_limits(
        Owner::new(20),
        Limits {
            max_terms: 2,
            max_coefficient_bits: 8,
            max_bytes: 4096,
            max_work: 1000,
        },
    )?;
    let largest = ctx.ratio(255.into(), 1.into())?;
    expect_eq!(largest.neg()?.constant(), &r(-255, 1));
    expect_true!(largest.sub(&largest)?.is_zero());
    Ok(())
}

#[gtest]
fn storage_and_cumulative_work_limits_reject_without_leaking_stage() -> Result<()> {
    let bytes = Context::with_limits(
        Owner::new(30),
        Limits {
            max_terms: 1,
            max_coefficient_bits: 64,
            max_bytes: 128,
            max_work: 1000,
        },
    )?;
    expect_eq!(
        bytes.symbol(Symbol::new(bytes.owner(), 0)),
        Err(ExactError::StorageLimit)
    );
    expect_eq!(bytes.usage().staged_bytes, 0);
    let work = Context::with_limits(
        Owner::new(31),
        Limits {
            max_terms: 1,
            max_coefficient_bits: 64,
            max_bytes: 4096,
            max_work: 3,
        },
    )?;
    let first = work.zero()?;
    expect_eq!(work.zero(), Err(ExactError::WorkLimit));
    drop(first);
    expect_eq!(work.usage().staged_bytes, 0);
    Ok(())
}

#[gtest]
fn export_is_bounded_and_retained_until_drop() -> Result<()> {
    let ctx = Context::with_limits(
        Owner::new(40),
        Limits {
            max_terms: 1,
            max_coefficient_bits: 64,
            max_bytes: 4096,
            max_work: 1000,
        },
    )?;
    let value = ctx.pi()?;
    let before = ctx.usage().retained_bytes;
    let exported = value.export()?;
    expect_eq!(exported.as_str(), "owner=40;r=0/1;pi=1/1;terms=[]");
    expect_true!(ctx.usage().retained_bytes > before);
    drop(exported);
    expect_eq!(ctx.usage().retained_bytes, before);
    let tiny_work = Context::with_limits(
        Owner::new(41),
        Limits {
            max_terms: 1,
            max_coefficient_bits: 64,
            max_bytes: 4096,
            max_work: 3,
        },
    )?;
    expect_eq!(tiny_work.zero()?.export(), Err(ExactError::WorkLimit));
    Ok(())
}

#[gtest]
fn imported_raw_rationals_are_normalized_or_rejected() -> Result<()> {
    let ctx = Context::new(Owner::new(50));
    let raw = Rational::new_raw(2.into(), 4.into());
    let value = ctx.assemble(raw, r(0, 1), Vec::new())?;
    expect_eq!(value.constant().numer(), &BigInt::from(1));
    expect_eq!(value.constant().denom(), &BigInt::from(2));
    let invalid = Rational::new_raw(1.into(), 0.into());
    expect_eq!(
        ctx.assemble(invalid, r(0, 1), Vec::new()),
        Err(ExactError::ZeroDenominator)
    );
    Ok(())
}

#[gtest]
fn substitution_charges_mapping_sort_work_before_expansion() -> Result<()> {
    let source = Context::new(Owner::new(60));
    let target = Context::with_limits(
        Owner::new(61),
        Limits {
            max_terms: 1,
            max_coefficient_bits: 64,
            max_bytes: 4096,
            max_work: 30,
        },
    )?;
    let replacement = target.zero()?;
    let mappings = (0..10)
        .map(|i| (Symbol::new(source.owner(), i), replacement.clone()))
        .collect::<Vec<_>>();
    expect_eq!(
        source.zero()?.substitute_into(&target, &mappings),
        Err(ExactError::WorkLimit)
    );
    Ok(())
}

#[gtest]
fn foreign_constants_obey_receiving_coefficient_limits() -> Result<()> {
    let wide = Context::new(Owner::new(70));
    let narrow = Context::with_limits(
        Owner::new(71),
        Limits {
            max_coefficient_bits: 4,
            ..Limits::default()
        },
    )?;
    let zero = narrow.zero()?;
    for value in [
        wide.ratio(16.into(), 1.into())?,
        wide.pi()?.scale_ratio(16.into(), 1.into())?,
    ] {
        expect_eq!(zero.add(&value), Err(ExactError::CoefficientLimit));
        expect_eq!(zero.sub(&value), Err(ExactError::CoefficientLimit));
    }
    Ok(())
}

#[gtest]
fn import_charges_capacity_and_sort_before_canonicalization() -> Result<()> {
    let owner = Owner::new(72);
    let small = Context::with_limits(
        owner,
        Limits {
            max_bytes: 4096,
            ..Limits::default()
        },
    )?;
    let mut overallocated = Vec::with_capacity(4096);
    overallocated.push((Symbol::new(owner, 0), r(1, 1)));
    expect_eq!(
        small.assemble(r(0, 1), r(0, 1), overallocated),
        Err(ExactError::StorageLimit)
    );
    let little_work = Context::with_limits(
        owner,
        Limits {
            max_work: 40,
            ..Limits::default()
        },
    )?;
    let reverse = (0..16)
        .rev()
        .map(|i| (Symbol::new(owner, i), r(1, 1)))
        .collect();
    expect_eq!(
        little_work.assemble(r(0, 1), r(0, 1), reverse),
        Err(ExactError::WorkLimit)
    );
    expect_eq!(small.usage().staged_bytes, 0);
    expect_eq!(little_work.usage().staged_bytes, 0);
    Ok(())
}
