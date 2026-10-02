use crate::{Basis, Chebyshev, Complex64, Error, Interval, Laurent, Limits, Result, finite, zeros};
use quest_numerics::arithmetic::{Backend, EnclosureBackend, ExactConstant, PointBackend};
use std::{
    ops::{Add, Div, Mul, Sub},
    sync::Arc,
};

mod shape_sealed {
    pub trait Sealed {}
}

/// An admitted coefficient count. Only reviewed static and dynamic shapes can
/// define storage contracts; signed support remains a property of the basis.
pub trait Shape: shape_sealed::Sealed + Clone + std::fmt::Debug {
    fn coefficient_count(&self) -> usize;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynamicShape(pub usize);
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StaticShape<const COUNT: usize>;
impl shape_sealed::Sealed for DynamicShape {}
impl<const COUNT: usize> shape_sealed::Sealed for StaticShape<COUNT> {}
impl Shape for DynamicShape {
    fn coefficient_count(&self) -> usize {
        self.0
    }
}
impl<const COUNT: usize> Shape for StaticShape<COUNT> {
    fn coefficient_count(&self) -> usize {
        COUNT
    }
}
fn check_shape(shape: &impl Shape, actual: usize) -> Result<()> {
    let expected = shape.coefficient_count();
    if expected == actual {
        Ok(())
    } else {
        Err(Error::Shape { expected, actual })
    }
}

/// Immutable basis coefficients with admitted signed support.
#[derive(Debug, Clone)]
pub struct Polynomial<B: Basis, C = Complex64, D: Shape = DynamicShape> {
    basis: B,
    coefficients: Arc<Vec<C>>,
    shape: D,
    limits: Limits,
    last_order: i32,
    effective_support: Option<(i32, i32)>,
}
impl<B: Basis, C: Clone, D: Shape> Polynomial<B, C, D> {
    #[must_use]
    pub fn coefficients(&self) -> &[C] {
        self.coefficients.as_slice()
    }
    #[must_use]
    pub const fn basis(&self) -> &B {
        &self.basis
    }
    #[must_use]
    /// Inclusive orders represented by storage, including zero coefficients.
    /// Empty storage has no span.
    pub fn stored_support(&self) -> Option<(i32, i32)> {
        (!self.coefficients.is_empty()).then(|| (self.basis.offset(), self.last_order))
    }
    /// First and last nonzero orders under the constructor-selected arithmetic.
    /// The constructor backend must satisfy `PointBackend`'s exact comparison laws.
    #[must_use]
    pub const fn effective_support(&self) -> Option<(i32, i32)> {
        self.effective_support
    }
    /// Highest nonzero basis order. Laurent degree may be negative; zero has no degree.
    #[must_use]
    pub fn degree(&self) -> Option<i32> {
        self.effective_support.map(|(_, last)| last)
    }
    /// Last coefficient index, saturating to zero for empty storage.
    /// This is an allocation/recurrence parameter, not mathematical degree.
    #[must_use]
    pub fn stored_order(&self) -> usize {
        self.coefficients.len().saturating_sub(1)
    }
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.effective_support.is_none()
    }
    #[must_use]
    pub const fn limits(&self) -> Limits {
        self.limits
    }

    #[must_use]
    pub const fn shape(&self) -> &D {
        &self.shape
    }
    /// Re-admit the same owned coefficients under another checked count policy.
    /// # Errors
    /// Rejects a shape whose count differs from the retained storage.
    pub fn with_shape<T: Shape>(self, shape: T) -> Result<Polynomial<B, C, T>> {
        check_shape(&shape, self.coefficients.len())?;
        Ok(Polynomial {
            basis: self.basis,
            coefficients: self.coefficients,
            shape,
            limits: self.limits,
            last_order: self.last_order,
            effective_support: self.effective_support,
        })
    }

