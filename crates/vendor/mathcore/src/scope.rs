//! Preflighted scope-validation scratch shared by symbolic construction paths.
use crate::{arithmetic::ArithmeticError as Error, exact::Symbol};
type Result<T> = std::result::Result<T, Error>;
pub struct ScopePlan {
	count: usize,
	pub(crate) work: usize,
	pub(crate) bytes: usize,
	pub(crate) search_work: usize,
}
impl ScopePlan {
	pub(crate) fn new(
		count: usize,
		max_variables: usize,
		max_bytes: usize,
		max_work: usize,
		live: usize,
	) -> Result<Self> {
		if count > max_variables {
			return Err(Error::Budget("symbol scope variables"));
		}
		let log = if count <= 1 {
			0
		} else {
			usize::BITS
				.checked_sub(
					count
						.checked_sub(1)
						.ok_or(Error::Budget("symbol scope work"))?
						.leading_zeros(),
				)
				.ok_or(Error::Budget("symbol scope work"))?
		};
		let log = usize::try_from(log).map_err(|_| Error::Budget("symbol scope work"))?;
		// Copy and duplicate scan cost 2n; the in-place unstable sort is
		// conservatively modeled as 4n ceil(log2(n)) comparisons/moves.
		let work = count
			.checked_mul(
				log.checked_mul(4)
					.and_then(|n| n.checked_add(2))
					.ok_or(Error::Budget("symbol scope work"))?,
			)
			.ok_or(Error::Budget("symbol scope work"))?;
		let bytes = count
			.checked_mul(size_of::<(Symbol, usize)>())
			.and_then(|n| n.checked_add(size_of::<Scope>()))
			.ok_or(Error::Budget("symbol scope storage"))?;
		if work > max_work || live.checked_add(bytes).is_none_or(|n| n > max_bytes) {
			return Err(Error::Budget("symbol scope admission"));
		}
		Ok(Self {
			count,
			work,
			bytes,
			search_work: if count == 0 {
				0
			} else {
				log.checked_add(1)
					.ok_or(Error::Budget("symbol scope work"))?
			},
		})
	}
	pub(crate) fn validate(&self, symbols: &[Symbol]) -> Result<Scope> {
		if symbols.len() != self.count {
			return Err(Error::Domain("symbol scope shape"));
		}
		let mut ordered = Vec::new();
		ordered
			.try_reserve_exact(self.count)
			.map_err(|_| Error::Budget("symbol scope allocation"))?;
		ordered.extend(
			symbols
				.iter()
				.copied()
				.enumerate()
				.map(|(index, symbol)| (symbol, index)),
		);
		ordered.sort_unstable_by_key(|(symbol, _)| *symbol);
		if ordered
			.windows(2)
			.any(|pair| matches!(pair,[(a,_),(b,_)] if a==b))
		{
			return Err(Error::Domain("duplicate symbol scope"));
		}
		Ok(Scope { ordered })
	}
}
pub struct Scope {
	ordered: Vec<(Symbol, usize)>,
}
impl Scope {
	pub(crate) fn resolve(&self, symbol: Symbol) -> Result<usize> {
		let index = self
			.ordered
			.binary_search_by_key(&symbol, |(value, _)| *value)
			.map_err(|_| Error::Domain("missing expression binding"))?;
		self.ordered
			.get(index)
			.map(|(_, index)| *index)
			.ok_or(Error::Domain("missing expression binding"))
	}
}
