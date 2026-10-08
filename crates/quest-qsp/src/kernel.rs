#[cfg(test)]
use crate::zeros;
use crate::{Complex64, Control, Error, Policy, Result, SynthesisAlgorithm, finite};
use quest_numerics::{
	Accounted, ConvolutionWorkspace, ExecutionPolicy, FftDirection, FftWorkspace, Normalization,
	OperationResources, SharedConvolutionWorkspace,
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

// Fixed-length internal vectors own their reservation. Mutations only replace entries.
struct Coefficients {
	values: Vec<Complex64>,
	reservation: quest_numerics::MemoryReservation,
}
impl std::ops::Deref for Coefficients {
	type Target = Vec<Complex64>;
	fn deref(&self) -> &Self::Target {
		&self.values
	}
}
impl std::ops::DerefMut for Coefficients {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.values
	}
}
fn coefficient_zeros(count: usize, resources: &OperationResources) -> Result<Coefficients> {
	let reservation = reserve_payload(resources, count, 1)?;
	let mut values = Vec::new();
	values.try_reserve_exact(count).map_err(|_| {
		resources.allocation_failed();
		Error::Numerics(quest_numerics::Error::Allocation)
	})?;
	values.resize(count, Complex64::new(0.0, 0.0));
	Ok(Coefficients {
		values,
		reservation,
	})
}
fn coefficient_clone(values: &[Complex64], resources: &OperationResources) -> Result<Coefficients> {
	let mut output = coefficient_zeros(values.len(), resources)?;
	output.copy_from_slice(values);
	Ok(output)
}
fn coefficient_reverse(
	values: &[Complex64],
	resources: &OperationResources,
) -> Result<Coefficients> {
	let mut output = coefficient_zeros(values.len(), resources)?;
	for (out, value) in output.iter_mut().zip(values.iter().rev()) {
		*out = value.conj();
	}
	Ok(output)
}
struct Convolutions<'pool> {
	execution: ExecutionPolicy<'pool>,
	plans: BTreeMap<usize, ConvolutionWorkspace>,
	policy: Policy,
	bytes: usize,
	work_used: usize,
	resources: OperationResources,
}
impl<'pool> Convolutions<'pool> {
	#[cfg(test)]
	fn new(policy: Policy, execution: ExecutionPolicy<'pool>) -> Self {
		Self::with_resources(
			policy,
			execution,
			OperationResources::from_limits(policy.limits),
		)
	}
	fn with_resources(
		mut policy: Policy,
		execution: ExecutionPolicy<'pool>,
		resources: OperationResources,
	) -> Self {
		policy.limits = resources.limits();
		Self {
			execution,
			plans: BTreeMap::new(),
			policy,
			bytes: 0,
			work_used: 0,
			resources,
		}
	}
	fn product(&mut self, left: &[Complex64], right: &[Complex64]) -> Result<Coefficients> {
		// Cache plans by maximum operand length. Every inverse node at the same
		// tree level reuses buffers; first/second halves remain ordered.
		let size = left
			.len()
			.max(right.len())
			.checked_next_power_of_two()
			.ok_or(Error::Budget("convolution support"))?;
		if !self.plans.contains_key(&size) {
			let capacity = size.min(self.policy.limits.shapes.max_coefficients);
			let plan = ConvolutionWorkspace::new_with_resources(
				capacity,
				capacity,
				self.policy.backend,
				&self.resources,
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
		let values = plan.convolve_with_policy(left, right, self.execution)?;
		let mut output = coefficient_zeros(values.len(), &self.resources)?;
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
	resources: OperationResources,
}
impl<'pool> SharedConvolutions<'pool> {
	#[cfg(test)]
	fn new(policy: Policy, execution: ExecutionPolicy<'pool>) -> Self {
		Self::with_resources(
			policy,
			execution,
			OperationResources::from_limits(policy.limits),
		)
	}
	fn with_resources(
		mut policy: Policy,
		execution: ExecutionPolicy<'pool>,
		resources: OperationResources,
	) -> Self {
		policy.limits = resources.limits();
		Self {
			execution,
			plans: BTreeMap::new(),
			policy,
			bytes: 0,
			work_used: 0,
			resources,
		}
	}
	fn windows(
		&mut self,
		left: [&[Complex64]; 4],
		right: [&[Complex64]; 2],
		windows: [(usize, usize); 4],
	) -> Result<[Coefficients; 4]> {
		let size = left
			.iter()
			.chain(right.iter())
			.map(|values| values.len())
			.max()
			.and_then(usize::checked_next_power_of_two)
			.ok_or(Error::Budget("convolution support"))?;
		if !self.plans.contains_key(&size) {
			let capacity = size.min(self.policy.limits.shapes.max_coefficients);
			let plan = SharedConvolutionWorkspace::new_with_resources(
				capacity,
				capacity,
				self.policy.backend,
				&self.resources,
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
		let mut output: [Option<Coefficients>; 4] = std::array::from_fn(|_| None);
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
			let values = session.product(left, index)?;
			let mut values_out = coefficient_zeros(count, &self.resources)?;
			for (index, value) in values_out.iter_mut().enumerate() {
				*value = at(
					values,
					offset
						.checked_add(index)
						.ok_or(Error::Budget("NLFT product window"))?,
				);
			}
			*out = Some(values_out);
		}
		let [a, b, c, d] = output;
		Ok([
			a.ok_or(Error::Budget("missing product"))?,
			b.ok_or(Error::Budget("missing product"))?,
			c.ok_or(Error::Budget("missing product"))?,
			d.ok_or(Error::Budget("missing product"))?,
		])
	}
}

fn at(values: &[Complex64], index: usize) -> Complex64 {
	values
		.get(index)
		.copied()
		.unwrap_or(Complex64::new(0.0, 0.0))
}

type CompletionResult = (Vec<Complex64>, CompletionData<Complex64>, f64, usize);
pub enum CompletionData<C> {
	InverseNlft,
	Rhw(Vec<C>),
}

#[expect(
	clippy::too_many_lines,
	reason = "Ordered completion retry stages share one ownership ledger"
)]
pub fn complete_with_resources(
	target: &[Complex64],
	policy: Policy,
	execution: ExecutionPolicy<'_>,
	resources: &OperationResources,
) -> Result<Accounted<CompletionResult>> {
	if target.is_empty() {
		return Err(Error::Target("empty target"));
	}
	let mut grid = target
		.len()
		.checked_mul(4)
		.and_then(usize::checked_next_power_of_two)
		.ok_or(Error::Budget("completion grid"))?
		.max(32);
	resources
		.completion(grid)
		.map_err(quest_numerics::Error::from)?;
	let mut last_residual = None;
	resources
		.coefficients(target.len())
		.map_err(quest_numerics::Error::from)?;
	let (transforms, grid_vectors, coefficient_vectors) = match policy.algorithm {
		SynthesisAlgorithm::InverseNlftDivideConquer => (4, 2, 1),
		SynthesisAlgorithm::RhwHalfCholesky => (5, 3, 2),
	};
	while grid <= policy.limits.shapes.max_completion_grid {
		// Scope FFT plans and samples so they are gone before convolution plans
		// and residual payloads are allocated.
		let (mut a_star, ratio, ratio_ownership) = {
			let _ = (transforms, grid_vectors, coefficient_vectors);
			let mut values = coefficient_zeros(grid, resources)?;
			values
				.get_mut(..target.len())
				.ok_or(Error::Budget("completion grid"))?
				.copy_from_slice(target);
			let mut fft =
				FftWorkspace::new_for_completion(grid, policy.backend, resources.clone())?;
			fft.transform(&mut values, FftDirection::Inverse, Normalization::None)?;
			let ratio_samples = match policy.algorithm {
				SynthesisAlgorithm::InverseNlftDivideConquer => None,
				SynthesisAlgorithm::RhwHalfCholesky => Some(coefficient_clone(&values, resources)?),
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
			let ratio_ownership = if ratio_samples.is_some() {
				Some(reserve_payload(resources, target.len(), 1)?)
			} else {
				None
			};
			let ratio = if let Some(mut samples) = ratio_samples {
				for (sample, exponent) in samples.iter_mut().zip(values.iter()) {
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
			let mut a_star = coefficient_zeros(target.len(), resources)?;
			for (index, value) in a_star.iter_mut().enumerate() {
				let slot = if index == 0 {
					0
				} else {
					grid.checked_sub(index)
						.ok_or(Error::Budget("completion support"))?
				};
				*value = at(&values, slot).conj();
			}
			(a_star, ratio, ratio_ownership)
		};
		// Positive real zero mode is the fixed outer-factor convention.
		a_star
			.first_mut()
			.ok_or(Error::Target("empty complement"))?
			.im = 0.0;
		let (residual, _) =
			completion_residual_with_resources(&a_star, target, policy, execution, resources)?;
		last_residual = Some(residual);
		if residual <= policy.accuracy.response_tolerance / 8.0 {
			let mut ownership = vec![a_star.reservation];
			ownership.extend(ratio_ownership);
			return Ok(Accounted::new_many(
				(a_star.values, ratio, residual, grid),
				ownership,
			));
		}
		grid = grid
			.checked_mul(2)
			.ok_or(Error::Budget("completion refinement"))?;
	}
	Err(Error::NotEstablished {
		stage: "Weiss completion",
		bound: last_residual.ok_or(Error::Budget("no completion grid admitted"))?,
		tolerance: policy.accuracy.response_tolerance / 8.0,
	})
}

fn completion_residual_with_resources(
	a_star: &[Complex64],
	b: &[Complex64],
	policy: Policy,
	execution: ExecutionPolicy<'_>,
	resources: &OperationResources,
) -> Result<(f64, usize)> {
	let mut convolutions = Convolutions::with_resources(policy, execution, resources.clone());
	let a_reverse = coefficient_reverse(a_star, resources)?;
	let a_product = convolutions.product(a_star, &a_reverse)?;
	drop(a_reverse);
	let b_reverse = coefficient_reverse(b, resources)?;
	let b_product = convolutions.product(b, &b_reverse)?;
	let middle = b
		.len()
		.checked_sub(1)
		.ok_or(Error::Target("empty residual"))?;
	let mut total = 0.0;
	for (index, (a, b)) in a_product.iter().zip(b_product.iter()).enumerate() {
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
	xi: Coefficients,
	eta: Coefficients,
}

fn midpoint_from_windows(windows: [Coefficients; 4]) -> Result<(Coefficients, Coefficients)> {
	let [mut a, xb, mut b, xa] = windows;
	// All products have succeeded. Reuse two compact windows as the outputs,
	// retaining the original per-index a-then-b validation order.
	for (index, (a, b)) in a.iter_mut().zip(b.iter_mut()).enumerate() {
		*a = finite((*a).add(at(&xb, index)), "NLFT midpoint")?;
		*b = finite((*b).sub(at(&xa, index)), "NLFT midpoint")?;
	}
	Ok((a, b))
}
fn transfer_from_windows(windows: [Coefficients; 4]) -> Result<InverseNode> {
	let [ex, mut xi, mut eta, xx] = windows;
	for (index, (x, e)) in xi.iter_mut().zip(eta.iter_mut()).enumerate() {
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

#[cfg(test)]
pub fn inverse(
	a_star: &[Complex64],
	b: &[Complex64],
	policy: Policy,
	execution: ExecutionPolicy<'_>,
) -> Result<(Vec<Complex64>, usize)> {
	let (result, used) = inverse_with_resources(
		a_star,
		b,
		policy,
		execution,
		&OperationResources::from_limits(policy.limits),
	)?;
	let (values, _ownership) = result.into_parts();
	Ok((values, used))
}
pub fn inverse_with_resources(
	a_star: &[Complex64],
	b: &[Complex64],
	policy: Policy,
	execution: ExecutionPolicy<'_>,
	resources: &OperationResources,
) -> Result<(Accounted<Vec<Complex64>>, usize)> {
	let before = resources.report().work_units;
	let mut workspace = InverseNlftWorkspace::new(policy.backend, resources.clone(), execution);
	let gamma = workspace.inverse(a_star, b)?;
	let used = resources
		.report()
		.work_units
		.checked_sub(before)
		.ok_or(Error::Budget("inverse work"))?;
	Ok((gamma, used))
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
		let mut xi_values = coefficient_zeros(1, &work.resources)?;
		let mut eta = coefficient_zeros(1, &work.resources)?;
		*xi_values
			.first_mut()
			.ok_or(Error::Target("empty transfer"))? = xi;
		*eta.first_mut().ok_or(Error::Target("empty transfer"))? = scale;
		return Ok(Some(InverseNode { xi: xi_values, eta }));
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
	let eta_conj = coefficient_reverse(&upper.eta, &work.resources)?;
	let xi_conj = coefficient_reverse(&upper.xi, &work.resources)?;
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

pub fn response_residual_with_resources(
	controls: &[Control],
	target: &[Complex64],
	policy: Policy,
	execution: ExecutionPolicy<'_>,
	resources: OperationResources,
) -> Result<f64> {
	let coefficients = product_tree(
		controls,
		&mut Convolutions::with_resources(policy, execution, resources),
	)?;
	let mut residual = 0.0;
	for (index, value) in coefficients.aa.iter().enumerate() {
		residual += value.sub(at(target, index)).norm();
	}
	if !residual.is_finite() {
		return Err(Error::NonFinite("control reconstruction"));
	}
	if residual > policy.accuracy.response_tolerance {
		return Err(Error::NotEstablished {
			stage: "binary64 reconstruction",
			bound: residual,
			tolerance: policy.accuracy.response_tolerance,
		});
	}
	Ok(residual)
}

struct PolynomialMatrix {
	aa: Coefficients,
	ab: Coefficients,
	ba: Coefficients,
	bb: Coefficients,
}
fn product_tree(controls: &[Control], work: &mut Convolutions<'_>) -> Result<PolynomialMatrix> {
	if controls.len() == 1 {
		let [[a, b], [c, d]] = *controls
			.first()
			.ok_or(Error::Target("empty control product"))?;
		let make = |value| -> Result<Coefficients> {
			let mut output = coefficient_zeros(1, &work.resources)?;
			*output.first_mut().ok_or(Error::Target("empty product"))? = value;
			Ok(output)
		};
		return Ok(PolynomialMatrix {
			aa: make(a)?,
			ab: make(b)?,
			ba: make(c)?,
			bb: make(d)?,
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
) -> Result<Coefficients> {
	let first = work.product(a, b)?;
	let second = work.product(c, d)?;
	let mut result = coefficient_zeros(
		first
			.len()
			.checked_add(1)
			.ok_or(Error::Budget("control support"))?,
		&work.resources,
	)?;
	for (index, value) in result.iter_mut().enumerate() {
		let shifted = index
			.checked_sub(1)
			.map_or(Complex64::new(0.0, 0.0), |i| at(&first, i));
		*value = shifted.add(at(&second, index));
	}
	Ok(result)
}

#[cfg(test)]
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
	// Sibling allocation/planning order is fixed before FFT parallel dispatch.
	// Reuse one cache instead of retaining extra tree-worker caches.
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
				for (a, b) in actual.iter().zip(expected.iter()) {
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
			policy.limits.resources.max_work_units = 100;
			let mut work = Convolutions::new(policy, ExecutionPolicy::Rayon(&pool));
			expect_true!(matches!(
				children(&left, &right, &mut work),
				Err(Error::Numerics(quest_numerics::Error::Resource(_)))
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
#[cfg(test)]
pub fn half_cholesky(c: &[Complex64], policy: Policy) -> Result<(Vec<Complex64>, usize)> {
	let (result, used) =
		half_cholesky_with_resources(c, policy, &OperationResources::from_limits(policy.limits))?;
	let (values, _ownership) = result.into_parts();
	Ok((values, used))
}
#[expect(
	clippy::many_single_char_names,
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	reason = "Validated nonempty n and 0 <= k < j < n in the unchanged displacement recurrence"
)]
pub fn half_cholesky_with_resources(
	c: &[Complex64],
	_policy: Policy,
	resources: &OperationResources,
) -> Result<(Accounted<Vec<Complex64>>, usize)> {
	let n = c.len();
	if n == 0 {
		return Err(Error::Target("empty Weiss ratio"));
	}
	let work = n
		.checked_mul(n)
		.and_then(|v| v.checked_mul(32))
		.ok_or(Error::Budget("Half-Cholesky work"))?;
	resources
		.charge_work(work)
		.map_err(quest_numerics::Error::from)?;
	let mut first = coefficient_zeros(n, resources)?;
	first[0] = Complex64::new(1.0, 0.0);
	let mut second = coefficient_reverse(c, resources)?;
	let mut solution = coefficient_clone(&second, resources)?;
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
	let output = coefficient_reverse(&solution, resources)?;
	Ok((Accounted::new(output.values, output.reservation), work))
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
		policy.limits.resources.max_work_units = 17 * 17 * 32 - 1;
		assert!(matches!(
			half_cholesky(&ratio, policy),
			Err(Error::Numerics(quest_numerics::Error::Resource(_)))
		));
		policy = Policy::default();
		policy.limits.resources.max_peak_bytes = 4 * 17 * size_of::<Complex64>() - 1;
		assert!(matches!(
			half_cholesky(&ratio, policy),
			Err(Error::Numerics(quest_numerics::Error::Resource(_)))
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
				xi: coefficient_clone(&[xi], &work.resources)?,
				eta: coefficient_clone(&[scale], &work.resources)?,
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
		let eta_conj = coefficient_reverse(&upper.eta, &work.resources)?;
		let xi_conj = coefficient_reverse(&upper.xi, &work.resources)?;
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
		for (index, (x, e)) in xi.iter_mut().zip(eta.iter_mut()).enumerate() {
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
		Ok(InverseNode {
			xi: coefficient_clone(&xi, &work.resources)?,
			eta: coefficient_clone(&eta, &work.resources)?,
		})
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
				actual_work
					.checked_sub(count)
					.ok_or(Error::Budget("test leaf work"))?
					+ omitted
					+ shared_savings(count, false),
				work.work_used,
				"count={count}"
			);
			let mut limited = policy;
			limited.limits.resources.max_work_units = actual_work;
			assert_eq!(
				bits(&inverse(&a, &b, limited, ExecutionPolicy::Sequential)?.0),
				bits(&actual)
			);
			if actual_work > 0 {
				limited.limits.resources.max_work_units = actual_work - 1;
				assert!(matches!(
					inverse(&a, &b, limited, ExecutionPolicy::Sequential),
					Err(Error::Numerics(quest_numerics::Error::Resource(_)))
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
			limited.limits.resources.max_work_units = actual.1;
			assert_eq!(
				bits(&inverse(&a, &b, limited, execution)?.0),
				bits(&actual.0)
			);
			limited.limits.resources.max_work_units -= 1;
			assert!(matches!(
				inverse(&a, &b, limited, execution),
				Err(Error::Numerics(quest_numerics::Error::Resource(_)))
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
		policy.limits.resources.max_peak_bytes =
			10 * size_of::<Complex64>() + plan.buffer_bytes + plan.planner_bytes_estimate;
		inverse(&a, &b, policy, ExecutionPolicy::Sequential)?;
		policy.limits.resources.max_peak_bytes -= 1;
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
			Err(Error::NonFinite("inverse NLFT input"))
		));
		Ok(())
	}
}

/// Prepared nonlinear Fourier scattering data in ascending stored powers.
#[derive(Debug, Clone)]
pub struct ScatteringPair {
	/// Conjugate complement. Physical a is reversed and conjugated.
	pub conjugate_complement: Vec<Complex64>,
	/// Physical b on support [0, degree].
	pub target: Vec<Complex64>,
}
fn reserve_payload(
	resources: &OperationResources,
	count: usize,
	copies: usize,
) -> Result<quest_numerics::MemoryReservation> {
	let bytes = count
		.checked_mul(copies)
		.and_then(|n| n.checked_mul(size_of::<Complex64>()))
		.ok_or(Error::Budget("recursive payload storage"))?;
	Ok(resources
		.reserve(bytes, 0)
		.map_err(quest_numerics::Error::from)?)
}
/// Inverse plans and scratch remain owned, charged and reused between calls.
pub struct InverseNlftWorkspace<'pool> {
	work: SharedConvolutions<'pool>,
}
impl<'pool> InverseNlftWorkspace<'pool> {
	/// Number of retained convolution plans, unchanged during warm reuse.
	#[must_use]
	pub fn cached_plan_count(&self) -> usize {
		self.work.plans.len()
	}
	/// Exact numerical coefficient/scratch capacities retained by the cache.
	/// # Errors
	/// Rejects accounting overflow.
	pub fn cached_buffer_bytes(&self) -> Result<usize> {
		self.work.plans.values().try_fold(0_usize, |sum, plan| {
			sum.checked_add(plan.resource_usage().buffer_bytes)
				.ok_or(Error::Budget("cached buffers"))
		})
	}

	#[must_use]
	pub fn new(
		backend: crate::FftBackend,
		resources: OperationResources,
		execution: ExecutionPolicy<'pool>,
	) -> Self {
		let policy = Policy {
			backend,
			limits: resources.limits(),
			..Policy::default()
		};
		Self {
			work: SharedConvolutions::with_resources(policy, execution, resources),
		}
	}
	/// Invert physical scattering arrays in ascending exponent order.
	/// Physical a has support [-degree, 0], b has support [0, degree].
	/// Owned reverse/conjugate preparation is included and charged here.
	/// # Errors
	/// Rejects support/finite/pivot failures or cumulative resource exhaustion.
	pub fn inverse_physical(
		&mut self,
		a: &[Complex64],
		b: &[Complex64],
	) -> Result<Accounted<Vec<Complex64>>> {
		if a.len() != b.len() || b.is_empty() {
			return Err(Error::Target("inverse NLFT pair support"));
		}
		self.work
			.resources
			.coefficients(a.len())
			.map_err(quest_numerics::Error::from)?;
		self.work
			.resources
			.charge_work(a.len())
			.map_err(quest_numerics::Error::from)?;
		let complement = coefficient_reverse(a, &self.work.resources)?;
		self.inverse(&complement, b)
	}
	/// Invert one canonical pair; output ownership remains charged until drop.
	/// # Errors
	/// Rejects shape, nonfinite, singular pivots or cumulative resource exhaustion.
	pub fn inverse(
		&mut self,
		a_star: &[Complex64],
		b: &[Complex64],
	) -> Result<Accounted<Vec<Complex64>>> {
		if a_star.len() != b.len() || b.is_empty() {
			return Err(Error::Target("inverse NLFT pair support"));
		}
		self.work
			.resources
			.coefficients(b.len())
			.map_err(quest_numerics::Error::from)?;
		for value in a_star.iter().chain(b) {
			finite(*value, "inverse NLFT input")?;
		}

		self.work
			.resources
			.charge_work(b.len())
			.map_err(quest_numerics::Error::from)?;
		let mut gamma = coefficient_zeros(b.len(), &self.work.resources)?;
		inverse_node(a_star, b, &mut gamma, &mut self.work, false)?;
		Ok(Accounted::new(gamma.values, gamma.reservation))
	}
	#[must_use]
	pub fn resource_report(&self) -> quest_numerics::ResourceReport {
		self.work.resources.report()
	}
}
/// Forward plans and scratch remain owned, charged and reused between calls.
pub struct ForwardNlftWorkspace<'pool> {
	work: Convolutions<'pool>,
}
impl<'pool> ForwardNlftWorkspace<'pool> {
	/// Number of retained convolution plans, unchanged during warm reuse.
	#[must_use]
	pub fn cached_plan_count(&self) -> usize {
		self.work.plans.len()
	}
	/// Exact numerical coefficient/scratch capacities retained by the cache.
	/// # Errors
	/// Rejects accounting overflow.
	pub fn cached_buffer_bytes(&self) -> Result<usize> {
		self.work.plans.values().try_fold(0_usize, |sum, plan| {
			sum.checked_add(plan.resource_usage().buffer_bytes)
				.ok_or(Error::Budget("cached buffers"))
		})
	}

	#[must_use]
	pub fn new(
		backend: crate::FftBackend,
		resources: OperationResources,
		execution: ExecutionPolicy<'pool>,
	) -> Self {
		let policy = Policy {
			backend,
			limits: resources.limits(),
			..Policy::default()
		};
		Self {
			work: Convolutions::with_resources(policy, execution, resources),
		}
	}
	/// Normalise reflections and reconstruct both canonical output polynomials.
	/// # Errors
	/// Rejects empty/nonfinite inputs or cumulative resource exhaustion.
	pub fn forward(&mut self, reflections: &[Complex64]) -> Result<Accounted<ScatteringPair>> {
		if reflections.is_empty() {
			return Err(Error::Target("empty reflection sequence"));
		}
		self.work
			.resources
			.coefficients(reflections.len())
			.map_err(quest_numerics::Error::from)?;
		let _controls = self
			.work
			.resources
			.reserve(
				reflections
					.len()
					.checked_mul(size_of::<Control>())
					.ok_or(Error::Budget("control storage"))?,
				0,
			)
			.map_err(quest_numerics::Error::from)?;
		self.work
			.resources
			.charge_work(reflections.len())
			.map_err(quest_numerics::Error::from)?;
		let controls = controls(reflections)?;
		let matrix = product_tree(&controls, &mut self.work)?;
		let mut complement = matrix.aa;
		complement.values.reverse();
		for value in &mut complement.values {
			*value = value.conj();
		}
		Ok(Accounted::new_many(
			ScatteringPair {
				conjugate_complement: complement.values,
				target: matrix.ab.values,
			},
			vec![complement.reservation, matrix.ab.reservation],
		))
	}
	#[must_use]
	pub fn resource_report(&self) -> quest_numerics::ResourceReport {
		self.work.resources.report()
	}
}

#[cfg(test)]
#[allow(
	clippy::panic_in_result_fn,
	reason = "Regression test asserts allocator rejection and RAII release"
)]
mod owned_allocation_tests {
	#[test]
	fn rejected_vec_capacity_releases_its_reservation_and_records_allocation_failure()
	-> super::Result<()> {
		let limits = quest_numerics::OperationLimits {
			shapes: quest_numerics::ShapeLimits {
				max_coefficients: usize::MAX,
				max_fft_len: usize::MAX,
				max_completion_grid: usize::MAX,
			},
			resources: quest_numerics::ResourceLimits {
				max_peak_bytes: usize::MAX,
				max_work_units: usize::MAX,
			},
		};
		let resources = quest_numerics::OperationResources::from_limits(limits);
		let count = usize::try_from(isize::MAX)
			.map_err(|_| super::Error::Budget("test capacity"))?
			.checked_div(size_of::<super::Complex64>())
			.and_then(|n| n.checked_add(1))
			.ok_or(super::Error::Budget("test capacity"))?;
		assert!(matches!(
			super::coefficient_zeros(count, &resources),
			Err(super::Error::Numerics(quest_numerics::Error::Allocation))
		));
		assert_eq!(resources.report().live_bytes, 0);
		assert_eq!(
			resources.report().last_rejection,
			Some(quest_numerics::ResourceError::Allocation)
		);
		Ok(())
	}
}