    /// Admit real scalar coefficients without changing their stored precision.
    /// # Errors
    /// Rejects shape/support/storage/work limits and backend-invalid coefficients.
    pub fn from_scalars<A: PointBackend<Scalar = C>>(
        basis: B,
        coefficients: Vec<C>,
        shape: D,
        backend: &mut A,
        limits: Limits,
    ) -> Result<Self>
    where
        A::Error: Into<Error>,
    {
        check_shape(&shape, coefficients.len())?;
        let count = coefficients.len();
        let work = count
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .ok_or(Error::Budget("coefficient validation work"))?;
        if count > limits.max_coefficients
            || work > limits.max_work
            || i32::try_from(count).is_err()
        {
            return Err(Error::Budget("coefficient storage or validation work"));
        }
        let bytes = coefficient_heap_bytes(&coefficients, backend)?;
        let zero = backend
            .constant(&ExactConstant::Integer(0))
            .map_err(Into::into)?;
        let mut support: Option<(i32, i32)> = None;
        for (index, value) in coefficients.iter().enumerate() {
            backend.visit().map_err(Into::into)?;
            // Comparison validates backend error-valued/nonfinite scalars before
            // deciding effective zero, including trailing zero/support shortcuts.
            let nonzero =
                backend.compare(value, &zero).map_err(Into::into)? != std::cmp::Ordering::Equal;
            if nonzero {
                let order = i32::try_from(index)
                    .map_err(|_| Error::SupportOverflow)?
                    .checked_add(basis.offset())
                    .ok_or(Error::SupportOverflow)?;
                support = Some((support.map_or(order, |(first, _)| first), order));
            }
        }
        if bytes > limits.max_bytes || isize::try_from(bytes).is_err() {
            return Err(Error::Budget("coefficient storage"));
        }
        let last_order = basis
            .offset()
            .checked_add(
                i32::try_from(count.saturating_sub(1)).map_err(|_| Error::SupportOverflow)?,
            )
            .ok_or(Error::SupportOverflow)?;
        Ok(Self {
            basis,
            coefficients: Arc::new(coefficients),
            shape,
            limits,
            last_order,
            effective_support: support,
        })
    }

    /// Evaluate with statically selected checked point arithmetic.
    /// # Errors
    /// Propagates arithmetic/resource errors and genuine Laurent poles.
    pub fn evaluate_with<A: Backend<Scalar = C>>(&self, backend: &mut A, argument: C) -> Result<C>
    where
        A::Error: Into<Error>,
    {
        self.evaluate_lifted(backend, argument, |value, _| Ok(value.clone()))
    }
    /// Enclose the exact stored real coefficients through singleton endpoints.
    /// # Errors
    /// Rejects enclosure/resource failures and genuine Laurent poles.
    pub fn evaluate_enclosure<A: EnclosureBackend<Endpoint = C>>(
        &self,
        backend: &mut A,
        argument: A::Scalar,
    ) -> Result<A::Scalar>
    where
        A::Error: Into<Error>,
    {
        self.evaluate_lifted(backend, argument, |value, backend| {
            backend.singleton(value).map_err(Into::into)
        })
    }

    /// Enclose value, first derivative, and second derivative of the exact
    /// retained coefficients through the same recurrence used for point values.
    /// # Errors
    /// Propagates arithmetic/resource errors and derivative-domain poles.
    pub fn jet_enclosure<A: EnclosureBackend<Endpoint = C>>(
        &self,
        backend: &mut A,
        argument: A::Scalar,
    ) -> Result<quest_numerics::ad::Jet<A::Scalar>>
    where
        A::Error: Into<Error>,
    {
        let mut ad = quest_numerics::ad::JetBackend(backend);
        let variable = ad.variable(argument).map_err(Into::into)?;
        self.evaluate_lifted(&mut ad, variable, |coefficient, ad| {
            let value = ad.0.singleton(coefficient).map_err(Into::into)?;
            let zero =
                ad.0.constant(&ExactConstant::Integer(0))
                    .map_err(Into::into)?;
            Ok(quest_numerics::ad::Jet {
                value,
                first: zero.clone(),
                second: zero,
            })
        })
    }
    /// Value and two mathematical derivatives in the selected point arithmetic.
    /// # Errors
    /// Propagates arithmetic/resource errors and derivative-domain poles.
    pub fn jet_with<A: Backend<Scalar = C>>(
        &self,
        backend: &mut A,
        argument: C,
    ) -> Result<quest_numerics::ad::Jet<C>>
    where
        A::Error: Into<Error>,
    {
        let mut ad = quest_numerics::ad::JetBackend(backend);
        let variable = ad.variable(argument).map_err(Into::into)?;
        self.evaluate_lifted(&mut ad, variable, |coefficient, ad| {
            let zero =
                ad.0.constant(&ExactConstant::Integer(0))
                    .map_err(Into::into)?;
            Ok(quest_numerics::ad::Jet {
                value: coefficient.clone(),
                first: zero.clone(),
                second: zero,
            })
        })
    }

