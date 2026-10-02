use std::{fmt, ops::Mul, sync::Arc};

use rustfft::{Fft, FftPlannerScalar};

use crate::policy::{check_limit, checked_len, finite, zeros};
use crate::{Complex64, Error, ExecutionPolicy, Limits, ResourceUsage, Result};

mod shared;
pub use shared::{SharedConvolutionSession, SharedConvolutionWorkspace};

/// Arithmetic implementation chosen at workspace construction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FftBackend {
    /// Portable scalar `RustFFT` planner, independent of available SIMD.
    #[default]
    Scalar,
    /// Require an available compiled SIMD planner (AVX, SSE, or Neon).
    /// Enable the `simd` crate feature to compile these backends.
    Simd,
}

/// Sign of the complex exponential in the transform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FftDirection {
    /// Negative exponential: `sum_j x_j exp(-2 pi i j k / N)`.
    Forward,
    /// Positive exponential: `sum_j x_j exp(+2 pi i j k / N)`.
    Inverse,
}

/// Explicit scaling, independent of transform direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Normalization {
    /// Leave the transform unscaled.
    None,
    /// Divide every result by transform length.
    ByLength,
}

/// Forward and inverse plans with reusable in-place scratch.
///
/// Warm transforms do not allocate. The caller owns the input/output slice.
/// On invalid input, the slice remains unchanged. If arithmetic produces a
/// nonfinite result, the slice contains the failed transform and must be reset
/// by the caller. Backend selection is fixed for the workspace lifetime.
pub struct FftWorkspace {
    forward: Arc<dyn Fft<f64>>,
    inverse: Arc<dyn Fft<f64>>,
    scratch: Vec<Complex64>,
    len: usize,
    inverse_len: f64,
    usage: ResourceUsage,
    backend: FftBackend,
}

impl fmt::Debug for FftWorkspace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FftWorkspace")
            .field("len", &self.len)
            .field("backend", &self.backend)
            .field("usage", &self.usage)
            .finish_non_exhaustive()
    }
}

type Plans = (Arc<dyn Fft<f64>>, Arc<dyn Fft<f64>>);

fn plans(len: usize, backend: FftBackend) -> Result<Plans> {
    match backend {
        FftBackend::Scalar => {
            let mut planner = FftPlannerScalar::<f64>::new();
            Ok((planner.plan_fft_forward(len), planner.plan_fft_inverse(len)))
        }
        FftBackend::Simd => simd_plans(len),
    }
}

#[cfg(not(feature = "simd"))]
fn simd_plans(_len: usize) -> Result<Plans> {
    Err(Error::BackendUnavailable)
}

#[cfg(feature = "simd")]
fn simd_plans(len: usize) -> Result<Plans> {
    if let Ok(mut planner) = rustfft::FftPlannerAvx::<f64>::new() {
        return Ok((planner.plan_fft_forward(len), planner.plan_fft_inverse(len)));
    }
    if let Ok(mut planner) = rustfft::FftPlannerSse::<f64>::new() {
        return Ok((planner.plan_fft_forward(len), planner.plan_fft_inverse(len)));
    }
    if let Ok(mut planner) = rustfft::FftPlannerNeon::<f64>::new() {
        return Ok((planner.plan_fft_forward(len), planner.plan_fft_inverse(len)));
    }
    Err(Error::BackendUnavailable)
}

fn bytes(count: usize) -> Result<usize> {
    count
        .checked_mul(size_of::<Complex64>())
        .ok_or(Error::Overflow)
}

fn work(len: usize, transforms: usize) -> Result<usize> {
    // A power-of-two plan uses FFT butterflies: account eight scalar-operation
    // units per point per stage. This is a deterministic admission model, not
    // an instruction count or benchmark. Keep the conservative quadratic model
    // for arbitrary factorizations; no assumption about Bluestein is required.
    let per_transform = if len.is_power_of_two() {
        let stages = usize::try_from(len.ilog2().max(1)).map_err(|_| Error::Overflow)?;
        len.checked_mul(stages)
            .and_then(|value| value.checked_mul(8))
    } else {
        len.checked_mul(len)
    }
    .ok_or(Error::Overflow)?;
    per_transform.checked_mul(transforms).ok_or(Error::Overflow)
}

fn plan_allowance(len: usize) -> Result<usize> {
    // This models both plans and construction temporaries, not a hard allocator
    // bound. RustFFT exposes neither a memory query nor fallible planning.
    bytes(len.checked_mul(64).ok_or(Error::Overflow)?)
}

fn admit(usage: ResourceUsage, limits: Limits) -> Result<()> {
    let total = usage
        .buffer_bytes
        .checked_add(usage.planner_bytes_estimate)
        .ok_or(Error::Overflow)?;
    check_limit("bytes", total, limits.max_bytes)?;
    check_limit("work", usage.work_units, limits.max_work)
}

