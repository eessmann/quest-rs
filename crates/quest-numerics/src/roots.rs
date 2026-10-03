#![allow(
	clippy::needless_pass_by_value,
	reason = "Contractors own input enclosures and returned branches"
)]
//! Root-preserving contractors.
//!
//! All results are conditional on the callback
//! enclosing one continuously differentiable function and its derivative on
//! every requested box. Arithmetic admission does not prove this premise.
use crate::arithmetic::{ArithmeticError, EnclosureBackend, First};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RootEvidence {
	pub at_most_one: bool,
	pub exists: bool,
}
impl RootEvidence {
	#[must_use]
	pub const fn unique(self) -> bool {
		self.at_most_one && self.exists
	}
}
/// Explicit mathematical premise for supplied value/derivative callbacks.
///
/// Arithmetic admission alone does not establish this claim: all evaluations
/// must enclose the same continuously differentiable function and its derivative
/// (Jacobian in the vector case) throughout every requested box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Premise {
	EnclosesContinuouslyDifferentiableFunction,
}
#[derive(Clone, Debug)]
pub struct Contraction<T> {
	pub images: Vec<T>,
	pub evidence: RootEvidence,
	pub premise: Premise,
}
#[derive(Clone, Copy, Debug)]
pub struct CoverLimits {
	pub max_iterations: usize,
	pub max_boxes: usize,
	pub max_bytes: usize,
}
impl Default for CoverLimits {
	fn default() -> Self {
		Self {
			max_iterations: 10000,
			max_boxes: 10000,
			max_bytes: 64 * 1024 * 1024,
		}
	}
}
#[derive(Clone, Debug)]
pub struct RootBox<T> {
	/// True when the function enclosure proves it vanishes throughout a non-singleton box.
	pub continuum: bool,
	pub interval: T,
	pub evidence: RootEvidence,
}
#[derive(Clone, Debug)]
pub struct RootCover<T, E = ArithmeticError> {
	pub failure: Option<E>,
	pub covered: Vec<RootBox<T>>,
	pub excluded: Vec<T>,
	pub unresolved: Vec<T>,
	pub iterations: usize,
	pub premise: Premise,
}
impl<T, E> RootCover<T, E> {
	pub const fn complete(&self) -> bool {
		self.failure.is_none() && self.unresolved.is_empty()
	}
}
const fn result<T>(images: Vec<T>, evidence: RootEvidence, premise: Premise) -> Contraction<T> {
	Contraction {
		images,
		evidence,
		premise,
	}
}
fn checked_center<B: EnclosureBackend>(
	b: &mut B,
	x: &B::Scalar,
	c: &B::Scalar,
) -> Result<(), B::Error> {
	let lo = b.lower(c)?;
	let hi = b.upper(c)?;
	if !b.same(&lo, &hi)? {
		return Err(ArithmeticError::Domain("non-singleton center").into());
	}
	let intersection = b.intersection(x, c)?;
	if intersection.is_none() {
		return Err(ArithmeticError::Domain("center outside interval").into());
	}
	Ok(())
}
/// Extended quotient intersected with a finite domain. A denominator spanning
/// zero retains both disjoint branches. Zero divided by zero retains the domain.
fn extended_divide<B: EnclosureBackend>(
	b: &mut B,
	mut n: B::Scalar,
	mut d: B::Scalar,
	domain: B::Scalar,
) -> Result<Vec<B::Scalar>, B::Error> {
	if !b.contains_zero(&d)? {
		let q = b.div(n, d)?;
		return Ok(b.intersection(&q, &domain)?.into_iter().collect());
	}
	if b.contains_zero(&n)? {
		return Ok(vec![domain]);
	}
	if !b.nonnegative(&n)? {
		n = b.neg(n)?;
		d = b.neg(d)?;
	}
	let mut out = Vec::with_capacity(2);
	let dl = b.lower(&d)?;
	let du = b.upper(&d)?;
	let nl = b.lower(&n)?;
	if !b.nonnegative(&dl)? {
		let q = b.div(nl.clone(), dl)?;
		let bound = b.upper(&q)?;
		let lower = b.lower(&domain)?;
		// If bound is below the domain, the hull would falsely add a branch.
		let delta = b.sub(bound.clone(), lower.clone())?;
		if b.nonnegative(&delta)? {
			let ray = b.hull(&lower, &bound)?;
			if let Some(x) = b.intersection(&ray, &domain)? {
				out.push(x);
			}
		}
	}
	if !b.is_zero(&du)? && b.nonnegative(&du)? {
		let q = b.div(nl, du)?;
		let bound = b.lower(&q)?;
		let upper = b.upper(&domain)?;
		let delta = b.sub(upper.clone(), bound.clone())?;
		if b.nonnegative(&delta)? {
			let ray = b.hull(&bound, &upper)?;
			if let Some(x) = b.intersection(&ray, &domain)? {
				out.push(x);
			}
		}
	}
	Ok(out)
}
fn newton_values<B: EnclosureBackend>(
	b: &mut B,
	x: B::Scalar,
	c: B::Scalar,
	value: B::Scalar,
	derivative: B::Scalar,
	premise: Premise,
) -> Result<Contraction<B::Scalar>, B::Error> {
	let at_most_one = !b.contains_zero(&derivative)?;
	let exists_at_center = b.is_zero(&value)?;
	let displacement = b.sub(x.clone(), c.clone())?;
	let negative = b.neg(value)?;
	let branches = extended_divide(b, negative, derivative, displacement)?;
	let mut images = Vec::with_capacity(branches.len());
	let mut strict = false;
	for y in branches {
		let image = b.add(y, c.clone())?;
		strict |= b.strict_subset(&image, &x)?;
		if let Some(y) = b.intersection(&image, &x)? {
			images.push(y);
		}
	}
	let exists = !images.is_empty() && (exists_at_center || (at_most_one && strict));
	Ok(result(
		images,
		RootEvidence {
			at_most_one,
			exists,
		},
		premise,
	))
}
/// # Errors
/// Rejects invalid centers or propagates backend and callback failures.
pub fn newton<B: EnclosureBackend, F>(
	b: &mut B,
	x: B::Scalar,
	c: B::Scalar,
	mut f: F,
	premise: Premise,
) -> Result<Contraction<B::Scalar>, B::Error>
where
	F: FnMut(&mut B, &B::Scalar) -> Result<First<B::Scalar>, B::Error>,
{
	checked_center(b, &x, &c)?;
	let value = f(b, &c)?.value;
	let derivative = f(b, &x)?.first;
	newton_values(b, x, c, value, derivative, premise)
}
/// # Errors
/// Rejects invalid centers or propagates backend and callback failures.
pub fn hansen_sengupta<B: EnclosureBackend, F>(
	b: &mut B,
	x: B::Scalar,
	c: B::Scalar,
	mut f: F,
	preconditioner: B::Scalar,
	premise: Premise,
) -> Result<Contraction<B::Scalar>, B::Error>
where
	F: FnMut(&mut B, &B::Scalar) -> Result<First<B::Scalar>, B::Error>,
{
	checked_center(b, &x, &c)?;
	let value = f(b, &c)?.value;
	let derivative = f(b, &x)?.first;
	if b.contains_zero(&preconditioner)? {
		return Ok(result(vec![x], RootEvidence::default(), premise));
	}
	let value = b.mul(preconditioner.clone(), value)?;
	let derivative = b.mul(preconditioner, derivative)?;
	newton_values(b, x, c, value, derivative, premise)
}
/// # Errors
/// Rejects invalid centers or propagates backend and callback failures.
pub fn krawczyk<B: EnclosureBackend, F>(
	b: &mut B,
	x: B::Scalar,
	c: B::Scalar,
	mut f: F,
	preconditioner: B::Scalar,
	premise: Premise,
) -> Result<Contraction<B::Scalar>, B::Error>
where
	F: FnMut(&mut B, &B::Scalar) -> Result<First<B::Scalar>, B::Error>,
{
	checked_center(b, &x, &c)?;
	let value = f(b, &c)?.value;
	let derivative = f(b, &x)?.first;
	let exact_root = b.is_zero(&value)?;
	let product = b.mul(preconditioner.clone(), derivative)?;
	let one = b.point(1.0)?;
	let slope = b.sub(one, product)?;
	let at_most_one = !b.contains_zero(&preconditioner)? && b.magnitude_lt_one(&slope)?;
	let correction = b.mul(preconditioner, value)?;
	let base = b.sub(c.clone(), correction)?;
	let displacement = b.sub(x.clone(), c)?;
	let remainder = b.mul(slope, displacement)?;
	let image = b.add(base, remainder)?;
	let exists = exact_root || (at_most_one && b.strict_subset(&image, &x)?);
	let images = b.intersection(&image, &x)?.into_iter().collect();
	Ok(result(
		images,
		RootEvidence {
			at_most_one,
			exists,
		},
		premise,
	))
}
/// Deterministic depth-first cover.
///
/// A separately supplied callback requires an explicit premise:
/// ```compile_fail
/// use quest_numerics::{Interval, arithmetic::{First, Interval64Backend}, roots::{cover, CoverLimits}};
/// let zero = Interval::point(0.0).unwrap();
/// let _ = cover(&mut Interval64Backend, zero,
///     |_, x| Ok(First { value: *x, first: zero }), CoverLimits::default(), &1e-6);
/// ```
///
/// Budgets and resolution stalls retain every
/// remaining candidate box. Covered boxes may contain no root: evidence flags
/// distinguish coverage from existence and uniqueness. A proven zero continuum
/// is retained as one covered box regardless of its width. Storage admission includes scalar-owned limbs and
/// retained boxes plus a conservative local scratch allowance. Callback memory
/// and opaque upstream transcendental caches remain backend/caller resources.
/// # Errors
/// Fails only if the minimal recovery buffer cannot be allocated. Backend,
/// input-admission, metadata, and later allocation failures are retained in
/// `failure`, with the current parent and all pending boxes unresolved.
#[allow(
	clippy::too_many_lines,
	reason = "Atomic cover steps retain the parent on failure and commit coverage together"
)]
pub fn cover<B: EnclosureBackend, F>(
	b: &mut B,
	input: B::Scalar,
	mut function: F,
	limits: CoverLimits,
	tolerance: &B::Endpoint,
	premise: Premise,
) -> Result<RootCover<B::Scalar, B::Error>, B::Error>
where
	F: FnMut(&mut B, &B::Scalar) -> Result<First<B::Scalar>, B::Error>,
{
	let mut result = RootCover {
		failure: None,
		covered: Vec::new(),
		excluded: Vec::new(),
		unresolved: Vec::new(),
		iterations: 0,
		premise,
	};
	// Keep one recovery slot even if the main workspace cannot be admitted.
	result
		.unresolved
		.try_reserve_exact(1)
		.map_err(|_| ArithmeticError::Budget("root recovery allocation"))?;
	let admission = (|| -> Result<_, B::Error> {
		b.validate(&input)?;
		b.width_le(&input, tolerance)?;
		Ok(b.storage_bytes(&input)?.max(b.working_scalar_bytes()))
	})();
	let scratch_scalar = match admission {
		Ok(bytes) => bytes,
		Err(error) => {
			result.failure = Some(error);
			result.unresolved.push(input);
			return Ok(result);
		}
	};
	let scratch_bytes = scratch_scalar.saturating_mul(32);
	let capacity = limits.max_boxes.min(
		limits
			.max_bytes
			.saturating_sub(scratch_bytes)
			.checked_div(std::mem::size_of::<B::Scalar>().max(1))
			.unwrap_or(0)
			.checked_div(4)
			.unwrap_or(0),
	);
	if capacity == 0 {
		result.unresolved.push(input);
		return Ok(result);
	}
	let mut pending = Vec::new();
	if pending.try_reserve_exact(1).is_err() {
		result.failure = Some(ArithmeticError::Budget("root allocation").into());
		result.unresolved.push(input);
		return Ok(result);
	}
	pending.push(input);
	while let Some(x) = pending.pop() {
		if result.iterations >= limits.max_iterations {
			retain_parent(&mut result, pending, x);
			break;
		}
		let admission = (|| -> Result<_, B::Error> {
			let retained = retained_bytes(b, &pending, &result)?;
			Ok(retained.saturating_add(
				b.storage_bytes(&x)?
					.max(b.working_scalar_bytes())
					.saturating_mul(32),
			) <= limits.max_bytes)
		})();
		match admission {
			Ok(true) => {}
			Ok(false) => {
				retain_parent(&mut result, pending, x);
				break;
			}
			Err(error) => {
				result.failure = Some(error);
				retain_parent(&mut result, pending, x);
				break;
			}
		}
		result.iterations = result.iterations.saturating_add(1);
		// Commit a step only after all its arithmetic succeeded. On failure the
		// original parent remains an enclosure of all branches from this step.
		let step = (|| -> Result<_, B::Error> {
			let mut covered = Vec::new();
			let mut unresolved = Vec::new();
			let mut children = Vec::new();
			let evaluation = function(b, &x)?;
			if !b.contains_zero(&evaluation.value)? {
				return Ok((covered, unresolved, children, true));
			}
			if b.is_zero(&evaluation.value)? {
				let lower = b.lower(&x)?;
				let upper = b.upper(&x)?;
				let continuum = !b.same(&lower, &upper)?;
				let evidence = RootEvidence {
					exists: true,
					at_most_one: !b.contains_zero(&evaluation.first)?,
				};
				covered.push(RootBox {
					interval: x.clone(),
					evidence,
					continuum,
				});
				return Ok((covered, unresolved, children, false));
			}
			let center = b.midpoint(&x)?;
			let value = function(b, &center)?.value;
			let contraction =
				newton_values(b, x.clone(), center, value, evaluation.first, premise)?;
			if contraction.images.is_empty() {
				return Ok((covered, unresolved, children, true));
			}
			for image in contraction.images {
				if b.width_le(&image, tolerance)? {
					let mut evidence = contraction.evidence;
					let lo = b.lower(&image)?;
					let hi = b.upper(&image)?;
					if !evidence.exists && b.same(&lo, &hi)? {
						let at_image = function(b, &image)?.value;
						evidence.exists = b.is_zero(&at_image)?;
					}
					covered.push(RootBox {
						interval: image,
						evidence,
						continuum: false,
					});
					continue;
				}
				if !b.same(&image, &x)? {
					children.push(image);
				} else if let Some((left, right)) = b.bisect(&image)? {
					children.push(left);
					children.push(right);
				} else {
					unresolved.push(image);
				}
			}
			Ok((covered, unresolved, children, false))
		})();
		let (covered, unresolved, children, excluded) = match step {
			Ok(step) => step,
			Err(error) => {
				result.failure = Some(error);
				retain_parent(&mut result, pending, x);
				break;
			}
		};
		let admission = (|| -> Result<_, B::Error> {
			let mut additional = 0_usize;
			for value in covered
				.iter()
				.map(|v| &v.interval)
				.chain(unresolved.iter())
				.chain(children.iter())
			{
				additional = additional
					.saturating_add(b.storage_bytes(value)?)
					.saturating_add(std::mem::size_of::<RootBox<B::Scalar>>());
			}
			// After a later pop, recovery must fit that parent plus all prior
			// unresolved boxes without allocating. Grow only for admitted work.
			let recovery_slots = pending
				.len()
				.saturating_add(children.len())
				.saturating_add(result.unresolved.len())
				.saturating_add(unresolved.len());
			let pending_growth = if recovery_slots > pending.capacity() {
				recovery_slots.saturating_mul(std::mem::size_of::<B::Scalar>())
			} else {
				0
			};
			let retained = retained_bytes(b, &pending, &result)?;
			if retained
				.saturating_add(pending_growth)
				.saturating_add(additional.saturating_mul(2))
				.saturating_add(scratch_bytes)
				> limits.max_bytes
			{
				return Ok(false);
			}
			let stored = pending
				.len()
				.saturating_add(result.covered.len())
				.saturating_add(result.excluded.len())
				.saturating_add(result.unresolved.len());
			let proposed = covered
				.len()
				.saturating_add(unresolved.len())
				.saturating_add(children.len())
				.saturating_add(usize::from(excluded));
			if stored.saturating_add(proposed) > capacity {
				return Ok(false);
			}
			pending
				.try_reserve_exact(recovery_slots.saturating_sub(pending.len()))
				.map_err(|_| ArithmeticError::Budget("root allocation"))?;
			result
				.covered
				.try_reserve_exact(covered.len())
				.map_err(|_| ArithmeticError::Budget("root allocation"))?;
			result
				.unresolved
				.try_reserve_exact(unresolved.len())
				.map_err(|_| ArithmeticError::Budget("root allocation"))?;
			if excluded {
				result
					.excluded
					.try_reserve_exact(1)
					.map_err(|_| ArithmeticError::Budget("root allocation"))?;
			}
			// Recheck actual capacities: allocation implementations may provide
			// more than requested. Never commit a step beyond the byte budget.
			let retained = retained_bytes(b, &pending, &result)?;
			Ok(retained
				.saturating_add(additional)
				.saturating_add(scratch_bytes)
				<= limits.max_bytes)
		})();
		match admission {
			Ok(true) => {}
			Ok(false) => {
				retain_parent(&mut result, pending, x);
				break;
			}
			Err(error) => {
				result.failure = Some(error);
				retain_parent(&mut result, pending, x);
				break;
			}
		}
		result.covered.extend(covered);
		result.unresolved.extend(unresolved);
		if excluded {
			result.excluded.push(x);
		}
		for child in children.into_iter().rev() {
			pending.push(child);
		}
	}
	Ok(result)
}
/// Function value and a heap-owned statically shaped Jacobian.
#[derive(Clone, Debug)]
pub struct VectorEvaluation<T, const N: usize> {
	pub value: [T; N],
	pub jacobian: crate::shapes::Matrix<T, N, N>,
}
/// Sequential interval Gauss--Seidel sweep on the preconditioned Jacobian.
///
///
/// Splits are retained; branch admission returns the original box. The separate
/// Krawczyk norm and inclusion gates establish at-most-one and existence.
/// # Errors
/// Rejects shape/accounting overflow or propagates backend and callback errors.
#[allow(
	clippy::indexing_slicing,
	clippy::needless_range_loop,
	reason = "All array accesses use loop bounds 0..N on statically sized arrays"
)]
#[allow(
	clippy::too_many_lines,
	reason = "One root-preserving sweep with a separate inclusion gate"
)]
pub fn vector_hansen_sengupta<B: EnclosureBackend, F, const N: usize>(
	b: &mut B,
	input: [B::Scalar; N],
	center: [B::Scalar; N],
	mut function: F,
	preconditioner: &crate::shapes::Matrix<B::Scalar, N, N>,
	limits: CoverLimits,
	premise: Premise,
) -> Result<Contraction<[B::Scalar; N]>, B::Error>
where
	F: FnMut(&mut B, &[B::Scalar; N]) -> Result<VectorEvaluation<B::Scalar, N>, B::Error>,
{
	const { assert!(N > 0, "zero root dimension") };
	// Preconditioner, retained source Jacobian, and preconditioned matrix can
	// coexist. Include static I/O, RHS/displacement, both branch buffers, and
	// scalar scratch. Callback-internal temporaries are caller-owned.
	let cells = N
		.checked_mul(N)
		.and_then(|v| v.checked_mul(3))
		.and_then(|v| {
			N.checked_mul(limits.max_boxes)?
				.checked_mul(2)?
				.checked_add(v)?
				.checked_add(N.checked_mul(8)?)?
				.checked_add(32)
		})
		.ok_or(ArithmeticError::Budget("vector shape"))?;
	let mut scalar_bytes = b.working_scalar_bytes();
	for x in input
		.iter()
		.chain(center.iter())
		.chain(preconditioner.as_slice())
	{
		scalar_bytes = scalar_bytes.max(b.storage_bytes(x)?);
	}
	let admitted = |scalar_bytes: usize| {
		cells
			.checked_mul(scalar_bytes)
			.and_then(|bytes| {
				bytes.checked_add(
					limits
						.max_boxes
						.checked_mul(std::mem::size_of::<[B::Scalar; N]>())?
						.checked_mul(2)?,
				)
			})
			.and_then(|bytes| {
				bytes.checked_add(7_usize.checked_mul(std::mem::size_of::<Vec<B::Scalar>>())?)
			})
			.is_some_and(|bytes| bytes <= limits.max_bytes)
	};
	if limits.max_boxes == 0 || !admitted(scalar_bytes) {
		return Ok(result(vec![input], RootEvidence::default(), premise));
	}
	for i in 0..N {
		checked_center(b, &input[i], &center[i])?;
	}
	let VectorEvaluation {
		value: at_center,
		jacobian: unused,
	} = function(b, &center)?;
	drop(unused);
	for value in &at_center {
		scalar_bytes = scalar_bytes.max(b.storage_bytes(value)?);
	}
	if !admitted(scalar_bytes) {
		return Ok(result(vec![input], RootEvidence::default(), premise));
	}
	let VectorEvaluation {
		value: unused,
		jacobian: on_box,
	} = function(b, &input)?;
	drop(unused);
	// Open callbacks can return larger-precision values than the selected
	// backend normally produces. Admit their actual limbs before cloning them
	// or allocating the preconditioned matrix and branch buffers.
	for value in on_box.as_slice() {
		scalar_bytes = scalar_bytes.max(b.storage_bytes(value)?);
	}
	if !admitted(scalar_bytes) {
		return Ok(result(vec![input], RootEvidence::default(), premise));
	}
	let zero = b.point(0.0)?;
	let mut matrix = Vec::new();
	matrix
		.try_reserve_exact(
			N.checked_mul(N)
				.ok_or(ArithmeticError::Budget("matrix shape"))?,
		)
		.map_err(|_| ArithmeticError::Budget("matrix allocation"))?;
	let mut rhs = std::array::from_fn::<_, N, _>(|_| zero.clone());
	for i in 0..N {
		for k in 0..N {
			let term = b.mul(preconditioner.get(i, k)?.clone(), at_center[k].clone())?;
			rhs[i] = b.sub(rhs[i].clone(), term)?;
		}
		for j in 0..N {
			let mut value = zero.clone();
			for k in 0..N {
				let term = b.mul(preconditioner.get(i, k)?.clone(), on_box.get(k, j)?.clone())?;
				value = b.add(value, term)?;
			}
			matrix.push(value);
		}
	}
	let matrix = crate::shapes::Matrix::<_, N, N>::from_vec(matrix)?;
	let mut displacement = input.clone();
	for i in 0..N {
		displacement[i] = b.sub(input[i].clone(), center[i].clone())?;
	}
	let mut strict = true;
	let mut contraction = true;
	let mut exact_root = true;
	for i in 0..N {
		exact_root &= b.is_zero(&at_center[i])?;
		let mut image = b.add(center[i].clone(), rhs[i].clone())?;
		let mut norm = zero.clone();
		for j in 0..N {
			let identity = b.point(if i == j { 1.0 } else { 0.0 })?;
			let slope = b.sub(identity, matrix.get(i, j)?.clone())?;
			let term = b.mul(slope.clone(), displacement[j].clone())?;
			image = b.add(image, term)?;
			let negative = b.neg(slope.clone())?;
			let absolute = b.hull(&slope, &negative)?;
			let bound = b.upper(&absolute)?;
			norm = b.add(norm, bound)?;
		}
		strict &= b.strict_subset(&image, &input[i])?;
		contraction &= b.magnitude_lt_one(&norm)?;
	}
	let evidence = RootEvidence {
		at_most_one: contraction,
		exists: exact_root || (contraction && strict),
	};
	let mut branches = Vec::new();
	branches
		.try_reserve_exact(limits.max_boxes)
		.map_err(|_| ArithmeticError::Budget("vector branches"))?;
	branches.push(displacement);
	for i in 0..N {
		let mut next = Vec::new();
		next.try_reserve_exact(limits.max_boxes)
			.map_err(|_| ArithmeticError::Budget("vector branches"))?;
		for branch in &branches {
			let mut residual = rhs[i].clone();
			for k in 0..N {
				if i != k {
					let term = b.mul(matrix.get(i, k)?.clone(), branch[k].clone())?;
					residual = b.sub(residual, term)?;
				}
			}
			let images =
				extended_divide(b, residual, matrix.get(i, i)?.clone(), branch[i].clone())?;
			if next.len().saturating_add(images.len()) > limits.max_boxes {
				return Ok(result(vec![input], RootEvidence::default(), premise));
			}
			for image in images {
				let mut child = branch.clone();
				child[i] = image;
				next.push(child);
			}
		}
		branches = next;
	}
	let mut images = Vec::new();
	for mut branch in branches {
		let mut valid = true;
		for i in 0..N {
			let absolute = b.add(branch[i].clone(), center[i].clone())?;
			if let Some(x) = b.intersection(&absolute, &input[i])? {
				branch[i] = x;
			} else {
				valid = false;
				break;
			}
		}
		if valid {
			images.push(branch);
		}
	}
	Ok(result(images, evidence, premise))
}

