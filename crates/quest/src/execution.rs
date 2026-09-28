use crate::oracle_execution::{OracleCache, OracleInventory};
use crate::{Complex64, Environment, Error, Outcome, QubitCount, Result, Shots};
use crate::{
    environment::Reservation,
    error::BackendResult,
    register::{Register, RegisterKind, matrix},
    values::{bytes_for, reserve_vec},
};
use cxx::UniquePtr;
use quest_circuit::{
    BoundGate, Control, ControlState, ExecutablePlan, Operation, ValidatedProgram,
};
use std::collections::BTreeMap;

pub enum NativeMatrix {
    Dense {
        forward: UniquePtr<quest_sys::CompMatr>,
        adjoint: UniquePtr<quest_sys::CompMatr>,
    },
    Diagonal {
        forward: UniquePtr<quest_sys::DiagMatr>,
        adjoint: UniquePtr<quest_sys::DiagMatr>,
    },
}
pub struct NativeControls {
    pub(crate) wires: Vec<i32>,
    pub(crate) states: Vec<i32>,
    pub(crate) zeros: Vec<usize>,
    pub(crate) phase_targets: Vec<i32>,
}
impl NativeControls {
    fn new(controls: &[Control], target: Option<i32>) -> Result<Self> {
        let mut result = Self::with_capacity(controls.len(), target.is_some())?;
        result.load(
            controls.iter().map(|control| {
                Ok((
                    i32::try_from(control.qubit().index()).map_err(|_| Error::Overflow)?,
                    control.state() == ControlState::One,
                ))
            }),
            target,
        )?;
        Ok(result)
    }
    pub(crate) fn with_capacity(count: usize, extra_target: bool) -> Result<Self> {
        Ok(Self {
            wires: reserve_vec(count)?,
            states: reserve_vec(count)?,
            zeros: reserve_vec(count)?,
            phase_targets: reserve_vec(
                count
                    .checked_add(usize::from(extra_target))
                    .ok_or(Error::Overflow)?,
            )?,
        })
    }
    /// Reuse admitted scratch; never allocate while dispatching an instruction.
    pub(crate) fn load(
        &mut self,
        controls: impl Iterator<Item = Result<(i32, bool)>>,
        target: Option<i32>,
    ) -> Result<()> {
        self.wires.clear();
        self.states.clear();
        self.zeros.clear();
        self.phase_targets.clear();
        for control in controls {
            let (wire, positive) = control?;
            let index = usize::try_from(wire).map_err(|_| Error::Overflow)?;
            if self.wires.len() == self.wires.capacity()
                || self.states.len() == self.states.capacity()
                || self.phase_targets.len() == self.phase_targets.capacity()
                || (!positive && self.zeros.len() == self.zeros.capacity())
            {
                return Err(Error::Value("control scratch capacity exceeded"));
            }
            self.wires.push(wire);
            self.states.push(i32::from(positive));
            self.phase_targets.push(wire);
            if !positive {
                self.zeros.push(index);
            }
        }
        if let Some(target) = target {
            if self.phase_targets.len() == self.phase_targets.capacity() {
                return Err(Error::Value("phase scratch capacity exceeded"));
            }
            self.phase_targets.push(target);
        }
        Ok(())
    }
}
enum PreparedOp {
    Oracle {
        body: usize,
        targets: Vec<usize>,
        controls: Vec<quest_circuit::language::vm::QuantumControl>,
        adjoint: bool,
    },
    Gate {
        gate: BoundGate,
        targets: Vec<i32>,
        controls: NativeControls,
    },
    GlobalPhase {
        radians: f64,
        controls: NativeControls,
    },
    Measure {
        qubit: usize,
        bit: usize,
    },
    Barrier,
    Numerical {
        cache: usize,
        targets: Vec<i32>,
    },
    Channel {
        cache: usize,
        targets: Vec<i32>,
    },
    Reset {
        target: usize,
        cache: usize,
    },
    Conditional {
        bit: usize,
        expected: bool,
        operation: Box<Self>,
    },
}

/// Native resources prepared transactionally. Execution can modify a register
/// before an error; errors identify the completed instruction prefix.
pub struct PreparedProgram<'env> {
    matrices: Vec<NativeMatrix>,
    oracles: OracleCache,
    channels: Vec<UniquePtr<quest_sys::KrausMap>>,
    reservation: Reservation<'env>,
    plan: ExecutablePlan,
    operations: Vec<PreparedOp>,
    fingerprint: quest_sys::NumericalFingerprint,
}
#[derive(Debug)]
pub struct RunResult {
    pub bits: Vec<bool>,
    pub completed_instructions: usize,
}
#[derive(Debug)]
pub struct SampleResult {
    pub counts: BTreeMap<Vec<bool>, usize>,
    pub shots: usize,
    pub seeds: Vec<u32>,
}

