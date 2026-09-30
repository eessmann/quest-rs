//! Giles--Selinger (arXiv:1212.0506), lemmas 19--21 and 24.
//! This implementation uses determinant-one row operations in every reduction.
// Paper variables and exact BigInt formulas are retained for auditability.
// Matrix dimensions and phase exponents are admitted before machine arithmetic.
#![allow(clippy::many_single_char_names, clippy::arithmetic_side_effects)]
use crate::{AncillaPolicy, Budget, ExactSynthesis, Result, SynthesisError, SynthesisOptions};
use quest_math::{
    BasisIndex, Cyclotomic, ExactMatrix, RowOperation, Sequence, Sqrt2Exponent, SynthesisProof,
    verify_synthesis,
};

fn half_root(budget: &Budget) -> Result<Cyclotomic> {
    Ok(Cyclotomic::new(
        [0.into(), 1.into(), 0.into(), (-1).into()],
        1,
        budget.options.limits,
    )?)
}
fn pair(
    a: &Cyclotomic,
    b: &Cyclotomic,
    m: u8,
    su: bool,
    budget: &mut Budget,
) -> Result<(Cyclotomic, Cyclotomic)> {
    budget.charge(1)?;
    let limits = budget.options.limits;
    let h = half_root(budget)?;
    let b = Cyclotomic::omega(m).checked_mul(b, limits)?;
    let mut x = a.checked_add(&b, limits)?.checked_mul(&h, limits)?;
    let mut y = a
        .checked_add(&b.checked_mul(&Cyclotomic::omega(4), limits)?, limits)?
        .checked_mul(&h, limits)?;
    if su {
        x = x.checked_mul(&Cyclotomic::omega(2), limits)?;
        y = y.checked_mul(&Cyclotomic::omega((10 - m) & 7), limits)?;
    }
    Ok((x, y))
}
fn exponent(a: &Cyclotomic, b: &Cyclotomic, budget: &Budget) -> Result<u32> {
    Ok(a.least_sqrt2_exponent(budget.options.limits)?
        .0
        .max(b.least_sqrt2_exponent(budget.options.limits)?.0))
}
fn index(dim: usize, row: usize, col: usize) -> Result<usize> {
    row.checked_mul(dim)
        .and_then(|n| n.checked_add(col))
        .ok_or(SynthesisError::Budget {
            resource: "matrix offset",
        })
}
fn entry(values: &[Cyclotomic], dim: usize, row: usize, col: usize) -> Result<&Cyclotomic> {
    values
        .get(index(dim, row, col)?)
        .ok_or(SynthesisError::Invalid("matrix shape"))
}
fn put(
    values: &mut [Cyclotomic],
    dim: usize,
    row: usize,
    col: usize,
    value: Cyclotomic,
) -> Result<()> {
    *values
        .get_mut(index(dim, row, col)?)
        .ok_or(SynthesisError::Invalid("matrix shape"))? = value;
    Ok(())
}
#[allow(clippy::too_many_lines)] // Exhaustive row semantics are kept together for audit.
fn apply(
    values: &mut [Cyclotomic],
    dim: usize,
    step: RowOperation,
    trace: &mut Vec<RowOperation>,
    budget: &mut Budget,
) -> Result<()> {
    if trace.len() >= budget.options.max_proof_steps || trace.len() >= budget.options.limits.gates {
        return Err(SynthesisError::Budget {
            resource: "proof steps",
        });
    }
    budget.charge(dim)?;
    quest_math::admit_synthesis_storage(
        usize::try_from(dim.ilog2()).map_err(|_| SynthesisError::Invalid("matrix dimension"))?,
        trace.len().checked_add(1).ok_or(SynthesisError::Budget {
            resource: "proof storage",
        })?,
        0,
        budget.options.limits,
    )?;
    trace.try_reserve(1).map_err(|_| SynthesisError::Budget {
        resource: "proof allocation",
    })?;
    let limits = budget.options.limits;
    match step {
        RowOperation::Swap(BasisIndex(a), BasisIndex(b)) => {
            for col in 0..dim {
                let x = entry(values, dim, b, col)?.clone();
                let y = entry(values, dim, a, col)?.clone();
                put(values, dim, a, col, x)?;
                put(values, dim, b, col, y)?;
            }
        }
        RowOperation::Hadamard(BasisIndex(a), BasisIndex(b)) => {
            for col in 0..dim {
                let (x, y) = pair(
                    entry(values, dim, a, col)?,
                    entry(values, dim, b, col)?,
                    0,
                    false,
                    budget,
                )?;
                put(values, dim, a, col, x)?;
                put(values, dim, b, col, y)?;
            }
        }
        RowOperation::Phase(BasisIndex(a), power) => {
            for col in 0..dim {
                put(
                    values,
                    dim,
                    a,
                    col,
                    Cyclotomic::omega(power).checked_mul(entry(values, dim, a, col)?, limits)?,
                )?;
            }
        }
        RowOperation::EmbeddedPhase { qubit, power } => {
            for row in 0..dim {
                if row & (1usize << qubit) != 0 {
                    budget.charge(dim)?;
                    for col in 0..dim {
                        put(
                            values,
                            dim,
                            row,
                            col,
                            Cyclotomic::omega(power)
                                .checked_mul(entry(values, dim, row, col)?, limits)?,
                        )?;
                    }
                }
            }
        }
        RowOperation::ImaginarySwap(BasisIndex(a), BasisIndex(b), false) => {
            for col in 0..dim {
                let x = Cyclotomic::omega(2).checked_mul(entry(values, dim, b, col)?, limits)?;
                let y = Cyclotomic::omega(2).checked_mul(entry(values, dim, a, col)?, limits)?;
                put(values, dim, a, col, x)?;
                put(values, dim, b, col, y)?;
            }
        }
        RowOperation::ImaginaryHadamard(BasisIndex(a), BasisIndex(b), m, false) => {
            for col in 0..dim {
                let (x, y) = pair(
                    entry(values, dim, a, col)?,
                    entry(values, dim, b, col)?,
                    m,
                    true,
                    budget,
                )?;
                put(values, dim, a, col, x)?;
                put(values, dim, b, col, y)?;
            }
        }
        RowOperation::OppositePhase(BasisIndex(a), BasisIndex(b), power) => {
            for col in 0..dim {
                let x =
                    Cyclotomic::omega(power).checked_mul(entry(values, dim, a, col)?, limits)?;
                let y = Cyclotomic::omega((8 - power) & 7)
                    .checked_mul(entry(values, dim, b, col)?, limits)?;
                put(values, dim, a, col, x)?;
                put(values, dim, b, col, y)?;
            }
        }
        _ => return Err(SynthesisError::Invalid("generator row operation")),
    }
    trace.push(step);
    Ok(())
}

