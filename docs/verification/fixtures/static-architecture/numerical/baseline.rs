include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../allocator.rs"));
use quest_polynomial::Interval;
use std::hint::black_box;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    header()?;
    let function = black_box(quest_polynomial::function!(|x| (x.clone() * x + 1.0).ln()));
    measure("canonical_construct", 10_000, || {
        black_box(quest_polynomial::function!(|x| (x.clone() * x + 1.0).ln()));
        Ok(1)
    })?;
    measure("canonical_value", 1_000_000, || {
        black_box(function.evaluate(black_box(0.3))?);
        Ok(1)
    })?;
    measure("canonical_jet", 1_000_000, || {
        black_box(function.jet(black_box(0.3))?);
        Ok(3)
    })?;
    let domain = Interval::new(0.2, 0.3)?;
    measure("canonical_interval", 10_000, || {
        black_box(function.evaluate_interval(black_box(domain))?);
        Ok(2)
    })?;
    measure("canonical_interval_jet", 10_000, || {
        black_box(function.jet_interval(black_box(domain))?);
        Ok(6)
    })?;
    let mut accuracy = [0.0; 3];
    measure("remez_exp_degree3_gap1e-8", 10, || {
        let r = quest_polynomial::RemezBuilder::new()
            .target(quest_polynomial::function!(|x| x.exp()))
            .domain(Interval::new(-1.0, 1.0)?)?
            .degree(3)
            .tolerance(1e-8)
            .run()?;
        assert!(r.error_bound().upper() - r.minimax_lower_bound() <= 1e-8);
        assert!(r.error_bound().upper() < 0.006);
        accuracy = [
            r.error_bound().upper(),
            r.minimax_lower_bound(),
            r.error_bound().upper() - r.minimax_lower_bound(),
        ];
        let n = r.polynomial().coefficients().len();
        black_box(r);
        Ok(n)
    })?;
    eprintln!(
        "remez_uniform_upper={:.17e} minimax_lower={:.17e} gap_upper={:.17e}",
        accuracy[0], accuracy[1], accuracy[2]
    );
    measure("newton_two_roots", 10000, || {
        let r = quest_numerics::extended_newton(
            Interval::new(-2.0, 2.0)?,
            black_box(0.0),
            |x| x.square()?.checked_sub(Interval::point(1.0)?),
            |x| x.checked_mul(Interval::point(2.0)?),
        )?;
        assert_eq!(r.images().len(), 2);
        assert!(r.images()[0].contains(-1.0));
        assert!(r.images()[1].contains(1.0));
        assert_eq!(r.images()[0].lower(), -2.0);
        assert!((r.images()[0].upper() + 0.25).abs() < 1e-14);
        assert!((r.images()[1].lower() - 0.25).abs() < 1e-14);
        assert_eq!(r.images()[1].upper(), 2.0);
        let n = r.images().len() * 2;
        black_box(r);
        Ok(n)
    })?;
    Ok(())
}
