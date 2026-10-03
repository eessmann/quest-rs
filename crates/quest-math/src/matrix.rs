use crate::{Cyclotomic, Error, Gate, Limits, Operation, Result, Sequence};
use dashu_base::BitTest;
use dashu_int::IBig;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactMatrix {
	pub(crate) qubits: usize,
	pub(crate) entries: Vec<Cyclotomic>,
}
impl ExactMatrix {
	/// Admit a dense row-major matrix; this checks storage, not unitarity.
	/// # Errors
	/// Rejects dimensions, coefficients and storage exceeding request bounds.
	pub fn from_entries(qubits: usize, entries: Vec<Cyclotomic>, limits: Limits) -> Result<Self> {
		let result = Self { qubits, entries };
		result.admit(limits)?;
		Ok(result)
	}
	/// Re-admit existing storage before cloning or arithmetic.
	/// # Errors
	/// Rejects dimensions, coefficients and storage exceeding request bounds.
	pub fn admit(&self, limits: Limits) -> Result<()> {
		let Self { qubits, entries } = self;
		let qubits = *qubits;
		if qubits > limits.qubits {
			return Err(Error::Invalid("matrix width exceeds limit".into()));
		}
		let dimension = mask(qubits)?;
		let count = dimension
			.checked_mul(dimension)
			.ok_or_else(|| Error::Resource("matrix shape".into()))?;
		if entries.len() != count {
			return Err(Error::Invalid("matrix shape".into()));
		}
		memory(0, dimension, limits)?;
		for entry in entries {
			entry.checked_add(&Cyclotomic::zero(), limits)?;
		}
		Ok(())
	}
	#[must_use]
	pub const fn qubits(&self) -> usize {
		self.qubits
	}
	#[must_use]
	pub fn entries(&self) -> &[Cyclotomic] {
		&self.entries
	}
	/// Construct a checked one- or two-qubit identity.
	/// # Errors
	/// Rejects unsupported width or exhausted matrix resources.
	pub fn identity(qubits: usize, limits: Limits) -> Result<Self> {
		small_dimension(qubits, limits)?;
		reconstruct(
			&Sequence {
				qubits,
				operations: Vec::new(),
			},
			limits,
		)
	}
	/// Construct the embedded full matrix of one admitted operation.
	/// # Errors
	/// Rejects unsupported width, operands or exhausted resources.
	pub fn for_operation(qubits: usize, operation: &Operation, limits: Limits) -> Result<Self> {
		small_dimension(qubits, limits)?;
		reconstruct(
			&Sequence {
				qubits,
				operations: vec![operation.clone()],
			},
			limits,
		)
	}
	/// Checked full-phase left matrix product, `self * right`.
	/// # Errors
	/// Rejects mismatched widths or arithmetic/resource excesses.
	pub fn multiply(&self, right: &Self, limits: Limits) -> Result<Self> {
		let dimension = small_dimension(self.qubits, limits)?;
		let entries_count = dimension
			.checked_mul(dimension)
			.ok_or_else(|| Error::Resource("MITM matrix dimension".into()))?;
		if self.qubits != right.qubits
			|| self.entries.len() != entries_count
			|| right.entries.len() != entries_count
		{
			return Err(Error::Invalid("small matrix shape".into()));
		}
		memory(0, dimension, limits)?;
		let mut entries = zeros(entries_count)?;
		for row in 0..dimension {
			for col in 0..dimension {
				let mut value = Cyclotomic::zero();
				for inner in 0..dimension {
					let product = get(&self.entries, dimension, row, inner)?
						.checked_mul(get(&right.entries, dimension, inner, col)?, limits)?;
					value = value.checked_add(&product, limits)?;
				}
				*get_mut(&mut entries, dimension, row, col)? = value;
			}
		}
		Ok(Self {
			qubits: self.qubits,
			entries,
		})
	}
	/// Checked conjugate transpose.
	/// # Errors
	/// Rejects invalid small-matrix shape or arithmetic/resource excesses.
	pub fn adjoint(&self, limits: Limits) -> Result<Self> {
		let dimension = small_dimension(self.qubits, limits)?;
		let entries_count = dimension
			.checked_mul(dimension)
			.ok_or_else(|| Error::Resource("MITM matrix dimension".into()))?;
		if self.entries.len() != entries_count {
			return Err(Error::Invalid("small matrix shape".into()));
		}
		memory(0, dimension, limits)?;
		let mut entries = zeros(entries_count)?;
		for row in 0..dimension {
			for col in 0..dimension {
				*get_mut(&mut entries, dimension, row, col)? =
					get(&self.entries, dimension, col, row)?.conjugated(limits)?;
			}
		}
		Ok(Self {
			qubits: self.qubits,
			entries,
		})
	}
	/// Canonical key of every normalized entry, including global phase.
	/// # Errors
	/// Rejects invalid shape, coefficient limits or allocation failure.
	pub fn full_phase_key(&self, limits: Limits) -> Result<MatrixKey> {
		let dimension = small_dimension(self.qubits, limits)?;
		let entries_count = dimension
			.checked_mul(dimension)
			.ok_or_else(|| Error::Resource("MITM matrix dimension".into()))?;
		if self.entries.len() != entries_count {
			return Err(Error::Invalid("small matrix shape".into()));
		}
		let mut entries = Vec::new();
		entries
			.try_reserve_exact(self.entries.len())
			.map_err(|_| Error::Resource("matrix key allocation".into()))?;
		for value in &self.entries {
			if value.coefficients().iter().any(|coefficient| {
				u64::try_from(coefficient.bit_len()).unwrap_or(u64::MAX) > limits.coefficient_bits
			}) || u64::from(value.denominator_exponent()) > limits.coefficient_bits
			{
				return Err(crate::types::budget(
					"coefficient bits",
					limits.coefficient_bits.saturating_add(1),
					limits.coefficient_bits,
				));
			}
			entries.push(EntryKey {
				denominator_exponent: value.denominator_exponent(),
				coefficients: value.coefficients().clone(),
			});
		}
		Ok(MatrixKey {
			qubits: self.qubits,
			entries,
		})
	}
}

