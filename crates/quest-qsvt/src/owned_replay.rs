//! Compact owning source descriptions and whole-unitary lazy replay.
use crate::matching::{admit, bit};
use crate::{
	Complex64, Error, LogicalSpace, MatchingEncoding, MatchingHeader, NumericalPolicy,
	OracleFragment, ReplayGate, ReplayKind, Result, StructuredStencilEncoding, TensorShiftEncoding,
};
use std::ops::Range;

/// Computational projector, stored independently of the Hilbert-space dimension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactProjector {
	pub fixed_mask: usize,
	pub fixed_value: usize,
	pub logical_range: Range<usize>,
}
impl CompactProjector {
	/// Borrow the compact description as the conventional ordered logical space.
	/// # Errors
	/// Rejects invalid bits, ranges, widths, or descriptor storage budgets.
	pub fn logical_space<Side>(
		&self,
		num_qubits: usize,
		policy: NumericalPolicy,
	) -> Result<LogicalSpace<Side>> {
		LogicalSpace::constrained_range(
			bit(num_qubits)?,
			self.fixed_mask,
			self.fixed_value,
			self.logical_range.clone(),
			policy,
		)
	}
	/// Stream disjoint bit cubes without allocating an expanded coordinate list.
	/// # Errors
	/// Rejects invalid descriptors and propagates visitor errors.
	pub fn visit_cubes(
		&self,
		num_qubits: usize,
		mut visitor: impl FnMut(usize, usize) -> Result<()>,
	) -> Result<()> {
		self.logical_space::<crate::Left>(num_qubits, NumericalPolicy::default())?;
		let full = bit(num_qubits)?
			.checked_sub(1)
			.ok_or(Error::Budget("projector width"))?;
		let free = full & !self.fixed_mask;
		let mut start = self.logical_range.start;
		while start < self.logical_range.end {
			let remaining = self
				.logical_range
				.end
				.checked_sub(start)
				.ok_or(Error::Budget("projector range"))?;
			let exponent = start.trailing_zeros().min(remaining.ilog2());
			let size =
				bit(usize::try_from(exponent).map_err(|_| Error::Budget("projector cube"))?)?;
			let varying = deposit(
				size.checked_sub(1).ok_or(Error::Budget("projector cube"))?,
				free,
			);
			visitor(full & !varying, self.fixed_value | deposit(start, free))?;
			start = start
				.checked_add(size)
				.ok_or(Error::Budget("projector range"))?;
		}
		Ok(())
	}
}
const fn deposit(mut packed: usize, mut mask: usize) -> usize {
	let mut value = 0;
	while mask != 0 {
		let low = 1usize << mask.trailing_zeros();
		if packed & 1 != 0 {
			value |= low;
		}
		packed >>= 1;
		mask &= mask.wrapping_sub(1);
	}
	value
}
/// Physical source layout, including work bits and their clean preparation sector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodingLayout {
	pub num_qubits: usize,
	pub system_mask: usize,
	pub workspace_mask: usize,
	pub clean_workspace_mask: usize,
	pub clean_workspace_value: usize,
}
/// Explicit error attestations. `None` means no independent numerical bound is supplied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EncodingErrors {
	pub preparation: Option<f64>,
	pub encoding: Option<f64>,
	pub binary64_parameters: bool,
}
/// Portable scalar identity and projector contract for one particular whole unitary.
/// Fingerprints detect accidental mismatches; they are not cryptographic proofs.
#[derive(Clone, Debug, PartialEq)]
pub struct EncodingDescriptor {
	pub rows: usize,
	pub cols: usize,
	pub normalization: f64,
	pub layout: EncodingLayout,
	pub left: CompactProjector,
	pub right: CompactProjector,
	pub errors: EncodingErrors,
	/// Declared operator provenance; arithmetic term ordering does not change it.
	pub source_identity: u64,
	/// Particular whole-unitary recipe, including color ordering and failure completion.
	pub construction_identity: u64,
}
impl EncodingDescriptor {
	/// # Errors
	/// Rejects inconsistent widths, projector dimensions, workspace or error bounds.
	pub fn validate(&self) -> Result<()> {
		let full = bit(self.layout.num_qubits)?
			.checked_sub(1)
			.ok_or(Error::Budget("encoding width"))?;
		let layout = self.layout;
		if !self.normalization.is_finite()
			|| self.normalization <= 0.0
			|| layout.system_mask & layout.workspace_mask != 0
			|| layout.system_mask | layout.workspace_mask != full
			|| layout.clean_workspace_mask & !layout.workspace_mask != 0
			|| layout.clean_workspace_value & !layout.clean_workspace_mask != 0
		{
			return Err(Error::Encoding(
				"invalid owning encoding layout/normalization",
			));
		}
		for projector in [&self.left, &self.right] {
			if projector.fixed_mask & layout.clean_workspace_mask != layout.clean_workspace_mask
				|| projector.fixed_value & layout.clean_workspace_mask
					!= layout.clean_workspace_value
			{
				return Err(Error::Encoding("projector disagrees with clean workspace"));
			}
		}
		for bound in [self.errors.preparation, self.errors.encoding]
			.into_iter()
			.flatten()
		{
			if !bound.is_finite() || bound < 0.0 {
				return Err(Error::Encoding("invalid encoding error bound"));
			}
		}
		if self
			.left
			.logical_space::<crate::Left>(layout.num_qubits, NumericalPolicy::default())?
			.logical_dimension()
			!= self.rows
			|| self
				.right
				.logical_space::<crate::Right>(layout.num_qubits, NumericalPolicy::default())?
				.logical_dimension()
				!= self.cols
		{
			return Err(Error::Encoding("owning encoding logical dimensions"));
		}
		Ok(())
	}
	/// Adapt a scalar matching manifest without retaining its source records.
	/// # Errors
	/// Rejects invalid matching widths, dimensions and normalization.
	pub fn from_matching_header(header: MatchingHeader) -> Result<Self> {
		header.validate()?;
		let system_mask = header
			.system_dimension()?
			.checked_sub(1)
			.and_then(|m| m.checked_mul(2))
			.ok_or(Error::Budget("matching layout"))?;
		let mut result = flagged_descriptor(
			header.num_qubits()?,
			system_mask,
			header.rows,
			header.cols,
			header.alpha,
			header.source_identity,
			fingerprint([
				0x4d41_5443_4849_4e47,
				header.record_digest,
				identity_word(header.record_count)?,
				header.beta.to_bits(),
				identity_word(header.num_colors)?,
			]),
		)?;
		result.errors.encoding = None;
		Ok(result)
	}
}
fn flagged_descriptor(
	width: usize,
	system_mask: usize,
	rows: usize,
	cols: usize,
	alpha: f64,
	source: u64,
	construction: u64,
) -> Result<EncodingDescriptor> {
	let workspace_mask = bit(width)?
		.checked_sub(1)
		.ok_or(Error::Budget("encoding layout"))?
		& !system_mask;
	let result = EncodingDescriptor {
		rows,
		cols,
		normalization: alpha,
		layout: EncodingLayout {
			num_qubits: width,
			system_mask,
			workspace_mask,
			clean_workspace_mask: workspace_mask,
			clean_workspace_value: 0,
		},
		left: CompactProjector {
			fixed_mask: workspace_mask,
			fixed_value: 0,
			logical_range: 0..rows,
		},
		right: CompactProjector {
			fixed_mask: workspace_mask,
			fixed_value: 0,
			logical_range: 0..cols,
		},
		errors: EncodingErrors {
			preparation: Some(0.0),
			encoding: None,
			binary64_parameters: true,
		},
		source_identity: source,
		construction_identity: construction,
	};
	result.validate()?;
	Ok(result)
}
// Feed bytes, not whole words: even numbers of binary64 sign-bit changes
// otherwise cancel identically under XOR and multiplication by an odd value.
// This remains a bounded accidental-integrity fingerprint, not authentication.
pub fn fingerprint_word(mut hash: u64, word: u64) -> u64 {
	for byte in word.to_le_bytes() {
		hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
	}
	hash
}
pub fn fingerprint(words: impl IntoIterator<Item = u64>) -> u64 {
	words
		.into_iter()
		.fold(0xcbf2_9ce4_8422_2325, fingerprint_word)
}
fn identity_word(value: usize) -> Result<u64> {
	u64::try_from(value).map_err(|_| Error::Budget("encoding identity word"))
}
pub fn shift_source_identity(encoding: &TensorShiftEncoding) -> Result<u64> {
	// Independent disjoint additions commute; operand declaration order belongs
	// to the construction recipe, not the mathematical permutation.
	let mut digest = 0u64;
	for shift in encoding.shifts() {
		if shift.offset() != 0 {
			digest = digest.wrapping_add(crate::record_fingerprint(
				0x5348_4652_4543_5632,
				[
					identity_word(shift.start())?,
					identity_word(shift.width())?,
					identity_word(shift.offset())?,
				],
			));
		}
	}
	Ok(fingerprint([
		0x5348_4653_4f55_5232,
		identity_word(encoding.num_qubits())?,
		digest,
	]))
}
fn shift_identity(encoding: &TensorShiftEncoding) -> Result<u64> {
	let mut hash = fingerprint([identity_word(encoding.num_qubits())?]);
	for shift in encoding.shifts() {
		hash = fingerprint([
			hash,
			identity_word(shift.start())?,
			identity_word(shift.width())?,
			identity_word(shift.offset())?,
		]);
	}
	Ok(hash)
}
/// Owning replay source. Implementations retain their recipes and stream bounded primitives.
///
/// The descriptor binds both operator provenance and the complete unitary construction,
/// including failure and padding sectors. Replay must agree with that immutable contract.
pub trait ReplayEncoding: Clone + std::fmt::Debug {
	/// # Errors
	/// Rejects descriptor derivation or accounting failures.
	fn descriptor(&self) -> Result<EncodingDescriptor>;
	/// # Errors
	/// Rejects integer storage-accounting overflow.
	fn retained_bytes(&self) -> Result<usize>;
	/// # Errors
	/// Propagates source and visitor errors; stops immediately on visitor failure.
	fn visit_replay(
		&self,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()>;
	/// Compatibility adapter with bounded conventional circuit storage.
	/// # Errors
	/// Rejects gate count/storage overflow and compiler admission failures.
	fn replay_oracle(&self, policy: NumericalPolicy) -> Result<OracleFragment> {
		let descriptor = self.descriptor()?;
		descriptor.validate()?;
		let mut count = 0usize;
		self.visit_replay(false, &mut |_| {
			count = count
				.checked_add(1)
				.ok_or(Error::Budget("owned replay count"))?;
			Ok(())
		})?;
		crate::replay::oracle_from_replay(
			descriptor.layout.num_qubits,
			count,
			self.retained_bytes()?,
			policy,
			|visitor| self.visit_replay(false, visitor),
		)
	}
	/// Replay under signed controls and caller-selected physical operands.
	/// # Errors
	/// Rejects malformed or overlapping operands and propagates visitor errors.
	fn visit_mapped_replay(
		&self,
		targets: &[usize],
		outer_mask: usize,
		outer_value: usize,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		validate_mapping(
			self.descriptor()?.layout.num_qubits,
			targets,
			outer_mask,
			outer_value,
		)?;
		self.visit_replay(adjoint, &mut |gate| {
			visitor(gate.mapped(targets, outer_mask, outer_value)?)
		})
	}
}
impl ReplayEncoding for MatchingEncoding {
	fn descriptor(&self) -> Result<EncodingDescriptor> {
		EncodingDescriptor::from_matching_header(MatchingHeader::from_encoding(self)?)
	}
	fn retained_bytes(&self) -> Result<usize> {
		Ok(self.resources().retained_bytes)
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		self.visit_gates(adjoint, visitor)
	}
}
impl ReplayEncoding for TensorShiftEncoding {
	fn descriptor(&self) -> Result<EncodingDescriptor> {
		let dimension = bit(self.num_qubits())?;
		let source = self.cached_source_identity();
		let mut result = flagged_descriptor(
			self.num_qubits(),
			dimension
				.checked_sub(1)
				.ok_or(Error::Budget("shift layout"))?,
			dimension,
			dimension,
			1.0,
			source,
			fingerprint([0x5348_4946_545f_5631, shift_identity(self)?]),
		)?;
		result.errors = EncodingErrors {
			preparation: Some(0.0),
			encoding: Some(0.0),
			binary64_parameters: false,
		};
		Ok(result)
	}
	fn retained_bytes(&self) -> Result<usize> {
		self.retained_bytes()
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		self.visit_gates(adjoint, visitor)
	}
}
pub fn stencil_source_identity(encoding: &StructuredStencilEncoding) -> Result<u64> {
	let mut digest = 0_u64;
	for term in encoding.terms() {
		digest = digest.wrapping_add(crate::record_fingerprint(
			0x5354_4e52_4543_5632,
			[
				term.weight().re.to_bits(),
				term.weight().im.to_bits(),
				term.shift().cached_source_identity(),
			],
		));
	}
	Ok(fingerprint([
		0x5354_4e53_4f55_5232,
		identity_word(encoding.system_qubits())?,
		digest,
	]))
}
impl ReplayEncoding for StructuredStencilEncoding {
	fn descriptor(&self) -> Result<EncodingDescriptor> {
		let dimension = bit(self.system_qubits())?;
		let mut ordered = fingerprint([identity_word(self.system_qubits())?]);
		for term in self.terms() {
			ordered = fingerprint([
				ordered,
				term.weight().re.to_bits(),
				term.weight().im.to_bits(),
				shift_identity(term.shift())?,
			]);
		}
		let source = self.cached_source_identity();
		flagged_descriptor(
			self.num_qubits(),
			dimension
				.checked_sub(1)
				.and_then(|m| m.checked_mul(2))
				.ok_or(Error::Budget("stencil layout"))?,
			dimension,
			dimension,
			self.normalization().get(),
			source,
			fingerprint([
				0x5354_454e_4349_4c31,
				ordered,
				self.beta().to_bits(),
				identity_word(self.num_colors())?,
			]),
		)
	}
	fn retained_bytes(&self) -> Result<usize> {
		self.retained_bytes()
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		self.visit_gates(adjoint, visitor)
	}
}
pub fn validate_mapping(width: usize, targets: &[usize], mask: usize, value: usize) -> Result<()> {
	if targets.len() != width || value & !mask != 0 {
		return Err(Error::Encoding("owned replay mapping"));
	}
	let mut occupied = mask;
	for &target in targets {
		let bit = bit(target)?;
		if occupied & bit != 0 {
			return Err(Error::Encoding("owned replay operand overlap"));
		}
		occupied |= bit;
	}
	Ok(())
}
pub fn admit_state(
	state: &[Complex64],
	width: usize,
	retained: usize,
	policy: NumericalPolicy,
) -> Result<()> {
	if state.len() != bit(width)? {
		return Err(Error::Encoding("owned replay state shape"));
	}
	admit(
		state
			.len()
			.checked_mul(size_of::<Complex64>())
			.and_then(|b| b.checked_add(retained))
			.ok_or(Error::Budget("owned replay state storage"))?,
		policy,
	)?;
	if state.iter().any(|z| !z.re.is_finite() || !z.im.is_finite()) {
		return Err(Error::NonFinite);
	}
	Ok(())
}
#[allow(
	clippy::arithmetic_side_effects,
	reason = "Whole-register bounded finite binary64 reference kernels"
)]
pub fn apply_gate(state: &mut [Complex64], gate: ReplayGate) -> Result<()> {
	if gate.control_value & !gate.control_mask != 0 || gate.control_mask >= state.len() {
		return Err(Error::Encoding("owned replay controls"));
	}
	if let ReplayKind::Phase(angle) = gate.kind {
		if !angle.is_finite() || gate.target.is_some() {
			return Err(Error::Encoding("owned replay scalar phase"));
		}
		let phase = Complex64::from_polar(1.0, angle);
		for (i, z) in state.iter_mut().enumerate() {
			if i & gate.control_mask == gate.control_value {
				*z *= phase;
			}
		}
		return Ok(());
	}
	let target = bit(gate.target.ok_or(Error::Encoding("owned replay target"))?)?;
	if target >= state.len() || target & gate.control_mask != 0 {
		return Err(Error::Encoding("owned replay target overlap"));
	}
	for low in 0..state.len() {
		if low & target != 0 || low & gate.control_mask != gate.control_value {
			continue;
		}
		let high = low | target;
		let a = *state
			.get(low)
			.ok_or(Error::Encoding("owned replay index"))?;
		let b = *state
			.get(high)
			.ok_or(Error::Encoding("owned replay index"))?;
		let (x, y) = match gate.kind {
			ReplayKind::H => (
				(a + b) * std::f64::consts::FRAC_1_SQRT_2,
				(a - b) * std::f64::consts::FRAC_1_SQRT_2,
			),
			ReplayKind::X => (b, a),
			ReplayKind::Ry(angle) => {
				if !angle.is_finite() {
					return Err(Error::NonFinite);
				}
				let (s, c) = (0.5 * angle).sin_cos();
				(a * c - b * s, a * s + b * c)
			}
			ReplayKind::Phase(_) => return Err(Error::Encoding("owned replay primitive")),
		};
		*state
			.get_mut(low)
			.ok_or(Error::Encoding("owned replay index"))? = x;
		*state
			.get_mut(high)
			.ok_or(Error::Encoding("owned replay index"))? = y;
	}
	Ok(())
}
