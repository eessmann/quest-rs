//! Immutable coherent fragments. Calls retain their identity through planning.
use crate::{BoundGate, BoundProgram, Control, Error, MatrixPolicy, Operation, QubitId, Result};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Debug)]
struct OracleBody {
    qubits: usize,
    operations: Box<[Operation]>,
    queries: usize,
    depth: usize,
}

/// Shared coherent circuit data with an explicit numerical adjoint view.
///
/// Per-matrix admission is not a unitarity bound for the composed fragment.
/// Numerical data never acquire exact cancellation or inverse privileges.
#[derive(Debug, Clone)]
pub struct OracleFragment {
    body: Arc<OracleBody>,
    adjoint: bool,
}
/// A bound body awaiting explicit numerical matrix admission policy.
#[derive(Debug)]
pub struct NeedsOracleTolerance;
/// A checked finite tolerance; this is configuration, not composed-unitary evidence.
#[derive(Debug)]
pub struct OracleTolerance {
    value: f64,
}
/// Consuming builder for immutable coherent oracle publication.
#[derive(Debug)]
pub struct OracleBuilder<State = NeedsOracleTolerance> {
    program: BoundProgram,
    matrix_policy: MatrixPolicy,
    state: State,
}
impl<State> OracleBuilder<State> {
    #[must_use]
    pub const fn matrix_policy(mut self, policy: MatrixPolicy) -> Self {
        self.matrix_policy = policy;
        self
    }
}
impl OracleBuilder<NeedsOracleTolerance> {
    /// Select the per-matrix admission tolerance explicitly.
    /// # Errors
    /// Rejects negative and nonfinite tolerances.
    pub fn matrix_tolerance(self, tolerance: f64) -> Result<OracleBuilder<OracleTolerance>> {
        if !tolerance.is_finite() || tolerance < 0.0 {
            return Err(Error::NonFinite);
        }
        Ok(OracleBuilder {
            program: self.program,
            matrix_policy: self.matrix_policy,
            state: OracleTolerance { value: tolerance },
        })
    }
}
impl OracleBuilder<OracleTolerance> {
    /// Check every numerical matrix and freeze the coherent body.
    /// # Errors
    /// Rejects effects, failed numerical admissions and exhausted budgets.
    pub fn build(self) -> Result<OracleFragment> {
        OracleFragment::freeze(self.program, self.state.value, self.matrix_policy)
    }
}
impl OracleFragment {
    /// Start consuming construction from an already bound program.
    #[must_use]
    pub fn builder(program: BoundProgram) -> OracleBuilder {
        OracleBuilder {
            program,
            matrix_policy: MatrixPolicy::default(),
            state: NeedsOracleTolerance,
        }
    }
    /// Freeze a bound, effect-free program in its admitted execution order.
    /// `tolerance` applies separately to each numerical matrix, not to the body.
    ///
    /// # Errors
    /// Rejects effects, failed matrix admissions and excessive nesting or counts.
    pub fn from_program(
        program: BoundProgram,
        tolerance: f64,
        policy: MatrixPolicy,
    ) -> Result<Self> {
        Self::builder(program)
            .matrix_policy(policy)
            .matrix_tolerance(tolerance)?
            .build()
    }
    fn freeze(program: BoundProgram, tolerance: f64, policy: MatrixPolicy) -> Result<Self> {
        let mut queries = 1usize;
        let mut depth = 1usize;
        for instruction in program.instructions() {
            match instruction.operation() {
                Operation::Gate { .. }
                | Operation::GlobalPhase { .. }
                | Operation::Barrier { .. } => {}
                Operation::Numerical { matrix, .. } => {
                    matrix.check_unitary(tolerance, policy)?;
                }
                Operation::Oracle { fragment, .. } => {
                    queries = queries
                        .checked_add(fragment.query_count())
                        .ok_or(Error::Budget("oracle queries"))?;
                    depth = depth.max(
                        fragment
                            .body
                            .depth
                            .checked_add(1)
                            .ok_or(Error::Budget("oracle nesting"))?,
                    );
                }
                _ => return Err(Error::NotUnitary),
            }
        }
        if depth > 64 {
            return Err(Error::Budget("oracle nesting"));
        }
        Ok(Self {
            body: Arc::new(OracleBody {
                qubits: program.num_qubits(),
                operations: program
                    .instructions
                    .into_iter()
                    .map(|i| i.operation)
                    .collect(),
                queries,
                depth,
            }),
            adjoint: false,
        })
    }
    #[must_use]
    pub fn num_qubits(&self) -> usize {
        self.body.qubits
    }
    /// Number of calls including this body and its nested oracle occurrences.
    #[must_use]
    pub fn query_count(&self) -> usize {
        self.body.queries
    }
    /// Conservative retained payload bytes; shared bodies count once. Matrix
    /// handles in different operations are counted separately; allocator overhead
    /// is excluded, as in the language IR accounting contract.
    /// # Errors
    /// Rejects storage arithmetic overflow.
    pub fn shared_storage_bytes<'a>(
        fragments: impl IntoIterator<Item = &'a Self>,
    ) -> Result<usize> {
        let mut seen = BTreeSet::new();
        fragments.into_iter().try_fold(0usize, |total, fragment| {
            total
                .checked_add(fragment.retained_bytes(&mut seen)?)
                .ok_or(Error::Budget("oracle storage"))
        })
    }
    fn retained_bytes(&self, seen: &mut BTreeSet<*const OracleBody>) -> Result<usize> {
        if !seen.insert(Arc::as_ptr(&self.body)) {
            return Ok(0);
        }
        let initial = self
            .operations()
            .len()
            .checked_mul(size_of::<Operation>())
            .and_then(|bytes| bytes.checked_add(size_of::<OracleBody>()))
            .ok_or(Error::Budget("oracle storage"))?;
        self.operations()
            .iter()
            .try_fold(initial, |total, operation| {
                let (targets, controls, extra) = match operation {
                    Operation::Gate {
                        targets, controls, ..
                    } => (targets.capacity(), controls.capacity(), 0),
                    Operation::GlobalPhase { controls, .. } => (0, controls.capacity(), 0),
                    Operation::Numerical {
                        matrix,
                        targets,
                        controls,
                    } => (targets.capacity(), controls.capacity(), matrix.bytes()),
                    Operation::Oracle {
                        fragment,
                        targets,
                        controls,
                    } => (
                        targets.capacity(),
                        controls.capacity(),
                        fragment.retained_bytes(seen)?,
                    ),
                    Operation::Barrier { qubits } => (qubits.capacity(), 0, 0),
                    _ => return Err(Error::NotUnitary),
                };
                targets
                    .checked_mul(size_of::<QubitId>())
                    .and_then(|bytes| {
                        bytes.checked_add(controls.checked_mul(size_of::<Control>())?)
                    })
                    .and_then(|bytes| bytes.checked_add(extra))
                    .and_then(|bytes| total.checked_add(bytes))
                    .ok_or(Error::Budget("oracle storage"))
            })
    }
    /// Original immutable body. Local qubit indices define the ordered interface.
    #[must_use]
    pub fn operations(&self) -> &[Operation] {
        &self.body.operations
    }
    #[must_use]
    pub const fn is_adjoint(&self) -> bool {
        self.adjoint
    }
    #[must_use]
    pub fn adjoint(&self) -> Self {
        Self {
            body: Arc::clone(&self.body),
            adjoint: !self.adjoint,
        }
    }
    #[must_use]
    pub fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.body, &other.body)
    }
    /// Remap one body layer, retaining nested calls. Adjoint reverses execution
    /// order, conjugates numerical matrices and negates controlled global phase.
    ///
    /// # Errors
    /// Rejects arity, foreign/duplicate operands and numerical allocation failure.
    pub fn decompose(
        &self,
        targets: &[QubitId],
        controls: &[Control],
        policy: MatrixPolicy,
    ) -> Result<Vec<Operation>> {
        self.check_operands(targets, controls)?;
        let map = |q: QubitId| targets.get(q.index()).copied().ok_or(Error::InvalidId);
        let map_targets =
            |local: &[QubitId]| local.iter().copied().map(map).collect::<Result<Vec<_>>>();
        let map_controls = |local: &[Control]| {
            local
                .iter()
                .map(|c| Ok(Control::new(map(c.qubit())?, c.state())))
                .chain(controls.iter().copied().map(Ok))
                .collect::<Result<Vec<_>>>()
        };
        let mut output = Vec::new();
        output
            .try_reserve_exact(self.body.operations.len())
            .map_err(|_| Error::Budget("oracle decomposition"))?;
        for position in 0..self.body.operations.len() {
            let index = if self.adjoint {
                self.body
                    .operations
                    .len()
                    .checked_sub(position.saturating_add(1))
                    .ok_or(Error::InvalidId)?
            } else {
                position
            };
            let operation = self.body.operations.get(index).ok_or(Error::InvalidId)?;
            output.push(match operation {
                Operation::Gate {
                    gate,
                    targets,
                    controls,
                } => Operation::Gate {
                    gate: if self.adjoint {
                        gate.adjoint()
                    } else {
                        gate.clone()
                    },
                    targets: map_targets(targets)?,
                    controls: map_controls(controls)?,
                },
                Operation::GlobalPhase { radians, controls } => Operation::GlobalPhase {
                    radians: if self.adjoint { -*radians } else { *radians },
                    controls: map_controls(controls)?,
                },
                Operation::Numerical {
                    matrix,
                    targets,
                    controls,
                } => Operation::Numerical {
                    matrix: if self.adjoint {
                        matrix.conjugate_transpose(policy)?
                    } else {
                        matrix.clone()
                    },
                    targets: map_targets(targets)?,
                    controls: map_controls(controls)?,
                },
                Operation::Oracle {
                    fragment,
                    targets,
                    controls,
                } => Operation::Oracle {
                    fragment: if self.adjoint {
                        fragment.adjoint()
                    } else {
                        fragment.clone()
                    },
                    targets: map_targets(targets)?,
                    controls: map_controls(controls)?,
                },
                Operation::Barrier { qubits } => Operation::Barrier {
                    qubits: map_targets(qubits)?
                        .into_iter()
                        .chain(controls.iter().map(|c| c.qubit()))
                        .collect(),
                },
                _ => return Err(Error::NotUnitary),
            });
        }
        Ok(output)
    }
    pub(crate) fn check_operands(&self, targets: &[QubitId], controls: &[Control]) -> Result<()> {
        if targets.len() != self.num_qubits() {
            return Err(Error::Arity {
                expected: self.num_qubits(),
                actual: targets.len(),
            });
        }
        let owner = targets.first().ok_or(Error::InvalidId)?.owner;
        let mut seen = BTreeSet::new();
        for q in targets
            .iter()
            .copied()
            .chain(controls.iter().map(|c| c.qubit()))
        {
            if q.owner != owner {
                return Err(Error::InvalidId);
            }
            if !seen.insert(q) {
                return Err(Error::DuplicateOperand);
            }
        }
        Ok(())
    }
}
impl BoundGate {
    /// Numerical adjoint, without a symbolic inverse/cancellation claim.
    #[must_use]
    pub const fn adjoint(&self) -> Self {
        match self {
            Self::S => Self::Sdg,
            Self::Sdg => Self::S,
            Self::T => Self::Tdg,
            Self::Tdg => Self::T,
            Self::Sx => Self::Sxdg,
            Self::Sxdg => Self::Sx,
            Self::Rx(a) => Self::Rx(-*a),
            Self::Ry(a) => Self::Ry(-*a),
            Self::Rz(a) => Self::Rz(-*a),
            Self::Phase(a) => Self::Phase(-*a),
            Self::U { theta, phi, lambda } => Self::U {
                theta: -*theta,
                phi: -*lambda,
                lambda: -*phi,
            },
            Self::Id => Self::Id,
            Self::X => Self::X,
            Self::Y => Self::Y,
            Self::Z => Self::Z,
            Self::H => Self::H,
            Self::Swap => Self::Swap,
        }
    }
}

