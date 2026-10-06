//! Exclusive matching routing interval; no general register or gate access escapes.
use crate::{Complex64, Register, StateVector, error::BackendResult};

/// Owns the mutable input borrow until all staged writes have been committed.
/// At one rank an independent scratch register supplies the output partition;
/// distributed CPU registers instead stage into their existing communication array.
pub(super) struct RoutingState<'a, 'input, 'output> {
	input: &'a mut Register<'input, StateVector>,
	scratch: Option<&'a mut Register<'output, StateVector>>,
}
impl<'a, 'input, 'output> RoutingState<'a, 'input, 'output> {
	pub(super) fn stage(
		input: &'a mut Register<'input, StateVector>,
		scratch: Option<&'a mut Register<'output, StateVector>>,
	) -> crate::Result<Self> {
		if let Some(output) = scratch.as_ref() {
			// Mutable access is acquired below without exposing either native owner.
			if input.deployment().local_amplitudes() != output.deployment().local_amplitudes() {
				return Err(crate::Error::Value("matching staging partition length"));
			}
		}
		let mut state = Self { input, scratch };
		if let Some(output) = state.scratch.as_deref_mut() {
			quest_sys::set_qureg_to_clone(output.pin(), &state.input.native)
				.context("staging matching into owned scratch")?;
		} else {
			quest_sys::stage_cpu_communication_buffer(state.input.pin())
				.context("staging matching into native communication buffer")?;
		}
		Ok(state)
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
		if let Some(output) = self.scratch.as_deref_mut() {
			quest_sys::write_local_indexed_qureg_amps(output.pin(), indices, values)
				.context("writing matching owned scratch")
		} else {
			quest_sys::set_cpu_communication_buffer_indexed(self.input.pin(), indices, values)
				.context("writing matching native communication buffer")
		}
	}
	pub(super) fn commit(self) -> crate::Result<()> {
		if let Some(output) = self.scratch {
			quest_sys::set_qureg_to_clone(self.input.pin(), &output.native)
				.context("committing matching owned scratch")
		} else {
			quest_sys::commit_cpu_communication_buffer(self.input.pin())
				.context("committing matching native communication buffer")
		}
	}
}
