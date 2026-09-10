use std::{collections::BTreeMap, sync::Arc};

use crate::BigRational;
use num_bigint::BigInt;
use num_traits::Zero;

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

#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::redundant_pub_crate,
    reason = "Keep internal state out of the crate wildcard public re-export"
)]
pub(crate) enum AngleExpr {
    Pi(BigRational),
    Parameter(ParameterId),
    Opaque(f64),
    Negative(Arc<Self>),
}

impl Angle {
    pub(crate) fn substitute(&self, bindings: &BTreeMap<ParameterId, Self>) -> Result<Self> {
        Ok(match &self.0 {
            AngleExpr::Parameter(p) => bindings.get(p).ok_or(Error::Binding)?.clone(),
            AngleExpr::Negative(a) => Self((**a).clone()).substitute(bindings)?.negated(),
            _ => self.clone(),
        })
    }
    /// # Errors
    /// Rejects a zero denominator.
    pub fn pi(numerator: i64, denominator: i64) -> Result<Self> {
        if denominator == 0 {
            return Err(Error::ZeroDenominator);
        }
        Ok(Self(AngleExpr::Pi(BigRational::new(
            numerator.into(),
            denominator.into(),
        ))))
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
        Ok(Self(AngleExpr::Pi(BigRational::new(
            value.numer().clone(),
            value.denom().clone(),
        ))))
    }
    /// # Errors
    /// Rejects nonfinite angle values.
    pub const fn radians(value: f64) -> Result<Self> {
        if !value.is_finite() {
            return Err(Error::NonFinite);
        }
        Ok(Self(AngleExpr::Opaque(value)))
    }
    #[must_use]
    pub const fn parameter(id: ParameterId) -> Self {
        Self(AngleExpr::Parameter(id))
    }
    #[must_use]
    pub fn negated(&self) -> Self {
        match &self.0 {
            AngleExpr::Pi(x) => Self(AngleExpr::Pi(std::ops::Neg::neg(x))),
            AngleExpr::Negative(x) => Self((**x).clone()),
            x => Self(AngleExpr::Negative(Arc::new(x.clone()))),
        }
    }
    pub(crate) fn is_exact(&self) -> bool {
        match &self.0 {
            AngleExpr::Opaque(_) => false,
            AngleExpr::Negative(a) => Self((**a).clone()).is_exact(),
            _ => true,
        }
    }
    pub(crate) fn plus_exact(&self, other: &Self) -> Option<Self> {
        if !self.is_exact() || !other.is_exact() {
            return None;
        }
        if self == &other.negated() {
            return Some(Self(AngleExpr::Pi(BigRational::zero())));
        }
        match (&self.0, &other.0) {
            (AngleExpr::Pi(a), AngleExpr::Pi(b)) => {
                let sum = Self(AngleExpr::Pi(std::ops::Add::add(a, b)));
                // Preserve the finite binding domain. In particular, two
                // individually finite rotations must not merge into overflow.
                sum.evaluate(&BTreeMap::new()).ok()?;
                Some(sum)
            }
            // A sum of symbolic finite values need not remain finite. Keep
            // their evaluation separate; structural inverse pairs above are
            // still safe for every admitted parameter binding.
            _ => None,
        }
    }
    pub(crate) fn is_zero(&self) -> bool {
        matches!(&self.0, AngleExpr::Pi(x) if x.is_zero())
    }
    pub(crate) fn parameters(&self, output: &mut Vec<ParameterId>) {
        fn visit(x: &AngleExpr, out: &mut Vec<ParameterId>) {
            match x {
                AngleExpr::Parameter(p) => out.push(*p),
                AngleExpr::Negative(a) => visit(a, out),
                _ => {}
            }
        }
        visit(&self.0, output);
    }
    pub(crate) fn evaluate(&self, bindings: &BTreeMap<ParameterId, f64>) -> Result<f64> {
        fn eval(x: &AngleExpr, bindings: &BTreeMap<ParameterId, f64>) -> Result<f64> {
            let value = match x {
                AngleExpr::Pi(x) => crate::rational::to_pi_f64(x).map_err(|error| match error {
                    crate::rational::PiConversionError::NonFinite => Error::NonFinite,
                    crate::rational::PiConversionError::Precision => {
                        Error::Budget("rational pi precision")
                    }
                })?,
                AngleExpr::Parameter(p) => *bindings.get(p).ok_or(Error::Binding)?,
                AngleExpr::Opaque(x) => *x,
                AngleExpr::Negative(x) => -eval(x, bindings)?,
            };
            if value.is_finite() {
                Ok(value)
            } else {
                Err(Error::NonFinite)
            }
        }
        eval(&self.0, bindings)
    }
}

