#![doc = include_str!("../docs/analysis.md")]
use crate::{Route, ValidatedTransform};
use quest_numerics::Interval;
mod diagnostics;
pub use diagnostics::{
    DiagnosticBuilder, DiagnosticMethod, EmpiricalEvidence, MissingReference, Reference,
    ReferenceParity, SuppliedReference,
};
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Model(#[from] crate::Error),
    #[error(transparent)]
    Numerics(#[from] quest_numerics::Error),
    #[error("invalid analytical input: {0}")]
    Input(&'static str),
    #[error("analysis resource limit: {0}")]
    Budget(&'static str),
    #[error("numerical singular value decomposition did not converge")]
    Svd,
}
/// A named caller assertion, not a verifier-produced proof.
#[derive(Debug, Clone)]
pub struct Assumption(String);
impl Assumption {
    /// # Errors
    /// Requires a nonempty explanation of at most 4096 bytes.
    pub fn stated(description: impl Into<String>) -> Result<Self> {
        let description = description.into();
        if description.trim().is_empty() || description.len() > 4096 {
            return Err(Error::Input("assumption description"));
        }
        Ok(Self(description))
    }
    #[must_use]
    pub fn description(&self) -> &str {
        &self.0
    }
}
pub struct Missing;
pub struct Supplied<T>(T);
/// Required normalization, absolute error and mathematical contract are typed states.
/// ```compile_fail
/// quest_qsvt::analysis::BlockEncodingBound::builder().build();
/// ```
pub struct EncodingBoundBuilder<A = Missing, E = Missing, P = Missing> {
    alpha: A,
    error: E,
    premise: P,
    oracle: Option<(Interval, Assumption)>,
}
/// Conditional guarantee ||A - alpha L† U R|| <= epsilon with both A/alpha
/// and L†UR contractions.
///
/// The normalization is an enclosure of the exact
/// composed expression, never an implicitly exact rounded product.
#[derive(Debug, Clone)]
pub struct BlockEncodingBound {
    alpha: Interval,
    normalized_error: Interval,
    oracle: Option<Interval>,
    assumptions: Vec<Assumption>,
}
impl BlockEncodingBound {
    #[must_use]
    pub const fn builder() -> EncodingBoundBuilder {
        EncodingBoundBuilder {
            alpha: Missing,
            error: Missing,
            premise: Missing,
            oracle: None,
        }
    }
    #[must_use]
    pub const fn normalization(&self) -> Interval {
        self.alpha
    }
    #[must_use]
    pub const fn normalized_error(&self) -> Interval {
        self.normalized_error
    }
    #[must_use]
    pub const fn full_oracle_error(&self) -> Option<Interval> {
        self.oracle
    }
    #[must_use]
    pub fn assumptions(&self) -> &[Assumption] {
        &self.assumptions
    }
    /// # Errors
    /// Rejects an unbounded interval result.
    pub fn absolute_error(&self) -> Result<Interval> {
        Ok(self.alpha.checked_mul(self.normalized_error)?)
    }
    /// Under both contraction premises, normalized product error is eL + eR.
    /// # Errors
    /// Rejects nonfinite composed arithmetic or provenance allocation failure.
    pub fn product(self, rhs: Self) -> Result<Self> {
        let alpha = self.alpha.checked_mul(rhs.alpha)?;
        let normalized_error = self.normalized_error.checked_add(rhs.normalized_error)?;
        let oracle = self
            .oracle
            .zip(rhs.oracle)
            .map(|(a, b)| a.checked_add(b))
            .transpose()?;
        let mut assumptions = self.assumptions;
        assumptions
            .try_reserve(rhs.assumptions.len())
            .map_err(|_| Error::Budget("assumption composition"))?;
        assumptions.extend(rhs.assumptions);
        Ok(Self {
            alpha,
            normalized_error,
            oracle,
            assumptions,
        })
    }
    /// The off-diagonal Hermitian lift preserves normalization and operator-norm error.
    #[must_use]
    pub const fn hermitianize(self) -> Self {
        self
    }
}
impl<E, P> EncodingBoundBuilder<Missing, E, P> {
    /// # Errors
    /// Requires a finite strictly positive normalization, interpreted exactly as supplied.
    pub fn normalization(
        self,
        alpha: f64,
    ) -> Result<EncodingBoundBuilder<Supplied<Interval>, E, P>> {
        if !alpha.is_finite() || alpha <= 0.0 {
            return Err(Error::Input("positive finite normalization"));
        }
        Ok(EncodingBoundBuilder {
            alpha: Supplied(Interval::point(alpha)?),
            error: self.error,
            premise: self.premise,
            oracle: self.oracle,
        })
    }
}
impl<A, P> EncodingBoundBuilder<A, Missing, P> {
    /// # Errors
    /// Requires a finite nonnegative absolute projected-block error.
    pub fn absolute_error(
        self,
        error: f64,
    ) -> Result<EncodingBoundBuilder<A, Supplied<Interval>, P>> {
        Ok(EncodingBoundBuilder {
            alpha: self.alpha,
            error: Supplied(nonnegative(error)?),
            premise: self.premise,
            oracle: self.oracle,
        })
    }
}
impl<A, E> EncodingBoundBuilder<A, E, Missing> {
    /// Assert the stated normalization/error semantics and contraction of both
    /// the exact normalized reference and the encoded block. No finite matrix
    /// residual establishes these mathematical assertions automatically.
    #[must_use]
    pub fn assume_contract(
        self,
        premise: Assumption,
    ) -> EncodingBoundBuilder<A, E, Supplied<Assumption>> {
        EncodingBoundBuilder {
            alpha: self.alpha,
            error: self.error,
            premise: Supplied(premise),
            oracle: self.oracle,
        }
    }
}
impl<A, E, P> EncodingBoundBuilder<A, E, P> {
    /// Assert a separate full-unitary operator-norm error. This never follows
    /// merely from the projected-block error.
    /// # Errors
    /// Requires a finite nonnegative error.
    pub fn full_oracle_error(mut self, error: f64, premise: Assumption) -> Result<Self> {
        self.oracle = Some((nonnegative(error)?, premise));
        Ok(self)
    }
}
impl EncodingBoundBuilder<Supplied<Interval>, Supplied<Interval>, Supplied<Assumption>> {
    /// # Errors
    /// Rejects an unbounded normalized error.
    pub fn build(self) -> Result<BlockEncodingBound> {
        let normalized_error = self.error.0.checked_div(self.alpha.0)?;
        let mut assumptions = vec![self.premise.0];
        let oracle = self.oracle.map(|(bound, premise)| {
            assumptions.push(premise);
            bound
        });
        Ok(BlockEncodingBound {
            alpha: self.alpha.0,
            normalized_error,
            oracle,
            assumptions,
        })
    }
}
/// Theorem premises refer to one exact, immutable transform object.
///
/// This includes its actual frozen phase conversions and extraction convention.
/// No unrelated phase sequence or upstream synthesis certificate can be attached.
/// All three mathematical premise groups must be supplied:
/// ```compile_fail
/// # fn f(t: &quest_qsvt::ValidatedTransform) {
/// quest_qsvt::analysis::StandardPremises::for_transform(t).unwrap().build();
/// # }
/// ```
pub struct StandardPremises<'t> {
    transform: &'t ValidatedTransform,
    assumptions: Vec<Assumption>,
    phase_error: Option<Interval>,
}
pub struct PremiseBuilder<'t, S = Missing, L = Missing, C = Missing> {
    transform: &'t ValidatedTransform,
    subspaces: S,
    linkage: L,
    completion: C,
}
impl<'t> StandardPremises<'t> {
    /// # Errors
    /// Generalized routes cannot acquire the standard theorem premise type.
    pub fn for_transform(transform: &'t ValidatedTransform) -> Result<PremiseBuilder<'t>> {
        if transform.route() != Route::Standard {
            return Err(Error::Input("standard theorem requires the standard route"));
        }
        Ok(PremiseBuilder {
            transform,
            subspaces: Missing,
            linkage: Missing,
            completion: Missing,
        })
    }
    #[must_use]
    pub fn assumptions(&self) -> &[Assumption] {
        &self.assumptions
    }
}
impl<'t, L, C> PremiseBuilder<'t, Missing, L, C> {
    /// Assert exact unitary/projected-isometry semantics of the source and the
    /// standard route's extraction, including applicability of the supplied source
    /// encoding bound to this transform. Numerical residuals are insufficient.
    #[must_use]
    pub fn assume_projected_unitary_subspaces(
        self,
        value: Assumption,
    ) -> PremiseBuilder<'t, Supplied<Assumption>, L, C> {
        PremiseBuilder {
            transform: self.transform,
            subspaces: Supplied(value),
            linkage: self.linkage,
            completion: self.completion,
        }
    }
}
/// A response premise produced by explicit assumption or transform-bound certification.
/// Its fields are private so evidence cannot be rebound by callers.
pub struct PhaseResponsePremise {
    assumption: Option<Assumption>,
    error: Option<Interval>,
}
impl<'t, S, C> PremiseBuilder<'t, S, Missing, C> {
    /// Establish actual-phase linkage using only the certificate retained by this transform.
    /// The certified uniform error is added automatically to the final bound.
    /// # Errors
    /// Imported or uncertified sequences cannot supply this evidence.
    #[cfg(feature = "certification")]
    pub fn certified_actual_phase_response(
        self,
    ) -> Result<PremiseBuilder<'t, S, Supplied<PhaseResponsePremise>, C>> {
        let certificate = self.transform.projector_certificate().ok_or(Error::Input(
            "transform has no certificate for its converted phase payload",
        ))?;
        let error = Interval::new(0.0, certificate.response_bound().upper_f64())?;
        Ok(PremiseBuilder {
            transform: self.transform,
            subspaces: self.subspaces,
            linkage: Supplied(PhaseResponsePremise {
                assumption: None,
                error: Some(error),
            }),
            completion: self.completion,
        })
    }

    /// Assert that the actual frozen phases, including convention conversion,
    /// extract the claimed degree-matching polynomial response for this transform.
    /// A certificate for phases before a rounded conversion does not establish it.
    #[must_use]
    pub fn assume_actual_phase_response(
        self,
        value: Assumption,
    ) -> PremiseBuilder<'t, S, Supplied<PhaseResponsePremise>, C> {
        PremiseBuilder {
            transform: self.transform,
            subspaces: self.subspaces,
            linkage: Supplied(PhaseResponsePremise {
                assumption: Some(value),
                error: None,
            }),
            completion: self.completion,
        }
    }
}
impl<'t, S, L> PremiseBuilder<'t, S, L, Missing> {
    /// Assert the required degree parity, contractive polynomial on [-1,1], and
    /// QSP completion hypotheses of Gilyén et al.'s standard robustness lemma.
    #[must_use]
    pub fn assume_parity_and_completion(
        self,
        value: Assumption,
    ) -> PremiseBuilder<'t, S, L, Supplied<Assumption>> {
        PremiseBuilder {
            transform: self.transform,
            subspaces: self.subspaces,
            linkage: self.linkage,
            completion: Supplied(value),
        }
    }
}
impl<'t>
    PremiseBuilder<'t, Supplied<Assumption>, Supplied<PhaseResponsePremise>, Supplied<Assumption>>
{
    #[must_use]
    pub fn build(self) -> StandardPremises<'t> {
        let mut assumptions = vec![self.subspaces.0, self.completion.0];
        if let Some(assumption) = self.linkage.0.assumption {
            assumptions.push(assumption);
        }
        StandardPremises {
            transform: self.transform,
            assumptions,
            phase_error: self.linkage.0.error,
        }
    }
}
/// Finite caller-supplied absolute contributions to a final operator-norm bound.
#[derive(Debug, Clone)]
pub struct ErrorBudget {
    approximation: Interval,
    synthesis: Interval,
    execution: Interval,
    premise: Assumption,
}
impl ErrorBudget {
    #[must_use]
    pub const fn approximation(&self) -> Interval {
        self.approximation
    }
    #[must_use]
    pub const fn synthesis(&self) -> Interval {
        self.synthesis
    }
    #[must_use]
    pub const fn execution(&self) -> Interval {
        self.execution
    }