impl OracleFragment {
    /// Export an explicit portable built-in decomposition. Numerical matrices
    /// require a caller-supplied decomposition and are never silently omitted.
    /// # Errors
    /// Rejects unsupported matrix payloads, remapping and export budgets.
    pub fn export_qasm(&self, limits: crate::qasm::ExportLimits) -> crate::qasm::Result<String> {
        use crate::language::syntax::{
            Expression, ExpressionKind, Module, Statement, StatementKind,
        };
        let failure = |message: &str| {
            Box::new(crate::language::Diagnostic::new(
                crate::language::Stage::Export,
                crate::language::DiagnosticCause::UnsupportedCapability {
                    capability: message.into(),
                },
                message,
            ))
        };
        let builder = crate::ProgramBuilder::new(self.num_qubits(), 0)
            .map_err(|error| failure(&error.to_string()))?;
        let targets = (0..self.num_qubits())
            .map(|index| builder.qubit(index))
            .collect::<Result<Vec<_>>>()
            .map_err(|error| failure(&error.to_string()))?;
        let mut operations = Vec::new();
        self.portable_operations(&targets, &[], limits.bytes, &mut operations)
            .map_err(|error| failure(&error.to_string()))?;
        let mut statements = vec![Statement {
            span: None,
            kind: StatementKind::Qubit {
                name: "q".into(),
                size: Some(Expression {
                    span: None,
                    kind: ExpressionKind::Number(self.num_qubits().to_string()),
                }),
            },
        }];
        for operation in operations {
            statements
                .push(portable_statement(operation).map_err(|error| failure(&error.to_string()))?);
        }
        crate::qasm::export_syntax(&Module { statements }, limits)
    }
    fn portable_operations(
        &self,
        targets: &[QubitId],
        controls: &[Control],
        budget: usize,
        output: &mut Vec<Operation>,
    ) -> Result<()> {
        if self.operations().len() > budget.saturating_sub(output.len()) {
            return Err(Error::Budget("oracle export operations"));
        }
        for operation in self.decompose(targets, controls, MatrixPolicy::default())? {
            match operation {
                Operation::Numerical { .. } => {
                    return Err(Error::Unsupported(
                        "portable numerical oracle decomposition",
                    ));
                }
                Operation::Oracle {
                    fragment,
                    targets,
                    controls,
                } => fragment.portable_operations(&targets, &controls, budget, output)?,
                operation => {
                    if output.len() >= budget {
                        return Err(Error::Budget("oracle export operations"));
                    }
                    output.push(operation);
                }
            }
        }
        Ok(())
    }
}
fn portable_statement(operation: Operation) -> Result<crate::language::syntax::Statement> {
    use crate::language::syntax::{
        Expression, ExpressionKind, Modifier, Statement, StatementKind, UnaryOperator,
    };
    let number = |value: f64| {
        let positive = Expression {
            span: None,
            kind: ExpressionKind::Number(value.abs().to_string()),
        };
        if value.is_sign_negative() {
            Expression {
                span: None,
                kind: ExpressionKind::Unary(UnaryOperator::Negate, Box::new(positive)),
            }
        } else {
            positive
        }
    };
    let operand = |qubit: QubitId| Expression {
        span: None,
        kind: ExpressionKind::Index(
            Box::new(Expression {
                span: None,
                kind: ExpressionKind::Name("q".into()),
            }),
            Box::new(Expression {
                span: None,
                kind: ExpressionKind::Number(qubit.index().to_string()),
            }),
        ),
    };
    let (name, arguments, targets, controls) = match operation {
        Operation::Gate {
            gate,
            targets,
            controls,
        } => {
            let (name, values) = match gate {
                BoundGate::Id => ("id", vec![]),
                BoundGate::X => ("x", vec![]),
                BoundGate::Y => ("y", vec![]),
                BoundGate::Z => ("z", vec![]),
                BoundGate::H => ("h", vec![]),
                BoundGate::S => ("s", vec![]),
                BoundGate::Sdg => ("sdg", vec![]),
                BoundGate::T => ("t", vec![]),
                BoundGate::Tdg => ("tdg", vec![]),
                BoundGate::Sx => ("sx", vec![]),
                BoundGate::Sxdg => ("sxdg", vec![]),
                BoundGate::Swap => ("swap", vec![]),
                BoundGate::Rx(a) => ("rx", vec![a]),
                BoundGate::Ry(a) => ("ry", vec![a]),
                BoundGate::Rz(a) => ("rz", vec![a]),
                BoundGate::Phase(a) => ("p", vec![a]),
                BoundGate::U { theta, phi, lambda } => ("U", vec![theta, phi, lambda]),
            };
            (
                name,
                values.into_iter().map(number).collect(),
                targets,
                controls,
            )
        }
        Operation::GlobalPhase { radians, controls } => {
            ("gphase", vec![number(radians)], Vec::new(), controls)
        }
        Operation::Barrier { qubits } => {
            return Ok(Statement {
                span: None,
                kind: StatementKind::Barrier(qubits.into_iter().map(operand).collect()),
            });
        }
        _ => return Err(Error::Unsupported("portable oracle operation")),
    };
    Ok(Statement {
        span: None,
        kind: StatementKind::Gate {
            name: name.into(),
            arguments,
            operands: controls
                .iter()
                .map(|control| operand(control.qubit()))
                .chain(targets.into_iter().map(operand))
                .collect(),
            modifiers: controls
                .iter()
                .map(|control| Modifier::Control {
                    positive: control.state() == crate::ControlState::One,
                    count: None,
                })
                .collect(),
        },
    })
}
