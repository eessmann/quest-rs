use super::{
	Complex64, Error, ExecutionPolicy, FftBackend, FftDirection, FftWorkspace, Limits,
	Normalization, ResourceUsage, Result, admit, bytes, checked_len, finite, multiply,
	plan_allowance, support_len, work, zeros,
};

/// Reusable linear convolutions sharing two immutable right-hand inputs.
///
/// Three fixed-size arrays hold the left input/product and two right spectra.
/// Padding, plans, arithmetic order, and normalization match
/// [`super::ConvolutionWorkspace`] constructed with the same maximum supports.
/// Each session prepares its right spectra lazily and never reuses spectra from
/// an earlier session.
#[derive(Debug)]
pub struct SharedConvolutionWorkspace {
	fft: FftWorkspace,
	left: Vec<Complex64>,
	right: [Vec<Complex64>; 2],
	max_left: usize,
	max_right: usize,
	#[cfg(feature = "rayon")]
	second_scratch: Option<Vec<Complex64>>,
	usage: ResourceUsage,
}

impl SharedConvolutionWorkspace {
	/// Prepare sequential convolutions up to these input lengths.
	///
	/// # Errors
	/// Rejects zero/excessive lengths, overflow, resource budgets, unavailable
	/// SIMD, or failed wrapper allocations. Opaque `RustFFT` planning allocations
	/// have the same limitations as [`FftWorkspace::new`].
	pub fn new(
		max_left: usize,
		max_right: usize,
		backend: FftBackend,
		limits: Limits,
	) -> Result<Self> {
		Self::new_with_policy(
			max_left,
			max_right,
			backend,
			limits,
			ExecutionPolicy::Sequential,
		)
	}

	/// Prepare optional parallel forward-pair scratch for a caller-owned pool.
	///
	/// No pool borrow is retained. As with ordinary convolution, additional
	/// scratch is prepared for pools with more than one worker and FFT lengths
	/// at least 2048. Later sessions can select any execution policy; without
	/// prepared parallel scratch, forward transforms execute sequentially.
	///
	/// # Errors
	/// Same failures as [`Self::new`], including additional scratch admission.
	pub fn new_with_policy(
		max_left: usize,
		max_right: usize,
		backend: FftBackend,
		limits: Limits,
		execution: ExecutionPolicy<'_>,
	) -> Result<Self> {
		checked_len(max_left, limits)?;
		checked_len(max_right, limits)?;
		let support = support_len(max_left, max_right)?;
		let len = support.checked_next_power_of_two().ok_or(Error::Overflow)?;
		checked_len(len, limits)?;
		let data_bytes = bytes(len.checked_mul(3).ok_or(Error::Overflow)?)?;
		let mut usage = ResourceUsage {
			buffer_bytes: data_bytes,
			planner_bytes_estimate: plan_allowance(len)?,
			work_units: work(len, 3)?.checked_add(len).ok_or(Error::Overflow)?,
		};
		admit(usage, limits)?;
		let fft_limits = Limits {
			max_bytes: limits
				.max_bytes
				.checked_sub(data_bytes)
				.ok_or(Error::Overflow)?,
			..limits
		};
		let fft = FftWorkspace::new(len, backend, fft_limits)?;
		usage.buffer_bytes = data_bytes
			.checked_add(fft.resource_usage().buffer_bytes)
			.ok_or(Error::Overflow)?;
		#[cfg(feature = "rayon")]
		let parallel = matches!(execution, ExecutionPolicy::Rayon(pool) if pool.current_num_threads() > 1 && len >= 2048);
		#[cfg(not(feature = "rayon"))]
		let _ = execution;
		#[cfg(feature = "rayon")]
		if parallel {
			usage.buffer_bytes = usage
				.buffer_bytes
				.checked_add(bytes(fft.scratch.len())?)
				.ok_or(Error::Overflow)?;
		}
		admit(usage, limits)?;
		#[cfg(feature = "rayon")]
		let second_scratch = if parallel {
			Some(zeros(fft.scratch.len())?)
		} else {
			None
		};
		Ok(Self {
			fft,
			left: zeros(len)?,
			right: [zeros(len)?, zeros(len)?],
			max_left,
			max_right,
			#[cfg(feature = "rayon")]
			second_scratch,
			usage,
		})
	}

	/// Fixed FFT length, including zero padding.
	#[must_use]
	pub const fn fft_len(&self) -> usize {
		self.fft.len()
	}

	/// Exact owned buffer bytes and modeled planner/work admission.
	///
	/// Work is the upper bound for one cold product, not an entire session:
	/// `3*T + L`, where `T = 8*L*max(log2(L),1)`. Once a selected right spectrum
	/// is ready, its next product costs `2*T + L`; use session `work_for` when
	/// charging individual products. Constructor limits admit the cold bound.
	#[must_use]
	pub const fn resource_usage(&self) -> ResourceUsage {
		self.usage
	}