fn reduce_denominators(
    values: &mut [Cyclotomic],
    dim: usize,
    column: usize,
    su: bool,
    trace: &mut Vec<RowOperation>,
    budget: &mut Budget,
) -> Result<()> {
    loop {
        budget.charge(dim - column)?;
        let mut k = 0;
        for row in column..dim {
            k = k.max(
                entry(values, dim, row, column)?
                    .least_sqrt2_exponent(budget.options.limits)?
                    .0,
            );
        }
        if k == 0 {
            break;
        }
        budget.charge(dim)?;
        let mut first = None;
        let mut found = None;
        for row in column..dim {
            let residue = entry(values, dim, row, column)?
                .residue_at(Sqrt2Exponent(k), budget.options.limits)?;
            if residue.reducible() {
                continue;
            }
            if let Some((a, norm)) = first {
                if norm == residue.norm() {
                    found = Some((a, row));
                    break;
                }
            } else {
                first = Some((row, residue.norm()));
            }
        }
        let (a, b) = found.ok_or(SynthesisError::NotUnitary)?;
        let av = entry(values, dim, a, column)?;
        let bv = entry(values, dim, b, column)?;
        let mut chosen = None;
        for m in 0..4 {
            let (x, y) = pair(av, bv, m, su, budget)?;
            if exponent(&x, &y, budget)? < k {
                chosen = Some((m, None));
                break;
            }
        }
        if chosen.is_none() {
            'outer: for m in 0..4 {
                let (x, y) = pair(av, bv, m, su, budget)?;
                if exponent(&x, &y, budget)? > k {
                    continue;
                }
                for n in 0..4 {
                    let (x, y) = pair(&x, &y, n, su, budget)?;
                    if exponent(&x, &y, budget)? < k {
                        chosen = Some((m, Some(n)));
                        break 'outer;
                    }
                }
            }
        }
        let (m, n) = chosen.ok_or(SynthesisError::NotUnitary)?;
        for power in std::iter::once(m).chain(n) {
            if su {
                apply(
                    values,
                    dim,
                    RowOperation::ImaginaryHadamard(BasisIndex(a), BasisIndex(b), power, false),
                    trace,
                    budget,
                )?;
            } else {
                if power != 0 {
                    apply(
                        values,
                        dim,
                        RowOperation::Phase(BasisIndex(b), power),
                        trace,
                        budget,
                    )?;
                }
                apply(
                    values,
                    dim,
                    RowOperation::Hadamard(BasisIndex(a), BasisIndex(b)),
                    trace,
                    budget,
                )?;
            }
        }
    }
    Ok(())
}

