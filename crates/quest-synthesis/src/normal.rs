//! Matsumoto--Amano normal form via the exact SO(3) channel denominator.
//! Mathematical reference: arXiv:1312.6584, section 4.
use crate::{Budget, Result, SynthesisError, SynthesisOptions};
use quest_math::{Cyclotomic, ExactMatrix, Gate, Operation, Sequence, reconstruct, verify_exact};
use std::collections::{HashSet, VecDeque};

// Conservative retained-storage accounting. A Clifford matrix plus its hash key
// has sixteen small ring coefficients each; four 1024-byte units cover both,
// container overhead and spare capacity. Every retained word operation and
// syllable receives its own additional 1024-byte unit. Popped queue entries are
// deliberately not credited back, so this bounds the simultaneous live set.
struct Storage {
	bytes: u64,
	limit: u64,
}
impl Storage {
	fn reserve(&mut self, units: usize) -> Result<()> {
		self.bytes = u64::try_from(units)
			.ok()
			.and_then(|n| n.checked_mul(1024))
			.and_then(|n| self.bytes.checked_add(n))
			.ok_or(SynthesisError::Budget {
				resource: "normal-form storage",
			})?;
		if self.bytes > self.limit {
			return Err(SynthesisError::Budget {
				resource: "normal-form storage",
			});
		}
		Ok(())
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syllable {
	Ht,
	Sht,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalForm {
	leading_t: bool,
	syllables: Vec<Syllable>,
	clifford: Sequence,
	sequence: Sequence,
}
impl NormalForm {
	#[must_use]
	pub const fn leading_t(&self) -> bool {
		self.leading_t
	}
	#[must_use]
	pub fn syllables(&self) -> &[Syllable] {
		&self.syllables
	}
	#[must_use]
	pub const fn clifford(&self) -> &Sequence {
		&self.clifford
	}
	#[must_use]
	pub const fn sequence(&self) -> &Sequence {
		&self.sequence
	}
}
fn operation(gate: Gate) -> Operation {
	Operation {
		gate,
		targets: if gate == Gate::W { vec![] } else { vec![0] },
		controls: vec![],
	}
}
fn gates(gates: &[Gate]) -> Sequence {
	Sequence {
		qubits: 1,
		operations: gates.iter().copied().map(operation).collect(),
	}
}
fn channel_exponent(matrix: &ExactMatrix, budget: &mut Budget) -> Result<u32> {
	let limits = budget.options.limits;
	let adjoint = matrix.adjoint(limits)?;
	let mut exponent = 0;
	for a in [Gate::X, Gate::Y, Gate::Z] {
		for b in [Gate::X, Gate::Y, Gate::Z] {
			budget.charge(64)?;
			let pauli_a = reconstruct(&gates(&[a]), limits)?;
			let pauli_b = reconstruct(&gates(&[b]), limits)?;
			let product = pauli_a
				.multiply(matrix, limits)?
				.multiply(&pauli_b, limits)?
				.multiply(&adjoint, limits)?;
			let trace = product
				.entries()
				.first()
				.ok_or(SynthesisError::Invalid("channel matrix"))?
				.checked_add(
					product
						.entries()
						.get(3)
						.ok_or(SynthesisError::Invalid("channel matrix"))?,
					limits,
				)?;
			let entry = Cyclotomic::new(
				trace.coefficients().clone(),
				trace
					.denominator_exponent()
					.checked_add(1)
					.ok_or(SynthesisError::Budget {
						resource: "channel denominator",
					})?,
				limits,
			)?;
			exponent = exponent.max(entry.least_sqrt2_exponent(limits)?.0);
		}
	}
	Ok(exponent)
}
fn clifford_word(
	target: &ExactMatrix,
	budget: &mut Budget,
	storage: &mut Storage,
) -> Result<Sequence> {
	let limits = budget.options.limits;
	storage.reserve(4)?;
	let mut queue = VecDeque::from([(ExactMatrix::identity(1, limits)?, gates(&[]))]);
	let mut seen = HashSet::new();
	seen.insert(ExactMatrix::identity(1, limits)?.full_phase_key(limits)?);
	let generators = [Gate::H, Gate::S];
	while let Some((matrix, word)) = queue.pop_front() {
		budget.charge(1)?;
		if matrix == *target {
			return Ok(word);
		}
		for gate in generators {
			budget.charge(8)?;
			storage.reserve(4)?;
			let next = reconstruct(&gates(&[gate]), limits)?.multiply(&matrix, limits)?;
			if seen.insert(next.full_phase_key(limits)?) {
				if seen.len() > 192 {
					return Err(SynthesisError::Invalid("Clifford group closure"));
				}
				storage.reserve(word.operations.len().checked_add(1).ok_or(
					SynthesisError::Budget {
						resource: "normal-form storage",
					},
				)?)?;
				let mut sequence = word.clone();
				sequence.operations.push(operation(gate));
				queue.push_back((next, sequence));
			}
		}
	}
	Err(SynthesisError::Invalid(
		"non-Clifford zero-denominator channel",
	))
}

/// Canonical `(T|epsilon)(HT|SHT)* C` normal form, including scalar phase.
/// The Clifford suffix is a deterministic breadth-first H/S word.
/// # Errors
/// Rejects non-one-qubit words, arithmetic excess, and logical work exhaustion.
#[allow(clippy::needless_pass_by_value, clippy::too_many_lines)] // Complete normal-form admission and reconstruction audit path.
pub fn normalize_one_qubit(input: &Sequence, options: SynthesisOptions) -> Result<NormalForm> {
	if input.qubits != 1 {
		return Err(SynthesisError::Invalid("normal form requires one qubit"));
	}
	let mut budget = Budget {
		options: options.clone(),
		used: 0,
	};
	budget.charge(0)?;
	let mut storage = Storage {
		bytes: quest_math::admit_synthesis_storage(1, 0, input.operations.len(), options.limits)?,
		limit: u64::try_from(options.limits.bytes).map_err(|_| SynthesisError::Budget {
			resource: "normal-form storage",
		})?,
	};
	budget.charge(
		input
			.operations
			.len()
			.checked_add(1)
			.and_then(|n| n.checked_mul(64))
			.ok_or(SynthesisError::Budget {
				resource: "normal-form reconstruction work",
			})?,
	)?;
	let mut matrix = reconstruct(input, options.limits)?;
	let mut exponent = channel_exponent(&matrix, &mut budget)?;
	let mut leading_t = false;
	let mut syllables = Vec::new();
	let mut first = true;
	while exponent != 0 {
		let mut chosen = None;
		for kind in 0..3 {
			if kind == 0 && !first {
				continue;
			}
			let inverse = match kind {
				0 => gates(&[Gate::Tdg]),
				1 => gates(&[Gate::H, Gate::Tdg]),
				_ => gates(&[Gate::Sdg, Gate::H, Gate::Tdg]),
			};
			let next = reconstruct(&inverse, options.limits)?.multiply(&matrix, options.limits)?;
			if channel_exponent(&next, &mut budget)? < exponent {
				chosen = Some((kind, next));
				break;
			}
		}
		let (kind, next) = chosen.ok_or(SynthesisError::Invalid("channel normal-form residue"))?;
		if kind == 0 {
			leading_t = true;
		} else {
			storage.reserve(1)?;
			syllables.push(if kind == 1 {
				Syllable::Ht
			} else {
				Syllable::Sht
			});
		}
		matrix = next;
		exponent = exponent
			.checked_sub(1)
			.ok_or(SynthesisError::Invalid("normal form exponent"))?;
		first = false;
	}
	let clifford = clifford_word(&matrix, &mut budget, &mut storage)?;
	storage.reserve(clifford.operations.len())?;
	let mut sequence = clifford.clone();
	for syllable in syllables.iter().rev() {
		storage.reserve(if *syllable == Syllable::Ht { 2 } else { 3 })?;
		budget.output(
			sequence.operations.len(),
			if *syllable == Syllable::Ht { 2 } else { 3 },
		)?;
		sequence
			.operations
			.extend(gates(&[Gate::T, Gate::H]).operations);
		if *syllable == Syllable::Sht {
			sequence.operations.push(operation(Gate::S));
		}
	}
	if leading_t {
		storage.reserve(1)?;
		budget.output(sequence.operations.len(), 1)?;
		sequence.operations.push(operation(Gate::T));
	}
	budget.charge(
		sequence
			.operations
			.len()
			.checked_add(input.operations.len())
			.and_then(|n| n.checked_add(2))
			.and_then(|n| n.checked_mul(64))
			.ok_or(SynthesisError::Budget {
				resource: "normal-form verification work",
			})?,
	)?;
	verify_exact(&sequence, input, options.limits)?;
	budget.charge(0)?;
	Ok(NormalForm {
		leading_t,
		syllables,
		clifford,
		sequence,
	})
}
