use super::{
    Angle, BitId, Control, Error, Gate, GateDefinitionId, Instruction, MatrixPolicy,
    NumericalOperator, OccurrenceId, Operation, ParameterId, QubitId, Result, SourceSpan,
    model::{Occurrence, SemanticOperation},
};
use petgraph::{
    Direction,
    stable_graph::{NodeIndex, StableDiGraph},
    visit::{EdgeRef, IntoEdgeReferences},
};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

static NEXT_PROGRAM: AtomicU64 = AtomicU64::new(1);
static NEXT_SNAPSHOT: AtomicU64 = AtomicU64::new(1);
static NEXT_BOUND_SNAPSHOT: AtomicU64 = AtomicU64::new(1);

/// Mint an identity for a newly published immutable program snapshot.
/// # Errors
/// Rejects exhausted snapshot identifiers.
pub fn fresh_snapshot_id() -> Result<RegionSnapshotId> {
    let id = NEXT_SNAPSHOT
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| Error::Budget("snapshot identifiers"))?;
    Ok(RegionSnapshotId(id))
}
/// Mint an identity for one new bound publication.
/// # Errors
/// Rejects exhausted bound snapshot identifiers.
pub fn fresh_bound_snapshot_id() -> Result<BoundSnapshotId> {
    let id = NEXT_BOUND_SNAPSHOT
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| Error::Budget("bound snapshot identifiers"))?;
    Ok(BoundSnapshotId(id))
}

