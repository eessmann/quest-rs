//! Prepared whole matching unitaries on arbitrary flag/color/failure states.
use super::Result;
use crate::{
	Environment, QubitCount, Register, StateVector,
	environment::{Reservation, RuntimeResources},
	error::BackendResult,
};
use quest_qsvt::{MatchingHeader, MatchingShard};
use std::{collections::BTreeSet, ops::Range};
#[cfg(all(feature = "mpi", quest_native_mpi))]
mod batched;
#[cfg(all(feature = "mpi", quest_native_mpi))]
pub mod collective;
mod local_batch;

pub(super) struct MatchingLayout {
	pub(super) count: QubitCount,
	pub(super) targets: Vec<usize>,
	pub(super) header: MatchingHeader,
	active_mask: usize,
}
impl MatchingLayout {
	pub(super) fn new(
		header: MatchingHeader,
		count: QubitCount,
		targets: Vec<usize>,
	) -> crate::Result<Self> {
		header
			.validate()
			.map_err(|_| crate::Error::Value("invalid matching manifest"))?;
		if targets.len() != header.num_qubits().map_err(|_| crate::Error::Overflow)? {
			return Err(crate::Error::Value("matching target count"));
		}
		let mut active_mask = 0;
		for &target in &targets {
			if target >= count.get() || active_mask & bit(target)? != 0 {
				return Err(crate::Error::Value("invalid matching targets"));
			}
			active_mask |= bit(target)?;
		}
		Ok(Self {
			count,
			targets,
			header,
			active_mask,
		})
	}
	pub(super) fn controls(
		&self,
		mask: usize,
		value: usize,
	) -> crate::Result<(Vec<i32>, Vec<i32>)> {
		if mask >= self.count.dimension() || value & !mask != 0 || mask & self.active_mask != 0 {
			return Err(crate::Error::Value("invalid matching outer controls"));
		}
		let mut positions = Vec::new();
		let mut outcomes = Vec::new();
		for position in 0..self.count.get() {
			if mask & bit(position)? != 0 {
				positions.push(i32::try_from(position).map_err(|_| crate::Error::Overflow)?);
				outcomes.push(i32::from(value & bit(position)? != 0));
			}
		}
		Ok((positions, outcomes))
	}
	pub(super) fn flag(&self) -> crate::Result<usize> {
		bit(*self
			.targets
			.first()
			.ok_or(crate::Error::Value("missing matching flag"))?)
	}
	pub(super) const fn system_range(&self) -> Range<usize> {
		1..self.header.system_qubits.saturating_add(1)
	}
	pub(super) const fn color_range(&self) -> Range<usize> {
		self.header.system_qubits.saturating_add(1)..self.targets.len()
	}
	pub(super) fn extract(&self, basis: usize, range: Range<usize>) -> crate::Result<usize> {
		let mut packed = 0;
		for (local, target_index) in range.enumerate() {
			let target = *self
				.targets
				.get(target_index)
				.ok_or(crate::Error::Value("matching layout index"))?;
			if basis & bit(target)? != 0 {
				packed |= bit(local)?;
			}
		}
		Ok(packed)
	}
	pub(super) fn replace_system(&self, basis: usize, system: usize) -> crate::Result<usize> {
		let mut result = basis;
		for (local, target_index) in self.system_range().enumerate() {
			let mask = bit(*self
				.targets
				.get(target_index)
				.ok_or(crate::Error::Value("matching layout index"))?)?;
			result &= !mask;
			if system & bit(local)? != 0 {
				result |= mask;
			}
		}
		Ok(result)
	}
	pub(super) fn hadamards(
		&self,
		register: &mut Register<'_, StateVector>,
		positions: &[i32],
		outcomes: &[i32],
	) -> crate::Result<()> {
		for index in self.color_range() {
			let target = i32::try_from(
				*self
					.targets
					.get(index)
					.ok_or(crate::Error::Value("matching color target"))?,
			)
			.map_err(|_| crate::Error::Overflow)?;
			if positions.is_empty() {
				quest_sys::apply_hadamard(register.pin(), target)
			} else {
				quest_sys::apply_multi_state_controlled_hadamard(
					register.pin(),
					positions,
					outcomes,
					target,
				)
			}
			.context("applying matching color preparation")?;
		}
		Ok(())
	}
}
pub(super) fn bit(position: usize) -> crate::Result<usize> {
	1usize
		.checked_shl(u32::try_from(position).map_err(|_| crate::Error::Overflow)?)
		.ok_or(crate::Error::Overflow)
}
#[cfg(all(feature = "mpi", quest_native_mpi))]
pub(super) fn read_local(
	register: &Register<'_, StateVector>,
	index: usize,
) -> crate::Result<quest_qsvt::Complex64> {
	let mut value = [quest_sys::QuestComplex { re: 0.0, im: 0.0 }];
	quest_sys::read_local_qureg_amps(
		&register.native,
		i64::try_from(index).map_err(|_| crate::Error::Overflow)?,
		&mut value,
	)
	.context("reading matching local amplitude")?;
	Ok(quest_qsvt::Complex64::new(value[0].re, value[0].im))
}
#[cfg(all(feature = "mpi", quest_native_mpi))]
pub(super) fn write_local(
	register: &mut Register<'_, StateVector>,
	index: usize,
	value: quest_qsvt::Complex64,
) -> crate::Result<()> {
	quest_sys::write_local_qureg_amps(
		register.pin(),
		i64::try_from(index).map_err(|_| crate::Error::Overflow)?,
		&[quest_sys::QuestComplex {
			re: value.re,
			im: value.im,
		}],
	)
	.context("writing matching local amplitude")
}
pub(super) fn reserve_descriptor<'env>(
	resources: &'env RuntimeResources,
	shard: &MatchingShard,
	layout: &MatchingLayout,
) -> crate::Result<Reservation<'env>> {
	let bytes = shard
		.storage_bytes()
		.map_err(|_| crate::Error::Overflow)?
		.checked_add(
			layout
				.targets
				.capacity()
				.checked_mul(size_of::<usize>())
				.ok_or(crate::Error::Overflow)?,
		)
		.and_then(|n| n.checked_add(4096))
		.ok_or(crate::Error::Overflow)?;
	resources.reserve(bytes)
}
fn validate_single(shard: &MatchingShard) -> crate::Result<()> {
	let mut destinations = BTreeSet::new();
	for record in shard.records() {
		if !destinations.insert((record.color, record.destination)) {
			return Err(crate::Error::Value("matching permutation is not injective"));
		}
		if record.destination != record.source
			&& !shard
				.records()
				.iter()
				.any(|other| (other.color, other.source) == (record.color, record.destination))
		{
			return Err(crate::Error::Value("matching permutation is not closed"));
		}
	}
	Ok(())
}
/// Owning CPU preparation borrowing its environment and one native scratch state.
/// The supplied shard is retained; no oracle circuit or dense unitary is stored.
pub struct PreparedMatching<'env> {
	scratch: Register<'env, StateVector>,
	layout: MatchingLayout,
	shard: MatchingShard,
	_reservation: Reservation<'env>,
	_workspace_reservation: Reservation<'env>,
	workspace: local_batch::Workspace,
}
impl Environment {
	/// Prepare the complete matching unitary, retaining only its owned sparse shard.
	/// Targets are flag, ascending local system bits, ascending local color bits.
	/// # Errors
	/// Rejects distributed/GPU execution, bad layout, non-bijective completion or budgets.
	pub fn prepare_matching(
		&self,
		shard: MatchingShard,
		count: QubitCount,
		targets: Vec<usize>,
	) -> Result<PreparedMatching<'_>> {
		if self.resources.capabilities().gpu
			|| self.resources.capabilities().distributed
			|| shard.rank() != 0
			|| shard.parts() != 1
		{
			return Err(crate::Error::Value(
				"local matching preparation requires a single CPU shard",
			)
			.into());
		}
		let layout = MatchingLayout::new(shard.header(), count, targets)?;
		validate_single(&shard)?;
		let reservation = reserve_descriptor(&self.resources, &shard, &layout)?;
		let workspace_reservation = self.resources.reserve(local_batch::Workspace::bytes())?;
		let workspace = local_batch::Workspace::new()?;
		let scratch = self.resources.state_vector(count)?;
		Ok(PreparedMatching {
			scratch,
			layout,
			shard,
			_reservation: reservation,
			_workspace_reservation: workspace_reservation,
			workspace,
		})
	}
}
impl PreparedMatching<'_> {
	#[must_use]
	pub fn targets(&self) -> &[usize] {
		&self.layout.targets
	}
	#[must_use]
	pub const fn num_qubits(&self) -> QubitCount {
		self.layout.count
	}
	#[must_use]
	pub const fn shard(&self) -> &MatchingShard {
		&self.shard
	}
	/// Apply U or U† to an arbitrary whole state, coherently controlled on outer
	/// physical bits. Outer controls must be disjoint from all active targets.
	/// # Errors
	/// Rejects mismatched owner/width, invalid controls and native failures.
	pub fn apply(
		&mut self,
		register: &mut Register<'_, StateVector>,
		adjoint: bool,
		outer_mask: usize,
		outer_value: usize,
	) -> Result<()> {
		if !std::ptr::eq(register.resources(), self.scratch.resources())
			|| register.num_qubits() != self.layout.count
		{
			return Err(crate::Error::Value("matching register owner or width").into());
		}
		let (positions, outcomes) = self.layout.controls(outer_mask, outer_value)?;
		self.layout.hadamards(register, &positions, &outcomes)?;
		quest_sys::set_qureg_to_clone(self.scratch.pin(), &register.native)
			.context("staging matching permutation")?;
		self.workspace.apply(
			&self.layout,
			&self.shard,
			register,
			&mut self.scratch,
			adjoint,
			(outer_mask, outer_value),
		)?;
		quest_sys::set_qureg_to_clone(register.pin(), &self.scratch.native)
			.context("installing matching permutation")?;
		self.layout.hadamards(register, &positions, &outcomes)?;
		Ok(())
	}
}
