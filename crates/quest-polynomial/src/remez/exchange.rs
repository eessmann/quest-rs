//! Candidate generation and deterministic exchange. Enclosure proofs live in
//! `proof`; a small numerical residual alone never creates a certificate.
use super::{
	AdmittedFunction, Arc, ArithmeticError, Attempt, Chebyshev, EnclosureBackend, Error,
	ExactConstant, ExactDomain, Limits, LinearSolver, PointBackend, Polynomial, RemezOptions,
	Result, RootCover, Shape, proof,
};
use crate::Basis;
use std::cmp::Ordering;

struct Candidate<C, D: Shape> {
	polynomial: Polynomial<Chebyshev, C, D>,
	export: Option<Arc<[f64]>>,
}
pub(super) struct Established<C, T, D: Shape> {
	pub polynomial: Polynomial<Chebyshev, C, D>,
	pub uniform: T,
	pub lower: T,
	pub gap: T,
}

fn storage<T>(count: usize, limits: Limits) -> Result<Vec<T>> {
	let bytes = count
		.checked_mul(size_of::<T>())
		.ok_or(Error::Budget("Remez allocation size"))?;
	if bytes > limits.resources.max_peak_bytes || isize::try_from(bytes).is_err() {
		return Err(Error::Budget("Remez allocation"));
	}
	let mut values = Vec::new();
	values
		.try_reserve_exact(count)
		.map_err(|_| Error::Budget("Remez allocation"))?;
	Ok(values)
}
fn integer<P: PointBackend<Error = ArithmeticError>>(p: &mut P, n: usize) -> Result<P::Scalar> {
	Ok(p.constant(&ExactConstant::Integer(
		i64::try_from(n).map_err(|_| Error::Budget("Remez integer"))?,
	))?)
}
fn absolute<P: PointBackend<Error = ArithmeticError>>(
	p: &mut P,
	x: P::Scalar,
) -> Result<P::Scalar> {
	let zero = p.point(0.0)?;
	if p.compare(&x, &zero)? == Ordering::Less {
		Ok(p.neg(x)?)
	} else {
		Ok(x)
	}
}
fn sort<P: PointBackend<Error = ArithmeticError>>(
	p: &P,
	points: &mut Vec<P::Scalar>,
) -> Result<()> {
	// Fallible in-place heapsort. A failed comparison stops immediately; it is
	// never substituted into a standard sort's total-order contract.
	fn sift<P: PointBackend<Error = ArithmeticError>>(
		p: &P,
		values: &mut [P::Scalar],
		mut root: usize,
		end: usize,
	) -> Result<()> {
		loop {
			let Some(mut child) = root.checked_mul(2).and_then(|n| n.checked_add(1)) else {
				return Ok(());
			};
			if child >= end {
				return Ok(());
			}
			let right = child.saturating_add(1);
			if right < end
				&& p.compare(
					values.get(child).ok_or(Error::Domain)?,
					values.get(right).ok_or(Error::Domain)?,
				)? == Ordering::Less
			{
				child = right;
			}
			if p.compare(
				values.get(root).ok_or(Error::Domain)?,
				values.get(child).ok_or(Error::Domain)?,
			)? != Ordering::Less
			{
				return Ok(());
			}
			values.swap(root, child);
			root = child;
		}
	}
	let len = points.len();
	for root in (0..len / 2).rev() {
		sift(p, points, root, len)?;
	}
	for end in (1..len).rev() {
		points.swap(0, end);
		sift(p, points, 0, end)?;
	}
	if len > 0 {
		let mut write = 1usize;
		for read in 1..len {
			if p.compare(
				points.get(write.saturating_sub(1)).ok_or(Error::Domain)?,
				points.get(read).ok_or(Error::Domain)?,
			)? != Ordering::Equal
			{
				points.swap(write, read);
				write = write.saturating_add(1);
			}
		}
		points.truncate(write);
	}
	Ok(())
}