fn add_retained(total: &mut usize, addition: usize) -> Result<()> {
    *total = total
        .checked_add(addition)
        .ok_or(Error::Budget("program storage"))?;
    Ok(())
}
#[derive(Debug, Clone, Copy)]
pub struct ProgramLimits {
    pub max_qubits: usize,
    pub max_bits: usize,
    pub max_operations: usize,
    pub max_matrix_bytes: usize,
    /// Maximum retained rewrite-history storage.
    pub max_provenance_bytes: usize,
}
impl Default for ProgramLimits {
    fn default() -> Self {
        Self {
            max_qubits: 1024,
            max_bits: 1 << 20,
            max_operations: 1 << 20,
            max_matrix_bytes: 64 * 1024 * 1024,
            max_provenance_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub struct QuantumRegionBuilder {
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
    provenance: super::ProvenanceGraph,
}
impl QuantumRegionBuilder {
    /// # Errors
    /// Rejects dimensions or ranges outside their supported bounds.
    pub fn new(num_qubits: usize, num_bits: usize) -> Result<Self> {
        Self::with_limits(num_qubits, num_bits, ProgramLimits::default())
    }
    /// # Errors
    /// Rejects dimensions that exceed the supplied program limits.
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
            provenance: super::ProvenanceGraph::default(),
        })
    }
    #[must_use]
    pub const fn num_qubits(&self) -> usize {
        self.num_qubits
    }
    #[must_use]
    pub const fn num_bits(&self) -> usize {
        self.num_bits
    }
    /// # Errors
    /// Rejects an index outside this program's qubit range.
    pub const fn qubit(&self, index: usize) -> Result<QubitId> {
        if index >= self.num_qubits {
            Err(Error::InvalidId)
        } else {
            Ok(QubitId {
                owner: self.owner,
                index,
            })
        }
    }
    /// # Errors
    /// Rejects an index outside this program's classical bit range.
    pub const fn bit(&self, index: usize) -> Result<BitId> {
        if index >= self.num_bits {
            Err(Error::InvalidId)
        } else {
            Ok(BitId {
                owner: self.owner,
                index,
            })
        }
    }
    /// # Errors
    /// Rejects empty or duplicate names and exhausted parameter limits.
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
    /// # Errors
    /// Rejects duplicate names or a definition exceeding the program limits.
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
    /// Append one retained shared oracle occurrence.
    /// # Errors
    /// Rejects invalid operands, arity and exhausted occurrence limits.
    pub fn oracle(
        &mut self,
        fragment: &super::OracleFragment,
        targets: &[QubitId],
        controls: &[Control],
    ) -> Result<OccurrenceId> {
        fragment.check_operands(targets, controls)?;
        let controls = self.operands(targets, controls)?;
        self.push(SemanticOperation::Oracle {
            fragment: fragment.clone(),
            targets: targets.into(),
            controls,
        })
    }
    /// # Errors
    /// Rejects invalid identifiers, arity or parameter mismatches, overlapping operands, and exhausted expansion limits.
    #[expect(
        clippy::too_many_lines,
        reason = "Definition expansion validates and stages a single transactional append"
    )]
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
            let mapped = super::model::remap_operands(o.operation.operands(), arguments, controls)?;
            let operation = match &o.operation {
                SemanticOperation::Gate { gate, .. } => self.gate_operation(
                    gate.substitute(&bindings, self.owner)?,
                    &mapped.targets,
                    &mapped.controls,
                )?,
                SemanticOperation::GlobalPhase { angle, .. } => SemanticOperation::GlobalPhase {
                    angle: angle.substitute(&bindings, self.owner)?,
                    controls: self.operands(&[], &mapped.controls)?,
                },
                SemanticOperation::Barrier { .. } => SemanticOperation::Barrier {
                    qubits: mapped.scope(),
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
                Ok((
                    *id,
                    OccurrenceId {
                        owner: self.owner,
                        index: self
                            .occurrences
                            .len()
                            .checked_add(offset)
                            .ok_or(Error::Budget("definition expansion"))?,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
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
        let ids = (0..expanded.len())
            .map(|offset| {
                Ok(OccurrenceId {
                    owner: self.owner,
                    index: self
                        .occurrences
                        .len()
                        .checked_add(offset)
                        .ok_or(Error::Budget("definition expansion"))?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.occurrences
            .try_reserve(expanded.len())
            .map_err(|_| Error::Budget("definition allocation"))?;
        self.explicit_edges
            .try_reserve(explicit_edges.len())
            .map_err(|_| Error::Budget("definition allocation"))?;
        let histories = self
            .provenance
            .sources(&ids, self.limits.max_provenance_bytes)?;
        for ((&id, provenance), (operation, source)) in ids.iter().zip(histories).zip(expanded) {
            self.occurrences.push(Occurrence {
                id,
                provenance,
                source,
                operation,
            });
        }
        self.explicit_edges.extend(explicit_edges);
        Ok(ids)
    }
    const fn check_qubit(&self, q: QubitId) -> Result<()> {
        let in_bounds = q.index < self.num_qubits;
        if in_bounds && q.owner == self.owner {
            Ok(())
        } else {
            Err(Error::InvalidId)
        }
    }
    const fn check_bit(&self, b: BitId) -> Result<()> {
        let in_bounds = b.index < self.num_bits;
        if in_bounds && b.owner == self.owner {
            Ok(())
        } else {
            Err(Error::InvalidId)
        }
    }
    fn check_angle(&self, a: &Angle) -> Result<()> {
        let mut ids = vec![];
        a.parameters(&mut ids)?;
        if ids
            .iter()
            .any(|x| x.owner != self.owner || x.index >= self.parameters.len())
        {
            Err(Error::InvalidId)
        } else {
            Ok(())
        }
    }
    fn operands(&self, targets: &[QubitId], controls: &[Control]) -> Result<Arc<[Control]>> {
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
        Ok(controls.into())
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
            targets: targets.into(),
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
            provenance: self
                .provenance
                .source(id, self.limits.max_provenance_bytes)?,
            source: self.source.clone(),
            operation,
        });
        Ok(id)
    }
    /// # Errors
    /// Rejects invalid identifiers, gate arity, angles, overlapping operands, or exhausted operation limits.
    pub fn gate(
        &mut self,
        gate: Gate,
        targets: &[QubitId],
        controls: &[Control],
    ) -> Result<OccurrenceId> {
        let operation = self.gate_operation(gate, targets, controls)?;
        self.push(operation)
    }
    /// # Errors
    /// Rejects invalid condition or gate operands, angles, arity, or exhausted operation limits.
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
    /// # Errors
    /// Rejects invalid angles, control identifiers, duplicate controls, or exhausted operation limits.
    pub fn global_phase(&mut self, angle: Angle, controls: &[Control]) -> Result<OccurrenceId> {
        self.check_angle(&angle)?;
        let controls = self.operands(&[], controls)?;
        self.push(SemanticOperation::GlobalPhase { angle, controls })
    }
    /// # Errors
    /// Rejects invalid qubit or bit identifiers and exhausted operation limits.
    pub fn measure(&mut self, qubit: QubitId, bit: BitId) -> Result<OccurrenceId> {
        self.check_qubit(qubit)?;
        self.check_bit(bit)?;
        self.push(SemanticOperation::Measure { qubit, bit })
    }
    /// # Errors
    /// Rejects an invalid qubit identifier or exhausted operation limits.
    pub fn reset(&mut self, qubit: QubitId) -> Result<OccurrenceId> {
        self.check_qubit(qubit)?;
        self.push(SemanticOperation::Reset { qubit })
    }
    /// An empty scope is a whole-register barrier.
    /// # Errors
    /// Rejects invalid or duplicate qubits and exhausted operation limits.
    pub fn barrier(&mut self, qubits: &[QubitId]) -> Result<OccurrenceId> {
        let qubits = if qubits.is_empty() {
            (0..self.num_qubits)
                .map(|i| self.qubit(i))
                .collect::<Result<Vec<_>>>()?
        } else {
            qubits.to_vec()
        };
        self.operands(&qubits, &[])?;
        self.push(SemanticOperation::Barrier {
            qubits: qubits.into(),
        })
    }
    /// # Errors
    /// Rejects operand or matrix dimensions, invalid identifiers, and exhausted resource limits.
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
            targets: targets.into(),
            controls,
        })?;
        self.matrix_bytes = bytes;
        Ok(id)
    }
    /// # Errors
    /// Rejects invalid operands, Kraus dimensions, completeness tolerance, or exhausted resource limits.
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
        super::matrix::check_channel(
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
            kraus: kraus.into(),
            targets: targets.into(),
        })?;
        self.matrix_bytes = bytes;
        Ok(id)
    }
    /// Adds an explicit order constraint. Acyclicity is established at finish.
    /// # Errors
    /// Rejects occurrence identifiers that do not belong to this program.
    pub fn depend(&mut self, before: OccurrenceId, after: OccurrenceId) -> Result<()> {
        for id in [before, after] {
            if id.owner != self.owner || id.index >= self.occurrences.len() {
                return Err(Error::InvalidId);
            }
        }
        self.explicit_edges.push((before, after));
        Ok(())
    }
    /// # Errors
    /// Rejects cyclic dependencies or invalid retained identifiers.
    pub fn finish(self) -> Result<QuantumRegion> {
        QuantumRegion::from_parts(
            self.owner,
            self.num_qubits,
            self.num_bits,
            self.parameters,
            self.occurrences,
            self.explicit_edges,
            self.limits,
            Arc::new(self.provenance),
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
/// A retained mandatory ordering edge and the reason it exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DependencyEdge {
    pub before: OccurrenceId,
    pub after: OccurrenceId,
    pub kind: DependencyKind,
}
/// Identity of the admitted ideal program from which a bound plan arose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RegionSnapshotId(u64);
impl RegionSnapshotId {
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}
/// Identity of one immutable bound program publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BoundSnapshotId(u64);
impl BoundSnapshotId {
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct QuantumRegion {
    pub(crate) owner: u64,
    snapshot_id: RegionSnapshotId,
    pub(crate) num_qubits: usize,
    pub(crate) num_bits: usize,
    pub(crate) parameters: Vec<String>,
    pub(crate) occurrences: Vec<Occurrence>,
    pub(crate) explicit_edges: Vec<(OccurrenceId, OccurrenceId)>,
    pub(crate) limits: ProgramLimits,
    pub(crate) provenance: Arc<super::ProvenanceGraph>,
    graph: StableDiGraph<OccurrenceId, DependencyKind>,
    nodes: BTreeMap<OccurrenceId, NodeIndex>,
    schedule: Vec<OccurrenceId>,
}
/// A general admitted program, without exact-unitary privileges.
impl QuantumRegion {
    /// Conservative bind-work allowance derived from every original source DAG.
    /// # Errors
    /// Rejects work arithmetic overflow.
    pub fn binding_work_estimate(&self) -> Result<u64> {
        let count = u64::try_from(self.parameters.len())
            .map_err(|_| Error::Budget("binding work"))?
            .checked_add(
                u64::try_from(self.schedule.len()).map_err(|_| Error::Budget("binding work"))?,
            )
            .and_then(|count| count.checked_add(u64::try_from(self.graph.edge_count()).ok()?))
            .ok_or(Error::Budget("binding work"))?;
        let mut work = count.checked_mul(64).ok_or(Error::Budget("binding work"))?;
        for occurrence in &self.occurrences {
            work = work
                .checked_add(occurrence.operation.binding_work_estimate()?)
                .ok_or(Error::Budget("binding work"))?;
        }
        Ok(work)
    }
    /// Conservative live ideal-program bytes used for pre-bind reservation.
    /// # Errors
    /// Rejects size arithmetic overflow.
    pub fn retained_bytes(&self) -> Result<usize> {
        let mut bytes = size_of::<Self>();
        add_retained(
            &mut bytes,
            self.parameters
                .capacity()
                .checked_mul(size_of::<String>())
                .ok_or(Error::Budget("ideal storage"))?,
        )?;
        for name in &self.parameters {
            add_retained(&mut bytes, name.capacity())?;
        }
        add_retained(
            &mut bytes,
            self.occurrences
                .capacity()
                .checked_mul(size_of::<Occurrence>())
                .ok_or(Error::Budget("ideal storage"))?,
        )?;
        for occurrence in &self.occurrences {
            add_retained(&mut bytes, occurrence.operation.retained_bytes()?)?;
            if let Some(source) = &occurrence.source {
                add_retained(
                    &mut bytes,
                    source
                        .source()
                        .len()
                        .checked_add(const { 3 * size_of::<usize>() })
                        .ok_or(Error::Budget("ideal storage"))?,
                )?;
            }
        }
        add_retained(
            &mut bytes,
            self.explicit_edges
                .capacity()
                .checked_mul(size_of::<(OccurrenceId, OccurrenceId)>())
                .ok_or(Error::Budget("ideal storage"))?,
        )?;
        add_retained(
            &mut bytes,
            self.schedule
                .capacity()
                .checked_mul(size_of::<OccurrenceId>())
                .ok_or(Error::Budget("ideal storage"))?,
        )?;
        let (nodes, edges) = self.graph.capacity();
        add_retained(
            &mut bytes,
            nodes
                .checked_mul(size_of::<(OccurrenceId, [usize; 6])>())
                .ok_or(Error::Budget("ideal graph storage"))?,
        )?;
        add_retained(
            &mut bytes,
            edges
                .checked_mul(size_of::<(DependencyKind, [usize; 8])>())
                .ok_or(Error::Budget("ideal graph storage"))?,
        )?;
        add_retained(
            &mut bytes,
            self.nodes
                .len()
                .checked_mul(
                    const { size_of::<(OccurrenceId, NodeIndex)>() + 5 * size_of::<usize>() },
                )
                .ok_or(Error::Budget("ideal graph storage"))?,
        )?;
        add_retained(&mut bytes, self.provenance.retained_bytes()?)?;
        Ok(bytes)
    }
    #[must_use]
    pub const fn snapshot_id(&self) -> RegionSnapshotId {
        self.snapshot_id
    }
    #[must_use]
    pub fn dependency_count(&self) -> usize {
        self.graph.edge_count()
    }
    /// Borrowed graph order is converted into owned, typed mandatory edges.
    #[must_use]
    pub fn dependencies(&self) -> Vec<DependencyEdge> {
        self.graph
            .edge_references()
            .map(|edge| DependencyEdge {
                before: self.graph[edge.source()],
                after: self.graph[edge.target()],
                kind: *edge.weight(),
            })
            .collect()
    }
    #[must_use]
    pub fn provenance(&self) -> &super::ProvenanceGraph {
        &self.provenance
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Central reconstruction validates the graph, source history, resources, and occurrence identities together"
    )]
    /// # Errors
    /// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
    #[expect(
        clippy::too_many_lines,
        reason = "Validate ownership, payloads, dependencies and publication together"
    )]
    pub fn from_parts(
        owner: u64,
        num_qubits: usize,
        num_bits: usize,
        parameters: Vec<String>,
        occurrences: Vec<Occurrence>,
        explicit_edges: Vec<(OccurrenceId, OccurrenceId)>,
        limits: ProgramLimits,
        provenance: Arc<super::ProvenanceGraph>,
    ) -> Result<Self> {
        let mut validator = QuantumRegionBuilder::with_limits(num_qubits, num_bits, limits)?;
        validator.owner = owner;
        if parameters.iter().any(String::is_empty)
            || parameters.iter().collect::<BTreeSet<_>>().len() != parameters.len()
        {
            return Err(Error::ParameterName);
        }
        validator.parameters.clone_from(&parameters);
        if occurrences.len() > limits.max_operations {
            return Err(Error::Budget("operation count"));
        }
        let mut ids = BTreeSet::new();
        let mut matrix_bytes = 0usize;
        for occurrence in &occurrences {
            if occurrence.id.owner != owner || !ids.insert(occurrence.id) {
                return Err(Error::InvalidId);
            }
            validator.validate_operation(&occurrence.operation, 0)?;
            matrix_bytes = matrix_bytes
                .checked_add(occurrence.operation.retained_bytes()?)
                .ok_or(Error::Budget("region storage"))?;
        }
        // The operation storage includes scalar/operand metadata as well as matrices.
        let storage_limit = limits
            .max_matrix_bytes
            .saturating_add(limits.max_operations.saturating_mul(4096));
        if matrix_bytes > storage_limit {
            return Err(Error::Budget("region storage"));
        }
        if provenance.retained_bytes()? > limits.max_provenance_bytes {
            return Err(Error::Budget("provenance storage"));
        }
        for item in &occurrences {
            provenance.node(item.provenance)?;
        }
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
                *degree = degree.checked_sub(1).ok_or(Error::Cycle)?;
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
            snapshot_id: fresh_snapshot_id()?,
            num_qubits,
            num_bits,
            parameters,
            occurrences,
            explicit_edges,
            limits,
            provenance,
            graph,
            nodes,
            schedule,
        })
    }
    #[must_use]
    pub const fn num_qubits(&self) -> usize {
        self.num_qubits
    }
    #[must_use]
    pub const fn num_bits(&self) -> usize {
        self.num_bits
    }
    /// # Errors
    /// Rejects an index outside this program's qubit range.
    pub const fn qubit(&self, index: usize) -> Result<QubitId> {
        if index >= self.num_qubits {
            Err(Error::InvalidId)
        } else {
            Ok(QubitId {
                owner: self.owner,
                index,
            })
        }
    }
    /// # Errors
    /// Rejects an index outside this program's classical bit range.
    pub const fn bit(&self, index: usize) -> Result<BitId> {
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
    #[must_use]
    pub fn schedule(&self) -> &[OccurrenceId] {
        &self.schedule
    }
    /// Longest dependency path, counting each operation (including barriers
    /// and effects) as one layer. Explicit ordering constraints are included.
    #[must_use]
    pub fn dependency_depth(&self) -> usize {
        let mut depths = BTreeMap::new();
        for id in &self.schedule {
            // Every scheduled ID and predecessor is admitted by build().
            let depth = self
                .nodes
                .get(id)
                .into_iter()
                .flat_map(|node| self.graph.edges_directed(*node, Direction::Incoming))
                .filter_map(|edge| self.graph.node_weight(edge.source()))
                .filter_map(|predecessor| depths.get(predecessor).copied())
                .max()
                .unwrap_or(0usize)
                .saturating_add(1);
            depths.insert(*id, depth);
        }
        depths.values().copied().max().unwrap_or(0)
    }
    /// # Errors
    /// Rejects occurrence identifiers that do not belong to this program.
    pub fn has_dependency(&self, before: OccurrenceId, after: OccurrenceId) -> Result<bool> {
        let a = *self.nodes.get(&before).ok_or(Error::InvalidId)?;
        let b = *self.nodes.get(&after).ok_or(Error::InvalidId)?;
        Ok(petgraph::algo::has_path_connecting(&self.graph, a, b, None))
    }
    /// # Errors
    /// Rejects effects and numerical operators without exact unitary semantics.
    pub fn into_unitary(self) -> Result<UnitaryCircuit> {
        if self.occurrences.iter().all(|o| o.operation.exact_unitary()) {
            Ok(UnitaryCircuit(self))
        } else {
            Err(Error::NotUnitary)
        }
    }
    /// # Errors
    /// Rejects missing, duplicate, foreign, or nonfinite bindings and nonfinite angle evaluation.
    pub fn bind(self, bindings: &[(ParameterId, f64)]) -> Result<BoundRegion> {
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
                let (operation, angle_targets) = o.operation.bind(&values)?;
                Ok(Instruction {
                    id: *id,
                    provenance: o.provenance,
                    source: o.source.clone(),
                    operation,
                    angle_targets: angle_targets.into(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let dependencies = self.dependencies();
        Ok(BoundRegion {
            num_qubits: self.num_qubits,
            num_bits: self.num_bits,
            source_snapshot_id: self.snapshot_id,
            snapshot_id: fresh_bound_snapshot_id()?,
            instructions,
            bindings: values,
            provenance: self.provenance,
            limits: self.limits,
            dependencies,
        })
    }
}

#[derive(Debug, Clone)]
pub struct UnitaryCircuit(QuantumRegion);
impl UnitaryCircuit {
    #[must_use]
    pub fn into_program(self) -> QuantumRegion {
        self.0
    }
    #[must_use]
    pub const fn program(&self) -> &QuantumRegion {
        &self.0
    }
    /// # Errors
    /// Rejects invalid retained identifiers or cyclic dependencies when rebuilding the adjoint.
    pub fn adjoint(self) -> Result<Self> {
        let mut p = self.0;
        p.occurrences.reverse();
        // Adjoint is a new composition: occurrence identities/provenance remain
        // stable, while their scheduling order follows the reversed circuit.
        for o in &mut p.occurrences {
            match &mut o.operation {
                SemanticOperation::Gate { gate, .. } => *gate = gate.adjoint()?,
                SemanticOperation::GlobalPhase { angle, .. } => *angle = angle.negated()?,
                SemanticOperation::Barrier { .. } => {}
                _ => return Err(Error::NotUnitary),
            }
        }
        // Quantum hazards are rebuilt in reverse composition order; reversing
        // explicit edges retains their semantics without serializing disjoint
        // operations that had no dependency in the original circuit.
        p.explicit_edges = p.explicit_edges.into_iter().map(|(a, b)| (b, a)).collect();
        Ok(Self(QuantumRegion::from_parts(
            p.owner,
            p.num_qubits,
            p.num_bits,
            p.parameters,
            p.occurrences,
            p.explicit_edges,
            p.limits,
            p.provenance,
        )?))
    }
    /// Coherently controls the complete circuit, including its global phase.
    /// Control qubits must belong to this program and be untouched by its body.
    /// # Errors
    /// Rejects invalid or overlapping controls, unsupported operations, and invalid rebuilt dependencies.
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
                && o.operation.qubits().any(|q| unique.contains(&q))
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
                    let mut combined = existing
                        .iter()
                        .copied()
                        .chain(controls.iter().copied())
                        .collect::<Vec<_>>();
                    combined.sort();
                    *existing = combined.into();
                }
                SemanticOperation::Barrier { qubits } => {
                    let mut combined = qubits.to_vec();
                    for c in controls {
                        if !combined.contains(&c.qubit()) {
                            combined.push(c.qubit());
                        }
                    }
                    *qubits = combined.into();
                }
                _ => return Err(Error::NotUnitary),
            }
        }
        Ok(Self(QuantumRegion::from_parts(
            p.owner,
            p.num_qubits,
            p.num_bits,
            p.parameters,
            p.occurrences,
            p.explicit_edges,
            p.limits,
            p.provenance,
        )?))
    }
}

