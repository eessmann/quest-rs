use crate::{Complex64, Control, Error, Policy, Result, SynthesisAlgorithm, finite, zeros};
use quest_numerics::{
    ConvolutionWorkspace, ExecutionPolicy, FftDirection, FftWorkspace, Normalization,
    SharedConvolutionWorkspace,
};
use std::{
    collections::BTreeMap,
    ops::{Add, Div, Mul, Neg, Sub},
};

pub fn matrix_product(left: Control, right: Control) -> Control {
    let [[left_00, left_01], [left_10, left_11]] = left;
    let [[right_00, right_01], [right_10, right_11]] = right;
    [
        [
            left_00.mul(right_00).add(left_01.mul(right_10)),
            left_00.mul(right_01).add(left_01.mul(right_11)),
        ],
        [
            left_10.mul(right_00).add(left_11.mul(right_10)),
            left_10.mul(right_01).add(left_11.mul(right_11)),
        ],
    ]
}

/// Normalization avoids squaring an unscaled reflection coefficient.
pub fn controls(gamma: &[Complex64]) -> Result<Vec<Control>> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(gamma.len())
        .map_err(|_| Error::Budget("controls"))?;
    for value in gamma {
        output.push(control(*value)?);
    }
    Ok(output)
}

fn control(value: Complex64) -> Result<Control> {
    finite(value, "reflection normalization")?;
    let scale = 1.0_f64.max(value.re.abs()).max(value.im.abs());
    let real = value.re / scale;
    let imag = value.im / scale;
    let one = 1.0 / scale;
    let norm = one.hypot(real).hypot(imag);
    let diagonal = Complex64::new(one / norm, 0.0);
    let off = Complex64::new(real / norm, imag / norm);
    Ok([[diagonal, off], [off.conj().neg(), diagonal]])
}

/// Reconstruct the actual exported phase trigonometry. A tan/normalization
/// round-trip is mathematically equivalent but can erase binary64 export error.
pub fn phase_controls(phases: &[f64]) -> Result<Vec<Control>> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(phases.len())
        .map_err(|_| Error::Budget("phase controls"))?;
    for &phase in phases {
        let diagonal = finite(Complex64::new(phase.cos(), 0.0), "phase cosine")?;
        let off = finite(Complex64::new(phase.sin(), 0.0), "phase sine")?;
        output.push([[diagonal, off], [off.neg(), diagonal]]);
    }
    Ok(output)
}

struct Convolutions<'pool> {
    execution: ExecutionPolicy<'pool>,
    plans: BTreeMap<usize, ConvolutionWorkspace>,
    policy: Policy,
    bytes: usize,
    work_used: usize,
}
impl<'pool> Convolutions<'pool> {
    const fn new(policy: Policy, execution: ExecutionPolicy<'pool>) -> Self {
        Self {
            execution,
            plans: BTreeMap::new(),
            policy,
            bytes: 0,
            work_used: 0,
        }
    }
    fn product(&mut self, left: &[Complex64], right: &[Complex64]) -> Result<Vec<Complex64>> {
        // Cache plans by maximum operand length. Every inverse node at the same
        // tree level reuses buffers; first/second halves remain ordered.
        let size = left
            .len()
            .max(right.len())
            .checked_next_power_of_two()
            .ok_or(Error::Budget("convolution support"))?;
        if !self.plans.contains_key(&size) {
            let limits = quest_numerics::Limits {
                max_bytes: self
                    .policy
                    .limits
                    .max_bytes
                    .checked_sub(self.bytes)
                    .ok_or(Error::Budget("plan storage"))?,
                ..self.policy.limits
            };
            let plan = ConvolutionWorkspace::new_with_policy(
                size,
                size,
                self.policy.backend,
                limits,
                self.execution,
            )?;
            let usage = plan.resource_usage();
            self.bytes = self
                .bytes
                .checked_add(usage.buffer_bytes)
                .and_then(|n| n.checked_add(usage.planner_bytes_estimate))
                .ok_or(Error::Budget("plan storage"))?;
            self.plans.insert(size, plan);
        }
        let plan = self
            .plans
            .get_mut(&size)
            .ok_or(Error::Budget("missing convolution plan"))?;
        self.work_used = self
            .work_used
            .checked_add(plan.resource_usage().work_units)
            .ok_or(Error::Budget("convolution work"))?;
        if self.work_used > self.policy.limits.max_work {
            return Err(Error::Budget("convolution work"));
        }
        let values = plan.convolve_with_policy(left, right, self.execution)?;
        let mut output = zeros(values.len(), self.policy.limits)?;
        output.copy_from_slice(values);
        Ok(output)
    }
}

