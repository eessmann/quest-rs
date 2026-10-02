#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
use crate::oracle_execution::{OracleCache, OracleInventory};
use crate::{Complex64, Error, Result};
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
use crate::{Outcome, QubitCount, environment::Reservation};
use crate::{
    error::BackendResult,
    register::{Register, RegisterKind, matrix},
    values::{bytes_for, reserve_vec},
};
use cxx::UniquePtr;
use quest_compile::{
    BoundGate,
    dispatch_recipe::{self, DispatchStep, MatrixRecipe, PrimitiveGate},
};
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
use quest_compile::{Control, ControlState, Operation, RegionPlan};
use std::collections::{BTreeMap, BTreeSet};

pub type MatrixCacheKey = (usize, Vec<bool>);

pub fn matrix_cache_key(
    matrix: &quest_compile::NumericalOperator,
    controls: impl IntoIterator<Item = bool>,
) -> MatrixCacheKey {
    (
        matrix.view().as_ptr().addr(),
        controls.into_iter().collect(),
    )
}

/// One transactional native pool per prepared owner. Keys use retained immutable
/// source identity and the ordered signed-control profile, never target wires.
#[derive(Default)]
pub struct MatrixPreparation {
    matrices: Vec<NativeMatrix>,
    indices: BTreeMap<MatrixCacheKey, usize>,
}
impl MatrixPreparation {
    pub fn include(
        &mut self,
        matrix: &quest_compile::NumericalOperator,
        controls: &[bool],
    ) -> Result<usize> {
        let key = matrix_cache_key(matrix, controls.iter().copied());
        if let Some(&index) = self.indices.get(&key) {
            return Ok(index);
        }
        self.matrices
            .try_reserve(1)
            .map_err(|_| Error::Allocation)?;
        // Publish only after both native variants have been created successfully.
        let native = prepare_numerical(matrix, controls)?;
        let index = self.matrices.len();
        self.matrices.push(native);
        self.indices.insert(key, index);
        Ok(index)
    }
    pub fn finish(self) -> Vec<NativeMatrix> {
        self.matrices
    }
}

/// Size the same signed-control embedding that materialization uses. One
/// admission set spans ordinary payloads and every reachable oracle profile.
pub fn admit_matrix(
    matrix: &quest_compile::NumericalOperator,
    controls: &[bool],
    gpu: bool,
    seen: &mut BTreeSet<MatrixCacheKey>,
) -> Result<usize> {
    let recipe = MatrixRecipe::new(matrix, controls)?;
    let dimension = recipe.dimension();
    let entries = if recipe.is_diagonal() {
        dimension
    } else {
        dimension.checked_mul(dimension).ok_or(Error::Overflow)?
    };
    let key_storage = controls
        .len()
        .checked_add(
            const {
                size_of::<MatrixCacheKey>()
                    + 2 * size_of::<NativeMatrix>()
                    + 12 * size_of::<usize>()
                    + 256
            },
        )
        .ok_or(Error::Overflow)?;
    let bytes = bytes_for(entries, if gpu { 16 } else { 12 })?
        .checked_add(matrix.bytes())
        .and_then(|bytes| bytes.checked_add(key_storage))
        .ok_or(Error::Overflow)?;
    // Check recipe sizing even for aliases; an invalid resource never shortcuts admission.
    if seen.insert(matrix_cache_key(matrix, controls.iter().copied())) {
        Ok(bytes)
    } else {
        Ok(0)
    }
}

