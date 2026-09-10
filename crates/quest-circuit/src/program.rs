use crate::{
    model::{Occurrence, SemanticOperation},
    *,
};
use petgraph::{
    Direction,
    stable_graph::{NodeIndex, StableDiGraph},
    visit::EdgeRef,
};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_PROGRAM: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy)]
pub struct ProgramLimits {
    pub max_qubits: usize,
    pub max_bits: usize,
    pub max_operations: usize,
    pub max_matrix_bytes: usize,
}
impl Default for ProgramLimits {
    fn default() -> Self {
        Self {
            max_qubits: 1024,
            max_bits: 1 << 20,
            max_operations: 1 << 20,
            max_matrix_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub struct ProgramBuilder {
    owner: u64,
    num_qubits: usize,
    num_bits: usize,
    parameters: Vec<String>,
    occurrences: Vec<Occurrence>,
    definitions: Vec<(String, UnitaryCircuit)>,
    explicit_edges: Vec<(OccurrenceId, OccurrenceId)>,
    limits: ProgramLimits,
    source: Option<SourceSpan>,
    matrix_bytes: usize,
}
impl ProgramBuilder {
    pub fn new(num_qubits: usize, num_bits: usize) -> Result<Self> {
        Self::with_limits(num_qubits, num_bits, ProgramLimits::default())
    }
    pub fn with_limits(num_qubits: usize, num_bits: usize, limits: ProgramLimits) -> Result<Self> {
        if num_qubits == 0 || num_qubits > limits.max_qubits {
            return Err(Error::Budget("qubit count"));
        }
        if num_bits > limits.max_bits {
            return Err(Error::Budget("classical bit count"));
        }
        let owner = NEXT_PROGRAM
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |x| x.checked_add(1))
            .map_err(|_| Error::Budget("program identifiers"))?;
        Ok(Self {
            owner,
            num_qubits,
            num_bits,
            parameters: vec![],
            occurrences: vec![],
            definitions: vec![],
            explicit_edges: vec![],
            limits,
            source: None,
            matrix_bytes: 0,
        })
    }
    pub const fn num_qubits(&self) -> usize {
        self.num_qubits
    }
    pub const fn num_bits(&self) -> usize {
        self.num_bits
    }
    pub fn qubit(&self, index: usize) -> Result<QubitId> {
        if index >= self.num_qubits {
            Err(Error::InvalidId)
        } else {
            Ok(QubitId {
                owner: self.owner,
                index,
            })
        }
    }
    pub fn bit(&self, index: usize) -> Result<BitId> {
        if index >= self.num_bits {
            Err(Error::InvalidId)
        } else {
            Ok(BitId {
                owner: self.owner,
                index,
            })
        }
    }
    pub fn parameter(&mut self, name: impl Into<String>) -> Result<ParameterId> {
        let name = name.into();
        if name.is_empty() || self.parameters.contains(&name) {
            return Err(Error::ParameterName);
        }
        if self.parameters.len() >= self.limits.max_operations {
            return Err(Error::Budget("parameter count"));
        }
        let id = ParameterId {
            owner: self.owner,
            index: self.parameters.len(),
        };
        self.parameters.push(name);
        Ok(id)
    }
    pub fn set_source(&mut self, source: Option<SourceSpan>) {
        self.source = source;
    }
    /// Definitions contain complete immutable unitary programs. Forward or
    /// recursive references cannot be constructed; calls expand transactionally.
    pub fn define(
        &mut self,
        name: impl Into<String>,
        body: UnitaryCircuit,
    ) -> Result<GateDefinitionId> {
        let name = name.into();
        if name.is_empty()
            || self
                .definitions
                .iter()
                .any(|(existing, _)| existing == &name)
        {
            return Err(Error::ParameterName);
        }
        if self.definitions.len() >= self.limits.max_operations {
            return Err(Error::Budget("gate definitions"));
        }
        let id = GateDefinitionId {
            owner: self.owner,
            index: self.definitions.len(),
        };
        self.definitions.push((name, body));
        Ok(id)
    }
    pub fn call(
        &mut self,
        definition: GateDefinitionId,
        arguments: &[QubitId],
        parameters: &[Angle],
        controls: &[Control],
    ) -> Result<Vec<OccurrenceId>> {
        if definition.owner != self.owner {
            return Err(Error::InvalidId);
        }
        let (_, body) = self
            .definitions
            .get(definition.index)
            .ok_or(Error::InvalidId)?;
        let body = body.program();
        if arguments.len() != body.num_qubits {
            return Err(Error::Arity {
                expected: body.num_qubits,
                actual: arguments.len(),
            });
        }
        if parameters.len() != body.parameters.len() {
            return Err(Error::Binding);
        }
        self.operands(arguments, controls)?;
        for a in parameters {
            self.check_angle(a)?;
        }
        if self
            .occurrences
            .len()
            .checked_add(body.occurrences.len())
            .ok_or(Error::Budget("definition expansion"))?
            > self.limits.max_operations
        {
            return Err(Error::Budget("definition expansion"));
        }
        let bindings: BTreeMap<_, _> = body
            .parameters()
            .zip(parameters)
            .map(|((id, _), angle)| (id, angle.clone()))
            .collect();
        let ordered: BTreeMap<_, _> = body.occurrences.iter().map(|o| (o.id, o)).collect();
        let mut expanded = Vec::with_capacity(body.occurrences.len());
        for id in body.schedule() {
            let o = ordered.get(id).ok_or(Error::InvalidId)?;
            let map_controls = |inner: &[Control]| -> Vec<Control> {
                inner
                    .iter()
                    .map(|c| Control::new(arguments[c.qubit().index], c.state()))
                    .chain(controls.iter().copied())
                    .collect()
            };
            let operation = match &o.operation {
                SemanticOperation::Gate {
                    gate,
                    targets,
                    controls: inner,
                } => self.gate_operation(
                    gate.substitute(&bindings)?,
                    &targets
                        .iter()
                        .map(|q| arguments[q.index])
                        .collect::<Vec<_>>(),
                    &map_controls(inner),
                )?,
                SemanticOperation::GlobalPhase {
                    angle,
                    controls: inner,
                } => SemanticOperation::GlobalPhase {
                    angle: angle.substitute(&bindings)?,
                    controls: self.operands(&[], &map_controls(inner))?,
                },
                SemanticOperation::Barrier { qubits } => SemanticOperation::Barrier {
                    qubits: qubits
                        .iter()
                        .map(|q| arguments[q.index])
                        .chain(controls.iter().map(|c| c.qubit()))
                        .collect(),
                },
                _ => return Err(Error::NotUnitary),
            };
            expanded.push((operation, self.source.clone().or_else(|| o.source.clone())));
        }
        let mapped: BTreeMap<_, _> = body
            .schedule()
            .iter()
            .enumerate()
            .map(|(offset, id)| {
                (
                    *id,
                    OccurrenceId {
                        owner: self.owner,
                        index: self.occurrences.len() + offset,
                    },
                )
            })
            .collect();
        let explicit_edges = body
            .explicit_edges
            .iter()
            .map(|(a, b)| {
                Ok((
                    *mapped.get(a).ok_or(Error::InvalidId)?,
                    *mapped.get(b).ok_or(Error::InvalidId)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut ids = Vec::with_capacity(expanded.len());
        for (operation, source) in expanded {
            let id = OccurrenceId {
                owner: self.owner,
                index: self.occurrences.len(),
            };
            self.occurrences.push(Occurrence {
                id,
                provenance: vec![id],
                source,
                operation,
            });
            ids.push(id);
        }
        self.explicit_edges.extend(explicit_edges);
        Ok(ids)
    }
    fn check_qubit(&self, q: QubitId) -> Result<()> {
        if q.owner != self.owner || q.index >= self.num_qubits {
            Err(Error::InvalidId)
        } else {
            Ok(())
        }
    }
    fn check_bit(&self, b: BitId) -> Result<()> {
        if b.owner != self.owner || b.index >= self.num_bits {
            Err(Error::InvalidId)
        } else {
            Ok(())
        }
    }
    fn check_angle(&self, a: &Angle) -> Result<()> {
        let mut ids = vec![];
        a.parameters(&mut ids);
        if ids
            .iter()
            .any(|x| x.owner != self.owner || x.index >= self.parameters.len())
        {
            Err(Error::InvalidId)
        } else {
            Ok(())
        }
    }
    fn operands(&self, targets: &[QubitId], controls: &[Control]) -> Result<Vec<Control>> {
        let mut used = BTreeSet::new();
        for q in targets
            .iter()
            .copied()
            .chain(controls.iter().map(|c| c.qubit()))
        {
            self.check_qubit(q)?;
            if !used.insert(q) {
                return Err(Error::DuplicateOperand);
            }
        }
        let mut controls = controls.to_vec();
        controls.sort();
        Ok(controls)
    }
    fn gate_operation(
        &self,
        gate: Gate,
        targets: &[QubitId],
        controls: &[Control],
    ) -> Result<SemanticOperation> {
        if gate.arity() != targets.len() {
            return Err(Error::Arity {
                expected: gate.arity(),
                actual: targets.len(),
            });
        }
        let controls = self.operands(targets, controls)?;
        for angle in gate.angles() {
            self.check_angle(angle)?;
        }
        Ok(SemanticOperation::Gate {
            gate,
            targets: targets.to_vec(),
            controls,
        })
    }
    fn push(&mut self, operation: SemanticOperation) -> Result<OccurrenceId> {
        if self.occurrences.len() >= self.limits.max_operations {
            return Err(Error::Budget("operation count"));
        }
        let id = OccurrenceId {
            owner: self.owner,
            index: self.occurrences.len(),
        };
        self.occurrences.push(Occurrence {
            id,
            provenance: vec![id],
            source: self.source.clone(),
            operation,
        });
        Ok(id)
    }
    pub fn gate(
        &mut self,
        gate: Gate,
        targets: &[QubitId],
        controls: &[Control],
    ) -> Result<OccurrenceId> {
        let operation = self.gate_operation(gate, targets, controls)?;
        self.push(operation)
    }
    pub fn gate_if(
        &mut self,
        bit: BitId,
        expected: bool,
        gate: Gate,
        targets: &[QubitId],
        controls: &[Control],
    ) -> Result<OccurrenceId> {
        self.check_bit(bit)?;
        let operation = Box::new(self.gate_operation(gate, targets, controls)?);
        self.push(SemanticOperation::Conditional {
            bit,
            expected,
            operation,
        })
    }
    pub fn global_phase(&mut self, angle: Angle, controls: &[Control]) -> Result<OccurrenceId> {
        self.check_angle(&angle)?;
        let controls = self.operands(&[], controls)?;
        self.push(SemanticOperation::GlobalPhase { angle, controls })
    }
    pub fn measure(&mut self, qubit: QubitId, bit: BitId) -> Result<OccurrenceId> {
        self.check_qubit(qubit)?;
        self.check_bit(bit)?;
        self.push(SemanticOperation::Measure { qubit, bit })
    }
    pub fn reset(&mut self, qubit: QubitId) -> Result<OccurrenceId> {
        self.check_qubit(qubit)?;
        self.push(SemanticOperation::Reset { qubit })
    }
    /// An empty scope is a whole-register barrier.
    pub fn barrier(&mut self, qubits: &[QubitId]) -> Result<OccurrenceId> {
        let qubits = if qubits.is_empty() {
            (0..self.num_qubits)
                .map(|i| self.qubit(i))
                .collect::<Result<Vec<_>>>()?
        } else {
            qubits.to_vec()
        };
        self.operands(&qubits, &[])?;
        self.push(SemanticOperation::Barrier { qubits })
    }
    pub fn numerical(
        &mut self,
        matrix: NumericalOperator,
        targets: &[QubitId],
        controls: &[Control],
    ) -> Result<OccurrenceId> {
        if matrix.num_qubits() != targets.len() {
            return Err(Error::Arity {
                expected: matrix.num_qubits(),
                actual: targets.len(),
            });
        }
        let controls = self.operands(targets, controls)?;
        let bytes = self
            .matrix_bytes
            .checked_add(matrix.bytes())
            .ok_or(Error::Budget("program matrix bytes"))?;
        if bytes > self.limits.max_matrix_bytes {
            return Err(Error::Budget("program matrix bytes"));
        }
        let id = self.push(SemanticOperation::Numerical {
            matrix,
            targets: targets.to_vec(),
            controls,
        })?;
        self.matrix_bytes = bytes;
        Ok(id)
    }
    pub fn channel(
        &mut self,
        kraus: Vec<NumericalOperator>,
        targets: &[QubitId],
        tolerance: f64,
    ) -> Result<OccurrenceId> {
        self.operands(targets, &[])?;
        let k = kraus.first().ok_or(Error::MatrixShape)?;
        if k.num_qubits() != targets.len() {
            return Err(Error::Arity {
                expected: k.num_qubits(),
                actual: targets.len(),
            });
        }
        crate::matrix::check_channel(
            &kraus,
            tolerance,
            MatrixPolicy {
                max_bytes: self
                    .limits
                    .max_matrix_bytes
                    .saturating_sub(self.matrix_bytes),
            },
        )?;
        let bytes = kraus.iter().try_fold(self.matrix_bytes, |acc, k| {
            acc.checked_add(k.bytes())
                .ok_or(Error::Budget("program matrix bytes"))
        })?;
        if bytes > self.limits.max_matrix_bytes {
            return Err(Error::Budget("program matrix bytes"));
        }
        let id = self.push(SemanticOperation::Channel {
            kraus,
            targets: targets.to_vec(),
        })?;
        self.matrix_bytes = bytes;
        Ok(id)
    }
    /// Adds an explicit order constraint. Acyclicity is established at finish.
    pub fn depend(&mut self, before: OccurrenceId, after: OccurrenceId) -> Result<()> {
        for id in [before, after] {
            if id.owner != self.owner || id.index >= self.occurrences.len() {
                return Err(Error::InvalidId);
            }
        }
        self.explicit_edges.push((before, after));
        Ok(())
    }
    pub fn finish(self) -> Result<ValidatedProgram> {
        ValidatedProgram::from_parts(
            self.owner,
            self.num_qubits,
            self.num_bits,
            self.parameters,
            self.occurrences,
            self.explicit_edges,
            self.limits,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    Quantum,
    Classical,
    Stochastic,
    Explicit,
}

#[derive(Debug, Clone)]
pub struct ValidatedProgram {
    pub(crate) owner: u64,
    pub(crate) num_qubits: usize,
    pub(crate) num_bits: usize,
    pub(crate) parameters: Vec<String>,
    pub(crate) occurrences: Vec<Occurrence>,
    pub(crate) explicit_edges: Vec<(OccurrenceId, OccurrenceId)>,
    pub(crate) limits: ProgramLimits,
    graph: StableDiGraph<OccurrenceId, DependencyKind>,
    nodes: BTreeMap<OccurrenceId, NodeIndex>,
    schedule: Vec<OccurrenceId>,
}
/// A general admitted program, without exact-unitary privileges.
pub type Program = ValidatedProgram;

impl ValidatedProgram {
    pub(crate) fn from_parts(
        owner: u64,
        num_qubits: usize,
        num_bits: usize,
        parameters: Vec<String>,
        occurrences: Vec<Occurrence>,
        explicit_edges: Vec<(OccurrenceId, OccurrenceId)>,
        limits: ProgramLimits,
    ) -> Result<Self> {
        let mut graph = StableDiGraph::new();
        let mut nodes = BTreeMap::new();
        let mut quantum = BTreeMap::new();
        let mut writes = BTreeMap::new();
        let mut readers: BTreeMap<BitId, Vec<NodeIndex>> = BTreeMap::new();
        let mut stochastic = None;
        for occurrence in &occurrences {
            let node = graph.add_node(occurrence.id);
            nodes.insert(occurrence.id, node);
            for q in occurrence.operation.qubits() {
                if let Some(previous) = quantum.insert(q, node) {
                    graph.add_edge(previous, node, DependencyKind::Quantum);
                }
            }
            match &occurrence.operation {
                SemanticOperation::Measure { bit, .. } => {
                    if let Some(previous) = writes.insert(*bit, node) {
                        graph.add_edge(previous, node, DependencyKind::Classical);
                    }
                    for reader in readers.remove(bit).unwrap_or_default() {
                        graph.add_edge(reader, node, DependencyKind::Classical);
                    }
                }
                SemanticOperation::Conditional { bit, .. } => {
                    if let Some(previous) = writes.get(bit) {
                        graph.add_edge(*previous, node, DependencyKind::Classical);
                    }
                    readers.entry(*bit).or_default().push(node);
                }
                _ => {}
            }
            if occurrence.operation.stochastic()
                && let Some(previous) = stochastic.replace(node)
            {
                graph.add_edge(previous, node, DependencyKind::Stochastic);
            }
        }
        for (a, b) in &explicit_edges {
            let a = *nodes.get(a).ok_or(Error::InvalidId)?;
            let b = *nodes.get(b).ok_or(Error::InvalidId)?;
            graph.add_edge(a, b, DependencyKind::Explicit);
        }
        // Kahn scheduling uses public occurrence order, never reusable graph slots.
        let mut degrees = BTreeMap::new();
        let mut ready = BinaryHeap::new();
        for node in graph.node_indices() {
            let degree = graph.edges_directed(node, Direction::Incoming).count();
            degrees.insert(node, degree);
            if degree == 0 {
                ready.push(Reverse((graph[node], node)));
            }
        }
        let mut schedule = Vec::with_capacity(occurrences.len());
        while let Some(Reverse((id, node))) = ready.pop() {
            schedule.push(id);
            for edge in graph.edges_directed(node, Direction::Outgoing) {
                let target = edge.target();
                let degree = degrees.get_mut(&target).ok_or(Error::Cycle)?;
                *degree -= 1;
                if *degree == 0 {
                    ready.push(Reverse((graph[target], target)));
                }
            }
        }
        if schedule.len() != occurrences.len() {
            return Err(Error::Cycle);
        }
        Ok(Self {
            owner,
            num_qubits,
            num_bits,
            parameters,
            occurrences,
            explicit_edges,
            limits,
            graph,
            nodes,
            schedule,
        })
    }
    pub const fn num_qubits(&self) -> usize {
        self.num_qubits
    }
    pub const fn num_bits(&self) -> usize {
        self.num_bits
    }
    pub fn qubit(&self, index: usize) -> Result<QubitId> {
        if index >= self.num_qubits {
            Err(Error::InvalidId)
        } else {
            Ok(QubitId {
                owner: self.owner,
                index,
            })
        }
    }
    pub fn bit(&self, index: usize) -> Result<BitId> {
        if index >= self.num_bits {
            Err(Error::InvalidId)
        } else {
            Ok(BitId {
                owner: self.owner,
                index,
            })
        }
    }
    pub fn parameters(&self) -> impl ExactSizeIterator<Item = (ParameterId, &str)> {
        self.parameters.iter().enumerate().map(|(index, name)| {
            (
                ParameterId {
                    owner: self.owner,
                    index,
                },
                name.as_str(),
            )
        })
    }
    pub fn schedule(&self) -> &[OccurrenceId] {
        &self.schedule
    }
    /// Longest dependency path, counting each operation (including barriers
    /// and effects) as one layer. Explicit ordering constraints are included.
    pub fn dependency_depth(&self) -> usize {
        let mut depths = BTreeMap::new();
        for id in &self.schedule {
            let node = self.nodes[id];
            let depth = self
                .graph
                .edges_directed(node, Direction::Incoming)
                .map(|edge| depths[&self.graph[edge.source()]])
                .max()
                .unwrap_or(0)
                + 1;
            depths.insert(*id, depth);
        }
        depths.values().copied().max().unwrap_or(0)
    }
    pub fn has_dependency(&self, before: OccurrenceId, after: OccurrenceId) -> Result<bool> {
        let a = *self.nodes.get(&before).ok_or(Error::InvalidId)?;
        let b = *self.nodes.get(&after).ok_or(Error::InvalidId)?;
        Ok(petgraph::algo::has_path_connecting(&self.graph, a, b, None))
    }
    pub fn into_unitary(self) -> Result<UnitaryCircuit> {
        if self.occurrences.iter().all(|o| o.operation.exact_unitary()) {
            Ok(UnitaryCircuit(self))
        } else {
            Err(Error::NotUnitary)
        }
    }
    pub fn bind(self, bindings: &[(ParameterId, f64)]) -> Result<BoundProgram> {
        let mut values = BTreeMap::new();
        for (id, value) in bindings {
            if id.owner != self.owner
                || id.index >= self.parameters.len()
                || !value.is_finite()
                || values.insert(*id, *value).is_some()
            {
                return Err(Error::Binding);
            }
        }
        if values.len() != self.parameters.len() {
            return Err(Error::Binding);
        }
        let by_id: BTreeMap<_, _> = self.occurrences.iter().map(|o| (o.id, o)).collect();
        let instructions = self
            .schedule
            .iter()
            .map(|id| {
                let o = by_id.get(id).ok_or(Error::InvalidId)?;
                Ok(Instruction {
                    id: *id,
                    provenance: o.provenance.clone(),
                    source: o.source.clone(),
                    operation: o.operation.bind(&values)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(BoundProgram {
            num_qubits: self.num_qubits,
            num_bits: self.num_bits,
            instructions,
            bindings: values,
            limits: self.limits,
            dependencies: self
                .graph
                .edge_indices()
                .map(|edge| {
                    let (a, b) = self.graph.edge_endpoints(edge).expect("live graph edge");
                    (self.graph[a], self.graph[b])
                })
                .collect(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct UnitaryCircuit(ValidatedProgram);
impl UnitaryCircuit {
    pub fn into_program(self) -> ValidatedProgram {
        self.0
    }
    pub fn program(&self) -> &ValidatedProgram {
        &self.0
    }
    pub fn adjoint(self) -> Result<Self> {
        let mut p = self.0;
        p.occurrences.reverse();
        // Adjoint is a new composition: occurrence identities/provenance remain
        // stable, while their scheduling order follows the reversed circuit.
        for o in &mut p.occurrences {
            match &mut o.operation {
                SemanticOperation::Gate { gate, .. } => *gate = gate.adjoint(),
                SemanticOperation::GlobalPhase { angle, .. } => *angle = angle.negated(),
                SemanticOperation::Barrier { .. } => {}
                _ => return Err(Error::NotUnitary),
            }
        }
        // Quantum hazards are rebuilt in reverse composition order; reversing
        // explicit edges retains their semantics without serializing disjoint
        // operations that had no dependency in the original circuit.
        p.explicit_edges = p.explicit_edges.into_iter().map(|(a, b)| (b, a)).collect();
        Ok(Self(ValidatedProgram::from_parts(
            p.owner,
            p.num_qubits,
            p.num_bits,
            p.parameters,
            p.occurrences,
            p.explicit_edges,
            p.limits,
        )?))
    }
    /// Coherently controls the complete circuit, including its global phase.
    /// Control qubits must belong to this program and be untouched by its body.
    pub fn controlled(self, controls: &[Control]) -> Result<Self> {
        let mut p = self.0;
        let mut unique = BTreeSet::new();
        for c in controls {
            if c.qubit().owner != p.owner || c.qubit().index >= p.num_qubits {
                return Err(Error::InvalidId);
            }
            if !unique.insert(c.qubit()) {
                return Err(Error::DuplicateOperand);
            }
        }
        for o in &mut p.occurrences {
            if !matches!(o.operation, SemanticOperation::Barrier { .. })
                && o.operation.qubits().iter().any(|q| unique.contains(q))
            {
                return Err(Error::DuplicateOperand);
            }
            match &mut o.operation {
                SemanticOperation::Gate {
                    controls: existing, ..
                }
                | SemanticOperation::GlobalPhase {
                    controls: existing, ..
                } => {
                    existing.extend_from_slice(controls);
                    existing.sort();
                }
                SemanticOperation::Barrier { qubits } => {
                    for c in controls {
                        if !qubits.contains(&c.qubit()) {
                            qubits.push(c.qubit());
                        }
                    }
                }
                _ => return Err(Error::NotUnitary),
            }
        }
        Ok(Self(ValidatedProgram::from_parts(
            p.owner,
            p.num_qubits,
            p.num_bits,
            p.parameters,
            p.occurrences,
            p.explicit_edges,
            p.limits,
        )?))
    }
}

#[derive(Debug, Clone)]
pub struct BoundProgram {
    pub(crate) num_qubits: usize,
    pub(crate) num_bits: usize,
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) bindings: BTreeMap<ParameterId, f64>,
    pub(crate) limits: ProgramLimits,
    pub(crate) dependencies: Vec<(OccurrenceId, OccurrenceId)>,
}
impl BoundProgram {
    pub const fn num_qubits(&self) -> usize {
        self.num_qubits
    }
    pub const fn num_bits(&self) -> usize {
        self.num_bits
    }
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }
    /// Longest retained dependency path, with every operation costing one.
    pub fn dependency_depth(&self) -> usize {
        let mut incoming: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (a, b) in &self.dependencies {
            incoming.entry(*b).or_default().push(*a);
        }
        let mut depths = BTreeMap::new();
        for instruction in &self.instructions {
            let depth = incoming
                .get(&instruction.id)
                .into_iter()
                .flatten()
                .map(|a| depths[a])
                .max()
                .unwrap_or(0)
                + 1;
            depths.insert(instruction.id, depth);
        }
        depths.values().copied().max().unwrap_or(0)
    }
    pub fn lower(self) -> Result<LoweredProgram> {
        i32::try_from(self.num_qubits).map_err(|_| Error::NativeIndex)?;
        i32::try_from(self.num_bits).map_err(|_| Error::NativeIndex)?;
        Ok(LoweredProgram(self))
    }
}
#[derive(Debug, Clone)]
pub struct LoweredProgram(BoundProgram);
impl LoweredProgram {
    pub fn plan(self) -> Result<ExecutablePlan> {
        Ok(ExecutablePlan(self.0))
    }
}

#[derive(Debug, Clone)]
pub struct ExecutablePlan(BoundProgram);
impl ExecutablePlan {
    pub fn dependency_depth(&self) -> usize {
        self.0.dependency_depth()
    }
    pub const fn num_qubits(&self) -> usize {
        self.0.num_qubits
    }
    pub const fn num_bits(&self) -> usize {
        self.0.num_bits
    }
    pub fn instructions(&self) -> &[Instruction] {
        &self.0.instructions
    }
    pub fn bindings(&self) -> &BTreeMap<ParameterId, f64> {
        &self.0.bindings
    }
    pub fn limits(&self) -> ProgramLimits {
        self.0.limits
    }
    pub fn requires_density_matrix(&self) -> bool {
        self.0
            .instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::Channel { .. }))
    }
    pub fn stochastic_order(&self) -> Vec<OccurrenceId> {
        self.0
            .instructions
            .iter()
            .filter(|i| {
                matches!(
                    i.operation,
                    Operation::Measure { .. } | Operation::Reset { .. } | Operation::Channel { .. }
                )
            })
            .map(|i| i.id)
            .collect()
    }
}