// Inverse groups retain two immutable RHS spectra only within each session.
// Ordinary convolutions used by completion/certification keep their footprint.
struct SharedConvolutions<'pool> {
    execution: ExecutionPolicy<'pool>,
    plans: BTreeMap<usize, SharedConvolutionWorkspace>,
    policy: Policy,
    bytes: usize,
    work_used: usize,
}
impl<'pool> SharedConvolutions<'pool> {
    const fn new(policy: Policy, execution: ExecutionPolicy<'pool>) -> Self {
        Self {
            execution,
            plans: BTreeMap::new(),
            policy,
            bytes: 0,
            work_used: 0,
        }
    }
    fn windows(
        &mut self,
        left: [&[Complex64]; 4],
        right: [&[Complex64]; 2],
        windows: [(usize, usize); 4],
    ) -> Result<[Vec<Complex64>; 4]> {
        let size = left
            .iter()
            .chain(right.iter())
            .map(|values| values.len())
            .max()
            .and_then(usize::checked_next_power_of_two)
            .ok_or(Error::Budget("convolution support"))?;
        if !self.plans.contains_key(&size) {
            let limits = quest_numerics::Limits {
                max_bytes: self
                    .policy
                    .limits
                    .max_bytes
                    .checked_sub(self.bytes)
                    .ok_or(Error::Budget("plan storage"))?,
                ..self.policy.limits
            };
            let plan = SharedConvolutionWorkspace::new_with_policy(
                size,
                size,
                self.policy.backend,
                limits,
                self.execution,
            )?;
            let usage = plan.resource_usage();
            self.bytes = self
                .bytes
                .checked_add(usage.buffer_bytes)
                .and_then(|n| n.checked_add(usage.planner_bytes_estimate))
                .ok_or(Error::Budget("plan storage"))?;
            self.plans.insert(size, plan);
        }
        let plan = self
            .plans
            .get_mut(&size)
            .ok_or(Error::Budget("missing convolution plan"))?;
        let mut session = plan.session(right, self.execution);
        let mut output: [Vec<Complex64>; 4] = std::array::from_fn(|_| Vec::new());
        for (((left, (offset, count)), out), index) in left
            .into_iter()
            .zip(windows)
            .zip(&mut output)
            .zip([0, 1, 1, 0])
        {
            // Charge each lazy cold/warm product before its numerical execution.
            self.work_used = self
                .work_used
                .checked_add(session.work_for(index)?)
                .ok_or(Error::Budget("convolution work"))?;
            if self.work_used > self.policy.limits.max_work {
                return Err(Error::Budget("convolution work"));
            }
            let values = session.product(left, index)?;
            *out = zeros(count, self.policy.limits)?;
            for (index, value) in out.iter_mut().enumerate() {
                *value = at(
                    values,
                    offset
                        .checked_add(index)
                        .ok_or(Error::Budget("NLFT product window"))?,
                );
            }
        }
        Ok(output)
    }
}

fn at(values: &[Complex64], index: usize) -> Complex64 {
    values
        .get(index)
        .copied()
        .unwrap_or(Complex64::new(0.0, 0.0))
}
fn reverse_conjugate(values: &[Complex64]) -> Vec<Complex64> {
    values.iter().rev().map(Complex64::conj).collect()
}

pub enum CompletionData<C> {
    InverseNlft,
    Rhw(Vec<C>),
}

#[expect(
    clippy::too_many_lines,
    reason = "Ordered Weiss transforms and shared retry budget kept together"
)]
pub fn complete(
    target: &[Complex64],
    policy: Policy,
    execution: ExecutionPolicy<'_>,
) -> Result<(Vec<Complex64>, CompletionData<Complex64>, f64, usize)> {
    if target.is_empty() {
        return Err(Error::Target("empty target"));
    }
    let mut grid = target
        .len()
        .checked_mul(4)
        .and_then(usize::checked_next_power_of_two)
        .ok_or(Error::Budget("completion grid"))?
        .max(32);
    let mut last_residual = None;
    let mut remaining = policy;
    let (transforms, grid_vectors, coefficient_vectors) = match policy.algorithm {
        SynthesisAlgorithm::InverseNlftDivideConquer => (4, 2, 1),
        SynthesisAlgorithm::RhwHalfCholesky => (5, 3, 2),
    };
    while grid <= policy.max_completion_grid {
        let work = grid
            .checked_mul(
                usize::try_from(grid.ilog2().max(1)).map_err(|_| Error::Budget("FFT work"))?,
            )
            .and_then(|n| n.checked_mul(8))
            .and_then(|n| n.checked_mul(transforms))
            .ok_or(Error::Budget("FFT work"))?;
        let rest = remaining
            .limits
            .max_work
            .checked_sub(work)
            .ok_or(Error::Budget("Weiss work"))?;
        // Scope FFT plans and samples so they are gone before convolution plans
        // and residual payloads are allocated.
        let (mut a_star, ratio) = {
            let workspace = crate::workspace_policy(
                policy,
                grid.checked_mul(grid_vectors)
                    .and_then(|v| {
                        target
                            .len()
                            .checked_mul(coefficient_vectors)
                            .and_then(|n| v.checked_add(n))
                    })
                    .ok_or(Error::Budget("Weiss payload"))?,
            )?;
            let mut values = zeros(grid, policy.limits)?;
            values
                .get_mut(..target.len())
                .ok_or(Error::Budget("completion grid"))?
                .copy_from_slice(target);
            let mut fft = FftWorkspace::new(grid, policy.backend, workspace.limits)?;
            fft.transform(&mut values, FftDirection::Inverse, Normalization::None)?;
            let ratio_samples = match policy.algorithm {
                SynthesisAlgorithm::InverseNlftDivideConquer => None,
                SynthesisAlgorithm::RhwHalfCholesky => Some(values.clone()),
            };
            pointwise(&mut values, execution, |_, value| {
                let norm = value.re.hypot(value.im);
                let remainder = (-norm).mul_add(norm, 1.0);
                if remainder <= 0.0 || !remainder.is_finite() {
                    return Err(Error::NotEstablished {
                        stage: "Weiss logarithm domain",
                        bound: norm,
                        tolerance: 1.0,
                    });
                }
                Ok(Complex64::new(0.5 * remainder.ln(), 0.0))
            })?;
            fft.transform(&mut values, FftDirection::Forward, Normalization::ByLength)?;
            // Schwarz extension in QuEST QSP convention is anti-analytic:
            // zero mode unchanged, strictly negative Fourier modes doubled.
            pointwise(&mut values, execution, |index, value| {
                Ok(if index == 0 {
                    value
                } else if index <= grid / 2 {
                    Complex64::new(0.0, 0.0)
                } else {
                    value.mul(2.0)
                })
            })?;
            fft.transform(&mut values, FftDirection::Inverse, Normalization::None)?;
            // G* is anti-analytic. RHW needs b/a = b exp(-G*), not
            // the outer complement coefficients consumed by inverse NLFT.
            let ratio = if let Some(mut samples) = ratio_samples {
                for (sample, exponent) in samples.iter_mut().zip(&values) {
                    *sample = finite(sample.mul(exponent.neg().exp()), "Weiss ratio")?;
                }
                fft.transform(&mut samples, FftDirection::Forward, Normalization::ByLength)?;
                CompletionData::Rhw(
                    samples
                        .get(..target.len())
                        .ok_or(Error::Budget("Weiss ratio support"))?
                        .to_vec(),
                )
            } else {
                CompletionData::InverseNlft
            };
            pointwise(&mut values, execution, |_, value| {
                finite(value.exp(), "Weiss exponential")
            })?;
            fft.transform(&mut values, FftDirection::Forward, Normalization::ByLength)?;
            let mut a_star = zeros(target.len(), policy.limits)?;
            for (index, value) in a_star.iter_mut().enumerate() {
                let slot = if index == 0 {
                    0
                } else {
                    grid.checked_sub(index)
                        .ok_or(Error::Budget("completion support"))?
                };
                *value = at(&values, slot).conj();
            }
            (a_star, ratio)
        };
        remaining.limits.max_work = rest;
        // Positive real zero mode is the fixed outer-factor convention.
        a_star
            .first_mut()
            .ok_or(Error::Target("empty complement"))?
            .im = 0.0;
        let (residual, work_used) = completion_residual(&a_star, target, remaining, execution)?;
        remaining.limits.max_work = remaining
            .limits
            .max_work
            .checked_sub(work_used)
            .ok_or(Error::Budget("completion work"))?;
        last_residual = Some(residual);
        if residual <= policy.response_tolerance / 8.0 {
            return Ok((a_star, ratio, residual, grid));
        }
        grid = grid
            .checked_mul(2)
            .ok_or(Error::Budget("completion refinement"))?;
    }
    Err(Error::NotEstablished {
        stage: "Weiss completion",
        bound: last_residual.ok_or(Error::Budget("no completion grid admitted"))?,
        tolerance: policy.response_tolerance / 8.0,
    })
}

