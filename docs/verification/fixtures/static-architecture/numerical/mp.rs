include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../allocator.rs"));
use quest_numerics::arithmetic::{Backend, ExactConstant, MpBackend, MpIntervalBackend, Precision};
use quest_polynomial::{
    Accuracy, DynamicShape, ExactDomain, MpHouseholder, RemezOptions, RemezRequest,
};
use std::hint::black_box;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    header();
    measure("mp256_rational_third_degree0_uniform1e-50", 10, || {
        let precision = Precision {
            bits: 256,
            ..Precision::default()
        };
        let exact = quest_polynomial::typed::exact(ExactConstant::Rational(1, 3));
        let r = RemezRequest::new(
            quest_polynomial::function!(|x| exact),
            ExactDomain::binary64(-1.0, 1.0),
            DynamicShape(1),
            MpBackend::new(precision)?,
            MpIntervalBackend::new(precision)?,
            MpHouseholder,
        )
        .options(RemezOptions {
            accuracy: Accuracy::UniformError(ExactConstant::Decimal("1e-50".into())),
            root_width: ExactConstant::Decimal("1e-55".into()),
            ..RemezOptions::default()
        })
        .run()?;
        let mut backend = MpBackend::new(precision)?;
        let tolerance = backend.constant(&ExactConstant::Decimal("1e-50".into()))?;
        assert!(r.uniform_error().unconditional_bound().upper() <= &tolerance);
        assert!(r.binary64_polynomial().is_err());
        let n = r.polynomial().coefficients().len();
        black_box(r);
        Ok(n)
    })?;
    Ok(())
}
