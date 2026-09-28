use crate::{
    Complex64, Error, Missing, Normalization, OperandLayout, ProjectedEncoding, Projection, Result,
    materialize_program, matrix,
};
use faer::Mat;
use quest_circuit::BoundProgram;
use quest_qsp::{
    CanonicalWxImag, ControlSequence, ConvertedProjectorPhases, PhaseConvention, PhaseSequence,
    WxLaurent, WxSymmetric,
};

/// Explicit construction and logical-extraction route.
///
/// See [`TransformBuilder`] for the corresponding consuming builder methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Typed standard QSP phases; output side follows degree parity.
    Standard,
    /// Generalized polynomial of a whole-oracle Hermitian encoding.
    DirectHermitian,
    /// Full Hermitian lift on the ordered direct sum of left and right spaces.
    HermitianizedFull,
    /// Right-right block of the Hermitian lift's polynomial response.
    HermitianizedEven,
    /// Left-right block of the Hermitian lift's polynomial response.
    HermitianizedOdd,
    /// Reduced polynomial in `B†B`, where `B = A / alpha`.
    MultiplicationEven,
    /// `B p(B†B)`, including the intermediate projection and source continuation.
    MultiplicationOdd,
}
impl Route {
    /// Whether the route needs an auxiliary qubit in addition to the response.
    #[must_use]
    pub const fn auxiliary(self) -> bool {
        !matches!(self, Self::Standard | Self::DirectHermitian)
    }
}
/// Distinct counters: polynomial/walk degree, oriented source uses, and retained
/// oracle instructions including structural wrappers and nested source calls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueryCounts {
    /// Phase/control degree; multiplication routes report their reduced degree.
    pub semantic: usize,
    /// Applications of the supplied source in its forward orientation.
    pub source_forward: usize,
    /// Applications of the supplied source's numerical adjoint.
    pub source_adjoint: usize,
    /// Oracle instructions including structural wrappers and nested calls.
    /// This is not a native FFI-call count or a density-operation count.
    pub retained_oracle_calls: usize,
}
/// Finite construction observations, with no automatic theorem implications.
#[derive(Debug, Clone, Copy, Default)]
pub struct TransformEvidence {
    /// Present for direct-route whole-oracle Hermiticity admission.
    pub whole_oracle_hermiticity_residual: Option<f64>,
    /// Present for direct-route left/right projector admission.
    pub projector_agreement_residual: Option<f64>,
    /// Diagnostic roundoff estimate for phase conversion, not an outward
    /// certificate. A source-polynomial certificate does not cover these shifts.
    pub phase_conversion_roundoff_estimate: f64,
}
/// Coherent continuation after the main circuit. A projected continuation
/// carries its bridge and source program together, so neither can be omitted.
#[derive(Debug, Clone)]
pub enum TransformContinuation {
    Direct,
    Projected {
        bridge: Box<Projection>,
        program: BoundProgram,
    },
}
/// Owned staged transform. Projection and continuation cannot be implicitly
/// discarded to obtain an oracle. All matrices and circuit bodies are immutable.
/// ```compile_fail
/// fn invalid(transform: quest_qsvt::ValidatedTransform) -> quest_circuit::Result<()> {
///     let mut body = quest_circuit::ProgramBuilder::new(3, 0)?;
///     body.oracle(&transform, &[], &[])?;
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ValidatedTransform {
    pub(super) encoding: ProjectedEncoding,
    pub(super) route: Route,
    pub(super) convention: &'static str,
    pub(super) degree: usize,
    pub(super) layout: OperandLayout,
    pub(super) main: BoundProgram,
    pub(super) input: Projection,
    pub(super) continuation_stage: TransformContinuation,
    pub(super) output: Projection,
    pub(super) queries: QueryCounts,
    pub(super) evidence: TransformEvidence,
}
impl ValidatedTransform {
    /// The owned source encoding, including its original physical normalization.
    #[must_use]
    pub const fn encoding(&self) -> &ProjectedEncoding {
        &self.encoding
    }
    /// Selected route; determines logical input/output spaces and staging.
    #[must_use]
    pub const fn route(&self) -> Route {
        self.route
    }
    /// Original standard phase tag or `ni-generalized-upper-left-final-k`.
    #[must_use]
    pub const fn convention(&self) -> &'static str {
        self.convention
    }
    /// Supplied sequence degree, including reduced degree for multiplication.
    #[must_use]
    pub const fn degree(&self) -> usize {
        self.degree
    }
    /// Ordered source targets and physical response/auxiliary positions.
    #[must_use]
    pub const fn operands(&self) -> &OperandLayout {
        &self.layout
    }
    /// Coherent main circuit, excluding projections and any continuation.
    #[must_use]
    pub const fn main(&self) -> &BoundProgram {
        &self.main
    }
    /// Input embedding and its fixed ancillary controls.
    #[must_use]
    pub const fn input(&self) -> &Projection {
        &self.input
    }
    /// Projection between main and continuation for odd multiplication.
    #[must_use]
    pub const fn bridge(&self) -> Option<&Projection> {
        match &self.continuation_stage {
            TransformContinuation::Direct => None,
            TransformContinuation::Projected { bridge, .. } => Some(bridge),
        }
    }
    /// Final coherent source application for odd multiplication, even at degree zero.
    #[must_use]
    pub const fn continuation(&self) -> Option<&BoundProgram> {
        match &self.continuation_stage {
            TransformContinuation::Direct => None,
            TransformContinuation::Projected { program, .. } => Some(program),
        }
    }
    /// One admitted stage following the main circuit.
    #[must_use]
    pub const fn continuation_stage(&self) -> &TransformContinuation {
        &self.continuation_stage
    }
    /// Logical output extraction after all coherent and projection stages.
    #[must_use]
    pub const fn output(&self) -> &Projection {
        &self.output
    }
    /// Original `alpha`; multiplication routes do not replace it by `alpha²`.
    #[must_use]
    pub const fn normalization(&self) -> Normalization {
        self.encoding.normalization()
    }
    /// Model source invocations, separate from native execution accounting.
    #[must_use]
    pub const fn query_counts(&self) -> QueryCounts {
        self.queries
    }
    /// Numerical construction diagnostics, separate from source certificates.
    #[must_use]
    pub const fn evidence(&self) -> TransformEvidence {
        self.evidence
    }
    /// Always `None`: construction supplies no theorem premises.
    ///
    /// Use [`Self::analysis`] with explicit assumptions for conditional standard
    /// bounds, or [`Self::diagnostics`] for finite observations.
    #[must_use]
    pub const fn theorem_error_bound(&self) -> Option<f64> {
        None
    }
    /// Independently form the subnormalized logical block in execution order.
    /// This includes the bridge projector and final continuation when present.
    ///
    /// The resulting shape is output logical dimension by input logical
    /// dimension. It is neither renormalized nor multiplied by source `alpha`.
    /// Storage scales with the physical Hilbert space, making this a cold
    /// reference rather than a scalable native execution path.
    /// # Errors
    /// Rejects construction budgets and dimension mismatches.
    pub fn materialize_block(&self) -> Result<Mat<Complex64>> {
        self.materialize_block_with_policy(self.encoding.policy())
    }
    pub(crate) fn materialize_block_with_policy(
        &self,
        policy: crate::NumericalPolicy,
    ) -> Result<Mat<Complex64>> {
        let dimension = 1usize
            .checked_shl(
                u32::try_from(self.layout.num_qubits())
                    .map_err(|_| Error::Budget("transform dimension"))?,
            )
            .ok_or(Error::Budget("transform dimension"))?;
        policy.check(dimension, dimension, 12)?;
        let input = self
            .input
            .materialize_isometry(self.layout.num_qubits(), policy)?;
        let main = materialize_program(&self.main, policy)?;
        let mut current = matrix::multiply(main.as_ref(), input.as_ref(), policy)?;
        if let TransformContinuation::Projected { bridge, program } = &self.continuation_stage {
            let basis = bridge.materialize_isometry(self.layout.num_qubits(), policy)?;
            let logical = matrix::multiply(basis.adjoint(), current.as_ref(), policy)?;
            current = matrix::multiply(basis.as_ref(), logical.as_ref(), policy)?;
            let continuation = materialize_program(program, policy)?;
            current = matrix::multiply(continuation.as_ref(), current.as_ref(), policy)?;
        }
        let output = self
            .output
            .materialize_isometry(self.layout.num_qubits(), policy)?;
        matrix::multiply(output.adjoint(), current.as_ref(), policy)
    }
}