fn completion_residual(
    a_star: &[Complex64],
    b: &[Complex64],
    policy: Policy,
    execution: ExecutionPolicy<'_>,
) -> Result<(f64, usize)> {
    let policy = payload_allowance(policy, b.len())?;
    let mut convolutions = Convolutions::new(policy, execution);
    let a_product = convolutions.product(a_star, &reverse_conjugate(a_star))?;
    let b_product = convolutions.product(b, &reverse_conjugate(b))?;
    let middle = b
        .len()
        .checked_sub(1)
        .ok_or(Error::Target("empty residual"))?;
    let mut total = 0.0;
    for (index, (a, b)) in a_product.iter().zip(&b_product).enumerate() {
        let ideal = Complex64::new(if index == middle { 1.0 } else { 0.0 }, 0.0);
        total += a.add(b).sub(ideal).norm();
    }
    if total.is_finite() {
        Ok((total, convolutions.work_used))
    } else {
        Err(Error::NonFinite("completion residual"))
    }
}

struct InverseNode {
    xi: Vec<Complex64>,
    eta: Vec<Complex64>,
}

fn midpoint_from_windows(windows: [Vec<Complex64>; 4]) -> Result<(Vec<Complex64>, Vec<Complex64>)> {
    let [mut a, xb, mut b, xa] = windows;
    // All products have succeeded. Reuse two compact windows as the outputs,
    // retaining the original per-index a-then-b validation order.
    for (index, (a, b)) in a.iter_mut().zip(&mut b).enumerate() {
        *a = finite((*a).add(at(&xb, index)), "NLFT midpoint")?;
        *b = finite((*b).sub(at(&xa, index)), "NLFT midpoint")?;
    }
    Ok((a, b))
}
fn transfer_from_windows(windows: [Vec<Complex64>; 4]) -> Result<InverseNode> {
    let [ex, mut xi, mut eta, xx] = windows;
    for (index, (x, e)) in xi.iter_mut().zip(&mut eta).enumerate() {
        // sharp(p)=z^m conjugate(p), so reversed coefficients begin at 1.
        let shifted_first = index
            .checked_sub(1)
            .map_or(Complex64::new(0.0, 0.0), |i| at(&ex, i));
        let shifted_second = index
            .checked_sub(1)
            .map_or(Complex64::new(0.0, 0.0), |i| at(&xx, i));
        *x = finite(shifted_first.add(*x), "NLFT reconstruction")?;
        *e = finite((*e).sub(shifted_second), "NLFT reconstruction")?;
    }
    Ok(InverseNode { xi, eta })
}

pub fn inverse(
    a_star: &[Complex64],
    b: &[Complex64],
    policy: Policy,
    execution: ExecutionPolicy<'_>,
) -> Result<(Vec<Complex64>, usize)> {
    if a_star.len() != b.len() || b.is_empty() {
        return Err(Error::Target("inverse NLFT pair support"));
    }
    let policy = payload_allowance(policy, b.len())?;
    let mut gamma = zeros(b.len(), policy.limits)?;
    let mut work = SharedConvolutions::new(policy, execution);
    inverse_node(a_star, b, &mut gamma, &mut work, false)?;
    Ok((gamma, work.work_used))
}

