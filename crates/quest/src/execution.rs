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

struct NativeMatrix {
    forward: UniquePtr<quest_sys::CompMatr>,
    adjoint: UniquePtr<quest_sys::CompMatr>,
}
struct NativeControls {
    wires: Vec<i32>,
    states: Vec<i32>,
    zeros: Vec<usize>,
    phase_targets: Vec<i32>,
}
impl NativeControls {
    fn new(controls: &[Control], target: Option<i32>) -> Result<Self> {
        let mut wires = reserve_vec(controls.len())?;
        let mut states = reserve_vec(controls.len())?;
        let mut zeros = reserve_vec(controls.len())?;
        let mut phase_targets = reserve_vec(
            controls
                .len()
                .checked_add(usize::from(target.is_some()))
                .ok_or(Error::Overflow)?,
        )?;
        for control in controls {
            let qubit = control.qubit().index();
            wires.push(qubit as i32);
            states.push(i32::from(control.state() == ControlState::One));
            phase_targets.push(qubit as i32);
            if control.state() == ControlState::Zero {
                zeros.push(qubit);
            }
        }
        if let Some(target) = target {
            phase_targets.push(target);
        }
        Ok(Self {
            wires,
            states,
            zeros,
            phase_targets,
        })
    }
}
enum PreparedOp {
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
    pub fn prepare(&self, program: ValidatedProgram) -> Result<PreparedProgram<'_>> {
        self.prepare_plan(program.bind(&[])?.lower()?.plan()?)
    }
    pub fn prepare_plan(&self, plan: ExecutablePlan) -> Result<PreparedProgram<'_>> {
        QubitCount::new(plan.num_qubits())?;
        let fingerprint =
            quest_sys::get_numerical_fingerprint().context("checking numerical environment")?;
        admit_fingerprint(&fingerprint)?;
        let mut required = plan
            .instructions()
            .len()
            .checked_mul(
                std::mem::size_of::<quest_circuit::Instruction>()
                    + std::mem::size_of::<PreparedOp>()
                    + std::mem::size_of::<NativeMatrix>()
                    + std::mem::size_of::<UniquePtr<quest_sys::KrausMap>>(),
            )
            .ok_or(Error::Overflow)?;
        required = required
            .checked_add(plan.num_bits())
            .ok_or(Error::Overflow)?;
        for instruction in plan.instructions() {
            required = required
                .checked_add(
                    instruction
                        .provenance()
                        .len()
                        .checked_mul(std::mem::size_of::<quest_circuit::OccurrenceId>())
                        .ok_or(Error::Overflow)?,
                )
                .ok_or(Error::Overflow)?;
            if let Some(span) = instruction.source() {
                required = required
                    .checked_add(span.source().len())
                    .ok_or(Error::Overflow)?;
            }
            required = required
                .checked_add(estimate(instruction.operation(), self.capabilities().gpu)?)
                .ok_or(Error::Overflow)?;
        }
        let reservation = self.reserve(required)?;
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
            )?);
        }
        // Locals release native handles on any error before the prepared owner is published.
        Ok(PreparedProgram {
            matrices,
            channels,
            reservation,
            plan,
            operations,
            fingerprint,
        })
    }
}
impl PreparedProgram<'_> {
    pub fn plan(&self) -> &ExecutablePlan {
        &self.plan
    }
    pub fn run<K: RegisterKind>(&mut self, register: &mut Register<'_, K>) -> Result<RunResult> {
        if !std::ptr::eq(register.environment(), self.reservation.environment)
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
        for (index, operation) in self.operations.iter().enumerate() {
            execute(
                operation,
                register,
                &mut bits,
                &self.matrices,
                &self.channels,
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
    /// Seeds QuEST's process RNG once per batch and restores |0...0> for every shot.
    /// Supply 1–16 seeds; native RNG storage retains a 4096-byte budget allowance.
    /// Density channels use exact density evolution; state vectors use reset trajectories.
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
            .checked_mul(std::mem::size_of::<u32>() * 8)
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
        let mut counts = BTreeMap::new();
        let count = QubitCount::new(self.plan.num_qubits())?;
        if self.operations.iter().any(requires_density) {
            let mut register = environment.density_matrix(count)?;
            for _ in 0..shots.get() {
                register.init_zero()?;
                *counts.entry(self.run(&mut register)?.bits).or_insert(0) += 1;
            }
        } else {
            let mut register = environment.state_vector(count)?;
            for _ in 0..shots.get() {
                register.init_zero()?;
                *counts.entry(self.run(&mut register)?.bits).or_insert(0) += 1;
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
fn admit_fingerprint(fp: &quest_sys::NumericalFingerprint) -> Result<()> {
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
    let control_bytes = |count: usize| {
        count
            .checked_mul(
                std::mem::size_of::<Control>()
                    + 3 * std::mem::size_of::<i32>()
                    + std::mem::size_of::<usize>(),
            )
            .ok_or(Error::Overflow)
    };
    let targets_bytes = |count: usize| {
        count
            .checked_mul(std::mem::size_of::<quest_circuit::QubitId>() + std::mem::size_of::<i32>())
            .ok_or(Error::Overflow)
    };
    match op {
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
            let entries = dimension.checked_mul(dimension).ok_or(Error::Overflow)?;
            bytes_for(entries, if gpu { 16 } else { 12 })?
                .checked_add(targets_bytes(width)?)
                .ok_or(Error::Overflow)
        }
        Operation::Channel { kraus, .. } => {
            let d = kraus[0].dimension().max(2);
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
        Operation::Conditional { operation, .. } => estimate(operation, gpu)?
            .checked_add(std::mem::size_of::<PreparedOp>())
            .ok_or(Error::Overflow),
        _ => Ok(0),
    }
}
fn prepare_operation(
    op: &Operation,
    matrices: &mut Vec<NativeMatrix>,
    channels: &mut Vec<UniquePtr<quest_sys::KrausMap>>,
    cache: &mut BTreeMap<(usize, Vec<bool>), usize>,
) -> Result<PreparedOp> {
    match op {
        Operation::Numerical {
            matrix: numerical,
            targets,
            controls,
        } => {
            let key = (
                numerical.view().as_ptr() as usize,
                controls
                    .iter()
                    .map(|c| c.state() == ControlState::One)
                    .collect::<Vec<_>>(),
            );
            let mut native_targets: Vec<i32> = targets.iter().map(|q| q.index() as i32).collect();
            native_targets.extend(controls.iter().map(|c| c.qubit().index() as i32));
            if native_targets.is_empty() {
                native_targets.push(0);
            }
            let index = if let Some(&index) = cache.get(&key) {
                index
            } else {
                let dim = numerical
                    .dimension()
                    .checked_shl(controls.len() as u32)
                    .ok_or(Error::Overflow)?
                    .max(2);
                let mut extended = matrix(dim, dim)?;
                let active = controls.iter().enumerate().fold(0usize, |mask, (i, c)| {
                    mask | ((c.state() == ControlState::One) as usize) << i
                });
                let local_dim = numerical.dimension();
                for row in 0..dim {
                    for col in 0..dim {
                        extended[(row, col)] = if local_dim == 1 && controls.is_empty() {
                            if row == col {
                                numerical.view()[(0, 0)]
                            } else {
                                Complex64::new(0., 0.)
                            }
                        } else if row / local_dim == active && col / local_dim == active {
                            numerical.view()[(row % local_dim, col % local_dim)]
                        } else if row == col {
                            Complex64::new(1., 0.)
                        } else {
                            Complex64::new(0., 0.)
                        };
                    }
                }
                let forward = native_matrix(extended.as_ref())?;
                let adjoint = native_matrix(extended.as_ref().adjoint())?;
                let index = matrices.len();
                matrices.push(NativeMatrix { forward, adjoint });
                cache.insert(key, index);
                index
            };
            Ok(PreparedOp::Numerical {
                cache: index,
                targets: native_targets,
            })
        }
        Operation::Channel { kraus, targets } => {
            let dim = kraus[0].dimension().max(2);
            let mut values = reserve_vec(
                dim.checked_mul(dim)
                    .and_then(|n| n.checked_mul(kraus.len()))
                    .ok_or(Error::Overflow)?,
            )?;
            for matrix in kraus {
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
                dim.ilog2() as i32,
                i32::try_from(kraus.len()).map_err(|_| Error::Overflow)?,
            )
            .context("allocating Kraus channel")?;
            quest_sys::set_kraus_map_flat(map.pin_mut(), &values, kraus.len() as i32, dim as i64)
                .context("transferring Kraus channel")?;
            let index = channels.len();
            channels.push(map);
            Ok(PreparedOp::Channel {
                cache: index,
                targets: if targets.is_empty() {
                    vec![0]
                } else {
                    targets.iter().map(|q| q.index() as i32).collect()
                },
            })
        }
        Operation::Reset { qubit } => {
            let zero = quest_sys::QuestComplex { re: 0., im: 0. };
            let one = quest_sys::QuestComplex { re: 1., im: 0. };
            let mut map =
                quest_sys::create_kraus_map(1, 2).context("allocating density reset channel")?;
            quest_sys::set_kraus_map_flat(
                map.pin_mut(),
                &[one, zero, zero, zero, zero, one, zero, zero],
                2,
                2,
            )
            .context("preparing reset channel")?;
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
            operation: Box::new(prepare_operation(operation, matrices, channels, cache)?),
        }),
        Operation::Gate {
            gate,
            targets,
            controls,
        } => {
            let targets = targets.iter().map(|q| q.index() as i32).collect::<Vec<_>>();
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
fn native_matrix<T: faer::traits::Conjugate<Canonical = Complex64>>(
    view: faer::MatRef<'_, T>,
) -> Result<UniquePtr<quest_sys::CompMatr>> {
    let mut values = reserve_vec(
        view.nrows()
            .checked_mul(view.ncols())
            .ok_or(Error::Overflow)?,
    )?;
    for row in 0..view.nrows() {
        for col in 0..view.ncols() {
            let v = crate::register::logical(view, row, col);
            values.push(quest_sys::QuestComplex { re: v.re, im: v.im });
        }
    }
    let mut native = quest_sys::create_comp_matr(view.nrows().ilog2() as i32)
        .context("allocating native matrix")?;
    quest_sys::set_comp_matr_flat(native.pin_mut(), &values, view.nrows() as i64)
        .context("transferring row-major matrix")?;
    Ok(native)
}
fn execute<K: RegisterKind>(
    op: &PreparedOp,
    register: &mut Register<'_, K>,
    bits: &mut [bool],
    matrices: &[NativeMatrix],
    channels: &[UniquePtr<quest_sys::KrausMap>],
) -> Result<()> {
    match op {
        PreparedOp::Numerical { cache, targets } => {
            quest_sys::leftapply_comp_matr(register.pin(), targets, &matrices[*cache].forward)
                .context("applying numerical operator")?;
            if register.is_density() {
                quest_sys::rightapply_comp_matr(register.pin(), targets, &matrices[*cache].adjoint)
                    .context("applying numerical adjoint to density")?;
            }
            Ok(())
        }
        PreparedOp::Channel { cache, targets } => {
            quest_sys::mix_kraus_map(register.pin(), targets, &channels[*cache])
                .context("applying density channel")
        }
        PreparedOp::Reset { target, cache } => {
            if register.is_density() {
                quest_sys::mix_kraus_map(register.pin(), &[*target as i32], &channels[*cache])
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
            if bits[*bit] == *expected {
                execute(operation, register, bits, matrices, channels)?;
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
            bits[*bit] = register.measure(*qubit)?.as_bool();
            Ok(())
        }
        PreparedOp::Barrier => Ok(()),
    }
}

fn phase<K: RegisterKind>(
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
fn apply_gate<K: RegisterKind>(
    register: &mut Register<'_, K>,
    gate: &BoundGate,
    targets: &[i32],
    controls: &NativeControls,
) -> Result<()> {
    let qs = &controls.wires;
    let states = &controls.states;
    let t = targets[0];
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
        BoundGate::S | BoundGate::Sdg | BoundGate::T | BoundGate::Tdg | BoundGate::Phase(_) => {
            let angle = match gate {
                BoundGate::S => std::f64::consts::FRAC_PI_2,
                BoundGate::Sdg => -std::f64::consts::FRAC_PI_2,
                BoundGate::T => std::f64::consts::FRAC_PI_4,
                BoundGate::Tdg => -std::f64::consts::FRAC_PI_4,
                BoundGate::Phase(a) => *a,
                _ => unreachable!(),
            };
            for &control in &controls.zeros {
                register.x(control)?;
            }
            quest_sys::apply_multi_qubit_phase_shift(
                register.pin(),
                &controls.phase_targets,
                angle,
            )
            .context("applying phase gate")?;
            for &control in controls.zeros.iter().rev() {
                register.x(control)?;
            }
            return Ok(());
        }
        BoundGate::Swap if controls.wires.is_empty() => {
            quest_sys::apply_swap(register.pin(), t, targets[1])
        }
        BoundGate::Swap => {
            quest_sys::apply_multi_state_controlled_swap(register.pin(), qs, states, t, targets[1])
        }
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
            return apply_gate(register, &BoundGate::Phase(*phi), targets, controls);
        }
    };
    result.context("applying standard gate")
}