// Before each commit, pending capacity admits pending plus unresolved boxes.
// Popping the current parent frees its recovery slot. Moving existing unresolved
// boxes into that buffer therefore never allocates during failure recovery.
fn retain_parent<T, E>(result: &mut RootCover<T, E>, mut pending: Vec<T>, parent: T) {
	pending.push(parent);
	pending.append(&mut result.unresolved);
	result.unresolved = pending;
}

fn retained_bytes<B: EnclosureBackend>(
	b: &B,
	pending: &Vec<B::Scalar>,
	result: &RootCover<B::Scalar, B::Error>,
) -> Result<usize, B::Error> {
	let scalar = std::mem::size_of::<B::Scalar>();
	let mut total = pending
		.capacity()
		.saturating_mul(scalar)
		.saturating_add(
			result
				.covered
				.capacity()
				.saturating_mul(std::mem::size_of::<RootBox<B::Scalar>>()),
		)
		.saturating_add(result.excluded.capacity().saturating_mul(scalar))
		.saturating_add(result.unresolved.capacity().saturating_mul(scalar))
		.saturating_add(4_usize.saturating_mul(std::mem::size_of::<Vec<B::Scalar>>()));
	for x in pending
		.iter()
		.chain(result.covered.iter().map(|x| &x.interval))
		.chain(result.excluded.iter())
		.chain(result.unresolved.iter())
	{
		total = total.saturating_add(b.storage_bytes(x)?.saturating_sub(scalar));
	}
	Ok(total)
}
