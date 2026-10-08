//! Exclusive routing owns a separately admitted output register.
use crate::{Complex64, Register, StateVector, error::BackendResult};

/// Retains exclusive input and scratch borrows until commit or abandonment.
/// Dropping before commit leaves the input amplitudes unchanged by routing.
pub(super) struct RoutingState<'a, 'input, 'output> {
	input: &'a mut Register<'input, StateVector>,
	scratch: &'a mut Register<'output, StateVector>,
}
impl<'a, 'input, 'output> RoutingState<'a, 'input, 'output> {
	pub(super) fn stage(
		input: &'a mut Register<'input, StateVector>,
		scratch: &'a mut Register<'output, StateVector>,
	) -> crate::Result<Self> {
		if input.deployment().local_amplitudes() != scratch.deployment().local_amplitudes() {
			return Err(crate::Error::Value("matching staging partition length"));
		}
		quest_sys::set_qureg_to_clone(scratch.pin(), &input.native)
			.context("staging matching into owned scratch")?;
		Ok(Self { input, scratch })
	}
	pub(super) const fn local_amplitudes(&self) -> usize {
		self.input.deployment().local_amplitudes()
	}
	pub(super) fn read_local(&self, index: usize) -> crate::Result<Complex64> {
		super::read_local(self.input, index)
	}
	pub(super) fn read_indexed(
		&self,
		indices: &[i64],
		values: &mut [quest_sys::QuestComplex],
	) -> crate::Result<()> {
		quest_sys::read_local_indexed_qureg_amps(&self.input.native, indices, values)
			.context("reading matching indexed input")
	}
	pub(super) fn write_local(&mut self, index: usize, value: Complex64) -> crate::Result<()> {
		self.write_indexed(
			&[i64::try_from(index).map_err(|_| crate::Error::Overflow)?],
			&[quest_sys::QuestComplex {
				re: value.re,
				im: value.im,
			}],
		)
	}
	pub(super) fn write_indexed(
		&mut self,
		indices: &[i64],
		values: &[quest_sys::QuestComplex],
	) -> crate::Result<()> {
		quest_sys::write_local_indexed_qureg_amps(self.scratch.pin(), indices, values)
			.context("writing matching owned scratch")
	}
	pub(super) fn commit(self) -> crate::Result<()> {
		quest_sys::set_qureg_to_clone(self.input.pin(), &self.scratch.native)
			.context("committing matching owned scratch")
	}
}