impl Environment {
    /// Convenience path for programs with no unbound symbolic parameters.
    /// # Errors
    /// Rejects invalid bindings, unsupported numerical configuration, resource limits, or native preparation failure.
    pub fn prepare(&self, program: ValidatedProgram) -> Result<PreparedProgram<'_>> {
        self.prepare_plan(program.bind(&[])?.plan()?)
    }
    /// # Errors
    /// Rejects unsupported numerical configuration, resource limits, or native preparation failure.
    pub fn prepare_plan(&self, plan: ExecutablePlan) -> Result<PreparedProgram<'_>> {
        self.resources.prepare_plan(plan)
    }
}
impl crate::environment::RuntimeResources {
    /// # Errors
    /// Rejects unsupported numerical configuration, resource limits, or native preparation failure.
    pub fn prepare_plan(&self, plan: ExecutablePlan) -> Result<PreparedProgram<'_>> {
        self.admit_plan(plan)?.materialize()
    }
    pub(crate) fn admit_plan(&self, plan: ExecutablePlan) -> Result<AdmittedPlan<'_>> {
        QubitCount::new(plan.num_qubits())?;
        let fingerprint =
            quest_sys::get_numerical_fingerprint().context("checking numerical environment")?;
        admit_fingerprint(&fingerprint)?;
        let mut inventory = OracleInventory::with_budget(
            self.memory_budget()
                .bytes()
                .saturating_sub(self.allocated_bytes()),
        );
        for instruction in plan.instructions() {
            inventory.include_operation(instruction.operation(), plan.num_qubits())?;
        }
        let mut required = plan
            .instructions()
            .len()
            .checked_mul(
                const {
                    std::mem::size_of::<quest_circuit::Instruction>()
                        + std::mem::size_of::<PreparedOp>()
                        + std::mem::size_of::<NativeMatrix>()
                        + std::mem::size_of::<UniquePtr<quest_sys::KrausMap>>()
                },
            )
            .ok_or(Error::Overflow)?;
        let provenance_bytes = plan.provenance().retained_bytes()?;
        required = required
            .checked_add(plan.num_bits())
            .and_then(|bytes| bytes.checked_add(provenance_bytes))
            .ok_or(Error::Overflow)?;
        for instruction in plan.instructions() {
            if let Some(span) = instruction.source() {
                required = required
                    .checked_add(span.source().len())
                    .ok_or(Error::Overflow)?;
            }
            required = required
                .checked_add(estimate(instruction.operation(), self.capabilities().gpu)?)
                .ok_or(Error::Overflow)?;
        }
        required = required
            .checked_add(inventory.estimated_bytes(plan.num_qubits(), self.capabilities().gpu)?)
            .ok_or(Error::Overflow)?;
        let reservation = self.reserve(required)?;
        Ok(AdmittedPlan {
            plan,
            fingerprint,
            inventory,
            reservation,
        })
    }
}
pub struct AdmittedPlan<'env> {
    plan: ExecutablePlan,
    fingerprint: quest_sys::NumericalFingerprint,
    inventory: OracleInventory,
    reservation: Reservation<'env>,
}
impl<'env> AdmittedPlan<'env> {
    #[cfg(feature = "qsvt")]
    pub(crate) fn dispatch_scratch(&self) -> Result<Reservation<'_>> {
        self.reservation
            .environment
            .reserve(self.inventory.dispatch_scratch_bytes()?)
    }
    #[cfg(feature = "qsvt")]
    pub(crate) const fn plan(&self) -> &ExecutablePlan {
        &self.plan
    }

    pub(crate) fn materialize(self) -> Result<PreparedProgram<'env>> {
        let Self {
            plan,
            fingerprint,
            inventory,
            reservation,
        } = self;
        let oracles = OracleCache::prepare(&inventory, plan.num_qubits())?;
        let mut matrices = reserve_vec(plan.instructions().len())?;
        let mut channels = reserve_vec(plan.instructions().len())?;
        let mut cache = BTreeMap::new();
        let mut operations = reserve_vec(plan.instructions().len())?;
        for instruction in plan.instructions() {
            operations.push(prepare_operation(
                instruction.operation(),
                &mut matrices,
                &mut channels,
                &mut cache,
                &inventory,
            )?);
        }
        // Locals release native handles on any error before the prepared owner is published.
        Ok(PreparedProgram {
            matrices,
            oracles,
            channels,
            reservation,
            plan,
            operations,
            fingerprint,
        })
    }
}
impl PreparedProgram<'_> {
    /// Distinct shared canonical oracle bodies retained in native preparation.
    #[must_use]
    pub const fn prepared_oracle_bodies(&self) -> usize {
        self.oracles.body_count()
    }
    /// Numerical payload/control variants; each owns a forward/adjoint pair.
    #[must_use]
    pub const fn prepared_oracle_matrix_variants(&self) -> usize {
        self.oracles.matrix_count()
    }
    #[must_use]
    pub const fn plan(&self) -> &ExecutablePlan {
        &self.plan
    }
    /// # Errors
    /// Rejects register or configuration mismatch and reports the completed instruction prefix on execution failure.
    pub fn run<K: RegisterKind>(&mut self, register: &mut Register<'_, K>) -> Result<RunResult> {
        let bits = self.admit_run(register)?;
        self.run_admitted(register, bits)
    }
    pub(crate) fn admit_run<K: RegisterKind>(
        &self,
        register: &Register<'_, K>,
    ) -> Result<Vec<bool>> {
        if !std::ptr::eq(register.resources(), self.reservation.environment)
            || register.num_qubits().get() != self.plan.num_qubits()
        {
            return Err(Error::RegisterMismatch);
        }
        if !register.is_density() && self.operations.iter().any(requires_density) {
            return Err(Error::RegisterMismatch);
        }
        let fingerprint =
            quest_sys::get_numerical_fingerprint().context("checking execution environment")?;
        if fingerprint != self.fingerprint {
            return Err(Error::ConfigurationChanged);
        }
        let mut bits = reserve_vec(self.plan.num_bits())?;
        bits.resize(self.plan.num_bits(), false);
        Ok(bits)
    }
    pub(crate) fn run_admitted<K: RegisterKind>(
        &mut self,
        register: &mut Register<'_, K>,
        mut bits: Vec<bool>,
    ) -> Result<RunResult> {
        for (index, operation) in self.operations.iter().enumerate() {
            execute(
                operation,
                register,
                &mut bits,
                &self.matrices,
                &self.channels,
                &mut self.oracles,
            )
            .map_err(|source| Error::Execution {
                instruction: index,
                completed: index,
                source: Box::new(source),
            })?;
        }
        Ok(RunResult {
            bits,
            completed_instructions: self.operations.len(),
        })
    }
    /// Seeds `QuEST`'s process RNG once per batch and restores |0...0> for every shot.
    /// Supply 1–16 seeds; native RNG storage retains a 4096-byte budget allowance.
    /// Density channels use exact density evolution; state vectors use reset trajectories.
    /// # Errors
    /// Rejects invalid seed counts, changed numerical configuration, resource limits, or native execution failure.
    pub fn sample_zeroed(&mut self, shots: Shots, seeds: &[u32]) -> Result<SampleResult> {
        if seeds.is_empty() || seeds.len() > 16 {
            return Err(Error::Value(
                "sampling requires between 1 and 16 explicit RNG seeds",
            ));
        }
        // Worst-case every shot produces a distinct bit string; bound result storage first.
        let fingerprint =
            quest_sys::get_numerical_fingerprint().context("checking sample environment")?;
        if fingerprint != self.fingerprint {
            return Err(Error::ConfigurationChanged);
        }
        let seed_bytes = seeds
            .len()
            .checked_mul(const { std::mem::size_of::<u32>() * 8 })
            .ok_or(Error::Overflow)?;
        let sample_bytes = self
            .plan
            .num_bits()
            .checked_add(128)
            .and_then(|n| n.checked_mul(shots.get()))
            .and_then(|n| n.checked_add(seed_bytes))
            .ok_or(Error::Overflow)?;
        let environment = self.reservation.environment;
        let _results = environment.reserve(sample_bytes)?;
        environment.admit_seed_storage()?;
        quest_sys::set_qu_est_seeds(seeds).context("seeding sample batch")?;
        let mut counts = BTreeMap::<Vec<bool>, usize>::new();
        let count = QubitCount::new(self.plan.num_qubits())?;
        if self.operations.iter().any(requires_density) {
            let mut register = environment.density_matrix(count)?;
            for _ in 0..shots.get() {
                register.init_zero()?;
                let count = counts.entry(self.run(&mut register)?.bits).or_default();
                *count = count.checked_add(1).ok_or(Error::Overflow)?;
            }
        } else {
            let mut register = environment.state_vector(count)?;
            for _ in 0..shots.get() {
                register.init_zero()?;
                let count = counts.entry(self.run(&mut register)?.bits).or_default();
                *count = count.checked_add(1).ok_or(Error::Overflow)?;
            }
        }
        let mut recorded_seeds = reserve_vec(seeds.len())?;
        recorded_seeds.extend_from_slice(seeds);
        Ok(SampleResult {
            counts,
            shots: shots.get(),
            seeds: recorded_seeds,
        })
    }
}
pub const fn admit_fingerprint(fp: &quest_sys::NumericalFingerprint) -> Result<()> {
    if !fp.underflow_control_supported
        || !fp.round_to_nearest
        || fp.flush_to_zero
        || fp.denormals_are_zero
    {
        return Err(Error::Unsupported(
            "round-to-nearest with gradual underflow is required",
        ));
    }
    Ok(())
}
fn requires_density(op: &PreparedOp) -> bool {
    match op {
        PreparedOp::Channel { .. } => true,
        PreparedOp::Conditional { operation, .. } => requires_density(operation),
        _ => false,
    }
}
fn estimate(op: &Operation, gpu: bool) -> Result<usize> {
    op.operand_storage_bytes()?
        .checked_add(estimate_native(op, gpu)?)
        .ok_or(Error::Overflow)
}
fn estimate_native(op: &Operation, gpu: bool) -> Result<usize> {
    let control_bytes = |count: usize| {
        count
            .checked_mul(const { 3 * std::mem::size_of::<i32>() + std::mem::size_of::<usize>() })
            .ok_or(Error::Overflow)
    };
    let targets_bytes = |count: usize| {
        count
            .checked_mul(std::mem::size_of::<i32>())
            .ok_or(Error::Overflow)
    };
    match op {
        Operation::Oracle {
            targets, controls, ..
        } => control_bytes(controls.len())?
            .checked_add(targets_bytes(targets.len())?)
            .and_then(|n| n.checked_add(targets.len().checked_mul(size_of::<usize>())?))
            .ok_or(Error::Overflow),
        Operation::Gate {
            targets, controls, ..
        } => control_bytes(controls.len())?
            .checked_add(targets_bytes(targets.len())?)
            .and_then(|n| n.checked_add(4))
            .ok_or(Error::Overflow),
        Operation::GlobalPhase { controls, .. } => control_bytes(controls.len()),
        Operation::Barrier { qubits } => targets_bytes(qubits.len()),
        Operation::Numerical {
            matrix, controls, ..
        } => {
            let width = matrix
                .num_qubits()
                .checked_add(controls.len())
                .ok_or(Error::Overflow)?;
            let dimension = 1usize
                .checked_shl(u32::try_from(width.max(1)).map_err(|_| Error::Overflow)?)
                .ok_or(Error::Overflow)?;
            let entries = if matrix.is_diagonal() {
                dimension
            } else {
                dimension.checked_mul(dimension).ok_or(Error::Overflow)?
            };
            bytes_for(entries, if gpu { 16 } else { 12 })?
                .checked_add(matrix.bytes())
                .ok_or(Error::Overflow)?
                .checked_add(targets_bytes(width)?)
                .ok_or(Error::Overflow)
        }
        Operation::Channel { kraus, .. } => {
            let d = kraus
                .first()
                .ok_or(Error::Value("empty Kraus channel"))?
                .dimension()
                .max(2);
            let d2 = d.checked_mul(d).ok_or(Error::Overflow)?;
            let elements = d2
                .checked_mul(d2)
                .and_then(|n| n.checked_mul(if gpu { 8 } else { 4 }))
                .and_then(|n| {
                    d2.checked_mul(kraus.len())
                        .and_then(|m| m.checked_mul(6))
                        .and_then(|m| n.checked_add(m))
                })
                .ok_or(Error::Overflow)?;
            bytes_for(elements, 1)
        }
        Operation::Reset { .. } => bytes_for(256, 1),
        Operation::Conditional { operation, .. } => estimate_native(operation, gpu)?
            .checked_add(std::mem::size_of::<PreparedOp>())
            .ok_or(Error::Overflow),
        Operation::Measure { .. } => Ok(0),
    }
}
#[expect(
    clippy::too_many_lines,
    reason = "Preparation owns the transactional native allocation and cache publication for every operation"
)]
fn prepare_operation(
    op: &Operation,
    matrices: &mut Vec<NativeMatrix>,
    channels: &mut Vec<UniquePtr<quest_sys::KrausMap>>,
    cache: &mut BTreeMap<(usize, Vec<bool>), usize>,
    inventory: &OracleInventory,
) -> Result<PreparedOp> {
    match op {
        Operation::Oracle {
            fragment,
            targets,
            controls,
        } => Ok(PreparedOp::Oracle {
            body: inventory.index(fragment)?,
            targets: targets.iter().map(|target| target.index()).collect(),
            controls: controls
                .iter()
                .map(|control| quest_circuit::language::vm::QuantumControl {
                    qubit: control.qubit().index(),
                    positive: control.state() == ControlState::One,
                })
                .collect(),
            adjoint: fragment.is_adjoint(),
        }),
        Operation::Numerical {
            matrix: numerical,
            targets,
            controls,
        } => {
            let key = (
                numerical.view().as_ptr().addr(),
                controls
                    .iter()
                    .map(|c| c.state() == ControlState::One)
                    .collect::<Vec<_>>(),
            );
            let mut native_targets: Vec<i32> = targets
                .iter()
                .map(|q| i32::try_from(q.index()).map_err(|_| Error::Overflow))
                .collect::<Result<_>>()?;
            for control in controls.iter() {
                native_targets
                    .push(i32::try_from(control.qubit().index()).map_err(|_| Error::Overflow)?);
            }
            if native_targets.is_empty() {
                native_targets.push(0);
            }
            let index = if let Some(&index) = cache.get(&key) {
                index
            } else {
                let native = prepare_numerical(numerical, &key.1)?;
                let index = matrices.len();
                matrices.push(native);
                cache.insert(key, index);
                index
            };
            Ok(PreparedOp::Numerical {
                cache: index,
                targets: native_targets,
            })
        }
        Operation::Channel { kraus, targets } => {
            let dim = kraus
                .first()
                .ok_or(Error::Value("empty Kraus channel"))?
                .dimension()
                .max(2);
            let mut values = reserve_vec(
                dim.checked_mul(dim)
                    .and_then(|n| n.checked_mul(kraus.len()))
                    .ok_or(Error::Overflow)?,
            )?;
            for matrix in kraus.iter() {
                for r in 0..dim {
                    for c in 0..dim {
                        let v = if matrix.dimension() == 1 {
                            if r == c {
                                matrix.view()[(0, 0)]
                            } else {
                                Complex64::new(0., 0.)
                            }
                        } else {
                            matrix.view()[(r, c)]
                        };
                        values.push(quest_sys::QuestComplex { re: v.re, im: v.im });
                    }
                }
            }
            let mut map = quest_sys::create_kraus_map(
                i32::try_from(dim.ilog2()).map_err(|_| Error::Overflow)?,
                i32::try_from(kraus.len()).map_err(|_| Error::Overflow)?,
            )
            .context("allocating Kraus channel")?;
            quest_sys::set_kraus_map_flat(
                map.pin_mut(),
                &values,
                i32::try_from(kraus.len()).map_err(|_| Error::Overflow)?,
                i64::try_from(dim).map_err(|_| Error::Overflow)?,
            )
            .context("transferring Kraus channel")?;
            let index = channels.len();
            channels.push(map);
            Ok(PreparedOp::Channel {
                cache: index,
                targets: if targets.is_empty() {
                    vec![0]
                } else {
                    targets
                        .iter()
                        .map(|q| i32::try_from(q.index()).map_err(|_| Error::Overflow))
                        .collect::<Result<_>>()?
                },
            })
        }
        Operation::Reset { qubit } => {
            let map = reset_channel()?;
            let index = channels.len();
            channels.push(map);
            Ok(PreparedOp::Reset {
                target: qubit.index(),
                cache: index,
            })
        }
        Operation::Conditional {
            bit,
            expected,
            operation,
        } => Ok(PreparedOp::Conditional {
            bit: bit.index(),
            expected: *expected,
            operation: Box::new(prepare_operation(
                operation, matrices, channels, cache, inventory,
            )?),
        }),
        Operation::Gate {
            gate,
            targets,
            controls,
        } => {
            let targets = targets
                .iter()
                .map(|q| i32::try_from(q.index()).map_err(|_| Error::Overflow))
                .collect::<Result<Vec<_>>>()?;
            let controls = NativeControls::new(controls, targets.first().copied())?;
            Ok(PreparedOp::Gate {
                gate: gate.clone(),
                targets,
                controls,
            })
        }
        Operation::GlobalPhase { radians, controls } => Ok(PreparedOp::GlobalPhase {
            radians: *radians,
            controls: NativeControls::new(controls, None)?,
        }),
        Operation::Measure { qubit, bit } => Ok(PreparedOp::Measure {
            qubit: qubit.index(),
            bit: bit.index(),
        }),
        Operation::Barrier { .. } => Ok(PreparedOp::Barrier),
    }
}
pub fn prepare_numerical(
    numerical: &quest_circuit::NumericalOperator,
    controls: &[bool],
) -> Result<NativeMatrix> {
    let dim = numerical
        .dimension()
        .checked_shl(u32::try_from(controls.len()).map_err(|_| Error::Overflow)?)
        .ok_or(Error::Overflow)?
        .max(2);
    let active = controls
        .iter()
        .enumerate()
        .fold(0usize, |mask, (i, c)| mask | usize::from(*c) << i);
    let local_dim = numerical.dimension();
    let divisor =
        std::num::NonZeroUsize::new(local_dim).ok_or(Error::Value("empty numerical matrix"))?;
    let value = |row, col| {
        if local_dim == 1 && controls.is_empty() {
            if row == col {
                numerical.view()[(0, 0)]
            } else {
                Complex64::new(0.0, 0.0)
            }
        } else if row / divisor == active && col / divisor == active {
            numerical.view()[(row % divisor, col % divisor)]
        } else if row == col {
            Complex64::new(1.0, 0.0)
        } else {
            Complex64::new(0.0, 0.0)
        }
    };
    if numerical.is_diagonal() {
        let mut values = reserve_vec(dim)?;
        for row in 0..dim {
            let v = value(row, row);
            values.push(quest_sys::QuestComplex { re: v.re, im: v.im });
        }
        let width = i32::try_from(dim.ilog2()).map_err(|_| Error::Overflow)?;
        let mut forward =
            quest_sys::create_diag_matr(width).context("allocating native diagonal")?;
        quest_sys::set_diag_matr(forward.pin_mut(), &values)
            .context("transferring native diagonal")?;
        for value in &mut values {
            value.im = -value.im;
        }
        let mut adjoint =
            quest_sys::create_diag_matr(width).context("allocating diagonal adjoint")?;
        quest_sys::set_diag_matr(adjoint.pin_mut(), &values)
            .context("transferring diagonal adjoint")?;
        return Ok(NativeMatrix::Diagonal { forward, adjoint });
    }
    let mut extended = matrix(dim, dim)?;
    for row in 0..dim {
        for col in 0..dim {
            extended[(row, col)] = value(row, col);
        }
    }
    Ok(NativeMatrix::Dense {
        forward: native_matrix(extended.as_ref())?,
        adjoint: native_matrix(extended.as_ref().adjoint())?,
    })
}
pub fn execute_matrix<K: RegisterKind>(
    matrix: &NativeMatrix,
    register: &mut Register<'_, K>,
    targets: &[i32],
    inverse: bool,
) -> Result<()> {
    match matrix {
        NativeMatrix::Dense { forward, adjoint } => {
            let (forward, adjoint) = if inverse {
                (adjoint, forward)
            } else {
                (forward, adjoint)
            };
            quest_sys::leftapply_comp_matr(register.pin(), targets, forward)
                .context("applying numerical operator")?;
            if register.is_density() {
                quest_sys::rightapply_comp_matr(register.pin(), targets, adjoint)
                    .context("applying numerical adjoint to density")?;
            }
        }
        NativeMatrix::Diagonal { forward, adjoint } => {
            let (forward, adjoint) = if inverse {
                (adjoint, forward)
            } else {
                (forward, adjoint)
            };
            quest_sys::leftapply_diag_matr(register.pin(), targets, forward)
                .context("applying diagonal operator")?;
            if register.is_density() {
                quest_sys::rightapply_diag_matr(register.pin(), targets, adjoint)
                    .context("applying diagonal adjoint to density")?;
            }
        }
    }
    Ok(())
}
pub fn reset_channel() -> Result<UniquePtr<quest_sys::KrausMap>> {
    let zero = quest_sys::QuestComplex { re: 0., im: 0. };
    let one = quest_sys::QuestComplex { re: 1., im: 0. };
    let mut map = quest_sys::create_kraus_map(1, 2).context("allocating density reset channel")?;
    quest_sys::set_kraus_map_flat(
        map.pin_mut(),
        &[one, zero, zero, zero, zero, one, zero, zero],
        2,
        2,
    )
    .context("preparing reset channel")?;
    Ok(map)
}
pub fn native_matrix<T: faer::traits::Conjugate<Canonical = Complex64>>(
    view: faer::MatRef<'_, T>,
) -> Result<UniquePtr<quest_sys::CompMatr>> {
    let values = crate::register::pack_matrix(view)?;
    let mut native = quest_sys::create_comp_matr(
        i32::try_from(view.nrows().ilog2()).map_err(|_| Error::Overflow)?,
    )
    .context("allocating native matrix")?;
    quest_sys::set_comp_matr_flat(
        native.pin_mut(),
        &values,
        i64::try_from(view.nrows()).map_err(|_| Error::Overflow)?,
    )
    .context("transferring row-major matrix")?;
    Ok(native)
}
fn execute<K: RegisterKind>(
    op: &PreparedOp,
    register: &mut Register<'_, K>,
    bits: &mut [bool],
    matrices: &[NativeMatrix],
    channels: &[UniquePtr<quest_sys::KrausMap>],
    oracles: &mut OracleCache,
) -> Result<()> {
    match op {
        PreparedOp::Oracle {
            body,
            targets,
            controls,
            adjoint,
        } => oracles.run(*body, targets, controls, *adjoint, register),
        PreparedOp::Numerical { cache, targets } => execute_matrix(
            matrices
                .get(*cache)
                .ok_or(Error::Value("invalid prepared matrix cache"))?,
            register,
            targets,
            false,
        ),
        PreparedOp::Channel { cache, targets } => quest_sys::mix_kraus_map(
            register.pin(),
            targets,
            channels
                .get(*cache)
                .ok_or(Error::Value("invalid prepared channel cache"))?,
        )
        .context("applying density channel"),
        PreparedOp::Reset { target, cache } => {
            if register.is_density() {
                quest_sys::mix_kraus_map(
                    register.pin(),
                    &[i32::try_from(*target).map_err(|_| Error::Overflow)?],
                    channels
                        .get(*cache)
                        .ok_or(Error::Value("invalid prepared channel cache"))?,
                )
                .context("resetting density qubit")
            } else {
                if register.measure(*target)? == Outcome::One {
                    register.x(*target)?;
                }
                Ok(())
            }
        }
        PreparedOp::Conditional {
            bit,
            expected,
            operation,
        } => {
            if *bits.get(*bit).ok_or(Error::Value("invalid prepared bit"))? == *expected {
                execute(operation, register, bits, matrices, channels, oracles)?;
            }
            Ok(())
        }
        PreparedOp::Gate {
            gate,
            targets,
            controls,
        } => apply_gate(register, gate, targets, controls),
        PreparedOp::GlobalPhase { radians, controls } => phase(register, *radians, controls),
        PreparedOp::Measure { qubit, bit } => {
            *bits
                .get_mut(*bit)
                .ok_or(Error::Value("invalid prepared bit"))? = register.measure(*qubit)?.as_bool();
            Ok(())
        }
        PreparedOp::Barrier => Ok(()),
    }
}

