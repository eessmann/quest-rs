use crate::{Error, Result, error::BackendResult};
use quest_sys::mpi::MpiCollectiveLane;
pub(super) fn agree<T>(lane: &mut MpiCollectiveLane<'_>, value: Result<T>) -> Result<T> {
	if !lane
		.all_agree(value.is_ok())
		.context("agreeing constraint chart admission")?
	{
		return Err(Error::Value("collective constraint chart rejected"));
	}
	value
}
pub(super) fn fatal<T>(body: impl FnOnce() -> Result<T>) -> T {
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(body))
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
}
// Binomial delivery on the owned transport context. Both directions are bounded
// eight-byte frames; receivers return a dummy word instead of an unbounded send.
pub(super) fn scalar(
	lane: &mut MpiCollectiveLane<'_>,
	rank: usize,
	parts: usize,
	owner: usize,
	mut value: f64,
) -> Result<f64> {
	let virtual_rank = rank
		.checked_add(parts)
		.and_then(|v| v.checked_sub(owner))
		.ok_or(Error::Overflow)?
		% parts;
	let mut stride = 1usize;
	while stride < parts {
		let frontier = stride.checked_mul(2).ok_or(Error::Overflow)?;
		if virtual_rank < frontier {
			let peer = (virtual_rank ^ stride)
				.checked_add(owner)
				.ok_or(Error::Overflow)?
				% parts;
			let send = value.to_le_bytes();
			let mut receive = [0; 8];
			if lane
				.send_receive_bytes(
					&send,
					i32::try_from(peer).map_err(|_| Error::Overflow)?,
					3401,
					&mut receive,
				)
				.context("delivering chart scalar")?
				!= 8
			{
				return Err(Error::Value("chart delivery packet size"));
			}
			if virtual_rank >= stride {
				value = f64::from_le_bytes(receive);
			}
		}
		stride = frontier;
	}
	Ok(value)
}
pub(super) fn reduce(
	lane: &mut MpiCollectiveLane<'_>,
	rank: usize,
	parts: usize,
	mut value: f64,
	norm: bool,
) -> Result<f64> {
	let mut stride = 1usize;
	while stride < parts {
		let peer = rank ^ stride;
		let send = value.to_le_bytes();
		let mut recv = [0; 8];
		if lane
			.send_receive_bytes(
				&send,
				i32::try_from(peer).map_err(|_| Error::Overflow)?,
				3400,
				&mut recv,
			)
			.context("reducing chart scalar")?
			!= 8
		{
			return Err(Error::Value("chart scalar packet size"));
		}
		let other = f64::from_le_bytes(recv);
		let (left, right) = if rank & stride == 0 {
			(value, other)
		} else {
			(other, value)
		};
		value = if norm {
			left.hypot(right)
		} else {
			left + right
		};
		stride = stride.checked_mul(2).ok_or(Error::Overflow)?;
	}
	Ok(value)
}