/// Implemented only for the sealed public QSP phase convention markers.
pub trait StandardConvention: PhaseConvention + Sized {
    #[doc(hidden)]
    fn projector_phases(sequence: &PhaseSequence<Self>) -> ConvertedProjectorPhases;
}
impl StandardConvention for WxSymmetric {
    fn projector_phases(sequence: &PhaseSequence<Self>) -> ConvertedProjectorPhases {
        sequence.canonical().projector_phases_with_diagnostics()
    }
}
impl StandardConvention for WxLaurent {
    fn projector_phases(sequence: &PhaseSequence<Self>) -> ConvertedProjectorPhases {
        sequence.canonical().projector_phases_with_diagnostics()
    }
}
impl StandardConvention for CanonicalWxImag {
    fn projector_phases(sequence: &PhaseSequence<Self>) -> ConvertedProjectorPhases {
        sequence.projector_phases_with_diagnostics()
    }
}

/// Builder state retaining an owned encoding; constructed by `encoding()`.
#[derive(Debug)]
pub struct SuppliedEncoding(ProjectedEncoding);
/// Builder state retaining a typed standard phase sequence.
#[derive(Debug)]
pub struct StandardRecipe<C: StandardConvention>(PhaseSequence<C>);
/// Builder state retaining generalized controls and an explicit route.
#[derive(Debug)]
pub struct GeneralizedRecipe {
    controls: ControlSequence,
    route: Route,
}

