use crate::Complex64;

/// A rejected numerical domain, shape, or resource request.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
	/// Dimensions must be positive and representable.
	#[error("invalid numerical length: {0}")]
	Length(&'static str),
	/// Integer resource accounting overflowed.
	#[error("resource size overflow")]
	Overflow,
	/// The request exceeds a caller limit.
	#[error("{resource} budget exceeded: requested {requested}, limit {limit}")]
	Budget {
		resource: &'static str,
		requested: usize,
		limit: usize,
	},
	/// A fallible wrapper-owned allocation failed.
	#[error("numerical buffer allocation failed")]
	Allocation,
	/// The first nonfinite entry, in input/output order.
	#[error("nonfinite complex value at index {index}")]
	NonFinite { index: usize },
	/// No finite enclosing interval exists for this operation.
	#[error("invalid or unbounded interval")]
	Interval,
	/// The complete input interval must belong to the function domain.
	#[error("interval outside domain of {0}")]
	Domain(&'static str),
	/// The requested instruction set is absent or disabled at compile time.
	#[error("requested FFT SIMD backend is unavailable")]
	BackendUnavailable,
}

/// A checked numerical result.
pub type Result<T> = std::result::Result<T, Error>;

/// Resource admission for construction and each reusable operation.
///
/// Work units are a conservative model, not a measured operation count. Bytes
/// include exact wrapper-owned data plus a planner allowance. `RustFFT` does not
/// expose its allocation sizes or fallible planning; `max_bytes` is therefore
/// not an allocator-enforced cap on the opaque upstream plans.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
	/// Maximum transform length or input coefficient count.
	pub max_len: usize,
	/// Maximum accounted bytes, including the modeled planner allowance.
	pub max_bytes: usize,
	/// Maximum modeled work units per transform/convolution.
	pub max_work: usize,
}

impl Default for Limits {
	fn default() -> Self {
		Self {
			max_len: 1_048_576,
			max_bytes: 536_870_912,
			max_work: usize::try_from(68_719_476_736_u64).unwrap_or(usize::MAX),
		}
	}
}

/// Auditable admission model for a constructed workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceUsage {
	/// Bytes for the wrapper's coefficient and scratch buffers.
	pub buffer_bytes: usize,
	/// Reserved allowance for opaque planner allocations; an estimate only.
	pub planner_bytes_estimate: usize,
	/// Deterministic arithmetic work estimate: 8*N*max(log2(N),1) for each
	/// power-of-two FFT, N*N otherwise; convolution adds pointwise work.
	/// This models arithmetic, not measured instructions or runtime.
	pub work_units: usize,
}

/// Explicit execution ownership. No process-global pool is used.
#[derive(Clone, Copy, Debug, Default)]
pub enum ExecutionPolicy<'pool> {
	/// Execute in the calling thread.
	#[default]
	Sequential,
	/// Execute independent pointwise products in this caller-owned pool.
	#[cfg(feature = "rayon")]
	Rayon(&'pool rayon::ThreadPool),
	/// Carries the borrow lifetime when Rayon support is compiled out.
	#[doc(hidden)]
	#[cfg(not(feature = "rayon"))]
	Lifetime(std::marker::PhantomData<&'pool ()>),
}

pub const fn check_limit(resource: &'static str, requested: usize, limit: usize) -> Result<()> {
	if requested > limit {
		Err(Error::Budget {
			resource,
			requested,
			limit,
		})
	} else {
		Ok(())
	}
}

pub fn checked_len(len: usize, limits: Limits) -> Result<()> {
	if len == 0 {
		return Err(Error::Length("zero"));
	}
	// u32 conversion supplies an exact f64 normalization factor without casts.
	u32::try_from(len).map_err(|_| Error::Length("exceeds binary64 FFT indexing policy"))?;
	check_limit("length", len, limits.max_len)
}

pub fn finite(values: &[Complex64]) -> Result<()> {
	if let Some(index) = values
		.iter()
		.position(|value| !value.re.is_finite() || !value.im.is_finite())
	{
		return Err(Error::NonFinite { index });
	}
	Ok(())
}

pub fn zeros(len: usize) -> Result<Vec<Complex64>> {
	let mut values = Vec::new();
	values
		.try_reserve_exact(len)
		.map_err(|_| Error::Allocation)?;
	values.resize(len, Complex64::new(0.0, 0.0));
	Ok(values)
}
