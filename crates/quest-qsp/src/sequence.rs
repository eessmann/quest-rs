use crate::{
    Complex64, Control, Error, FrozenCandidate, RealParityWx, Result, UnitCircleResponse, finite,
};
use std::{
    marker::PhantomData,
    ops::{Add, Mul, Neg},
    sync::Arc,
};

mod sealed {
    pub trait Sealed {}
}
/// Interpretation of phase payloads; names match the C++ interchange tags.
pub trait PhaseConvention: sealed::Sealed {
    /// Stable interchange tag identifying how the phase payload is interpreted.
    const TAG: &'static str;
    #[doc(hidden)]
    const SYMMETRIC: bool;
}
/// `pyqsp-wx-symmetric`: mirrored rotations must agree within `1e-12` at import.
#[derive(Debug, Clone, Copy)]
pub struct WxSymmetric;
/// `pyqsp-wx-laurent`: `real_parity_wx` conversion shifts the first rotation by pi/2.
#[derive(Debug, Clone, Copy)]
pub struct WxLaurent;
/// `wx-imaginary-u00`: the real target is the imaginary part of Wx U00.
#[derive(Debug, Clone, Copy)]
pub struct WxImaginaryU00;
impl sealed::Sealed for WxSymmetric {}
impl sealed::Sealed for WxLaurent {}
impl sealed::Sealed for WxImaginaryU00 {}
impl PhaseConvention for WxSymmetric {
    const TAG: &'static str = "pyqsp-wx-symmetric";
    const SYMMETRIC: bool = true;
}
impl PhaseConvention for WxLaurent {
    const TAG: &'static str = "pyqsp-wx-laurent";
    const SYMMETRIC: bool = false;
}
impl PhaseConvention for WxImaginaryU00 {
    const TAG: &'static str = "wx-imaginary-u00";
    const SYMMETRIC: bool = false;
}

/// Immutable finite angles with a compile-time convention tag.
///
/// Import validation is numerical and does not independently certify a target
/// response. Convention conversion retains a diagnostic rather than an outward
/// guarantee. For example, mirrored Wx rotations can be imported and explicitly
/// converted for a projector-phase consumer:
///
/// ```
/// use quest_qsp::{PhaseSequence, WxSymmetric};
/// let phases = PhaseSequence::<WxSymmetric>::builder(vec![0.2, 0.2]).build()?;
/// let projector = phases.real_parity_wx().projector_phases_with_diagnostics();
/// assert_eq!(projector.values().len(), 2);
/// assert!(projector.roundoff_estimate().is_finite());
/// # Ok::<(), quest_qsp::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct PhaseSequence<C: PhaseConvention> {
    pub(crate) values: Arc<Vec<f64>>,
    conversion_roundoff_estimate: f64,
    _convention: PhantomData<C>,
}
/// Owns imported angles until [`Self::build`] validates the tagged convention.
#[derive(Debug)]
pub struct PhaseSequenceBuilder<C: PhaseConvention> {
    values: Vec<f64>,
    _convention: PhantomData<C>,
}
impl<C: PhaseConvention> PhaseSequence<C> {
    /// Take ownership of angles in radians; validation occurs at `build`.
    #[must_use]
    pub const fn builder(values: Vec<f64>) -> PhaseSequenceBuilder<C> {
        PhaseSequenceBuilder {
            values,
            _convention: PhantomData,
        }
    }
    /// Frozen angles in radians in signal-product order.
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }
    /// Accumulated numerical convention-conversion diagnostic, not a certificate.
    #[must_use]
    pub const fn conversion_roundoff_estimate(&self) -> f64 {
        self.conversion_roundoff_estimate
    }
    /// Number of signal factors, one fewer than the number of angles.
    #[must_use]
    pub fn degree(&self) -> usize {
        self.values.len().saturating_sub(1)
    }
    /// Interchange tag of this sequence's compile-time convention.
    #[must_use]
    pub const fn convention(&self) -> &'static str {
        C::TAG
    }
}
impl<C: PhaseConvention> PhaseSequenceBuilder<C> {
    /// Validate finite phases and the tagged symmetry requirement.
    ///
    /// # Errors
    /// Rejects empty/nonfinite values or asymmetric Wx-symmetric input.
    pub fn build(self) -> Result<PhaseSequence<C>> {
        if self.values.is_empty() || self.values.iter().any(|v| !v.is_finite()) {
            return Err(Error::Target("phase sequence must be nonempty and finite"));
        }
        if C::SYMMETRIC
            && self
                .values
                .iter()
                .zip(self.values.iter().rev())
                .any(|(a, b)| {
                    // Reducing huge angles modulo rounded binary64 TAU can
                    // declare distinct physical rotations equal. Compare the
                    // actual exported rotations, without subtracting angles.
                    let (a_sin, a_cos) = a.sin_cos();
                    let (b_sin, b_cos) = b.sin_cos();
                    (a_sin - b_sin).hypot(a_cos - b_cos) > 1e-12
                })
        {
            return Err(Error::Target("phase sequence is not Wx symmetric"));
        }
        Ok(PhaseSequence {
            values: Arc::new(self.values),
            conversion_roundoff_estimate: 0.0,
            _convention: PhantomData,
        })
    }
}
impl PhaseSequence<WxSymmetric> {
    /// Retag the shared symmetric Wx payload as `real_parity_wx` imaginary-U00 phases.
    /// This conversion does not change or recompute the stored angles.
    #[must_use]
    pub fn real_parity_wx(&self) -> PhaseSequence<WxImaginaryU00> {
        PhaseSequence {
            values: Arc::clone(&self.values),
            conversion_roundoff_estimate: self.conversion_roundoff_estimate,
            _convention: PhantomData,
        }
    }
}
/// Numerically constructed projector phases and their conversion diagnostic.
/// This result grants no exact phase identity or source-certificate privilege.
#[derive(Debug, Clone)]
pub struct ConvertedProjectorPhases {
    values: Vec<f64>,
    roundoff_estimate: f64,
}
impl ConvertedProjectorPhases {
    /// Converted projector-rotation angles in radians, in product order.
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }
    /// Sum of observed rotation residuals plus binary64 arithmetic allowances.
    /// This is not an outward error bound or a certificate for transcendental evaluation.
    #[must_use]
    pub const fn roundoff_estimate(&self) -> f64 {
        self.roundoff_estimate
    }
}
#[derive(Clone, Copy)]
enum PhaseShift {
    QuarterForward,
    QuarterBackward,
    EighthBackward,
}

