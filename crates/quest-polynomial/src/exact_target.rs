//! Cold conversion from bounded exact expressions to numerical monomial targets.
use crate::{Complex64, Error, Interval, Limits, Monomial, Polynomial, Result};
use mathcore::{
	arithmetic::ExactConstant,
	dynamic::DynamicExpression,
	identity::Symbol,
	multivariate::{PolynomialLimits, SparsePolynomial},
};
use quest_numerics::arithmetic::{Backend, F64Backend, Interval64Backend};

/// Explicit binary64 target construction with retained exact source and error.
///
/// The bound covers coefficient rounding of the canonical polynomial on [-1,1].
/// It is not permission to change ordered source evaluation or a certificate
/// for QSP synthesis, basis conversion, or a QSVT circuit.
#[derive(Clone, Debug)]
pub struct ExactMonomialTarget {
	source: DynamicExpression,
	exact: SparsePolynomial,
	polynomial: Polynomial<Monomial>,
	rounding_bound: f64,
}
impl ExactMonomialTarget {
	/// Canonicalize a single-variable exact polynomial and round coefficients once.
	/// # Errors
	/// Rejects non-polynomial source, unsupported constants, resource exhaustion,
	/// and any failed outward rounding bound.
	pub fn from_expression(
		source: DynamicExpression,
		symbol: Symbol,
		algebra: PolynomialLimits,
		limits: Limits,
	) -> Result<Self> {
		let exact = source.polynomial(&[symbol], algebra)?;
		let count = usize::try_from(exact.degree())
			.map_err(|_| Error::SupportOverflow)?
			.checked_add(1)
			.ok_or(Error::SupportOverflow)?;
		crate::check_storage(limits, count, 1)?;
		let work = exact
			.logical_work()
			.checked_add(count)
			.ok_or(Error::Budget("exact target work"))?;
		let bytes = source
			.retained_bytes()
			.checked_add(exact.retained_bytes()?)
			.and_then(|b| b.checked_add(count.checked_mul(size_of::<Complex64>())?))
			.and_then(|b| b.checked_add(size_of::<Self>()))
			.ok_or(Error::Budget("exact target storage"))?;
		if work > limits.resources.max_work_units || bytes > limits.resources.max_peak_bytes {
			return Err(Error::Budget("exact target construction"));
		}
		let mut coefficients = crate::zeros(count, limits)?;
		let mut point = F64Backend;
		let mut enclosure = Interval64Backend;
		let mut bound = Interval::point(0.0)?;
		for (powers, value) in exact.terms() {
			let degree = usize::try_from(*powers.first().ok_or(Error::UnsupportedConversion)?)
				.map_err(|_| Error::SupportOverflow)?;
			let constant = ExactConstant::Ratio {
				numerator: value.numerator().to_string(),
				denominator: value.denominator().to_string(),
			};
			let rounded = point.constant(&constant)?;
			let exact_interval = enclosure.constant(&constant)?;
			let difference = exact_interval.checked_sub(Interval::point(rounded)?)?;
			let radius = difference.lower().abs().max(difference.upper().abs());
			bound = bound.checked_add(Interval::point(radius)?)?;
			*coefficients.get_mut(degree).ok_or(Error::SupportOverflow)? =
				Complex64::new(rounded, 0.0);
		}
		let polynomial = Polynomial::new(Monomial, coefficients, limits)?;
		Ok(Self {
			source,
			exact,
			polynomial,
			rounding_bound: bound.upper(),
		})
	}
	#[must_use]
	pub const fn source(&self) -> &DynamicExpression {
		&self.source
	}
	#[must_use]
	pub const fn exact(&self) -> &SparsePolynomial {
		&self.exact
	}
	#[must_use]
	pub const fn polynomial(&self) -> &Polynomial<Monomial> {
		&self.polynomial
	}
	#[must_use]
	pub const fn rounding_bound(&self) -> f64 {
		self.rounding_bound
	}
}
