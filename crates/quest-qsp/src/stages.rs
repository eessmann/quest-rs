use crate::admission::contractivity;
use crate::{Complex64, Control, Error, FftBackend, Result, finite, kernel, zeros};
use quest_numerics::{ExecutionPolicy, Limits};
use quest_polynomial::{Basis, Chebyshev, Laurent, Polynomial};
use std::{
    marker::PhantomData,
    ops::{Add, Mul, Neg},
    sync::Arc,
};

/// Numerical factorization, independent of precision, convention and FFT backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SynthesisAlgorithm {
    /// Weiss ratio followed by the rank-two structured Half-Cholesky recurrence.
    RhwHalfCholesky,
    /// Default divide-and-conquer inverse nonlinear Fourier transform.
    #[default]
    InverseNlftDivideConquer,
}

/// Arithmetic used to generate a frozen binary64 payload, independent of solver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthesisPrecision {
    /// Production arithmetic in binary64.
    Binary64,
    /// Explicit offline arithmetic at this many bits before binary64 export.
    Arbitrary {
        /// Working precision of the successful offline synthesis attempt.
        bits: u32,
    },
}

/// Production numerical policy, independent of certification precision.
///
/// Memory admission includes retained data and concurrent workspace estimates.
/// Work is charged across FFT/convolution calls within each consuming stage,
/// including completion retries and inverse plus response reconstruction. Opaque
/// `RustFFT` planner memory remains an estimate rather than an allocator quota.
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    /// Requested numerical factorization. No automatic fallback occurs.
    pub algorithm: SynthesisAlgorithm,
    /// Maximum binary64 response-reconstruction diagnostic (default `1e-11`).
    /// Completion uses one eighth of this tolerance; neither check is an
    /// independent certificate of the exported sequence.
    pub response_tolerance: f64,
    /// Required positive gap from unit-circle magnitude one (default `1e-12`).
    pub contractivity_margin: f64,
    /// Largest grid allowed for contractivity refinement and Weiss completion.
    pub max_completion_grid: usize,
    /// Explicit scalar or available compiled SIMD FFT implementation.
    pub backend: FftBackend,
    /// Coefficient, modeled-byte and work budgets, charged per consuming stage.
    pub limits: Limits,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            algorithm: SynthesisAlgorithm::default(),
            response_tolerance: 1e-11,
            contractivity_margin: 1e-12,
            max_completion_grid: 1_048_576,
            backend: FftBackend::Scalar,
            limits: Limits::default(),
        }
    }
}
impl Policy {
    pub(crate) fn validate(self) -> Result<()> {
        if !(self.response_tolerance.is_finite()
            && self.response_tolerance > 0.0
            && self.contractivity_margin.is_finite()
            && self.contractivity_margin > 0.0
            && self.contractivity_margin < 1.0
            && self.max_completion_grid > 0)
        {
            return Err(Error::Policy(
                "positive finite tolerances and nonzero grid required",
            ));
        }
        Ok(())
    }
}

fn admit_target_storage(target: usize, source: usize, policy: Policy) -> Result<()> {
    if target > policy.limits.max_len || source > policy.limits.max_len {
        return Err(Error::Budget("retained target support"));
    }
    let bytes = target
        .checked_add(source)
        .and_then(|count| count.checked_mul(size_of::<Complex64>()))
        .ok_or(Error::Budget("retained target storage"))?;
    if bytes > policy.limits.max_bytes {
        return Err(Error::Budget("retained target storage"));
    }
    Ok(())
}

/// `UnitCircleResponse` upper-left polynomial response with the final K factor retained.
///
/// The signal product is `C0 diag(z,1) C1 ... diag(z,1) Cd`; its domain for
/// unitary interpretation is the complex unit circle.
#[derive(Debug, Clone, Copy)]
pub struct UnitCircleResponse;
/// `RealParityWx` Wx symmetric phases; target is the imaginary part of U00.
///
/// The source basis is real Chebyshev with one exact parity. Use
/// [`FrozenCandidate::response`] for the real signal in `[-1,1]`.
#[derive(Debug, Clone, Copy)]
pub struct RealParityWx;
/// Builder state before either a `real_parity_wx` or `unit_circle_response` target is supplied.
#[derive(Debug)]
pub struct MissingTarget;
/// Builder state retaining a copied, structurally validated target in mode `M`.
/// Strict contractivity is established only by [`SynthesisBuilder::admit`].
#[derive(Debug)]
pub struct ReadyTarget<M> {
    source_offset: i32,
    source_length: usize,
    target: Vec<Complex64>,
    source: Arc<Vec<Complex64>>,
    _mode: PhantomData<M>,
}