fn reduce_column(
    values: &mut [Cyclotomic],
    dim: usize,
    column: usize,
    su: bool,
    trace: &mut Vec<RowOperation>,
    budget: &mut Budget,
) -> Result<()> {
    reduce_denominators(values, dim, column, su, trace, budget)?;
    budget.charge(dim - column)?;
    let mut nonzero = None;
    for row in column..dim {
        if !entry(values, dim, row, column)?.is_zero() {
            if nonzero.is_some() {
                return Err(SynthesisError::NotUnitary);
            }
            nonzero = Some(row);
        }
    }
    let row = nonzero.ok_or(SynthesisError::NotUnitary)?;
    if row != column {
        apply(
            values,
            dim,
            if su {
                RowOperation::ImaginarySwap(BasisIndex(column), BasisIndex(row), false)
            } else {
                RowOperation::Swap(BasisIndex(column), BasisIndex(row))
            },
            trace,
            budget,
        )?;
    }
    let diagonal = entry(values, dim, column, column)?;
    let phase = diagonal
        .eighth_root_phase()
        .ok_or(SynthesisError::NotUnitary)?
        .power();
    if column + 1 < dim && phase != 0 {
        apply(
            values,
            dim,
            if su {
                RowOperation::OppositePhase(
                    BasisIndex(column),
                    BasisIndex(column + 1),
                    (8 - phase) & 7,
                )
            } else {
                RowOperation::Phase(BasisIndex(column), (8 - phase) & 7)
            },
            trace,
            budget,
        )?;
    }
    Ok(())
}

fn reduce(
    target: &ExactMatrix,
    prefix: Option<u8>,
    budget: &mut Budget,
) -> Result<(Vec<RowOperation>, u8)> {
    let dim = 1usize
        .checked_shl(
            u32::try_from(target.qubits()).map_err(|_| SynthesisError::Invalid("matrix width"))?,
        )
        .ok_or(SynthesisError::Budget {
            resource: "matrix dimension",
        })?;
    budget.charge(target.entries().len())?;
    let mut values = target.entries().to_vec();
    let mut trace = Vec::new();
    let su = target.qubits() > 1;
    if let Some(power) = prefix {
        apply(
            &mut values,
            dim,
            RowOperation::EmbeddedPhase { qubit: 0, power },
            &mut trace,
            budget,
        )?;
    }
    for column in 0..dim {
        reduce_column(&mut values, dim, column, su, &mut trace, budget)?;
    }
    budget.charge(values.len())?;
    for row in 0..dim {
        for col in 0..dim {
            if row != col && !entry(&values, dim, row, col)?.is_zero() {
                return Err(SynthesisError::NotUnitary);
            }
        }
    }
    let last = dim - 1;
    let diagonal = entry(&values, dim, last, last)?;
    let phase = diagonal
        .eighth_root_phase()
        .ok_or(SynthesisError::NotUnitary)?
        .power();
    if phase != 0 {
        apply(
            &mut values,
            dim,
            RowOperation::Phase(BasisIndex(last), (8 - phase) & 7),
            &mut trace,
            budget,
        )?;
    }
    Ok((trace, phase))
}

