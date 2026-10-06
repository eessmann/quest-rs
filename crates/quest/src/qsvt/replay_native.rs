//! Reused checked primitive dispatch for bounded coherent replay sources.
use crate::{
	Error, QubitCount, Register, Result, StateVector,
	environment::{Reservation, RuntimeResources},
	execution::{self, NativeControls},
};
use quest_qsvt::{ReplayGate, ReplayKind};

/// Admitted control scratch for one environment and complete register width.
/// Source callers independently admit whole-stream work, storage and provenance.
pub struct ReplayGateExecutor<'env> {
	resources: &'env RuntimeResources,
	count: QubitCount,
	controls: NativeControls,
	reservation: Reservation<'env>,
}
impl<'env> ReplayGateExecutor<'env> {
	/// # Errors
	/// Rejects GPU deployment and insufficient control-scratch resources.
	pub fn new(register: &Register<'env, StateVector>) -> Result<Self> {
		if register.deployment().is_gpu_accelerated() {
			return Err(Error::Unsupported("CPU replay gate executor on GPU"));
		}
		Self::with_resources(register.resources(), register.num_qubits())
	}
	pub(crate) fn with_resources(
		resources: &'env RuntimeResources,
		count: QubitCount,
	) -> Result<Self> {
		let width = count.get();
		let bytes = width
			.checked_mul(
				size_of::<i32>()
					.checked_mul(3)
					.and_then(|n| n.checked_add(size_of::<usize>()))
					.ok_or(Error::Overflow)?,
			)
			.and_then(|n| n.checked_add(size_of::<i32>()))
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.ok_or(Error::Overflow)?;
		let mut reservation = resources.reserve(bytes)?;
		let controls = NativeControls::with_capacity(width, true)?;
		let actual = [
			(controls.wires.capacity(), size_of::<i32>()),
			(controls.states.capacity(), size_of::<i32>()),
			(controls.zeros.capacity(), size_of::<usize>()),
			(controls.phase_targets.capacity(), size_of::<i32>()),
		]
		.into_iter()
		.try_fold(size_of::<Self>(), |sum, (count, width)| {
			count
				.checked_mul(width)
				.and_then(|n| sum.checked_add(n))
				.ok_or(Error::Overflow)
		})?;
		reservation.resize(actual)?;
		Ok(Self {
			resources,
			count,
			controls,
			reservation,
		})
	}
	pub(crate) const fn retained_bytes(&self) -> usize {
		self.reservation.bytes()
	}
	/// Validate width, environment, signed controls, finite angles and targets.
	/// # Errors
	/// Rejects unsupported deployment, mismatched register identity or malformed primitives.
	pub fn validate(&self, register: &Register<'_, StateVector>, gate: ReplayGate) -> Result<()> {
		if !std::ptr::eq(self.resources, register.resources())
			|| self.count != register.num_qubits()
			|| register.deployment().is_gpu_accelerated()
		{
			return Err(Error::Value("replay executor/register mismatch"));
		}
		if gate.control_value & !gate.control_mask != 0
			|| gate.control_mask >= self.count.dimension()
		{
			return Err(Error::Value("invalid replay signed controls"));
		}
		let needs_dense = match gate.kind {
			ReplayKind::H | ReplayKind::X | ReplayKind::Ry(_) => true,
			ReplayKind::Phase(angle) => {
				gate.control_mask & !gate.control_value != 0
					&& quest_compile::dispatch_recipe::scalar_phase_recipe(
						angle,
						usize::try_from((gate.control_mask & !gate.control_value).count_ones())
							.map_err(|_| Error::Overflow)?,
					)?
					.native_calls()
						!= 0
			}
		};
		if needs_dense
			&& register.deployment().is_distributed()
			&& register.deployment().local_amplitudes() < 2
		{
			return Err(Error::Unsupported(
				"dense one-target replay requires two local amplitudes",
			));
		}
		match gate.kind {
			ReplayKind::Phase(angle) => {
				if !angle.is_finite() || gate.target.is_some() {
					return Err(Error::Value("invalid replay scalar phase"));
				}
			}
			ReplayKind::H | ReplayKind::X | ReplayKind::Ry(_) => {
				let target = gate
					.target
					.ok_or(Error::Value("missing replay gate target"))?;
				if target >= self.count.get() || gate.control_mask & (1usize << target) != 0 {
					return Err(Error::Value("invalid replay target/control overlap"));
				}
				if let ReplayKind::Ry(angle) = gate.kind
					&& !angle.is_finite()
				{
					return Err(Error::Value("nonfinite replay rotation"));
				}
			}
		}
		Ok(())
	}
	/// Dispatch a single already source-admitted primitive without allocating controls.
	/// # Errors
	/// Rejects malformed gates before mutation; propagates native dispatch failures.
	pub fn apply(
		&mut self,
		register: &mut Register<'_, StateVector>,
		gate: ReplayGate,
	) -> Result<()> {
		self.validate(register, gate)?;
		self.controls.load(
			(0..self.count.get())
				.filter(|&bit| gate.control_mask & (1usize << bit) != 0)
				.map(|bit| {
					Ok((
						i32::try_from(bit).map_err(|_| Error::Overflow)?,
						gate.control_value & (1usize << bit) != 0,
					))
				}),
			None,
		)?;
		match gate.kind {
			ReplayKind::Phase(angle) => execution::phase(register, angle, &self.controls),
			kind => {
				let target =
					i32::try_from(gate.target.ok_or(Error::Value("missing replay target"))?)
						.map_err(|_| Error::Overflow)?;
				let gate = match kind {
					ReplayKind::H => quest_compile::BoundGate::H,
					ReplayKind::X => quest_compile::BoundGate::X,
					ReplayKind::Ry(angle) => quest_compile::BoundGate::Ry(angle),
					ReplayKind::Phase(_) => return Err(Error::Value("invalid replay primitive")),
				};
				execution::apply_gate(register, &gate, &[target], &self.controls)
			}
		}
	}
}