/// Configures a target; no numerical operation exists on the missing-target state.
///
/// Select [`Self::real_parity_wx`] or [`Self::unit_circle_response`], then consume the builder
/// with [`Self::admit`]. The resulting target can be completed and synthesized.
/// Each stage owns its data; the input polynomial borrow does not escape target
/// selection. See the crate quickstarts for both conventions.
///
/// ```compile_fail
/// quest_qsp::SynthesisBuilder::new().admit().unwrap();
/// ```
#[derive(Debug)]
pub struct SynthesisBuilder<S = MissingTarget> {
    state: S,
    policy: Policy,
}
impl Default for SynthesisBuilder<MissingTarget> {
    fn default() -> Self {
        Self::new()
    }
}
impl SynthesisBuilder<MissingTarget> {
    /// Start with the scalar binary64 default policy and no target.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: MissingTarget,
            policy: Policy::default(),
        }
    }

    /// Supply a nonnegative-support Laurent target, padding its offset explicitly.
    ///
    /// # Errors
    /// Rejects negative support, overflowing support or insufficient storage.
    pub fn unit_circle_response(
        self,
        target: &Polynomial<Laurent>,
    ) -> Result<SynthesisBuilder<ReadyTarget<UnitCircleResponse>>> {
        let (first, last) = target
            .stored_support()
            .unwrap_or_else(|| (target.basis().offset(), target.basis().offset()));
        if first < 0 {
            return Err(Error::Target(
                "unit_circle_response synthesis requires nonnegative support",
            ));
        }
        let count = usize::try_from(last)
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(Error::Budget("support"))?;
        admit_target_storage(count, count, self.policy)?;
        let mut values = zeros(count, self.policy.limits)?;
        let start = usize::try_from(first).map_err(|_| Error::Budget("support"))?;
        let end = start
            .checked_add(target.coefficients().len())
            .ok_or(Error::Budget("support"))?;
        values
            .get_mut(start..end)
            .ok_or(Error::Budget("support"))?
            .copy_from_slice(target.coefficients());
        let source = Arc::new(values.clone());
        Ok(SynthesisBuilder {
            state: ReadyTarget {
                source_offset: first,
                source_length: target.coefficients().len(),
                target: values,
                source,
                _mode: PhantomData,
            },
            policy: self.policy,
        })
    }

    /// Supply a real Chebyshev target with one exact parity. No parity projection
    /// or coefficient chopping is performed implicitly.
    ///
    /// # Errors
    /// Rejects complex coefficients, mixed parity, inexact subnormal halving,
    /// support overflow or insufficient storage.
    pub fn real_parity_wx(
        self,
        target: &Polynomial<Chebyshev>,
    ) -> Result<SynthesisBuilder<ReadyTarget<RealParityWx>>> {
        let source = target.coefficients();
        let degree = source
            .iter()
            .rposition(|v| *v != Complex64::new(0.0, 0.0))
            .unwrap_or(0);
        if source
            .iter()
            .enumerate()
            .any(|(i, v)| v.im != 0.0 || (i % 2 != degree % 2 && v.re != 0.0))
        {
            return Err(Error::Target(
                "real_parity_wx target must be real and have one exact parity",
            ));
        }
        let count = degree.checked_add(1).ok_or(Error::Budget("degree"))?;
        admit_target_storage(count, source.len(), self.policy)?;
        let mut values = zeros(count, self.policy.limits)?;
        for (index, coefficient) in source.iter().enumerate().take(values.len()) {
            if index % 2 != degree % 2 {
                continue;
            }
            let high = degree.checked_add(index).ok_or(Error::Budget("support"))? / 2;
            let low = degree.checked_sub(index).ok_or(Error::Budget("support"))? / 2;
            let half = coefficient.mul(0.5);
            if half.mul(2.0) != *coefficient {
                return Err(Error::Target(
                    "real_parity_wx conversion underflows a coefficient",
                ));
            }
            let entry = values.get_mut(high).ok_or(Error::Budget("support"))?;
            *entry = entry.add(half);
            let entry = values.get_mut(low).ok_or(Error::Budget("support"))?;
            *entry = entry.add(half);
        }
        Ok(SynthesisBuilder {
            state: ReadyTarget {
                source_offset: 0,
                source_length: source.len(),
                target: values,
                source: Arc::new(source.to_vec()),
                _mode: PhantomData,
            },
            policy: self.policy,
        })
    }
}
impl<S> SynthesisBuilder<S> {
    /// Replace configuration. Retained target support and storage are rechecked
    /// by `admit`, so setting a policy before or after target selection agrees.
    /// Set increased allocation limits before target selection when the input
    /// cannot fit the previous policy; an earlier allocation failure cannot be
    /// undone by this method. Policy values are validated by `admit`.
    #[must_use]
    pub const fn policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }
}
impl<M> SynthesisBuilder<ReadyTarget<M>> {
    /// Establish a positive contractivity margin using outward intervals. A
    /// coefficient l1 test is followed by bounded circle subdivision if needed.
    ///
    /// # Errors
    /// Rejects invalid policy or an unestablished positive contractivity margin.
    pub fn admit(self) -> Result<AdmittedTarget<M>> {
        self.policy.validate()?;
        admit_target_storage(
            self.state.target.len(),
            self.state.source.len(),
            self.policy,
        )?;
        let working = crate::workspace_policy(
            self.policy,
            self.state
                .target
                .len()
                .checked_add(self.state.source.len())
                .ok_or(Error::Budget("admission storage"))?,
        )?;
        let norm_upper = contractivity(&self.state.target, working)?;
        Ok(AdmittedTarget {
            source_offset: self.state.source_offset,
            source_length: self.state.source_length,
            target: Arc::new(self.state.target),
            source: self.state.source,
            norm_upper,
            policy: self.policy,
            _mode: PhantomData,
        })
    }
}