fn small_dimension(qubits: usize, limits: Limits) -> Result<usize> {
	if !(1..=2).contains(&qubits) || qubits > limits.qubits {
		return Err(Error::Invalid("MITM matrix width".into()));
	}
	mask(qubits)
}

/// Hashable full-phase canonical matrix identity for one or two qubits.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatrixKey {
	qubits: usize,
	entries: Vec<EntryKey>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EntryKey {
	denominator_exponent: u32,
	coefficients: [IBig; 4],
}
impl MatrixKey {
	/// Conservative owned key bytes, including independent bigint limbs.
	/// # Errors
	/// Rejects storage arithmetic overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		let mut bytes = std::mem::size_of::<Self>()
			.checked_add(
				self.entries
					.capacity()
					.checked_mul(std::mem::size_of::<EntryKey>())
					.ok_or_else(|| Error::Resource("matrix key storage".into()))?,
			)
			.ok_or_else(|| Error::Resource("matrix key storage".into()))?;
		for entry in &self.entries {
			for coefficient in &entry.coefficients {
				let limb_bytes = usize::try_from(
					u64::try_from(coefficient.bit_len())
						.unwrap_or(u64::MAX)
						.div_ceil(64),
				)
				.map_err(|_| Error::Resource("matrix key storage".into()))?
				.checked_mul(16)
				.and_then(|n| n.checked_add(24))
				.ok_or_else(|| Error::Resource("matrix key storage".into()))?;
				bytes = bytes
					.checked_add(limb_bytes)
					.ok_or_else(|| Error::Resource("matrix key storage".into()))?;
			}
		}
		Ok(bytes)
	}
}
/// Owned exact identities. Construction requires full matrix equality including phase.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ExactCertificate {
	candidate: Sequence,
	target: Sequence,
}
impl ExactCertificate {
	#[must_use]
	pub const fn candidate(&self) -> &Sequence {
		&self.candidate
	}
	#[must_use]
	pub const fn target(&self) -> &Sequence {
		&self.target
	}
}
/// Admit and reconstruct a full exact matrix in little-endian basis order.
/// # Errors
/// Rejects invalid operands, widths and arithmetic/allocation resource excesses.
pub fn reconstruct(sequence: &Sequence, limits: Limits) -> Result<ExactMatrix> {
	let dimension = admit(sequence, limits)?;
	let entries = dimension
		.checked_mul(dimension)
		.ok_or_else(|| Error::Resource("matrix dimension".into()))?;
	let mut matrix = zeros(entries)?;
	for index in 0..dimension {
		*get_mut(&mut matrix, dimension, index, index)? = Cyclotomic::one();
	}
	for operation in &sequence.operations {
		let (local_dimension, local) = local_gate(operation.gate, limits)?;
		let mut next = zeros(entries)?;
		for row in 0..dimension {
			let active = operation
				.controls
				.iter()
				.try_fold(true, |active, control| {
					Ok::<_, Error>(
						active && ((row & mask(control.qubit)? != 0) == control.positive),
					)
				})?;
			if !active {
				for column in 0..dimension {
					*get_mut(&mut next, dimension, row, column)? =
						get(&matrix, dimension, row, column)?.clone();
				}
				continue;
			}
			let output = extract(row, &operation.targets)?;
			for input in 0..local_dimension {
				let coefficient = get(&local, local_dimension, output, input)?;
				if coefficient.is_zero() {
					continue;
				}
				let source = replace(row, input, &operation.targets)?;
				for column in 0..dimension {
					let product = coefficient
						.checked_mul(get(&matrix, dimension, source, column)?, limits)?;
					let destination = get_mut(&mut next, dimension, row, column)?;
					*destination = destination.checked_add(&product, limits)?;
				}
			}
		}
		matrix = next;
	}
	Ok(ExactMatrix {
		qubits: sequence.qubits,
		entries: matrix,
	})
}
/// Independently reconstruct both sequences and compare every exact entry.
/// # Errors
/// Rejects unequal matrices, invalid inputs and resource excesses.
pub fn verify_exact(
	candidate: &Sequence,
	target: &Sequence,
	limits: Limits,
) -> Result<ExactCertificate> {
	admit(candidate, limits)?;
	admit(target, limits)?;
	if candidate.qubits != target.qubits
		|| reconstruct(candidate, limits)? != reconstruct(target, limits)?
	{
		return Err(Error::NotEquivalent);
	}
	Ok(ExactCertificate {
		candidate: candidate.clone(),
		target: target.clone(),
	})
}
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PhaseRecovery {
	sequence: Sequence,
	phase_power: u8,
	certificate: ExactCertificate,
}
impl PhaseRecovery {
	#[must_use]
	pub const fn sequence(&self) -> &Sequence {
		&self.sequence
	}
	#[must_use]
	pub const fn phase_power(&self) -> u8 {
		self.phase_power
	}
	#[must_use]
	pub const fn certificate(&self) -> &ExactCertificate {
		&self.certificate
	}
}
/// Recover an omitted scalar only by testing all eight exact eighth roots.
/// # Errors
/// Rejects nonmatching matrices and resource excesses, including correction gates.
pub fn recover_eighth_root_phase(
	candidate: &Sequence,
	target: &Sequence,
	limits: Limits,
) -> Result<PhaseRecovery> {
	admit(candidate, limits)?;
	admit(target, limits)?;
	if candidate.qubits != target.qubits {
		return Err(Error::PhaseNotRecovered);
	}
	let candidate_matrix = reconstruct(candidate, limits)?;
	let target_matrix = reconstruct(target, limits)?;
	for power in 0..8u8 {
		let phase = Cyclotomic::omega(power);
		let mut equal = true;
		for (left, right) in candidate_matrix.entries.iter().zip(&target_matrix.entries) {
			if phase.checked_mul(left, limits)? != *right {
				equal = false;
				break;
			}
		}
		if equal {
			let count = candidate
				.operations
				.len()
				.checked_add(usize::from(power))
				.ok_or_else(|| Error::Resource("phase correction gate count".into()))?;
			if count > limits.gates {
				return Err(crate::types::budget(
					"gates",
					crate::types::size(count)?,
					crate::types::size(limits.gates)?,
				));
			}
			memory(count, mask(candidate.qubits)?, limits)?;
			let mut sequence = candidate.clone();
			sequence
				.operations
				.try_reserve(usize::from(power))
				.map_err(|_| Error::Resource("phase correction allocation".into()))?;
			for _ in 0..power {
				sequence.operations.push(Operation {
					gate: Gate::W,
					targets: vec![],
					controls: vec![],
				});
			}
			let certificate = ExactCertificate {
				candidate: sequence.clone(),
				target: target.clone(),
			};
			return Ok(PhaseRecovery {
				sequence,
				phase_power: power,
				certificate,
			});
		}
	}
	Err(Error::PhaseNotRecovered)
}
pub fn admit(sequence: &Sequence, limits: Limits) -> Result<usize> {
	let width_limit = limits.qubits.min(4);
	if sequence.qubits > width_limit {
		return Err(crate::types::budget(
			"qubits",
			crate::types::size(sequence.qubits)?,
			crate::types::size(width_limit)?,
		));
	}
	if sequence.operations.len() > limits.gates {
		return Err(crate::types::budget(
			"gates",
			crate::types::size(sequence.operations.len())?,
			crate::types::size(limits.gates)?,
		));
	}
	if limits.coefficient_bits == 0 {
		return Err(crate::types::budget("coefficient bits", 1, 0));
	}
	let dimension = mask(sequence.qubits)?;
	memory(sequence.operations.len(), dimension, limits)?;
	for operation in &sequence.operations {
		if operation.targets.len() != operation.gate.target_count() {
			return Err(Error::Invalid("gate target count".into()));
		}
		if operation.controls.len() > sequence.qubits {
			return Err(Error::Invalid("too many controls".into()));
		}
		let mut occupied = 0;
		for qubit in operation
			.targets
			.iter()
			.copied()
			.chain(operation.controls.iter().map(|control| control.qubit))
		{
			if qubit >= sequence.qubits {
				return Err(Error::Invalid("operand outside register".into()));
			}
			let bit = mask(qubit)?;
			if occupied & bit != 0 {
				return Err(Error::Invalid("duplicate or overlapping operands".into()));
			}
			occupied |= bit;
		}
	}
	Ok(dimension)
}
/// Three matrix buffers and conservative room for simultaneous owned sequence copies.
pub fn memory(gates: usize, dimension: usize, limits: Limits) -> Result<u64> {
	let storage = dimension
		.checked_mul(dimension)
		.and_then(|n| n.checked_mul(12))
		.and_then(|n| n.checked_add(128))
		.ok_or_else(|| Error::Resource("matrix storage".into()))?;
	let matrix = crate::types::allocation_bytes(limits.coefficient_bits, storage)?;
	let sequences = crate::types::size(gates)?
		.checked_mul(1024)
		.ok_or_else(|| Error::Resource("sequence bytes".into()))?;
	let requested = matrix
		.checked_add(sequences)
		.ok_or_else(|| Error::Resource("combined allocation bytes".into()))?;
	let limit = crate::types::size(limits.bytes)?;
	if requested > limit {
		return Err(crate::types::budget("allocation bytes", requested, limit));
	}
	Ok(requested)
}