fn reference<F, P, D, L>(
	f: &F,
	p: &mut P,
	solver: &L,
	shape: D,
	points: &[P::Scalar],
	options: &RemezOptions,
) -> Result<Candidate<P::Scalar, D>>
where
	F: AdmittedFunction,
	P: PointBackend<Error = ArithmeticError>,
	D: Shape,
	L: LinearSolver<P>,
{
	let coefficients = shape.coefficient_count();
	let n = coefficients
		.checked_add(1)
		.ok_or(Error::Budget("Remez dimension"))?;
	if points.len() != n {
		return Err(Error::Shape {
			expected: n,
			actual: points.len(),
		});
	}
	let mut matrix = storage(
		n.checked_mul(n).ok_or(Error::Budget("Remez matrix"))?,
		options.limits,
	)?;
	let mut rhs = storage(n, options.limits)?;
	for (row, x) in points.iter().enumerate() {
		let mut previous = p.point(0.0)?;
		let mut current = p.point(1.0)?;
		for degree in 0..coefficients {
			if degree > 0 {
				let (scale, shift, back) = Chebyshev.recurrence_with(
					u32::try_from(degree).map_err(|_| Error::SupportOverflow)?,
					p,
				)?;
				let ax = p.mul(scale, x.clone())?;
				let factor = p.add(ax, shift)?;
				let product = p.mul(factor, current.clone())?;
				let correction = p.mul(back, previous)?;
				previous = current;
				current = p.sub(product, correction)?;
			}
			matrix.push(current.clone());
		}
		matrix.push(p.point(if row % 2 == 0 { 1.0 } else { -1.0 })?);
		rhs.push(f.evaluate(p, x.clone())?);
	}
	let solution = solver.solve(p, &matrix, &rhs, n, options.limits)?;
	if solution.values.len() != n || solution.rank != n {
		return Err(Error::NotEstablished("alternation system rank"));
	}
	let mut values = storage(coefficients, options.limits)?;
	let mut export = if options.export_binary64 {
		Some(storage(coefficients, options.limits)?)
	} else {
		None
	};
	for value in solution.values.into_iter().take(coefficients) {
		values.push(if let Some(export) = &mut export {
			let exported = p.to_f64(&value)?;
			export.push(exported);
			p.point(exported)?
		} else {
			value
		});
	}
	Ok(Candidate {
		polynomial: Polynomial::from_scalars(Chebyshev, values, shape, p, options.limits)?,
		export: export.map(Arc::from),
	})
}
fn extrema<F, P, I, D>(
	f: &F,
	polynomial: &Polynomial<Chebyshev, P::Scalar, D>,
	p: &mut P,
	b: &mut I,
	roots: &RootCover<I::Scalar>,
	inner: (&P::Scalar, &P::Scalar),
	limits: Limits,
) -> Result<Vec<P::Scalar>>
where
	F: AdmittedFunction,
	P: PointBackend<Error = ArithmeticError>,
	I: EnclosureBackend<Endpoint = P::Scalar, Error = ArithmeticError>,
	D: Shape,
{
	let (lower, upper) = inner;
	let mut points = storage(
		roots
			.covered
			.len()
			.checked_add(2)
			.ok_or(Error::Budget("extrema"))?,
		limits,
	)?;
	points.push(lower.clone());
	for root in &roots.covered {
		let center = b.midpoint(&root.interval)?;
		let center = b.lower_endpoint(&center)?;
		if p.compare(&center, lower)? == Ordering::Greater
			&& p.compare(&center, upper)? == Ordering::Less
		{
			points.push(center);
		}
	}
	points.push(upper.clone());
	sort(p, &mut points)?;
	let mut selected: Vec<(P::Scalar, P::Scalar, bool)> = storage(points.len(), limits)?;
	let zero = p.point(0.0)?;
	for x in points {
		let actual = f.evaluate(p, x.clone())?;
		let candidate = polynomial.evaluate_with(p, x.clone())?;
		let value = p.sub(actual, candidate)?;
		let sign = p.compare(&value, &zero)?;
		if sign == Ordering::Equal {
			continue;
		}
		let positive = sign == Ordering::Greater;
		let magnitude = absolute(p, value)?;
		if let Some(last) = selected.last_mut()
			&& last.2 == positive
		{
			if p.compare(&magnitude, &last.1)? == Ordering::Greater {
				*last = (x, magnitude, positive);
			}
		} else {
			selected.push((x, magnitude, positive));
		}
	}
	Ok(selected.into_iter().map(|(x, _, _)| x).collect())
}
fn initial_reference<P: PointBackend<Error = ArithmeticError>>(
	p: &mut P,
	lower: &P::Scalar,
	upper: &P::Scalar,
	n: usize,
	limits: Limits,
) -> Result<Vec<P::Scalar>> {
	let two = p.point(2.0)?;
	let half_lower = p.div(lower.clone(), two.clone())?;
	let half_upper = p.div(upper.clone(), two)?;
	let center = p.add(half_lower.clone(), half_upper.clone())?;
	let radius = p.sub(half_upper, half_lower)?;
	let mut points = storage(n, limits)?;
	let pi = p.pi()?;
	let divisor = integer(p, n.saturating_sub(1))?;
	for k in 0..n {
		let index = integer(p, k)?;
		let angle = p.mul(pi.clone(), index)?;
		let angle = p.div(angle, divisor.clone())?;
		let cosine = p.cos(angle)?;
		let offset = p.mul(radius.clone(), cosine)?;
		points.push(p.sub(center.clone(), offset)?);
	}
	if let Some(first) = points.first_mut() {
		*first = lower.clone();
	}
	if let Some(last) = points.last_mut() {
		*last = upper.clone();
	}
	Ok(points)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run<F, P, I, D, L>(
	f: &F,
	domain: &ExactDomain,
	shape: &D,
	p: &mut P,
	b: &mut I,
	solver: &L,
	options: &RemezOptions,
	attempt: &mut Attempt<P::Scalar, I::Scalar, D>,
) -> Result<Established<P::Scalar, I::Scalar, D>>
where
	F: AdmittedFunction,
	P: PointBackend<Error = ArithmeticError>,
	I: EnclosureBackend<Endpoint = P::Scalar, Error = ArithmeticError>,
	D: Shape,
	L: LinearSolver<P>,
{
	let coefficients = shape.coefficient_count();
	let n = coefficients
		.checked_add(1)
		.ok_or(Error::Budget("Remez degree"))?;
	if coefficients == 0 || options.max_iterations == 0 || options.max_subdivisions == 0 {
		return Err(Error::Domain);
	}
	crate::check_storage(options.limits, n, 10)?;
	let scalar_bytes = p.working_scalar_bytes().max(b.working_scalar_bytes());
	let cells = n
		.checked_mul(n)
		.and_then(|v| v.checked_mul(32))
		.ok_or(Error::Budget("Remez QR workspace"))?;
	let expression_scalars = f
		.structure()
		.map_or(0, |metadata| metadata.depth.saturating_mul(64));
	let bytes = cells
		.checked_add(expression_scalars)
		.and_then(|n| n.checked_mul(scalar_bytes))
		.ok_or(Error::Budget("Remez workspace"))?;
	if bytes > options.limits.resources.max_peak_bytes {
		return Err(Error::Budget("Remez QR workspace"));
	}
	let proof::AdmittedDomain {
		outer,
		inner_lower: lower,
		inner_upper: upper,
	} = proof::admit_domain(b, domain)?;
	let width = b.constant(&options.root_width)?;
	if !proof::positive(b, &width)? {
		return Err(Error::Domain);
	}
	let width = b.lower_endpoint(&width)?;
	// Admit the complete target and derivative domain before exchange begins.
	f.jet(b, outer.clone())?;
	let mut points = initial_reference(p, &lower, &upper, n, options.limits)?;
	let mut cover_options = options.clone();
	// Reserve the remaining three quarters for live endpoint copies, extrema,
	// root-proof temporaries and sorting; the cover owns at most one quarter.
	cover_options.limits.resources.max_peak_bytes = options
		.limits
		.resources
		.max_peak_bytes
		.saturating_sub(bytes)
		/ 4;
	for iteration in 0..options.max_iterations {
		let Candidate { polynomial, export } =
			reference(f, p, solver, shape.clone(), &points, options)?;
		// Until a new candidate is available, the previous cover still belongs
		// to the retained polynomial and must survive a failed QR attempt.
		attempt.coverage = None;
		attempt.iterations = iteration.saturating_add(1);
		attempt.candidate = Some(polynomial.clone());
		attempt.binary64_export = export;
		proof::admit_polynomial(b, &polynomial)?;
		if let Some(export) = &attempt.binary64_export {
			proof::check_export(b, &polynomial, export)?;
		}
		let roots = proof::cover(f, &polynomial, b, outer.clone(), &width, &cover_options)?;
		attempt.coverage = Some(roots);
		let roots = attempt
			.coverage
			.as_ref()
			.ok_or(Error::NotEstablished("missing root cover"))?;
		let uniform = proof::uniform(f, &polynomial, b, &outer, roots)?;
		let new_points = extrema(
			f,
			&polynomial,
			p,
			b,
			roots,
			(&lower, &upper),
			options.limits,
		)?;
		let (lower_bound, start) =
			proof::alternation(f, &polynomial, b, &new_points, n, &lower, &upper)?;
		let gap = b.sub(uniform.clone(), lower_bound.clone())?;
		if proof::satisfies(b, &options.accuracy, &uniform, &gap)? {
			return Ok(Established {
				polynomial,
				uniform,
				lower: lower_bound,
				gap,
			});
		}
		let start = start.ok_or(Error::NotEstablished(
			"insufficient proved strict alternation",
		))?;
		let next = new_points
			.into_iter()
			.skip(start)
			.take(n)
			.collect::<Vec<_>>();
		let mut same = next.len() == points.len();
		for (a, c) in next.iter().zip(&points) {
			same &= p.compare(a, c)? == Ordering::Equal;
		}
		if same {
			return Err(Error::NotEstablished(
				"Remez precision or coefficient-export stall",
			));
		}
		points = next;
	}
	Err(Error::NotEstablished("Remez iteration limit"))
}