/// Owned target with an outward strict-contractivity bound, ready for completion.
///
/// This admission establishes the source-domain requirement, not an accuracy
/// certificate for controls that have yet to be synthesized.
#[derive(Debug, Clone)]
pub struct AdmittedTarget<M> {
    pub(crate) source_offset: i32,
    pub(crate) source_length: usize,
    pub(crate) target: Arc<Vec<Complex64>>,
    pub(crate) source: Arc<Vec<Complex64>>,
    pub(crate) norm_upper: f64,
    pub(crate) policy: Policy,
    pub(crate) _mode: PhantomData<M>,
}
impl<M> AdmittedTarget<M> {
    /// Complete the target through binary64 FFT Weiss factorization.
    ///
    /// # Errors
    /// Reports numerical failure, resource limits or an unestablished residual.
    pub fn complete(self) -> Result<CompletedPolynomial<M>> {
        self.complete_with(ExecutionPolicy::Sequential)
    }
    /// Complete with a borrowed caller execution pool. No pool is retained.
    /// # Errors
    /// Same numerical failures as [`Self::complete`], with parallel scratch admitted separately.
    pub fn complete_with(self, execution: ExecutionPolicy<'_>) -> Result<CompletedPolynomial<M>> {
        let working = crate::workspace_policy(
            self.policy,
            self.target
                .len()
                .checked_add(self.source.len())
                .ok_or(Error::Budget("completion retained storage"))?,
        )?;
        let (a_star, ratio, residual, grid) = kernel::complete(&self.target, working, execution)?;
        let ratio = WeissRatio {
            coefficients: ratio,
            target: Arc::clone(&self.target),
            norm_upper: self.norm_upper,
            grid,
            _mode: PhantomData,
        };
        Ok(CompletedPolynomial {
            admitted: self,
            a_star,
            ratio,
            residual,
            grid,
        })
    }
    /// Admitted nonnegative-power coefficients in increasing exponent order.
    /// `RealParityWx` sources have already undergone the mode's Chebyshev conversion.
    #[must_use]
    pub fn coefficients(&self) -> &[Complex64] {
        &self.target
    }
    /// Original coefficients, in the builder's mode-specific source basis.
    #[must_use]
    pub fn source_coefficients(&self) -> &[Complex64] {
        &self.source
    }
    /// Outward upper bound for target magnitude over the complete unit circle.
    #[must_use]
    pub const fn contractivity_upper_bound(&self) -> f64 {
        self.norm_upper
    }
}

