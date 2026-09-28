use std::{
    collections::BTreeMap,
    ops::Deref,
    sync::{Arc, OnceLock},
};

use crate::BigRational;
use num_bigint::BigInt;
use num_traits::Zero;
use quest_symbolic::{Expr, Owner, Symbol};

use crate::{Error, NumericalOperator, Result};

macro_rules! owned_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name {
            pub(crate) owner: u64,
            pub(crate) index: usize,
        }
        impl $name {
            pub const fn index(self) -> usize {
                self.index
            }
        }
    };
}
owned_id!(QubitId);
owned_id!(BitId);
owned_id!(ParameterId);
owned_id!(OccurrenceId);
owned_id!(GateDefinitionId);

/// A finite floating angle remains opaque to exact rewrite rules.
#[derive(Debug, Clone, PartialEq)]
pub struct Angle(pub(crate) AngleExpr);

#[derive(Debug)]
#[expect(
    clippy::redundant_pub_crate,
    reason = "Internal wrapper is reachable only through the private model module"
)]
pub(crate) struct SymbolicAngle {
    expr: Expr,
    finite_constant: OnceLock<f64>,
}
impl SymbolicAngle {
    fn new(expr: Expr) -> Arc<Self> {
        Arc::new(Self {
            expr,
            finite_constant: OnceLock::new(),
        })
    }
}
impl Deref for SymbolicAngle {
    type Target = Expr;
    fn deref(&self) -> &Self::Target {
        &self.expr
    }
}
impl PartialEq for SymbolicAngle {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::redundant_pub_crate,
    reason = "Keep internal state out of the crate wildcard public re-export"
)]
pub(crate) enum AngleExpr {
    Symbolic(Arc<SymbolicAngle>),
    Opaque(f64),
    Negative(Arc<Self>),
}

/// Exact target identity retained beside a numerical bound gate parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundAngleTarget {
    DyadicRadians {
        bits: u64,
    },
    RationalPi {
        numerator: BigInt,
        denominator: BigInt,
    },
    AffinePi {
        radians_numerator: BigInt,
        radians_denominator: BigInt,
        pi_numerator: BigInt,
        pi_denominator: BigInt,
    },
}