impl FftWorkspace {
    /// Plan both directions and allocate reusable scratch.
    ///
    /// # Errors
    /// Rejects zero/excessive lengths, resource budgets, unavailable SIMD, and
    /// failed wrapper allocations. `RustFFT`'s opaque planner uses infallible
    /// allocation internally; allocation failure there cannot be recovered here.
    pub fn new(len: usize, backend: FftBackend, limits: Limits) -> Result<Self> {
        checked_len(len, limits)?;
        let mut usage = ResourceUsage {
            buffer_bytes: 0,
            planner_bytes_estimate: plan_allowance(len)?,
            work_units: work(len, 1)?,
        };
        // Reject work and the plan allowance before calling the planner.
        admit(usage, limits)?;
        let (forward, inverse) = plans(len, backend)?;
        let scratch_len = forward
            .get_inplace_scratch_len()
            .max(inverse.get_inplace_scratch_len());
        usage.buffer_bytes = bytes(scratch_len)?;
        admit(usage, limits)?;
        let scratch = zeros(scratch_len)?;
        let length = f64::from(u32::try_from(len).map_err(|_| Error::Overflow)?);
        Ok(Self {
            forward,
            inverse,
            scratch,
            len,
            inverse_len: 1.0 / length,
            usage,
            backend,
        })
    }

    /// Transform length fixed at construction.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Workspaces always contain a positive length.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Exact owned buffer bytes and modeled plan/work admission.
    #[must_use]
    pub const fn resource_usage(&self) -> ResourceUsage {
        self.usage
    }

    /// Transform one finite slice with explicit direction and scaling.
    ///
    /// # Errors
    /// Rejects a wrong slice length or the first nonfinite input/output entry.
    pub fn transform(
        &mut self,
        values: &mut [Complex64],
        direction: FftDirection,
        normalization: Normalization,
    ) -> Result<()> {
        if values.len() != self.len {
            return Err(Error::Length("slice does not match FFT plan"));
        }
        finite(values)?;
        match direction {
            FftDirection::Forward => self.forward.process_with_scratch(values, &mut self.scratch),
            FftDirection::Inverse => self.inverse.process_with_scratch(values, &mut self.scratch),
        }
        if normalization == Normalization::ByLength {
            for value in values.iter_mut() {
                *value = value.mul(self.inverse_len);
            }
        }
        finite(values)
    }
}

/// Reusable zero-padded complex linear convolution.
///
/// Input coefficient order is increasing exponent; output length is exactly
/// `left.len() + right.len() - 1`. Laurent support offsets belong to the caller.
/// A workspace may accept shorter positive inputs on subsequent calls. Its FFT
/// length and allocation stay fixed, with stale padding cleared before reuse.
#[derive(Debug)]
pub struct ConvolutionWorkspace {
    fft: FftWorkspace,
    left: Vec<Complex64>,
    right: Vec<Complex64>,
    max_left: usize,
    max_right: usize,
    #[cfg(feature = "rayon")]
    second_scratch: Option<Vec<Complex64>>,
    usage: ResourceUsage,
}

impl ConvolutionWorkspace {
    /// Plan linear convolution up to the specified input lengths.
    ///
    /// # Errors
    /// Rejects zero sizes, arithmetic overflow, resource budgets, unavailable
    /// SIMD, or failed wrapper allocations. See FFT planning limitations above.
    pub fn new(
        max_left: usize,
        max_right: usize,
        backend: FftBackend,
        limits: Limits,
    ) -> Result<Self> {
        Self::new_with_policy(
            max_left,
            max_right,
            backend,
            limits,
            ExecutionPolicy::Sequential,
        )
    }

    /// Prepare scratch for independent forward FFTs in the selected caller pool.
    /// No pool borrow is stored in the workspace. Sequential preparation retains
    /// the original buffer footprint; parallel preparation accounts both scratch arrays.
    /// # Errors
    /// Rejects the same inputs as [`Self::new`], including the additional scratch budget.
    pub fn new_with_policy(
        max_left: usize,
        max_right: usize,
        backend: FftBackend,
        limits: Limits,
        execution: ExecutionPolicy<'_>,
    ) -> Result<Self> {
        checked_len(max_left, limits)?;
        checked_len(max_right, limits)?;
        let support = support_len(max_left, max_right)?;
        let len = support.checked_next_power_of_two().ok_or(Error::Overflow)?;
        checked_len(len, limits)?;
        let data_bytes = bytes(len.checked_mul(2).ok_or(Error::Overflow)?)?;
        let mut usage = ResourceUsage {
            buffer_bytes: data_bytes,
            planner_bytes_estimate: plan_allowance(len)?,
            work_units: work(len, 3)?.checked_add(len).ok_or(Error::Overflow)?,
        };
        admit(usage, limits)?;
        let fft_limits = Limits {
            max_bytes: limits
                .max_bytes
                .checked_sub(data_bytes)
                .ok_or(Error::Overflow)?,
            ..limits
        };
        let fft = FftWorkspace::new(len, backend, fft_limits)?;
        usage.buffer_bytes = data_bytes
            .checked_add(fft.resource_usage().buffer_bytes)
            .ok_or(Error::Overflow)?;
        #[cfg(feature = "rayon")]
        let parallel = matches!(execution, ExecutionPolicy::Rayon(pool) if pool.current_num_threads() > 1 && len >= 2048);
        #[cfg(not(feature = "rayon"))]
        let _ = execution;
        #[cfg(feature = "rayon")]
        if parallel {
            usage.buffer_bytes = usage
                .buffer_bytes
                .checked_add(bytes(fft.scratch.len())?)
                .ok_or(Error::Overflow)?;
        }
        admit(usage, limits)?;
        #[cfg(feature = "rayon")]
        let second_scratch = if parallel {
            Some(zeros(fft.scratch.len())?)
        } else {
            None
        };
        Ok(Self {
            fft,
            #[cfg(feature = "rayon")]
            second_scratch,
            left: zeros(len)?,
            right: zeros(len)?,
            max_left,
            max_right,
            usage,
        })
    }

