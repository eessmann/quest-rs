//! Shared operation admission. Bytes are modeled live ownership, never an allocator quota.
use std::{
	ops::Deref,
	sync::{Arc, Mutex, MutexGuard},
};

/// Independent representable shape gates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeLimits {
	pub max_coefficients: usize,
	pub max_fft_len: usize,
	pub max_completion_grid: usize,
}
impl Default for ShapeLimits {
	fn default() -> Self {
		Self {
			max_coefficients: 1_048_576,
			max_fft_len: 1_048_576,
			max_completion_grid: 1_048_576,
		}
	}
}
impl ShapeLimits {
	/// Degree one million is 1,000,001 slots. Completion requires a separate larger grid.
	#[must_use]
	pub const fn million_degree() -> Self {
		Self {
			max_coefficients: 1_000_001,
			max_fft_len: 1 << 21,
			max_completion_grid: 1 << 22,
		}
	}
	/// # Errors
	/// Rejects accounting overflow or the corresponding configured limit.
	pub const fn coefficients(self, count: usize) -> Result<(), ResourceError> {
		check("coefficients", count, self.max_coefficients)
	}
	/// # Errors
	/// Rejects accounting overflow or the corresponding configured limit.
	pub const fn fft(self, count: usize) -> Result<(), ResourceError> {
		check("padded FFT length", count, self.max_fft_len)
	}
	/// # Errors
	/// Rejects accounting overflow or the corresponding configured limit.
	pub const fn completion(self, count: usize) -> Result<(), ResourceError> {
		check("completion grid", count, self.max_completion_grid)
	}
}
/// Shared peak modeled storage and cumulative arithmetic admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceLimits {
	pub max_peak_bytes: usize,
	pub max_work_units: usize,
}
impl Default for ResourceLimits {
	fn default() -> Self {
		Self {
			max_peak_bytes: 536_870_912,
			max_work_units: usize::try_from(8_589_934_592_u64).unwrap_or(usize::MAX),
		}
	}
}
impl ResourceLimits {
	/// Explicit opt-in capacity profile. This does not establish numerical accuracy.
	#[must_use]
	#[cfg(target_pointer_width = "64")]
	pub const fn million_degree() -> Self {
		Self {
			max_peak_bytes: 8_589_934_592,
			max_work_units: 137_438_953_472,
		}
	}
}
/// One coherent configuration, shared by polynomial and FFT admission.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OperationLimits {
	pub shapes: ShapeLimits,
	pub resources: ResourceLimits,
}
impl OperationLimits {
	#[must_use]
	#[cfg(target_pointer_width = "64")]
	pub const fn million_degree() -> Self {
		Self {
			shapes: ShapeLimits::million_degree(),
			resources: ResourceLimits::million_degree(),
		}
	}
}
/// Checked admission failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResourceError {
	/// A fallible first-party allocation failed. Opaque `RustFFT` OOM is unrecoverable.
	#[error("owned buffer allocation failed")]
	Allocation,
	#[error("resource accounting overflow")]
	Overflow,
	#[error("{resource} limit exceeded: requested {requested}, limit {limit}")]
	Limit {
		resource: &'static str,
		requested: usize,
		limit: usize,
	},
}
const fn check(
	resource: &'static str,
	requested: usize,
	limit: usize,
) -> Result<(), ResourceError> {
	if requested > limit {
		Err(ResourceError::Limit {
			resource,
			requested,
			limit,
		})
	} else {
		Ok(())
	}
}
/// Snapshot of actual reservations and completed/admitted work batches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceReport {
	pub limits: OperationLimits,
	pub requested_peak_bytes: usize,
	pub requested_work_units: usize,
	pub last_rejection: Option<ResourceError>,
	pub live_bytes: usize,
	pub peak_bytes: usize,
	pub live_buffer_bytes: usize,
	pub live_planner_bytes_estimate: usize,
	pub work_units: usize,
	pub reservations: usize,
}
#[derive(Debug)]
struct Ledger {
	shapes: ShapeLimits,
	limits: ResourceLimits,
	report: Mutex<ResourceReport>,
}
impl Ledger {
	fn report(&self) -> MutexGuard<'_, ResourceReport> {
		self.report
			.lock()
			.unwrap_or_else(std::sync::PoisonError::into_inner)
	}
}
/// Cloneable operation owner. Clones share one monotonic ledger.
#[derive(Clone, Debug)]
pub struct OperationResources(Arc<Ledger>);
impl Default for OperationResources {
	fn default() -> Self {
		Self::from_limits(OperationLimits::default())
	}
}
impl OperationResources {
	#[must_use]
	pub fn new(shapes: ShapeLimits, limits: ResourceLimits) -> Self {
		Self(Arc::new(Ledger {
			shapes,
			limits,
			report: Mutex::new(ResourceReport {
				limits: OperationLimits {
					shapes,
					resources: limits,
				},
				..ResourceReport::default()
			}),
		}))
	}
	#[must_use]
	pub fn from_limits(limits: OperationLimits) -> Self {
		Self::new(limits.shapes, limits.resources)
	}
	#[must_use]
	pub fn limits(&self) -> OperationLimits {
		OperationLimits {
			shapes: self.0.shapes,
			resources: self.0.limits,
		}
	}
	#[must_use]
	pub fn report(&self) -> ResourceReport {
		*self.0.report()
	}
	/// Preflight a complete work batch without spending it during planning.
	/// # Errors
	/// Rejects cumulative overflow or the configured work limit.
	pub fn admit_work(&self, amount: usize) -> Result<(), ResourceError> {
		let mut report = self.0.report();
		let Some(next) = report.work_units.checked_add(amount) else {
			report.last_rejection = Some(ResourceError::Overflow);
			return Err(ResourceError::Overflow);
		};
		report.requested_work_units = report.requested_work_units.max(next);
		if let Err(error) = check("cumulative work", next, self.0.limits.max_work_units) {
			report.last_rejection = Some(error);
			return Err(error);
		}
		drop(report);
		Ok(())
	}
	/// Preflight storage known before a planner or a parallel dispatch.
	/// # Errors
	/// Rejects overflow or the configured peak limit.
	pub fn admit_peak(
		&self,
		buffer_bytes: usize,
		planner_bytes: usize,
	) -> Result<(), ResourceError> {
		let mut report = self.0.report();
		let Some(next) = buffer_bytes
			.checked_add(planner_bytes)
			.and_then(|n| n.checked_add(report.live_bytes))
		else {
			report.last_rejection = Some(ResourceError::Overflow);
			return Err(ResourceError::Overflow);
		};
		report.requested_peak_bytes = report.requested_peak_bytes.max(next);
		if let Err(error) = check("peak modeled bytes", next, self.0.limits.max_peak_bytes) {
			report.last_rejection = Some(error);
			return Err(error);
		}
		drop(report);
		Ok(())
	}
	/// Charge once before a whole kernel/stage. Failed execution retains this charge.
	/// # Errors
	/// Rejects accounting overflow or the corresponding configured limit.
	pub fn charge_work(&self, amount: usize) -> Result<(), ResourceError> {
		let mut report = self.0.report();
		let Some(next) = report.work_units.checked_add(amount) else {
			report.last_rejection = Some(ResourceError::Overflow);
			return Err(ResourceError::Overflow);
		};
		report.requested_work_units = report.requested_work_units.max(next);
		if let Err(error) = check("cumulative work", next, self.0.limits.max_work_units) {
			report.last_rejection = Some(error);
			return Err(error);
		}
		report.work_units = next;
		drop(report);
		Ok(())
	}
	/// Record a resource failure at an ownership boundary.
	pub fn record_rejection(&self, error: ResourceError) {
		self.0.report().last_rejection = Some(error);
	}
	/// Check source slots against the coefficient gate.
	/// # Errors
	/// Rejects accounting overflow or the corresponding configured limit.
	pub fn coefficients(&self, count: usize) -> Result<(), ResourceError> {
		self.0
			.shapes
			.coefficients(count)
			.inspect_err(|error| self.record_rejection(*error))
	}
	/// Check padded convolution transforms separately from source slots.
	/// # Errors
	/// Rejects accounting overflow or the corresponding configured limit.
	pub fn fft(&self, count: usize) -> Result<(), ResourceError> {
		self.0
			.shapes
			.fft(count)
			.inspect_err(|error| self.record_rejection(*error))
	}
	/// Check completion transforms against their independent gate.
	/// # Errors
	/// Rejects accounting overflow or the corresponding configured limit.
	pub fn completion(&self, count: usize) -> Result<(), ResourceError> {
		self.0
			.shapes
			.completion(count)
			.inspect_err(|error| self.record_rejection(*error))
	}
	/// Record a fallible first-party allocation failure in the same report.
	pub fn allocation_failed(&self) {
		self.0.report().last_rejection = Some(ResourceError::Allocation);
	}
	/// Reserve before allocation; clones count shared owned storage once.
	/// # Errors
	/// Rejects accounting overflow or the corresponding configured limit.
	pub fn reserve(
		&self,
		buffer_bytes: usize,
		planner_bytes_estimate: usize,
	) -> Result<MemoryReservation, ResourceError> {
		let mut report = self.0.report();
		let Some(total) = buffer_bytes.checked_add(planner_bytes_estimate) else {
			report.last_rejection = Some(ResourceError::Overflow);
			return Err(ResourceError::Overflow);
		};
		let Some(live) = report.live_bytes.checked_add(total) else {
			report.last_rejection = Some(ResourceError::Overflow);
			return Err(ResourceError::Overflow);
		};
		report.requested_peak_bytes = report.requested_peak_bytes.max(live);
		if let Err(error) = check("peak modeled bytes", live, self.0.limits.max_peak_bytes) {
			report.last_rejection = Some(error);
			return Err(error);
		}
		let buffers = report
			.live_buffer_bytes
			.checked_add(buffer_bytes)
			.ok_or(ResourceError::Overflow)?;
		let plans = report
			.live_planner_bytes_estimate
			.checked_add(planner_bytes_estimate)
			.ok_or(ResourceError::Overflow)?;
		let reservations = report
			.reservations
			.checked_add(1)
			.ok_or(ResourceError::Overflow)?;
		report.live_bytes = live;
		report.peak_bytes = report.peak_bytes.max(live);
		report.live_buffer_bytes = buffers;
		report.live_planner_bytes_estimate = plans;
		report.reservations = reservations;
		drop(report);
		Ok(MemoryReservation(Arc::new(Reserved {
			ledger: Arc::clone(&self.0),
			buffer_bytes,
			planner_bytes_estimate,
		})))
	}
	/// Admit an ordered parallel batch atomically before dispatch. No partial admission.
	/// # Errors
	/// Rejects accounting overflow or the corresponding configured limit.
	pub fn reserve_many(
		&self,
		requests: &[(usize, usize)],
	) -> Result<Vec<MemoryReservation>, ResourceError> {
		let (buffers, plans) = requests
			.iter()
			.try_fold((0_usize, 0_usize), |(buffers, plans), &(b, p)| {
				Ok::<_, ResourceError>((
					buffers.checked_add(b).ok_or(ResourceError::Overflow)?,
					plans.checked_add(p).ok_or(ResourceError::Overflow)?,
				))
			})
			.inspect_err(|error| self.record_rejection(*error))?;
		let total = buffers
			.checked_add(plans)
			.ok_or(ResourceError::Overflow)
			.inspect_err(|error| self.record_rejection(*error))?;
		let mut report = self.0.report();
		let Some(live) = report.live_bytes.checked_add(total) else {
			report.last_rejection = Some(ResourceError::Overflow);
			return Err(ResourceError::Overflow);
		};
		report.requested_peak_bytes = report.requested_peak_bytes.max(live);
		if let Err(error) = check("peak modeled bytes", live, self.0.limits.max_peak_bytes) {
			report.last_rejection = Some(error);
			return Err(error);
		}
		let next_buffers = report
			.live_buffer_bytes
			.checked_add(buffers)
			.ok_or(ResourceError::Overflow)?;
		let next_plans = report
			.live_planner_bytes_estimate
			.checked_add(plans)
			.ok_or(ResourceError::Overflow)?;
		let reservations = report
			.reservations
			.checked_add(requests.len())
			.ok_or(ResourceError::Overflow)?;
		report.live_bytes = live;
		report.peak_bytes = report.peak_bytes.max(live);
		report.live_buffer_bytes = next_buffers;
		report.live_planner_bytes_estimate = next_plans;
		report.reservations = reservations;
		drop(report);
		Ok(requests
			.iter()
			.map(|&(buffer_bytes, planner_bytes_estimate)| {
				MemoryReservation(Arc::new(Reserved {
					ledger: Arc::clone(&self.0),
					buffer_bytes,
					planner_bytes_estimate,
				}))
			})
			.collect())
	}
}
#[derive(Debug)]
struct Reserved {
	ledger: Arc<Ledger>,
	buffer_bytes: usize,
	planner_bytes_estimate: usize,
}
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Each token subtracts exactly one prior checked reservation; Arc drops it once"
)]
impl Drop for Reserved {
	fn drop(&mut self) {
		let mut report = self.ledger.report();
		report.live_buffer_bytes -= self.buffer_bytes;
		report.live_planner_bytes_estimate -= self.planner_bytes_estimate;
		report.live_bytes -= self.buffer_bytes + self.planner_bytes_estimate;
	}
}
/// Shared ownership token; the last clone releases storage admission.
#[derive(Clone, Debug)]
pub struct MemoryReservation(Arc<Reserved>);
impl MemoryReservation {
	#[must_use]
	pub fn bytes(&self) -> usize {
		self.0
			.buffer_bytes
			.saturating_add(self.0.planner_bytes_estimate)
	}
}
/// Result data and its owned-storage reservation travel together.
#[derive(Debug)]
pub struct Accounted<T> {
	value: T,
	reservations: Vec<MemoryReservation>,
}
impl<T> Accounted<T> {
	#[must_use]
	pub fn new(value: T, reservation: MemoryReservation) -> Self {
		Self {
			value,
			reservations: vec![reservation],
		}
	}
	/// Keep independently owned buffers charged together with their output.
	#[must_use]
	pub const fn new_many(value: T, reservations: Vec<MemoryReservation>) -> Self {
		Self {
			value,
			reservations,
		}
	}
	/// Transfer data together with its lifetime tokens. The caller must retain
	/// the tokens until the corresponding storage is dropped.
	pub fn into_parts(self) -> (T, Vec<MemoryReservation>) {
		(self.value, self.reservations)
	}
	/// Transform ownership without dropping its reservation.
	pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Accounted<U> {
		Accounted {
			value: f(self.value),
			reservations: self.reservations,
		}
	}
	#[must_use]
	pub const fn get(&self) -> &T {
		&self.value
	}
}
impl<T> Deref for Accounted<T> {
	type Target = T;
	fn deref(&self) -> &T {
		&self.value
	}
}