pub fn kraus_bytes(kraus: &[quest_compile::NumericalOperator], gpu: bool) -> Result<usize> {
    let dimension = kraus
        .first()
        .ok_or(Error::Value("empty Kraus channel"))?
        .dimension()
        .max(2);
    let square = dimension.checked_mul(dimension).ok_or(Error::Overflow)?;
    let count = square
        .checked_mul(square)
        .and_then(|n| n.checked_mul(if gpu { 8 } else { 4 }))
        .and_then(|n| {
            square
                .checked_mul(kraus.len())
                .and_then(|m| m.checked_mul(6))
                .and_then(|m| n.checked_add(m))
        })
        .ok_or(Error::Overflow)?;
    bytes_for(count, 1)
}

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
    #[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
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
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
enum PreparedOp {
    Oracle {
        body: usize,
        targets: Vec<usize>,
        controls: Vec<quest_compile::language::vm::QuantumControl>,
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
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
pub struct PreparedRegion<'env> {
    matrices: Vec<NativeMatrix>,
    oracles: OracleCache,
    channels: Vec<UniquePtr<quest_sys::KrausMap>>,
    reservation: Reservation<'env>,
    plan: RegionPlan,
    operations: Vec<PreparedOp>,
    fingerprint: quest_sys::NumericalFingerprint,
}
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
impl crate::environment::RuntimeResources {
    pub(crate) fn admit_plan(&self, plan: RegionPlan) -> Result<AdmittedPlan<'_>> {
        QubitCount::new(plan.num_qubits())?;
        let fingerprint =
            quest_sys::get_numerical_fingerprint().context("checking numerical environment")?;
        admit_fingerprint(&fingerprint)?;
        let mut inventory = OracleInventory::with_budget(
            self.memory_budget()
                .bytes()
                .saturating_sub(self.allocated_bytes()),
        );
        let mut seen_matrices = BTreeSet::new();
        for instruction in plan.instructions() {
            inventory.include_operation(instruction.operation(), plan.num_qubits())?;
        }
        let mut required = plan
            .instructions()
            .len()
            .checked_mul(
                const {
                    std::mem::size_of::<quest_compile::Instruction>()
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
            required = required
                .checked_add(instruction.angle_target_storage_bytes()?)
                .ok_or(Error::Overflow)?;
            if let Some(span) = instruction.source() {
                required = required
                    .checked_add(span.source().len())
                    .ok_or(Error::Overflow)?;
            }
            required = required
                .checked_add(estimate(
                    instruction.operation(),
                    self.capabilities().gpu,
                    &mut seen_matrices,
                )?)
                .ok_or(Error::Overflow)?;
        }
        required = required
            .checked_add(inventory.estimated_bytes(
                plan.num_qubits(),
                self.capabilities().gpu,
                &mut seen_matrices,
            )?)
            .ok_or(Error::Overflow)?;
        // Admission keys are temporary; the reserved key allowance also covers
        // the materialization cache after this set is released.
        drop(seen_matrices);
        let reservation = self.reserve(required)?;
        Ok(AdmittedPlan {
            plan,
            fingerprint,
            inventory,
            reservation,
        })
    }
}
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
pub struct AdmittedPlan<'env> {
    plan: RegionPlan,
    fingerprint: quest_sys::NumericalFingerprint,
    inventory: OracleInventory,
    reservation: Reservation<'env>,
}
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
impl<'env> AdmittedPlan<'env> {
    #[cfg(feature = "qsvt")]
    pub(crate) fn dispatch_scratch(&self) -> Result<Reservation<'_>> {
        self.reservation
            .environment
            .reserve(self.inventory.dispatch_scratch_bytes()?)
    }
    #[cfg(feature = "qsvt")]
    pub(crate) const fn plan(&self) -> &RegionPlan {
        &self.plan
    }

    pub(crate) fn materialize(self) -> Result<PreparedRegion<'env>> {
        let Self {
            plan,
            fingerprint,
            inventory,
            reservation,
        } = self;
        let mut matrices = MatrixPreparation::default();
        let oracles = OracleCache::prepare(&inventory, plan.num_qubits(), &mut matrices)?;
        let mut channels = reserve_vec(plan.instructions().len())?;
        let mut operations = reserve_vec(plan.instructions().len())?;
        for instruction in plan.instructions() {
            operations.push(prepare_operation(
                instruction.operation(),
                &mut matrices,
                &mut channels,
                &inventory,
            )?);
        }
        // Locals release native handles on any error before the prepared owner is published.
        Ok(PreparedRegion {
            matrices: matrices.finish(),
            oracles,
            channels,
            reservation,
            plan,
            operations,
            fingerprint,
        })
    }
}
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
impl PreparedRegion<'_> {
    /// Distinct shared canonical oracle bodies retained in native preparation.
    #[must_use]
    #[cfg(all(feature = "mpi", quest_native_mpi))]
    pub const fn prepared_oracle_bodies(&self) -> usize {
        self.oracles.body_count()
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
    ) -> Result<()> {
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
        Ok(())
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
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
fn requires_density(op: &PreparedOp) -> bool {
    match op {
        PreparedOp::Channel { .. } => true,
        PreparedOp::Conditional { operation, .. } => requires_density(operation),
        _ => false,
    }
}
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
fn estimate(
    op: &Operation,
    gpu: bool,
    seen_matrices: &mut BTreeSet<MatrixCacheKey>,
) -> Result<usize> {
    op.operand_storage_bytes()?
        .checked_add(estimate_native(op, gpu, seen_matrices)?)
        .ok_or(Error::Overflow)
}
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
fn estimate_native(
    op: &Operation,
    gpu: bool,
    seen_matrices: &mut BTreeSet<MatrixCacheKey>,
) -> Result<usize> {
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
            let targets = targets_bytes(width.max(1))?;
            let profile = controls
                .iter()
                .map(|control| control.state() == ControlState::One)
                .collect::<Vec<_>>();
            admit_matrix(matrix, &profile, gpu, seen_matrices)?
                .checked_add(targets)
                .ok_or(Error::Overflow)
        }
        Operation::Channel { kraus, .. } => kraus_bytes(kraus, gpu),
        Operation::Reset { .. } => bytes_for(256, 1),
        Operation::Conditional { operation, .. } => estimate_native(operation, gpu, seen_matrices)?
            .checked_add(std::mem::size_of::<PreparedOp>())
            .ok_or(Error::Overflow),
        Operation::Measure { .. } => Ok(0),
    }
}
#[expect(
    clippy::too_many_lines,
    reason = "Preparation owns the transactional native allocation and cache publication for every operation"
)]
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
fn prepare_operation(
    op: &Operation,
    matrices: &mut MatrixPreparation,
    channels: &mut Vec<UniquePtr<quest_sys::KrausMap>>,
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
                .map(|control| quest_compile::language::vm::QuantumControl {
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
            let profile = controls
                .iter()
                .map(|c| c.state() == ControlState::One)
                .collect::<Vec<_>>();
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
            let index = matrices.include(numerical, &profile)?;
            Ok(PreparedOp::Numerical {
                cache: index,
                targets: native_targets,
            })
        }
        Operation::Channel { kraus, targets } => {
            let map = prepare_kraus(kraus)?;
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
            operation: Box::new(prepare_operation(operation, matrices, channels, inventory)?),
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
pub fn prepare_kraus(
    kraus: &[quest_compile::NumericalOperator],
) -> Result<UniquePtr<quest_sys::KrausMap>> {
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
    Ok(map)
}

pub fn prepare_numerical(
    numerical: &quest_compile::NumericalOperator,
    controls: &[bool],
) -> Result<NativeMatrix> {
    let recipe = MatrixRecipe::new(numerical, controls)?;
    let dim = recipe.dimension();
    if recipe.is_diagonal() {
        let mut values = reserve_vec(dim)?;
        for row in 0..dim {
            let v = recipe.value(row, row);
            values.push(quest_sys::QuestComplex { re: v.re, im: v.im });
        }
        let width = i32::try_from(dim.ilog2()).map_err(|_| Error::Overflow)?;
        let mut forward =
            quest_sys::create_diag_matr(width).context("allocating native diagonal")?;
        quest_sys::set_diag_matr(forward.pin_mut(), &values)
            .context("transferring native diagonal")?;
        for (row, value) in values.iter_mut().enumerate() {
            let adjoint = recipe.adjoint_value(row, row);
            *value = quest_sys::QuestComplex {
                re: adjoint.re,
                im: adjoint.im,
            };
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
            extended[(row, col)] = recipe.value(row, col);
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
#[cfg(any(feature = "qsvt", all(feature = "mpi", quest_native_mpi)))]
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
        } => oracles.run(*body, targets, controls, *adjoint, register, matrices),
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
    let recipe = dispatch_recipe::scalar_phase_recipe(angle, controls.zeros.len())?;
    for step in recipe.steps() {
        execute_step(register, step, &[], controls)?;
    }
    Ok(())
}

fn signed_phase<K: RegisterKind>(
    register: &mut Register<'_, K>,
    angle: f64,
    controls: &NativeControls,
    target: bool,
) -> Result<()> {
    for &control in &controls.zeros {
        register.x(control)?;
    }
    if target {
        quest_sys::apply_multi_qubit_phase_shift(register.pin(), &controls.phase_targets, angle)
            .context("applying phase gate")?;
    } else if controls.wires.is_empty() {
        quest_sys::apply_global_phase(register.pin(), angle).context("applying global phase")?;
    } else {
        quest_sys::apply_multi_qubit_phase_shift(register.pin(), &controls.wires, angle)
            .context("applying controlled phase")?;
    }
    for &control in controls.zeros.iter().rev() {
        register.x(control)?;
    }
    Ok(())
}
pub fn apply_gate<K: RegisterKind>(
    register: &mut Register<'_, K>,
    gate: &BoundGate,
    targets: &[i32],
    controls: &NativeControls,
) -> Result<()> {
    let recipe = dispatch_recipe::gate_recipe(gate, controls.zeros.len())?;
    for step in recipe.steps() {
        execute_step(register, step, targets, controls)?;
    }
    Ok(())
}

pub fn execute_step<K: RegisterKind>(
    register: &mut Register<'_, K>,
    step: DispatchStep,
    targets: &[i32],
    controls: &NativeControls,
) -> Result<()> {
    match step {
        DispatchStep::Native(gate) => apply_primitive(register, gate, targets, controls),
        DispatchStep::PhaseGate(angle) => signed_phase(register, angle, controls, true),
        DispatchStep::ScalarPhase(angle) => signed_phase(register, angle, controls, false),
    }
}

fn apply_primitive<K: RegisterKind>(
    register: &mut Register<'_, K>,
    gate: PrimitiveGate,
    targets: &[i32],
    controls: &NativeControls,
) -> Result<()> {
    let qs = &controls.wires;
    let states = &controls.states;
    let t = *targets.first().ok_or(Error::Value("missing gate target"))?;
    let result = match gate {
        PrimitiveGate::H if controls.wires.is_empty() => {
            quest_sys::apply_hadamard(register.pin(), t)
        }
        PrimitiveGate::H => {
            quest_sys::apply_multi_state_controlled_hadamard(register.pin(), qs, states, t)
        }
        PrimitiveGate::X if controls.wires.is_empty() => {
            quest_sys::apply_pauli_x(register.pin(), t)
        }
        PrimitiveGate::X => {
            quest_sys::apply_multi_state_controlled_pauli_x(register.pin(), qs, states, t)
        }
        PrimitiveGate::Y if controls.wires.is_empty() => {
            quest_sys::apply_pauli_y(register.pin(), t)
        }
        PrimitiveGate::Y => {
            quest_sys::apply_multi_state_controlled_pauli_y(register.pin(), qs, states, t)
        }
        PrimitiveGate::Z if controls.wires.is_empty() => {
            quest_sys::apply_pauli_z(register.pin(), t)
        }
        PrimitiveGate::Z => {
            quest_sys::apply_multi_state_controlled_pauli_z(register.pin(), qs, states, t)
        }
        PrimitiveGate::Rx(a) if controls.wires.is_empty() => {
            quest_sys::apply_rotate_x(register.pin(), t, a)
        }
        PrimitiveGate::Rx(a) => {
            quest_sys::apply_multi_state_controlled_rotate_x(register.pin(), qs, states, t, a)
        }
        PrimitiveGate::Ry(a) if controls.wires.is_empty() => {
            quest_sys::apply_rotate_y(register.pin(), t, a)
        }
        PrimitiveGate::Ry(a) => {
            quest_sys::apply_multi_state_controlled_rotate_y(register.pin(), qs, states, t, a)
        }
        PrimitiveGate::Rz(a) if controls.wires.is_empty() => {
            quest_sys::apply_rotate_z(register.pin(), t, a)
        }
        PrimitiveGate::Rz(a) => {
            quest_sys::apply_multi_state_controlled_rotate_z(register.pin(), qs, states, t, a)
        }
        PrimitiveGate::Swap if controls.wires.is_empty() => quest_sys::apply_swap(
            register.pin(),
            t,
            *targets.get(1).ok_or(Error::Value("missing swap target"))?,
        ),
        PrimitiveGate::Swap => quest_sys::apply_multi_state_controlled_swap(
            register.pin(),
            qs,
            states,
            t,
            *targets.get(1).ok_or(Error::Value("missing swap target"))?,
        ),
    };
    result.context("applying standard gate")
}

#[cfg(all(test, any(feature = "qsvt", all(feature = "mpi", quest_native_mpi))))]
mod matrix_budget_tests {
    use super::{MatrixCacheKey, estimate};
    use googletest::prelude::*;
    use quest_compile::{
        Control, ControlState, MatrixPolicy, NumericalOperator, Operation, QuantumRegionBuilder,
    };
    use std::collections::BTreeSet;

    #[gtest]
    fn conditional_numerical_occurrences_reuse_only_matching_control_profile()
    -> googletest::Result<()> {
        let builder = QuantumRegionBuilder::new(6, 1)?;
        let targets = (0..5)
            .map(|index| builder.qubit(index))
            .collect::<Result<Vec<_>, _>>()?;
        let control = builder.qubit(5)?;
        let bit = builder.bit(0)?;
        let matrix = faer::Mat::from_fn(32, 32, |row, col| {
            crate::Complex64::new(f64::from(row == (col.wrapping_add(1) & 31)), 0.0)
        });
        let matrix = NumericalOperator::from_view(matrix.as_ref(), MatrixPolicy::default())?;
        let wrapped = |state| Operation::Conditional {
            bit,
            expected: false,
            operation: Box::new(Operation::Numerical {
                matrix: matrix.clone(),
                targets: targets.clone().into(),
                controls: vec![Control::new(control, state)].into(),
            }),
        };
        let mut seen = BTreeSet::<MatrixCacheKey>::new();
        let first = estimate(&wrapped(ControlState::Zero), false, &mut seen)?;
        let repeated = estimate(&wrapped(ControlState::Zero), false, &mut seen)?;
        let other_profile = estimate(&wrapped(ControlState::One), false, &mut seen)?;
        // A 64x64 dense native pair has at least 64^2 * 16 bytes of payload.
        verify_that!(first, gt(65_536))?;
        verify_that!(repeated, gt(100))?;
        verify_that!(repeated, lt(1_000))?;
        verify_that!(other_profile, gt(65_536))?;
        Ok(())
    }
}

#[cfg(test)]
mod matrix_resource_tests {
    use super::*;
    use googletest::prelude::*;
    use quest_compile::{MatrixPolicy, NumericalOperator};

    #[gtest]
    fn admission_uses_dispatch_recipe_and_exact_signed_alias_identity() -> googletest::Result<()> {
        let source =
            faer::Mat::from_fn(2, 2, |row, col| Complex64::new(f64::from(row != col), 0.0));
        let matrix = NumericalOperator::from_view(&source, MatrixPolicy::default())?;
        let clone = matrix.clone();
        let independent = NumericalOperator::from_view(&source, MatrixPolicy::default())?;
        let mut seen = BTreeSet::new();
        let first = admit_matrix(&matrix, &[false, true], false, &mut seen)?;
        let dimension = MatrixRecipe::new(&matrix, &[false, true])?.dimension();
        expect_true!(
            first >= bytes_for(dimension.checked_mul(dimension).ok_or(Error::Overflow)?, 12)?
        );
        expect_eq!(admit_matrix(&clone, &[false, true], false, &mut seen)?, 0);
        expect_eq!(
            admit_matrix(&clone, &[true, false], false, &mut seen)?,
            first
        );
        expect_eq!(
            admit_matrix(&independent, &[false, true], false, &mut seen)?,
            first
        );
        let mut gpu_seen = BTreeSet::new();
        expect_true!(admit_matrix(&matrix, &[false, true], true, &mut gpu_seen)? > first);
        Ok(())
    }

    #[gtest]
    fn scalar_native_embedding_and_overflow_precede_admission_publication() -> googletest::Result<()>
    {
        let source = faer::Mat::from_fn(1, 1, |_, _| Complex64::new(0.0, 1.0));
        let scalar = NumericalOperator::from_view(&source, MatrixPolicy::default())?;
        let mut seen = BTreeSet::new();
        expect_eq!(MatrixRecipe::new(&scalar, &[])?.dimension(), 2);
        expect_true!(admit_matrix(&scalar, &[], false, &mut seen)? >= bytes_for(2, 12)?);
        let before = seen.len();
        let controls = vec![false; usize::try_from(usize::BITS)?];
        expect_true!(admit_matrix(&scalar, &controls, false, &mut seen).is_err());
        expect_eq!(seen.len(), before);
        Ok(())
    }
}
