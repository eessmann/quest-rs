use crate::{Complex64, Error, IoPolicy, Result, catalog_data};
use quest_polynomial::{Chebyshev, Polynomial};

/// Static `PennyLane` inverse family. Labels are provenance, not error certificates.
#[derive(Debug)]
pub struct CatalogFamily {
    pub(super) kappa: u32,
    pub(super) epsilon: f64,
    pub(super) degree: usize,
    pub(super) coefficients: &'static [u8],
}
impl CatalogFamily {
    #[must_use]
    pub const fn kappa(&self) -> u32 {
        self.kappa
    }
    #[must_use]
    pub const fn epsilon_label(&self) -> f64 {
        self.epsilon
    }
    #[must_use]
    pub const fn degree(&self) -> usize {
        self.degree
    }
    #[must_use]
    pub fn reciprocal_scale(&self) -> f64 {
        0.5 / f64::from(self.kappa)
    }
    #[must_use]
    pub const fn source_revision(&self) -> &'static str {
        catalog_data::SOURCE_REVISION
    }
    /// Materialize the exact stored binary64 coefficients without HDF5, Python,
    /// synthesis or any arbitrary-precision dependency.
    ///
    /// # Errors
    /// Rejects storage limits or inconsistent generated data.
    pub fn polynomial(&self, policy: IoPolicy) -> Result<Polynomial<Chebyshev>> {
        let count = self
            .degree
            .checked_add(1)
            .ok_or(Error::Budget("catalog degree"))?;
        if count > policy.max_coefficients
            || self.coefficients.len()
                != count.checked_mul(8).ok_or(Error::Budget("catalog bytes"))?
        {
            return Err(Error::Budget("catalog coefficients"));
        }
        policy.check(count, 1)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| Error::Budget("catalog allocation"))?;
        for chunk in self.coefficients.as_chunks::<8>().0 {
            values.push(Complex64::new(f64::from_le_bytes(*chunk), 0.0));
        }
        Ok(Polynomial::new(
            Chebyshev,
            values,
            policy.polynomial_limits(),
        )?)
    }
}
#[must_use]
pub const fn catalog_families() -> &'static [CatalogFamily] {
    catalog_data::FAMILIES
}
/// Select an exact family label. No tolerance is relaxed and no nearby family
/// is substituted silently.
#[must_use]
pub fn find_catalog_family(kappa: u32, epsilon: f64) -> Option<&'static CatalogFamily> {
    catalog_families()
        .iter()
        .find(|family| family.kappa == kappa && family.epsilon.to_bits() == epsilon.to_bits())
}
