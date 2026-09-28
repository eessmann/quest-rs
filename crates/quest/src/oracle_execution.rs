//! Shared canonical bodies with cached numerical control profiles and reusable scratch.
use crate::{
    Error, Register, RegisterKind, Result,
    execution::{
        NativeControls, NativeMatrix, apply_gate, execute_matrix, phase, prepare_numerical,
    },
    values::reserve_vec,
};
use quest_circuit::{
    BoundGate, Control, ControlState, Operation, OracleFragment, language::vm::QuantumControl,
};
use std::collections::{BTreeMap, BTreeSet};

pub struct OracleInventory {
    budget: usize,
    discovery_bytes: usize,
    bodies: Vec<OracleFragment>,
    profiles: Vec<BTreeMap<Vec<bool>, usize>>,
    depth: usize,
}
impl Default for OracleInventory {
    fn default() -> Self {
        Self::with_budget(quest_circuit::language::semantic::CompileLimits::default().storage_bytes)
    }
}
impl OracleInventory {
    #[cfg(feature = "qsvt")]
    pub(crate) fn dispatch_scratch_bytes(&self) -> Result<usize> {
        self.profiles.iter().try_fold(0_usize, |bytes, profiles| {
            profiles
                .len()
                .checked_mul(256)
                .and_then(|n| bytes.checked_add(n))
                .ok_or(Error::Overflow)
        })
    }
    pub(crate) const fn with_budget(budget: usize) -> Self {
        Self {
            budget,
            discovery_bytes: 0,
            bodies: Vec::new(),
            profiles: Vec::new(),
            depth: 0,
        }
    }
    fn charge_discovery(&mut self, bytes: usize) -> Result<()> {
        let requested = self
            .discovery_bytes
            .checked_add(bytes)
            .ok_or(Error::Overflow)?;
        if requested > self.budget {
            return Err(Error::Budget {
                requested,
                available: self.budget,
            });
        }
        self.discovery_bytes = requested;
        Ok(())
    }

