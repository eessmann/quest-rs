//! Arithmetic policies for the same mathematical expression. A backend owns its
//! precision, domain checks, rounding and work accounting; scalars need not be Copy.
use crate::function::Real;
use crate::{Error, Jet, Result};
use std::marker::PhantomData;

pub trait Backend {
    type Scalar: Clone;
    type Error: From<Error>;
    /// # Errors
    /// Rejects undefined arithmetic or exhausted backend resources.
    fn visit(&mut self) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
    /// # Errors
    /// Rejects undefined arithmetic or exhausted backend resources.
    fn point(&mut self, value: f64) -> std::result::Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects undefined arithmetic or exhausted backend resources.
    fn add(
        &mut self,
        a: Self::Scalar,
        b: Self::Scalar,
    ) -> std::result::Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects undefined arithmetic or exhausted backend resources.
    fn sub(
        &mut self,
        a: Self::Scalar,
        b: Self::Scalar,
    ) -> std::result::Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects undefined arithmetic or exhausted backend resources.
    fn mul(
        &mut self,
        a: Self::Scalar,
        b: Self::Scalar,
    ) -> std::result::Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects undefined arithmetic or exhausted backend resources.
    fn div(
        &mut self,
        a: Self::Scalar,
        b: Self::Scalar,
    ) -> std::result::Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects undefined arithmetic or exhausted backend resources.
    fn neg(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects undefined arithmetic or exhausted backend resources.
    fn exp(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects undefined arithmetic or exhausted backend resources.
    fn ln(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects undefined arithmetic or exhausted backend resources.
    fn sin(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects undefined arithmetic or exhausted backend resources.
    fn cos(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error>;
    /// # Errors
    /// Rejects undefined arithmetic or exhausted backend resources.
    fn sqrt(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error>;
}
#[derive(Debug, Default)]
pub struct ScalarBackend<T>(PhantomData<T>);
impl<T> ScalarBackend<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}
impl<T: Real> Backend for ScalarBackend<T> {
    type Scalar = T;
    type Error = Error;
    fn point(&mut self, value: f64) -> Result<T> {
        T::point(value)
    }
    fn add(&mut self, a: T, b: T) -> Result<T> {
        a.add(b)
    }
    fn sub(&mut self, a: T, b: T) -> Result<T> {
        a.sub(b)
    }
    fn mul(&mut self, a: T, b: T) -> Result<T> {
        a.mul(b)
    }
    fn div(&mut self, a: T, b: T) -> Result<T> {
        a.div(b)
    }
    fn neg(&mut self, a: T) -> Result<T> {
        a.neg()
    }
    fn exp(&mut self, a: T) -> Result<T> {
        a.exp()
    }
    fn ln(&mut self, a: T) -> Result<T> {
        a.ln()
    }
    fn sin(&mut self, a: T) -> Result<T> {
        a.sin()
    }
    fn cos(&mut self, a: T) -> Result<T> {
        a.cos()
    }
    fn sqrt(&mut self, a: T) -> Result<T> {
        a.sqrt()
    }
}
/// Lift any arithmetic backend to value, first derivative and second derivative.
/// Differentiation is structural; the backend still controls rounding and precision.
pub struct JetBackend<'a, B: Backend>(pub &'a mut B);
impl<B: Backend> JetBackend<'_, B> {
    /// # Errors
    /// Propagates failures when creating the derivative constants.
    pub fn variable(&mut self, value: B::Scalar) -> std::result::Result<Jet<B::Scalar>, B::Error> {
        Ok(Jet {
            value,
            first: self.0.point(1.0)?,
            second: self.0.point(0.0)?,
        })
    }
    fn chain(
        &mut self,
        a: Jet<B::Scalar>,
        value: B::Scalar,
        first: B::Scalar,
        second: B::Scalar,
    ) -> std::result::Result<Jet<B::Scalar>, B::Error> {
        let d = self.0.mul(first.clone(), a.first.clone())?;
        let d2 = self.0.mul(second, a.first.clone())?;
        let d2 = self.0.mul(d2, a.first)?;
        let term = self.0.mul(first, a.second)?;
        Ok(Jet {
            value,
            first: d,
            second: self.0.add(d2, term)?,
        })
    }
    pub(crate) fn recip(
        &mut self,
        a: Jet<B::Scalar>,
    ) -> std::result::Result<Jet<B::Scalar>, B::Error> {
        let one = self.0.point(1.0)?;
        let v = self.0.div(one, a.value.clone())?;
        let v2 = self.0.mul(v.clone(), v.clone())?;
        let first = self.0.neg(v2.clone())?;
        let second = self.0.mul(v2, v.clone())?;
        let two = self.0.point(2.0)?;
        let second = self.0.mul(two, second)?;
        self.chain(a, v, first, second)
    }
}
impl<B: Backend> Backend for JetBackend<'_, B> {
    type Scalar = Jet<B::Scalar>;
    type Error = B::Error;
    fn visit(&mut self) -> std::result::Result<(), Self::Error> {
        self.0.visit()
    }
    fn point(&mut self, x: f64) -> std::result::Result<Self::Scalar, Self::Error> {
        Ok(Jet {
            value: self.0.point(x)?,
            first: self.0.point(0.0)?,
            second: self.0.point(0.0)?,
        })
    }
    fn add(
        &mut self,
        a: Self::Scalar,
        b: Self::Scalar,
    ) -> std::result::Result<Self::Scalar, Self::Error> {
        Ok(Jet {
            value: self.0.add(a.value, b.value)?,
            first: self.0.add(a.first, b.first)?,
            second: self.0.add(a.second, b.second)?,
        })
    }
    fn sub(
        &mut self,
        a: Self::Scalar,
        b: Self::Scalar,
    ) -> std::result::Result<Self::Scalar, Self::Error> {
        let b = self.neg(b)?;
        self.add(a, b)
    }
    fn neg(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error> {
        Ok(Jet {
            value: self.0.neg(a.value)?,
            first: self.0.neg(a.first)?,
            second: self.0.neg(a.second)?,
        })
    }
    #[expect(
        clippy::many_single_char_names,
        reason = "Short local names follow the derivative product rule"
    )]
    fn mul(
        &mut self,
        a: Self::Scalar,
        b: Self::Scalar,
    ) -> std::result::Result<Self::Scalar, Self::Error> {
        let value = self.0.mul(a.value.clone(), b.value.clone())?;
        let l = self.0.mul(a.first.clone(), b.value.clone())?;
        let r = self.0.mul(a.value.clone(), b.first.clone())?;
        let first = self.0.add(l, r)?;
        let l = self.0.mul(a.second, b.value)?;
        let m = self.0.mul(a.first, b.first)?;
        let two = self.0.point(2.0)?;
        let m = self.0.mul(two, m)?;
        let r = self.0.mul(a.value, b.second)?;
        let second = self.0.add(l, m)?;
        Ok(Jet {
            value,
            first,
            second: self.0.add(second, r)?,
        })
    }
    fn div(
        &mut self,
        a: Self::Scalar,
        b: Self::Scalar,
    ) -> std::result::Result<Self::Scalar, Self::Error> {
        let b = self.recip(b)?;
        self.mul(a, b)
    }
    fn exp(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error> {
        let v = self.0.exp(a.value.clone())?;
        self.chain(a, v.clone(), v.clone(), v)
    }
    fn ln(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error> {
        let v = self.0.ln(a.value.clone())?;
        let one = self.0.point(1.0)?;
        let d = self.0.div(one, a.value.clone())?;
        let d2 = self.0.mul(d.clone(), d.clone())?;
        let d2 = self.0.neg(d2)?;
        self.chain(a, v, d, d2)
    }
    fn sin(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error> {
        let v = self.0.sin(a.value.clone())?;
        let d = self.0.cos(a.value.clone())?;
        let d2 = self.0.neg(v.clone())?;
        self.chain(a, v, d, d2)
    }
    fn cos(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error> {
        let v = self.0.cos(a.value.clone())?;
        let d = self.0.sin(a.value.clone())?;
        let d = self.0.neg(d)?;
        let d2 = self.0.neg(v.clone())?;
        self.chain(a, v, d, d2)
    }
    fn sqrt(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error> {
        let v = self.0.sqrt(a.value.clone())?;
        let half = self.0.point(0.5)?;
        let d = self.0.div(half, v.clone())?;
        let v3 = self.0.mul(v.clone(), v.clone())?;
        let v3 = self.0.mul(v3, v.clone())?;
        let quarter = self.0.point(-0.25)?;
        let d2 = self.0.div(quarter, v3)?;
        self.chain(a, v, d, d2)
    }
}
