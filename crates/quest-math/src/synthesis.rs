//! Independent compositional checker for matrix reduction and elementary lowering.
// Phase exponents are admitted below eight, and all row divisors are admitted
// nonzero powers of two. BigInt arithmetic is intentionally exact and unbounded
// only within the separately enforced coefficient/byte limits.
#![allow(clippy::arithmetic_side_effects)]
use crate::{Cyclotomic, Error, ExactMatrix, Limits, Operation, Result, Sequence};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BasisIndex(pub usize);

/// Left row operations, in chronological order, reducing the target to identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowOperation {
    Phase(BasisIndex, u8),
    Swap(BasisIndex, BasisIndex),
    Hadamard(BasisIndex, BasisIndex),
    ImaginarySwap(BasisIndex, BasisIndex, bool),
    ImaginaryHadamard(BasisIndex, BasisIndex, u8, bool),
    OppositePhase(BasisIndex, BasisIndex, u8),
    EmbeddedPhase { qubit: usize, power: u8 },
}

/// Untrusted witness. Each block implements the inverse of a reduction step,
/// in reverse step order. A clean ancilla is the new highest-numbered wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthesisProof {
    pub reduction: Vec<RowOperation>,
    pub block_ends: Vec<usize>,
    pub clean_ancilla: bool,
}

/// Constructed only after exact replay of both reduction and elementary blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthesisCertificate {
    target: ExactMatrix,
    candidate: Sequence,
    proof: SynthesisProof,
    work: u64,
}
impl SynthesisCertificate {
    #[must_use]
    pub const fn target(&self) -> &ExactMatrix {
        &self.target
    }
    #[must_use]
    pub const fn candidate(&self) -> &Sequence {
        &self.candidate
    }
    #[must_use]
    pub const fn proof(&self) -> &SynthesisProof {
        &self.proof
    }
    #[must_use]
    pub const fn work(&self) -> u64 {
        self.work
    }
}

struct Work {
    used: u64,
    limit: u64,
}
impl Work {
    fn charge(&mut self, count: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(crate::types::size(count)?)
            .ok_or_else(|| Error::Resource("proof work".into()))?;
        if self.used > self.limit {
            return Err(crate::types::budget("proof work", self.used, self.limit));
        }
        Ok(())
    }
}

fn dimension(qubits: usize) -> Result<usize> {
    1usize
        .checked_shl(u32::try_from(qubits).map_err(|_| Error::Resource("proof dimension".into()))?)
        .ok_or_else(|| Error::Resource("proof dimension".into()))
}

/// Admit combined dense scratch, proof and elementary output storage.
///
/// The conservative estimate includes coefficient storage and replay temporaries.
/// # Errors
/// Rejects width, count overflow, or allocation beyond the caller's byte budget.
pub fn admit_synthesis_storage(
    qubits: usize,
    proof_steps: usize,
    output_gates: usize,
    limits: Limits,
) -> Result<u64> {
    if qubits > limits.qubits {
        return Err(Error::Invalid("synthesis storage width".into()));
    }
    let count = proof_steps
        .checked_add(output_gates)
        .ok_or_else(|| Error::Resource("synthesis storage count".into()))?;
    crate::matrix::memory(count, dimension(qubits)?, limits)
}

fn admit_row(step: RowOperation, dim: usize) -> Result<()> {
    match step {
        RowOperation::Phase(BasisIndex(a), power) if a < dim && power < 8 => Ok(()),
        RowOperation::Swap(BasisIndex(a), BasisIndex(b))
        | RowOperation::Hadamard(BasisIndex(a), BasisIndex(b))
            if a < dim && b < dim && a != b =>
        {
            Ok(())
        }
        RowOperation::ImaginarySwap(BasisIndex(a), BasisIndex(b), _)
            if a < dim && b < dim && a != b =>
        {
            Ok(())
        }
        RowOperation::ImaginaryHadamard(BasisIndex(a), BasisIndex(b), m, _)
        | RowOperation::OppositePhase(BasisIndex(a), BasisIndex(b), m)
            if a < dim && b < dim && a != b && m < 8 =>
        {
            Ok(())
        }
        RowOperation::EmbeddedPhase { qubit, power } if dimension(qubit)? < dim && power < 8 => {
            Ok(())
        }
        _ => Err(Error::Invalid("invalid reduction row or phase".into())),
    }
}

