//! Route-bound polynomial meaning for generalized QSVT.
use crate::{Error, Result};
use quest_polynomial::{
    Basis, Chebyshev, Conversion, Even, Laurent, Monomial, Odd, Parity, ParityPolynomial,
    Polynomial,
};
use quest_qsp::{ControlSequence, FrozenCandidate, UnitCircleResponse};
use std::{marker::PhantomData, sync::Arc};

mod sealed {
    pub trait Argument {}
    pub trait Component {}
}
/// The normalized Hermitian argument `x`; block extraction gives singular responses.
#[derive(Debug, Clone, Copy)]
pub struct HermitianArgument;
/// The normalized Gram argument `y = x²`. Coefficients are in `T_k(y)`.
#[derive(Debug, Clone, Copy)]
pub struct GramArgument;
/// An argument domain admitted by the generalized route implementation.
pub trait ResponseArgument: sealed::Argument {
    const DOMAIN: ArgumentDomain;
}
impl sealed::Argument for HermitianArgument {}
impl sealed::Argument for GramArgument {}
impl ResponseArgument for HermitianArgument {
    const DOMAIN: ArgumentDomain = ArgumentDomain::Hermitian;
}
impl ResponseArgument for GramArgument {
    const DOMAIN: ArgumentDomain = ArgumentDomain::Gram;
}
/// The argument is always normalized by the transform's retained source alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentDomain {
    Hermitian,
    Gram,
}
/// Select all coefficients before any block extraction.
#[derive(Debug, Clone, Copy)]
pub struct FullResponse;
/// Explicitly select the even component of a Hermitian response.
#[derive(Debug, Clone, Copy)]
pub struct EvenResponse;
/// Explicitly select the odd component of a Hermitian response.
#[derive(Debug, Clone, Copy)]
pub struct OddResponse;
pub trait ResponseComponent: sealed::Component {
    const COMPONENT: ComponentSelection;
}
impl sealed::Component for FullResponse {}
impl sealed::Component for EvenResponse {}
impl sealed::Component for OddResponse {}
impl ResponseComponent for FullResponse {
    const COMPONENT: ComponentSelection = ComponentSelection::Full;
}
impl ResponseComponent for EvenResponse {
    const COMPONENT: ComponentSelection = ComponentSelection::Even;
}
impl ResponseComponent for OddResponse {
    const COMPONENT: ComponentSelection = ComponentSelection::Odd;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentSelection {
    Full,
    Even,
    Odd,
}

/// Immutable route target.
///
/// Transferring Chebyshev coefficients `c_k` to the
/// unit-circle target `sum c_k z^k` is deliberate; it is not the substitution
/// `x=(z+z^-1)/2`. Monomial inputs require an explicit basis conversion.
#[derive(Debug, Clone)]
pub struct RouteTarget<V: ResponseArgument> {
    polynomial: Polynomial<Chebyshev>,
    conversion: Option<Conversion<Chebyshev, Monomial>>,
    unreduced: Option<Polynomial<Monomial>>,
    unit_circle_source: Option<Polynomial<Laurent>>,
    argument: PhantomData<V>,
}
impl<V: ResponseArgument> RouteTarget<V> {
    #[must_use]
    pub const fn from_chebyshev(polynomial: Polynomial<Chebyshev>) -> Self {
        Self {
            polynomial,
            conversion: None,
            unreduced: None,
            unit_circle_source: None,
            argument: PhantomData,
        }
    }
    /// # Errors
    /// Rejects unsupported or resource-exhausted basis conversion.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Route construction consumes the input while conversion retains its shared immutable storage"
    )]
    pub fn from_monomial(polynomial: Polynomial<Monomial>) -> Result<Self> {
        let conversion = polynomial.to_basis(Chebyshev)?;
        Ok(Self {
            polynomial: conversion.polynomial().clone(),
            conversion: Some(conversion),
            unreduced: None,
            unit_circle_source: None,
            argument: PhantomData,
        })
    }
    /// Transfer each absolute-order coefficient of `z^k` to `T_k(argument)`.
    ///
    /// This explicitly assigns the route's Hermitian or Gram argument. It is
    /// not evaluation at a substituted Laurent variable. Source offset, span
    /// and coefficient bits remain attached to the immutable route meaning.
    /// # Errors
    /// Rejects negative support, padding overflow, and aggregate storage/work limits.
    pub fn from_unit_circle_coefficients(source: Polynomial<Laurent>) -> Result<Self> {
        let offset = usize::try_from(source.basis().offset())
            .map_err(|_| Error::Encoding("negative unit-circle route support"))?;
        let count = offset
            .checked_add(source.coefficients().len().max(1))
            .ok_or(Error::Encoding("unit-circle route support overflow"))?;
        let limits = source.limits();
        let bytes = count
            .checked_add(source.coefficients().len())
            .and_then(|n| n.checked_mul(size_of::<quest_polynomial::Complex64>()))
            .ok_or(Error::Encoding("unit-circle route storage overflow"))?;
        if count > limits.max_coefficients
            || count > limits.max_work
            || bytes > limits.max_bytes
            || isize::try_from(bytes).is_err()
            || i32::try_from(count).is_err()
        {
            return Err(quest_polynomial::Error::Budget("unit-circle route transfer").into());
        }
        let mut coefficients = Vec::new();
        coefficients
            .try_reserve_exact(count)
            .map_err(|_| quest_polynomial::Error::Budget("unit-circle route allocation"))?;
        coefficients.resize(count, quest_polynomial::Complex64::new(0.0, 0.0));
        let end = offset
            .checked_add(source.coefficients().len())
            .ok_or(Error::Encoding("unit-circle route support overflow"))?;
        coefficients
            .get_mut(offset..end)
            .ok_or(Error::Encoding("unit-circle route source span"))?
            .copy_from_slice(source.coefficients());
        Ok(Self {
            polynomial: Polynomial::new(Chebyshev, coefficients, limits)?,
            conversion: None,
            unreduced: None,
            unit_circle_source: Some(source),
            argument: PhantomData,
        })
    }
    #[must_use]
    pub const fn polynomial(&self) -> &Polynomial<Chebyshev> {
        &self.polynomial
    }
    #[must_use]
    pub const fn conversion(&self) -> Option<&Conversion<Chebyshev, Monomial>> {
        self.conversion.as_ref()
    }
    #[must_use]
    pub const fn unreduced_source(&self) -> Option<&Polynomial<Monomial>> {
        self.unreduced.as_ref()
    }
    /// Original Laurent storage when explicit coefficient transfer was requested.
    #[must_use]
    pub const fn unit_circle_source(&self) -> Option<&Polynomial<Laurent>> {
        self.unit_circle_source.as_ref()
    }
    /// Transfer basis coefficients to the QSP unit-circle response, preserving zeros.
    /// # Errors
    /// Rejects polynomial allocation limits.
    pub fn unit_circle_target(&self) -> Result<Polynomial<Laurent>> {
        if let Some(source) = &self.unit_circle_source {
            return Ok(source.clone());
        }
        Ok(Polynomial::new(
            Laurent::new(0),
            self.polynomial.coefficients().to_vec(),
            self.polynomial.limits(),
        )?)
    }
    /// Bind a solver's frozen controls to this exact requested target. This
    /// retains candidate diagnostics; it does not invent a certificate.
    /// # Errors
    /// Rejects any different target coefficient or stored span.
    pub fn bind(self, candidate: FrozenCandidate<UnitCircleResponse>) -> Result<RouteResponse<V>> {
        self.check(&candidate)?;
        let controls = candidate.control_sequence();
        Ok(self.publish(controls, ResponseEvidence::Candidate(Arc::new(candidate))))
    }
    /// Retain the common certifier's evidence for these exact control matrices.
    /// # Errors
    /// Rejects a certificate with a different target.
    #[cfg(feature = "certification")]
    pub fn bind_certified(
        self,
        certificate: quest_qsp::certification::Certified<UnitCircleResponse>,
    ) -> Result<RouteResponse<V>> {
        self.check(certificate.candidate())?;
        let controls = certificate.candidate().control_sequence();
        Ok(self.publish(controls, ResponseEvidence::Certified(Arc::new(certificate))))
    }
    fn check(&self, candidate: &FrozenCandidate<UnitCircleResponse>) -> Result<()> {
        let expected = self.polynomial.coefficients();
        // The solver pads empty Laurent storage to a constant zero. Bind the
        // original source span as well as the frozen coefficients; padding is
        // not permission to substitute a differently stored route target.
        let source_span = self
            .unit_circle_source
            .as_ref()
            .map_or((0, expected.len()), |source| {
                (source.basis().offset(), source.coefficients().len())
            });
        let matching_span = candidate.source_storage() == source_span;
        let matching_coefficients = if expected.is_empty() {
            candidate.target() == [quest_polynomial::Complex64::new(0.0, 0.0)]
        } else {
            expected.len() == candidate.target().len()
                && expected.iter().zip(candidate.target()).all(|(a, b)| {
                    a.re.to_bits() == b.re.to_bits() && a.im.to_bits() == b.im.to_bits()
                })
        };
        if !matching_span || !matching_coefficients {
            return Err(Error::Encoding(
                "QSP evidence belongs to a different route target",
            ));
        }
        Ok(())
    }
    fn publish(self, controls: ControlSequence, evidence: ResponseEvidence) -> RouteResponse<V> {
        RouteResponse {
            controls,
            meaning: RouteMeaning {
                argument: V::DOMAIN,
                component: ComponentSelection::Full,
                target: Some(self.polynomial),
                conversion: self.conversion,
                unreduced: self.unreduced,
                unit_circle_source: self.unit_circle_source,
                evidence,
            },
            markers: PhantomData,
        }
    }
}
impl RouteTarget<GramArgument> {
    /// Convert an exactly even `f(x)` to its reduced `p(y)` with `f(x)=p(x²)`.
    /// # Errors
    /// Rejects conversion or allocation limits; parity was already checked.
    pub fn reduce_even(source: ParityPolynomial<Monomial, Even>) -> Result<Self> {
        Self::reduce(source)
    }
    /// Convert an exactly odd `f(x)` to `p(y)` with `f(x)=x p(x²)`.
    /// # Errors
    /// Rejects conversion or allocation limits; parity was already checked.
    pub fn reduce_odd(source: ParityPolynomial<Monomial, Odd>) -> Result<Self> {
        Self::reduce(source)
    }
    fn reduce<P: Parity>(source: ParityPolynomial<Monomial, P>) -> Result<Self> {
        let original = source.into_polynomial();
        let reduced = Polynomial::new(
            Monomial,
            original
                .coefficients()
                .iter()
                .skip(usize::from(P::ODD))
                .step_by(2)
                .copied()
                .collect(),
            original.limits(),
        )?;
        let mut result = Self::from_monomial(reduced)?;
        result.unreduced = Some(original);
        Ok(result)
    }
}
/// Evidence for the exact immutable generalized control payload.
#[derive(Debug, Clone)]
pub enum ResponseEvidence {
    /// Imported numerical controls; no target accuracy is established.
    Imported,
    Candidate(Arc<FrozenCandidate<UnitCircleResponse>>),
    #[cfg(feature = "certification")]
    Certified(Arc<quest_qsp::certification::Certified<UnitCircleResponse>>),
}
/// Erased immutable meaning retained by the completed transform.
#[derive(Debug, Clone)]
pub struct RouteMeaning {
    argument: ArgumentDomain,
    component: ComponentSelection,
    target: Option<Polynomial<Chebyshev>>,
    conversion: Option<Conversion<Chebyshev, Monomial>>,
    unreduced: Option<Polynomial<Monomial>>,
    unit_circle_source: Option<Polynomial<Laurent>>,
    evidence: ResponseEvidence,
}
impl RouteMeaning {
    #[must_use]
    pub const fn argument(&self) -> ArgumentDomain {
        self.argument
    }
    #[must_use]
    pub const fn component(&self) -> ComponentSelection {
        self.component
    }
    /// Source polynomial before explicit even/odd component extraction.
    #[must_use]
    pub const fn target(&self) -> Option<&Polynomial<Chebyshev>> {
        self.target.as_ref()
    }
    #[must_use]
    pub const fn conversion(&self) -> Option<&Conversion<Chebyshev, Monomial>> {
        self.conversion.as_ref()
    }
    #[must_use]
    pub const fn unreduced_source(&self) -> Option<&Polynomial<Monomial>> {
        self.unreduced.as_ref()
    }
    /// Original Laurent storage when explicit coefficient transfer was requested.
    #[must_use]
    pub const fn unit_circle_source(&self) -> Option<&Polynomial<Laurent>> {
        self.unit_circle_source.as_ref()
    }
    #[must_use]
    pub const fn evidence(&self) -> &ResponseEvidence {
        &self.evidence
    }
}
/// Controls whose polynomial variable and block extraction are explicit types.
///
/// ```compile_fail
/// fn wrong(builder: quest_qsvt::TransformBuilder<quest_qsvt::SuppliedEncoding>,
///          controls: quest_qsvt::RouteResponse<quest_qsvt::HermitianArgument>) {
///     builder.multiplication_even(controls);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct RouteResponse<V: ResponseArgument, P: ResponseComponent = FullResponse> {
    controls: ControlSequence,
    meaning: RouteMeaning,
    markers: PhantomData<(V, P)>,
}
impl<V: ResponseArgument> RouteResponse<V> {
    /// Admit imported controls with explicitly weaker evidence and a chosen variable.
    #[must_use]
    pub const fn imported(controls: ControlSequence) -> Self {
        Self {
            controls,
            meaning: RouteMeaning {
                argument: V::DOMAIN,
                component: ComponentSelection::Full,
                target: None,
                conversion: None,
                unreduced: None,
                unit_circle_source: None,
                evidence: ResponseEvidence::Imported,
            },
            markers: PhantomData,
        }
    }
}
impl RouteResponse<HermitianArgument> {
    #[must_use]
    pub fn even_component(self) -> RouteResponse<HermitianArgument, EvenResponse> {
        self.select()
    }
    #[must_use]
    pub fn odd_component(self) -> RouteResponse<HermitianArgument, OddResponse> {
        self.select()
    }
    fn select<P: ResponseComponent>(self) -> RouteResponse<HermitianArgument, P> {
        let mut meaning = self.meaning;
        meaning.component = P::COMPONENT;
        RouteResponse {
            controls: self.controls,
            meaning,
            markers: PhantomData,
        }
    }
}
impl<V: ResponseArgument, P: ResponseComponent> RouteResponse<V, P> {
    #[must_use]
    pub const fn meaning(&self) -> &RouteMeaning {
        &self.meaning
    }
    pub(crate) fn into_parts(self) -> (ControlSequence, RouteMeaning) {
        (self.controls, self.meaning)
    }
}