fn zeros(length: usize) -> Result<Vec<Cyclotomic>> {
	let mut values = Vec::new();
	values
		.try_reserve_exact(length)
		.map_err(|_| Error::Resource("matrix allocation".into()))?;
	values.resize_with(length, Cyclotomic::zero);
	Ok(values)
}
fn offset(dimension: usize, row: usize, column: usize) -> Result<usize> {
	row.checked_mul(dimension)
		.and_then(|value| value.checked_add(column))
		.ok_or_else(|| Error::Resource("matrix index".into()))
}
fn get(values: &[Cyclotomic], dimension: usize, row: usize, column: usize) -> Result<&Cyclotomic> {
	values
		.get(offset(dimension, row, column)?)
		.ok_or_else(|| Error::Resource("matrix index outside allocation".into()))
}
fn get_mut(
	values: &mut [Cyclotomic],
	dimension: usize,
	row: usize,
	column: usize,
) -> Result<&mut Cyclotomic> {
	values
		.get_mut(offset(dimension, row, column)?)
		.ok_or_else(|| Error::Resource("matrix index outside allocation".into()))
}
fn mask(bit: usize) -> Result<usize> {
	1usize
		.checked_shl(u32::try_from(bit).map_err(|_| Error::Resource("bit position".into()))?)
		.ok_or_else(|| Error::Resource("bit mask".into()))
}
fn extract(global: usize, targets: &[usize]) -> Result<usize> {
	let mut local = 0;
	for (position, target) in targets.iter().enumerate() {
		if global & mask(*target)? != 0 {
			local |= mask(position)?;
		}
	}
	Ok(local)
}
fn replace(mut global: usize, local: usize, targets: &[usize]) -> Result<usize> {
	for (position, target) in targets.iter().enumerate() {
		let bit = mask(*target)?;
		if local & mask(position)? == 0 {
			global &= !bit;
		} else {
			global |= bit;
		}
	}
	Ok(global)
}
// Shared with the independent synthesis verifier, never exposed from the crate.
#[allow(clippy::redundant_pub_crate)]
pub(super) fn local_gate(gate: Gate, limits: Limits) -> Result<(usize, Vec<Cyclotomic>)> {
	let one = Cyclotomic::one();
	let zero = Cyclotomic::zero();
	let negative = Cyclotomic::omega(4);
	let entries = match gate {
		Gate::W => return Ok((1, vec![Cyclotomic::omega(1)])),
		Gate::H => {
			let half = Cyclotomic::new([0, 1, 0, -1].map(IBig::from), 1, limits)?;
			vec![
				half.clone(),
				half.clone(),
				half.clone(),
				half.checked_mul(&negative, limits)?,
			]
		}
		Gate::X => vec![zero.clone(), one.clone(), one, zero],
		Gate::Y => vec![
			zero.clone(),
			Cyclotomic::omega(6),
			Cyclotomic::omega(2),
			zero,
		],
		Gate::Z => vec![one, zero.clone(), zero, negative],
		Gate::S | Gate::Sdg | Gate::T | Gate::Tdg => {
			let power = match gate {
				Gate::S => 2,
				Gate::Sdg => 6,
				Gate::T => 1,
				_ => 7,
			};
			vec![one, zero.clone(), zero, Cyclotomic::omega(power)]
		}
		Gate::Cx | Gate::Cz | Gate::Swap => {
			let mut entries = zeros(16)?;
			let rows = match gate {
				Gate::Cx => [0, 3, 2, 1],
				Gate::Swap => [0, 2, 1, 3],
				_ => [0, 1, 2, 3],
			};
			for (column, row) in rows.into_iter().enumerate() {
				*get_mut(&mut entries, 4, row, column)? = if gate == Gate::Cz && column == 3 {
					negative.clone()
				} else {
					one.clone()
				};
			}
			return Ok((4, entries));
		}
	};
	Ok((2, entries))
}