impl Angle {
    pub(crate) fn binding_work_estimate(&self) -> Result<u64> {
        match &self.0 {
            AngleExpr::Symbolic(value) => value.binding_work_estimate().map_err(Error::from),
            AngleExpr::Opaque(_) => Ok(1),
            AngleExpr::Negative(inner) => Self((**inner).clone())
                .binding_work_estimate()?
                .checked_add(1)
                .ok_or(Error::Budget("angle binding work")),
        }
    }
    pub(crate) fn retained_bytes(&self) -> Result<usize> {
        match &self.0 {
            AngleExpr::Symbolic(value) => value
                .retained_bytes()
                .map_err(Error::from)?
                .checked_add(
                    const {
                        std::mem::size_of::<SymbolicAngle>() - std::mem::size_of::<Expr>()
                            + 3 * std::mem::size_of::<usize>()
                    },
                )
                .ok_or(Error::Budget("symbolic angle storage")),
            AngleExpr::Opaque(_) => Ok(0),
            AngleExpr::Negative(inner) => Self((**inner).clone())
                .retained_bytes()?
                .checked_add(
                    const { std::mem::size_of::<AngleExpr>() + 2 * std::mem::size_of::<usize>() },
                )
                .ok_or(Error::Budget("angle storage")),
        }
    }
    pub(crate) fn equivalent_checked(&self, other: &Self) -> Result<bool> {
        match (&self.0, &other.0) {
            (AngleExpr::Symbolic(a), AngleExpr::Symbolic(b)) => Ok(a.equivalent_checked(b)?),
            _ => Ok(self == other),
        }
    }
    pub(crate) fn equivalence_work_estimate(&self) -> Result<u64> {
        match &self.0 {
            AngleExpr::Symbolic(value) => u64::try_from(value.source_node_count())
                .map_err(|_| Error::Budget("angle equivalence work")),
            AngleExpr::Opaque(_) => Ok(1),
            AngleExpr::Negative(inner) => Self((**inner).clone())
                .equivalence_work_estimate()?
                .checked_add(1)
                .ok_or(Error::Budget("angle equivalence work")),
        }
    }
    pub(crate) fn substitute(
        &self,
        bindings: &BTreeMap<ParameterId, Self>,
        owner: u64,
    ) -> Result<Self> {
        match &self.0 {
            AngleExpr::Symbolic(value) => {
                let mut replacements = Vec::new();
                for symbol in value.parameters() {
                    let id = ParameterId {
                        owner: symbol.owner().id(),
                        index: usize::try_from(symbol.index()).map_err(|_| Error::InvalidId)?,
                    };
                    replacements.push((symbol, bindings.get(&id).ok_or(Error::Binding)?));
                }
                if replacements.is_empty() {
                    return Ok(self.clone());
                }
                if replacements.iter().any(|(_, angle)| !angle.is_exact()) {
                    if let Some((symbol, negative)) = value.signed_parameter_leaf() {
                        let id = ParameterId {
                            owner: symbol.owner().id(),
                            index: usize::try_from(symbol.index()).map_err(|_| Error::InvalidId)?,
                        };
                        let argument = bindings.get(&id).ok_or(Error::Binding)?;
                        return if negative {
                            argument.negated()
                        } else {
                            Ok(argument.clone())
                        };
                    }
                    return Err(Error::Unsupported("opaque compound angle substitution"));
                }
                let exact = replacements
                    .into_iter()
                    .map(|(symbol, angle)| match &angle.0 {
                        AngleExpr::Symbolic(value) => Ok((symbol, value.expr.clone())),
                        _ => Err(Error::Unsupported("opaque compound angle substitution")),
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Self(AngleExpr::Symbolic(SymbolicAngle::new(
                    value.substitute_into(Owner::new(owner), &exact)?,
                ))))
            }
            AngleExpr::Negative(inner) => Self((**inner).clone())
                .substitute(bindings, owner)?
                .negated(),
            AngleExpr::Opaque(_) => Ok(self.clone()),
        }
    }
    /// # Errors
    /// Rejects a zero denominator.
    pub fn pi(numerator: i64, denominator: i64) -> Result<Self> {
        if denominator == 0 {
            return Err(Error::ZeroDenominator);
        }
        Ok(Self(AngleExpr::Symbolic(SymbolicAngle::new(Expr::pi(
            BigRational::new(numerator.into(), denominator.into()),
        )?))))
    }
    /// Admit an arbitrary rational multiple of pi. Ratios made with
    /// `BigRational::new_raw` are validated and normalized here. Conversion to
    /// finite machine radians remains a fallible step during binding.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Admission takes ownership consistently with all angle constructors"
    )]
    /// # Errors
    /// Rejects a zero denominator, including one supplied through a raw rational.
    pub fn rational_pi(value: BigRational) -> Result<Self> {
        if value.denom().is_zero() {
            return Err(Error::ZeroDenominator);
        }
        if value.numer().bits().max(value.denom().bits()) > 16_384 {
            return Err(Error::Budget("exact angle coefficient bits"));
        }
        Ok(Self(AngleExpr::Symbolic(SymbolicAngle::new(Expr::pi(
            BigRational::new(value.numer().clone(), value.denom().clone()),
        )?))))
    }
    /// Admit an exact `r + s*pi` angle.
    /// # Errors
    /// Rejects invalid rationals or exhausted exact-expression budgets.
    pub fn affine(radians: BigRational, pi: BigRational) -> Result<Self> {
        if radians.denom().is_zero() || pi.denom().is_zero() {
            return Err(Error::ZeroDenominator);
        }
        if [
            radians.numer().bits(),
            radians.denom().bits(),
            pi.numer().bits(),
            pi.denom().bits(),
        ]
        .into_iter()
        .any(|bits| bits > 16_384)
        {
            return Err(Error::Budget("exact angle coefficient bits"));
        }
        Ok(Self(AngleExpr::Symbolic(SymbolicAngle::new(Expr::affine(
            radians, pi,
        )?))))
    }
    /// # Errors
    /// Rejects nonfinite angle values.
    pub const fn radians(value: f64) -> Result<Self> {
        if !value.is_finite() {
            return Err(Error::NonFinite);
        }
        Ok(Self(AngleExpr::Opaque(value)))
    }
    /// # Errors
    /// Rejects an unrepresentable parameter index or exhausted exact budgets.
    pub fn parameter(id: ParameterId) -> Result<Self> {
        let index = u64::try_from(id.index).map_err(|_| Error::InvalidId)?;
        Ok(Self(AngleExpr::Symbolic(SymbolicAngle::new(
            Expr::parameter(Symbol::new(Owner::new(id.owner), index))?,
        ))))
    }
    /// # Errors
    /// Rejects exhausted exact-expression budgets.
    pub fn negated(&self) -> Result<Self> {
        match &self.0 {
            AngleExpr::Symbolic(value) => {
                let result = SymbolicAngle::new(value.neg()?);
                if let Some(finite) = value.finite_constant.get() {
                    let _ = result.finite_constant.set(-finite);
                }
                Ok(Self(AngleExpr::Symbolic(result)))
            }
            AngleExpr::Negative(x) => Ok(Self((**x).clone())),
            x @ AngleExpr::Opaque(_) => Ok(Self(AngleExpr::Negative(Arc::new(x.clone())))),
        }
    }
    /// Checked exact addition. Opaque radians do not gain algebraic privileges.
    /// # Errors
    /// Rejects opaque operands, foreign owners, or exhausted exact budgets.
    pub fn added(&self, other: &Self) -> Result<Self> {
        match (&self.0, &other.0) {
            (AngleExpr::Symbolic(a), AngleExpr::Symbolic(b)) => {
                Ok(Self(AngleExpr::Symbolic(SymbolicAngle::new(a.add(b)?))))
            }
            _ => Err(Error::Unsupported("opaque angle addition")),
        }
    }
    /// Checked exact rational scaling.
    /// # Errors
    /// Rejects opaque operands, zero denominators, or exhausted budgets.
    pub fn scaled_ratio(&self, numerator: BigInt, denominator: BigInt) -> Result<Self> {
        match &self.0 {
            AngleExpr::Symbolic(value) => Ok(Self(AngleExpr::Symbolic(SymbolicAngle::new(
                value.scale_ratio(numerator, denominator)?,
            )))),
            _ => Err(Error::Unsupported("opaque angle scaling")),
        }
    }
    pub(crate) const fn is_exact(&self) -> bool {
        matches!(&self.0, AngleExpr::Symbolic(_))
    }
    pub(crate) fn rational_pi_identity(&self) -> Option<BigRational> {
        if let AngleExpr::Symbolic(value) = &self.0 {
            value.pi_identity()
        } else {
            None
        }
    }
    pub(crate) fn safe_inverse(&self) -> bool {
        match &self.0 {
            AngleExpr::Symbolic(value) if value.signed_parameter_leaf().is_some() => true,
            AngleExpr::Symbolic(value) if !value.has_source_parameters() => {
                self.evaluate(&BTreeMap::new()).is_ok()
            }
            _ => false,
        }
    }
    pub(crate) fn plus_exact(&self, other: &Self) -> Option<Self> {
        let (AngleExpr::Symbolic(a), AngleExpr::Symbolic(b)) = (&self.0, &other.0) else {
            return None;
        };
        if a.has_source_parameters() || b.has_source_parameters() {
            return None;
        }
        self.evaluate(&BTreeMap::new()).ok()?;
        other.evaluate(&BTreeMap::new()).ok()?;
        let sum = self.added(other).ok()?;
        if let AngleExpr::Symbolic(value) = &sum.0 {
            let finite =
                crate::rational::to_affine_f64(value.constant(), value.pi_coefficient()).ok()?;
            let _ = value.finite_constant.set(finite);
        }
        Some(sum)
    }
    pub(crate) fn is_zero(&self) -> bool {
        matches!(&self.0, AngleExpr::Symbolic(value)
            if value.is_zero()
                && !value.has_source_parameters()
                && self.evaluate(&BTreeMap::new()).is_ok())
    }
    pub(crate) fn parameters(&self, output: &mut Vec<ParameterId>) -> Result<()> {
        match &self.0 {
            AngleExpr::Symbolic(value) => {
                for symbol in value.parameters() {
                    output.push(ParameterId {
                        owner: symbol.owner().id(),
                        index: usize::try_from(symbol.index()).map_err(|_| Error::InvalidId)?,
                    });
                }
            }
            AngleExpr::Negative(inner) => Self((**inner).clone()).parameters(output)?,
            AngleExpr::Opaque(_) => {}
        }
        Ok(())
    }
    pub(crate) fn evaluate(&self, bindings: &BTreeMap<ParameterId, f64>) -> Result<f64> {
        self.evaluate_target(bindings).map(|(value, _)| value)
    }
    pub(crate) fn evaluate_target(
        &self,
        bindings: &BTreeMap<ParameterId, f64>,
    ) -> Result<(f64, BoundAngleTarget)> {
        match &self.0 {
            AngleExpr::Opaque(value) => Ok((
                *value,
                BoundAngleTarget::DyadicRadians {
                    bits: value.to_bits(),
                },
            )),
            AngleExpr::Negative(inner) => {
                let (value, target) = Self((**inner).clone()).evaluate_target(bindings)?;
                let value = -value;
                let target = match target {
                    BoundAngleTarget::DyadicRadians { .. } => BoundAngleTarget::DyadicRadians {
                        bits: value.to_bits(),
                    },
                    BoundAngleTarget::RationalPi {
                        numerator,
                        denominator,
                    } => BoundAngleTarget::RationalPi {
                        numerator: std::ops::Neg::neg(numerator),
                        denominator,
                    },
                    BoundAngleTarget::AffinePi {
                        radians_numerator,
                        radians_denominator,
                        pi_numerator,
                        pi_denominator,
                    } => BoundAngleTarget::AffinePi {
                        radians_numerator: std::ops::Neg::neg(radians_numerator),
                        radians_denominator,
                        pi_numerator: std::ops::Neg::neg(pi_numerator),
                        pi_denominator,
                    },
                };
                Ok((value, target))
            }
            AngleExpr::Symbolic(expr) => evaluate_symbolic_target(expr, bindings),
        }
    }
}
fn evaluate_symbolic_target(
    expr: &SymbolicAngle,
    bindings: &BTreeMap<ParameterId, f64>,
) -> Result<(f64, BoundAngleTarget)> {
    if let Some(finite) = expr.finite_constant.get() {
        return Ok((*finite, exact_target_from_expr(expr)));
    }
    if let Some((symbol, negative)) = expr.signed_parameter_leaf() {
        let id = ParameterId {
            owner: symbol.owner().id(),
            index: usize::try_from(symbol.index()).map_err(|_| Error::InvalidId)?,
        };
        let value = *bindings.get(&id).ok_or(Error::Binding)?;
        let value = if negative { -value } else { value };
        return Ok((
            value,
            BoundAngleTarget::DyadicRadians {
                bits: value.to_bits(),
            },
        ));
    }
    if let Some(pi) = expr.pi_identity() {
        let value = crate::rational::to_pi_f64(&pi).map_err(map_pi_conversion)?;
        let _ = expr.finite_constant.set(value);
        return Ok((
            value,
            BoundAngleTarget::RationalPi {
                numerator: pi.numer().clone(),
                denominator: pi.denom().clone(),
            },
        ));
    }
    let parameters: Vec<_> = expr.parameters().collect();
    let mut exact_bindings = Vec::new();
    exact_bindings
        .try_reserve_exact(parameters.len())
        .map_err(|_| Error::Budget("angle bindings"))?;
    for symbol in parameters {
        let id = ParameterId {
            owner: symbol.owner().id(),
            index: usize::try_from(symbol.index()).map_err(|_| Error::InvalidId)?,
        };
        let value = bindings.get(&id).ok_or(Error::Binding)?;
        exact_bindings.push((
            symbol,
            crate::rational::dyadic_from_bits(value.to_bits()).ok_or(Error::NonFinite)?,
        ));
    }
    let (radians, pi) = expr.bind_checked(&exact_bindings, |radians, pi| {
        crate::rational::to_affine_f64(radians, pi)
            .map(|_| ())
            .map_err(map_pi_conversion)
    })?;
    let value = crate::rational::to_affine_f64(&radians, &pi).map_err(map_pi_conversion)?;
    if exact_bindings.is_empty() {
        let _ = expr.finite_constant.set(value);
    }
    Ok((
        value,
        BoundAngleTarget::AffinePi {
            radians_numerator: radians.numer().clone(),
            radians_denominator: radians.denom().clone(),
            pi_numerator: pi.numer().clone(),
            pi_denominator: pi.denom().clone(),
        },
    ))
}
fn exact_target_from_expr(expr: &Expr) -> BoundAngleTarget {
    expr.pi_identity().map_or_else(
        || BoundAngleTarget::AffinePi {
            radians_numerator: expr.constant().numer().clone(),
            radians_denominator: expr.constant().denom().clone(),
            pi_numerator: expr.pi_coefficient().numer().clone(),
            pi_denominator: expr.pi_coefficient().denom().clone(),
        },
        |pi| BoundAngleTarget::RationalPi {
            numerator: pi.numer().clone(),
            denominator: pi.denom().clone(),
        },
    )
}
const fn map_pi_conversion(error: crate::rational::PiConversionError) -> Error {
    match error {
        crate::rational::PiConversionError::NonFinite => Error::NonFinite,
        crate::rational::PiConversionError::Precision => Error::Budget("rational pi precision"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ControlState {
    Zero,
    One,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Control {
    qubit: QubitId,
    state: ControlState,
}
impl Control {
    #[must_use]
    pub const fn new(qubit: QubitId, state: ControlState) -> Self {
        Self { qubit, state }
    }
    #[must_use]
    pub const fn qubit(self) -> QubitId {
        self.qubit
    }
    #[must_use]
    pub const fn state(self) -> ControlState {
        self.state
    }
}

/// Built-in gates use the `OpenQASM` 3.1 phase convention.
#[derive(Debug, Clone, PartialEq)]
pub enum Gate {
    Id,
    X,
    Y,
    Z,
    H,
    S,
    Sdg,
    T,
    Tdg,
    Sx,
    Sxdg,
    Swap,
    Rx(Angle),
    Ry(Angle),
    Rz(Angle),
    Phase(Angle),
    U {
        theta: Angle,
        phi: Angle,
        lambda: Angle,
    },
}

impl Gate {
    /// Lift a bound gate without assigning exact symbolic identities to radians.
    /// # Errors
    /// Rejects nonfinite bound angle values.
    pub fn from_bound(gate: &BoundGate) -> Result<Self> {
        Ok(match *gate {
            BoundGate::Id => Self::Id,
            BoundGate::X => Self::X,
            BoundGate::Y => Self::Y,
            BoundGate::Z => Self::Z,
            BoundGate::H => Self::H,
            BoundGate::S => Self::S,
            BoundGate::Sdg => Self::Sdg,
            BoundGate::T => Self::T,
            BoundGate::Tdg => Self::Tdg,
            BoundGate::Sx => Self::Sx,
            BoundGate::Sxdg => Self::Sxdg,
            BoundGate::Swap => Self::Swap,
            BoundGate::Rx(value) => Self::Rx(Angle::radians(value)?),
            BoundGate::Ry(value) => Self::Ry(Angle::radians(value)?),
            BoundGate::Rz(value) => Self::Rz(Angle::radians(value)?),
            BoundGate::Phase(value) => Self::Phase(Angle::radians(value)?),
            BoundGate::U { theta, phi, lambda } => Self::U {
                theta: Angle::radians(theta)?,
                phi: Angle::radians(phi)?,
                lambda: Angle::radians(lambda)?,
            },
        })
    }
    pub(crate) fn equivalent_checked(&self, other: &Self) -> Result<bool> {
        match (self, other) {
            (Self::Rx(a), Self::Rx(b))
            | (Self::Ry(a), Self::Ry(b))
            | (Self::Rz(a), Self::Rz(b))
            | (Self::Phase(a), Self::Phase(b)) => a.equivalent_checked(b),
            (
                Self::U {
                    theta: at,
                    phi: ap,
                    lambda: al,
                },
                Self::U {
                    theta: bt,
                    phi: bp,
                    lambda: bl,
                },
            ) => Ok(at.equivalent_checked(bt)?
                && ap.equivalent_checked(bp)?
                && al.equivalent_checked(bl)?),
            _ => Ok(self == other),
        }
    }
    pub(crate) fn substitute(
        &self,
        bindings: &BTreeMap<ParameterId, Angle>,
        owner: u64,
    ) -> Result<Self> {
        Ok(match self {
            Self::Rx(a) => Self::Rx(a.substitute(bindings, owner)?),
            Self::Ry(a) => Self::Ry(a.substitute(bindings, owner)?),
            Self::Rz(a) => Self::Rz(a.substitute(bindings, owner)?),
            Self::Phase(a) => Self::Phase(a.substitute(bindings, owner)?),
            Self::U { theta, phi, lambda } => Self::U {
                theta: theta.substitute(bindings, owner)?,
                phi: phi.substitute(bindings, owner)?,
                lambda: lambda.substitute(bindings, owner)?,
            },
            x => x.clone(),
        })
    }
    #[must_use]
    pub const fn arity(&self) -> usize {
        self.kind().definition().target_count
    }
    /// Shared semantic registry identity for this circuit adapter.
    #[must_use]
    pub const fn kind(&self) -> quest_language::GateKind {
        match self {
            Self::Id => quest_language::GateKind::Id,
            Self::X => quest_language::GateKind::X,
            Self::Y => quest_language::GateKind::Y,
            Self::Z => quest_language::GateKind::Z,
            Self::H => quest_language::GateKind::H,
            Self::S => quest_language::GateKind::S,
            Self::Sdg => quest_language::GateKind::Sdg,
            Self::T => quest_language::GateKind::T,
            Self::Tdg => quest_language::GateKind::Tdg,
            Self::Sx => quest_language::GateKind::Sx,
            Self::Sxdg => quest_language::GateKind::Sxdg,
            Self::Swap => quest_language::GateKind::Swap,
            Self::Rx(..) => quest_language::GateKind::Rx,
            Self::Ry(..) => quest_language::GateKind::Ry,
            Self::Rz(..) => quest_language::GateKind::Rz,
            Self::Phase(..) => quest_language::GateKind::Phase,
            Self::U { .. } => quest_language::GateKind::U,
        }
    }
    /// # Errors
    /// Rejects exhausted exact angle budgets during negation.
    pub fn adjoint(&self) -> Result<Self> {
        Ok(match self {
            Self::S => Self::Sdg,
            Self::Sdg => Self::S,
            Self::T => Self::Tdg,
            Self::Tdg => Self::T,
            Self::Sx => Self::Sxdg,
            Self::Sxdg => Self::Sx,
            Self::Rx(x) => Self::Rx(x.negated()?),
            Self::Ry(x) => Self::Ry(x.negated()?),
            Self::Rz(x) => Self::Rz(x.negated()?),
            Self::Phase(x) => Self::Phase(x.negated()?),
            Self::U { theta, phi, lambda } => Self::U {
                theta: theta.negated()?,
                phi: lambda.negated()?,
                lambda: phi.negated()?,
            },
            other => other.clone(),
        })
    }
    pub(crate) fn angles(&self) -> impl Iterator<Item = &Angle> + Clone {
        match self {
            Self::Rx(x) | Self::Ry(x) | Self::Rz(x) | Self::Phase(x) => [Some(x), None, None],
            Self::U { theta, phi, lambda } => [Some(theta), Some(phi), Some(lambda)],
            _ => [None, None, None],
        }
        .into_iter()
        .flatten()
    }
    pub(crate) fn bind(
        &self,
        bindings: &BTreeMap<ParameterId, f64>,
    ) -> Result<(BoundGate, Vec<Option<BoundAngleTarget>>)> {
        let mut parameters = [0.0; 3];
        let mut targets = Vec::new();
        for (output, angle) in parameters.iter_mut().zip(self.angles()) {
            let (value, target) = angle.evaluate_target(bindings)?;
            *output = value;
            targets.push(Some(target));
        }
        let parameters = parameters
            .get(..self.kind().definition().parameter_count)
            .ok_or(Error::Unsupported("gate parameter capacity"))?;
        Ok((BoundGate::from_kind(self.kind(), parameters)?, targets))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BoundGate {
    Id,
    X,
    Y,
    Z,
    H,
    S,
    Sdg,
    T,
    Tdg,
    Sx,
    Sxdg,
    Swap,
    Rx(f64),
    Ry(f64),
    Rz(f64),
    Phase(f64),
    U { theta: f64, phi: f64, lambda: f64 },
}

impl BoundGate {
    /// Shared semantic registry identity for this circuit adapter.
    #[must_use]
    pub const fn kind(&self) -> quest_language::GateKind {
        match self {
            Self::Id => quest_language::GateKind::Id,
            Self::X => quest_language::GateKind::X,
            Self::Y => quest_language::GateKind::Y,
            Self::Z => quest_language::GateKind::Z,
            Self::H => quest_language::GateKind::H,
            Self::S => quest_language::GateKind::S,
            Self::Sdg => quest_language::GateKind::Sdg,
            Self::T => quest_language::GateKind::T,
            Self::Tdg => quest_language::GateKind::Tdg,
            Self::Sx => quest_language::GateKind::Sx,
            Self::Sxdg => quest_language::GateKind::Sxdg,
            Self::Swap => quest_language::GateKind::Swap,
            Self::Rx(..) => quest_language::GateKind::Rx,
            Self::Ry(..) => quest_language::GateKind::Ry,
            Self::Rz(..) => quest_language::GateKind::Rz,
            Self::Phase(..) => quest_language::GateKind::Phase,
            Self::U { .. } => quest_language::GateKind::U,
        }
    }
    /// Adapt a registry gate after intrinsic controls have been separated into operands.
    /// Global phase is a scalar operation and must use the caller's scalar dispatch.
    /// # Errors
    /// Rejects missing/extra or nonfinite parameters and scalar global phase.
    pub fn from_kind(kind: quest_language::GateKind, parameters: &[f64]) -> Result<Self> {
        use quest_language::{Decomposition, GateKind as G};
        let expected = kind.definition().parameter_count;
        if parameters.len() != expected {
            return Err(Error::ParameterArity {
                expected,
                actual: parameters.len(),
            });
        }
        if parameters.iter().any(|value| !value.is_finite()) {
            return Err(Error::NonFinite);
        }
        if let Decomposition::Controlled { base, .. } = kind.definition().decomposition {
            return Self::from_kind(base, parameters);
        }
        let parameter = |index| {
            parameters.get(index).copied().ok_or(Error::ParameterArity {
                expected,
                actual: parameters.len(),
            })
        };
        Ok(match kind {
            G::Id => Self::Id,
            G::X => Self::X,
            G::Y => Self::Y,
            G::Z => Self::Z,
            G::H => Self::H,
            G::S => Self::S,
            G::Sdg => Self::Sdg,
            G::T => Self::T,
            G::Tdg => Self::Tdg,
            G::Sx => Self::Sx,
            G::Sxdg => Self::Sxdg,
            G::Swap => Self::Swap,
            G::Rx => Self::Rx(parameter(0)?),
            G::Ry => Self::Ry(parameter(0)?),
            G::Rz => Self::Rz(parameter(0)?),
            G::Phase => Self::Phase(parameter(0)?),
            G::U => Self::U {
                theta: parameter(0)?,
                phi: parameter(1)?,
                lambda: parameter(2)?,
            },
            G::GlobalPhase => {
                return Err(Error::Unsupported("global phase requires scalar dispatch"));
            }
            G::Cx | G::Cy | G::Cz | G::Ccx => {
                return Err(Error::Unsupported("registry control base"));
            }
        })
    }
    /// Parameters in shared-registry order, without allocating a temporary list.
    pub fn parameters(&self) -> impl Iterator<Item = f64> + Clone {
        match self {
            Self::Rx(x) | Self::Ry(x) | Self::Rz(x) | Self::Phase(x) => [Some(*x), None, None],
            Self::U { theta, phi, lambda } => [Some(*theta), Some(*phi), Some(*lambda)],
            _ => [None, None, None],
        }
        .into_iter()
        .flatten()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A source identifier and half-open byte range into frontend-owned text.
///
/// Macro operations use the compiler's display filename (including any path
/// remapping) and the original operation-keyword span. No file contents or
/// expansion stack are stored. Rendering checks the supplied text's boundaries.
pub struct SourceSpan {
    source: Arc<str>,
    start: usize,
    end: usize,
}
impl SourceSpan {
    /// # Errors
    /// Rejects an end offset preceding the start offset.
    pub fn new(source: impl Into<Arc<str>>, start: usize, end: usize) -> Result<Self> {
        if end < start {
            return Err(Error::SourceRange);
        }
        Ok(Self {
            source: source.into(),
            start,
            end,
        })
    }
    /// Source identifier; text is supplied by the frontend's source resolver.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
    #[must_use]
    pub const fn range(&self) -> std::ops::Range<usize> {
        self.start..self.end
    }
}

#[derive(Debug, Clone)]
#[expect(
    clippy::redundant_pub_crate,
    reason = "Keep internal state out of the crate wildcard public re-export"
)]
pub(crate) enum SemanticOperation {
    Gate {
        gate: Gate,
        targets: Arc<[QubitId]>,
        controls: Arc<[Control]>,
    },
    GlobalPhase {
        angle: Angle,
        controls: Arc<[Control]>,
    },
    Numerical {
        matrix: NumericalOperator,
        targets: Arc<[QubitId]>,
        controls: Arc<[Control]>,
    },
    Oracle {
        fragment: crate::OracleFragment,
        targets: Arc<[QubitId]>,
        controls: Arc<[Control]>,
    },
    Measure {
        qubit: QubitId,
        bit: BitId,
    },
    Reset {
        qubit: QubitId,
    },
    Barrier {
        qubits: Arc<[QubitId]>,
    },
    Channel {
        kraus: Arc<[NumericalOperator]>,
        targets: Arc<[QubitId]>,
    },
    Conditional {
        bit: BitId,
        expected: bool,
        operation: Box<Self>,
    },
}

#[derive(Debug, Clone)]
pub enum Operation {
    Gate {
        gate: BoundGate,
        targets: Arc<[QubitId]>,
        controls: Arc<[Control]>,
    },
    GlobalPhase {
        radians: f64,
        controls: Arc<[Control]>,
    },
    Numerical {
        matrix: NumericalOperator,
        targets: Arc<[QubitId]>,
        controls: Arc<[Control]>,
    },
    Oracle {
        fragment: crate::OracleFragment,
        targets: Arc<[QubitId]>,
        controls: Arc<[Control]>,
    },
    Measure {
        qubit: QubitId,
        bit: BitId,
    },
    Reset {
        qubit: QubitId,
    },
    Barrier {
        qubits: Arc<[QubitId]>,
    },
    Channel {
        kraus: Arc<[NumericalOperator]>,
        targets: Arc<[QubitId]>,
    },
    Conditional {
        bit: BitId,
        expected: bool,
        operation: Box<Self>,
    },
}

/// Ordered borrowed operands. A conditional exposes its quantum body's operands.
/// Classical read/write dependencies remain separate from this view.
#[derive(Debug, Clone, Copy)]
pub struct Operands<'a> {
    targets: &'a [QubitId],
    controls: &'a [Control],
}
impl<'a> Operands<'a> {
    #[must_use]
    pub const fn targets(self) -> &'a [QubitId] {
        self.targets
    }
    #[must_use]
    pub const fn controls(self) -> &'a [Control] {
        self.controls
    }
    pub fn qubits(self) -> impl Iterator<Item = QubitId> + Clone + 'a {
        self.targets
            .iter()
            .copied()
            .chain(self.controls.iter().map(|control| control.qubit()))
    }
}
macro_rules! operand_view {
    ($operation:expr) => {
        match $operation {
            Self::Gate {
                targets, controls, ..
            }
            | Self::Numerical {
                targets, controls, ..
            }
            | Self::Oracle {
                targets, controls, ..
            } => Operands { targets, controls },
            Self::GlobalPhase { controls, .. } => Operands {
                targets: &[],
                controls,
            },
            Self::Measure { qubit, .. } | Self::Reset { qubit } => Operands {
                targets: std::slice::from_ref(qubit),
                controls: &[],
            },
            Self::Barrier { qubits } => Operands {
                targets: qubits,
                controls: &[],
            },
            Self::Channel { targets, .. } => Operands {
                targets,
                controls: &[],
            },
            Self::Conditional { operation, .. } => operation.operands(),
        }
    };
}
impl Operation {
    /// Retained operand payloads and Arc headers, counted per occurrence even when shared.
    /// Allocator bookkeeping is excluded. Conditional body inline storage is included.
    /// # Errors
    /// Rejects storage arithmetic overflow.
    pub fn operand_storage_bytes(&self) -> Result<usize> {
        fn bank<T>(values: &[T]) -> Result<usize> {
            values
                .len()
                .checked_mul(size_of::<T>())
                .and_then(|bytes| bytes.checked_add(const { 2 * size_of::<usize>() }))
                .and_then(|bytes| bytes.checked_add(align_of::<T>()))
                .ok_or(Error::Budget("operand storage"))
        }
        let bytes = match self {
            Self::Gate {
                targets, controls, ..
            }
            | Self::Numerical {
                targets, controls, ..
            }
            | Self::Oracle {
                targets, controls, ..
            } => bank(targets.as_ref())?.checked_add(bank(controls.as_ref())?),
            Self::GlobalPhase { controls, .. } => Some(bank(controls.as_ref())?),
            Self::Barrier { qubits } => Some(bank(qubits.as_ref())?),
            Self::Channel { targets, kraus } => {
                bank(targets.as_ref())?.checked_add(bank(kraus.as_ref())?)
            }
            Self::Conditional { operation, .. } => operation
                .operand_storage_bytes()?
                .checked_add(size_of::<Self>()),
            Self::Measure { .. } | Self::Reset { .. } => Some(0),
        };
        bytes.ok_or(Error::Budget("operand storage"))
    }
    #[must_use]
    pub fn operands(&self) -> Operands<'_> {
        operand_view!(self)
    }
    pub fn qubits(&self) -> impl Iterator<Item = QubitId> + Clone + '_ {
        self.operands().qubits()
    }
}
#[expect(
    clippy::redundant_pub_crate,
    reason = "Keep internal mapping out of the wildcard public re-export"
)]
pub(crate) struct MappedOperands {
    pub(crate) targets: Arc<[QubitId]>,
    pub(crate) controls: Arc<[Control]>,
}
impl MappedOperands {
    pub(crate) fn scope(self) -> Arc<[QubitId]> {
        self.targets
            .iter()
            .copied()
            .chain(self.controls.iter().map(|c| c.qubit()))
            .collect()
    }
}
#[expect(
    clippy::redundant_pub_crate,
    reason = "Keep internal mapping out of the wildcard public re-export"
)]
pub(crate) fn remap_operands(
    local: Operands<'_>,
    mapping: &[QubitId],
    outer: &[Control],
) -> Result<MappedOperands> {
    let map = |qubit: QubitId| mapping.get(qubit.index()).copied().ok_or(Error::InvalidId);
    Ok(MappedOperands {
        targets: local
            .targets
            .iter()
            .copied()
            .map(map)
            .collect::<Result<_>>()?,
        controls: local
            .controls
            .iter()
            .map(|c| Ok(Control::new(map(c.qubit())?, c.state())))
            .chain(outer.iter().copied().map(Ok))
            .collect::<Result<_>>()?,
    })
}