fn inverse_node(
    a_star: &[Complex64],
    b: &[Complex64],
    gamma: &mut [Complex64],
    work: &mut SharedConvolutions<'_>,
    transfer: bool,
) -> Result<Option<InverseNode>> {
    let count = b.len();
    if count == 1 {
        let pivot = at(a_star, 0);
        if pivot == Complex64::new(0.0, 0.0) {
            return Err(Error::SingularPivot);
        }
        let reflection = finite(at(b, 0).div(pivot), "inverse NLFT pivot")?;
        *gamma
            .first_mut()
            .ok_or(Error::Target("empty reflection support"))? = reflection;
        if !transfer {
            return Ok(None);
        }
        let [[scale, xi], _] = control(reflection)?;
        return Ok(Some(InverseNode {
            xi: vec![xi],
            eta: vec![scale],
        }));
    }
    let lower_len = count / 2;
    let upper_len = count
        .checked_sub(lower_len)
        .ok_or(Error::Budget("NLFT split"))?;
    let (gamma_upper, gamma_lower) = gamma
        .split_at_mut_checked(upper_len)
        .ok_or(Error::Budget("NLFT split"))?;
    let upper = inverse_node(
        a_star
            .get(..upper_len)
            .ok_or(Error::Budget("NLFT prefix"))?,
        b.get(..upper_len).ok_or(Error::Budget("NLFT prefix"))?,
        gamma_upper,
        work,
        true,
    )?
    .ok_or(Error::Target("missing upper transfer"))?;
    let eta_conj = reverse_conjugate(&upper.eta);
    let xi_conj = reverse_conjugate(&upper.xi);
    let conjugate_offset = upper_len
        .checked_sub(1)
        .ok_or(Error::Budget("NLFT offset"))?;
    let (midpoint_a, midpoint_b) = midpoint_from_windows(work.windows(
        [&eta_conj, &xi_conj, &upper.eta, &upper.xi],
        [a_star, b],
        [
            (conjugate_offset, lower_len),
            (conjugate_offset, lower_len),
            (upper_len, lower_len),
            (upper_len, lower_len),
        ],
    )?)?;
    // Second-half inverse depends on the completed first-half midpoint update.
    let lower = inverse_node(&midpoint_a, &midpoint_b, gamma_lower, work, true)?
        .ok_or(Error::Target("missing lower transfer"))?;
    drop((midpoint_a, midpoint_b));
    // Both children remain full transfer producers; only the caller's root
    // discards transfer reconstruction after its reflections are established.
    if !transfer {
        return Ok(None);
    }
    let shifted_count = count.checked_sub(1).ok_or(Error::Budget("NLFT offset"))?;
    let node = transfer_from_windows(work.windows(
        [&eta_conj, &upper.xi, &upper.eta, &xi_conj],
        [&lower.xi, &lower.eta],
        [
            (0, shifted_count),
            (0, count),
            (0, count),
            (0, shifted_count),
        ],
    )?)?;
    Ok(Some(node))
}

pub fn response_residual(
    controls: &[Control],
    target: &[Complex64],
    policy: Policy,
    execution: ExecutionPolicy<'_>,
) -> Result<f64> {
    let policy = payload_allowance(policy, controls.len())?;
    let coefficients = product_tree(controls, &mut Convolutions::new(policy, execution))?;
    let mut residual = 0.0;
    for (index, value) in coefficients.aa.iter().enumerate() {
        residual += value.sub(at(target, index)).norm();
    }
    if !residual.is_finite() {
        return Err(Error::NonFinite("control reconstruction"));
    }
    if residual > policy.response_tolerance {
        return Err(Error::NotEstablished {
            stage: "binary64 reconstruction",
            bound: residual,
            tolerance: policy.response_tolerance,
        });
    }
    Ok(residual)
}

struct PolynomialMatrix {
    aa: Vec<Complex64>,
    ab: Vec<Complex64>,
    ba: Vec<Complex64>,
    bb: Vec<Complex64>,
}
fn product_tree(controls: &[Control], work: &mut Convolutions<'_>) -> Result<PolynomialMatrix> {
    if controls.len() == 1 {
        let [[a, b], [c, d]] = *controls
            .first()
            .ok_or(Error::Target("empty control product"))?;
        return Ok(PolynomialMatrix {
            aa: vec![a],
            ab: vec![b],
            ba: vec![c],
            bb: vec![d],
        });
    }
    let (left, right) = controls
        .split_at_checked(controls.len() / 2)
        .ok_or(Error::Budget("control split"))?;
    if left.is_empty() || right.is_empty() {
        return Err(Error::Target("empty controls"));
    }
    let (left, right) = children(left, right, work)?;
    Ok(PolynomialMatrix {
        aa: entry_product(&left.aa, &right.aa, &left.ab, &right.ba, work)?,
        ab: entry_product(&left.aa, &right.ab, &left.ab, &right.bb, work)?,
        ba: entry_product(&left.ba, &right.aa, &left.bb, &right.ba, work)?,
        bb: entry_product(&left.ba, &right.ab, &left.bb, &right.bb, work)?,
    })
}
fn entry_product(
    a: &[Complex64],
    b: &[Complex64],
    c: &[Complex64],
    d: &[Complex64],
    work: &mut Convolutions<'_>,
) -> Result<Vec<Complex64>> {
    let first = work.product(a, b)?;
    let second = work.product(c, d)?;
    let mut result = zeros(
        first
            .len()
            .checked_add(1)
            .ok_or(Error::Budget("control support"))?,
        work.policy.limits,
    )?;
    for (index, value) in result.iter_mut().enumerate() {
        let shifted = index
            .checked_sub(1)
            .map_or(Complex64::new(0.0, 0.0), |i| at(&first, i));
        *value = shifted.add(at(&second, index));
    }
    Ok(result)
}

fn payload_allowance(policy: Policy, length: usize) -> Result<Policy> {
    // A depth-first inverse retains fewer than 20*n complex scalars across
    // its geometric chain of live parent nodes; the product tree retains fewer
    // than 24*n. 64*n also covers output vectors, leaf controls and Vec headers.
    // Concurrent forward children have disjoint supports, so their live payload
    // sums obey the same bound. Opaque FFT planner estimates are admitted
    // separately against the remainder, partitioned between concurrent branches.
    crate::workspace_policy(
        policy,
        length
            .checked_mul(64)
            .ok_or(Error::Budget("recursive payload storage"))?,
    )
}