    /// Fixed FFT length, including zero padding.
    #[must_use]
    pub const fn fft_len(&self) -> usize {
        self.fft.len()
    }

    /// Exact owned buffers and modeled planner/work admission.
    #[must_use]
    pub const fn resource_usage(&self) -> ResourceUsage {
        self.usage
    }

    /// Compute linear convolution sequentially without allocating.
    ///
    /// # Errors
    /// Rejects empty/oversized input, nonfinite input, and nonfinite intermediate
    /// or output coefficients. The first input is checked before the second.
    pub fn convolve(&mut self, left: &[Complex64], right: &[Complex64]) -> Result<&[Complex64]> {
        self.convolve_with_policy(left, right, ExecutionPolicy::Sequential)
    }

    /// Compute with caller-owned execution of independent operations.
    ///
    /// Parallel preparation permits concurrent forward FFTs with distinct scratch.
    /// Otherwise the FFTs remain serial. Pointwise multiplication has no
    /// cross-element reduction; finite scans always use left-first index order.
    /// Numerical work buffers are reused. Rayon's runtime may allocate external
    /// job-queue blocks and lazy OS synchronization storage, even after earlier
    /// calls. Batching inside `pool.install` avoids repeated external injection,
    /// but does not promise an allocation-free multithreaded scheduler.
    ///
    /// # Errors
    /// Same shape, budget, and finite-value failures as [`Self::convolve`].
    pub fn convolve_with_policy(
        &mut self,
        left: &[Complex64],
        right: &[Complex64],
        policy: ExecutionPolicy<'_>,
    ) -> Result<&[Complex64]> {
        if left.is_empty()
            || right.is_empty()
            || left.len() > self.max_left
            || right.len() > self.max_right
        {
            return Err(Error::Length(
                "convolution input exceeds planned support or is empty",
            ));
        }
        finite(left)?;
        finite(right)?;
        let support = support_len(left.len(), right.len())?;
        self.left.fill(Complex64::new(0.0, 0.0));
        self.right.fill(Complex64::new(0.0, 0.0));
        self.left
            .get_mut(..left.len())
            .ok_or(Error::Length("left support"))?
            .copy_from_slice(left);
        self.right
            .get_mut(..right.len())
            .ok_or(Error::Length("right support"))?
            .copy_from_slice(right);
        self.forward_pair(policy)?;
        multiply(&mut self.left, &self.right, policy);
        self.fft.transform(
            &mut self.left,
            FftDirection::Inverse,
            Normalization::ByLength,
        )?;
        self.left
            .get(..support)
            .ok_or(Error::Length("output support"))
    }
    fn forward_pair(&mut self, policy: ExecutionPolicy<'_>) -> Result<()> {
        #[cfg(feature = "rayon")]
        if let (ExecutionPolicy::Rayon(pool), Some(second_scratch)) =
            (policy, self.second_scratch.as_mut())
        {
            let forward = &self.fft.forward;
            let left = &mut self.left;
            let right = &mut self.right;
            let first_scratch = &mut self.fft.scratch;
            pool.install(|| {
                rayon::join(
                    || forward.process_with_scratch(left, first_scratch),
                    || forward.process_with_scratch(right, second_scratch),
                )
            });
            finite(&self.left)?;
            finite(&self.right)?;
            return Ok(());
        }
        #[cfg(not(feature = "rayon"))]
        let _ = policy;
        self.fft
            .transform(&mut self.left, FftDirection::Forward, Normalization::None)?;
        self.fft
            .transform(&mut self.right, FftDirection::Forward, Normalization::None)
    }
}

fn support_len(left: usize, right: usize) -> Result<usize> {
    left.checked_add(right)
        .and_then(|n| n.checked_sub(1))
        .ok_or(Error::Overflow)
}

fn multiply(left: &mut [Complex64], right: &[Complex64], policy: ExecutionPolicy<'_>) {
    match policy {
        #[cfg(feature = "rayon")]
        ExecutionPolicy::Rayon(pool) if left.len() >= 2048 && pool.current_num_threads() > 1 => {
            use rayon::prelude::*;
            pool.install(|| {
                left.par_iter_mut()
                    .zip(right.par_iter())
                    .for_each(|(a, b)| *a = a.mul(b));
            });
        }
        _ => {
            for (a, b) in left.iter_mut().zip(right) {
                *a = a.mul(b);
            }
        }
    }
}
