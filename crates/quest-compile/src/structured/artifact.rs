//! Explicit, bounded compiled publications. Digests detect corruption, not equivalence.
use super::{
    CompileLimits, Constructed, Executable, InputSpecialization, LanguageError, MacroLocation,
    Program, ScalarValue, SourceMap, TypedModule, ssa, syntax,
};
use crate::{
    Angle, BoundAngleTarget, BoundGate, Control, ControlState, Gate, MatrixPolicy,
    NumericalOperator, Operation, OracleFragment, QuantumPayload, QuantumRegionBuilder,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[expect(
    clippy::format_collect,
    reason = "The fixed 32-byte digest has bounded formatting allocation"
)]
fn digest(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
const VERSION: u32 = 1;
const CONVENTIONS: &str =
    "quest-ssa-v1;target0-lsb;full-phase;signed-controls;binary64-captures;exact-affine-distinct";
#[derive(Debug, Clone, Copy)]
pub struct ArtifactLimits {
    pub bytes: usize,
    pub compile: CompileLimits,
    pub matrix_bytes: usize,
}
impl Default for ArtifactLimits {
    fn default() -> Self {
        Self {
            bytes: 64 * 1024 * 1024,
            compile: CompileLimits::default(),
            matrix_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("compiled artifact version or conventions are incompatible; rebuild from source")]
    Incompatible,
    #[error("compiled artifact integrity mismatch")]
    Integrity,
    #[error("compiled artifact exceeds its resource limits")]
    Budget,
    #[error("invalid compiled artifact: {0}")]
    Invalid(String),
    #[error(transparent)]
    Language(#[from] LanguageError),
    #[error(transparent)]
    Circuit(#[from] crate::Error),
}
type Result<T> = std::result::Result<T, ArtifactError>;
fn invalid(error: impl std::fmt::Display) -> ArtifactError {
    ArtifactError::Invalid(error.to_string())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    conventions: String,
    digest: String,
    payload: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Publication {
    source: syntax::Module,
    program: ssa::Program,
    publisher: ssa::SnapshotId,
    previous: Vec<ssa::SnapshotId>,
    captures: Vec<ScalarValue>,
    exact: BTreeMap<usize, AngleData>,
    payloads: BTreeMap<usize, PayloadData>,
    oracles: BTreeMap<usize, OracleData>,
    sources: SourceMap,
    locations: Vec<MacroLocation>,
    limits: CompileLimits,
    finite_sources: Vec<FiniteSourceEvidence>,
    evidence: Vec<CompilationEvidence>,
    specializations: Vec<InputSpecialization>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiniteSourceEvidence {
    pub ideal_snapshot: String,
    pub bound_snapshot: String,
    pub bindings: Vec<(u64, usize, u64)>,
    pub occurrences: Vec<OccurrenceEvidence>,
    pub history: Vec<HistoryEvidence>,
    pub certificates: Vec<CompilationEvidence>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrenceEvidence {
    pub owner: u64,
    pub index: usize,
    pub provenance: String,
    pub source: Option<(String, usize, usize)>,
    pub targets: Vec<Option<AngleData>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryEvidence {
    pub id: String,
    pub source: Option<(u64, usize)>,
    pub inputs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AngleData {
    Opaque {
        bits: u64,
    },
    Affine {
        radians_numerator: String,
        radians_denominator: String,
        pi_numerator: String,
        pi_denominator: String,
    },
}
impl AngleData {
    fn target(target: &BoundAngleTarget) -> Self {
        match target {
            BoundAngleTarget::DyadicRadians { bits } => Self::Opaque { bits: *bits },
            BoundAngleTarget::RationalPi {
                numerator,
                denominator,
            } => Self::Affine {
                radians_numerator: "0".into(),
                radians_denominator: "1".into(),
                pi_numerator: numerator.to_string(),
                pi_denominator: denominator.to_string(),
            },
            BoundAngleTarget::AffinePi {
                radians_numerator,
                radians_denominator,
                pi_numerator,
                pi_denominator,
            } => Self::Affine {
                radians_numerator: radians_numerator.to_string(),
                radians_denominator: radians_denominator.to_string(),
                pi_numerator: pi_numerator.to_string(),
                pi_denominator: pi_denominator.to_string(),
            },
        }
    }
    fn angle(&self) -> Result<Angle> {
        fn ratio(n: &str, d: &str) -> Result<crate::BigRational> {
            if n.len() > 5000 || d.len() > 5000 {
                return Err(ArtifactError::Budget);
            }
            let n = n.parse::<num_bigint::BigInt>().map_err(invalid)?;
            let d = d.parse::<num_bigint::BigInt>().map_err(invalid)?;
            if d <= 0.into() {
                return Err(invalid("nonpositive exact denominator"));
            }
            Ok(crate::BigRational::new(n, d))
        }
        Ok(match self {
            Self::Opaque { bits } => Angle::radians(f64::from_bits(*bits)).map_err(invalid)?,
            Self::Affine {
                radians_numerator,
                radians_denominator,
                pi_numerator,
                pi_denominator,
            } => Angle::affine(
                ratio(radians_numerator, radians_denominator)?,
                ratio(pi_numerator, pi_denominator)?,
            )
            .map_err(invalid)?,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatrixData {
    dimension: usize,
    entries: Vec<(u64, u64)>,
}
impl MatrixData {
    fn encode(matrix: &NumericalOperator) -> Self {
        Self {
            dimension: matrix.dimension(),
            entries: (0..matrix.dimension())
                .flat_map(|row| {
                    (0..matrix.dimension()).map(move |col| {
                        let value = matrix.view()[(row, col)];
                        (value.re.to_bits(), value.im.to_bits())
                    })
                })
                .collect(),
        }
    }
    #[expect(
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        reason = "Matrix dimensions and complete entry count are checked before the allocation closure"
    )]
    fn decode(self, limits: ArtifactLimits) -> Result<NumericalOperator> {
        let count = self
            .dimension
            .checked_mul(self.dimension)
            .ok_or(ArtifactError::Budget)?;
        if self.entries.len() != count || !self.dimension.is_power_of_two() {
            return Err(invalid("matrix shape"));
        }
        let policy = MatrixPolicy {
            max_bytes: limits.matrix_bytes,
        };
        policy.check(self.dimension, 1).map_err(invalid)?;
        if self
            .entries
            .iter()
            .any(|(re, im)| !f64::from_bits(*re).is_finite() || !f64::from_bits(*im).is_finite())
        {
            return Err(invalid("nonfinite matrix"));
        }
        let matrix = faer::Mat::from_fn(self.dimension, self.dimension, |r, c| {
            let (re, im) = self.entries[r * self.dimension + c];
            num_complex::Complex64::new(f64::from_bits(re), f64::from_bits(im))
        });
        NumericalOperator::from_owned(matrix).map_err(invalid)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum PayloadData {
    Matrix {
        matrix: MatrixData,
        controls: Vec<bool>,
    },
    Channel {
        kraus: Vec<MatrixData>,
        tolerance: u64,
    },
}
impl PayloadData {
    fn encode(payload: &QuantumPayload) -> Result<Self> {
        Ok(match payload {
            QuantumPayload::Matrix {
                matrix,
                control_states,
            } => Self::Matrix {
                matrix: MatrixData::encode(matrix),
                controls: control_states.to_vec(),
            },
            QuantumPayload::Channel { kraus } => {
                let tolerance =
                    match crate::matrix::check_channel(kraus, 0.0, MatrixPolicy::default()) {
                        Ok(()) => 0.0,
                        Err(crate::Error::ChannelCompleteness { residual, .. }) => residual,
                        Err(error) => return Err(error.into()),
                    };
                Self::Channel {
                    kraus: kraus.iter().map(MatrixData::encode).collect(),
                    tolerance: tolerance.to_bits(),
                }
            }
        })
    }
    fn decode(self, limits: ArtifactLimits) -> Result<QuantumPayload> {
        Ok(match self {
            Self::Matrix { matrix, controls } => QuantumPayload::Matrix {
                matrix: matrix.decode(limits)?,
                control_states: controls.into(),
            },
            Self::Channel { kraus, tolerance } => {
                let kraus = kraus
                    .into_iter()
                    .map(|m| m.decode(limits))
                    .collect::<Result<Vec<_>>>()?;
                crate::matrix::check_channel(
                    &kraus,
                    f64::from_bits(tolerance),
                    MatrixPolicy {
                        max_bytes: limits.matrix_bytes,
                    },
                )?;
                QuantumPayload::Channel {
                    kraus: kraus.into(),
                }
            }
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleData {
    qubits: usize,
    tolerance: u64,
    adjoint: bool,
    operations: Vec<OracleOperation>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum OracleOperation {
    Gate {
        kind: quest_language::GateKind,
        parameters: Vec<u64>,
        targets: Vec<usize>,
        controls: Vec<(usize, bool)>,
    },
    Phase {
        radians: u64,
        controls: Vec<(usize, bool)>,
    },
    Matrix {
        matrix: MatrixData,
        targets: Vec<usize>,
        controls: Vec<(usize, bool)>,
    },
    Oracle {
        body: Box<OracleData>,
        targets: Vec<usize>,
        controls: Vec<(usize, bool)>,
    },
    Barrier {
        qubits: Vec<usize>,
    },
}
fn controls(values: &[Control]) -> Vec<(usize, bool)> {
    values
        .iter()
        .map(|c| (c.qubit().index(), c.state() == ControlState::One))
        .collect()
}
impl OracleData {
    fn encode(fragment: &OracleFragment) -> Result<Self> {
        let mut operations = Vec::new();
        for operation in fragment.operations() {
            operations.push(match operation {
                Operation::Gate {
                    gate,
                    targets,
                    controls: c,
                } => OracleOperation::Gate {
                    kind: gate.kind(),
                    parameters: gate.parameters().map(f64::to_bits).collect(),
                    targets: targets.iter().map(|q| q.index()).collect(),
                    controls: controls(c),
                },
                Operation::GlobalPhase {
                    radians,
                    controls: c,
                } => OracleOperation::Phase {
                    radians: radians.to_bits(),
                    controls: controls(c),
                },
                Operation::Numerical {
                    matrix,
                    targets,
                    controls: c,
                } => OracleOperation::Matrix {
                    matrix: MatrixData::encode(matrix),
                    targets: targets.iter().map(|q| q.index()).collect(),
                    controls: controls(c),
                },
                Operation::Oracle {
                    fragment,
                    targets,
                    controls: c,
                } => OracleOperation::Oracle {
                    body: Box::new(Self::encode(fragment)?),
                    targets: targets.iter().map(|q| q.index()).collect(),
                    controls: controls(c),
                },
                Operation::Barrier { qubits } => OracleOperation::Barrier {
                    qubits: qubits.iter().map(|q| q.index()).collect(),
                },
                _ => return Err(invalid("effectful oracle")),
            });
        }
        Ok(Self {
            qubits: fragment.num_qubits(),
            tolerance: fragment.admission_tolerance().to_bits(),
            adjoint: fragment.is_adjoint(),
            operations,
        })
    }
    #[expect(
        clippy::items_after_statements,
        reason = "Operand remapping helpers are scoped to artifact reconstruction"
    )]
    fn decode(self, limits: ArtifactLimits, depth: usize) -> Result<OracleFragment> {
        if depth > limits.compile.call_depth
            || self.qubits > limits.compile.qubits
            || self.operations.len() > limits.compile.nodes
        {
            return Err(ArtifactError::Budget);
        }
        let mut builder = QuantumRegionBuilder::new(self.qubits, 0)?;
        fn qs(b: &QuantumRegionBuilder, values: Vec<usize>) -> Result<Vec<crate::QubitId>> {
            values
                .into_iter()
                .map(|q| b.qubit(q).map_err(Into::into))
                .collect()
        }
        fn cs(b: &QuantumRegionBuilder, values: Vec<(usize, bool)>) -> Result<Vec<Control>> {
            values
                .into_iter()
                .map(|(q, positive)| {
                    Ok(Control::new(
                        b.qubit(q)?,
                        if positive {
                            ControlState::One
                        } else {
                            ControlState::Zero
                        },
                    ))
                })
                .collect()
        }
        for operation in self.operations {
            match operation {
                OracleOperation::Gate {
                    kind,
                    parameters,
                    targets,
                    controls,
                } => {
                    let gate = Gate::from_bound(&BoundGate::from_kind(
                        kind,
                        &parameters
                            .into_iter()
                            .map(f64::from_bits)
                            .collect::<Vec<_>>(),
                    )?)?;
                    builder.gate(gate, &qs(&builder, targets)?, &cs(&builder, controls)?)?;
                }
                OracleOperation::Phase { radians, controls } => {
                    builder.global_phase(
                        Angle::radians(f64::from_bits(radians)).map_err(invalid)?,
                        &cs(&builder, controls)?,
                    )?;
                }
                OracleOperation::Matrix {
                    matrix,
                    targets,
                    controls,
                } => {
                    builder.numerical(
                        matrix.decode(limits)?,
                        &qs(&builder, targets)?,
                        &cs(&builder, controls)?,
                    )?;
                }
                OracleOperation::Oracle {
                    body,
                    targets,
                    controls,
                } => {
                    builder.oracle(
                        &body.decode(limits, depth.saturating_add(1))?,
                        &qs(&builder, targets)?,
                        &cs(&builder, controls)?,
                    )?;
                }
                OracleOperation::Barrier { qubits } => {
                    builder.barrier(&qs(&builder, qubits)?)?;
                }
            }
        }
        let fragment = OracleFragment::from_program(
            builder.finish()?.bind(&[])?,
            f64::from_bits(self.tolerance),
            MatrixPolicy {
                max_bytes: limits.matrix_bytes,
            },
        )?;
        Ok(if self.adjoint {
            fragment.adjoint()
        } else {
            fragment
        })
    }
}
impl FiniteSourceEvidence {
    fn encode(region: &crate::BoundRegion) -> Result<Self> {
        let mut history = Vec::new();
        let mut seen = BTreeSet::new();
        let mut pending = region
            .instructions()
            .iter()
            .map(quest_language::quantum::Instruction::provenance)
            .collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            let (source, inputs) = match region.provenance().node(id)? {
                crate::ProvenanceNode::Source(id) => (Some((id.owner, id.index())), Vec::new()),
                crate::ProvenanceNode::Rewrite(inputs) => {
                    pending.extend_from_slice(inputs);
                    (None, inputs.iter().map(|id| format!("{id:?}")).collect())
                }
            };
            history.push(HistoryEvidence {
                id: format!("{id:?}"),
                source,
                inputs,
            });
        }
        Ok(Self {
            ideal_snapshot: format!("{:?}", region.source_snapshot_id()),
            bound_snapshot: format!("{:?}", region.snapshot_id()),
            bindings: region
                .binding_storage()
                .iter()
                .map(|(id, value)| (id.owner, id.index(), value.to_bits()))
                .collect(),
            occurrences: region
                .instructions()
                .iter()
                .map(|i| OccurrenceEvidence {
                    owner: i.id().owner,
                    index: i.id().index(),
                    provenance: format!("{:?}", i.provenance()),
                    source: i
                        .source()
                        .map(|s| (s.source().to_owned(), s.range().start, s.range().end)),
                    targets: i
                        .angle_targets()
                        .iter()
                        .map(|a| a.as_ref().map(AngleData::target))
                        .collect(),
                })
                .collect(),
            history,
            certificates: region
                .provenance()
                .evidence()
                .iter()
                .map(|record| serde_json::from_slice(record).map_err(invalid))
                .collect::<Result<_>>()?,
        })
    }
    fn validate(&self) -> Result<()> {
        if self
            .bindings
            .iter()
            .any(|(_, _, v)| !f64::from_bits(*v).is_finite())
        {
            return Err(invalid("nonfinite original binding"));
        }
        let nodes = self
            .history
            .iter()
            .map(|h| h.id.as_str())
            .collect::<BTreeSet<_>>();
        if nodes.len() != self.history.len() {
            return Err(invalid("duplicate provenance identity"));
        }
        let mut degrees = BTreeMap::new();
        let mut users: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut queue = std::collections::VecDeque::new();
        for h in &self.history {
            if h.source.is_some() && !h.inputs.is_empty() {
                return Err(invalid("source provenance inputs"));
            }
            degrees.insert(h.id.as_str(), h.inputs.len());
            if h.inputs.is_empty() {
                queue.push_back(h.id.as_str());
            }
            for input in &h.inputs {
                if !nodes.contains(input.as_str()) {
                    return Err(invalid("missing provenance input"));
                }
                users.entry(input.as_str()).or_default().push(h.id.as_str());
            }
        }
        let mut visited = 0usize;
        while let Some(id) = queue.pop_front() {
            visited = visited.saturating_add(1);
            if let Some(next) = users.get(id) {
                for user in next {
                    let degree = degrees
                        .get_mut(user)
                        .ok_or_else(|| invalid("provenance degree"))?;
                    *degree = degree
                        .checked_sub(1)
                        .ok_or_else(|| invalid("provenance edge"))?;
                    if *degree == 0 {
                        queue.push_back(user);
                    }
                }
            }
        }
        if visited != nodes.len() {
            return Err(invalid("cyclic provenance"));
        }
        for item in &self.occurrences {
            if !nodes.contains(item.provenance.as_str())
                || item.source.as_ref().is_some_and(|(_, a, b)| a > b)
            {
                return Err(invalid("invalid occurrence provenance"));
            }
            for angle in item.targets.iter().flatten() {
                angle.angle()?;
            }
        }
        Ok(())
    }
}
impl Program<Executable> {
    /// Export original source authority, independently of optimized executable SSA.
    /// Host captures and native payloads are rejected when QASM cannot represent them.
    /// # Errors
    /// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
    pub fn export_source(&self, limits: quest_qasm::ExportLimits) -> quest_qasm::Result<String> {
        quest_qasm::export_syntax(self.syntax(), limits)
    }
    /// Export a versioned compiled publication, retaining the executable graph and capture banks.
    /// # Errors
    /// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
    pub fn export_compiled(&self, limits: ArtifactLimits) -> Result<String> {
        let retained = self.state.verified.retained_bytes()?;
        let matrix_copies = self.quantum_payloads().values().try_fold(0usize, |n, p| {
            n.checked_add(match p {
                QuantumPayload::Matrix { matrix, .. } => matrix.bytes(),
                QuantumPayload::Channel { kraus } => kraus
                    .iter()
                    .try_fold(0usize, |n, m| n.checked_add(m.bytes()))
                    .ok_or(ArtifactError::Budget)?,
            })
            .ok_or(ArtifactError::Budget)
        })?;
        let oracle_copies = self.oracle_captures().values().try_fold(0usize, |n, o| {
            n.checked_add(oracle_export_storage(o, 0)?)
                .ok_or(ArtifactError::Budget)
        })?;
        let matrix_copies = matrix_copies
            .checked_add(oracle_copies)
            .ok_or(ArtifactError::Budget)?;
        let peak = retained
            .checked_mul(8)
            .and_then(|n| matrix_copies.checked_mul(4).and_then(|m| n.checked_add(m)))
            .ok_or(ArtifactError::Budget)?;
        if peak > limits.bytes.min(limits.compile.storage_bytes) {
            return Err(ArtifactError::Budget);
        }

        if self.state.verified.retained_bytes()? > limits.bytes {
            return Err(ArtifactError::Budget);
        }
        let mut finite_sources = self.state.verified.state.artifact_sources.clone();
        if let Some(origin) = self.finite_origin() {
            finite_sources.push(FiniteSourceEvidence::encode(origin)?);
        }
        for origin in self.embedded_regions() {
            finite_sources.push(FiniteSourceEvidence::encode(origin)?);
        }
        let exact = self
            .exact_captures()
            .iter()
            .map(|(id, angle)| {
                Ok((
                    *id,
                    AngleData::target(&angle.evaluate_target(&BTreeMap::new()).map_err(invalid)?.1),
                ))
            })
            .collect::<Result<_>>()?;
        let publication = Publication {
            source: self.syntax().clone(),
            program: self.ssa().program().clone(),
            publisher: self.ssa().snapshot(),
            previous: self.state.verified.state.artifact_publishers.clone(),
            captures: self.captures().to_vec(),
            exact,
            payloads: self
                .quantum_payloads()
                .iter()
                .map(|(id, p)| Ok((*id, PayloadData::encode(p)?)))
                .collect::<Result<_>>()?,
            oracles: self
                .oracle_captures()
                .iter()
                .map(|(id, p)| Ok((*id, OracleData::encode(p)?)))
                .collect::<Result<_>>()?,
            sources: self.sources().clone(),
            locations: self.locations().to_vec(),
            limits: limits.compile,
            finite_sources,
            evidence: self.state.verified.state.compilation_evidence.clone(),
            specializations: self.state.verified.state.specializations.clone(),
        };
        let proof_limits = ArtifactLimits {
            bytes: limits
                .bytes
                .min(limits.compile.storage_bytes)
                .checked_sub(peak)
                .ok_or(ArtifactError::Budget)?,
            ..limits
        };
        for source in &publication.finite_sources {
            for certificate in &source.certificates {
                certificate.validate(proof_limits)?;
            }
        }
        let payload = bounded_json(&publication, limits.bytes)?;
        if payload.len() > limits.bytes {
            return Err(ArtifactError::Budget);
        }
        let envelope = Envelope {
            version: VERSION,
            conventions: CONVENTIONS.into(),
            digest: digest(&payload),
            payload,
        };
        let encoded = bounded_json(&envelope, limits.bytes)?;
        if encoded.len() > limits.bytes {
            return Err(ArtifactError::Budget);
        }
        Ok(encoded)
    }
    /// Load and independently verify a compiled publication; source is never substituted for SSA.
    /// # Errors
    /// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
    pub fn load_compiled(encoded: &str, limits: ArtifactLimits) -> Result<Self> {
        if encoded.len() > limits.bytes {
            return Err(ArtifactError::Budget);
        }
        // A conservative fixed-layout DTO/Vec/String and simultaneous reconstruction bound,
        // checked before either deserializer can allocate. No decoded item is more compact than
        // its JSON token by this factor; matrix admission scratch is charged separately below.
        let decoded_bytes = encoded
            .len()
            .checked_mul(128)
            .ok_or(ArtifactError::Budget)?;
        if decoded_bytes > limits.bytes.min(limits.compile.storage_bytes) {
            return Err(ArtifactError::Budget);
        }
        let envelope: Envelope = serde_json::from_str(encoded).map_err(invalid)?;
        if envelope.version != VERSION || envelope.conventions != CONVENTIONS {
            return Err(ArtifactError::Incompatible);
        }
        if envelope.digest != digest(&envelope.payload) {
            return Err(ArtifactError::Integrity);
        }
        let p: Publication = serde_json::from_str(&envelope.payload).map_err(invalid)?;
        let compile = CompileLimits {
            nodes: p.limits.nodes.min(limits.compile.nodes),
            blocks: p.limits.blocks.min(limits.compile.blocks),
            slots: p.limits.slots.min(limits.compile.slots),
            storage_bytes: p.limits.storage_bytes.min(limits.compile.storage_bytes),
            qubits: p.limits.qubits.min(limits.compile.qubits),
            call_depth: p.limits.call_depth.min(limits.compile.call_depth),
        };
        let matrix_work = p.reconstruction_bytes()?;
        if matrix_work > limits.matrix_bytes
            || decoded_bytes
                .checked_add(matrix_work)
                .ok_or(ArtifactError::Budget)?
                > compile.storage_bytes.min(limits.bytes)
        {
            return Err(ArtifactError::Budget);
        }
        let proof_limits = ArtifactLimits {
            bytes: limits
                .bytes
                .min(compile.storage_bytes)
                .checked_sub(decoded_bytes)
                .and_then(|n| n.checked_sub(matrix_work))
                .ok_or(ArtifactError::Budget)?,
            compile,
            ..limits
        };
        for evidence in &p.evidence {
            evidence.validate(proof_limits)?;
        }
        for evidence in &p.finite_sources {
            evidence.validate()?;
            for certificate in &evidence.certificates {
                certificate.validate(proof_limits)?;
            }
        }
        for specialization in &p.specializations {
            specialization.validate()?;
        }
        let typed = TypedModule::from_compiled_parts(p.source, p.program, compile)
            .map_err(LanguageError::from)?;
        let exact = p
            .exact
            .into_iter()
            .map(|(i, a)| Ok((i, a.angle()?)))
            .collect::<Result<_>>()?;
        let payloads = p
            .payloads
            .into_iter()
            .map(|(i, p)| Ok((i, p.decode(limits)?)))
            .collect::<Result<_>>()?;
        let oracles = p
            .oracles
            .into_iter()
            .map(|(i, o)| Ok((i, o.decode(limits, 0)?)))
            .collect::<Result<_>>()?;
        let mut constructed =
            Program::<Constructed>::from_template(typed, p.captures, p.sources, p.locations)
                .with_angle_captures(exact)
                .with_quantum_payloads(payloads)
                .with_oracles(oracles)?;
        constructed.state.artifact_sources = p.finite_sources;
        constructed.state.compilation_evidence = p.evidence;
        constructed.state.specializations = p.specializations;
        constructed.state.artifact_publishers = p.previous;
        constructed.state.artifact_publishers.push(p.publisher);
        let verified = constructed.verify()?;
        if verified.retained_bytes()? > compile.storage_bytes {
            return Err(ArtifactError::Budget);
        }
        Ok(verified.lower()?.plan()?)
    }
    /// Historical binding/provenance data retained by a loaded artifact. These are data, not proofs.
    #[must_use]
    pub fn publication_history(&self) -> &[ssa::SnapshotId] {
        &self.state.verified.state.artifact_publishers
    }
    #[must_use]
    pub fn artifact_sources(&self) -> &[FiniteSourceEvidence] {
        &self.state.verified.state.artifact_sources
    }
}
fn bounded_json(value: &impl Serialize, limit: usize) -> Result<String> {
    struct Writer {
        bytes: Vec<u8>,
        limit: usize,
        exceeded: bool,
    }
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self
                .bytes
                .len()
                .checked_add(bytes.len())
                .is_none_or(|n| n > self.limit)
            {
                self.exceeded = true;
                return Err(std::io::Error::other("artifact byte limit"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer {
        bytes: Vec::new(),
        limit,
        exceeded: false,
    };
    if let Err(error) = serde_json::to_writer(&mut writer, value) {
        return Err(if writer.exceeded {
            ArtifactError::Budget
        } else {
            invalid(error)
        });
    }
    String::from_utf8(writer.bytes).map_err(invalid)
}

/// Scope of persisted local evidence; historical records never certify the current executable.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub enum EvidenceScope {
    HistoricalLocalCertificate,
}
/// Historical independently checked rotation certificate data, bound to source/output publications.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompilationEvidence {
    pub version: u32,
    /// Explicitly historical diagnostics; these records grant no certified-executable or whole-program error capability.
    pub scope: EvidenceScope,
    pub occurrence: String,
    pub interface: Vec<String>,
    pub outputs: Vec<String>,
    pub algorithm: String,
    pub input: String,
    pub output: String,
    pub seed: u64,
    pub target: serde_json::Value,
    pub candidate: serde_json::Value,
    pub epsilon_bits: u64,
    pub limits: serde_json::Value,
}
impl CompilationEvidence {
    fn validate(&self, limits: ArtifactLimits) -> Result<()> {
        if self.version != 1 {
            return Err(ArtifactError::Incompatible);
        }
        #[cfg(any(feature = "workers", feature = "synthesis"))]
        {
            let target: quest_math::Target =
                serde_json::from_value(self.target.clone()).map_err(invalid)?;
            let candidate: quest_math::Sequence =
                serde_json::from_value(self.candidate.clone()).map_err(invalid)?;
            let mut policy: quest_math::Limits =
                serde_json::from_value(self.limits.clone()).map_err(invalid)?;
            policy.bytes = policy.bytes.min(limits.bytes);
            policy.gates = policy.gates.min(limits.compile.nodes);
            policy.taylor_terms = policy.taylor_terms.min(4096);
            policy.qubits = policy.qubits.min(1);
            policy.coefficient_bits = policy.coefficient_bits.min(16384);
            policy.precision_bits = policy.precision_bits.min(4096);
            quest_math::certify_rotation(&candidate, &target, self.epsilon_bits, policy)
                .map_err(invalid)?;
            Ok(())
        }
        #[cfg(not(any(feature = "workers", feature = "synthesis")))]
        {
            let _ = limits;
            Err(ArtifactError::Incompatible)
        }
    }
}
impl Program<Executable> {
    #[must_use]
    pub fn compilation_evidence(&self) -> &[CompilationEvidence] {
        &self.state.verified.state.compilation_evidence
    }
}

impl Publication {
    fn reconstruction_bytes(&self) -> Result<usize> {
        fn matrix(m: &MatrixData) -> Result<usize> {
            m.dimension
                .checked_mul(m.dimension)
                .and_then(|n| n.checked_mul(size_of::<num_complex::Complex64>()))
                .and_then(|n| n.checked_mul(4))
                .ok_or(ArtifactError::Budget)
        }
        fn oracle(o: &OracleData, depth: usize) -> Result<usize> {
            if depth > 64 {
                return Err(ArtifactError::Budget);
            }
            let dimension = 1usize
                .checked_shl(u32::try_from(o.qubits).map_err(|_| ArtifactError::Budget)?)
                .ok_or(ArtifactError::Budget)?;
            let mut bytes = dimension
                .checked_mul(dimension)
                .and_then(|n| n.checked_mul(64))
                .ok_or(ArtifactError::Budget)?;
            for op in &o.operations {
                let extra = match op {
                    OracleOperation::Matrix { matrix: m, .. } => matrix(m)?,
                    OracleOperation::Oracle { body, .. } => oracle(body, depth.saturating_add(1))?,
                    _ => 0,
                };
                bytes = bytes.checked_add(extra).ok_or(ArtifactError::Budget)?;
            }
            Ok(bytes)
        }
        let mut bytes = 0usize;
        for payload in self.payloads.values() {
            match payload {
                PayloadData::Matrix { matrix: m, .. } => {
                    bytes = bytes.checked_add(matrix(m)?).ok_or(ArtifactError::Budget)?;
                }
                PayloadData::Channel { kraus, .. } => {
                    for m in kraus {
                        bytes = bytes.checked_add(matrix(m)?).ok_or(ArtifactError::Budget)?;
                    }
                }
            }
        }
        for body in self.oracles.values() {
            bytes = bytes
                .checked_add(oracle(body, 0)?)
                .ok_or(ArtifactError::Budget)?;
        }
        Ok(bytes)
    }
}
/// Conservative retained evidence storage without allocating a serialized copy.
pub(super) fn evidence_storage(value: &impl Serialize) -> Result<usize> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("artifact storage overflow"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value).map_err(invalid)?;
    counter.0.checked_mul(128).ok_or(ArtifactError::Budget)
}
fn oracle_export_storage(fragment: &OracleFragment, depth: usize) -> Result<usize> {
    if depth > 64 {
        return Err(ArtifactError::Budget);
    }
    let mut bytes = fragment
        .operations()
        .len()
        .checked_mul(512)
        .ok_or(ArtifactError::Budget)?;
    for op in fragment.operations() {
        let added = match op {
            Operation::Numerical { matrix, .. } => matrix.bytes(),
            Operation::Oracle { fragment, .. } => {
                oracle_export_storage(fragment, depth.saturating_add(1))?
            }
            _ => 0,
        };
        bytes = bytes.checked_add(added).ok_or(ArtifactError::Budget)?;
    }
    Ok(bytes)
}
