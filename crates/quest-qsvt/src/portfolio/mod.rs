//! Bounded coherent encoding portfolio with explicit preparation and oracle costs.
mod composition;
mod lcu_plan;
mod qrom;
mod sparse_access;
pub use qrom::Qrom;
pub use sparse_access::SparseAccessEncoding;
mod matching_bounds;
pub use matching_bounds::PerMatchingBounds;
mod structured_maps;
use crate::{
	EncodingDescriptor, Error, ReplayEncoding, Result, state_preparation::PreparationLimits,
};
pub use composition::{KroneckerSum, TensorProduct, WeightedLcu};
pub use lcu_plan::{LcuPlan, LcuPlanLimits, LcuPlanResources, LcuStep};
pub use structured_maps::{ArithmeticStencil, ArithmeticStencilTerm, Boundary, StructuredScheme};
/// Independent stored-table, compilation, replay and preparation limits.
#[derive(Clone, Copy, Debug)]
pub struct PortfolioLimits {
	pub max_bytes: usize,
	pub max_table_entries: usize,
	pub max_compile_work: usize,
	pub max_gates: usize,
	pub preparation: PreparationLimits,
}
impl Default for PortfolioLimits {
	fn default() -> Self {
		Self {
			max_bytes: 67_108_864,
			max_table_entries: 1_048_576,
			max_compile_work: 67_108_864,
			max_gates: 8_388_608,
			preparation: PreparationLimits::default(),
		}
	}
}
/// Actual streamed primitive counts and modeled stored construction costs.
#[derive(Clone, Copy, Debug, Default)]
pub struct PortfolioResources {
	pub elementary_gates: usize,
	/// Named QROM queries or child SELECT calls in one replay (not T gates).
	pub oracle_queries: usize,
	/// Underlying resource-directory lookups in one replay, when known.
	pub directory_queries: usize,
	/// Integrity and dry-run directory lookups performed during construction.
	pub admission_queries: usize,
	pub table_entries: usize,
	pub precision_bits: usize,
	pub workspace_qubits: usize,
	pub preparation_gates: usize,
	pub preparation_compile_work: usize,
	pub compile_work: usize,
	pub retained_bytes: usize,
	pub construction_peak_bytes: usize,
	/// Difference between implemented normalization and the binary64 nominal sum.
	/// This is accounting evidence, not an independent real-arithmetic certificate.
	pub normalization_roundoff: f64,
}
pub(super) fn add(a: usize, b: usize) -> Result<usize> {
	a.checked_add(b)
		.ok_or(Error::Budget("portfolio count overflow"))
}
pub(super) fn mul(a: usize, b: usize) -> Result<usize> {
	a.checked_mul(b)
		.ok_or(Error::Budget("portfolio count overflow"))
}
pub(super) fn bits(n: usize) -> Result<usize> {
	crate::matching::bit(n)
}
pub(super) fn hash(words: impl IntoIterator<Item = u64>) -> u64 {
	crate::owned_replay::fingerprint(words)
}
pub(super) fn word(n: usize) -> Result<u64> {
	u64::try_from(n).map_err(|_| Error::Budget("portfolio identity word"))
}
pub(super) const fn admit(r: PortfolioResources, l: PortfolioLimits) -> Result<()> {
	if r.retained_bytes > l.max_bytes
		|| r.construction_peak_bytes > l.max_bytes
		|| r.table_entries > l.max_table_entries
		|| r.compile_work > l.max_compile_work
		|| r.elementary_gates > l.max_gates
	{
		return Err(Error::Budget("portfolio resources"));
	}
	Ok(())
}
/// Shared modeled work admission for dry runs and already performed construction.
pub(super) fn charge_work(used: &mut usize, amount: usize, limit: usize) -> Result<()> {
	let next = add(*used, amount)?;
	if next > limit {
		return Err(Error::Budget("portfolio compile work"));
	}
	*used = next;
	Ok(())
}
pub(super) fn count<E: ReplayEncoding>(e: &E, l: PortfolioLimits) -> Result<usize> {
	count_with_work(e, l, &mut 0)
}
/// Charges both orientations against one allowance shared with prior work/children.
/// A failing callback can observe one additional source emission; no replay is
/// entered with an exhausted allowance. Opaque source work between callbacks is
/// governed by that source's own admission contract.
pub(super) fn count_with_work<E: ReplayEncoding>(
	e: &E,
	l: PortfolioLimits,
	used: &mut usize,
) -> Result<usize> {
	let start = *used;
	let mut count = 0;
	for adjoint in [false, true] {
		if *used >= l.max_compile_work {
			return Err(Error::Budget("portfolio compile work"));
		}
		let mut own = 0;
		e.visit_replay(adjoint, &mut |_| {
			charge_work(used, 1, l.max_compile_work)?;
			own = add(own, 1)?;
			if own > l.max_gates {
				return Err(Error::Budget("portfolio gate scan"));
			}
			Ok(())
		})?;
		count = count.max(own);
	}
	// Retain the existing conservative two-times-max receipt for sources whose
	// forward and adjoint primitive counts differ.
	let conservative = mul(count, 2)?;
	let scanned = used.saturating_sub(start);
	charge_work(
		used,
		conservative.saturating_sub(scanned),
		l.max_compile_work,
	)?;
	Ok(count)
}
pub(super) fn targets(start: usize, width: usize) -> Result<[usize; 64]> {
	let mut map = [0; 64];
	if width > map.len() {
		return Err(Error::Budget("portfolio operand scratch"));
	}
	for (i, v) in map.iter_mut().take(width).enumerate() {
		*v = add(start, i)?;
	}
	Ok(map)
}
pub(super) fn full_projectors(d: &EncodingDescriptor) -> Result<()> {
	let dimension = bits(
		usize::try_from(d.layout.system_mask.count_ones())
			.map_err(|_| Error::Budget("portfolio system width"))?,
	)?;
	if d.left.fixed_mask != d.layout.workspace_mask
		|| d.right.fixed_mask != d.layout.workspace_mask
		|| d.left.logical_range != (0..dimension)
		|| d.right.logical_range != (0..dimension)
	{
		return Err(Error::Encoding(
			"tensor factors require full power-of-two system projectors",
		));
	}
	Ok(())
}
