use crate::{Command, Error, Result};
use std::{num::NonZeroUsize, str::FromStr};

/// Explicit caller-owned parallelism; the default remains one worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workers {
	Count(NonZeroUsize),
	Auto,
}
impl FromStr for Workers {
	type Err = Error;
	fn from_str(value: &str) -> Result<Self> {
		if value == "auto" {
			return Ok(Self::Auto);
		}
		value
			.parse()
			.map(Self::Count)
			.map_err(|_| Error::Input("workers must be auto or a positive integer"))
	}
}
impl Workers {
	/// Resolve automatic parallelism on the executing local/root rank.
	/// # Errors
	/// Reports an OS error if available parallelism cannot be determined.
	pub fn resolve(self) -> Result<NonZeroUsize> {
		match self {
			Self::Count(count) => Ok(count),
			Self::Auto => Ok(std::thread::available_parallelism()?),
		}
	}
}
pub struct Pool {
	count: usize,
	#[cfg(feature = "rayon")]
	inner: Option<rayon::ThreadPool>,
}
impl Pool {
	pub(crate) const fn serial() -> Self {
		Self {
			count: 1,
			#[cfg(feature = "rayon")]
			inner: None,
		}
	}
	pub(crate) fn build(selection: Workers, command: &Command) -> Result<Self> {
		let count = selection.resolve()?.get();
		if count == 1 {
			return Ok(Self::serial());
		}
		crate::worker_scope(command).ok_or(Error::Input(
			"workers require a catalogue check or explicit binary64 synthesis",
		))?;
		#[cfg(not(feature = "rayon"))]
		{
			Err(Error::Feature("rayon"))
		}
		#[cfg(feature = "rayon")]
		{
			let inner = rayon::ThreadPoolBuilder::new().num_threads(count).build()?;
			Ok(Self {
				count: inner.current_num_threads(),
				inner: Some(inner),
			})
		}
	}
	pub(crate) const fn count(&self) -> usize {
		self.count
	}
	#[cfg(feature = "rayon")]
	pub(crate) const fn inner(&self) -> Option<&rayon::ThreadPool> {
		self.inner.as_ref()
	}
}
