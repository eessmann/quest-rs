//! Enclosure proofs for the actual selected coefficient values. A minimax lower
//! bound additionally needs strict, ordered sign alternation (de la Vallée Poussin).
use super::{
	Accuracy, AdmittedFunction, ArithmeticError, Chebyshev, EnclosureBackend, Error, ExactConstant,
	Polynomial, RemezOptions, Result, RootCover, Shape,
};
use quest_numerics::{
	ad::{First, Jet},
	roots::{self, CoverLimits},
};

pub(super) fn residual_jet<F, I, D>(
	f: &F,
	p: &Polynomial<Chebyshev, I::Endpoint, D>,
	b: &mut I,
	x: I::Scalar,
) -> Result<Jet<I::Scalar>>
where
	F: AdmittedFunction,
	I: EnclosureBackend<Error = ArithmeticError>,
	D: Shape,
{
	let target = f.jet(b, x.clone())?;
	let polynomial = p.jet_enclosure(b, x)?;
	Ok(Jet {
		value: b.sub(target.value, polynomial.value)?,
		first: b.sub(target.first, polynomial.first)?,
		second: b.sub(target.second, polynomial.second)?,
	})
}
const fn callback_error(error: Error) -> ArithmeticError {
	match error {
		Error::Arithmetic(error) => error,
		Error::Budget(reason) => {
			ArithmeticError::Core(mathcore::arithmetic::ArithmeticError::Budget(reason))
		}
		Error::NonFinite => ArithmeticError::Core(mathcore::arithmetic::ArithmeticError::Nonfinite),
		Error::Interval(error) => ArithmeticError::Interval(error),
		_ => ArithmeticError::Core(mathcore::arithmetic::ArithmeticError::Domain(
			"polynomial derivative enclosure",
		)),
	}
}
pub(super) fn residual<F, I, D>(
	f: &F,
	p: &Polynomial<Chebyshev, I::Endpoint, D>,
	b: &mut I,
	x: I::Scalar,
) -> Result<I::Scalar>
where
	F: AdmittedFunction,
	I: EnclosureBackend<Error = ArithmeticError>,
	D: Shape,
{
	let target = f.evaluate(b, x.clone())?;
	let candidate = p.evaluate_enclosure(b, x)?;
	Ok(b.sub(target, candidate)?)
}
pub(super) fn magnitude<I: EnclosureBackend<Error = ArithmeticError>>(
	b: &mut I,
	x: &I::Scalar,
) -> Result<I::Scalar> {
	let negative = b.neg(x.clone())?;
	let hull = b.hull(x, &negative)?;
	Ok(b.upper(&hull)?)
}
pub(super) fn maximum<I: EnclosureBackend<Error = ArithmeticError>>(
	b: &mut I,
	a: &I::Scalar,
	c: &I::Scalar,
) -> Result<I::Scalar> {
	let hull = b.hull(a, c)?;
	Ok(b.upper(&hull)?)
}
pub(super) fn minimum<I: EnclosureBackend<Error = ArithmeticError>>(
	b: &mut I,
	a: &I::Scalar,
	c: &I::Scalar,
) -> Result<I::Scalar> {
	let hull = b.hull(a, c)?;
	Ok(b.lower(&hull)?)
}
pub(super) fn positive<I: EnclosureBackend<Error = ArithmeticError>>(
	b: &I,
	x: &I::Scalar,
) -> Result<bool> {
	Ok(b.nonnegative(x)? && !b.contains_zero(x)?)
}
pub(super) fn nonnegative_difference<I: EnclosureBackend<Error = ArithmeticError>>(
	b: &mut I,
	a: I::Scalar,
	c: I::Scalar,
) -> Result<bool> {
	let delta = b.sub(a, c)?;
	Ok(b.nonnegative(&delta)?)
}
pub(super) fn cover<F, I, D>(
	f: &F,
	p: &Polynomial<Chebyshev, I::Endpoint, D>,
	b: &mut I,
	domain: I::Scalar,
	width: &I::Endpoint,
	options: &RemezOptions,
) -> Result<RootCover<I::Scalar>>
where
	F: AdmittedFunction,
	I: EnclosureBackend<Error = ArithmeticError>,
	D: Shape,
{
	Ok(roots::cover(
		b,
		domain,
		|b, x| {
			let j = residual_jet(f, p, b, x.clone()).map_err(callback_error)?;
			Ok(First {
				value: j.first,
				first: j.second,
			})
		},
		CoverLimits {
			max_iterations: options.max_subdivisions,
			max_boxes: options.max_subdivisions,
			max_bytes: options.limits.resources.max_peak_bytes,
		},
		width,
		roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)?)
}
pub(super) fn uniform<F, I, D>(
	f: &F,
	p: &Polynomial<Chebyshev, I::Endpoint, D>,
	b: &mut I,
	domain: &I::Scalar,
	roots: &RootCover<I::Scalar>,
) -> Result<I::Scalar>
where
	F: AdmittedFunction,
	I: EnclosureBackend<Error = ArithmeticError>,
	D: Shape,
{
	if !roots.complete() {
		return Err(Error::NotEstablished("incomplete critical-point coverage"));
	}
	let mut upper = b.point(0.0)?;
	for endpoint in [b.lower(domain)?, b.upper(domain)?] {
		let value = residual(f, p, b, endpoint)?;
		let absolute = magnitude(b, &value)?;
		upper = maximum(b, &upper, &absolute)?;
	}
	for root in &roots.covered {
		let center = b.midpoint(&root.interval)?;
		let j = residual_jet(f, p, b, root.interval.clone())?;
		let at_center = residual(f, p, b, center.clone())?;
		let displacement = b.sub(root.interval.clone(), center)?;
		let correction = b.mul(j.first, displacement)?;
		let centered = b.add(at_center, correction)?;
		let value = b
			.intersection(&j.value, &centered)?
			.ok_or(Error::NotEstablished("inconsistent residual enclosures"))?;
		let absolute = magnitude(b, &value)?;
		upper = maximum(b, &upper, &absolute)?;
	}
	let zero = b.point(0.0)?;
	Ok(b.hull(&zero, &upper)?)
}
/// Independently verify one candidate alternation. Points are required to be in
/// the inner admitted domain; outer rounding of a rational endpoint must not
/// manufacture a lower bound on the original domain.
pub(super) fn alternation<F, I, D>(
	f: &F,
	p: &Polynomial<Chebyshev, I::Endpoint, D>,
	b: &mut I,
	points: &[I::Endpoint],
	count: usize,
	inner_lower: &I::Endpoint,
	inner_upper: &I::Endpoint,
) -> Result<(I::Scalar, Option<usize>)>
where
	F: AdmittedFunction,
	I: EnclosureBackend<Error = ArithmeticError>,
	D: Shape,
{
	let mut best = b.point(0.0)?;
	let mut best_start = None;
	for (start, window) in points.windows(count).enumerate() {
		let mut lower = None;
		let mut previous_sign = None;
		let mut previous_point = None;
		let mut valid = true;
		for x in window {
			let x = b.singleton(x)?;
			let lo = b.singleton(inner_lower)?;
			let hi = b.singleton(inner_upper)?;
			if !nonnegative_difference(b, x.clone(), lo)?
				|| !nonnegative_difference(b, hi, x.clone())?
			{
				valid = false;
				break;
			}
			if let Some(previous) = previous_point {
				let difference = b.sub(x.clone(), previous)?;
				if !positive(b, &difference)? {
					valid = false;
					break;
				}
			}
			previous_point = Some(x.clone());
			let error = residual(f, p, b, x)?;
			if b.contains_zero(&error)? {
				valid = false;
				break;
			}
			let sign = b.nonnegative(&error)?;
			if previous_sign == Some(sign) {
				valid = false;
				break;
			}
			previous_sign = Some(sign);
			let absolute = if sign { error } else { b.neg(error)? };
			let absolute = b.lower(&absolute)?;
			lower = Some(if let Some(old) = lower {
				minimum(b, &old, &absolute)?
			} else {
				absolute
			});
		}
		if valid && let Some(lower) = lower {
			let improvement = b.sub(lower.clone(), best.clone())?;
			if positive(b, &improvement)? {
				best = lower;
				best_start = Some(start);
			}
		}
	}
	Ok((best, best_start))
}
pub(super) fn satisfies<I: EnclosureBackend<Error = ArithmeticError>>(
	b: &mut I,
	accuracy: &Accuracy,
	uniform: &I::Scalar,
	gap: &I::Scalar,
) -> Result<bool> {
	fn within<I: EnclosureBackend<Error = ArithmeticError>>(
		b: &mut I,
		value: &I::Scalar,
		source: &ExactConstant,
	) -> Result<bool> {
		let tolerance = b.constant(source)?;
		if !positive(b, &tolerance)? {
			return Err(Error::Domain);
		}
		let tolerance = b.lower(&tolerance)?;
		let value = b.upper(value)?;
		nonnegative_difference(b, tolerance, value)
	}
	match accuracy {
		Accuracy::UniformError(t) => within(b, uniform, t),
		Accuracy::MinimaxGap(t) => within(b, gap, t),
		Accuracy::Both { uniform: t, gap: g } => Ok(within(b, uniform, t)? && within(b, gap, g)?),
	}
}