fn shift_phase(value: f64, shift: PhaseShift) -> (f64, f64) {
    let (sin, cos) = value.sin_cos();
    let (expected_sin, expected_cos, offset) = match shift {
        PhaseShift::QuarterForward => (cos, -sin, std::f64::consts::FRAC_PI_2),
        PhaseShift::QuarterBackward => (-cos, sin, -std::f64::consts::FRAC_PI_2),
        PhaseShift::EighthBackward => (
            (sin - cos) * std::f64::consts::FRAC_1_SQRT_2,
            (cos + sin) * std::f64::consts::FRAC_1_SQRT_2,
            -std::f64::consts::FRAC_PI_4,
        ),
    };
    let residual = |angle: f64| {
        let (s, c) = angle.sin_cos();
        (s - expected_sin).hypot(c - expected_cos)
    };
    let direct = value + offset;
    // Keep ordinary exported phase bits. Large finite angles can absorb the
    // whole offset; compose their actual rotations instead of reducing by TAU.
    let converted = if direct.is_finite() && residual(direct) <= 4.0 * f64::EPSILON {
        direct
    } else {
        expected_sin.atan2(expected_cos)
    };
    // A numerical construction diagnostic only: libm sin/cos/atan2 are not an
    // interval backend and this allowance must never become a certified bound.
    (
        converted,
        8.0_f64.mul_add(f64::EPSILON, residual(converted)),
    )
}
impl PhaseSequence<WxLaurent> {
    /// Numerically compose the first rotation with a positive quarter turn.
    /// Unshifted phases retain their original bits; no rounded-TAU reduction occurs.
    #[must_use]
    pub fn real_parity_wx(&self) -> PhaseSequence<WxImaginaryU00> {
        let mut values = self.values.as_ref().clone();
        let mut estimate = self.conversion_roundoff_estimate;
        if let Some(first) = values.first_mut() {
            let (converted, residual) = shift_phase(*first, PhaseShift::QuarterForward);
            *first = converted;
            estimate += residual;
        }
        PhaseSequence {
            values: Arc::new(values),
            conversion_roundoff_estimate: estimate,
            _convention: PhantomData,
        }
    }
}
impl PhaseSequence<WxImaginaryU00> {
    /// Numerically convert Wx rotations to exp(i phi (2P-I)) projector rotations.
    /// Use [`Self::projector_phases_with_diagnostics`] to retain conversion evidence.
    #[must_use]
    pub fn projector_phases(&self) -> Vec<f64> {
        self.projector_phases_with_diagnostics().values
    }
    /// Convert while retaining the numerical roundoff diagnostic.
    /// The returned values carry no inherited source certificate.
    #[must_use]
    pub fn projector_phases_with_diagnostics(&self) -> ConvertedProjectorPhases {
        let mut values = self.values.as_ref().clone();
        let mut estimate = self.conversion_roundoff_estimate;
        let count = values.len();
        for (index, value) in values.iter_mut().enumerate() {
            let shift = if count == 1 || (index > 0 && Some(index) != count.checked_sub(1)) {
                PhaseShift::QuarterBackward
            } else {
                PhaseShift::EighthBackward
            };
            let (converted, residual) = shift_phase(*value, shift);
            *value = converted;
            estimate += residual;
        }
        ConvertedProjectorPhases {
            values,
            roundoff_estimate: estimate,
        }
    }
}
impl FrozenCandidate<RealParityWx> {
    /// Share the already frozen symmetric phase payload; no numerical work.
    #[must_use]
    pub fn phase_sequence(&self) -> PhaseSequence<WxSymmetric> {
        PhaseSequence {
            values: Arc::clone(&self.phases),
            conversion_roundoff_estimate: 0.0,
            _convention: PhantomData,
        }
    }
}