    /// Bytes retained by the shared coefficient allocation, excluding the
    /// inline Polynomial/basis/shape and counting a shared allocation once.
    /// # Errors
    /// Rejects invalid backend scalar storage models and byte-count overflow.
    pub fn retained_heap_bytes<A: Backend<Scalar = C>>(&self, backend: &A) -> Result<usize>
    where
        A::Error: Into<Error>,
    {
        coefficient_heap_bytes(&self.coefficients, backend)
    }

    pub(crate) fn evaluate_lifted<A: Backend>(
        &self,
        backend: &mut A,
        argument: A::Scalar,
        mut lift: impl FnMut(&C, &mut A) -> Result<A::Scalar>,
    ) -> Result<A::Scalar>
    where
        A::Error: Into<Error>,
    {
        backend.visit().map_err(Into::into)?;
        let zero = backend
            .constant(&ExactConstant::Integer(0))
            .map_err(Into::into)?;
        // Validate even when the zero polynomial would bypass all recurrence work.
        backend.validate(&argument).map_err(Into::into)?;
        let mut offset = self.basis.offset();
        let mut coefficients = self.coefficients.as_slice();
        if let Some((_, last)) = self.effective_support {
            let end = last
                .checked_sub(offset)
                .and_then(|index| index.checked_add(1))
                .and_then(|index| usize::try_from(index).ok())
                .ok_or(Error::SupportOverflow)?;
            coefficients = coefficients.get(..end).ok_or(Error::SupportOverflow)?;
        }
        if self.is_zero() {
            offset = 0;
            coefficients = &[];
        } else if offset < 0 && self.effective_support.is_some_and(|(first, _)| first >= 0) {
            let skip =
                usize::try_from(offset.unsigned_abs()).map_err(|_| Error::SupportOverflow)?;
            coefficients = coefficients.get(skip..).ok_or(Error::SupportOverflow)?;
            offset = 0;
        }
        let magnitude = offset.unsigned_abs();
        let shift_work = if magnitude == 0 {
            0
        } else {
            usize::try_from(
                magnitude
                    .count_ones()
                    .checked_add(u32::BITS)
                    .and_then(|n| n.checked_sub(magnitude.leading_zeros()))
                    .ok_or(Error::Budget("evaluation work"))?,
            )
            .map_err(|_| Error::Budget("evaluation work"))?
            .checked_add(usize::from(offset < 0))
            .ok_or(Error::Budget("evaluation work"))?
        };
        let work = coefficients
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(shift_work))
            .ok_or(Error::Budget("evaluation work"))?;
        if work > self.limits.max_work {
            return Err(Error::Budget("evaluation work"));
        }
        if coefficients.len() == 1 {
            let coefficient = coefficients.first().ok_or(Error::Domain)?;
            let value = lift(coefficient, backend)?;
            let value = backend.add(value, zero).map_err(Into::into)?;
            if offset == 0 {
                return Ok(value);
            }
            let shift = backend_power(backend, argument, offset)?;
            return backend.mul(value, shift).map_err(Into::into);
        }
        let mut remaining = coefficients.iter().enumerate().rev();
        let Some((_, leading)) = remaining.next() else {
            return Ok(zero);
        };
        // The leading Clenshaw value is the coefficient itself. Its following
        // states are zero, so no higher basis recurrence is mathematically used.
        let leading = lift(leading, backend)?;
        let mut following = backend.add(leading, zero.clone()).map_err(Into::into)?;
        let mut after_following = zero;
        let mut needs_back = false;
        for (index, coefficient) in remaining {
            let order = u32::try_from(index).map_err(|_| Error::SupportOverflow)?;
            let (a, b, _) = self
                .basis
                .recurrence_with(order.checked_add(1).ok_or(Error::SupportOverflow)?, backend)
                .map_err(Into::into)?;
            let linear = backend.mul(argument.clone(), a).map_err(Into::into)?;
            let linear = backend.add(linear, b).map_err(Into::into)?;
            let value = backend.mul(linear, following.clone()).map_err(Into::into)?;
            let coefficient = lift(coefficient, backend)?;
            let mut value = backend.add(coefficient, value).map_err(Into::into)?;
            if needs_back {
                let (_, _, c) = self
                    .basis
                    .recurrence_with(order.checked_add(2).ok_or(Error::SupportOverflow)?, backend)
                    .map_err(Into::into)?;
                let back = backend.mul(after_following, c).map_err(Into::into)?;
                value = backend.sub(value, back).map_err(Into::into)?;
            }
            after_following = following;
            following = value;
            needs_back = true;
        }
        if offset == 0 {
            return Ok(following);
        }
        let shift = backend_power(backend, argument, offset)?;
        backend.mul(following, shift).map_err(Into::into)
    }
}