fn pointwise(
    values: &mut [Complex64],
    execution: ExecutionPolicy<'_>,
    operation: impl Fn(usize, Complex64) -> Result<Complex64> + Sync,
) -> Result<()> {
    #[cfg(feature = "rayon")]
    if let ExecutionPolicy::Rayon(pool) = execution
        && values.len() >= 4096
        && pool.current_num_threads() > 1
    {
        use rayon::prelude::*;
        let error = pool.install(|| {
            values
                .par_iter_mut()
                .enumerate()
                .map(|(index, value)| match operation(index, *value) {
                    Ok(result) => {
                        *value = result;
                        None
                    }
                    Err(error) => Some(error),
                })
                .find_first(Option::is_some)
                .flatten()
        });
        return error.map_or(Ok(()), Err);
    }
    #[cfg(not(feature = "rayon"))]
    let _ = execution;
    for (index, value) in values.iter_mut().enumerate() {
        *value = operation(index, *value)?;
    }
    Ok(())
}
fn children(
    left: &[Control],
    right: &[Control],
    work: &mut Convolutions<'_>,
) -> Result<(PolynomialMatrix, PolynomialMatrix)> {
    #[cfg(feature = "rayon")]
    if let ExecutionPolicy::Rayon(pool) = work.execution
        && left
            .len()
            .checked_add(right.len())
            .ok_or(Error::Budget("parallel tree support"))?
            >= 1024
        && pool.current_num_threads() > 1
    {
        let bytes = work
            .policy
            .limits
            .max_bytes
            .checked_sub(work.bytes)
            .ok_or(Error::Budget("parallel tree storage"))?;
        let units = work
            .policy
            .limits
            .max_work
            .checked_sub(work.work_used)
            .ok_or(Error::Budget("parallel tree work"))?;
        let mut left_policy = work.policy;
        left_policy.limits.max_bytes = bytes / 2;
        left_policy.limits.max_work = units / 2;
        let mut right_policy = work.policy;
        right_policy.limits.max_bytes = bytes
            .checked_sub(left_policy.limits.max_bytes)
            .ok_or(Error::Budget("parallel tree storage"))?;
        right_policy.limits.max_work = units
            .checked_sub(left_policy.limits.max_work)
            .ok_or(Error::Budget("parallel tree work"))?;
        let execution = work.execution;
        let (left, right) = pool.install(|| {
            rayon::join(
                || {
                    let mut child = Convolutions::new(left_policy, execution);
                    let value = product_tree(left, &mut child);
                    (value, child.work_used)
                },
                || {
                    let mut child = Convolutions::new(right_policy, execution);
                    let value = product_tree(right, &mut child);
                    (value, child.work_used)
                },
            )
        });
        // Join first, then propagate errors in the original left-before-right order.
        let left_value = left.0?;
        let right_value = right.0?;
        work.work_used = work
            .work_used
            .checked_add(left.1)
            .and_then(|v| v.checked_add(right.1))
            .ok_or(Error::Budget("parallel tree work"))?;
        if work.work_used > work.policy.limits.max_work {
            return Err(Error::Budget("parallel tree work"));
        }
        return Ok((left_value, right_value));
    }
    Ok((product_tree(left, work)?, product_tree(right, work)?))
}

#[cfg(all(test, feature = "rayon"))]
mod parallel_tests {
    use super::*;
    use googletest::prelude::*;
    #[gtest]
    fn joined_forward_trees_keep_work_counts_and_all_coefficient_bits() -> googletest::Result<()> {
        let controls = phase_controls(&vec![0.02; 1025])?;
        let mut serial = Convolutions::new(Policy::default(), ExecutionPolicy::Sequential);
        let expected = product_tree(&controls, &mut serial)?;
        for workers in [1, 2, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()?;
            let mut parallel = Convolutions::new(Policy::default(), ExecutionPolicy::Rayon(&pool));
            let actual = product_tree(&controls, &mut parallel)?;
            expect_that!(parallel.work_used, eq(serial.work_used));
            for (actual, expected) in [&actual.aa, &actual.ab, &actual.ba, &actual.bb]
                .into_iter()
                .zip([&expected.aa, &expected.ab, &expected.ba, &expected.bb])
            {
                for (a, b) in actual.iter().zip(expected) {
                    expect_that!(a.re.to_bits(), eq(b.re.to_bits()));
                    expect_that!(a.im.to_bits(), eq(b.im.to_bits()));
                }
            }
        }
        Ok(())
    }
    #[gtest]
    fn joined_tree_reports_left_budget_failure_before_right_nonfinite_input()
    -> googletest::Result<()> {
        let left = phase_controls(&vec![0.02; 512])?;
        let mut right = phase_controls(&vec![0.02; 512])?;
        if let Some([[value, _], _]) = right.first_mut() {
            *value = Complex64::new(f64::INFINITY, 0.0);
        }
        for workers in [2, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()?;
            let mut policy = Policy::default();
            policy.limits.max_work = 100;
            let mut work = Convolutions::new(policy, ExecutionPolicy::Rayon(&pool));
            expect_true!(matches!(
                children(&left, &right, &mut work),
                Err(Error::Budget("convolution work"))
            ));
        }
        Ok(())
    }
    #[gtest]
    fn parallel_pointwise_failure_selects_the_first_input_index() -> googletest::Result<()> {
        for workers in [1, 2, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()?;
            let mut values = vec![Complex64::new(1.0, 0.0); 8192];
            let result = pointwise(
                &mut values,
                ExecutionPolicy::Rayon(&pool),
                |index, value| {
                    if index == 7 {
                        Err(Error::NonFinite("first indexed failure"))
                    } else if index == 100 {
                        Err(Error::NonFinite("later indexed failure"))
                    } else {
                        Ok(value)
                    }
                },
            );
            expect_true!(matches!(
                result,
                Err(Error::NonFinite("first indexed failure"))
            ));
        }
        Ok(())
    }
}

