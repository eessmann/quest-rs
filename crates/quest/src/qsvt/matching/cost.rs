//! Checked whole-register ceilings for the fixed 64-pair matching router.
use crate::{Error, Result};

/// Conservative maximum-rank arithmetic and application-payload admission.
/// Native MPI protocol/validation traffic and allocator metadata are excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchingExecutionCost {
	pub protocol_batches: usize,
	pub maximum_local_candidates: usize,
	pub maximum_rank_work: usize,
	pub aggregate_work: usize,
	/// Each of send and receive is bounded by this amount on each rank.
	pub application_bytes_per_rank: usize,
	pub aggregate_application_bytes: usize,
	/// Router count/frame exchanges and emptiness agreements; excludes entry admission.
	pub routing_collective_calls: usize,
	/// Hadamards, local clones and indexed read/write calls; not backend kernels.
	pub native_dispatches: usize,
	/// Full local passes plus conservative indexed elements read/written.
	pub native_state_elements: usize,
}
fn add(a: usize, b: usize) -> Result<usize> {
	a.checked_add(b).ok_or(Error::Overflow)
}
fn mul(a: usize, b: usize) -> Result<usize> {
	a.checked_mul(b).ok_or(Error::Overflow)
}
impl MatchingExecutionCost {
	/// Compute complete ceilings before outer controls can skip active rounds.
	/// # Errors
	/// Rejects inconsistent shape and checked count/work overflow.
	pub fn admit(
		dimension: usize,
		parts: usize,
		flag: usize,
		width: usize,
		colors: usize,
		records: usize,
	) -> Result<Self> {
		if !dimension.is_power_of_two()
			|| !parts.is_power_of_two()
			|| parts > dimension
			|| !flag.is_power_of_two()
			|| flag >= dimension
			|| colors > width
			|| 1usize.checked_shl(u32::try_from(width).map_err(|_| Error::Overflow)?)
				!= Some(dimension)
		{
			return Err(Error::Value("invalid matching execution shape"));
		}
		let local = dimension.checked_div(parts).ok_or(Error::Overflow)?;
		let candidates = if flag < local { local / 2 } else { local };
		let owners = if flag < local { parts } else { parts / 2 };
		let batches = mul(owners, candidates.div_ceil(64))?;
		let peers = parts.checked_sub(1).ok_or(Error::Overflow)?;
		let wire = mul(mul(mul(4, batches)?, peers)?, 5128)?;
		let search = usize::try_from(add(records, 1)?.ilog2()).map_err(|_| Error::Overflow)?;
		// Four peer packet passes (<=128 packets), layout loops, key sort/search,
		// coefficient rotation and bounds checks; generous scalar-operation envelope.
		let round = mul(mul(8192, add(parts, 1)?)?, add(add(width, search)?, 1)?)?;
		let work = add(
			add(mul(mul(64, local)?, add(width, 1)?)?, mul(batches, round)?)?,
			mul(1024, width)?,
		)?;
		let full_passes = add(mul(2, colors)?, 2)?;
		Ok(Self {
			protocol_batches: batches,
			maximum_local_candidates: candidates,
			maximum_rank_work: work,
			aggregate_work: mul(parts, work)?,
			application_bytes_per_rank: wire,
			aggregate_application_bytes: mul(parts, wire)?,
			routing_collective_calls: add(batches, mul(mul(8, batches)?, peers)?)?,
			native_dispatches: add(full_passes, mul(2, batches)?)?,
			native_state_elements: add(mul(local, full_passes)?, mul(256, batches)?)?,
		})
	}
}
