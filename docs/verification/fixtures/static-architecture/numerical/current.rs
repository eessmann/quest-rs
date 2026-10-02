include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../allocator.rs"));
use quest_numerics::arithmetic::{Backend, ExactConstant, F64Backend, First, Interval64Backend};
use quest_polynomial::{Accuracy, ExactDomain, GenericFunction, Interval, RemezRequest};
use std::hint::black_box;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    header();
    let function = black_box(quest_polynomial::function!(|x| (x * x + 1.0).ln()));
    measure("canonical_construct", 10_000, || {
        black_box(quest_polynomial::function!(|x| (x * x + 1.0).ln()));
        Ok(1)
    })?;
    measure("canonical_value", 1_000_000, || {
        black_box(function.evaluate(&mut F64Backend, black_box(0.3))?);
        Ok(1)
    })?;
    measure("canonical_jet", 1_000_000, || {
        black_box(function.jet(&mut F64Backend, black_box(0.3))?);
        Ok(3)
    })?;
    let domain = Interval::new(0.2, 0.3)?;
    measure("canonical_interval", 10_000, || {
        black_box(function.evaluate(&mut Interval64Backend, black_box(domain))?);
        Ok(2)
    })?;
    measure("canonical_interval_jet", 10_000, || {
        black_box(function.jet(&mut Interval64Backend, black_box(domain))?);
        Ok(6)
    })?;
    let mut accuracy = [0.0; 3];
    measure("remez_exp_degree3_gap1e-8", 10, || {
        let r = RemezRequest::binary64(
            quest_polynomial::function!(|x| x.exp()),
            ExactDomain::binary64(-1.0, 1.0),
            3,
        )
        .accuracy(Accuracy::MinimaxGap(ExactConstant::Binary64(1e-8)))
        .export_binary64()
        .run()?;
        assert!(r.minimax_gap().gap().upper() <= 1e-8);
        assert!(r.uniform_error().unconditional_bound().upper() < 0.006);
        accuracy = [
            r.uniform_error().unconditional_bound().upper(),
            r.minimax_gap().lower_bound().lower(),
            r.minimax_gap().gap().upper(),
        ];
        let exported = r.binary64_polynomial()?;
        let n = exported.coefficients().len();
        black_box(exported);
        black_box(r);
        Ok(n)
    })?;
    eprintln!(
        "remez_uniform_upper={:.17e} minimax_lower={:.17e} gap_upper={:.17e}",
        accuracy[0], accuracy[1], accuracy[2]
    );
    measure("newton_two_roots", 10000, || {
        let r = quest_numerics::roots::newton(
            &mut Interval64Backend,
            Interval::new(-2.0, 2.0)?,
            Interval::point(black_box(0.0))?,
            |b, x| {
                Ok(First {
                    value: x.square()?.checked_sub(Interval::point(1.0)?)?,
                    first: b.mul(*x, Interval::point(2.0)?)?,
                })
            },
            quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
        )?;
        assert_eq!(r.images.len(), 2);
        assert!(r.images[0].contains(-1.0));
        assert!(r.images[1].contains(1.0));
        assert_eq!(r.images[0].lower(), -2.0);
        assert!((r.images[0].upper() + 0.25).abs() < 1e-14);
        assert!((r.images[1].lower() - 0.25).abs() < 1e-14);
        assert_eq!(r.images[1].upper(), 2.0);
        let n = r.images.len() * 2;
        black_box(r);
        Ok(n)
    })?;
    Ok(())
}