#[allow(clippy::many_single_char_names)]
fn row_pair(
    step: RowOperation,
    a: &Cyclotomic,
    b: &Cyclotomic,
    limits: Limits,
) -> Result<(Cyclotomic, Cyclotomic)> {
    match step {
        RowOperation::Phase(_, power) => {
            Ok((Cyclotomic::omega(power).checked_mul(a, limits)?, b.clone()))
        }
        RowOperation::Swap(..) => Ok((b.clone(), a.clone())),
        RowOperation::Hadamard(..) => {
            let h = Cyclotomic::new([0.into(), 1.into(), 0.into(), (-1).into()], 1, limits)?;
            Ok((
                a.checked_add(b, limits)?.checked_mul(&h, limits)?,
                a.checked_add(&Cyclotomic::omega(4).checked_mul(b, limits)?, limits)?
                    .checked_mul(&h, limits)?,
            ))
        }
        RowOperation::ImaginarySwap(_, _, inverse) => {
            let phase = Cyclotomic::omega(if inverse { 6 } else { 2 });
            Ok((phase.checked_mul(b, limits)?, phase.checked_mul(a, limits)?))
        }
        RowOperation::ImaginaryHadamard(_, _, m, inverse) => {
            let phased_b = Cyclotomic::omega(m).checked_mul(b, limits)?;
            let (x, y) = row_pair(
                RowOperation::Hadamard(BasisIndex(0), BasisIndex(1)),
                a,
                &phased_b,
                limits,
            )?;
            let phase = Cyclotomic::omega(if inverse { 6 } else { 2 });
            Ok((
                phase.checked_mul(&x, limits)?,
                phase.checked_mul(
                    &Cyclotomic::omega((8 - m) & 7).checked_mul(&y, limits)?,
                    limits,
                )?,
            ))
        }
        RowOperation::OppositePhase(_, _, power) => Ok((
            Cyclotomic::omega(power).checked_mul(a, limits)?,
            Cyclotomic::omega((8 - power) & 7).checked_mul(b, limits)?,
        )),
        RowOperation::EmbeddedPhase { .. } => {
            Err(Error::Invalid("embedded phase is not a row pair".into()))
        }
    }
}

const fn row_indices(step: RowOperation) -> (usize, usize) {
    match step {
        RowOperation::Phase(BasisIndex(a), _) => (a, a),
        RowOperation::Swap(BasisIndex(a), BasisIndex(b))
        | RowOperation::Hadamard(BasisIndex(a), BasisIndex(b))
        | RowOperation::ImaginarySwap(BasisIndex(a), BasisIndex(b), _)
        | RowOperation::ImaginaryHadamard(BasisIndex(a), BasisIndex(b), _, _)
        | RowOperation::OppositePhase(BasisIndex(a), BasisIndex(b), _) => (a, b),
        RowOperation::EmbeddedPhase { .. } => (0, 0),
    }
}

fn apply_row(
    values: &mut [Cyclotomic],
    dim: usize,
    step: RowOperation,
    limits: Limits,
    work: &mut Work,
) -> Result<()> {
    if let RowOperation::EmbeddedPhase { qubit, power } = step {
        let bit = dimension(qubit)?;
        work.charge(values.len())?;
        for (index, value) in values.iter_mut().enumerate() {
            if (index / dim) & bit != 0 {
                *value = Cyclotomic::omega(power).checked_mul(value, limits)?;
            }
        }
        return Ok(());
    }
    let (a, b) = row_indices(step);
    work.charge(dim)?;
    for col in 0..dim {
        let ai = a
            .checked_mul(dim)
            .and_then(|n| n.checked_add(col))
            .ok_or_else(|| Error::Resource("row offset".into()))?;
        let bi = b
            .checked_mul(dim)
            .and_then(|n| n.checked_add(col))
            .ok_or_else(|| Error::Resource("row offset".into()))?;
        let av = values
            .get(ai)
            .ok_or_else(|| Error::Invalid("row shape".into()))?;
        let bv = values
            .get(bi)
            .ok_or_else(|| Error::Invalid("row shape".into()))?;
        let (next_a, next_b) = row_pair(step, av, bv, limits)?;
        *values
            .get_mut(ai)
            .ok_or_else(|| Error::Invalid("row shape".into()))? = next_a;
        if a != b {
            *values
                .get_mut(bi)
                .ok_or_else(|| Error::Invalid("row shape".into()))? = next_b;
        }
    }
    Ok(())
}

