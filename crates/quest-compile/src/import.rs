//! Lossless finite-region admission through checked semantic operations.
use crate::{
    Angle, BoundAngleTarget, BoundRegion, Constructed, LanguageError, Operation, ParameterId,
    Program, QuantumPayload, QuantumRegion, RBig,
};
use quest_language::{
    classical::{FloatWidth, ScalarValue},
    semantic::finite::FiniteOperation as F,
};
use std::collections::BTreeMap;
pub struct Import {
    pub(crate) captures: Vec<ScalarValue>,
    pub(crate) exact: BTreeMap<usize, Angle>,
    pub(crate) oracles: BTreeMap<usize, crate::OracleFragment>,
    pub(crate) payloads: BTreeMap<usize, QuantumPayload>,
}
impl Import {
    fn argument(
        &mut self,
        value: f64,
        target: Option<&BoundAngleTarget>,
    ) -> Result<usize, LanguageError> {
        let index = self.captures.len();
        self.captures
            .push(ScalarValue::floating(FloatWidth::F64, value)?);
        let angle = match target {
            Some(BoundAngleTarget::RationalPi {
                numerator,
                denominator,
            }) => Some(Angle::rational_pi(RBig::from_parts_signed(
                numerator.clone(),
                denominator.clone(),
            ))?),
            Some(BoundAngleTarget::AffinePi {
                radians_numerator,
                radians_denominator,
                pi_numerator,
                pi_denominator,
            }) => Some(Angle::affine(
                RBig::from_parts_signed(radians_numerator.clone(), radians_denominator.clone()),
                RBig::from_parts_signed(pi_numerator.clone(), pi_denominator.clone()),
            )?),
            _ => None,
        };
        if let Some(angle) = angle {
            self.exact.insert(index, angle);
        }
        Ok(index)
    }
    pub(crate) fn operation(
        &mut self,
        operation: &Operation,
        angles: &[Option<BoundAngleTarget>],
    ) -> Result<F, LanguageError> {
        let controls = |controls: &[crate::Control]| {
            controls
                .iter()
                .map(|c| (c.qubit().index(), c.state() == crate::ControlState::One))
                .collect()
        };
        let targets = |targets: &[crate::QubitId]| targets.iter().map(|q| q.index()).collect();
        Ok(match operation {
            Operation::Gate {
                gate,
                targets: qs,
                controls: cs,
            } => F::Gate {
                gate: gate.kind(),
                arguments: gate
                    .parameters()
                    .enumerate()
                    .map(|(i, value)| self.argument(value, angles.get(i).and_then(Option::as_ref)))
                    .collect::<Result<Vec<_>, _>>()?,
                targets: targets(qs),
                controls: controls(cs),
            },
            Operation::GlobalPhase {
                radians,
                controls: cs,
            } => F::Gate {
                gate: quest_language::GateKind::GlobalPhase,
                arguments: vec![self.argument(*radians, angles.first().and_then(Option::as_ref))?],
                targets: Vec::new(),
                controls: controls(cs),
            },
            Operation::Measure { qubit, bit } => F::Measure {
                qubit: qubit.index(),
                bit: bit.index(),
            },
            Operation::Reset { qubit } => F::Reset(qubit.index()),
            Operation::Barrier { qubits } => F::Barrier(targets(qubits)),
            Operation::Conditional {
                bit,
                expected,
                operation,
            } => F::Conditional {
                bit: bit.index(),
                expected: *expected,
                operation: Box::new(self.operation(operation, angles)?),
            },
            Operation::Oracle {
                fragment,
                targets: qs,
                controls: cs,
            } => {
                let capture = self.oracles.len();
                self.oracles.insert(capture, fragment.clone());
                F::Oracle {
                    capture,
                    targets: targets(qs),
                    controls: controls(cs),
                }
            }
            Operation::Numerical {
                matrix,
                targets: qs,
                controls: cs,
            } => {
                let capture = self.payloads.len();
                self.payloads.insert(
                    capture,
                    QuantumPayload::Matrix {
                        matrix: matrix.clone(),
                        control_states: cs
                            .iter()
                            .map(|c| c.state() == crate::ControlState::One)
                            .collect(),
                    },
                );
                F::Payload {
                    capture,
                    operands: cs
                        .iter()
                        .map(|c| c.qubit().index())
                        .chain(qs.iter().map(|q| q.index()))
                        .collect(),
                }
            }
            Operation::Channel { kraus, targets: qs } => {
                let capture = self.payloads.len();
                self.payloads.insert(
                    capture,
                    QuantumPayload::Channel {
                        kraus: kraus.clone(),
                    },
                );
                F::Payload {
                    capture,
                    operands: targets(qs),
                }
            }
        })
    }
}
impl Program<Constructed> {
    /// Admit a finite capability after discharging its original binding obligations.
    /// # Errors
    /// Rejects incomplete bindings and semantic/SSA admission failures.
    pub fn from_region(
        region: QuantumRegion,
        bindings: &[(ParameterId, f64)],
    ) -> Result<Self, LanguageError> {
        Self::from_bound_region(region.bind(bindings)?)
    }
    /// Admit specialized finite operations without reconstructing source expressions.
    /// # Errors
    /// Rejects incompatible operand interfaces, captures, or SSA effects.
    pub fn from_bound_region(region: BoundRegion) -> Result<Self, LanguageError> {
        let mut builder = crate::ProgramBuilder::new()?;
        let mut qubits = Vec::new();
        for q in 0..region.num_qubits() {
            qubits.push(builder.qubit(&format!("q{q}"), 1)?);
        }
        let mut bits = Vec::new();
        for c in 0..region.num_bits() {
            let zero = builder.bitstring::<1>("0")?;
            bits.push(builder.output(&format!("c{c}"), &zero)?);
        }
        builder.region(region.clone(), &qubits, &bits)?;
        Ok(builder
            .finish()?
            .with_embedded_origins(Vec::new())
            .with_origin(region))
    }
}
