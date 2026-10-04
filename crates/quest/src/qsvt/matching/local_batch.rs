//! Reused bounded native transfers for single-rank matching application.
use super::{MatchingLayout, Result};
use crate::{Register, StateVector, error::BackendResult, values::reserve_vec};
use quest_qsvt::{Complex64, MatchingColumn, MatchingShard};
const PAIRS: usize = 64;

pub(super) struct Workspace {
	indices: Vec<i64>,
	outputs: Vec<i64>,
	columns: Vec<MatchingColumn>,
	values: Vec<quest_sys::QuestComplex>,
}
impl Workspace {
	pub(super) const fn bytes() -> usize {
		PAIRS.saturating_mul(
			size_of::<MatchingColumn>().saturating_add(
				size_of::<i64>()
					.saturating_mul(4)
					.saturating_add(size_of::<quest_sys::QuestComplex>().saturating_mul(2)),
			),
		)
	}
	pub(super) fn new() -> crate::Result<Self> {
		Ok(Self {
			indices: reserve_vec(PAIRS.saturating_mul(2))?,
			outputs: reserve_vec(PAIRS.saturating_mul(2))?,
			columns: reserve_vec(PAIRS)?,
			values: reserve_vec(PAIRS.saturating_mul(2))?,
		})
	}
	pub(super) fn apply(
		&mut self,
		layout: &MatchingLayout,
		shard: &MatchingShard,
		input: &Register<'_, StateVector>,
		output: &mut Register<'_, StateVector>,
		adjoint: bool,
		outer: (usize, usize),
	) -> Result<()> {
		let flag = layout.flag()?;
		self.clear();
		for basis in 0..input.dimension() {
			if basis & flag != 0 || basis & outer.0 != outer.1 {
				continue;
			}
			let system = layout.extract(basis, layout.system_range())?;
			let color = layout.extract(basis, layout.color_range())?;
			let column = shard.column(color, system)?;
			let permuted = layout.replace_system(basis, column.destination)?;
			let (source, destination) = if adjoint {
				(permuted, basis)
			} else {
				(basis, permuted)
			};
			for index in [source, source | flag] {
				self.indices
					.push(i64::try_from(index).map_err(|_| crate::Error::Overflow)?);
			}
			for index in [destination, destination | flag] {
				self.outputs
					.push(i64::try_from(index).map_err(|_| crate::Error::Overflow)?);
			}
			self.columns.push(column);
			if self.columns.len() == PAIRS {
				self.flush(input, output, adjoint)?;
			}
		}
		self.flush(input, output, adjoint)
	}
	fn clear(&mut self) {
		self.indices.clear();
		self.outputs.clear();
		self.columns.clear();
		self.values.clear();
	}
	fn flush(
		&mut self,
		input: &Register<'_, StateVector>,
		output: &mut Register<'_, StateVector>,
		adjoint: bool,
	) -> Result<()> {
		if self.columns.is_empty() {
			return Ok(());
		}
		self.values.resize(
			self.indices.len(),
			quest_sys::QuestComplex { re: 0.0, im: 0.0 },
		);
		quest_sys::read_local_indexed_qureg_amps(&input.native, &self.indices, &mut self.values)
			.context("reading matching local batch")?;
		for (column, pair) in self.columns.iter().zip(self.values.as_chunks_mut::<2>().0) {
			let [zero, one] = pair;
			let rotated = column.rotate(
				[
					Complex64::new(zero.re, zero.im),
					Complex64::new(one.re, one.im),
				],
				adjoint,
			);
			*zero = quest_sys::QuestComplex {
				re: rotated[0].re,
				im: rotated[0].im,
			};
			*one = quest_sys::QuestComplex {
				re: rotated[1].re,
				im: rotated[1].im,
			};
		}
		quest_sys::write_local_indexed_qureg_amps(output.pin(), &self.outputs, &self.values)
			.context("writing matching local batch")?;
		self.clear();
		Ok(())
	}
}