	/// Begin products with two immutable RHS inputs and fresh spectrum readiness.
	///
	/// RHS shapes and finite values are checked only when selected by a product,
	/// after checking the left input. The session exclusively borrows workspace
	/// buffers; dropping it releases those buffers and both input/pool borrows.
	pub const fn session<'workspace, 'inputs, 'pool>(
		&'workspace mut self,
		right: [&'inputs [Complex64]; 2],
		execution: ExecutionPolicy<'pool>,
	) -> SharedConvolutionSession<'workspace, 'inputs, 'pool> {
		SharedConvolutionSession {
			workspace: self,
			right,
			ready: [false; 2],
			execution,
		}
	}
}

/// Scoped products whose two right-hand transforms are prepared on first use.
pub struct SharedConvolutionSession<'workspace, 'inputs, 'pool> {
	workspace: &'workspace mut SharedConvolutionWorkspace,
	right: [&'inputs [Complex64]; 2],
	ready: [bool; 2],
	execution: ExecutionPolicy<'pool>,
}

impl SharedConvolutionSession<'_, '_, '_> {
	/// Work for the next product with the selected RHS, without preparing it.
	///
	/// # Errors
	/// Rejects indices other than 0 or 1, or arithmetic accounting overflow.
	pub fn work_for(&self, right_index: usize) -> Result<usize> {
		let ready = self.ready.get(right_index).ok_or(Error::Length(
			"shared convolution right index must be 0 or 1",
		))?;
		work(self.workspace.fft_len(), if *ready { 2 } else { 3 })?
			.checked_add(self.workspace.fft_len())
			.ok_or(Error::Overflow)
	}

	/// Compute a linear product, borrowing the output until the next mutation.
	///
	/// Input/output support, padding, multiplication operand order, and finite
	/// scans match ordinary convolution. Cold products can run their two forward
	/// transforms concurrently; warm products transform only the left input.
	/// All readiness is cleared after any failed product. Numerical buffers are
	/// reused; caller-owned Rayon scheduling may allocate external storage.
	///
	/// # Errors
	/// Rejects an invalid RHS index, empty/oversized inputs, or the first
	/// nonfinite input/intermediate/output in the ordinary convolution order.
	pub fn product(&mut self, left: &[Complex64], right_index: usize) -> Result<&[Complex64]> {
		match self.product_inner(left, right_index) {
			Ok(support) => self
				.workspace
				.left
				.get(..support)
				.ok_or(Error::Length("output support")),
			Err(error) => {
				self.ready.fill(false);
				Err(error)
			}
		}
	}

	fn product_inner(&mut self, left: &[Complex64], right_index: usize) -> Result<usize> {
		let right = self.right.get(right_index).ok_or(Error::Length(
			"shared convolution right index must be 0 or 1",
		))?;
		if left.is_empty()
			|| right.is_empty()
			|| left.len() > self.workspace.max_left
			|| right.len() > self.workspace.max_right
		{
			return Err(Error::Length(
				"convolution input exceeds planned support or is empty",
			));
		}
		finite(left)?;
		finite(right)?;
		let support = support_len(left.len(), right.len())?;
		self.workspace.left.fill(Complex64::new(0.0, 0.0));
		self.workspace
			.left
			.get_mut(..left.len())
			.ok_or(Error::Length("left support"))?
			.copy_from_slice(left);
		let ready = self.ready.get_mut(right_index).ok_or(Error::Length(
			"shared convolution right index must be 0 or 1",
		))?;
		let spectrum = self
			.workspace
			.right
			.get_mut(right_index)
			.ok_or(Error::Length(
				"shared convolution right index must be 0 or 1",
			))?;
		if *ready {
			self.workspace.fft.transform(
				&mut self.workspace.left,
				FftDirection::Forward,
				Normalization::None,
			)?;
		} else {
			spectrum.fill(Complex64::new(0.0, 0.0));
			spectrum
				.get_mut(..right.len())
				.ok_or(Error::Length("right support"))?
				.copy_from_slice(right);
			forward_pair(
				&mut self.workspace.fft,
				&mut self.workspace.left,
				spectrum,
				#[cfg(feature = "rayon")]
				self.workspace.second_scratch.as_mut(),
				self.execution,
			)?;
			*ready = true;
		}
		multiply(&mut self.workspace.left, spectrum, self.execution);
		self.workspace.fft.transform(
			&mut self.workspace.left,
			FftDirection::Inverse,
			Normalization::ByLength,
		)?;
		Ok(support)
	}
}

fn forward_pair(
	fft: &mut FftWorkspace,
	left: &mut [Complex64],
	right: &mut [Complex64],
	#[cfg(feature = "rayon")] second_scratch: Option<&mut Vec<Complex64>>,
	execution: ExecutionPolicy<'_>,
) -> Result<()> {
	#[cfg(feature = "rayon")]
	if let (ExecutionPolicy::Rayon(pool), Some(second_scratch)) = (execution, second_scratch) {
		let forward = &fft.forward;
		let first_scratch = &mut fft.scratch;
		pool.install(|| {
			rayon::join(
				|| forward.process_with_scratch(left, first_scratch),
				|| forward.process_with_scratch(right, second_scratch),
			)
		});
		finite(left)?;
		return finite(right);
	}
	#[cfg(not(feature = "rayon"))]
	let _ = execution;
	fft.transform(left, FftDirection::Forward, Normalization::None)?;
	fft.transform(right, FftDirection::Forward, Normalization::None)
}
