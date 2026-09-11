use googletest::prelude::*;
use quest_qsvt_io::{IoPolicy, catalog_families, find_catalog_family};

#[gtest]
fn all_twenty_one_families_preserve_exact_odd_support() -> Result<()> {
    expect_that!(catalog_families().len(), eq(21));
    for family in catalog_families() {
        let polynomial = family.polynomial(IoPolicy::default())?;
        expect_that!(
            polynomial.coefficients().len().checked_sub(1),
            eq(Some(family.degree()))
        );
        expect_true!(
            polynomial
                .coefficients()
                .iter()
                .step_by(2)
                .all(|v| v.re == 0.0 && v.im == 0.0)
        );
    }
    expect_that!(
        catalog_families()
            .iter()
            .map(quest_qsvt_io::CatalogFamily::degree)
            .max(),
        eq(Some(8105))
    );
    expect_true!(find_catalog_family(1500, 0.001).is_some());
    expect_true!(find_catalog_family(1500, 0.0001).is_none());
    expect_true!(find_catalog_family(1499, 0.001).is_none());
    Ok(())
}