/// Admit immutable coefficient metadata once, before repeated proof evaluations.
/// The exchange kernel's support cache must describe the selected stored
/// polynomial. Keeping this boundary check outside the extrema hot loop avoids
/// repeating coefficient scans and MP singleton allocations for every box.
pub(super) fn admit_polynomial<I, D>(
	b: &mut I,
	p: &Polynomial<Chebyshev, I::Endpoint, D>,
) -> Result<()>
where
	I: EnclosureBackend<Error = ArithmeticError>,
	D: Shape,
{
	let mut support = None;
	for (index, coefficient) in p.coefficients().iter().enumerate() {
		let value = b.singleton(coefficient)?;
		if !b.is_zero(&value)? {
			let order = i32::try_from(index).map_err(|_| Error::SupportOverflow)?;
			support = Some((support.map_or(order, |(first, _)| first), order));
		}
	}
	if support != p.effective_support() {
		return Err(Error::NotEstablished("inconsistent coefficient support"));
	}
	Ok(())
}

/// Candidate arithmetic is open, so even its binary64 conversion/reimport is
/// untrusted. Match the frozen external payload using enclosing arithmetic.
pub(super) fn check_export<I, D>(
	b: &mut I,
	p: &Polynomial<Chebyshev, I::Endpoint, D>,
	payload: &[f64],
) -> Result<()>
where
	I: EnclosureBackend<Error = ArithmeticError>,
	D: Shape,
{
	if payload.len() != p.coefficients().len() {
		return Err(Error::NotEstablished("export coefficient shape"));
	}
	for (source, coefficient) in payload.iter().zip(p.coefficients()) {
		let exported = b.point(*source)?;
		let selected = b.singleton(coefficient)?;
		if !b.same(&exported, &selected)? {
			return Err(Error::NotEstablished(
				"export changed by candidate arithmetic",
			));
		}
	}
	Ok(())
}

/// Outer endpoints cover the exact source domain. Inner endpoints restrict
/// alternation witnesses to points belonging to that same original domain.
pub(super) struct AdmittedDomain<T, E> {
	pub outer: T,
	pub inner_lower: E,
	pub inner_upper: E,
}
pub(super) fn admit_domain<I: EnclosureBackend<Error = ArithmeticError>>(
	b: &mut I,
	source: &super::ExactDomain,
) -> Result<AdmittedDomain<I::Scalar, I::Endpoint>> {
	let lower = b.constant(&source.lower)?;
	let upper = b.constant(&source.upper)?;
	let separation = b.sub(upper.clone(), lower.clone())?;
	if !positive(b, &separation)? {
		return Err(Error::NotEstablished("positive-width exact domain"));
	}
	Ok(AdmittedDomain {
		outer: b.hull(&lower, &upper)?,
		inner_lower: b.upper_endpoint(&lower)?,
		inner_upper: b.lower_endpoint(&upper)?,
	})
}