#[derive(Debug, Clone)]
pub struct BoundRegion {
    pub(crate) num_qubits: usize,
    pub(crate) num_bits: usize,
    pub(crate) source_snapshot_id: RegionSnapshotId,
    pub(crate) snapshot_id: BoundSnapshotId,
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) bindings: BTreeMap<ParameterId, f64>,
    pub(crate) limits: ProgramLimits,
    pub(crate) dependencies: Vec<DependencyEdge>,
    pub(crate) provenance: Arc<super::ProvenanceGraph>,
}
impl BoundRegion {
    /// Conservative live bound-program bytes used by optimizer admission.
    /// # Errors
    /// Rejects size arithmetic overflow.
    pub fn retained_bytes(&self) -> Result<usize> {
        self.retained_bytes_with(&mut super::RetainedStorage::default())
    }
    /// # Errors
    /// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
    pub fn retained_bytes_with(&self, storage: &mut super::RetainedStorage) -> Result<usize> {
        let mut bytes = size_of::<Self>();
        add_retained(
            &mut bytes,
            self.instructions
                .capacity()
                .checked_mul(size_of::<Instruction>())
                .ok_or(Error::Budget("bound storage"))?,
        )?;
        add_retained(
            &mut bytes,
            self.dependencies
                .capacity()
                .checked_mul(size_of::<DependencyEdge>())
                .ok_or(Error::Budget("bound storage"))?,
        )?;
        add_retained(
            &mut bytes,
            self.bindings
                .len()
                .checked_mul(const { size_of::<(ParameterId, f64)>() + 5 * size_of::<usize>() })
                .ok_or(Error::Budget("bound storage"))?,
        )?;
        add_retained(&mut bytes, self.provenance.retained_bytes()?)?;
        for instruction in &self.instructions {
            if let Some(source) = &instruction.source {
                add_retained(
                    &mut bytes,
                    source
                        .source()
                        .len()
                        .checked_add(const { 3 * size_of::<usize>() })
                        .ok_or(Error::Budget("bound storage"))?,
                )?;
            }
            add_retained(&mut bytes, instruction.operation.operand_storage_bytes()?)?;
            add_retained(&mut bytes, storage.operation(&instruction.operation)?)?;
            add_retained(&mut bytes, instruction.angle_target_storage_bytes()?)?;
        }
        Ok(bytes)
    }
    #[must_use]
    pub const fn snapshot_id(&self) -> BoundSnapshotId {
        self.snapshot_id
    }
    #[must_use]
    pub const fn source_snapshot_id(&self) -> RegionSnapshotId {
        self.source_snapshot_id
    }
    #[must_use]
    pub fn dependencies(&self) -> &[DependencyEdge] {
        &self.dependencies
    }
    #[must_use]
    pub fn provenance(&self) -> &super::ProvenanceGraph {
        &self.provenance
    }
    #[must_use]
    pub const fn num_qubits(&self) -> usize {
        self.num_qubits
    }
    #[must_use]
    pub const fn num_bits(&self) -> usize {
        self.num_bits
    }
    #[must_use]
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }
    /// Longest retained dependency path, with every operation costing one.
    #[must_use]
    pub fn dependency_depth(&self) -> usize {
        let mut incoming: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for edge in &self.dependencies {
            incoming.entry(edge.after).or_default().push(edge.before);
        }
        let mut depths = BTreeMap::new();
        for instruction in &self.instructions {
            let depth = incoming
                .get(&instruction.id)
                .into_iter()
                .flatten()
                .filter_map(|a| depths.get(a).copied())
                .max()
                // The path length is bounded by the admitted instruction count.
                .unwrap_or(0usize)
                .saturating_add(1);
            depths.insert(instruction.id, depth);
        }
        depths.values().copied().max().unwrap_or(0)
    }
    /// Check native index representation and publish the executable plan.
    /// # Errors
    /// Rejects wire counts that cannot be represented by native indices.
    pub fn plan(self) -> Result<RegionPlan> {
        i32::try_from(self.num_qubits).map_err(|_| Error::NativeIndex)?;
        i32::try_from(self.num_bits).map_err(|_| Error::NativeIndex)?;
        let positions: BTreeMap<_, _> = self
            .instructions
            .iter()
            .enumerate()
            .map(|(index, instruction)| (instruction.id, index))
            .collect();
        if positions.len() != self.instructions.len() {
            return Err(Error::InvalidId);
        }
        for edge in &self.dependencies {
            let before = positions.get(&edge.before).ok_or(Error::InvalidId)?;
            let after = positions.get(&edge.after).ok_or(Error::InvalidId)?;
            if before >= after {
                return Err(Error::Cycle);
            }
        }
        Ok(RegionPlan(self))
    }
}

