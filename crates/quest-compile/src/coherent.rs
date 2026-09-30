//! Checked finite lowering for backends that require a coherent transaction.
use crate::{
    Angle, BoundGate, BoundRegion, Control, ControlState, Error, Executable, Gate, LanguageError,
    Operation, Program, QuantumPayload, QuantumRegionBuilder,
};
use quest_language::{
    ssa,
    vm::{self, QuantumBackend},
};

struct Recorder<'a> {
    builder: QuantumRegionBuilder,
    program: &'a Program<Executable>,
    remaining_bytes: usize,
}
impl Recorder<'_> {
    fn admit_operation(&mut self) -> crate::Result<()> {
        // Covers geometric vector capacity, semantic/bound/plan operation copies,
        // operand copies, per-wire dependency edges and provenance nodes. Immutable
        // oracle/matrix bodies are covered once by the initial IR reservation.
        let bytes = self
            .program
            .num_qubits()
            .checked_mul(256)
            .and_then(|n| n.checked_add(4096))
            .ok_or(Error::Budget("coherent region storage"))?;
        self.remaining_bytes = self
            .remaining_bytes
            .checked_sub(bytes)
            .ok_or(Error::Budget("coherent region storage"))?;
        Ok(())
    }
    fn targets(&self, targets: &[usize]) -> crate::Result<Vec<crate::QubitId>> {
        targets.iter().map(|q| self.builder.qubit(*q)).collect()
    }
    fn controls(&self, controls: &[vm::QuantumControl]) -> crate::Result<Vec<Control>> {
        controls
            .iter()
            .map(|c| {
                Ok(Control::new(
                    self.builder.qubit(c.qubit)?,
                    if c.positive {
                        ControlState::One
                    } else {
                        ControlState::Zero
                    },
                ))
            })
            .collect()
    }
}
impl QuantumBackend for Recorder<'_> {
    type Error = Error;
    fn apply_gate(&mut self, request: vm::GateRequest<'_>) -> crate::Result<()> {
        self.admit_operation()?;
        let targets = self.targets(request.targets)?;
        let controls = self.controls(request.controls)?;
        if request.gate == quest_language::GateKind::GlobalPhase {
            let value = *request.parameters.first().ok_or(Error::Binding)?;
            self.builder.global_phase(
                Angle::radians(if request.inverse { -value } else { value })?,
                &controls,
            )?;
        } else {
            let gate = Gate::from_bound(&BoundGate::from_kind(request.gate, request.parameters)?)?;
            self.builder.gate(
                if request.inverse {
                    gate.adjoint()?
                } else {
                    gate
                },
                &targets,
                &controls,
            )?;
        }
        Ok(())
    }
    fn apply_oracle(&mut self, request: vm::OracleRequest<'_>) -> Option<crate::Result<()>> {
        Some((|| {
            self.admit_operation()?;
            let fragment = self
                .program
                .oracle_captures()
                .get(&request.capture)
                .ok_or(Error::InvalidId)?;
            let targets = self.targets(request.targets)?;
            let controls = self.controls(request.controls)?;
            self.builder.oracle(
                &if request.adjoint {
                    fragment.adjoint()
                } else {
                    fragment.clone()
                },
                &targets,
                &controls,
            )?;
            Ok(())
        })())
    }
    fn apply_payload(&mut self, capture: usize, wires: &[usize]) -> Option<crate::Result<()>> {
        Some((|| {
            self.admit_operation()?;
            match self
                .program
                .quantum_payloads()
                .get(&capture)
                .ok_or(Error::InvalidId)?
            {
                QuantumPayload::Matrix {
                    matrix,
                    control_states,
                } => {
                    let (control_wires, target_wires) = wires.split_at(control_states.len());
                    let controls = control_wires
                        .iter()
                        .zip(control_states.iter())
                        .map(|(q, positive)| {
                            Ok(Control::new(
                                self.builder.qubit(*q)?,
                                if *positive {
                                    ControlState::One
                                } else {
                                    ControlState::Zero
                                },
                            ))
                        })
                        .collect::<crate::Result<Vec<_>>>()?;
                    self.builder.numerical(
                        matrix.clone(),
                        &self.targets(target_wires)?,
                        &controls,
                    )?;
                }
                QuantumPayload::Channel { .. } => return Err(Error::NotUnitary),
            }
            Ok(())
        })())
    }
    fn measure(&mut self, _: usize) -> crate::Result<bool> {
        Err(Error::NotUnitary)
    }
    fn reset(&mut self, _: usize) -> crate::Result<()> {
        Err(Error::NotUnitary)
    }
    fn barrier(&mut self, qubits: &[usize]) -> crate::Result<()> {
        self.admit_operation()?;
        self.builder.barrier(&self.targets(qubits)?)?;
        Ok(())
    }
}
impl Program<Executable> {
    /// Lower a closed coherent program into a finite backend transaction.
    /// Input interfaces, classical branching, measurements, reset and channels are rejected.
    /// The common VM resolves gate definitions/modifiers and checks all source traps before publication.
    /// # Errors
    /// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
    pub fn try_coherent_region(&self) -> Result<BoundRegion, LanguageError> {
        self.try_coherent_region_with_limits(
            vm::InterpreterLimits::default(),
            crate::ProgramLimits::default(),
            64 * 1024 * 1024,
        )
    }
    /// Lower a closed coherent program with explicit replay and finite-region budgets.
    /// `max_bytes` bounds the conservative live recorder/bind/plan storage model,
    /// separately from the interpreter's storage cap and structural region limits.
    /// # Errors
    /// Rejects effects, unresolved input, traps, and either resource budget before publication.
    pub fn try_coherent_region_with_limits(
        &self,
        interpreter: vm::InterpreterLimits,
        region: crate::ProgramLimits,
        max_bytes: usize,
    ) -> Result<BoundRegion, LanguageError> {
        let initial_bytes = self
            .resources()
            .ir_bytes
            .checked_mul(4)
            .and_then(|n| n.checked_add(self.num_qubits().checked_mul(512)?))
            .and_then(|n| n.checked_add(4096))
            .ok_or(Error::Budget("coherent region storage"))?;
        let remaining_bytes = max_bytes
            .checked_sub(initial_bytes)
            .ok_or(Error::Budget("coherent region storage"))?;
        if self
            .ssa()
            .slots()
            .iter()
            .any(|slot| slot.interface == ssa::Interface::Input)
        {
            return Err(LanguageError::Unsupported("collective runtime inputs"));
        }
        for block in self.ssa().blocks() {
            if matches!(block.terminator, Some(ssa::Terminator::Branch { .. })) {
                return Err(LanguageError::Unsupported(
                    "collective classical control flow",
                ));
            }
            for instruction in &block.instructions {
                if matches!(
                    instruction.kind,
                    ssa::InstructionKind::Measure { .. } | ssa::InstructionKind::Reset { .. }
                ) {
                    return Err(Error::NotUnitary.into());
                }
            }
        }
        if self
            .quantum_payloads()
            .values()
            .any(|payload| matches!(payload, QuantumPayload::Channel { .. }))
        {
            return Err(Error::NotUnitary.into());
        }
        let mut recorder = Recorder {
            builder: QuantumRegionBuilder::with_limits(self.num_qubits(), 0, region)?,
            program: self,
            remaining_bytes,
        };
        vm::Interpreter::new(interpreter)
            .run(
                self.ssa(),
                &mut recorder,
                &vm::RunInputs::default(),
                self.captures(),
            )
            .map_err(|_| LanguageError::Unsupported("collective static execution failed"))?;
        let region = recorder.builder.finish()?.bind(&[])?;
        // Imported finite source keeps its exact targets and complete provenance when no compiler
        // transformation has changed the executable snapshot.
        if let Some(origin) = self.current_finite_origin() {
            if origin.instructions().iter().any(|instruction| {
                matches!(
                    instruction.operation(),
                    Operation::Measure { .. }
                        | Operation::Reset { .. }
                        | Operation::Channel { .. }
                        | Operation::Conditional { .. }
                )
            }) {
                return Err(Error::NotUnitary.into());
            }
            return Ok(origin.clone());
        }
        Ok(region)
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    #[test]
    fn coherent_replay_respects_aggregate_region_storage() {
        let program = Program::parse("qubit q; h q;", "bounded collective")
            .unwrap()
            .verify()
            .unwrap()
            .lower()
            .unwrap()
            .plan()
            .unwrap();
        assert!(
            program
                .try_coherent_region_with_limits(
                    vm::InterpreterLimits::default(),
                    crate::ProgramLimits::default(),
                    1
                )
                .is_err()
        );
        assert!(
            program
                .try_coherent_region_with_limits(
                    vm::InterpreterLimits::default(),
                    crate::ProgramLimits::default(),
                    1024 * 1024
                )
                .is_ok()
        );
        let expanded = Program::parse("qubit q; pow(1000) @ h q;", "bounded expansion")
            .unwrap()
            .verify()
            .unwrap()
            .lower()
            .unwrap()
            .plan()
            .unwrap();
        assert!(
            expanded
                .try_coherent_region_with_limits(
                    vm::InterpreterLimits::default(),
                    crate::ProgramLimits::default(),
                    512 * 1024
                )
                .is_err()
        );
        assert!(
            expanded
                .try_coherent_region_with_limits(
                    vm::InterpreterLimits::default(),
                    crate::ProgramLimits::default(),
                    8 * 1024 * 1024
                )
                .is_ok()
        );
    }
}