/// Gauge fixed by Weiss completion, independent of response convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OuterGauge {
    /// Anti-analytic outer factor a has positive real constant coefficient.
    PositiveRealConstant,
}
/// Fourier coefficients of b/a, bound to the admitted target and Weiss grid.
/// This is candidate-generation evidence, not a certificate for the final export.
#[derive(Debug)]
pub struct WeissRatio<M> {
    coefficients: Vec<Complex64>,
    target: Arc<Vec<Complex64>>,
    norm_upper: f64,
    grid: usize,
    _mode: PhantomData<M>,
}
impl<M> WeissRatio<M> {
    /// Nonnegative Fourier coefficients of b exp(-G*) in increasing order.
    #[must_use]
    pub fn coefficients(&self) -> &[Complex64] {
        &self.coefficients
    }
    /// Exact admitted binary64 target from which this ratio was computed.
    #[must_use]
    pub fn target(&self) -> &[Complex64] {
        &self.target
    }
    /// Outward target norm bound established before completion.
    #[must_use]
    pub const fn contractivity_upper_bound(&self) -> f64 {
        self.norm_upper
    }
    /// FFT grid used to compute this ratio and its associated outer factor.
    #[must_use]
    pub const fn grid(&self) -> usize {
        self.grid
    }
    /// Explicit outer-factor gauge, separate from the compile-time response mode.
    #[must_use]
    pub const fn gauge(&self) -> OuterGauge {
        OuterGauge::PositiveRealConstant
    }
}

/// Completed outer factor; this numerical result is not a final certificate.
#[derive(Debug)]
pub struct CompletedPolynomial<M> {
    admitted: AdmittedTarget<M>,
    a_star: Vec<Complex64>,
    ratio: WeissRatio<M>,
    residual: f64,
    grid: usize,
}
impl<M> CompletedPolynomial<M> {
    /// Typed ratio retaining target, grid, gauge and contractivity provenance.
    #[must_use]
    pub const fn weiss_ratio(&self) -> &WeissRatio<M> {
        &self.ratio
    }

    fn working_policy(&self) -> Result<Policy> {
        // Retained target/source/complement plus the overlapping reflection,
        // phase, control and convention-conversion vectors while freezing.
        let retained = self
            .admitted
            .target
            .len()
            .checked_mul(10)
            .and_then(|n| n.checked_add(self.admitted.source.len()))
            .and_then(|n| n.checked_add(self.a_star.len()))
            .ok_or(Error::Budget("synthesis retained storage"))?;
        crate::workspace_policy(self.admitted.policy, retained)
    }
    /// Conjugated complement coefficients, in increasing nonnegative exponents.
    #[must_use]
    pub fn conjugate_complement_coefficients(&self) -> &[Complex64] {
        &self.a_star
    }
    /// Binary64 completion diagnostic; not an independent outward bound.
    #[must_use]
    pub const fn completion_residual(&self) -> f64 {
        self.residual
    }
    /// FFT sample count used by the accepted completion attempt.
    #[must_use]
    pub const fn completion_grid(&self) -> usize {
        self.grid
    }
}

