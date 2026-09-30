//! Exact affine angles, source obligations, and opaque finite floating angles.
use crate::rational::BigRational;
use num_bigint::BigInt;
use num_traits::Zero;
use quest_symbolic::{Expr, Owner, Symbol};
use std::{
    collections::BTreeMap,
    ops::Deref,
    sync::{Arc, OnceLock},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParameterId {
    #[doc(hidden)]
    pub owner: u64,
    #[doc(hidden)]
    pub index: usize,
}
impl ParameterId {
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("identifier does not belong to this program, or is out of bounds")]
    InvalidId,
    #[error("invalid finite numerical value")]
    NonFinite,
    #[error("rational angle denominator must be nonzero")]
    ZeroDenominator,
    #[error("exact symbolic angle rejected: {0}")]
    Symbolic(#[from] quest_symbolic::Error),
    #[error("resource budget exceeded: {0}")]
    Budget(&'static str),
    #[error("parameter bindings must be complete, unique, finite, and program-owned")]
    Binding,
    #[error("unsupported capability: {0}")]
    Unsupported(&'static str),
}
pub type Result<T> = std::result::Result<T, Error>;
/// A finite floating angle remains opaque to exact rewrite rules.
#[derive(Debug, Clone, PartialEq)]
pub struct Angle(AngleExpr);

#[derive(Debug)]
pub struct SymbolicAngle {
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
pub enum AngleExpr {
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
    /// Independently replay a parameter-free exact source before inspecting its coefficients.
    #[must_use]
    pub fn independent_constant_summary(&self) -> Option<(&BigRational, &BigRational)> {
        if let AngleExpr::Symbolic(expr) = &self.0 {
            expr.independent_constant_summary()
        } else {
            None
        }
    }

    /// # Errors
    /// Rejects foreign or missing parameters, nonfinite arithmetic, and exhausted exact-arithmetic budgets.
    pub fn binding_work_estimate(&self) -> Result<u64> {
        match &self.0 {
            AngleExpr::Symbolic(value) => value.binding_work_estimate().map_err(Error::from),
            AngleExpr::Opaque(_) => Ok(1),
            AngleExpr::Negative(inner) => Self((**inner).clone())
                .binding_work_estimate()?
                .checked_add(1)
                .ok_or(Error::Budget("angle binding work")),
        }
    }
    /// # Errors
    /// Rejects foreign or missing parameters, nonfinite arithmetic, and exhausted exact-arithmetic budgets.
    pub fn retained_bytes(&self) -> Result<usize> {
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
    /// # Errors
    /// Rejects foreign or missing parameters, nonfinite arithmetic, and exhausted exact-arithmetic budgets.
    pub fn equivalent_checked(&self, other: &Self) -> Result<bool> {
        match (&self.0, &other.0) {
            (AngleExpr::Symbolic(a), AngleExpr::Symbolic(b)) => Ok(a.equivalent_checked(b)?),
            _ => Ok(self == other),
        }
    }
    /// # Errors
    /// Rejects foreign or missing parameters, nonfinite arithmetic, and exhausted exact-arithmetic budgets.
    pub fn equivalence_work_estimate(&self) -> Result<u64> {
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
    /// # Errors
    /// Rejects foreign or missing parameters, nonfinite arithmetic, and exhausted exact-arithmetic budgets.
    pub fn substitute(&self, bindings: &BTreeMap<ParameterId, Self>, owner: u64) -> Result<Self> {
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
    #[must_use]
    pub const fn is_exact(&self) -> bool {
        matches!(&self.0, AngleExpr::Symbolic(_))
    }
    #[must_use]
    pub fn rational_pi_identity(&self) -> Option<BigRational> {
        if let AngleExpr::Symbolic(value) = &self.0 {
            value.pi_identity()
        } else {
            None
        }
    }
    #[must_use]
    pub fn safe_inverse(&self) -> bool {
        match &self.0 {
            AngleExpr::Symbolic(value) if value.signed_parameter_leaf().is_some() => true,
            AngleExpr::Symbolic(value) if !value.has_source_parameters() => {
                self.evaluate(&BTreeMap::new()).is_ok()
            }
            _ => false,
        }
    }
    #[must_use]
    pub fn plus_exact(&self, other: &Self) -> Option<Self> {
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
    #[must_use]
    pub fn is_zero(&self) -> bool {
        matches!(&self.0, AngleExpr::Symbolic(value)
            if value.is_zero()
                && !value.has_source_parameters()
                && self.evaluate(&BTreeMap::new()).is_ok())
    }
    /// # Errors
    /// Rejects foreign or missing parameters, nonfinite arithmetic, and exhausted exact-arithmetic budgets.
    pub fn parameters(&self, output: &mut Vec<ParameterId>) -> Result<()> {
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
    /// # Errors
    /// Rejects foreign or missing parameters, nonfinite arithmetic, and exhausted exact-arithmetic budgets.
    pub fn evaluate(&self, bindings: &BTreeMap<ParameterId, f64>) -> Result<f64> {
        self.evaluate_target(bindings).map(|(value, _)| value)
    }
    /// # Errors
    /// Rejects foreign or missing parameters, nonfinite arithmetic, and exhausted exact-arithmetic budgets.
    pub fn evaluate_target(
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