fn elementary_on_vector(
    values: &mut [Cyclotomic],
    operation: &Operation,
    limits: Limits,
    work: &mut Work,
) -> Result<()> {
    let (local_dim, matrix) = crate::matrix::local_gate(operation.gate, limits)?;
    let masks = operation
        .targets
        .iter()
        .map(|&q| dimension(q))
        .collect::<Result<Vec<_>>>()?;
    let occupied = masks.iter().fold(0, |mask, bit| mask | bit);
    for base in 0..values.len() {
        if base & occupied != 0 {
            continue;
        }
        work.charge(
            local_dim
                .checked_mul(local_dim)
                .ok_or_else(|| Error::Resource("local work".into()))?,
        )?;
        let mut input = Vec::with_capacity(local_dim);
        for i in 0..local_dim {
            let index = masks.iter().enumerate().fold(base, |index, (k, bit)| {
                if i & (1 << k) != 0 {
                    index | bit
                } else {
                    index
                }
            });
            input.push(
                values
                    .get(index)
                    .ok_or_else(|| Error::Invalid("local vector index".into()))?
                    .clone(),
            );
        }
        for row in 0..local_dim {
            let mut value = Cyclotomic::zero();
            for (col, entry) in input.iter().enumerate() {
                let offset = row
                    .checked_mul(local_dim)
                    .and_then(|n| n.checked_add(col))
                    .ok_or_else(|| Error::Resource("local offset".into()))?;
                value = value.checked_add(
                    &matrix
                        .get(offset)
                        .ok_or_else(|| Error::Invalid("local matrix".into()))?
                        .checked_mul(entry, limits)?,
                    limits,
                )?;
            }
            let index = masks.iter().enumerate().fold(base, |index, (k, bit)| {
                if row & (1 << k) != 0 {
                    index | bit
                } else {
                    index
                }
            });
            *values
                .get_mut(index)
                .ok_or_else(|| Error::Invalid("local vector index".into()))? = value;
        }
    }
    Ok(())
}

/// Verify reduction to identity, each local elementary lowering, and their
/// composition.
///
/// Only columns of the clean-input embedding J are compared:
/// every block must satisfy C J = J V, including return of the clean wire.
/// # Errors
/// Rejects malformed evidence, wrong matrices, controls, resources, or work limits.
fn admit_proof(
    target: &ExactMatrix,
    candidate: &Sequence,
    proof: &SynthesisProof,
    limits: Limits,
) -> Result<(usize, usize)> {
    let expected_qubits = target
        .qubits
        .checked_add(usize::from(proof.clean_ancilla))
        .ok_or_else(|| Error::Resource("ancilla width".into()))?;
    if candidate.qubits != expected_qubits || candidate.qubits > limits.qubits {
        return Err(Error::Invalid("synthesis ancilla contract".into()));
    }
    if proof.reduction.len() > limits.gates
        || proof.block_ends.len() != proof.reduction.len()
        || candidate.operations.len() > limits.gates
    {
        return Err(Error::Invalid("synthesis proof or output length".into()));
    }
    let dim = dimension(target.qubits)?;
    let out_dim = dimension(candidate.qubits)?;
    admit_synthesis_storage(
        candidate.qubits,
        proof.reduction.len(),
        candidate.operations.len(),
        limits,
    )?;
    target.admit(limits)?;
    for operation in &candidate.operations {
        if !operation.controls.is_empty()
            || operation.targets.len() != operation.gate.target_count()
        {
            return Err(Error::Invalid("synthesis output must be elementary".into()));
        }
        let mut seen = 0usize;
        for &q in &operation.targets {
            if q >= candidate.qubits {
                return Err(Error::Invalid("synthesis gate wire".into()));
            }
            let bit = dimension(q)?;
            if seen & bit != 0 {
                return Err(Error::Invalid("synthesis repeated wire".into()));
            }
            seen |= bit;
        }
    }
    let mut end = 0;
    for &next in &proof.block_ends {
        if next < end || next > candidate.operations.len() {
            return Err(Error::Invalid("synthesis block partition".into()));
        }
        end = next;
    }
    if end != candidate.operations.len() {
        return Err(Error::Invalid("uncovered synthesis output".into()));
    }
    for &step in &proof.reduction {
        admit_row(step, dim)?;
    }
    Ok((dim, out_dim))
}