/// Complex extension of Ni/Ying 2410.06409v2, Algorithm 2 (rank-two
/// displacement Schur recurrence), with Laneve 2503.03026v2 §5.3 indexing.
/// K - Z K Z† = [e0,p][e0,p]†, p = conj(reverse(c)).
/// Fuse forward substitution into the column recurrence: O(n²) arithmetic,
/// O(n) storage; neither K nor dense L is ever materialized.
#[expect(
    clippy::many_single_char_names,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "Displacement recurrence uses four length-n vectors and checked nonempty n; all indices follow 0 <= k < j < n"
)]
pub fn half_cholesky(c: &[Complex64], policy: Policy) -> Result<(Vec<Complex64>, usize)> {
    let n = c.len();
    if n == 0 {
        return Err(Error::Target("empty Weiss ratio"));
    }
    let work = n
        .checked_mul(n)
        .and_then(|v| v.checked_mul(32))
        .ok_or(Error::Budget("Half-Cholesky work"))?;
    if work > policy.limits.max_work {
        return Err(Error::Budget("Half-Cholesky work"));
    }
    let _ = crate::workspace_policy(
        policy,
        n.checked_mul(4)
            .ok_or(Error::Budget("Half-Cholesky storage"))?,
    )?;
    let mut first = zeros(n, policy.limits)?;
    first[0] = Complex64::new(1.0, 0.0);
    let mut second = reverse_conjugate(c);
    let mut solution = second.clone();
    for k in 0..n {
        let x = first[k];
        let y = second[k];
        let scale = x.norm().hypot(y.norm());
        if !scale.is_finite() || scale == 0.0 {
            return Err(Error::SingularPivot);
        }
        let alpha = x / scale;
        let beta = y / scale;
        let rhs = solution[k];
        let mut previous = Complex64::new(scale, 0.0);
        for j in k + 1..n {
            let u = finite(
                first[j] * alpha.conj() + second[j] * beta.conj(),
                "Half-Cholesky generator",
            )?;
            let v = finite(
                -first[j] * beta + second[j] * alpha,
                "Half-Cholesky generator",
            )?;
            solution[j] = finite(solution[j] - (u / scale) * rhs, "Half-Cholesky solve")?;
            first[j] = previous;
            second[j] = v;
            previous = u;
        }
    }
    Ok((reverse_conjugate(&solution), work))
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::needless_range_loop,
    clippy::panic_in_result_fn,
    reason = "Independent dense test oracle is explicitly bounded to 32 coefficients"
)]
mod rhw_tests {
    use super::*;