impl SemanticOperation {
    pub(crate) fn equivalence_work_estimate(&self) -> Result<u64> {
        match self {
            Self::Gate { gate, .. } => gate.angles().try_fold(1u64, |total, angle| {
                total
                    .checked_add(angle.equivalence_work_estimate()?)
                    .ok_or(Error::Budget("operation equivalence work"))
            }),
            Self::GlobalPhase { angle, .. } => angle.equivalence_work_estimate(),
            _ => Ok(1),
        }
    }
    pub(crate) fn binding_work_estimate(&self) -> Result<u64> {
        let work = match self {
            Self::Gate { gate, .. } => gate.angles().try_fold(1u64, |total, angle| {
                total
                    .checked_add(angle.binding_work_estimate()?)
                    .ok_or(Error::Budget("angle binding work"))
            })?,
            Self::GlobalPhase { angle, .. } => angle.binding_work_estimate()?,
            Self::Conditional { operation, .. } => operation.binding_work_estimate()?,
            Self::Numerical { .. }
            | Self::Oracle { .. }
            | Self::Measure { .. }
            | Self::Reset { .. }
            | Self::Barrier { .. }
            | Self::Channel { .. } => 1,
        };
        Ok(work)
    }
    pub(crate) fn retained_bytes(&self) -> Result<usize> {
        let operands = self.operands();
        let array = |count: usize, element: usize| {
            count
                .checked_mul(element)
                .and_then(|bytes| bytes.checked_add(const { 3 * std::mem::size_of::<usize>() }))
                .ok_or(Error::Budget("ideal operand storage"))
        };
        let mut bytes = array(operands.targets().len(), std::mem::size_of::<QubitId>())?
            .checked_add(array(
                operands.controls().len(),
                std::mem::size_of::<Control>(),
            )?)
            .ok_or(Error::Budget("ideal operand storage"))?;
        let extra = match self {
            Self::Gate { gate, .. } => gate.angles().try_fold(0usize, |total, angle| {
                total
                    .checked_add(angle.retained_bytes()?)
                    .ok_or(Error::Budget("ideal angle storage"))
            })?,
            Self::GlobalPhase { angle, .. } => angle.retained_bytes()?,
            Self::Numerical { matrix, .. } => matrix.bytes(),
            Self::Oracle { fragment, .. } => {
                crate::OracleFragment::shared_storage_bytes([fragment])?
            }
            Self::Channel { kraus, .. } => kraus.iter().try_fold(
                array(kraus.len(), std::mem::size_of::<NumericalOperator>())?,
                |total, matrix| {
                    total
                        .checked_add(matrix.bytes())
                        .ok_or(Error::Budget("ideal channel storage"))
                },
            )?,
            Self::Conditional { operation, .. } => std::mem::size_of::<Self>()
                .checked_add(operation.retained_bytes()?)
                .ok_or(Error::Budget("ideal conditional storage"))?,
            Self::Measure { .. } | Self::Reset { .. } | Self::Barrier { .. } => 0,
        };
        bytes = bytes
            .checked_add(extra)
            .ok_or(Error::Budget("ideal storage"))?;
        Ok(bytes)
    }
    pub(crate) fn operands(&self) -> Operands<'_> {
        operand_view!(self)
    }
    pub(crate) fn qubits(&self) -> impl Iterator<Item = QubitId> + Clone + '_ {
        self.operands().qubits()
    }
    pub(crate) const fn stochastic(&self) -> bool {
        matches!(
            self,
            Self::Measure { .. } | Self::Reset { .. } | Self::Channel { .. }
        )
    }
    pub(crate) const fn exact_unitary(&self) -> bool {
        matches!(
            self,
            Self::Gate { .. } | Self::GlobalPhase { .. } | Self::Barrier { .. }
        )
    }
    pub(crate) fn bind(
        &self,
        b: &BTreeMap<ParameterId, f64>,
    ) -> Result<(Operation, Vec<Option<BoundAngleTarget>>)> {
        Ok(match self {
            Self::Gate {
                gate,
                targets,
                controls,
            } => {
                let (gate, identities) = gate.bind(b)?;
                (
                    Operation::Gate {
                        gate,
                        targets: targets.clone(),
                        controls: controls.clone(),
                    },
                    identities,
                )
            }
            Self::GlobalPhase { angle, controls } => {
                let (radians, target) = angle.evaluate_target(b)?;
                (
                    Operation::GlobalPhase {
                        radians,
                        controls: controls.clone(),
                    },
                    vec![Some(target)],
                )
            }
            Self::Numerical {
                matrix,
                targets,
                controls,
            } => (
                Operation::Numerical {
                    matrix: matrix.clone(),
                    targets: targets.clone(),
                    controls: controls.clone(),
                },
                vec![],
            ),
            Self::Oracle {
                fragment,
                targets,
                controls,
            } => (
                Operation::Oracle {
                    fragment: fragment.clone(),
                    targets: targets.clone(),
                    controls: controls.clone(),
                },
                vec![],
            ),
            Self::Measure { qubit, bit } => (
                Operation::Measure {
                    qubit: *qubit,
                    bit: *bit,
                },
                vec![],
            ),
            Self::Reset { qubit } => (Operation::Reset { qubit: *qubit }, vec![]),
            Self::Barrier { qubits } => (
                Operation::Barrier {
                    qubits: qubits.clone(),
                },
                vec![],
            ),
            Self::Channel { kraus, targets } => (
                Operation::Channel {
                    kraus: kraus.clone(),
                    targets: targets.clone(),
                },
                vec![],
            ),
            Self::Conditional {
                bit,
                expected,
                operation,
            } => {
                let (body, identities) = operation.bind(b)?;
                (
                    Operation::Conditional {
                        bit: *bit,
                        expected: *expected,
                        operation: Box::new(body),
                    },
                    identities,
                )
            }
        })
    }
}