/// Independently verify the row reduction and each elementary block under the
/// exact clean-input/clean-return contract `C J = J U`.
/// # Errors
/// Rejects malformed evidence, unequal matrices and exhausted resources.
#[allow(clippy::too_many_lines)] // Keep the complete independent replay contract in one audit path.
pub fn verify_synthesis(
    target: &ExactMatrix,
    candidate: &Sequence,
    proof: &SynthesisProof,
    limits: Limits,
    work_limit: u64,
) -> Result<SynthesisCertificate> {
    let mut work = Work {
        used: 0,
        limit: work_limit,
    };
    // Admission scans the target, elementary operands and proof partitions.
    // Charge those scans before examining untrusted variable-length inputs.
    work.charge(
        target
            .entries
            .len()
            .checked_add(
                candidate
                    .operations
                    .len()
                    .checked_mul(3)
                    .ok_or_else(|| Error::Resource("proof admission work".into()))?,
            )
            .and_then(|n| n.checked_add(proof.reduction.len().checked_mul(2)?))
            .and_then(|n| n.checked_add(proof.block_ends.len()))
            .ok_or_else(|| Error::Resource("proof admission work".into()))?,
    )?;
    let (dim, out_dim) = admit_proof(target, candidate, proof, limits)?;
    work.charge(target.entries.len())?;
    let mut reduced = target.entries.clone();
    for &step in &proof.reduction {
        apply_row(&mut reduced, dim, step, limits, &mut work)?;
    }
    work.charge(reduced.len())?;
    for (index, value) in reduced.iter().enumerate() {
        let expected = if index / dim == index % dim {
            Cyclotomic::one()
        } else {
            Cyclotomic::zero()
        };
        if *value != expected {
            return Err(Error::NotEquivalent);
        }
    }
    let mut start = 0;
    for (&step, &end) in proof.reduction.iter().rev().zip(&proof.block_ends) {
        let inverse = match step {
            RowOperation::Phase(row, power) => RowOperation::Phase(row, (8 - power) & 7),
            RowOperation::ImaginarySwap(a, b, inverse) => {
                RowOperation::ImaginarySwap(a, b, !inverse)
            }
            RowOperation::ImaginaryHadamard(a, b, m, inverse) => {
                RowOperation::ImaginaryHadamard(a, b, m, !inverse)
            }
            RowOperation::OppositePhase(a, b, m) => RowOperation::OppositePhase(a, b, (8 - m) & 7),
            RowOperation::EmbeddedPhase { qubit, power } => RowOperation::EmbeddedPhase {
                qubit,
                power: (8 - power) & 7,
            },
            other => other,
        };
        let (a, b) = row_indices(inverse);
        for input in 0..dim {
            work.charge(out_dim)?;
            let mut vector = vec![Cyclotomic::zero(); out_dim];
            *vector
                .get_mut(input)
                .ok_or_else(|| Error::Invalid("embedding input".into()))? = Cyclotomic::one();
            for operation in candidate
                .operations
                .get(start..end)
                .ok_or_else(|| Error::Invalid("block slice".into()))?
            {
                elementary_on_vector(&mut vector, operation, limits, &mut work)?;
            }
            let va = if a == input {
                Cyclotomic::one()
            } else {
                Cyclotomic::zero()
            };
            let vb = if b == input {
                Cyclotomic::one()
            } else {
                Cyclotomic::zero()
            };
            let (want_a, want_b) = if matches!(inverse, RowOperation::EmbeddedPhase { .. }) {
                (va, vb)
            } else {
                row_pair(inverse, &va, &vb, limits)?
            };
            for (row, actual) in vector.iter().enumerate() {
                let wanted = if let RowOperation::EmbeddedPhase { qubit, power } = inverse {
                    if row != input {
                        Cyclotomic::zero()
                    } else if row & dimension(qubit)? != 0 {
                        Cyclotomic::omega(power)
                    } else {
                        Cyclotomic::one()
                    }
                } else if row == a {
                    want_a.clone()
                } else if row == b {
                    want_b.clone()
                } else if row == input {
                    Cyclotomic::one()
                } else {
                    Cyclotomic::zero()
                };
                if *actual != wanted {
                    return Err(Error::NotEquivalent);
                }
            }
        }
        start = end;
    }
    work.charge(target.entries.len())?;
    work.charge(candidate.operations.len())?;
    work.charge(proof.reduction.len())?;
    Ok(SynthesisCertificate {
        target: target.clone(),
        candidate: candidate.clone(),
        proof: proof.clone(),
        work: work.used,
    })
}