pub fn phase<K: RegisterKind>(
    register: &mut Register<'_, K>,
    angle: f64,
    controls: &NativeControls,
) -> Result<()> {
    if controls.wires.is_empty() {
        return quest_sys::apply_global_phase(register.pin(), angle)
            .context("applying global phase");
    }
    for &control in &controls.zeros {
        register.x(control)?;
    }
    quest_sys::apply_multi_qubit_phase_shift(register.pin(), &controls.wires, angle)
        .context("applying controlled phase")?;
    for &control in controls.zeros.iter().rev() {
        register.x(control)?;
    }
    Ok(())
}
#[expect(
    clippy::too_many_lines,
    reason = "Exhaustive native gate dispatch keeps each controlled and uncontrolled mapping adjacent"
)]
pub fn apply_gate<K: RegisterKind>(
    register: &mut Register<'_, K>,
    gate: &BoundGate,
    targets: &[i32],
    controls: &NativeControls,
) -> Result<()> {
    let qs = &controls.wires;
    let states = &controls.states;
    let t = *targets.first().ok_or(Error::Value("missing gate target"))?;
    let result = match gate {
        BoundGate::Id => return Ok(()),
        BoundGate::H if controls.wires.is_empty() => quest_sys::apply_hadamard(register.pin(), t),
        BoundGate::H => {
            quest_sys::apply_multi_state_controlled_hadamard(register.pin(), qs, states, t)
        }
        BoundGate::X if controls.wires.is_empty() => quest_sys::apply_pauli_x(register.pin(), t),
        BoundGate::X => {
            quest_sys::apply_multi_state_controlled_pauli_x(register.pin(), qs, states, t)
        }
        BoundGate::Y if controls.wires.is_empty() => quest_sys::apply_pauli_y(register.pin(), t),
        BoundGate::Y => {
            quest_sys::apply_multi_state_controlled_pauli_y(register.pin(), qs, states, t)
        }
        BoundGate::Z if controls.wires.is_empty() => quest_sys::apply_pauli_z(register.pin(), t),
        BoundGate::Z => {
            quest_sys::apply_multi_state_controlled_pauli_z(register.pin(), qs, states, t)
        }
        BoundGate::Rx(a) if controls.wires.is_empty() => {
            quest_sys::apply_rotate_x(register.pin(), t, *a)
        }
        BoundGate::Rx(a) => {
            quest_sys::apply_multi_state_controlled_rotate_x(register.pin(), qs, states, t, *a)
        }
        BoundGate::Ry(a) if controls.wires.is_empty() => {
            quest_sys::apply_rotate_y(register.pin(), t, *a)
        }
        BoundGate::Ry(a) => {
            quest_sys::apply_multi_state_controlled_rotate_y(register.pin(), qs, states, t, *a)
        }
        BoundGate::Rz(a) if controls.wires.is_empty() => {
            quest_sys::apply_rotate_z(register.pin(), t, *a)
        }
        BoundGate::Rz(a) => {
            quest_sys::apply_multi_state_controlled_rotate_z(register.pin(), qs, states, t, *a)
        }
        BoundGate::S => {
            return apply_gate(
                register,
                &BoundGate::Phase(std::f64::consts::FRAC_PI_2),
                targets,
                controls,
            );
        }
        BoundGate::Sdg => {
            return apply_gate(
                register,
                &BoundGate::Phase(-std::f64::consts::FRAC_PI_2),
                targets,
                controls,
            );
        }
        BoundGate::T => {
            return apply_gate(
                register,
                &BoundGate::Phase(std::f64::consts::FRAC_PI_4),
                targets,
                controls,
            );
        }
        BoundGate::Tdg => {
            return apply_gate(
                register,
                &BoundGate::Phase(-std::f64::consts::FRAC_PI_4),
                targets,
                controls,
            );
        }
        BoundGate::Phase(angle) => {
            for &control in &controls.zeros {
                register.x(control)?;
            }
            quest_sys::apply_multi_qubit_phase_shift(
                register.pin(),
                &controls.phase_targets,
                *angle,
            )
            .context("applying phase gate")?;
            for &control in controls.zeros.iter().rev() {
                register.x(control)?;
            }
            return Ok(());
        }
        BoundGate::Swap if controls.wires.is_empty() => quest_sys::apply_swap(
            register.pin(),
            t,
            *targets.get(1).ok_or(Error::Value("missing swap target"))?,
        ),
        BoundGate::Swap => quest_sys::apply_multi_state_controlled_swap(
            register.pin(),
            qs,
            states,
            t,
            *targets.get(1).ok_or(Error::Value("missing swap target"))?,
        ),
        BoundGate::Sxdg => {
            apply_gate(
                register,
                &BoundGate::Rx(-std::f64::consts::FRAC_PI_2),
                targets,
                controls,
            )?;
            return phase(register, -std::f64::consts::FRAC_PI_4, controls);
        }
        BoundGate::Sx => {
            apply_gate(
                register,
                &BoundGate::Rx(std::f64::consts::FRAC_PI_2),
                targets,
                controls,
            )?;
            return phase(register, std::f64::consts::FRAC_PI_4, controls);
        }
        BoundGate::U { theta, phi, lambda } => {
            apply_gate(register, &BoundGate::Phase(*lambda), targets, controls)?;
            apply_gate(register, &BoundGate::Ry(*theta), targets, controls)?;
            apply_gate(register, &BoundGate::Phase(*phi), targets, controls)?;
            return phase(register, theta / 2.0, controls);
        }
    };
    result.context("applying standard gate")
}
