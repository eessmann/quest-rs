//! Lossless finite-region admission into the common structured execution pipeline.
use crate::{
    Angle, BigRational, BoundAngleTarget, BoundRegion, Constructed, Control, ControlState,
    LanguageError, Operation, ParameterId, Program, QuantumPayload, QuantumRegion, QubitId,
};
#[allow(unused_imports)]
use crate::{
    BoundParityPasses, ExactPasses, LinearPasses, NumericalPasses, OracleExport, ParityPasses,
    TerminalPasses,
};
use quest_language::{
    SourceMap,
    classical::{FloatWidth, ScalarValue},
    semantic::{self, CompileLimits},
    syntax::{self, Expression, ExpressionKind as E, Statement, StatementKind as S},
};
use std::collections::BTreeMap;
const fn expr(kind: E) -> Expression {
    Expression { kind, span: None }
}
const fn stmt(kind: S) -> Statement {
    Statement { kind, span: None }
}
fn qubit(id: QubitId) -> Expression {
    expr(E::Name(format!("q{}", id.index())))
}
fn bit(index: usize) -> Expression {
    expr(E::Name(format!("c{index}")))
}
pub struct Import {
    pub(crate) captures: Vec<ScalarValue>,
    pub(crate) exact: BTreeMap<usize, Angle>,
    pub(crate) oracles: BTreeMap<usize, crate::OracleFragment>,
    pub(crate) payloads: BTreeMap<usize, QuantumPayload>,
    pub(crate) declarations: Vec<Statement>,
}
impl Import {
    fn argument(
        &mut self,
        value: f64,
        target: Option<&BoundAngleTarget>,
    ) -> Result<Expression, LanguageError> {
        let index = self.captures.len();
        self.captures
            .push(ScalarValue::floating(FloatWidth::F64, value)?);
        let angle = match target {
            Some(BoundAngleTarget::RationalPi {
                numerator,
                denominator,
            }) => Some(Angle::rational_pi(BigRational::new(
                numerator.clone(),
                denominator.clone(),
            ))?),
            Some(BoundAngleTarget::AffinePi {
                radians_numerator,
                radians_denominator,
                pi_numerator,
                pi_denominator,
            }) => Some(Angle::affine(
                BigRational::new(radians_numerator.clone(), radians_denominator.clone()),
                BigRational::new(pi_numerator.clone(), pi_denominator.clone()),
            )?),
            _ => None,
        };
        if let Some(angle) = angle {
            self.exact.insert(index, angle);
        }
        Ok(expr(E::Capture(index)))
    }
    fn gate(
        name: &str,
        arguments: Vec<Expression>,
        targets: &[QubitId],
        controls: &[Control],
    ) -> Statement {
        stmt(S::Gate {
            name: name.into(),
            arguments,
            operands: controls
                .iter()
                .map(|c| qubit(c.qubit()))
                .chain(targets.iter().copied().map(qubit))
                .collect(),
            modifiers: controls
                .iter()
                .map(|c| syntax::Modifier::Control {
                    positive: c.state() == ControlState::One,
                    count: None,
                })
                .collect(),
        })
    }
    pub(crate) fn operation(
        &mut self,
        operation: &Operation,
        angles: &[Option<BoundAngleTarget>],
    ) -> Result<Statement, LanguageError> {
        Ok(match operation {
            Operation::Gate {
                gate,
                targets,
                controls,
            } => {
                let arguments = gate
                    .parameters()
                    .enumerate()
                    .map(|(i, value)| self.argument(value, angles.get(i).and_then(Option::as_ref)))
                    .collect::<Result<Vec<_>, _>>()?;
                Self::gate(gate.kind().definition().name, arguments, targets, controls)
            }
            Operation::GlobalPhase { radians, controls } => {
                let argument = self.argument(*radians, angles.first().and_then(Option::as_ref))?;
                Self::gate("gphase", vec![argument], &[], controls)
            }
            Operation::Measure { qubit: q, bit: b } => stmt(S::Assign {
                target: bit(b.index()),
                operator: None,
                value: expr(E::Measure(Box::new(qubit(*q)))),
            }),
            Operation::Reset { qubit: q } => stmt(S::Reset(qubit(*q))),
            Operation::Barrier { qubits } => {
                stmt(S::Barrier(qubits.iter().copied().map(qubit).collect()))
            }
            Operation::Conditional {
                bit: b,
                expected,
                operation,
            } => stmt(S::If {
                condition: expr(E::Binary(
                    syntax::BinaryOperator::Equal,
                    Box::new(bit(b.index())),
                    Box::new(expr(E::BitString(if *expected { "1" } else { "0" }.into()))),
                )),
                then_body: vec![self.operation(operation, angles)?],
                else_body: Vec::new(),
            }),
            Operation::Oracle {
                fragment,
                targets,
                controls,
            } => {
                let capture = self.captures.len();
                self.captures
                    .push(ScalarValue::floating(FloatWidth::F64, 0.0)?);
                self.oracles.insert(capture, fragment.clone());
                let name = format!("oracle_{capture}");
                self.declarations.push(stmt(S::Oracle {
                    name: name.clone(),
                    arity: expr(E::Number(fragment.num_qubits().to_string())),
                    capture,
                }));
                Self::gate(&name, Vec::new(), targets, controls)
            }
            Operation::Numerical {
                matrix,
                targets,
                controls,
            } => {
                let capture = self.payloads.len();
                self.payloads.insert(
                    capture,
                    QuantumPayload::Matrix {
                        matrix: matrix.clone(),
                        control_states: controls
                            .iter()
                            .map(|c| c.state() == ControlState::One)
                            .collect(),
                    },
                );
                stmt(S::Payload {
                    capture,
                    operands: controls
                        .iter()
                        .map(|c| qubit(c.qubit()))
                        .chain(targets.iter().copied().map(qubit))
                        .collect(),
                })
            }
            Operation::Channel { kraus, targets } => {
                let capture = self.payloads.len();
                self.payloads.insert(
                    capture,
                    QuantumPayload::Channel {
                        kraus: kraus.clone(),
                    },
                );
                stmt(S::Payload {
                    capture,
                    operands: targets.iter().copied().map(qubit).collect(),
                })
            }
        })
    }
}
impl Program<Constructed> {
    /// Admit a finite capability into the common executable program lifecycle.
    /// Original source binding obligations are discharged before translating any operation.
    /// # Errors
    /// Rejects incomplete/nonfinite bindings, invalid captures, and shared IR admission failures.
    pub fn from_region(
        region: QuantumRegion,
        bindings: &[(ParameterId, f64)],
    ) -> Result<Self, LanguageError> {
        Self::from_bound_region(region.bind(bindings)?)
    }
    /// Admit an already specialized finite capability while preserving exact target metadata.
    /// # Errors
    /// Rejects shared IR admission or payload interface failures.
    pub fn from_bound_region(region: BoundRegion) -> Result<Self, LanguageError> {
        let mut import = Import {
            captures: Vec::new(),
            exact: BTreeMap::new(),
            oracles: BTreeMap::new(),
            payloads: BTreeMap::new(),
            declarations: Vec::new(),
        };
        for q in 0..region.num_qubits() {
            import.declarations.push(stmt(S::Qubit {
                name: format!("q{q}"),
                size: None,
            }));
        }
        for c in 0..region.num_bits() {
            import.declarations.push(stmt(S::Declare {
                name: format!("c{c}"),
                ty: syntax::Type::Scalar(syntax::ScalarKind::Bit, None),
                initializer: Some(expr(E::BitString("0".into()))),
                qualifier: syntax::Qualifier::Output,
            }));
        }
        let body = region
            .instructions()
            .iter()
            .map(|instruction| {
                import.operation(instruction.operation(), instruction.angle_targets())
            })
            .collect::<Result<Vec<_>, _>>()?;
        import.declarations.extend(body);
        let typed = semantic::admit(
            syntax::Module {
                statements: import.declarations,
            },
            CompileLimits::default(),
        )?;
        Self::from_template(typed, import.captures, SourceMap::default(), Vec::new())
            .with_angle_captures(import.exact)
            .with_quantum_payloads(import.payloads)
            .with_oracles(import.oracles)
            .map(|program| program.with_origin(region))
    }
}
