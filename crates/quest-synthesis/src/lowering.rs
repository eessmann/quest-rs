//! Elementary lowering by Gray paths and exact SU(2) commutators.
// All machine phase arithmetic is modulo eight on already-admitted exponents.
#![allow(clippy::arithmetic_side_effects, clippy::redundant_pub_crate)]
use crate::{Budget, Result, SynthesisError};
use quest_math::{BasisIndex, Gate, Operation, RowOperation};

fn emit(
    out: &mut Vec<Operation>,
    gate: Gate,
    targets: &[usize],
    budget: &mut Budget,
) -> Result<()> {
    budget.output(out.len(), 1)?;
    out.try_reserve(1).map_err(|_| SynthesisError::Budget {
        resource: "gate allocation",
    })?;
    out.push(Operation {
        gate,
        targets: targets.to_vec(),
        controls: Vec::new(),
    });
    Ok(())
}
fn one(out: &mut Vec<Operation>, gate: Gate, target: usize, budget: &mut Budget) -> Result<()> {
    emit(out, gate, &[target], budget)
}
fn phase(out: &mut Vec<Operation>, target: usize, power: u8, budget: &mut Budget) -> Result<()> {
    match power & 7 {
        0 => Ok(()),
        1 => one(out, Gate::T, target, budget),
        2 => one(out, Gate::S, target, budget),
        3 => {
            one(out, Gate::S, target, budget)?;
            one(out, Gate::T, target, budget)
        }
        4 => one(out, Gate::Z, target, budget),
        5 => {
            one(out, Gate::Z, target, budget)?;
            one(out, Gate::T, target, budget)
        }
        6 => one(out, Gate::Sdg, target, budget),
        _ => one(out, Gate::Tdg, target, budget),
    }
}
fn crz(
    control: usize,
    target: usize,
    inverse: bool,
    out: &mut Vec<Operation>,
    budget: &mut Budget,
) -> Result<()> {
    if inverse {
        emit(out, Gate::Cx, &[control, target], budget)?;
        one(out, Gate::T, target, budget)?;
        emit(out, Gate::Cx, &[control, target], budget)?;
        one(out, Gate::Tdg, target, budget)
    } else {
        one(out, Gate::T, target, budget)?;
        emit(out, Gate::Cx, &[control, target], budget)?;
        one(out, Gate::Tdg, target, budget)?;
        emit(out, Gate::Cx, &[control, target], budget)
    }
}
fn controlled_iy(
    controls: &[usize],
    target: usize,
    inverse: bool,
    out: &mut Vec<Operation>,
    budget: &mut Budget,
) -> Result<()> {
    one(out, Gate::Sdg, target, budget)?;
    controlled_ix(controls, target, inverse, out, budget)?;
    one(out, Gate::S, target, budget)
}
// With A=iY and B=Rz(pi/2), H A B A^-1 B^-1 H = iX.
// Controls split between A and B; if either set is false the commutator is I.
fn controlled_ix(
    controls: &[usize],
    target: usize,
    inverse: bool,
    out: &mut Vec<Operation>,
    budget: &mut Budget,
) -> Result<()> {
    budget.charge(1)?;
    match controls {
        [] => {
            one(out, if inverse { Gate::Y } else { Gate::Z }, target, budget)?;
            one(out, if inverse { Gate::Z } else { Gate::Y }, target, budget)
        }
        [control] => {
            one(
                out,
                if inverse { Gate::Sdg } else { Gate::S },
                *control,
                budget,
            )?;
            emit(out, Gate::Cx, &[*control, target], budget)
        }
        _ => {
            let (&last, rest) = controls
                .split_last()
                .ok_or(SynthesisError::Invalid("controlled iX split"))?;
            one(out, Gate::H, target, budget)?;
            if inverse {
                controlled_iy(rest, target, true, out, budget)?;
                crz(last, target, true, out, budget)?;
                controlled_iy(rest, target, false, out, budget)?;
                crz(last, target, false, out, budget)?;
            } else {
                crz(last, target, true, out, budget)?;
                controlled_iy(rest, target, true, out, budget)?;
                crz(last, target, false, out, budget)?;
                controlled_iy(rest, target, false, out, budget)?;
            }
            one(out, Gate::H, target, budget)
        }
    }
}
fn signed_ix(
    pattern: usize,
    qubits: usize,
    target: usize,
    inverse: bool,
    out: &mut Vec<Operation>,
    budget: &mut Budget,
) -> Result<()> {
    let controls = (0..qubits).filter(|&q| q != target).collect::<Vec<_>>();
    for &q in &controls {
        if pattern & (1 << q) == 0 {
            one(out, Gate::X, q, budget)?;
        }
    }
    controlled_ix(&controls, target, inverse, out, budget)?;
    for &q in controls.iter().rev() {
        if pattern & (1 << q) == 0 {
            one(out, Gate::X, q, budget)?;
        }
    }
    Ok(())
}