#[derive(Debug, Clone)]
#[expect(
    clippy::redundant_pub_crate,
    reason = "Keep internal state out of the crate wildcard public re-export"
)]
pub(crate) struct Occurrence {
    pub(crate) id: OccurrenceId,
    pub(crate) provenance: crate::ProvenanceId,
    pub(crate) source: Option<SourceSpan>,
    pub(crate) operation: SemanticOperation,
}

#[derive(Debug, Clone)]
pub struct Instruction {
    pub(crate) id: OccurrenceId,
    pub(crate) provenance: crate::ProvenanceId,
    pub(crate) source: Option<SourceSpan>,
    pub(crate) operation: Operation,
    pub(crate) angle_targets: Arc<[Option<BoundAngleTarget>]>,
}
impl Instruction {
    #[must_use]
    pub const fn id(&self) -> OccurrenceId {
        self.id
    }
    #[must_use]
    pub const fn provenance(&self) -> crate::ProvenanceId {
        self.provenance
    }
    #[must_use]
    pub const fn source(&self) -> Option<&SourceSpan> {
        self.source.as_ref()
    }
    #[must_use]
    pub const fn operation(&self) -> &Operation {
        &self.operation
    }
    /// Exact source target identities for this instruction's angle parameters.
    #[must_use]
    pub fn angle_targets(&self) -> &[Option<BoundAngleTarget>] {
        &self.angle_targets
    }
    /// Conservative retained storage for exact angle-target sidecars.
    /// # Errors
    /// Rejects size arithmetic overflow.
    pub fn angle_target_storage_bytes(&self) -> Result<usize> {
        fn coefficient(value: &BigInt) -> Result<usize> {
            let limbs = value.bits().div_ceil(64);
            let bytes = usize::try_from(limbs)
                .map_err(|_| Error::Budget("angle target storage"))?
                .checked_mul(8)
                .and_then(|x| x.checked_mul(2))
                .and_then(|x| x.checked_add(const { 3 * std::mem::size_of::<usize>() }))
                .ok_or(Error::Budget("angle target storage"))?;
            Ok(bytes)
        }
        let mut bytes = self
            .angle_targets
            .len()
            .checked_mul(std::mem::size_of::<Option<BoundAngleTarget>>())
            .and_then(|x| x.checked_add(const { 3 * std::mem::size_of::<usize>() }))
            .ok_or(Error::Budget("angle target storage"))?;
        for target in self.angle_targets.iter().flatten() {
            let coefficients: &[&BigInt] = match target {
                BoundAngleTarget::DyadicRadians { .. } => &[],
                BoundAngleTarget::RationalPi {
                    numerator,
                    denominator,
                } => &[numerator, denominator],
                BoundAngleTarget::AffinePi {
                    radians_numerator,
                    radians_denominator,
                    pi_numerator,
                    pi_denominator,
                } => &[
                    radians_numerator,
                    radians_denominator,
                    pi_numerator,
                    pi_denominator,
                ],
            };
            for value in coefficients {
                bytes = bytes
                    .checked_add(coefficient(value)?)
                    .ok_or(Error::Budget("angle target storage"))?;
            }
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod rational_pi_binding_tests {
    use super::{Angle, BigInt, BigRational};
    use googletest::prelude::*;
    #[gtest]
    fn rational_pi_binding_rounds_only_after_multiplying_by_pi() -> googletest::Result<()> {
        let tiny = BigRational::new(
            BigInt::from(1),
            std::ops::Shl::shl(BigInt::from(1), 1075usize),
        );
        let angle = Angle::rational_pi(tiny)?;
        expect_eq!(
            angle
                .evaluate(&std::collections::BTreeMap::new())?
                .to_bits(),
            2
        );
        Ok(())
    }
}