fn coefficient_heap_bytes<C, A: Backend<Scalar = C>>(
    coefficients: &Vec<C>,
    backend: &A,
) -> Result<usize>
where
    A::Error: Into<Error>,
{
    let mut bytes = coefficients
        .capacity()
        .checked_mul(size_of::<C>())
        .and_then(|n| n.checked_add(const { size_of::<Vec<C>>() + 2 * size_of::<usize>() }))
        .ok_or(Error::Budget("coefficient storage overflow"))?;
    for value in coefficients {
        let heap = backend
            .storage_bytes(value)
            .map_err(Into::into)?
            .checked_sub(size_of::<C>())
            .ok_or(Error::Budget("invalid backend scalar storage model"))?;
        bytes = bytes
            .checked_add(heap)
            .ok_or(Error::Budget("coefficient storage overflow"))?;
    }
    Ok(bytes)
}

impl<B: Basis> Polynomial<B> {
    /// # Errors
    /// Rejects nonfinite coefficients, support overflow, or excessive storage.
    pub fn new(basis: B, coefficients: Vec<Complex64>, limits: Limits) -> Result<Self> {
        limits.check(coefficients.len(), 1)?;
        for value in &coefficients {
            finite(*value)?;
        }
        let last = i32::try_from(coefficients.len().saturating_sub(1))
            .map_err(|_| Error::SupportOverflow)?;
        let last_order = basis
            .offset()
            .checked_add(last)
            .ok_or(Error::SupportOverflow)?;
        let nonzero = |value: &Complex64| *value != Complex64::new(0.0, 0.0);
        let effective_support = coefficients
            .iter()
            .position(nonzero)
            .zip(coefficients.iter().rposition(nonzero))
            .map(|(first, last)| {
                let order = |index| {
                    i32::try_from(index)
                        .map_err(|_| Error::SupportOverflow)?
                        .checked_add(basis.offset())
                        .ok_or(Error::SupportOverflow)
                };
                Ok::<_, Error>((order(first)?, order(last)?))
            })
            .transpose()?;
        Ok(Self {
            basis,
            shape: DynamicShape(coefficients.len()),
            coefficients: Arc::new(coefficients),
            limits,
            last_order,
            effective_support,
        })
    }
}
impl<B: Basis, D: Shape> Polynomial<B, Complex64, D> {
    /// Evaluate by generalized Clenshaw followed by the explicit Laurent shift.
    /// # Errors
    /// Rejects nonfinite arithmetic and Laurent poles at zero.
    pub fn evaluate(&self, argument: Complex64) -> Result<Complex64> {
        finite(argument)?;
        let support = self.effective_support;
        if support.is_none() {
            return Ok(Complex64::new(0.0, 0.0));
        }
        if argument == Complex64::new(0.0, 0.0) && self.basis.offset() < 0 {
            if support.is_some_and(|(first, _)| first < 0) {
                return Err(Error::Domain);
            }
            let index = usize::try_from(self.basis.offset().unsigned_abs())
                .map_err(|_| Error::SupportOverflow)?;
            return Ok(self.coefficients.get(index).copied().unwrap_or_default());
        }
        if self.coefficients.len() == 1 {
            return finite(
                self.coefficients
                    .first()
                    .copied()
                    .ok_or(Error::Domain)?
                    .mul(power(argument, self.basis.offset())?),
            );
        }
        let mut following = Complex64::new(0.0, 0.0);
        let mut after_following = following;
        for (index, coefficient) in self.coefficients.iter().enumerate().rev() {
            let order = u32::try_from(index).map_err(|_| Error::SupportOverflow)?;
            let (a, b, _) = self
                .basis
                .recurrence(order.checked_add(1).ok_or(Error::SupportOverflow)?)?;
            let (_, _, c) = self
                .basis
                .recurrence(order.checked_add(2).ok_or(Error::SupportOverflow)?)?;
            let value = finite(
                coefficient
                    .add(argument.mul(a).add(b).mul(following))
                    .sub(after_following.mul(c)),
            )?;
            after_following = following;
            following = value;
        }
        finite(following.mul(power(argument, self.basis.offset())?))
    }