    pub(crate) fn include(
        &mut self,
        fragment: &OracleFragment,
        controls: &[bool],
        depth: usize,
        max_qubits: usize,
    ) -> Result<usize> {
        if depth > 64
            || controls
                .len()
                .checked_add(fragment.num_qubits())
                .ok_or(Error::Overflow)?
                > max_qubits
        {
            return Err(Error::Value(
                "oracle interface exceeds register or nesting bounds",
            ));
        }
        self.depth = self.depth.max(depth);
        let index = if let Some(index) = self
            .bodies
            .iter()
            .position(|body| body.shares_storage_with(fragment))
        {
            index
        } else {
            self.charge_discovery(256)?;
            let index = self.bodies.len();
            self.bodies.push(fragment.clone());
            self.profiles.push(BTreeMap::new());
            index
        };
        let previous = self
            .profiles
            .get(index)
            .ok_or(Error::Value("oracle inventory index"))?
            .get(controls)
            .copied();
        if previous.is_some_and(|previous| previous >= depth) {
            return Ok(index);
        }
        if previous.is_none() {
            self.charge_discovery(controls.len().checked_add(128).ok_or(Error::Overflow)?)?;
        }
        self.profiles
            .get_mut(index)
            .ok_or(Error::Value("oracle inventory index"))?
            .insert(controls.to_vec(), depth);
        for operation in fragment.operations() {
            if let Operation::Oracle {
                fragment,
                controls: local,
                ..
            } = operation
            {
                let nested = controls
                    .iter()
                    .copied()
                    .chain(
                        local
                            .iter()
                            .map(|control| control.state() == ControlState::One),
                    )
                    .collect::<Vec<_>>();
                self.include(
                    fragment,
                    &nested,
                    depth.checked_add(1).ok_or(Error::Overflow)?,
                    max_qubits,
                )?;
            }
        }
        Ok(index)
    }
    pub(crate) fn index(&self, fragment: &OracleFragment) -> Result<usize> {
        self.bodies
            .iter()
            .position(|body| body.shares_storage_with(fragment))
            .ok_or(Error::Value("missing oracle body"))
    }
    pub(crate) fn include_operation(&mut self, operation: &Operation, width: usize) -> Result<()> {
        match operation {
            Operation::Oracle {
                fragment, controls, ..
            } => {
                self.include(fragment, &states(controls), 1, width)?;
            }
            Operation::Conditional { operation, .. } => self.include_operation(operation, width)?,
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn estimated_bytes(&self, width: usize, gpu: bool) -> Result<usize> {
        if self.bodies.is_empty() {
            return Ok(0);
        }
        let mut bytes = OracleFragment::shared_storage_bytes(&self.bodies)?
            .checked_add(self.discovery_bytes)
            .ok_or(Error::Overflow)?;
        let mut seen_matrices = BTreeSet::new();
        bytes = bytes
            .checked_add(
                width
                    .checked_mul(self.depth.saturating_add(2))
                    .and_then(|n| n.checked_mul(128))
                    .ok_or(Error::Overflow)?,
            )
            .ok_or(Error::Overflow)?;
        for (body, profiles) in self.bodies.iter().zip(&self.profiles) {
            bytes = bytes
                .checked_add(
                    body.operations()
                        .len()
                        .checked_mul(256)
                        .ok_or(Error::Overflow)?,
                )
                .ok_or(Error::Overflow)?;
            for operation in body.operations() {
                let (targets, controls) = match operation {
                    Operation::Gate {
                        targets, controls, ..
                    }
                    | Operation::Numerical {
                        targets, controls, ..
                    }
                    | Operation::Oracle {
                        targets, controls, ..
                    } => (targets.len(), controls.len()),
                    Operation::GlobalPhase { controls, .. } => (0, controls.len()),
                    _ => (0, 0),
                };
                bytes = bytes
                    .checked_add(
                        targets
                            .checked_mul(size_of::<usize>())
                            .and_then(|n| {
                                n.checked_add(controls.checked_mul(size_of::<QuantumControl>())?)
                            })
                            .ok_or(Error::Overflow)?,
                    )
                    .ok_or(Error::Overflow)?;
                if let Operation::Numerical {
                    matrix, controls, ..
                } = operation
                {
                    for profile in profiles.keys() {
                        let count = profile
                            .len()
                            .checked_add(controls.len())
                            .ok_or(Error::Overflow)?;
                        bytes = bytes
                            .checked_add(
                                count
                                    .saturating_add(1)
                                    .checked_mul(128)
                                    .ok_or(Error::Overflow)?,
                            )
                            .ok_or(Error::Overflow)?;
                        let key = (
                            matrix.view().as_ptr().addr(),
                            profile
                                .iter()
                                .copied()
                                .chain(states(controls))
                                .collect::<Vec<_>>(),
                        );
                        if !seen_matrices.insert(key) {
                            continue;
                        }
                        let dimension = matrix
                            .dimension()
                            .checked_shl(u32::try_from(count).map_err(|_| Error::Overflow)?)
                            .ok_or(Error::Overflow)?
                            .max(2);
                        let entries = if matrix.is_diagonal() {
                            dimension
                        } else {
                            dimension.checked_mul(dimension).ok_or(Error::Overflow)?
                        };
                        bytes = bytes
                            .checked_add(crate::values::bytes_for(
                                entries,
                                if gpu { 16 } else { 12 },
                            )?)
                            .and_then(|n| n.checked_add(count.checked_mul(128)?))
                            .ok_or(Error::Overflow)?;
                    }
                }
            }
        }
        Ok(bytes)
    }
}
fn states(controls: &[Control]) -> Vec<bool> {
    controls
        .iter()
        .map(|control| control.state() == ControlState::One)
        .collect()
}
fn local_controls(controls: &[Control]) -> Vec<QuantumControl> {
    controls
        .iter()
        .map(|control| QuantumControl {
            qubit: control.qubit().index(),
            positive: control.state() == ControlState::One,
        })
        .collect()
}

enum OracleOp {
    Gate {
        gate: BoundGate,
        targets: Vec<usize>,
        controls: Vec<QuantumControl>,
    },
    Phase {
        radians: f64,
        controls: Vec<QuantumControl>,
    },
    Numerical {
        variants: BTreeMap<Vec<bool>, usize>,
        targets: Vec<usize>,
        controls: Vec<QuantumControl>,
    },
    Call {
        body: usize,
        targets: Vec<usize>,
        controls: Vec<QuantumControl>,
        adjoint: bool,
    },
    Barrier,
}
pub struct OracleCache {
    matrices: Vec<NativeMatrix>,
    bodies: Vec<Vec<OracleOp>>,
    frames: Vec<Frame>,
    leaf: Leaf,
}
struct Frame {
    targets: Vec<usize>,
    controls: Vec<QuantumControl>,
}
struct Leaf {
    targets: Vec<i32>,
    controls: NativeControls,
    profile: Vec<bool>,
}
impl OracleCache {
    pub(crate) fn prepare(inventory: &OracleInventory, width: usize) -> Result<Self> {
        let width = if inventory.bodies.is_empty() {
            0
        } else {
            width
        };
        let mut matrices = Vec::new();
        let mut matrix_cache = BTreeMap::new();
        let mut bodies = reserve_vec(inventory.bodies.len())?;
        for (body, profiles) in inventory.bodies.iter().zip(&inventory.profiles) {
            let mut operations = reserve_vec(body.operations().len())?;
            for operation in body.operations() {
                operations.push(prepare_operation(
                    operation,
                    profiles,
                    inventory,
                    &mut matrices,
                    &mut matrix_cache,
                )?);
            }
            bodies.push(operations);
        }
        let mut frames = reserve_vec(inventory.depth)?;
        for _ in 0..inventory.depth {
            frames.push(Frame {
                targets: reserve_vec(width)?,
                controls: reserve_vec(width)?,
            });
        }
        let leaf = Leaf {
            targets: reserve_vec(width)?,
            profile: reserve_vec(width)?,
            controls: NativeControls::with_capacity(width, false)?,
        };
        Ok(Self {
            matrices,
            bodies,
            frames,
            leaf,
        })
    }
    pub(crate) fn run<K: RegisterKind>(
        &mut self,
        body: usize,
        targets: &[usize],
        controls: &[QuantumControl],
        adjoint: bool,
        register: &mut Register<'_, K>,
    ) -> Result<()> {
        for &target in targets {
            register.check_qubit(target)?;
        }
        for control in controls {
            register.check_qubit(control.qubit)?;
        }
        execute_body(
            &self.bodies,
            &self.matrices,
            &mut self.frames,
            &mut self.leaf,
            body,
            targets,
            controls,
            adjoint,
            register,
        )
    }
    pub(crate) const fn body_count(&self) -> usize {
        self.bodies.len()
    }
    pub(crate) const fn matrix_count(&self) -> usize {
        self.matrices.len()
    }
}
fn prepare_operation(
    operation: &Operation,
    profiles: &BTreeMap<Vec<bool>, usize>,
    inventory: &OracleInventory,
    matrices: &mut Vec<NativeMatrix>,
    cache: &mut BTreeMap<(usize, Vec<bool>), usize>,
) -> Result<OracleOp> {
    let targets =
        |targets: &[quest_circuit::QubitId]| targets.iter().map(|target| target.index()).collect();
    Ok(match operation {
        Operation::Gate {
            gate,
            targets: local,
            controls,
        } => OracleOp::Gate {
            gate: gate.clone(),
            targets: targets(local),
            controls: local_controls(controls),
        },
        Operation::GlobalPhase { radians, controls } => OracleOp::Phase {
            radians: *radians,
            controls: local_controls(controls),
        },
        Operation::Oracle {
            fragment,
            targets: local,
            controls,
        } => OracleOp::Call {
            body: inventory.index(fragment)?,
            targets: targets(local),
            controls: local_controls(controls),
            adjoint: fragment.is_adjoint(),
        },
        Operation::Numerical {
            matrix,
            targets: local,
            controls,
        } => {
            let mut variants = BTreeMap::new();
            for profile in profiles.keys() {
                let profile = profile
                    .iter()
                    .copied()
                    .chain(states(controls))
                    .collect::<Vec<_>>();
                let key = (matrix.view().as_ptr().addr(), profile.clone());
                let index = if let Some(index) = cache.get(&key) {
                    *index
                } else {
                    let index = matrices.len();
                    matrices.push(prepare_numerical(matrix, &profile)?);
                    cache.insert(key, index);
                    index
                };
                variants.insert(profile, index);
            }
            OracleOp::Numerical {
                variants,
                targets: targets(local),
                controls: local_controls(controls),
            }
        }
        Operation::Barrier { .. } => OracleOp::Barrier,
        _ => return Err(Error::Unsupported("effect in coherent oracle")),
    })
}
fn bounded_push<T>(output: &mut Vec<T>, value: T) -> Result<()> {
    if output.len() == output.capacity() {
        return Err(Error::Value("oracle remapping scratch exhausted"));
    }
    output.push(value);
    Ok(())
}
impl Frame {
    fn remap(
        &mut self,
        local: &[usize],
        controls: &[QuantumControl],
        mapping: &[usize],
        inherited: &[QuantumControl],
    ) -> Result<()> {
        self.targets.clear();
        self.controls.clear();
        for &target in local {
            bounded_push(
                &mut self.targets,
                *mapping
                    .get(target)
                    .ok_or(Error::Value("oracle local target"))?,
            )?;
        }
        for &control in inherited {
            bounded_push(&mut self.controls, control)?;
        }
        for control in controls {
            bounded_push(
                &mut self.controls,
                QuantumControl {
                    qubit: *mapping
                        .get(control.qubit)
                        .ok_or(Error::Value("oracle local control"))?,
                    positive: control.positive,
                },
            )?;
        }
        Ok(())
    }
}
impl Leaf {
    fn remap(&mut self, frame: &Frame) -> Result<()> {
        self.targets.clear();
        self.profile.clear();
        for target in &frame.targets {
            bounded_push(
                &mut self.targets,
                i32::try_from(*target).map_err(|_| Error::Overflow)?,
            )?;
        }
        for control in &frame.controls {
            bounded_push(&mut self.profile, control.positive)?;
        }
        self.controls.load(
            frame.controls.iter().map(|control| {
                Ok((
                    i32::try_from(control.qubit).map_err(|_| Error::Overflow)?,
                    control.positive,
                ))
            }),
            self.targets.first().copied(),
        )?;
        Ok(())
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Recursion borrows disjoint immutable cache and bounded mutable scratch with the live register"
)]
fn execute_body<K: RegisterKind>(
    bodies: &[Vec<OracleOp>],
    matrices: &[NativeMatrix],
    frames: &mut [Frame],
    leaf: &mut Leaf,
    body: usize,
    mapping: &[usize],
    inherited: &[QuantumControl],
    adjoint: bool,
    register: &mut Register<'_, K>,
) -> Result<()> {
    let operations = bodies.get(body).ok_or(Error::Value("oracle body index"))?;
    let (frame, rest) = frames
        .split_first_mut()
        .ok_or(Error::Value("oracle nesting scratch"))?;
    for position in 0..operations.len() {
        let index = if adjoint {
            operations
                .len()
                .checked_sub(position.saturating_add(1))
                .ok_or(Error::Overflow)?
        } else {
            position
        };
        let operation = operations
            .get(index)
            .ok_or(Error::Value("oracle operation index"))?;
        let (targets, controls) = match operation {
            OracleOp::Gate {
                targets, controls, ..
            }
            | OracleOp::Numerical {
                targets, controls, ..
            }
            | OracleOp::Call {
                targets, controls, ..
            } => (targets.as_slice(), controls.as_slice()),
            OracleOp::Phase { controls, .. } => (&[][..], controls.as_slice()),
            OracleOp::Barrier => continue,
        };
        frame.remap(targets, controls, mapping, inherited)?;
        if let OracleOp::Call {
            body,
            adjoint: inner,
            ..
        } = operation
        {
            execute_body(
                bodies,
                matrices,
                rest,
                leaf,
                *body,
                &frame.targets,
                &frame.controls,
                adjoint ^ inner,
                register,
            )?;
            continue;
        }
        leaf.remap(frame)?;
        match operation {
            OracleOp::Gate { gate, .. } => apply_gate(
                register,
                &if adjoint {
                    gate.adjoint()
                } else {
                    gate.clone()
                },
                &leaf.targets,
                &leaf.controls,
            )?,
            OracleOp::Phase { radians, .. } => phase(
                register,
                if adjoint { -*radians } else { *radians },
                &leaf.controls,
            )?,
            OracleOp::Numerical { variants, .. } => {
                let index = *variants
                    .get(&leaf.profile)
                    .ok_or(Error::Value("unprepared oracle control profile"))?;
                for &wire in &leaf.controls.wires {
                    bounded_push(&mut leaf.targets, wire)?;
                }
                if leaf.targets.is_empty() {
                    bounded_push(&mut leaf.targets, 0)?;
                }
                execute_matrix(
                    matrices
                        .get(index)
                        .ok_or(Error::Value("oracle matrix index"))?,
                    register,
                    &leaf.targets,
                    adjoint,
                )?;
            }
            OracleOp::Call { .. } | OracleOp::Barrier => {
                return Err(Error::Value("oracle dispatch invariant"));
            }
        }
    }
    Ok(())
}

impl OracleInventory {
    pub(crate) fn from_structured(
        plan: &quest_circuit::StructuredPlan,
        budget: usize,
    ) -> Result<Self> {
        let mut inventory = Self::with_budget(budget);
        if plan.oracle_captures().is_empty() {
            return Ok(inventory);
        }
        let mut visited = BTreeMap::new();
        inventory.structured_region(plan, plan.ssa().program().entry, &[], 1, &mut visited)?;
        Ok(inventory)
    }
    fn structured_region(
        &mut self,
        plan: &quest_circuit::StructuredPlan,
        region: quest_circuit::language::ssa::RegionId,
        inherited: &[bool],
        depth: usize,
        visited: &mut BTreeMap<(usize, Vec<bool>), usize>,
    ) -> Result<()> {
        use quest_circuit::language::ssa::{GateModifier, InstructionKind};
        if depth > 64 || inherited.len() > plan.num_qubits() {
            return Err(Error::Value(
                "structured oracle control or call-depth bound",
            ));
        }
        let key = (region.index(), inherited.to_vec());
        if visited.get(&key).is_some_and(|previous| *previous >= depth) {
            return Ok(());
        }
        if !visited.contains_key(&key) {
            self.charge_discovery(inherited.len().checked_add(128).ok_or(Error::Overflow)?)?;
        }
        visited.insert(key, depth);
        let body = plan
            .ssa()
            .program()
            .regions
            .get(region.index())
            .ok_or(Error::Value("structured oracle region"))?;
        if let Some(oracle) = &body.oracle {
            let fragment = plan
                .oracle_captures()
                .get(&oracle.index())
                .ok_or(Error::Value("missing structured oracle capture"))?;
            self.include(fragment, inherited, 1, plan.num_qubits())?;
            return Ok(());
        }
        for block in plan
            .ssa()
            .blocks()
            .iter()
            .filter(|block| block.region == region)
        {
            for instruction in &block.instructions {
                if let InstructionKind::Call {
                    region, modifiers, ..
                } = &instruction.kind
                {
                    let mut controls = inherited.to_vec();
                    for modifier in modifiers {
                        if let GateModifier::Control { positive, count } = modifier {
                            let total =
                                controls.len().checked_add(*count).ok_or(Error::Overflow)?;
                            if total > plan.num_qubits() {
                                return Err(Error::Value(
                                    "structured oracle controls exceed register",
                                ));
                            }
                            controls.extend(std::iter::repeat_n(*positive, *count));
                        }
                    }
                    self.structured_region(
                        plan,
                        *region,
                        &controls,
                        depth.checked_add(1).ok_or(Error::Overflow)?,
                        visited,
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;
    use quest_circuit::{Gate, ProgramBuilder};
    fn body() -> crate::Result<OracleFragment> {
        let mut builder = ProgramBuilder::new(1, 0)?;
        builder.gate(Gate::X, &[builder.qubit(0)?], &[])?;
        Ok(OracleFragment::builder(builder.finish()?.bind(&[])?)
            .matrix_tolerance(1e-12)?
            .build()?)
    }
    #[gtest]
    fn inventory_keeps_distinct_profiles_and_deepest_shared_path() -> googletest::Result<()> {
        let leaf = body()?;
        let mut builder = ProgramBuilder::new(1, 0)?;
        builder.oracle(&leaf, &[builder.qubit(0)?], &[])?;
        let outer = OracleFragment::builder(builder.finish()?.bind(&[])?)
            .matrix_tolerance(1e-12)?
            .build()?;
        let mut inventory = OracleInventory::default();
        inventory.include(&leaf, &[false], 1, 2)?;
        inventory.include(&leaf.adjoint(), &[true], 1, 2)?;
        inventory.include(&outer, &[false], 1, 2)?;
        expect_eq!(inventory.bodies.len(), 2);
        expect_eq!(inventory.depth, 2);
        expect_eq!(
            inventory
                .profiles
                .first()
                .ok_or(Error::Value("missing profile"))?
                .len(),
            2
        );
        expect_eq!(
            inventory
                .profiles
                .first()
                .ok_or(Error::Value("missing profile"))?
                .get(&vec![false]),
            Some(&2)
        );
        Ok(())
    }
    #[gtest]
    fn profile_discovery_obeys_the_preparation_budget() -> googletest::Result<()> {
        let body = body()?;
        let mut inventory = OracleInventory::with_budget(1);
        expect_true!(inventory.include(&body, &[], 1, 1).is_err());
        expect_true!(inventory.bodies.is_empty());
        Ok(())
    }
    #[gtest]
    fn remapping_reuses_storage_and_rejects_unadmitted_growth() -> googletest::Result<()> {
        let mut frame = Frame {
            targets: reserve_vec(3)?,
            controls: reserve_vec(3)?,
        };
        let targets = frame.targets.as_ptr();
        let controls = frame.controls.as_ptr();
        for _ in 0..100 {
            frame.remap(
                &[1, 0],
                &[QuantumControl {
                    qubit: 2,
                    positive: false,
                }],
                &[2, 0, 1],
                &[],
            )?;
            expect_eq!(&frame.targets, &vec![0, 2]);
            expect_eq!(frame.targets.as_ptr(), targets);
            expect_eq!(frame.controls.as_ptr(), controls);
        }
        expect_true!(frame.remap(&[0, 1, 2, 0], &[], &[0, 1, 2], &[]).is_err());
        Ok(())
    }
}