/// Exact full-phase synthesis into actual elementary Clifford+T gates.
/// No-ancilla requests use determinant-one reduction after an admitted T prefix.
/// # Errors
/// Separates nonunitarity and determinant rejection from resource exhaustion.
#[allow(clippy::needless_pass_by_value)] // Public entry points own request configuration.
pub fn synthesize_matrix(
    target: &ExactMatrix,
    options: SynthesisOptions,
) -> Result<ExactSynthesis> {
    let mut budget = Budget {
        options: options.clone(),
        used: 0,
    };
    budget.charge(1)?;
    if target.qubits() == 0 {
        return Err(SynthesisError::Invalid(
            "synthesis requires at least one data wire",
        ));
    }
    budget.charge(target.entries().len())?;
    target.admit(options.limits)?;
    let (mut trace, phase) = reduce(target, None, &mut budget)?;
    let factor = if target.qubits() >= 4 {
        8
    } else {
        1u8 << (target.qubits() - 1)
    };
    let clean = options.ancilla_policy == AncillaPolicy::AllowOneClean && phase % factor != 0;
    if !clean && target.qubits() > 1 {
        if phase % factor != 0 {
            return Err(SynthesisError::AncillaRequired {
                qubits: target.qubits(),
                determinant_power: phase,
            });
        }
        if phase != 0 {
            trace = reduce(target, Some((8 - phase / factor) & 7), &mut budget)?.0;
        }
    }
    let qubits = target.qubits() + usize::from(clean);
    if qubits > options.limits.qubits {
        return Err(SynthesisError::Budget {
            resource: "qubits including ancilla",
        });
    }
    let reserved = quest_math::admit_synthesis_storage(qubits, trace.len(), 0, options.limits)?;
    // During lowering, output admission uses the remaining byte allowance;
    // all matrix/proof storage has already been conservatively reserved.
    budget.options.limits.bytes = options
        .limits
        .bytes
        .checked_sub(
            usize::try_from(reserved).map_err(|_| SynthesisError::Budget {
                resource: "combined storage",
            })?,
        )
        .ok_or(SynthesisError::Budget {
            resource: "combined storage",
        })?;
    let mut operations = Vec::new();
    let mut block_ends = Vec::new();
    for &step in trace.iter().rev() {
        crate::lowering::inverse_step(step, target.qubits(), clean, &mut operations, &mut budget)?;
        block_ends.push(operations.len());
    }
    let sequence = Sequence { qubits, operations };
    budget.options.limits = options.limits;
    let proof = SynthesisProof {
        reduction: trace,
        block_ends,
        clean_ancilla: clean,
    };
    let remaining = options
        .max_work
        .checked_sub(budget.used)
        .ok_or(SynthesisError::Budget { resource: "work" })?;
    let certificate = verify_synthesis(target, &sequence, &proof, options.limits, remaining)
        .map_err(|error| match error {
            quest_math::Error::Budget {
                ref resource,
                requested,
                ..
            } if resource == "proof work" => SynthesisError::WorkExhausted {
                used: budget.used.saturating_add(requested),
                limit: options.max_work,
            },
            quest_math::Error::Budget { .. } | quest_math::Error::Resource(_) => {
                SynthesisError::Math(error)
            }
            _ => SynthesisError::CertificateRejected(error),
        })?;
    budget.charge(0)?;
    Ok(ExactSynthesis {
        work: budget.used.saturating_add(certificate.work()),
        certificate,
    })
}
