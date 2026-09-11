use crate::{Complex64, Control, Error, Policy, Result, finite, zeros};
use quest_numerics::{
    ConvolutionWorkspace, ExecutionPolicy, FftDirection, FftWorkspace, Normalization,
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
        finite(*value, "reflection normalization")?;
        let scale = 1.0_f64.max(value.re.abs()).max(value.im.abs());
        let real = value.re / scale;
        let imag = value.im / scale;
        let one = 1.0 / scale;
        let norm = one.hypot(real).hypot(imag);
        let diagonal = Complex64::new(one / norm, 0.0);
        let off = Complex64::new(real / norm, imag / norm);
        output.push([[diagonal, off], [off.conj().neg(), diagonal]]);
    }
    Ok(output)
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

fn at(values: &[Complex64], index: usize) -> Complex64 {
    values
        .get(index)
        .copied()
        .unwrap_or(Complex64::new(0.0, 0.0))
}
fn reverse_conjugate(values: &[Complex64]) -> Vec<Complex64> {
    values.iter().rev().map(Complex64::conj).collect()
}

pub fn complete(
    target: &[Complex64],
    policy: Policy,
    execution: ExecutionPolicy<'_>,
) -> Result<(Vec<Complex64>, f64, usize)> {
    if target.is_empty() {
        return Err(Error::Target("empty target"));
    }
    let mut grid = target
        .len()
        .checked_mul(4)
        .and_then(usize::checked_next_power_of_two)
        .ok_or(Error::Budget("completion grid"))?
        .max(32);
    let mut last_residual = f64::INFINITY;
    let mut remaining = policy;
    while grid <= policy.max_completion_grid {
        let work = grid
            .checked_mul(
                usize::try_from(grid.ilog2().max(1)).map_err(|_| Error::Budget("FFT work"))?,
            )
            .and_then(|n| n.checked_mul(32))
            .ok_or(Error::Budget("FFT work"))?;
        let rest = remaining
            .limits
            .max_work
            .checked_sub(work)
            .ok_or(Error::Budget("Weiss work"))?;
        // Scope FFT plans and samples so they are gone before convolution plans
        // and residual payloads are allocated.
        let mut a_star = {
            let workspace = crate::workspace_policy(
                policy,
                grid.checked_add(target.len())
                    .ok_or(Error::Budget("Weiss payload"))?,
            )?;
            let mut values = zeros(grid, policy.limits)?;
            values
                .get_mut(..target.len())
                .ok_or(Error::Budget("completion grid"))?
                .copy_from_slice(target);
            let mut fft = FftWorkspace::new(grid, policy.backend, workspace.limits)?;
            fft.transform(&mut values, FftDirection::Inverse, Normalization::None)?;
            pointwise(&mut values, execution, |_, value| {
                let norm = value.re.hypot(value.im);
                let remainder = (-norm).mul_add(norm, 1.0);
                if remainder <= 0.0 || !remainder.is_finite() {
                    return Err(Error::Contractivity { upper: norm });
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
            a_star
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
        last_residual = residual;
        if last_residual <= policy.response_tolerance / 8.0 {
            return Ok((a_star, last_residual, grid));
        }
        grid = grid
            .checked_mul(2)
            .ok_or(Error::Budget("completion refinement"))?;
    }
    Err(Error::NotEstablished {
        stage: "Weiss completion",
        bound: last_residual,
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
    let mut work = Convolutions::new(policy, execution);
    inverse_node(a_star, b, &mut gamma, &mut work)?;
    Ok((gamma, work.work_used))
}

fn inverse_node(
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
    let upper = inverse_node(
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
    let lower = inverse_node(&midpoint_a, &midpoint_b, gamma_lower, work)?;
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
