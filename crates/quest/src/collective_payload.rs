//! Full semantic values and canonical cache-sharing graph; addresses are never serialized.
use crate::{Error, Result};
use quest_compile::{
    BoundGate, Control, ControlState, Operation, OracleFragment, QubitId, RegionPlan,
};

pub fn encode(plan: &RegionPlan, limit: usize) -> Result<Vec<u8>> {
    let mut out = Encoder {
        bytes: Vec::new(),
        limit,
        scratch_bytes: 0,
        matrices: Vec::new(),
        oracles: Vec::new(),
    };
    out.word(2)?; // Versioned canonical execution payload.
    out.word(plan.num_qubits())?;
    out.word(plan.num_bits())?;
    out.word(plan.instructions().len())?;
    for instruction in plan.instructions() {
        out.operation(instruction.operation(), 0)?;
    }
    Ok(out.bytes)
}
struct Encoder<'a> {
    bytes: Vec<u8>,
    limit: usize,
    scratch_bytes: usize,
    matrices: Vec<usize>,
    oracles: Vec<&'a OracleFragment>,
}
impl<'a> Encoder<'a> {
    fn admit_storage(&self, additional: usize) -> Result<()> {
        let requested = self
            .bytes
            .len()
            .checked_add(self.scratch_bytes)
            .and_then(|n| n.checked_add(additional))
            .ok_or(Error::Overflow)?;
        if requested > self.limit {
            return Err(Error::Budget {
                requested,
                available: self.limit,
            });
        }
        Ok(())
    }
    fn matrix_identity(&mut self, address: usize) -> Result<()> {
        let index = if let Some(index) = self.matrices.iter().position(|&seen| seen == address) {
            index
        } else {
            self.admit_storage(size_of::<usize>())?;
            self.matrices
                .try_reserve_exact(1)
                .map_err(|_| Error::Allocation)?;
            self.scratch_bytes = self
                .scratch_bytes
                .checked_add(size_of::<usize>())
                .ok_or(Error::Overflow)?;
            let index = self.matrices.len();
            self.matrices.push(address);
            index
        };
        // Only the deterministic first-occurrence index crosses ranks. Every
        // matrix's full numerical content is still encoded independently.
        self.word(index)
    }
    fn oracle_identity(&mut self, fragment: &'a OracleFragment) -> Result<()> {
        let index = if let Some(index) = self
            .oracles
            .iter()
            .position(|seen| seen.shares_storage_with(fragment))
        {
            index
        } else {
            self.admit_storage(size_of::<&OracleFragment>())?;
            self.oracles
                .try_reserve_exact(1)
                .map_err(|_| Error::Allocation)?;
            self.scratch_bytes = self
                .scratch_bytes
                .checked_add(size_of::<&OracleFragment>())
                .ok_or(Error::Overflow)?;
            let index = self.oracles.len();
            self.oracles.push(fragment);
            index
        };
        self.word(index)
    }
    fn raw(&mut self, value: &[u8]) -> Result<()> {
        self.admit_storage(value.len())?;
        self.bytes
            .try_reserve_exact(value.len())
            .map_err(|_| Error::Allocation)?;
        self.bytes.extend_from_slice(value);
        Ok(())
    }
    fn word(&mut self, value: usize) -> Result<()> {
        self.raw(
            &u64::try_from(value)
                .map_err(|_| Error::Overflow)?
                .to_le_bytes(),
        )
    }
    fn float(&mut self, value: f64) -> Result<()> {
        self.raw(&value.to_bits().to_le_bytes())
    }
    fn targets(&mut self, values: &[QubitId]) -> Result<()> {
        self.word(values.len())?;
        for value in values {
            self.word(value.index())?;
        }
        Ok(())
    }
    fn controls(&mut self, values: &[Control]) -> Result<()> {
        self.word(values.len())?;
        for value in values {
            self.word(value.qubit().index())?;
            self.word(usize::from(value.state() == ControlState::One))?;
        }
        Ok(())
    }
    fn gate(&mut self, gate: &BoundGate) -> Result<()> {
        let (tag, angles): (usize, &[f64]) = match gate {
            BoundGate::Id => (0, &[]),
            BoundGate::X => (1, &[]),
            BoundGate::Y => (2, &[]),
            BoundGate::Z => (3, &[]),
            BoundGate::H => (4, &[]),
            BoundGate::S => (5, &[]),
            BoundGate::Sdg => (6, &[]),
            BoundGate::T => (7, &[]),
            BoundGate::Tdg => (8, &[]),
            BoundGate::Sx => (9, &[]),
            BoundGate::Sxdg => (10, &[]),
            BoundGate::Swap => (11, &[]),
            BoundGate::Rx(a) => (12, std::slice::from_ref(a)),
            BoundGate::Ry(a) => (13, std::slice::from_ref(a)),
            BoundGate::Rz(a) => (14, std::slice::from_ref(a)),
            BoundGate::Phase(a) => (15, std::slice::from_ref(a)),
            BoundGate::U { theta, phi, lambda } => {
                self.word(16)?;
                self.float(*theta)?;
                self.float(*phi)?;
                return self.float(*lambda);
            }
        };
        self.word(tag)?;
        for angle in angles {
            self.float(*angle)?;
        }
        Ok(())
    }
    fn operation(&mut self, operation: &'a Operation, depth: usize) -> Result<()> {
        if depth > 256 {
            return Err(Error::Unsupported("collective oracle nesting beyond 256"));
        }
        match operation {
            Operation::Gate {
                gate,
                targets,
                controls,
            } => {
                self.word(0)?;
                self.gate(gate)?;
                self.targets(targets)?;
                self.controls(controls)
            }
            Operation::GlobalPhase { radians, controls } => {
                self.word(1)?;
                self.float(*radians)?;
                self.controls(controls)
            }
            Operation::Numerical {
                matrix,
                targets,
                controls,
            } => {
                self.word(2)?;
                self.matrix_identity(matrix.view().as_ptr().addr())?;
                self.targets(targets)?;
                self.controls(controls)?;
                self.word(matrix.dimension())?;
                let view = matrix.view();
                for row in 0..matrix.dimension() {
                    for col in 0..matrix.dimension() {
                        self.float(view[(row, col)].re)?;
                        self.float(view[(row, col)].im)?;
                    }
                }
                Ok(())
            }
            Operation::Oracle {
                fragment,
                targets,
                controls,
            } => {
                self.word(3)?;
                self.oracle_identity(fragment)?;
                self.targets(targets)?;
                self.controls(controls)?;
                self.word(fragment.num_qubits())?;
                self.word(usize::from(fragment.is_adjoint()))?;
                self.word(fragment.operations().len())?;
                for operation in fragment.operations() {
                    self.operation(operation, depth.checked_add(1).ok_or(Error::Overflow)?)?;
                }
                Ok(())
            }
            Operation::Barrier { qubits } => {
                self.word(4)?;
                self.targets(qubits)
            }
            Operation::Measure { .. }
            | Operation::Reset { .. }
            | Operation::Channel { .. }
            | Operation::Conditional { .. } => Err(Error::Unsupported(
                "collective preparation requires coherent operations",
            )),
        }
    }
}