    // Independent direct complex block solve from Laneve §5.2. For each
    // leading Toeplitz block B, solve [I,-B†; B,I] [u;v]=[0;e_last].
    // F_{n-m}=u[0]/v[m-1]. No LDL/displacement steps are shared.
    fn dense_rhw(c: &[Complex64]) -> Vec<Complex64> {
        assert!(c.len() <= 32, "bounded reference only");
        let n = c.len();
        let mut answer = vec![Complex64::new(0.0, 0.0); n];
        for m in 1..=n {
            let size = 2 * m;
            let mut a = vec![vec![Complex64::new(0.0, 0.0); size]; size];
            let mut rhs = vec![Complex64::new(0.0, 0.0); size];
            rhs[size - 1] = Complex64::new(1.0, 0.0);
            for i in 0..size {
                a[i][i] = Complex64::new(1.0, 0.0);
            }
            for row in 0..m {
                for col in 0..=row {
                    let b = c[n - 1 - (row - col)].conj();
                    a[m + row][col] = b;
                    a[col][m + row] = -b.conj();
                }
            }
            for k in 0..size {
                let pivot = (k..size)
                    .max_by(|&i, &j| a[i][k].norm().total_cmp(&a[j][k].norm()))
                    .unwrap();
                a.swap(k, pivot);
                rhs.swap(k, pivot);
                let diagonal = a[k][k];
                for j in k..size {
                    a[k][j] /= diagonal;
                }
                rhs[k] /= diagonal;
                for i in k + 1..size {
                    let scale = a[i][k];
                    for j in k..size {
                        let v = a[k][j];
                        a[i][j] -= scale * v;
                    }
                    let v = rhs[k];
                    rhs[i] -= scale * v;
                }
            }
            for k in (0..size).rev() {
                for j in k + 1..size {
                    let v = rhs[j];
                    rhs[k] -= a[k][j] * v;
                }
            }
            answer[n - m] = rhs[0] / rhs[size - 1];
        }
        answer
    }
    #[test]
    fn half_cholesky_admits_work_and_memory_before_allocating() {
        let ratio = vec![Complex64::new(0.1, 0.2); 17];
        let mut policy = Policy::default();
        policy.limits.max_work = 17 * 17 * 32 - 1;
        assert!(matches!(
            half_cholesky(&ratio, policy),
            Err(Error::Budget("Half-Cholesky work"))
        ));
        policy = Policy::default();
        policy.limits.max_bytes = 4 * 17 * size_of::<Complex64>() - 1;
        assert!(matches!(
            half_cholesky(&ratio, policy),
            Err(Error::Budget(_))
        ));
    }
    #[test]
    fn structured_matches_independent_direct_block_rhw() -> Result<()> {
        let mut seed = 0x415f_beef_u64;
        for n in [1, 2, 3, 5, 8, 17, 32] {
            let c: Vec<_> = (0..n)
                .map(|_| {
                    seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                    let re =
                        f64::from(u32::try_from(seed >> 32).unwrap()) / f64::from(u32::MAX) - 0.5;
                    seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                    let im =
                        f64::from(u32::try_from(seed >> 32).unwrap()) / f64::from(u32::MAX) - 0.5;
                    Complex64::new(re, im)
                })
                .collect();
            let actual = half_cholesky(&c, Policy::default())?.0;
            for (actual, expected) in actual.iter().zip(dense_rhw(&c)) {
                assert!(
                    (*actual - expected).norm() < 5e-13,
                    "n={n}: {actual} != {expected}"
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::panic_in_result_fn,
    reason = "Bounded deterministic inverse fixtures and independent pre-optimization oracle"
)]
mod inverse_tests {
    use super::*;

    fn fixture(count: usize) -> (Vec<Complex64>, Vec<Complex64>) {
        let a = (0..count)
            .map(|index| {
                if index == 0 {
                    Complex64::new(0.95, 0.0)
                } else {
                    Complex64::new(0.001 * f64::from(u32::try_from(index).unwrap()), -0.0005)
                }
            })
            .collect();
        let b = (0..count)
            .map(|index| Complex64::new(0.01 / f64::from(u32::try_from(index + 1).unwrap()), 0.002))
            .collect();
        (a, b)
    }
    fn bits(values: &[Complex64]) -> Vec<(u64, u64)> {
        values
            .iter()
            .map(|value| (value.re.to_bits(), value.im.to_bits()))
            .collect()
    }
    fn full_transfer_oracle(
        a_star: &[Complex64],
        b: &[Complex64],
        gamma: &mut [Complex64],
        work: &mut Convolutions<'_>,
    ) -> Result<InverseNode> {
        let count = b.len();
        if count == 1 {
            let pivot = at(a_star, 0);
            if pivot == Complex64::new(0.0, 0.0) {
                return Err(Error::SingularPivot);
            }
            let reflection = finite(at(b, 0).div(pivot), "inverse NLFT pivot")?;
            *gamma
                .first_mut()
                .ok_or(Error::Target("empty reflection support"))? = reflection;
            let matrices = controls(&[reflection])?;
            let [[scale, xi], _] = *matrices
                .first()
                .ok_or(Error::Target("empty leaf control"))?;
            return Ok(InverseNode {
                xi: vec![xi],
                eta: vec![scale],
            });
        }
        let lower_len = count / 2;
        let upper_len = count
            .checked_sub(lower_len)
            .ok_or(Error::Budget("NLFT split"))?;
        let (gamma_upper, gamma_lower) = gamma
            .split_at_mut_checked(upper_len)
            .ok_or(Error::Budget("NLFT split"))?;
        let upper = full_transfer_oracle(
            a_star
                .get(..upper_len)
                .ok_or(Error::Budget("NLFT prefix"))?,
            b.get(..upper_len).ok_or(Error::Budget("NLFT prefix"))?,
            gamma_upper,
            work,
        )?;
        let mut midpoint_a = zeros(lower_len, work.policy.limits)?;
        let mut midpoint_b = zeros(lower_len, work.policy.limits)?;
        let eta_conj = reverse_conjugate(&upper.eta);
        let xi_conj = reverse_conjugate(&upper.xi);
        let ea = work.product(&eta_conj, a_star)?;
        let xb = work.product(&xi_conj, b)?;
        let eb = work.product(&upper.eta, b)?;
        let xa = work.product(&upper.xi, a_star)?;
        let conjugate_offset = upper_len
            .checked_sub(1)
            .ok_or(Error::Budget("NLFT offset"))?;
        for (index, (a, b)) in midpoint_a.iter_mut().zip(&mut midpoint_b).enumerate() {
            let a_index = conjugate_offset
                .checked_add(index)
                .ok_or(Error::Budget("NLFT midpoint"))?;
            let b_index = upper_len
                .checked_add(index)
                .ok_or(Error::Budget("NLFT midpoint"))?;
            *a = finite(at(&ea, a_index).add(at(&xb, a_index)), "NLFT midpoint")?;
            *b = finite(at(&eb, b_index).sub(at(&xa, b_index)), "NLFT midpoint")?;
        }
        // Second-half inverse depends on the completed first-half midpoint update.
        let lower = full_transfer_oracle(&midpoint_a, &midpoint_b, gamma_lower, work)?;
        let ex = work.product(&eta_conj, &lower.xi)?;
        let xe = work.product(&upper.xi, &lower.eta)?;
        let ee = work.product(&upper.eta, &lower.eta)?;
        let xx = work.product(&xi_conj, &lower.xi)?;
        let mut xi = zeros(count, work.policy.limits)?;
        let mut eta = zeros(count, work.policy.limits)?;
        for (index, (x, e)) in xi.iter_mut().zip(&mut eta).enumerate() {
            // sharp(p)=z^m conjugate(p), so reversed coefficients begin at 1.
            let shifted_first = index
                .checked_sub(1)
                .map_or(Complex64::new(0.0, 0.0), |i| at(&ex, i));
            let shifted_second = index
                .checked_sub(1)
                .map_or(Complex64::new(0.0, 0.0), |i| at(&xx, i));
            *x = finite(shifted_first.add(at(&xe, index)), "NLFT reconstruction")?;
            *e = finite(at(&ee, index).sub(shifted_second), "NLFT reconstruction")?;
        }
        Ok(InverseNode { xi, eta })
    }

    fn shared_savings(count: usize, transfer: bool) -> usize {
        if count == 1 {
            return 0;
        }
        let upper = count.div_ceil(2);
        let lower = count / 2;
        let transform = |size: usize| {
            let length = if size == 1 { 1 } else { 2 * size };
            8 * length * usize::try_from(length.ilog2().max(1)).unwrap()
        };
        shared_savings(upper, true)
            + shared_savings(lower, true)
            + 2 * transform(count.next_power_of_two())
            + if transfer {
                2 * transform(upper.next_power_of_two())
            } else {
                0
            }
    }
    #[test]
    fn reflections_only_root_matches_full_transfer_bits_and_shared_work() -> Result<()> {
        for count in [1, 2, 3, 5, 8, 17, 32, 65] {
            let (a, b) = fixture(count);
            let policy = Policy::default();
            let (actual, actual_work) = inverse(&a, &b, policy, ExecutionPolicy::Sequential)?;
            let mut expected = zeros(count, policy.limits)?;
            let mut work = Convolutions::new(
                payload_allowance(policy, count)?,
                ExecutionPolicy::Sequential,
            );
            let original_transfer = full_transfer_oracle(&a, &b, &mut expected, &mut work)?;
            let mut full_gamma = zeros(count, policy.limits)?;
            let mut shared = SharedConvolutions::new(
                payload_allowance(policy, count)?,
                ExecutionPolicy::Sequential,
            );
            let shared_transfer = inverse_node(&a, &b, &mut full_gamma, &mut shared, true)?
                .ok_or(Error::Target("fixture transfer"))?;
            assert_eq!(bits(&shared_transfer.xi), bits(&original_transfer.xi));
            assert_eq!(bits(&shared_transfer.eta), bits(&original_transfer.eta));
            assert_eq!(bits(&full_gamma), bits(&expected));
            assert_eq!(
                shared.work_used + shared_savings(count, true),
                work.work_used
            );
            assert_eq!(bits(&actual), bits(&expected), "count={count}");
            let omitted = if count == 1 {
                0
            } else {
                let size = count.div_ceil(2).next_power_of_two();
                4 * ConvolutionWorkspace::new(size, size, policy.backend, policy.limits)?
                    .resource_usage()
                    .work_units
            };
            assert_eq!(
                actual_work + omitted + shared_savings(count, false),
                work.work_used,
                "count={count}"
            );
            let mut limited = policy;
            limited.limits.max_work = actual_work;
            assert_eq!(
                bits(&inverse(&a, &b, limited, ExecutionPolicy::Sequential)?.0),
                bits(&actual)
            );
            if actual_work > 0 {
                limited.limits.max_work = actual_work - 1;
                assert!(matches!(
                    inverse(&a, &b, limited, ExecutionPolicy::Sequential),
                    Err(Error::Budget("convolution work"))
                ));
            }
        }
        Ok(())
    }
    #[test]
    fn shared_inverse_matches_each_available_backend_and_is_repeatable() -> Result<()> {
        for backend in [
            quest_numerics::FftBackend::Scalar,
            quest_numerics::FftBackend::Simd,
        ] {
            let policy = Policy {
                backend,
                ..Policy::default()
            };
            if matches!(
                ConvolutionWorkspace::new(1, 1, backend, policy.limits),
                Err(quest_numerics::Error::BackendUnavailable)
            ) {
                continue;
            }
            for count in [1, 3, 8, 17, 65, 128, 256] {
                let (a, b) = fixture(count);
                let actual = inverse(&a, &b, policy, ExecutionPolicy::Sequential)?;
                let repeated = inverse(&a, &b, policy, ExecutionPolicy::Sequential)?;
                assert_eq!(bits(&actual.0), bits(&repeated.0));
                assert_eq!(actual.1, repeated.1);
                let mut expected = zeros(count, policy.limits)?;
                full_transfer_oracle(
                    &a,
                    &b,
                    &mut expected,
                    &mut Convolutions::new(
                        payload_allowance(policy, count)?,
                        ExecutionPolicy::Sequential,
                    ),
                )?;
                assert_eq!(bits(&actual.0), bits(&expected));
            }
        }
        Ok(())
    }
    #[cfg(feature = "rayon")]
    #[test]
    fn shared_inverse_preserves_bits_and_admission_in_one_two_four_worker_pools()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let count = 1025;
        let mut a = vec![Complex64::new(0.0, 0.0); count];
        a[0] = Complex64::new(0.95, 0.0);
        let b = vec![Complex64::new(0.000_01, 0.000_002); count];
        let policy = Policy::default();
        let sequential = inverse(&a, &b, policy, ExecutionPolicy::Sequential)?;
        for workers in [1, 2, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()?;
            let execution = ExecutionPolicy::Rayon(&pool);
            let actual = inverse(&a, &b, policy, execution)?;
            let mut expected = zeros(count, policy.limits)?;
            full_transfer_oracle(
                &a,
                &b,
                &mut expected,
                &mut Convolutions::new(payload_allowance(policy, count)?, execution),
            )?;
            assert_eq!(bits(&actual.0), bits(&expected));
            assert_eq!(bits(&actual.0), bits(&sequential.0));
            assert_eq!(actual.1, sequential.1);
            let mut limited = policy;
            limited.limits.max_work = actual.1;
            assert_eq!(
                bits(&inverse(&a, &b, limited, execution)?.0),
                bits(&actual.0)
            );
            limited.limits.max_work -= 1;
            assert!(matches!(
                inverse(&a, &b, limited, execution),
                Err(Error::Budget("convolution work"))
            ));
        }
        Ok(())
    }
    #[test]
    fn compact_windows_become_outputs_without_new_vector_storage() -> Result<()> {
        let input = vec![Complex64::new(0.25, -0.0); 2];
        let mut work = SharedConvolutions::new(Policy::default(), ExecutionPolicy::Sequential);
        let windows = work.windows(
            [&input, &input, &input, &input],
            [&input, &input],
            [(0, 2); 4],
        )?;
        let midpoint_storage = [
            (windows[0].as_ptr(), windows[0].capacity()),
            (windows[2].as_ptr(), windows[2].capacity()),
        ];
        let (a, b) = midpoint_from_windows(windows)?;
        assert_eq!((a.as_ptr(), a.capacity()), midpoint_storage[0]);
        assert_eq!((b.as_ptr(), b.capacity()), midpoint_storage[1]);
        let windows = work.windows(
            [&input, &input, &input, &input],
            [&input, &input],
            [(0, 1), (0, 2), (0, 2), (0, 1)],
        )?;
        let transfer_storage = [
            (windows[1].as_ptr(), windows[1].capacity()),
            (windows[2].as_ptr(), windows[2].capacity()),
        ];
        let node = transfer_from_windows(windows)?;
        assert_eq!((node.xi.as_ptr(), node.xi.capacity()), transfer_storage[0]);
        assert_eq!(
            (node.eta.as_ptr(), node.eta.capacity()),
            transfer_storage[1]
        );
        Ok(())
    }
    #[test]
    fn root_storage_boundary_and_singleton_pivot_checks() -> Result<()> {
        let (a, b) = fixture(2);
        let mut policy = Policy::default();
        let plan =
            SharedConvolutionWorkspace::new(2, 2, policy.backend, policy.limits)?.resource_usage();
        policy.limits.max_bytes =
            64 * 2 * size_of::<Complex64>() + plan.buffer_bytes + plan.planner_bytes_estimate;
        inverse(&a, &b, policy, ExecutionPolicy::Sequential)?;
        policy.limits.max_bytes -= 1;
        assert!(inverse(&a, &b, policy, ExecutionPolicy::Sequential).is_err());
        assert!(matches!(
            inverse(
                &[Complex64::new(0.0, 0.0)],
                &[b[0]],
                Policy::default(),
                ExecutionPolicy::Sequential
            ),
            Err(Error::SingularPivot)
        ));
        assert!(matches!(
            inverse(
                &[a[0]],
                &[Complex64::new(f64::INFINITY, 0.0)],
                Policy::default(),
                ExecutionPolicy::Sequential
            ),
            Err(Error::NonFinite("inverse NLFT pivot"))
        ));
        Ok(())
    }
}
