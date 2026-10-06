use crate::{Error, Result, error::BackendResult};
use quest_sys::mpi::MpiCollectiveLane;
pub(super) fn agree<T>(lane: &mut MpiCollectiveLane<'_>, result: Result<T>) -> Result<T> {
	if !lane
		.all_agree(result.is_ok())
		.context("agreeing amplitude preparation")?
	{
		return Err(Error::Value("collective amplitude admission rejected"));
	}
	result
}
pub(super) fn fatal<T>(body: impl FnOnce() -> Result<T>) -> T {
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(body))
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
		.unwrap_or_else(|_| quest_sys::mpi::abort_job())
}
pub(super) fn float(packet: &[u8], index: usize) -> Result<f64> {
	let start = index.checked_mul(8).ok_or(Error::Overflow)?;
	let end = start.checked_add(8).ok_or(Error::Overflow)?;
	Ok(f64::from_le_bytes(
		packet
			.get(start..end)
			.ok_or(Error::Overflow)?
			.try_into()
			.map_err(|_| Error::Overflow)?,
	))
}
pub(super) fn put(packet: &mut [u8], index: usize, value: f64) -> Result<()> {
	let start = index.checked_mul(8).ok_or(Error::Overflow)?;
	let end = start.checked_add(8).ok_or(Error::Overflow)?;
	packet
		.get_mut(start..end)
		.ok_or(Error::Overflow)?
		.copy_from_slice(&value.to_le_bytes());
	Ok(())
}
pub(super) fn broadcast(
	lane: &mut MpiCollectiveLane<'_>,
	owner: usize,
	packet: &mut [u8],
) -> Result<()> {
	lane.broadcast_bytes(i32::try_from(owner).map_err(|_| Error::Overflow)?, packet)
		.context("delivering amplitude scalar")
}