#[cfg(all(feature = "mpi", quest_native_mpi))]
mod collective {
	use super::{Error, ReplayGate, ReplayGateExecutor, ReplayKind, Result};
	use crate::{
		collective::{CollectiveRegister, equal},
		error::BackendResult,
	};
	/// Collective checked primitive executor. Every participant calls in the same order.
	pub struct CollectiveReplayGateExecutor<'env> {
		inner: ReplayGateExecutor<'env>,
	}
	impl<'env> CollectiveReplayGateExecutor<'env> {
		/// # Errors
		/// Collectively rejects unsupported deployment and scratch admission failure.
		pub fn new(register: &CollectiveRegister<'env, '_, '_>) -> Result<Self> {
			let mut lane = register.environment.begin(60, register.id, 0, 0)?;
			let result = ReplayGateExecutor::new(&register.inner);
			if !lane
				.all_agree(result.is_ok())
				.context("agreeing replay executor preparation")?
			{
				return Err(Error::Value("collective replay scratch admission"));
			}
			Ok(Self { inner: result? })
		}
		/// # Errors
		/// Collectively rejects primitive/identity mismatch before mutation. Native failure is fatal.
		pub fn apply(
			&mut self,
			register: &mut CollectiveRegister<'env, '_, '_>,
			gate: ReplayGate,
		) -> Result<()> {
			let mut lane = register.environment.begin(61, register.id, 0, 0)?;
			let (kind, angle) = match gate.kind {
				ReplayKind::H => (0, 0),
				ReplayKind::X => (1, 0),
				ReplayKind::Ry(a) => (2, a.to_bits()),
				ReplayKind::Phase(a) => (3, a.to_bits()),
			};
			let words = [
				kind,
				angle,
				u64::try_from(gate.target.unwrap_or(usize::MAX)).map_err(|_| Error::Overflow)?,
				u64::try_from(gate.control_mask).map_err(|_| Error::Overflow)?,
				u64::try_from(gate.control_value).map_err(|_| Error::Overflow)?,
			];
			let mut payload = [0; 40];
			for (word, bytes) in words.into_iter().zip(payload.as_chunks_mut::<8>().0) {
				bytes.copy_from_slice(&word.to_le_bytes());
			}
			equal(&mut lane, &payload)?;
			let result = self.inner.validate(&register.inner, gate);
			if !lane
				.all_agree(result.is_ok())
				.context("agreeing replay primitive")?
			{
				return Err(Error::Value("collective replay primitive rejected"));
			}
			result?;
			std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
				self.inner.apply(&mut register.inner, gate)
			}))
			.unwrap_or_else(|_| quest_sys::mpi::abort_job())
			.unwrap_or_else(|_| quest_sys::mpi::abort_job());
			Ok(())
		}
	}
}
#[cfg(all(feature = "mpi", quest_native_mpi))]
pub use collective::CollectiveReplayGateExecutor;