impl From<i64> for Angle {
    fn from(value: i64) -> Self {
        Self(AngleExpr::Pi(BigRational::from_integer(BigInt::from(
            value,
        ))))
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
    pub(crate) fn substitute(&self, bindings: &BTreeMap<ParameterId, Angle>) -> Result<Self> {
        Ok(match self {
            Self::Rx(a) => Self::Rx(a.substitute(bindings)?),
            Self::Ry(a) => Self::Ry(a.substitute(bindings)?),
            Self::Rz(a) => Self::Rz(a.substitute(bindings)?),
            Self::Phase(a) => Self::Phase(a.substitute(bindings)?),
            Self::U { theta, phi, lambda } => Self::U {
                theta: theta.substitute(bindings)?,
                phi: phi.substitute(bindings)?,
                lambda: lambda.substitute(bindings)?,
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
    #[must_use]
    pub fn adjoint(&self) -> Self {
        match self {
            Self::S => Self::Sdg,
            Self::Sdg => Self::S,
            Self::T => Self::Tdg,
            Self::Tdg => Self::T,
            Self::Sx => Self::Sxdg,
            Self::Sxdg => Self::Sx,
            Self::Rx(x) => Self::Rx(x.negated()),
            Self::Ry(x) => Self::Ry(x.negated()),
            Self::Rz(x) => Self::Rz(x.negated()),
            Self::Phase(x) => Self::Phase(x.negated()),
            Self::U { theta, phi, lambda } => Self::U {
                theta: theta.negated(),
                phi: lambda.negated(),
                lambda: phi.negated(),
            },
            other => other.clone(),
        }
    }
    pub(crate) fn angles(&self) -> Vec<&Angle> {
        match self {
            Self::Rx(x) | Self::Ry(x) | Self::Rz(x) | Self::Phase(x) => vec![x],
            Self::U { theta, phi, lambda } => vec![theta, phi, lambda],
            _ => vec![],
        }
    }
    pub(crate) fn bind(&self, b: &BTreeMap<ParameterId, f64>) -> Result<BoundGate> {
        Ok(match self {
            Self::Id => BoundGate::Id,
            Self::X => BoundGate::X,
            Self::Y => BoundGate::Y,
            Self::Z => BoundGate::Z,
            Self::H => BoundGate::H,
            Self::S => BoundGate::S,
            Self::Sdg => BoundGate::Sdg,
            Self::T => BoundGate::T,
            Self::Tdg => BoundGate::Tdg,
            Self::Sx => BoundGate::Sx,
            Self::Sxdg => BoundGate::Sxdg,
            Self::Swap => BoundGate::Swap,
            Self::Rx(a) => BoundGate::Rx(a.evaluate(b)?),
            Self::Ry(a) => BoundGate::Ry(a.evaluate(b)?),
            Self::Rz(a) => BoundGate::Rz(a.evaluate(b)?),
            Self::Phase(a) => BoundGate::Phase(a.evaluate(b)?),
            Self::U { theta, phi, lambda } => BoundGate::U {
                theta: theta.evaluate(b)?,
                phi: phi.evaluate(b)?,
                lambda: lambda.evaluate(b)?,
            },
        })
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
        targets: Vec<QubitId>,
        controls: Vec<Control>,
    },
    GlobalPhase {
        angle: Angle,
        controls: Vec<Control>,
    },
    Numerical {
        matrix: NumericalOperator,
        targets: Vec<QubitId>,
        controls: Vec<Control>,
    },
    Measure {
        qubit: QubitId,
        bit: BitId,
    },
    Reset {
        qubit: QubitId,
    },
    Barrier {
        qubits: Vec<QubitId>,
    },
    Channel {
        kraus: Vec<NumericalOperator>,
        targets: Vec<QubitId>,
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
        targets: Vec<QubitId>,
        controls: Vec<Control>,
    },
    GlobalPhase {
        radians: f64,
        controls: Vec<Control>,
    },
    Numerical {
        matrix: NumericalOperator,
        targets: Vec<QubitId>,
        controls: Vec<Control>,
    },
    Measure {
        qubit: QubitId,
        bit: BitId,
    },
    Reset {
        qubit: QubitId,
    },
    Barrier {
        qubits: Vec<QubitId>,
    },
    Channel {
        kraus: Vec<NumericalOperator>,
        targets: Vec<QubitId>,
    },
    Conditional {
        bit: BitId,
        expected: bool,
        operation: Box<Self>,
    },
}

impl SemanticOperation {
    pub(crate) fn qubits(&self) -> Vec<QubitId> {
        match self {
            Self::Gate {
                targets, controls, ..
            }
            | Self::Numerical {
                targets, controls, ..
            } => targets
                .iter()
                .copied()
                .chain(controls.iter().map(|c| c.qubit()))
                .collect(),
            Self::GlobalPhase { controls, .. } => controls.iter().map(|c| c.qubit()).collect(),
            Self::Measure { qubit, .. } | Self::Reset { qubit } => vec![*qubit],
            Self::Barrier { qubits } => qubits.clone(),
            Self::Channel { targets, .. } => targets.clone(),
            Self::Conditional { operation, .. } => operation.qubits(),
        }
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
    pub(crate) fn bind(&self, b: &BTreeMap<ParameterId, f64>) -> Result<Operation> {
        Ok(match self {
            Self::Gate {
                gate,
                targets,
                controls,
            } => Operation::Gate {
                gate: gate.bind(b)?,
                targets: targets.clone(),
                controls: controls.clone(),
            },
            Self::GlobalPhase { angle, controls } => Operation::GlobalPhase {
                radians: angle.evaluate(b)?,
                controls: controls.clone(),
            },
            Self::Numerical {
                matrix,
                targets,
                controls,
            } => Operation::Numerical {
                matrix: matrix.clone(),
                targets: targets.clone(),
                controls: controls.clone(),
            },
            Self::Measure { qubit, bit } => Operation::Measure {
                qubit: *qubit,
                bit: *bit,
            },
            Self::Reset { qubit } => Operation::Reset { qubit: *qubit },
            Self::Barrier { qubits } => Operation::Barrier {
                qubits: qubits.clone(),
            },
            Self::Channel { kraus, targets } => Operation::Channel {
                kraus: kraus.clone(),
                targets: targets.clone(),
            },
            Self::Conditional {
                bit,
                expected,
                operation,
            } => Operation::Conditional {
                bit: *bit,
                expected: *expected,
                operation: Box::new(operation.bind(b)?),
            },
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
    pub(crate) provenance: Vec<OccurrenceId>,
    pub(crate) source: Option<SourceSpan>,
    pub(crate) operation: SemanticOperation,
}

#[derive(Debug, Clone)]
pub struct Instruction {
    pub(crate) id: OccurrenceId,
    pub(crate) provenance: Vec<OccurrenceId>,
    pub(crate) source: Option<SourceSpan>,
    pub(crate) operation: Operation,
}
impl Instruction {
    #[must_use]
    pub const fn id(&self) -> OccurrenceId {
        self.id
    }
    #[must_use]
    pub fn provenance(&self) -> &[OccurrenceId] {
        &self.provenance
    }
    #[must_use]
    pub const fn source(&self) -> Option<&SourceSpan> {
        self.source.as_ref()
    }
    #[must_use]
    pub const fn operation(&self) -> &Operation {
        &self.operation
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