    /// # Errors
    /// Rejects complex coefficients or nonfinite/domain arithmetic.
    pub fn evaluate_real(&self, argument: f64) -> Result<f64> {
        if self.coefficients.iter().any(|value| value.im != 0.0) {
            return Err(Error::NotReal);
        }
        Ok(self.evaluate(Complex64::new(argument, 0.0))?.re)
    }
}

impl<D: Shape> Polynomial<Chebyshev, Complex64, D> {
    /// Substitute x=(z+1/z)/2 without a monomial conversion.
    /// # Errors
    /// Rejects support/allocation overflow, insufficient storage, or subnormal
    /// coefficients whose exact halving is not representable in binary64.
    pub fn on_cosine_circle(&self) -> Result<Polynomial<Laurent>> {
        let degree = self.stored_order();
        let count = degree
            .checked_mul(2)
            .and_then(|x| x.checked_add(1))
            .ok_or(Error::SupportOverflow)?;
        self.limits.check(count, 2)?;
        let mut coefficients = zeros(count, self.limits)?;
        for (index, value) in self.coefficients.iter().enumerate() {
            if index == 0 {
                *coefficients.get_mut(degree).ok_or(Error::SupportOverflow)? = *value;
            } else {
                for component in [value.re, value.im] {
                    let half = Interval::point(component)?.checked_mul(Interval::point(0.5)?)?;
                    if half.lower().to_bits() != half.upper().to_bits()
                        && !(half.lower() == 0.0 && half.upper() == 0.0)
                    {
                        return Err(Error::NotEstablished(
                            "cosine substitution loses a subnormal coefficient",
                        ));
                    }
                }
                let left = degree.checked_sub(index).ok_or(Error::SupportOverflow)?;
                let right = degree.checked_add(index).ok_or(Error::SupportOverflow)?;
                *coefficients.get_mut(left).ok_or(Error::SupportOverflow)? = value.mul(0.5);
                *coefficients.get_mut(right).ok_or(Error::SupportOverflow)? = value.mul(0.5);
            }
        }
        let offset = i32::try_from(degree)
            .map_err(|_| Error::SupportOverflow)?
            .checked_neg()
            .ok_or(Error::SupportOverflow)?;
        Polynomial::new(Laurent::new(offset), coefficients, self.limits)
    }
}

fn power(mut base: Complex64, exponent: i32) -> Result<Complex64> {
    let mut result = Complex64::new(1.0, 0.0);
    let mut magnitude = exponent.unsigned_abs();
    while magnitude != 0 {
        if magnitude & 1 != 0 {
            result = finite(result.mul(base))?;
        }
        magnitude >>= 1;
        if magnitude != 0 {
            base = finite(base.mul(base))?;
        }
    }
    if exponent < 0 {
        if result == Complex64::new(0.0, 0.0) {
            return Err(Error::Domain);
        }
        finite(Complex64::new(1.0, 0.0).div(result))
    } else {
        Ok(result)
    }
}

fn backend_power<A: Backend>(
    backend: &mut A,
    mut base: A::Scalar,
    exponent: i32,
) -> Result<A::Scalar>
where
    A::Error: Into<Error>,
{
    let one = backend
        .constant(&ExactConstant::Integer(1))
        .map_err(Into::into)?;
    let mut result = one.clone();
    let mut magnitude = exponent.unsigned_abs();
    while magnitude != 0 {
        if magnitude & 1 != 0 {
            result = backend.mul(result, base.clone()).map_err(Into::into)?;
        }
        magnitude >>= 1;
        if magnitude != 0 {
            base = backend.mul(base.clone(), base).map_err(Into::into)?;
        }
    }
    if exponent < 0 {
        backend.div(one, result).map_err(Into::into)
    } else {
        Ok(result)
    }
}
