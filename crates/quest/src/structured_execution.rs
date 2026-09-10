//! Environment-bound preparation and native dispatch for structured SSA.
use crate::{
    Environment, Error, Outcome, QubitCount, Register, RegisterKind, Result,
    environment::Reservation,
    error::{BackendResult, StructuredExecutionError},
    execution::{NativeControls, admit_fingerprint, apply_gate, phase},
    values::reserve_vec,
};
use cxx::UniquePtr;
use quest_circuit::{
    BoundGate, StructuredPlan, StructuredProgram,
    language::{
        Adjoint, GateKind,
        ssa::InstructionKind,
        vm::{self, GateRequest, QuantumBackend},
    },
};

/// A prepared structured interpreter. Native reset resources and scratch buffers
/// are admitted before publication and dropped before the owning environment.
pub struct PreparedStructuredProgram<'env> {
    reset: Option<UniquePtr<quest_sys::KrausMap>>,
    controls: NativeControls,
    targets: Vec<i32>,
    reservation: Reservation<'env>,
    plan: StructuredPlan,
    fingerprint: quest_sys::NumericalFingerprint,
}
impl Environment {
    /// Verify, lower and prepare the shared OpenQASM/Rust structured frontend.
    ///
    /// # Errors
    /// Rejects invalid language, resources, native configuration and preparation failures.
    pub fn prepare_structured(
        &self,
        program: StructuredProgram,
    ) -> Result<PreparedStructuredProgram<'_>> {
        self.prepare_structured_plan(program.verify()?.lower()?.plan()?)
    }
    /// Transfer an independently verified structured plan into environment-bound resources.
    ///
    /// # Errors
    /// Rejects native index, configuration and memory limits transactionally.
    pub fn prepare_structured_plan(
        &self,
        plan: StructuredPlan,
    ) -> Result<PreparedStructuredProgram<'_>> {
        QubitCount::new(plan.num_qubits())?;
        let fingerprint =
            quest_sys::get_numerical_fingerprint().context("checking structured environment")?;
        admit_fingerprint(&fingerprint)?;
        let reset_needed = plan
            .ssa()
            .blocks()
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(instruction.kind, InstructionKind::Reset { .. }));
        let scratch = plan.num_qubits().checked_mul(32).ok_or(Error::Overflow)?;
        let required = plan
            .resources()
            .ir_bytes
            .checked_add(plan.resources().source_bytes)
            .and_then(|n| n.checked_add(scratch))
            .and_then(|n| n.checked_add(if reset_needed { 4096 } else { 0 }))
            .ok_or(Error::Overflow)?;
        let reservation = self.reserve(required)?;
        let capacity = plan.num_qubits();
        let controls = NativeControls {
            wires: reserve_vec(capacity)?,
            states: reserve_vec(capacity)?,
            zeros: reserve_vec(capacity)?,
            phase_targets: reserve_vec(capacity)?,
        };
        let targets = reserve_vec(capacity)?;
        let reset = if reset_needed {
            let zero = quest_sys::QuestComplex { re: 0., im: 0. };
            let one = quest_sys::QuestComplex { re: 1., im: 0. };
            let mut map =
                quest_sys::create_kraus_map(1, 2).context("allocating structured reset")?;
            quest_sys::set_kraus_map_flat(
                map.pin_mut(),
                &[one, zero, zero, zero, zero, one, zero, zero],
                2,
                2,
            )
            .context("preparing structured reset")?;
            Some(map)
        } else {
            None
        };
        Ok(PreparedStructuredProgram {
            reset,
            controls,
            targets,
            reservation,
            plan,
            fingerprint,
        })
    }
}
impl PreparedStructuredProgram<'_> {
    #[must_use]
    pub const fn plan(&self) -> &StructuredPlan {
        &self.plan
    }
    /// Execute against the current register state with default bounded limits.
    ///
    /// # Errors
    /// Reports configuration, input or runtime failure with the completed quantum prefix.
    pub fn run<K: RegisterKind>(
        &mut self,
        register: &mut Register<'_, K>,
        inputs: &vm::RunInputs,
    ) -> Result<vm::RunOutput> {
        self.run_with_limits(register, inputs, vm::InterpreterLimits::default())
    }
    /// Execute with explicit step, call-frame and storage budgets.
    ///
    /// # Errors
    /// Rejects resource admission before execution; later failures retain completed effects.
    pub fn run_with_limits<K: RegisterKind>(
        &mut self,
        register: &mut Register<'_, K>,
        inputs: &vm::RunInputs,
        limits: vm::InterpreterLimits,
    ) -> Result<vm::RunOutput> {
        if !std::ptr::eq(register.environment(), self.reservation.environment)
            || register.num_qubits().get() != self.plan.num_qubits()
        {
            return Err(Error::RegisterMismatch);
        }
        let fingerprint = quest_sys::get_numerical_fingerprint()
            .context("checking structured execution environment")?;
        if fingerprint != self.fingerprint {
            return Err(Error::ConfigurationChanged);
        }
        let _run_storage = self.reservation.environment.reserve(limits.storage_bytes)?;
        let mut backend = Backend {
            register,
            reset: self.reset.as_ref(),
            controls: &mut self.controls,
            targets: &mut self.targets,
        };
        vm::Interpreter::new(limits)
            .run(self.plan.ssa(), &mut backend, inputs, self.plan.captures())
            .map_err(|error| {
                let diagnostic = runtime_diagnostic(&error, &self.plan);
                Error::StructuredExecution(Box::new(StructuredExecutionError::new(
                    error, diagnostic,
                )))
            })
    }
}
fn runtime_diagnostic(
    error: &vm::RuntimeError<Error>,
    plan: &StructuredPlan,
) -> quest_circuit::language::Diagnostic {
    let mut diagnostic = error.diagnostic(plan.sources());
    if diagnostic.labels.is_empty()
        && let Some(span) = diagnostic.occurrence
        && let Some(location) = plan
            .locations()
            .iter()
            .find(|location| location.span == span)
    {
        diagnostic.notes.push(format!(
            "Rust source location: {}:{}:{}",
            location.file, location.line, location.column
        ));
    }
    diagnostic
}
struct Backend<'a, 'env, K: RegisterKind> {
    register: &'a mut Register<'env, K>,
    reset: Option<&'a UniquePtr<quest_sys::KrausMap>>,
    controls: &'a mut NativeControls,
    targets: &'a mut Vec<i32>,
}
impl<K: RegisterKind> QuantumBackend for Backend<'_, '_, K> {
    type Error = Error;
    fn apply_gate(&mut self, request: GateRequest<'_>) -> Result<()> {
        self.controls.wires.clear();
        self.controls.states.clear();
        self.controls.zeros.clear();
        self.controls.phase_targets.clear();
        self.targets.clear();
        for control in request.controls {
            let index = self.register.check_qubit(control.qubit)?;
            self.controls.wires.push(index);
            self.controls.states.push(i32::from(control.positive));
            self.controls.phase_targets.push(index);
            if !control.positive {
                self.controls.zeros.push(control.qubit);
            }
        }
        for target in request.targets {
            self.targets.push(self.register.check_qubit(*target)?);
        }
        if let Some(target) = self.targets.first() {
            self.controls.phase_targets.push(*target);
        }
        let (kind, parameters) = adjoint_parameters(request)?;
        if kind == GateKind::GlobalPhase {
            return phase(self.register, parameter(&parameters, 0)?, self.controls);
        }
        let gate = bound_gate(kind, &parameters)?;
        apply_gate(self.register, &gate, self.targets, self.controls)
    }
    fn measure(&mut self, qubit: usize) -> Result<bool> {
        Ok(self.register.measure(qubit)?.as_bool())
    }
    fn reset(&mut self, qubit: usize) -> Result<()> {
        let target = self.register.check_qubit(qubit)?;
        if self.register.is_density() {
            let reset = self
                .reset
                .ok_or(Error::Value("missing prepared reset map"))?;
            quest_sys::mix_kraus_map(self.register.pin(), &[target], reset)
                .context("resetting structured density qubit")
        } else {
            if self.register.measure(qubit)? == Outcome::One {
                self.register.x(qubit)?;
            }
            Ok(())
        }
    }
    fn barrier(&mut self, qubits: &[usize]) -> Result<()> {
        for qubit in qubits {
            self.register.check_qubit(*qubit)?;
        }
        Ok(())
    }
}
fn parameter(values: &[f64], index: usize) -> Result<f64> {
    values
        .get(index)
        .copied()
        .filter(|value| value.is_finite())
        .ok_or(Error::Value("missing or nonfinite gate parameter"))
}
fn adjoint_parameters(request: GateRequest<'_>) -> Result<(GateKind, [f64; 3])> {
    let mut result = [0.; 3];
    let mut kind = request.gate;
    if request.parameters.len() != kind.definition().parameter_count {
        return Err(Error::Value("gate parameter arity"));
    }
    if request.inverse {
        match kind.definition().adjoint {
            Adjoint::Gate(gate) => kind = gate,
            Adjoint::Parameters(mapping) => {
                for (target, mapping) in result.iter_mut().zip(mapping) {
                    let value = parameter(request.parameters, mapping.input)?;
                    *target = if mapping.negate { -value } else { value };
                }
                return Ok((kind, result));
            }
            Adjoint::SelfInverse => {}
        }
    }
    for (target, value) in result.iter_mut().zip(request.parameters) {
        *target = *value;
    }
    Ok((kind, result))
}
fn bound_gate(kind: GateKind, parameters: &[f64]) -> Result<BoundGate> {
    Ok(match kind {
        GateKind::Id => BoundGate::Id,
        GateKind::X | GateKind::Cx | GateKind::Ccx => BoundGate::X,
        GateKind::Y | GateKind::Cy => BoundGate::Y,
        GateKind::Z | GateKind::Cz => BoundGate::Z,
        GateKind::H => BoundGate::H,
        GateKind::S => BoundGate::S,
        GateKind::Sdg => BoundGate::Sdg,
        GateKind::T => BoundGate::T,
        GateKind::Tdg => BoundGate::Tdg,
        GateKind::Sx => BoundGate::Sx,
        GateKind::Sxdg => BoundGate::Sxdg,
        GateKind::Swap => BoundGate::Swap,
        GateKind::Rx => BoundGate::Rx(parameter(parameters, 0)?),
        GateKind::Ry => BoundGate::Ry(parameter(parameters, 0)?),
        GateKind::Rz => BoundGate::Rz(parameter(parameters, 0)?),
        GateKind::Phase => BoundGate::Phase(parameter(parameters, 0)?),
        GateKind::U => BoundGate::U {
            theta: parameter(parameters, 0)?,
            phi: parameter(parameters, 1)?,
            lambda: parameter(parameters, 2)?,
        },
        GateKind::GlobalPhase => return Err(Error::Value("global phase must use scalar dispatch")),
    })
}