/// Immutable exported binary64 controls or phases. Certification never changes
/// these values and higher verifier precision cannot repair their roundoff.
///
/// `UnitCircleResponse` candidates expose [`Self::control_sequence`]; `real_parity_wx`
/// candidates expose [`Self::phase_sequence`] and [`Self::response`]. For both
/// modes, [`Self::evaluate`] evaluates the transformed Laurent matrix product.
/// Production residuals are diagnostics. With the `certification` feature,
/// explicitly pass this value to `CertificationBuilder` to obtain separate
/// outward verification of the same export.
#[derive(Debug, Clone)]
pub struct FrozenCandidate<M> {
    pub(crate) synthesis_precision: SynthesisPrecision,
    pub(crate) admitted: AdmittedTarget<M>,
    pub(crate) controls: Arc<Vec<Control>>,
    pub(crate) a_star: Arc<Vec<Complex64>>,
    pub(crate) phases: Arc<Vec<f64>>,
    pub(crate) completion_residual: f64,
    pub(crate) reconstruction_residual: Option<f64>,
    pub(crate) completion_grid: usize,
}
impl CompletedPolynomial<UnitCircleResponse> {
    /// Freeze unit-circle matrices with the explicitly selected numerical solver.
    ///
    /// # Errors
    /// Rejects singular pivots, resource limits or excessive reconstruction error.
    pub fn synthesize(self) -> Result<FrozenCandidate<UnitCircleResponse>> {
        self.synthesize_with(ExecutionPolicy::Sequential)
    }
    /// Freeze with caller-owned parallel execution of independent operations.
    /// The dependent inverse halves remain ordered; no pool borrow escapes.
    /// # Errors
    /// Reports original numerical errors or insufficient partitioned parallel resources.
    pub fn synthesize_with(
        self,
        execution: ExecutionPolicy<'_>,
    ) -> Result<FrozenCandidate<UnitCircleResponse>> {
        let mut working = self.working_policy()?;
        let (gamma, work_used) = match working.algorithm {
            SynthesisAlgorithm::RhwHalfCholesky => {
                kernel::half_cholesky(self.ratio.coefficients(), working)?
            }
            SynthesisAlgorithm::InverseNlftDivideConquer => {
                kernel::inverse(&self.a_star, &self.admitted.target, working, execution)?
            }
        };
        working.limits.max_work = working
            .limits
            .max_work
            .checked_sub(work_used)
            .ok_or(Error::Budget("synthesis work"))?;
        let mut controls = kernel::controls(&gamma)?;
        let last = controls
            .last_mut()
            .ok_or(Error::Target("empty control sequence"))?;
        // Right multiply by K = [[0,-1],[1,0]], preserving every scalar phase.
        let [[a, b], [c, d]] = *last;
        *last = [[b, a.neg()], [d, c.neg()]];
        let residual =
            kernel::response_residual(&controls, &self.admitted.target, working, execution)?;
        Ok(FrozenCandidate {
            synthesis_precision: SynthesisPrecision::Binary64,
            admitted: self.admitted,
            a_star: Arc::new(self.a_star),
            controls: Arc::new(controls),
            phases: Arc::new(Vec::new()),
            completion_residual: self.residual,
            reconstruction_residual: Some(residual),
            completion_grid: self.grid,
        })
    }
}
impl CompletedPolynomial<RealParityWx> {
    /// Freeze symmetric `real_parity_wx` Wx phases from the inverse NLFT.
    ///
    /// # Errors
    /// Rejects singular pivots, resource limits or excessive reconstruction error.
    pub fn synthesize(self) -> Result<FrozenCandidate<RealParityWx>> {
        self.synthesize_with(ExecutionPolicy::Sequential)
    }
    /// Freeze with caller-owned parallel execution of independent operations.
    /// The dependent inverse halves remain ordered; no pool borrow escapes.
    /// # Errors
    /// Reports original numerical errors or insufficient partitioned parallel resources.
    pub fn synthesize_with(
        self,
        execution: ExecutionPolicy<'_>,
    ) -> Result<FrozenCandidate<RealParityWx>> {
        let mut working = self.working_policy()?;
        let (gamma, work_used) = match working.algorithm {
            SynthesisAlgorithm::RhwHalfCholesky => {
                kernel::half_cholesky(self.ratio.coefficients(), working)?
            }
            SynthesisAlgorithm::InverseNlftDivideConquer => {
                kernel::inverse(&self.a_star, &self.admitted.target, working, execution)?
            }
        };
        working.limits.max_work = working
            .limits
            .max_work
            .checked_sub(work_used)
            .ok_or(Error::Budget("synthesis work"))?;
        let mut phases: Vec<f64> = gamma.iter().map(|value| value.re.atan()).collect();
        let count = phases.len();
        for index in 0..count / 2 {
            let mirror = count
                .checked_sub(index)
                .and_then(|v| v.checked_sub(1))
                .ok_or(Error::Budget("phase support"))?;
            let a = *phases.get(index).ok_or(Error::Budget("phase support"))?;
            let b = *phases.get(mirror).ok_or(Error::Budget("phase support"))?;
            let middle = a.midpoint(b);
            *phases
                .get_mut(index)
                .ok_or(Error::Budget("phase support"))? = middle;
            *phases
                .get_mut(mirror)
                .ok_or(Error::Budget("phase support"))? = middle;
        }
        let mut controls = kernel::phase_controls(&phases)?;
        let last = controls
            .last_mut()
            .ok_or(Error::Target("empty phase sequence"))?;
        let [[a, b], [c, d]] = *last;
        *last = [[b, a.neg()], [d, c.neg()]];
        let residual =
            kernel::response_residual(&controls, &self.admitted.target, working, execution)?;
        Ok(FrozenCandidate {
            synthesis_precision: SynthesisPrecision::Binary64,
            admitted: self.admitted,
            a_star: Arc::new(self.a_star),
            controls: Arc::new(controls),
            phases: Arc::new(phases),
            completion_residual: self.residual,
            reconstruction_residual: Some(residual),
            completion_grid: self.grid,
        })
    }
}
impl<M> FrozenCandidate<M> {
    /// Exact original source storage offset and coefficient count, before Laurent padding.
    #[must_use]
    pub const fn source_storage(&self) -> (i32, usize) {
        (self.admitted.source_offset, self.admitted.source_length)
    }

