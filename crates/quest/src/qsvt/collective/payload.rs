//! Exact semantic values and deterministic per-program native cache topology.
use crate::{Complex64, Error, Result, environment::RuntimeResources};
use quest_qsvt::{LogicalSpace, Projection, ProjectionSpace, ProjectorKind, ValidatedTransform};

pub(super) fn encode(
    transform: &ValidatedTransform,
    vectors: Option<(&[Complex64], &[Complex64])>,
    resources: &RuntimeResources,
    limit: usize,
    padding: usize,
) -> Result<Vec<u8>> {
    let mut out = Encoder {
        bytes: Vec::new(),
        limit,
    };
    out.word(1)?;
    out.word(padding)?;
    out.word(match transform.route() {
        quest_qsvt::Route::Standard => 0,
        quest_qsvt::Route::DirectHermitian => 1,
        quest_qsvt::Route::HermitianizedFull => 2,
        quest_qsvt::Route::HermitianizedEven => 3,
        quest_qsvt::Route::HermitianizedOdd => 4,
        quest_qsvt::Route::MultiplicationEven => 5,
        quest_qsvt::Route::MultiplicationOdd => 6,
    })?;
    out.blob(transform.convention().as_bytes())?;
    out.word(transform.degree())?;
    out.real(transform.normalization().get())?;
    out.word(transform.operands().num_qubits())?;
    out.word(transform.operands().num_idle_high_qubits())?;
    out.words(transform.operands().source())?;
    out.word(transform.operands().response())?;
    out.word(usize::from(transform.operands().auxiliary().is_some()))?;
    if let Some(q) = transform.operands().auxiliary() {
        out.word(q)?;
    }
    let queries = transform.query_counts();
    for n in [
        queries.semantic,
        queries.source_forward,
        queries.source_adjoint,
        queries.retained_oracle_calls,
    ] {
        out.word(n)?;
    }
    let evidence = transform.evidence();
    for value in [
        evidence.whole_oracle_hermiticity_residual,
        evidence.projector_agreement_residual,
    ] {
        out.word(usize::from(value.is_some()))?;
        if let Some(value) = value {
            out.real(value)?;
        }
    }
    out.real(evidence.phase_conversion_roundoff_estimate)?;
    out.projection(transform.input())?;
    out.word(usize::from(transform.bridge().is_some()))?;
    if let Some(p) = transform.bridge() {
        out.projection(p)?;
    }
    out.projection(transform.output())?;
    out.encoding(transform.encoding())?;
    out.plan(&super::super::plan(transform.main())?)?;
    out.word(usize::from(transform.continuation().is_some()))?;
    if let Some(p) = transform.continuation() {
        out.plan(&super::super::plan(p)?)?;
    }
    out.word(usize::from(vectors.is_some()))?;
    if let Some((input, reference)) = vectors {
        for values in <[&[Complex64]; 2]>::from((input, reference)) {
            out.word(values.len())?;
            for &value in values {
                out.complex(value)?;
            }
        }
        // Controlled wrappers are distinct prepared caches. Their exact sharing
        // schedules are compared independently from the original main program.
        out.plan(&super::super::hadamard::controlled_plan_padded(
            resources,
            transform.main(),
            transform.operands().num_qubits(),
            padding,
        )?)?;
        if let Some(p) = transform.continuation() {
            out.plan(&super::super::hadamard::controlled_plan_padded(
                resources,
                p,
                transform.operands().num_qubits(),
                padding,
            )?)?;
        }
    }
    Ok(out.bytes)
}
struct Encoder {
    bytes: Vec<u8>,
    limit: usize,
}
impl Encoder {
    fn encoding(&mut self, encoding: &quest_qsvt::ProjectedEncoding) -> Result<()> {
        // The source encoding is retained even when a degree-zero program does not
        // invoke it. Include its source body, interface representations and premise.
        self.space(encoding.left())?;
        self.space(encoding.right())?;
        self.real(encoding.normalization().get())?;
        match encoding.unitarity_evidence() {
            quest_qsvt::OracleUnitarityEvidence::Measured { residual } => {
                self.word(0)?;
                self.real(*residual)?;
            }
            quest_qsvt::OracleUnitarityEvidence::Assumed(premise) => {
                self.word(1)?;
                self.blob(premise.description().as_bytes())?;
            }
        }
        let mut source = quest_circuit::QuantumRegionBuilder::new(encoding.num_qubits(), 0)?;
        let targets = (0..encoding.num_qubits())
            .map(|q| source.qubit(q))
            .collect::<quest_circuit::Result<Vec<_>>>()?;
        source.oracle(encoding.oracle(), &targets, &[])?;
        self.plan(&source.finish()?.bind(&[])?.plan()?)?;
        Ok(())
    }
    fn raw(&mut self, bytes: &[u8]) -> Result<()> {
        let requested = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(Error::Overflow)?;
        if requested > self.limit {
            return Err(Error::Budget {
                requested,
                available: self.limit,
            });
        }
        self.bytes
            .try_reserve_exact(bytes.len())
            .map_err(|_| Error::Allocation)?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn word(&mut self, n: usize) -> Result<()> {
        self.raw(&u64::try_from(n).map_err(|_| Error::Overflow)?.to_le_bytes())
    }
    fn real(&mut self, value: f64) -> Result<()> {
        self.raw(&value.to_bits().to_le_bytes())
    }
    fn complex(&mut self, value: Complex64) -> Result<()> {
        self.real(value.re)?;
        self.real(value.im)
    }
    fn blob(&mut self, bytes: &[u8]) -> Result<()> {
        self.word(bytes.len())?;
        self.raw(bytes)
    }
    fn words(&mut self, values: &[usize]) -> Result<()> {
        self.word(values.len())?;
        for &v in values {
            self.word(v)?;
        }
        Ok(())
    }
    fn matrix(&mut self, matrix: faer::MatRef<'_, Complex64>) -> Result<()> {
        self.word(matrix.nrows())?;
        self.word(matrix.ncols())?;
        for row in 0..matrix.nrows() {
            for col in 0..matrix.ncols() {
                self.complex(matrix[(row, col)])?;
            }
        }
        Ok(())
    }
    fn plan(&mut self, plan: &quest_circuit::RegionPlan) -> Result<()> {
        // Keep both the temporary plan bytes and retained transform bytes within
        // the same limit, including the append copy while the temporary lives.
        let remaining = self
            .limit
            .saturating_sub(self.bytes.len())
            .saturating_sub(8)
            .checked_div(2)
            .ok_or(Error::Overflow)?;
        let bytes = crate::collective_payload::encode(plan, remaining)?;
        self.blob(&bytes)
    }
    fn space<S>(&mut self, space: &LogicalSpace<S>) -> Result<()> {
        self.real(space.construction_residual())?;
        self.word(space.physical_dimension())?;
        self.word(space.logical_dimension())?;
        // Representation controls native allocation even when projectors agree.
        match space.kind() {
            ProjectorKind::Coordinates(indices) => {
                self.word(0)?;
                self.words(indices)?;
            }
            ProjectorKind::Isometry => self.word(1)?,
            ProjectorKind::Dense(matrix) => {
                self.word(2)?;
                self.matrix(matrix.as_ref().as_ref())?;
            }
        }
        self.word(usize::from(space.dense_isometry().is_some()))?;
        if let Some(matrix) = space.dense_isometry() {
            self.matrix(matrix)?;
        }
        Ok(())
    }
    fn projection(&mut self, p: &Projection) -> Result<()> {
        self.words(p.targets())?;
        self.word(p.controls().len())?;
        for c in p.controls() {
            self.word(c.qubit)?;
            self.word(usize::from(c.value))?;
        }
        match p.space() {
            ProjectionSpace::Left(space) => {
                self.word(0)?;
                self.space(space)?;
            }
            ProjectionSpace::Right(space) => {
                self.word(1)?;
                self.space(space)?;
            }
            ProjectionSpace::Joint { left, right } => {
                self.word(2)?;
                self.space(left)?;
                self.space(right)?;
            }
        }
        Ok(())
    }
}