/// Numerically admitted `unit_circle_response` controls including the terminal K factor.
///
/// The product convention is `C0 diag(z,1) C1 ... diag(z,1) Cd`. Matrix imports
/// must already include K; angle imports append it during construction.
#[derive(Debug, Clone)]
pub struct ControlSequence {
    matrices: Arc<Vec<Control>>,
}
/// Builder state before any matrices or paired angles are supplied.
#[derive(Debug)]
pub struct MissingControls;
/// Builder state owning matrices that still require numerical admission.
#[derive(Debug)]
pub struct SuppliedControls {
    matrices: Vec<Control>,
}
/// Import `unit_circle_response` matrices or paired angles, then explicitly admit them.
#[derive(Debug)]
pub struct ControlSequenceBuilder<S = MissingControls> {
    state: S,
}
impl ControlSequence {
    /// Start an import builder without control data.
    #[must_use]
    pub const fn builder() -> ControlSequenceBuilder {
        ControlSequenceBuilder {
            state: MissingControls,
        }
    }
    /// Complete admitted matrices in product order, including terminal K.
    #[must_use]
    pub fn matrices(&self) -> &[Control] {
        &self.matrices
    }
    /// Number of signal factors, one fewer than the number of matrices.
    #[must_use]
    pub fn degree(&self) -> usize {
        self.matrices.len().saturating_sub(1)
    }
}
impl ControlSequenceBuilder {
    /// Supply complete matrices; this method performs no normalization or checks.
    #[must_use]
    pub const fn matrices(
        self,
        matrices: Vec<Control>,
    ) -> ControlSequenceBuilder<SuppliedControls> {
        ControlSequenceBuilder {
            state: SuppliedControls { matrices },
        }
    }
    /// Construct paper-native controls from psi rotation magnitudes and phi phases.
    ///
    /// # Errors
    /// Rejects unequal/empty arrays, nonfinite angles or failed allocation.
    pub fn angles(
        self,
        psi: &[f64],
        phi: &[f64],
    ) -> Result<ControlSequenceBuilder<SuppliedControls>> {
        if psi.len() != phi.len() || psi.is_empty() || psi.iter().chain(phi).any(|v| !v.is_finite())
        {
            return Err(Error::Target(
                "unit_circle_response psi/phi lengths or finiteness",
            ));
        }
        let mut matrices = Vec::new();
        matrices
            .try_reserve_exact(psi.len())
            .map_err(|_| Error::Budget("unit_circle_response control import"))?;
        for (&magnitude, &phase) in psi.iter().zip(phi) {
            let diagonal = Complex64::new(magnitude.cos(), 0.0);
            let off = Complex64::from_polar(magnitude.sin(), phase);
            matrices.push([[diagonal, off], [off.conj().neg(), diagonal]]);
        }
        let last = matrices.last_mut().ok_or(Error::Target("empty controls"))?;
        let [[a, b], [c, d]] = *last;
        *last = [[b, a.neg()], [d, c.neg()]];
        Ok(self.matrices(matrices))
    }
}
impl ControlSequenceBuilder<SuppliedControls> {
    /// Admit the actual matrices at fixed 1e-10 unitarity tolerance.
    /// No matrix is normalized or assigned exact inverse semantics.
    ///
    /// # Errors
    /// Rejects empty, nonfinite or insufficiently unitary matrix sequences.
    pub fn build(self) -> Result<ControlSequence> {
        if self.state.matrices.is_empty() {
            return Err(Error::Target("empty controls"));
        }
        for matrix in &self.state.matrices {
            for &entry in matrix.iter().flatten() {
                finite(entry, "control admission")?;
            }
            let [[a, b], [c, d]] = *matrix;
            let diagonal_0 = a.norm_sqr() + c.norm_sqr() - 1.0;
            let diagonal_1 = b.norm_sqr() + d.norm_sqr() - 1.0;
            let cross = a.conj().mul(b).add(c.conj().mul(d)).norm();
            let residual = diagonal_0.hypot(diagonal_1).hypot(cross).hypot(cross);
            if !residual.is_finite() || residual > 1e-10 {
                return Err(Error::NotEstablished {
                    stage: "control unitarity",
                    bound: residual,
                    tolerance: 1e-10,
                });
            }
        }
        Ok(ControlSequence {
            matrices: Arc::new(self.state.matrices),
        })
    }
}
impl FrozenCandidate<UnitCircleResponse> {
    /// Share the already frozen controls without additional numerical work.
    /// This preserves their values and does not run independent certification.
    #[must_use]
    pub fn control_sequence(&self) -> ControlSequence {
        ControlSequence {
            matrices: Arc::clone(&self.controls),
        }
    }
}