/// Consuming builder: supply one encoding and one typed sequence/route before
/// `build` becomes available. Physical operands can be supplied independently.
/// ```compile_fail
/// quest_qsvt::TransformBuilder::new().build();
/// ```
#[derive(Debug)]
pub struct TransformBuilder<E = Missing, R = Missing> {
    encoding: E,
    recipe: R,
    layout: Option<OperandLayout>,
}
impl Default for TransformBuilder {
    fn default() -> Self {
        Self::new()
    }
}
impl TransformBuilder {
    /// Begin with neither encoding nor route supplied.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            encoding: Missing,
            recipe: Missing,
            layout: None,
        }
    }
}
impl<R> TransformBuilder<Missing, R> {
    /// Consume the source encoding. Clone it explicitly when building several routes.
    #[must_use]
    pub fn encoding(self, encoding: ProjectedEncoding) -> TransformBuilder<SuppliedEncoding, R> {
        TransformBuilder {
            encoding: SuppliedEncoding(encoding),
            recipe: self.recipe,
            layout: self.layout,
        }
    }
}
impl<E, R> TransformBuilder<E, R> {
    /// Choose physical operands; `build()` checks source width and auxiliary use.
    /// Omit this to use [`OperandLayout::canonical`].
    #[must_use]
    pub fn operands(mut self, layout: OperandLayout) -> Self {
        self.layout = Some(layout);
        self
    }
}
impl<E> TransformBuilder<E, Missing> {
    /// Select a standard route with an admitted, convention-typed QSP sequence.
    ///
    /// Even degree extracts on the right space; odd degree maps right to left.
    /// Numerical phase-conversion diagnostics are retained separately from any
    /// upstream polynomial certificate.
    #[must_use]
    pub fn standard<C: StandardConvention>(
        self,
        sequence: PhaseSequence<C>,
    ) -> TransformBuilder<E, StandardRecipe<C>> {
        TransformBuilder {
            encoding: self.encoding,
            recipe: StandardRecipe(sequence),
            layout: self.layout,
        }
    }
    fn generalized(
        self,
        controls: ControlSequence,
        route: Route,
    ) -> TransformBuilder<E, GeneralizedRecipe> {
        TransformBuilder {
            encoding: self.encoding,
            recipe: GeneralizedRecipe { controls, route },
            layout: self.layout,
        }
    }
    /// Select `p(B)` on a common logical space, for `B = A / alpha`.
    ///
    /// Building requires numerical whole-oracle Hermiticity, projector
    /// agreement, and ordered left/right basis agreement at fixed `1e-12`.
    #[must_use]
    pub fn direct(self, controls: ControlSequence) -> TransformBuilder<E, GeneralizedRecipe> {
        self.generalized(controls, Route::DirectHermitian)
    }
    /// Select `p([[0, B], [B†, 0]])` on the joint left-then-right logical space.
    #[must_use]
    pub fn hermitianized_full(
        self,
        controls: ControlSequence,
    ) -> TransformBuilder<E, GeneralizedRecipe> {
        self.generalized(controls, Route::HermitianizedFull)
    }
    /// Select the right-right block of the Hermitianized polynomial response.
    #[must_use]
    pub fn hermitianized_even(
        self,
        controls: ControlSequence,
    ) -> TransformBuilder<E, GeneralizedRecipe> {
        self.generalized(controls, Route::HermitianizedEven)
    }
    /// Select the left-right block of the Hermitianized polynomial response.
    #[must_use]
    pub fn hermitianized_odd(
        self,
        controls: ControlSequence,
    ) -> TransformBuilder<E, GeneralizedRecipe> {
        self.generalized(controls, Route::HermitianizedOdd)
    }
    /// Select the reduced response `p(B†B)` on the right logical space.
    /// The supplied control degree remains the transform's reported degree.
    #[must_use]
    pub fn multiplication_even(
        self,
        controls: ControlSequence,
    ) -> TransformBuilder<E, GeneralizedRecipe> {
        self.generalized(controls, Route::MultiplicationEven)
    }
    /// Select `B p(B†B)` from right to left, for `B = A / alpha`.
    ///
    /// Retains a bridge projection and a forward source continuation even when
    /// the supplied reduced control degree is zero.
    #[must_use]
    pub fn multiplication_odd(
        self,
        controls: ControlSequence,
    ) -> TransformBuilder<E, GeneralizedRecipe> {
        self.generalized(controls, Route::MultiplicationOdd)
    }
}
impl<C: StandardConvention> TransformBuilder<SuppliedEncoding, StandardRecipe<C>> {
    /// # Errors
    /// Rejects invalid layouts, finite conversion failures, and construction budgets.
    pub fn build(self) -> Result<ValidatedTransform> {
        let encoding = self.encoding.0;
        let layout = checked_layout(self.layout, &encoding, Route::Standard)?;
        crate::routes::standard(encoding, &self.recipe.0, layout)
    }
}
impl TransformBuilder<SuppliedEncoding, GeneralizedRecipe> {
    /// # Errors
    /// Rejects route premises, layout mismatches, and construction budgets.
    pub fn build(self) -> Result<ValidatedTransform> {
        let encoding = self.encoding.0;
        let layout = checked_layout(self.layout, &encoding, self.recipe.route)?;
        crate::routes::generalized(encoding, &self.recipe.controls, self.recipe.route, layout)
    }
}
fn checked_layout(
    layout: Option<OperandLayout>,
    encoding: &ProjectedEncoding,
    route: Route,
) -> Result<OperandLayout> {
    let layout = match layout {
        Some(layout) => layout,
        None => OperandLayout::canonical(encoding.num_qubits(), route.auxiliary())?,
    };
    if layout.source().len() != encoding.num_qubits()
        || layout.auxiliary().is_some() != route.auxiliary()
    {
        return Err(Error::Encoding("operand layout does not match route"));
    }
    Ok(layout)
}