fn local(
    step: RowOperation,
    controls: &[usize],
    target: usize,
    out: &mut Vec<Operation>,
    budget: &mut Budget,
) -> Result<()> {
    match step {
        RowOperation::ImaginarySwap(_, _, inverse) => {
            controlled_ix(controls, target, inverse, out, budget)
        }
        RowOperation::ImaginaryHadamard(_, _, m, inverse) => {
            phase(out, target, m, budget)?;
            one(out, Gate::Sdg, target, budget)?;
            one(out, Gate::H, target, budget)?;
            one(out, Gate::Tdg, target, budget)?;
            controlled_ix(controls, target, inverse, out, budget)?;
            one(out, Gate::T, target, budget)?;
            one(out, Gate::H, target, budget)?;
            one(out, Gate::S, target, budget)?;
            phase(out, target, (8 - m) & 7, budget)
        }
        RowOperation::OppositePhase(_, _, power) => {
            // A T A^-1 T^-1 = diag(omega,omega^-1), with A=iX.
            for _ in 0..power {
                one(out, Gate::Tdg, target, budget)?;
                controlled_ix(controls, target, true, out, budget)?;
                one(out, Gate::T, target, budget)?;
                controlled_ix(controls, target, false, out, budget)?;
            }
            Ok(())
        }
        _ => Err(SynthesisError::Invalid("non-SU(2) local lowering")),
    }
}
fn two_level(
    step: RowOperation,
    a: usize,
    b: usize,
    qubits: usize,
    out: &mut Vec<Operation>,
    budget: &mut Budget,
) -> Result<()> {
    let differences = (0..qubits)
        .filter(|q| (a ^ b) & (1 << q) != 0)
        .collect::<Vec<_>>();
    let (&target, path) = differences
        .split_last()
        .ok_or(SynthesisError::Invalid("identical basis indices"))?;
    let mut current = a;
    let mut swaps = Vec::new();
    for &bit in path {
        swaps.push((current, bit));
        signed_ix(current, qubits, bit, false, out, budget)?;
        current ^= 1 << bit;
    }
    let controls = (0..qubits).filter(|&q| q != target).collect::<Vec<_>>();
    for &q in &controls {
        if current & (1 << q) == 0 {
            one(out, Gate::X, q, budget)?;
        }
    }
    if current & (1 << target) != 0 {
        one(out, Gate::X, target, budget)?;
    }
    let correction =
        u8::try_from((path.len() % 4) * 2).map_err(|_| SynthesisError::Invalid("Gray phase"))?;
    phase(out, target, correction, budget)?;
    local(step, &controls, target, out, budget)?;
    phase(out, target, (8 - correction) & 7, budget)?;
    if current & (1 << target) != 0 {
        one(out, Gate::X, target, budget)?;
    }
    for &q in controls.iter().rev() {
        if current & (1 << q) == 0 {
            one(out, Gate::X, q, budget)?;
        }
    }
    for &(pattern, bit) in swaps.iter().rev() {
        signed_ix(pattern, qubits, bit, true, out, budget)?;
    }
    Ok(())
}
pub(crate) fn inverse_step(
    step: RowOperation,
    qubits: usize,
    clean: bool,
    out: &mut Vec<Operation>,
    budget: &mut Budget,
) -> Result<()> {
    match step {
        RowOperation::Hadamard(BasisIndex(0), BasisIndex(1)) if qubits == 1 => {
            one(out, Gate::H, 0, budget)
        }
        RowOperation::Swap(BasisIndex(0), BasisIndex(1)) if qubits == 1 => {
            one(out, Gate::X, 0, budget)
        }
        RowOperation::EmbeddedPhase { qubit, power } => phase(out, qubit, (8 - power) & 7, budget),
        RowOperation::Phase(BasisIndex(row), power) => {
            let power = (8 - power) & 7;
            if qubits == 1 {
                if row == 0 {
                    one(out, Gate::X, 0, budget)?;
                }
                phase(out, 0, power, budget)?;
                if row == 0 {
                    one(out, Gate::X, 0, budget)?;
                }
                return Ok(());
            }
            if !clean {
                return Err(SynthesisError::Invalid("unbalanced no-ancilla phase"));
            }
            for q in 0..qubits {
                if row & (1 << q) == 0 {
                    one(out, Gate::X, q, budget)?;
                }
            }
            let controls = (0..qubits).collect::<Vec<_>>();
            controlled_ix(&controls, qubits, false, out, budget)?;
            phase(out, qubits, power, budget)?;
            controlled_ix(&controls, qubits, true, out, budget)?;
            for q in (0..qubits).rev() {
                if row & (1 << q) == 0 {
                    one(out, Gate::X, q, budget)?;
                }
            }
            Ok(())
        }
        RowOperation::ImaginarySwap(BasisIndex(a), BasisIndex(b), inverse) => two_level(
            RowOperation::ImaginarySwap(BasisIndex(a), BasisIndex(b), !inverse),
            a,
            b,
            qubits,
            out,
            budget,
        ),
        RowOperation::ImaginaryHadamard(BasisIndex(a), BasisIndex(b), m, inverse) => two_level(
            RowOperation::ImaginaryHadamard(BasisIndex(a), BasisIndex(b), m, !inverse),
            a,
            b,
            qubits,
            out,
            budget,
        ),
        RowOperation::OppositePhase(BasisIndex(a), BasisIndex(b), m) => two_level(
            RowOperation::OppositePhase(BasisIndex(a), BasisIndex(b), (8 - m) & 7),
            a,
            b,
            qubits,
            out,
            budget,
        ),
        _ => Err(SynthesisError::Invalid("unsupported reduction generator")),
    }
}