    /// # Errors
    /// Rejects negative or nonfinite terms. Their claimed meaning remains an assumption.
    pub fn stated(
        approximation: f64,
        synthesis: f64,
        execution: f64,
        premise: Assumption,
    ) -> Result<Self> {
        Ok(Self {
            approximation: nonnegative(approximation)?,
            synthesis: nonnegative(synthesis)?,
            execution: nonnegative(execution)?,
            premise,
        })
    }
    /// # Errors
    /// Rejects overflowing arithmetic.
    pub fn total(&self) -> Result<Interval> {
        Ok(self
            .approximation
            .checked_add(self.synthesis)?
            .checked_add(self.execution)?)
    }
    #[must_use]
    pub const fn assumption(&self) -> &Assumption {
        &self.premise
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    ConditionalOnExplicitPremises,
    Uncertified,
}
/// Finite calculations recorded during construction, with no exact privileges.
/// ```compile_fail
/// # fn f(t: &quest_qsvt::ValidatedTransform) {
/// t.analysis().standard_premises(t.evidence());
/// # }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct NumericalObservations {
    unitarity: Option<f64>,
    left_isometry: f64,
    right_isometry: f64,
    phase_conversion: f64,
}
impl NumericalObservations {
    #[must_use]
    pub const fn unitarity_residual(self) -> Option<f64> {
        self.unitarity
    }
    #[must_use]
    pub const fn left_isometry_residual(self) -> f64 {
        self.left_isometry
    }
    #[must_use]
    pub const fn right_isometry_residual(self) -> f64 {
        self.right_isometry
    }
    #[must_use]
    pub const fn phase_conversion_estimate(self) -> f64 {
        self.phase_conversion
    }
}
/// A borrowed analytical report is never a certificate of the evaluated circuit.
pub struct TransformReport<'t> {
    transform: &'t ValidatedTransform,
    status: Status,
    robustness: Option<Interval>,
    telescoping: Option<Interval>,
    total: Option<Interval>,
    encoding: BlockEncodingBound,
    premises: Vec<Assumption>,
    budget: Option<ErrorBudget>,
}
impl TransformReport<'_> {
    #[must_use]
    pub const fn observations(&self) -> NumericalObservations {
        let encoding = self.transform.encoding();
        NumericalObservations {
            unitarity: match encoding.unitarity_evidence() {
                crate::OracleUnitarityEvidence::Measured { residual } => Some(*residual),
                crate::OracleUnitarityEvidence::Assumed(_) => None,
            },
            left_isometry: encoding.left().construction_residual(),
            right_isometry: encoding.right().construction_residual(),
            phase_conversion: self.transform.evidence().phase_conversion_roundoff_estimate,
        }
    }

    #[must_use]
    pub const fn transform(&self) -> &ValidatedTransform {
        self.transform
    }
    #[must_use]
    pub const fn status(&self) -> Status {
        self.status
    }
    #[must_use]
    pub const fn robustness(&self) -> Option<Interval> {
        self.robustness
    }
    #[must_use]
    pub const fn oracle_telescoping(&self) -> Option<Interval> {
        self.telescoping
    }
    #[must_use]
    pub const fn total(&self) -> Option<Interval> {
        self.total
    }
    #[must_use]
    pub const fn encoding(&self) -> &BlockEncodingBound {
        &self.encoding
    }
    #[must_use]
    pub fn theorem_assumptions(&self) -> &[Assumption] {
        &self.premises
    }
    #[must_use]
    pub const fn budget(&self) -> Option<&ErrorBudget> {
        self.budget.as_ref()
    }
}
/// Required bound and standard-premise states prevent accidental theorem reports.
/// ```compile_fail
/// # fn f(t: &quest_qsvt::ValidatedTransform) {
/// t.analysis().build();
/// # }
/// ```
pub struct AnalysisBuilder<'t, E = Missing, P = Missing> {
    transform: &'t ValidatedTransform,
    encoding: E,
    premises: P,
    budget: Option<ErrorBudget>,
}
impl ValidatedTransform {
    /// Start a report requiring an explicit source bound and, for standard
    /// conditional certification, premises linked to this exact transform.
    #[must_use]
    pub const fn analysis(&self) -> AnalysisBuilder<'_> {
        AnalysisBuilder {
            transform: self,
            encoding: Missing,
            premises: Missing,
            budget: None,
        }
    }
}
impl<'t, P> AnalysisBuilder<'t, Missing, P> {
    /// # Errors
    /// Requires an exactly matching normalization, rejecting a merely enclosed rounded product.
    pub fn encoding_bound(
        self,
        bound: BlockEncodingBound,
    ) -> Result<AnalysisBuilder<'t, Supplied<BlockEncodingBound>, P>> {
        let alpha = self.transform.normalization().get();
        if bound.alpha.lower().to_bits() != alpha.to_bits()
            || bound.alpha.upper().to_bits() != alpha.to_bits()
        {
            return Err(Error::Input(
                "normalization is not the exact transform normalization",
            ));
        }
        Ok(AnalysisBuilder {
            transform: self.transform,
            encoding: Supplied(bound),
            premises: self.premises,
            budget: self.budget,
        })
    }
}
impl<'t, E> AnalysisBuilder<'t, E, Missing> {
    /// # Errors
    /// Rejects premises borrowed from another transform, including a clone with changed phases.
    pub fn standard_premises(
        self,
        premises: StandardPremises<'t>,
    ) -> Result<AnalysisBuilder<'t, E, StandardPremises<'t>>> {
        if !std::ptr::eq(self.transform, premises.transform) {
            return Err(Error::Input(
                "theorem premises belong to another frozen transform",
            ));
        }
        Ok(AnalysisBuilder {
            transform: self.transform,
            encoding: self.encoding,
            premises,
            budget: self.budget,
        })
    }
}
impl<E, P> AnalysisBuilder<'_, E, P> {
    #[must_use]
    pub fn budget(mut self, budget: ErrorBudget) -> Self {
        self.budget = Some(budget);
        self
    }
}
impl<'t> AnalysisBuilder<'t, Supplied<BlockEncodingBound>, Missing> {
    /// Keep generalized or missing-premise reports explicitly uncertified.
    /// # Errors
    /// Rejects overflowing query telescoping arithmetic.
    pub fn uncertified(self) -> Result<TransformReport<'t>> {
        let telescoping = telescope(self.transform, &self.encoding.0)?;
        Ok(TransformReport {
            transform: self.transform,
            status: Status::Uncertified,
            robustness: None,
            telescoping,
            total: None,
            encoding: self.encoding.0,
            premises: Vec::new(),
            budget: self.budget,
        })
    }
}
impl<'t> AnalysisBuilder<'t, Supplied<BlockEncodingBound>, StandardPremises<'t>> {
    /// Enclose 4*d*sqrt(epsilon/alpha), physical-source-query telescoping, and
    /// explicitly supplied budgets. The result is conditional on all named premises.
    /// # Errors
    /// Rejects unbounded arithmetic or a degree that cannot be represented exactly.
    pub fn build(self) -> Result<TransformReport<'t>> {
        let robustness = Interval::point(4.0)?
            .checked_mul(integer(self.transform.degree())?)?
            .checked_mul(self.encoding.0.normalized_error.sqrt()?)?;
        let telescoping = telescope(self.transform, &self.encoding.0)?;
        let mut total = robustness;
        if let Some(phase_error) = self.premises.phase_error {
            total = total.checked_add(phase_error)?;
        }
        if let Some(term) = telescoping {
            total = total.checked_add(term)?;
        }
        if let Some(budget) = &self.budget {
            total = total.checked_add(budget.total()?)?;
        }
        Ok(TransformReport {
            transform: self.transform,
            status: Status::ConditionalOnExplicitPremises,
            robustness: Some(robustness),
            telescoping,
            total: Some(total),
            encoding: self.encoding.0,
            premises: self.premises.assumptions,
            budget: self.budget,
        })
    }
}
fn telescope(
    transform: &ValidatedTransform,
    bound: &BlockEncodingBound,
) -> Result<Option<Interval>> {
    let queries = transform.query_counts();
    let count = queries
        .source_forward
        .checked_add(queries.source_adjoint)
        .ok_or(Error::Input("source query count overflow"))?;
    bound
        .oracle
        .map(|error| Ok(integer(count)?.checked_mul(error)?))
        .transpose()
}
fn nonnegative(value: f64) -> Result<Interval> {
    if value < 0.0 || !value.is_finite() {
        return Err(Error::Input("nonnegative finite bound"));
    }
    Ok(Interval::point(value)?)
}
fn integer(value: usize) -> Result<Interval> {
    let value = u32::try_from(value)
        .map_err(|_| Error::Input("exact degree/query arithmetic supports at most u32::MAX"))?;
    Ok(Interval::point(f64::from(value))?)
}