    /// Computation precision that generated these immutable binary64 exports.
    #[must_use]
    pub const fn synthesis_precision(&self) -> SynthesisPrecision {
        self.synthesis_precision
    }

    /// Factorization explicitly selected for this frozen payload.
    #[must_use]
    pub const fn algorithm(&self) -> SynthesisAlgorithm {
        self.admitted.policy.algorithm
    }
    /// Frozen Laurent-product matrices, including the terminal K factor.
    /// `RealParityWx` matrices are diagnostic rotations derived from the phases;
    /// the phase payload remains the `real_parity_wx` export authority.
    #[must_use]
    pub fn controls(&self) -> &[Control] {
        &self.controls
    }
    /// Completion diagnostic retained from synthesis.
    /// This is not the independent certification report's completion bound.
    #[must_use]
    pub const fn completion_residual(&self) -> f64 {
        self.completion_residual
    }
    /// Binary64 upper-left response reconstruction diagnostic.
    /// Offline synthesis leaves this unavailable (`None`); use its
    /// independent certification report's reconstruction bound instead.
    #[must_use]
    pub const fn reconstruction_residual(&self) -> Option<f64> {
        self.reconstruction_residual
    }
    /// FFT sample count retained from the accepted completion attempt.
    #[must_use]
    pub const fn completion_grid(&self) -> usize {
        self.completion_grid
    }
    /// Transformed target coefficients in increasing nonnegative exponents.
    /// For `real_parity_wx` candidates these are not the original Chebyshev array.
    #[must_use]
    pub fn target(&self) -> &[Complex64] {
        &self.admitted.target
    }
    /// Frozen conjugated complement with increasing nonnegative exponents.
    #[must_use]
    pub fn conjugate_complement(&self) -> &[Complex64] {
        &self.a_star
    }
    /// Evaluate the complete control product at a finite signal. Unitarity is
    /// expected only on the unit circle, but polynomial evaluation also accepts
    /// other finite values.
    ///
    /// # Errors
    /// Rejects a nonfinite input or overflowing polynomial evaluation.
    pub fn evaluate(&self, signal: Complex64) -> Result<Control> {
        finite(signal, "evaluation input")?;
        let one = Complex64::new(1.0, 0.0);
        let zero = Complex64::new(0.0, 0.0);
        let mut product = [[one, zero], [zero, one]];
        for (index, control) in self.controls.iter().enumerate() {
            if index > 0 {
                let [[a, b], [c, d]] = product;
                product = [[a.mul(signal), b], [c.mul(signal), d]];
            }
            product = kernel::matrix_product(product, *control);
        }
        for value in product.into_iter().flatten() {
            finite(value, "evaluation")?;
        }
        Ok(product)
    }
}

impl FrozenCandidate<RealParityWx> {
    /// Immutable symmetric Wx angles in radians, in product order.
    #[must_use]
    pub fn phases(&self) -> &[f64] {
        &self.phases
    }
    /// Imaginary U00 of exp(i phi0 Z) Wx(x) ... exp(i phid Z).
    ///
    /// # Errors
    /// Rejects signals outside the finite real interval [-1,1].
    pub fn response(&self, x: f64) -> Result<f64> {
        if !x.is_finite() || !(-1.0..=1.0).contains(&x) {
            return Err(Error::Target("Wx signal must lie in [-1,1]"));
        }
        let zero = Complex64::new(0.0, 0.0);
        let off = Complex64::new(0.0, x.mul_add(-x, 1.0).max(0.0).sqrt());
        let wx = [[Complex64::new(x, 0.0), off], [off, Complex64::new(x, 0.0)]];
        let mut product = [
            [Complex64::new(1.0, 0.0), zero],
            [zero, Complex64::new(1.0, 0.0)],
        ];
        for (index, phase) in self.phases.iter().enumerate() {
            if index > 0 {
                product = kernel::matrix_product(product, wx);
            }
            let p = Complex64::from_polar(1.0, *phase);
            product = kernel::matrix_product(product, [[p, zero], [zero, p.conj()]]);
        }
        let [[response, _], _] = product;
        Ok(response.im)
    }
}