#[cfg(test)]
mod plan_order_tests {
    use super::QuantumRegionBuilder;
    use super::{Error, Gate};
    use googletest::{Result, prelude::*};

    #[gtest]
    fn plan_rejects_reordered_mandatory_edge() -> Result<()> {
        let mut builder = QuantumRegionBuilder::new(1, 0)?;
        let qubit = builder.qubit(0)?;
        builder.gate(Gate::H, &[qubit], &[])?;
        builder.gate(Gate::X, &[qubit], &[])?;
        let mut bound = builder.finish()?.bind(&[])?;
        bound.instructions.swap(0, 1);
        expect_true!(matches!(bound.plan(), Err(Error::Cycle)));
        Ok(())
    }

    #[gtest]
    fn distinct_parameter_bindings_have_distinct_bound_snapshots() -> Result<()> {
        let mut builder = QuantumRegionBuilder::new(1, 0)?;
        let parameter = builder.parameter("theta")?;
        builder.gate(
            Gate::Rz(super::Angle::parameter(parameter)?),
            &[builder.qubit(0)?],
            &[],
        )?;
        let source = builder.finish()?;
        let zero = source.clone().bind(&[(parameter, 0.0)])?;
        let pi = source.bind(&[(parameter, std::f64::consts::PI)])?;
        expect_eq!(zero.source_snapshot_id(), pi.source_snapshot_id());
        expect_ne!(zero.snapshot_id(), pi.snapshot_id());
        let clone = zero.clone();
        expect_eq!(zero.snapshot_id(), clone.snapshot_id());
        let _plan = clone.plan()?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct RegionPlan(BoundRegion);
impl RegionPlan {
    /// Return the specialized finite capability for import into the common Program lifecycle.
    #[must_use]
    pub fn into_region(self) -> BoundRegion {
        self.0
    }

    #[must_use]
    pub const fn snapshot_id(&self) -> BoundSnapshotId {
        self.0.snapshot_id
    }
    #[must_use]
    pub const fn source_snapshot_id(&self) -> RegionSnapshotId {
        self.0.source_snapshot_id
    }
    #[must_use]
    pub fn dependencies(&self) -> &[DependencyEdge] {
        &self.0.dependencies
    }
    #[must_use]
    pub fn provenance(&self) -> &super::ProvenanceGraph {
        self.0.provenance()
    }
    /// Count retained oracle occurrences recursively, including each outer call.
    /// # Errors
    /// Rejects query-count overflow.
    pub fn oracle_query_count(&self) -> Result<usize> {
        self.instructions()
            .iter()
            .try_fold(0usize, |total, instruction| {
                let count = match instruction.operation() {
                    Operation::Oracle { fragment, .. } => fragment.query_count(),
                    _ => 0,
                };
                total
                    .checked_add(count)
                    .ok_or(Error::Budget("oracle queries"))
            })
    }

    #[must_use]
    pub fn dependency_depth(&self) -> usize {
        self.0.dependency_depth()
    }
    #[must_use]
    pub const fn num_qubits(&self) -> usize {
        self.0.num_qubits
    }
    #[must_use]
    pub const fn num_bits(&self) -> usize {
        self.0.num_bits
    }
    #[must_use]
    pub fn instructions(&self) -> &[Instruction] {
        &self.0.instructions
    }
    #[must_use]
    pub const fn bindings(&self) -> &BTreeMap<ParameterId, f64> {
        &self.0.bindings
    }
    #[must_use]
    pub const fn limits(&self) -> ProgramLimits {
        self.0.limits
    }
    #[must_use]
    pub fn requires_density_matrix(&self) -> bool {
        self.0
            .instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::Channel { .. }))
    }
    #[must_use]
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

impl QuantumRegion {
    #[must_use]
    pub const fn owner(&self) -> u64 {
        self.owner
    }
    #[must_use]
    pub const fn parameter_storage(&self) -> &Vec<String> {
        &self.parameters
    }
    #[must_use]
    pub const fn occurrences(&self) -> &Vec<super::model::Occurrence> {
        &self.occurrences
    }
    #[must_use]
    pub const fn explicit_edges(&self) -> &Vec<(OccurrenceId, OccurrenceId)> {
        &self.explicit_edges
    }
    #[must_use]
    pub const fn limits(&self) -> ProgramLimits {
        self.limits
    }
    #[must_use]
    pub const fn provenance_arc(&self) -> &Arc<super::ProvenanceGraph> {
        &self.provenance
    }
}
impl BoundRegion {
    #[must_use]
    pub const fn binding_storage(&self) -> &BTreeMap<ParameterId, f64> {
        &self.bindings
    }
    #[must_use]
    pub const fn limits(&self) -> ProgramLimits {
        self.limits
    }
    #[must_use]
    pub const fn provenance_arc(&self) -> &Arc<super::ProvenanceGraph> {
        &self.provenance
    }
    /// Independently check replacement ownership, ordering, provenance and resource limits before publication.
    /// # Errors
    /// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
    pub fn replace_compiled(
        mut self,
        instructions: Vec<Instruction>,
        dependencies: Vec<DependencyEdge>,
        provenance: Arc<super::ProvenanceGraph>,
    ) -> Result<Self> {
        let owner = self
            .instructions
            .first()
            .map(|i| i.id.owner)
            .or_else(|| self.bindings.keys().next().map(|p| p.owner))
            .or_else(|| instructions.first().map(|i| i.id.owner));
        let mut validator =
            QuantumRegionBuilder::with_limits(self.num_qubits, self.num_bits, self.limits)?;
        if let Some(owner) = owner {
            validator.owner = owner;
        }
        let mut ids = BTreeSet::new();
        if instructions.len() > self.limits.max_operations {
            return Err(Error::Budget("operation count"));
        }
        for instruction in &instructions {
            provenance.node(instruction.provenance)?;
            if Some(instruction.id.owner) != owner || !ids.insert(instruction.id) {
                return Err(Error::InvalidId);
            }
            validator.validate_bound_operation(&instruction.operation, 0)?;
        }
        self.instructions = instructions;
        self.dependencies = dependencies;
        self.provenance = provenance;
        self.snapshot_id = fresh_bound_snapshot_id()?;
        self.clone().plan()?;
        if self.retained_bytes()?
            > self
                .limits
                .max_matrix_bytes
                .saturating_add(self.limits.max_provenance_bytes)
                .saturating_add(self.limits.max_operations.saturating_mul(512))
        {
            return Err(Error::Budget("compiled region storage"));
        }
        Ok(self)
    }
}
impl BoundRegion {
    /// Attach compiler provenance. This metadata is not an equivalence witness.
    /// # Errors
    /// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
    pub fn retain_source(
        &mut self,
        snapshot: RegionSnapshotId,
        bindings: BTreeMap<ParameterId, f64>,
    ) -> Result<()> {
        if bindings.values().any(|value| !value.is_finite()) {
            return Err(Error::Binding);
        }
        self.source_snapshot_id = snapshot;
        self.bindings = bindings;
        Ok(())
    }
}

impl QuantumRegionBuilder {
    fn validate_operation(&self, operation: &SemanticOperation, depth: usize) -> Result<()> {
        if depth > 64 {
            return Err(Error::Budget("conditional nesting"));
        }
        match operation {
            SemanticOperation::Gate {
                gate,
                targets,
                controls,
            } => {
                self.gate_operation(gate.clone(), targets, controls)?;
            }
            SemanticOperation::GlobalPhase { angle, controls } => {
                self.check_angle(angle)?;
                self.operands(&[], controls)?;
            }
            SemanticOperation::Numerical {
                matrix,
                targets,
                controls,
            } => {
                self.operands(targets, controls)?;
                if matrix.num_qubits() != targets.len() {
                    return Err(Error::MatrixDimension);
                }
            }
            SemanticOperation::Oracle {
                fragment,
                targets,
                controls,
            } => {
                self.operands(targets, controls)?;
                fragment.check_operands(targets, controls)?;
            }
            SemanticOperation::Measure { qubit, bit } => {
                self.check_qubit(*qubit)?;
                self.check_bit(*bit)?;
            }
            SemanticOperation::Reset { qubit } => self.check_qubit(*qubit)?,
            SemanticOperation::Barrier { qubits } => {
                self.operands(qubits, &[])?;
            }
            SemanticOperation::Channel { kraus, targets } => {
                self.operands(targets, &[])?;
                if kraus.is_empty()
                    || kraus
                        .iter()
                        .any(|matrix| matrix.num_qubits() != targets.len())
                {
                    return Err(Error::MatrixDimension);
                }
            }
            SemanticOperation::Conditional { bit, operation, .. } => {
                self.check_bit(*bit)?;
                self.validate_operation(operation, depth.saturating_add(1))?;
            }
        }
        Ok(())
    }
}

impl QuantumRegionBuilder {
    fn validate_bound_operation(&self, operation: &Operation, depth: usize) -> Result<()> {
        if depth > 64 {
            return Err(Error::Budget("conditional nesting"));
        }
        let semantic = match operation {
            Operation::Gate {
                gate,
                targets,
                controls,
            } => SemanticOperation::Gate {
                gate: Gate::from_bound(gate)?,
                targets: targets.clone(),
                controls: controls.clone(),
            },
            Operation::GlobalPhase { radians, controls } => SemanticOperation::GlobalPhase {
                angle: Angle::radians(*radians)?,
                controls: controls.clone(),
            },
            Operation::Numerical {
                matrix,
                targets,
                controls,
            } => SemanticOperation::Numerical {
                matrix: matrix.clone(),
                targets: targets.clone(),
                controls: controls.clone(),
            },
            Operation::Oracle {
                fragment,
                targets,
                controls,
            } => SemanticOperation::Oracle {
                fragment: fragment.clone(),
                targets: targets.clone(),
                controls: controls.clone(),
            },
            Operation::Channel { kraus, targets } => SemanticOperation::Channel {
                kraus: kraus.clone(),
                targets: targets.clone(),
            },
            Operation::Measure { qubit, bit } => SemanticOperation::Measure {
                qubit: *qubit,
                bit: *bit,
            },
            Operation::Reset { qubit } => SemanticOperation::Reset { qubit: *qubit },
            Operation::Barrier { qubits } => SemanticOperation::Barrier {
                qubits: qubits.clone(),
            },
            Operation::Conditional { bit, operation, .. } => {
                self.check_bit(*bit)?;
                return self.validate_bound_operation(operation, depth.saturating_add(1));
            }
        };
        self.validate_operation(&semantic, depth)
    }
}
