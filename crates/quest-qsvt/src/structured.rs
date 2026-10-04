//! Reversible arithmetic tensor shifts, without enumerated permutations or PREP tables.
use crate::{
	EncodingBuilder, Error, ExplicitUnitaryPremise, Left, LogicalSpace, Normalization,
	NumericalPolicy, OracleFragment, ProjectedEncoding, ReplayGate, ReplayKind, Result, Right,
	matching::{admit, bit},
};
use quest_compile::{Control, ControlState, Gate, QuantumRegionBuilder};
use std::sync::Arc;
/// Modular addition on one contiguous range of physical bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShiftRegister {
	start: usize,
	width: usize,
	offset: usize,
}
impl ShiftRegister {
	/// Define |x> -> |x + offset mod 2^width> on bits start..start+width.
	///
	/// # Errors
	/// Rejects zero/overflowing widths and noncanonical offsets.
	pub fn new(start: usize, width: usize, offset: usize) -> Result<Self> {
		if width == 0 || offset >= bit(width)? {
			return Err(Error::Encoding("structured shift width/offset"));
		}
		bit(start
			.checked_add(width)
			.ok_or(Error::Budget("structured shift range"))?)?;
		Ok(Self {
			start,
			width,
			offset,
		})
	}
	/// Lowest physical bit.
	#[must_use]
	pub const fn start(self) -> usize {
		self.start
	}
	/// Number of bits in the shifted register.
	#[must_use]
	pub const fn width(self) -> usize {
		self.width
	}
	/// Canonical modular offset.
	#[must_use]
	pub const fn offset(self) -> usize {
		self.offset
	}
	fn mask(self) -> Result<usize> {
		bit(self.width)?
			.checked_sub(1)
			.and_then(|mask| mask.checked_shl(u32::try_from(self.start).ok()?))
			.ok_or(Error::Budget("structured shift mask"))
	}
}
/// Unit-normalized coherent encoding of independent modular tensor shifts.
///
/// The source is the stated reversible arithmetic permutation. This primitive
/// makes no claim to prepare coefficients or encode a weighted stencil sum.
#[derive(Clone, Debug)]
pub struct TensorShiftEncoding {
	num_qubits: usize,
	shifts: Arc<[ShiftRegister]>,
	gate_count: usize,
}
impl TensorShiftEncoding {
	/// Freeze disjoint modular shift recipes without enumerating basis indices.
	///
	/// # Errors
	/// Rejects overlapping/out-of-range registers, storage and width overflow.
	pub fn new(
		num_qubits: usize,
		shifts: Vec<ShiftRegister>,
		policy: NumericalPolicy,
	) -> Result<Self> {
		bit(num_qubits)?;
		let bytes = shifts
			.capacity()
			.checked_mul(size_of::<ShiftRegister>())
			.and_then(|b| b.checked_mul(2))
			.and_then(|b| b.checked_add(size_of::<Self>()))
			.ok_or(Error::Budget("structured shift storage"))?;
		admit(bytes, policy)?;
		let mut occupied = 0_usize;
		for &shift in &shifts {
			if shift
				.start
				.checked_add(shift.width)
				.is_none_or(|end| end > num_qubits)
			{
				return Err(Error::Encoding("structured shift register exceeds width"));
			}
			let mask = shift.mask()?;
			if occupied & mask != 0 {
				return Err(Error::Encoding("structured tensor shifts overlap"));
			}
			occupied |= mask;
		}
		let mut result = Self {
			num_qubits,
			shifts: Arc::from(shifts),
			gate_count: 0,
		};
		let mut count = 0_usize;
		result.visit_gates(false, |_| {
			count = count
				.checked_add(1)
				.ok_or(Error::Budget("structured gate count"))?;
			Ok(())
		})?;
		result.gate_count = count;
		Ok(result)
	}
	/// Retained arithmetic recipes and metadata, excluding allocator bookkeeping.
	/// # Errors
	/// Rejects accounting overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		self.shifts
			.len()
			.checked_mul(size_of::<ShiftRegister>())
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(2_usize.checked_mul(size_of::<usize>())?))
			.ok_or(Error::Budget("structured shift storage"))
	}
	/// Physical width.
	#[must_use]
	pub const fn num_qubits(&self) -> usize {
		self.num_qubits
	}
	/// Compact arithmetic source terms.
	#[must_use]
	pub fn shifts(&self) -> &[ShiftRegister] {
		&self.shifts
	}
	/// Number of streamed X primitives.
	#[must_use]
	pub const fn replay_gates(&self) -> usize {
		self.gate_count
	}
	/// Exact modular mapping for one basis index, preserving spectator bits.
	///
	/// # Errors
	/// Rejects indices outside the stated register and width overflow.
	pub fn map_index(&self, index: usize, adjoint: bool) -> Result<usize> {
		if index >= bit(self.num_qubits)? {
			return Err(Error::Encoding("structured basis index"));
		}
		let mut output = index;
		for &shift in &*self.shifts {
			let dimension = bit(shift.width)?;
			let mask = shift.mask()?;
			let local = (index & mask) >> shift.start;
			let offset = if adjoint {
				dimension
					.checked_sub(shift.offset)
					.ok_or(Error::Budget("structured inverse offset"))?
					& dimension
						.checked_sub(1)
						.ok_or(Error::Budget("structured inverse modulus"))?
			} else {
				shift.offset
			};
			let mapped = local.wrapping_add(offset)
				& dimension
					.checked_sub(1)
					.ok_or(Error::Budget("structured shift modulus"))?;
			output = (output & !mask)
				| mapped
					.checked_shl(
						u32::try_from(shift.start)
							.map_err(|_| Error::Budget("structured shift position"))?,
					)
					.ok_or(Error::Budget("structured shift position"))?;
		}
		Ok(output)
	}
	/// Replay a reversible ripple-carry addition and its exact reverse gate order.
	///
	/// # Errors
	/// Propagates visitor rejection and checked control-bit overflow.
	pub fn visit_gates(
		&self,
		adjoint: bool,
		mut visitor: impl FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		for ordinal in 0..self.shifts.len() {
			let index = if adjoint {
				self.shifts
					.len()
					.checked_sub(ordinal)
					.and_then(|n| n.checked_sub(1))
					.ok_or(Error::Budget("structured reverse register"))?
			} else {
				ordinal
			};
			let shift = *self
				.shifts
				.get(index)
				.ok_or(Error::Encoding("structured register"))?;
			for offset_ordinal in 0..shift.width {
				let low = if adjoint {
					shift
						.width
						.checked_sub(offset_ordinal)
						.and_then(|n| n.checked_sub(1))
						.ok_or(Error::Budget("structured reverse offset"))?
				} else {
					offset_ordinal
				};
				if shift.offset & bit(low)? == 0 {
					continue;
				}
				for target_ordinal in low..shift.width {
					let local_target = if adjoint {
						target_ordinal
					} else {
						shift
							.width
							.checked_sub(
								target_ordinal
									.checked_sub(low)
									.ok_or(Error::Budget("structured target order"))?,
							)
							.and_then(|n| n.checked_sub(1))
							.ok_or(Error::Budget("structured target order"))?
					};
					let target = shift
						.start
						.checked_add(local_target)
						.ok_or(Error::Budget("structured target"))?;
					let mut controls = 0_usize;
					for local in low..local_target {
						controls |= bit(shift
							.start
							.checked_add(local)
							.ok_or(Error::Budget("structured carry control"))?)?;
					}
					visitor(ReplayGate {
						kind: ReplayKind::X,
						target: Some(target),
						control_mask: controls,
						control_value: controls,
					})?;
				}
			}
		}
		Ok(())
	}
	/// Replay on remapped target bits under signed outer controls.
	///
	/// # Errors
	/// Rejects overlapping/repeated targets, invalid controls and visitor errors.
	pub fn visit_mapped_gates(
		&self,
		targets: &[usize],
		outer_mask: usize,
		outer_value: usize,
		adjoint: bool,
		mut visitor: impl FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		if targets.len() != self.num_qubits || outer_value & !outer_mask != 0 {
			return Err(Error::Encoding("structured replay mapping"));
		}
		let mut occupied = 0_usize;
		for &target in targets {
			let mask = bit(target)?;
			if occupied & mask != 0 || outer_mask & mask != 0 {
				return Err(Error::Encoding("structured replay operand overlap"));
			}
			occupied |= mask;
		}
		self.visit_gates(adjoint, |gate| {
			visitor(gate.mapped(targets, outer_mask, outer_value)?)
		})
	}
	/// Expand a small conventional oracle using instruction storage admission.
	///
	/// # Errors
	/// Rejects circuit storage and compiler construction failures.
	pub fn to_oracle(&self, policy: NumericalPolicy) -> Result<OracleFragment> {
		let bytes = self
			.gate_count
			.checked_mul(
				self.num_qubits
					.checked_mul(128)
					.and_then(|n| n.checked_add(2048))
					.ok_or(Error::Budget("structured circuit estimate"))?,
			)
			.ok_or(Error::Budget("structured circuit estimate"))?;
		admit(bytes, policy)?;
		let mut builder = QuantumRegionBuilder::new(self.num_qubits, 0)?;
		self.visit_gates(false, |gate| {
			let mut controls = crate::matching::reserve(
				usize::try_from(gate.control_mask.count_ones())
					.map_err(|_| Error::Budget("structured controls"))?,
			)?;
			for bit_position in 0..self.num_qubits {
				if gate.control_mask & bit(bit_position)? != 0 {
					controls.push(Control::new(
						builder.qubit(bit_position)?,
						ControlState::One,
					));
				}
			}
			builder.gate(
				Gate::X,
				&[builder.qubit(gate.target.ok_or(Error::Encoding("structured target"))?)?],
				&controls,
			)?;
			Ok(())
		})?;
		Ok(OracleFragment::builder(builder.finish()?.bind(&[])?)
			.matrix_tolerance(0.0)?
			.matrix_policy(policy.matrix_policy())
			.build()?)
	}
	/// Expose the arithmetic permutation as an alpha=1 projected encoding.
	///
	/// # Errors
	/// Rejects compact logical-space or instruction-storage admission failures.
	pub fn projected_encoding(&self, policy: NumericalPolicy) -> Result<ProjectedEncoding> {
		let dimension = bit(self.num_qubits)?;
		EncodingBuilder::new().oracle(self.to_oracle(policy)?)
            .left(LogicalSpace::<Left>::logical_range(dimension,0..dimension,policy)?)
            .right(LogicalSpace::<Right>::logical_range(dimension,0..dimension,policy)?)
            .normalization(Normalization::new(1.0)?.get())?.policy(policy)
            .unitarity_assumption(ExplicitUnitaryPremise::new("Disjoint modular tensor shifts are reversible arithmetic permutations composed of controlled X gates")?).build()
	}
}
