//! Exact affine phase-polynomial folding with a full scalar-phase term.
//!
//! A basis input `x` evolves to `A*x xor b` and acquires phase
//! `exp(i*pi*(c + sum a_p*parity(p & x)))`. Each phase coefficient is rational
//! and reduced modulo two; mask zero stores the scalar `c`. A complemented
//! parity contributes a scalar plus a negated parity coefficient. `Rz(theta)`
//! contributes `-theta/2` to the scalar and `theta` to its current affine parity.
//! Thus the pass preserves full phase, including `Rz(2*pi) = -I`.
//!
//! The candidate computes each parity into its first set wire, applies a phase,
//! uncomputes it, and then realizes the terminal affine map. Re-extraction checks
//! both the binary map and complete phase table before acceptance. Only a strictly
//! shorter candidate replaces the source. The representation is sufficient for
//! proof but does not find every equivalent phase polynomial or optimal circuit.
//!
//! Only rational-pi input angles grant these exact semantics. Symbolic and opaque
//! angles, effects, barriers, and unsupported controls cut program windows. All
//! explicit user edges disable the pass. Limits cover source/candidate sizes,
//! coefficient and intermediate bits, pass work, and conservative allocation
//! forecasts. They do not impose a wall-clock deadline or a process-RSS bound.
use crate::linear::{self, Work};
use crate::model::{AngleExpr, SemanticOperation};
use crate::{
    Angle, BigRational, Cnot, Error, Gate, LinearOptions, LinearRewrite, QubitId, Result,
    ValidatedProgram,
};
use num_bigint::BigInt;
use num_traits::{Signed, Zero};
use std::collections::BTreeMap;

/// Straight-line adapter shared by program windows and external block frontends.
/// Coefficients multiply mathematical pi; these DTOs are admitted on every call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AffinePhaseOperation {
    X {
        target: usize,
    },
    Cnot(Cnot),
    Phase {
        target: usize,
        coefficient: BigRational,
    },
    Rz {
        target: usize,
        coefficient: BigRational,
    },
    GlobalPhase {
        coefficient: BigRational,
    },
}
#[derive(Debug, Clone, Copy)]
pub struct ParityOptions {
    pub linear: LinearOptions,
    pub max_coefficient_bits: u64,
}
impl Default for ParityOptions {
    fn default() -> Self {
        Self {
            linear: LinearOptions::default(),
            max_coefficient_bits: 4096,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParitySynthesis {
    operations: Vec<AffinePhaseOperation>,
}
impl ParitySynthesis {
    #[must_use]
    pub fn operations(&self) -> &[AffinePhaseOperation] {
        &self.operations
    }
}
#[derive(Debug, Clone, Default)]
pub struct ParityReport {
    pub before_operations: usize,
    pub after_operations: usize,
    pub considered_windows: usize,
    pub accepted_windows: usize,
    pub work: usize,
    pub rewrites: Vec<LinearRewrite>,
}
#[derive(Debug, PartialEq, Eq)]
struct Signature {
    rows: Vec<u64>,
    offsets: u64,
    phases: BTreeMap<u64, BigRational>,
}
fn coefficient_bits(value: &BigRational, options: ParityOptions) -> Result<()> {
    if value.denom().is_zero() {
        return Err(Error::ZeroDenominator);
    }
    if value.numer().bits().max(value.denom().bits()) > options.max_coefficient_bits {
        return Err(Error::Budget("parity coefficient bits"));
    }
    Ok(())
}
fn reduced(value: &BigRational, options: ParityOptions) -> Result<BigRational> {
    coefficient_bits(value, options)?;
    let value = BigRational::new(value.numer().clone(), value.denom().clone());
    let period = std::ops::Mul::mul(value.denom(), BigInt::from(2));
    let mut numerator = std::ops::Rem::rem(value.numer(), &period);
    if numerator.is_negative() {
        std::ops::AddAssign::add_assign(&mut numerator, period);
    }
    let result = BigRational::new(numerator, value.denom().clone());
    coefficient_bits(&result, options)?;
    Ok(result)
}
fn add_phase(
    phases: &mut BTreeMap<u64, BigRational>,
    mask: u64,
    value: &BigRational,
    options: ParityOptions,
    work: &mut Work,
) -> Result<()> {
    let value = reduced(value, options)?;
    let value = if let Some(old) = phases.get(&mask) {
        let bits = old
            .numer()
            .bits()
            .max(old.denom().bits())
            .checked_add(value.numer().bits().max(value.denom().bits()))
            .and_then(|n| n.checked_add(1))
            .ok_or(Error::Budget("parity arithmetic bits"))?;
        if bits > options.max_coefficient_bits {
            return Err(Error::Budget("parity arithmetic bits"));
        }
        work.charge(usize::try_from(bits).map_err(|_| Error::Budget("parity work"))?)?;
        reduced(&std::ops::Add::add(old, value), options)?
    } else {
        value
    };
    if value.is_zero() {
        phases.remove(&mask);
    } else {
        phases.insert(mask, value);
    }
    Ok(())
}
fn allocation_preflight(width: usize, count: usize, options: ParityOptions) -> Result<()> {
    linear::preflight(width, count, options.linear)?;
    if options.max_coefficient_bits == 0 {
        return Err(Error::Budget("parity coefficient bits"));
    }
    let limb_bytes = options
        .max_coefficient_bits
        .checked_add(7)
        .and_then(|n| n.checked_div(8))
        .ok_or(Error::Budget("parity bytes"))?;
    let count = u64::try_from(count).map_err(|_| Error::Budget("parity bytes"))?;
    let bytes = limb_bytes
        .checked_mul(64)
        .and_then(|n| n.checked_add(4096))
        .and_then(|n| n.checked_mul(count.saturating_add(1)))
        .ok_or(Error::Budget("parity bytes"))?;
    if bytes > u64::try_from(options.linear.max_bytes).map_err(|_| Error::Budget("parity bytes"))? {
        return Err(Error::Budget("parity bytes"));
    }
    Ok(())
}
fn input_preflight(
    width: usize,
    operations: &[AffinePhaseOperation],
    options: ParityOptions,
) -> Result<()> {
    allocation_preflight(width, operations.len(), options)?;
    for operation in operations {
        match operation {
            AffinePhaseOperation::X { target }
            | AffinePhaseOperation::Phase { target, .. }
            | AffinePhaseOperation::Rz { target, .. }
                if *target >= width =>
            {
                return Err(Error::InvalidId);
            }
            AffinePhaseOperation::Cnot(gate) if gate.control >= width || gate.target >= width => {
                return Err(Error::InvalidId);
            }
            AffinePhaseOperation::Cnot(gate) if gate.control == gate.target => {
                return Err(Error::DuplicateOperand);
            }
            _ => {}
        }
        if let AffinePhaseOperation::Phase { coefficient, .. }
        | AffinePhaseOperation::Rz { coefficient, .. }
        | AffinePhaseOperation::GlobalPhase { coefficient } = operation
        {
            coefficient_bits(coefficient, options)?;
        }
    }
    Ok(())
}
fn signature(
    width: usize,
    operations: &[AffinePhaseOperation],
    options: ParityOptions,
    work: &mut Work,
) -> Result<Signature> {
    let mut result = Signature {
        rows: linear::identity(width)?,
        offsets: 0,
        phases: BTreeMap::new(),
    };
    for operation in operations {
        work.charge(1)?;
        match operation {
            AffinePhaseOperation::X { target } => result.offsets ^= linear::bit(*target)?,
            AffinePhaseOperation::Cnot(gate) => {
                linear::row_add(&mut result.rows, *gate)?;
                if result.offsets & linear::bit(gate.control)? != 0 {
                    result.offsets ^= linear::bit(gate.target)?;
                }
            }
            AffinePhaseOperation::GlobalPhase { coefficient } => {
                add_phase(&mut result.phases, 0, coefficient, options, work)?;
            }
            AffinePhaseOperation::Phase {
                target,
                coefficient,
            }
            | AffinePhaseOperation::Rz {
                target,
                coefficient,
            } => {
                let mut coefficient =
                    BigRational::new(coefficient.numer().clone(), coefficient.denom().clone());
                if matches!(operation, AffinePhaseOperation::Rz { .. }) {
                    let half =
                        std::ops::Div::div(std::ops::Neg::neg(&coefficient), BigInt::from(2));
                    add_phase(&mut result.phases, 0, &half, options, work)?;
                }
                if result.offsets & linear::bit(*target)? != 0 {
                    add_phase(&mut result.phases, 0, &coefficient, options, work)?;
                    coefficient = std::ops::Neg::neg(coefficient);
                }
                let mask = *result.rows.get(*target).ok_or(Error::InvalidId)?;
                add_phase(&mut result.phases, mask, &coefficient, options, work)?;
            }
        }
    }
    Ok(result)
}
fn candidate(
    signature: &Signature,
    limit: usize,
    options: ParityOptions,
    work: &mut Work,
) -> Result<Option<Vec<AffinePhaseOperation>>> {
    let mut output = vec![];
    for (&mask, coefficient) in &signature.phases {
        if mask == 0 {
            output.push(AffinePhaseOperation::GlobalPhase {
                coefficient: coefficient.clone(),
            });
        } else {
            let target = usize::try_from(mask.trailing_zeros())
                .map_err(|_| Error::Budget("parity target"))?;
            let mut compute = vec![];
            for control in 0..signature.rows.len() {
                work.charge(1)?;
                if control != target && mask & linear::bit(control)? != 0 {
                    compute.push(Cnot { control, target });
                }
            }
            let count = compute
                .len()
                .checked_mul(2)
                .and_then(|n| n.checked_add(output.len()))
                .and_then(|n| n.checked_add(1))
                .ok_or(Error::Budget("parity candidate"))?;
            if count >= limit {
                return Ok(None);
            }
            output.extend(compute.iter().copied().map(AffinePhaseOperation::Cnot));
            output.push(AffinePhaseOperation::Phase {
                target,
                coefficient: coefficient.clone(),
            });
            output.extend(compute.into_iter().rev().map(AffinePhaseOperation::Cnot));
        }
        if output.len() >= limit {
            return Ok(None);
        }
    }
    let (gaussian, pmh) = linear::synthesize_rows(&signature.rows, options.linear, work)?;
    let linear = if pmh.len() < gaussian.len() {
        pmh
    } else {
        gaussian
    };
    let total = output
        .len()
        .checked_add(linear.len())
        .and_then(|n| n.checked_add(usize::try_from(signature.offsets.count_ones()).ok()?))
        .ok_or(Error::Budget("parity candidate"))?;
    if total >= limit {
        return Ok(None);
    }
    output.extend(linear.into_iter().map(AffinePhaseOperation::Cnot));
    for target in 0..signature.rows.len() {
        if signature.offsets & linear::bit(target)? != 0 {
            output.push(AffinePhaseOperation::X { target });
        }
    }
    Ok(Some(output))
}
fn fold(
    width: usize,
    input: &[AffinePhaseOperation],
    options: ParityOptions,
    work: &mut Work,
) -> Result<ParitySynthesis> {
    input_preflight(width, input, options)?;
    let expected = signature(width, input, options, work)?;
    if let Some(candidate) = candidate(&expected, input.len(), options, work)? {
        input_preflight(width, &candidate, options)?;
        if signature(width, &candidate, options, work)? != expected {
            return Err(Error::NotUnitary);
        }
        Ok(ParitySynthesis {
            operations: candidate,
        })
    } else {
        Ok(ParitySynthesis {
            operations: input.to_vec(),
        })
    }
}
/// Fold an admitted affine phase polynomial including its scalar phase.
/// Exact replay verifies every candidate; ties and larger candidates retain input.
/// # Errors
/// Rejects invalid DTOs and exhausted allocation, coefficient or work bounds.
pub fn fold_parity(
    width: usize,
    input: &[AffinePhaseOperation],
    options: ParityOptions,
) -> Result<ParitySynthesis> {
    fold(
        width,
        input,
        options,
        &mut Work {
            used: 0,
            maximum: options.linear.max_work,
        },
    )
}
const fn rational_angle(angle: &Angle) -> Option<&BigRational> {
    if let AngleExpr::Pi(value) = &angle.0 {
        Some(value)
    } else {
        None
    }
}
fn admitted(operation: &SemanticOperation) -> bool {
    if linear::cnot(operation).is_some() {
        return true;
    }
    match operation {
        SemanticOperation::GlobalPhase { angle, controls } => {
            controls.is_empty() && rational_angle(angle).is_some()
        }
        SemanticOperation::Gate { gate, controls, .. } if controls.is_empty() => match gate {
            Gate::X | Gate::Z | Gate::S | Gate::Sdg | Gate::T | Gate::Tdg => true,
            Gate::Rz(angle) | Gate::Phase(angle) => rational_angle(angle).is_some(),
            _ => false,
        },
        _ => false,
    }
}
fn source_coefficient_check(operation: &SemanticOperation, options: ParityOptions) -> Result<()> {
    match operation {
        SemanticOperation::GlobalPhase { angle, .. }
        | SemanticOperation::Gate {
            gate: Gate::Rz(angle) | Gate::Phase(angle),
            ..
        } => {
            if let Some(value) = rational_angle(angle) {
                coefficient_bits(value, options)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn adapter(operation: &SemanticOperation) -> Option<AffinePhaseOperation> {
    if let Some(gate) = linear::cnot(operation) {
        return Some(AffinePhaseOperation::Cnot(gate));
    }
    match operation {
        SemanticOperation::GlobalPhase { angle, controls } if controls.is_empty() => {
            Some(AffinePhaseOperation::GlobalPhase {
                coefficient: rational_angle(angle)?.clone(),
            })
        }
        SemanticOperation::Gate {
            gate,
            targets,
            controls,
        } if controls.is_empty() => {
            let [target] = targets.as_slice() else {
                return None;
            };
            let target = target.index();
            Some(match gate {
                Gate::X => AffinePhaseOperation::X { target },
                Gate::Rz(angle) => AffinePhaseOperation::Rz {
                    target,
                    coefficient: rational_angle(angle)?.clone(),
                },
                Gate::Phase(angle) => AffinePhaseOperation::Phase {
                    target,
                    coefficient: rational_angle(angle)?.clone(),
                },
                Gate::Z => AffinePhaseOperation::Phase {
                    target,
                    coefficient: BigRational::from_integer(1.into()),
                },
                Gate::S => AffinePhaseOperation::Phase {
                    target,
                    coefficient: BigRational::new(1.into(), 2.into()),
                },
                Gate::Sdg => AffinePhaseOperation::Phase {
                    target,
                    coefficient: BigRational::new((-1).into(), 2.into()),
                },
                Gate::T => AffinePhaseOperation::Phase {
                    target,
                    coefficient: BigRational::new(1.into(), 4.into()),
                },
                Gate::Tdg => AffinePhaseOperation::Phase {
                    target,
                    coefficient: BigRational::new((-1).into(), 4.into()),
                },
                _ => return None,
            })
        }
        _ => None,
    }
}
fn operation(value: AffinePhaseOperation, owner: u64) -> Result<SemanticOperation> {
    let (gate, target) = match value {
        AffinePhaseOperation::Cnot(gate) => return Ok(linear::cnot_operation(gate, owner)),
        AffinePhaseOperation::GlobalPhase { coefficient } => {
            return Ok(SemanticOperation::GlobalPhase {
                angle: Angle::rational_pi(coefficient)?,
                controls: vec![],
            });
        }
        AffinePhaseOperation::X { target } => (Gate::X, target),
        AffinePhaseOperation::Phase {
            target,
            coefficient,
        } => (Gate::Phase(Angle::rational_pi(coefficient)?), target),
        AffinePhaseOperation::Rz {
            target,
            coefficient,
        } => (Gate::Rz(Angle::rational_pi(coefficient)?), target),
    };
    Ok(SemanticOperation::Gate {
        gate,
        targets: vec![QubitId {
            owner,
            index: target,
        }],
        controls: vec![],
    })
}
impl ValidatedProgram {
    /// Fold bounded contiguous rational-pi affine windows. Effects, unsupported
    /// controls and opaque/symbolic angles stop a window; explicit edges disable rewriting.
    /// # Errors
    /// Rejects exhausted resource limits and invalid rebuilt dependencies.
    pub fn optimize_parity(self, options: ParityOptions) -> Result<(Self, ParityReport)> {
        input_preflight(self.num_qubits, &[], options)?;
        if options.linear.max_window_operations == 0 {
            return Err(Error::Budget("parity window"));
        }
        let mut report = ParityReport {
            before_operations: self.occurrences.len(),
            after_operations: self.occurrences.len(),
            ..ParityReport::default()
        };
        if !self.explicit_edges.is_empty() {
            return Ok((self, report));
        }
        let options = ParityOptions {
            linear: linear::program_preflight(&self, options.linear)?,
            ..options
        };
        let mut work = Work {
            used: self.occurrences.len(),
            maximum: options.linear.max_work,
        };
        let mut output = vec![];
        let mut offset = 0;
        while let Some(current) = self.occurrences.get(offset) {
            if !admitted(&current.operation) {
                output.push(current.clone());
                offset = offset
                    .checked_add(1)
                    .ok_or(Error::Budget("parity offset"))?;
                continue;
            }
            let count = self
                .occurrences
                .iter()
                .skip(offset)
                .take(options.linear.max_window_operations)
                .take_while(|item| admitted(&item.operation))
                .count();
            allocation_preflight(self.num_qubits, count, options)?;
            let end = offset
                .checked_add(count)
                .ok_or(Error::Budget("parity offset"))?;
            let window = self.occurrences.get(offset..end).ok_or(Error::InvalidId)?;
            for item in window {
                source_coefficient_check(&item.operation, options)?;
            }
            let input: Vec<_> = window
                .iter()
                .filter_map(|item| adapter(&item.operation))
                .collect();
            let result = fold(self.num_qubits, &input, options, &mut work)?;
            report.considered_windows = report
                .considered_windows
                .checked_add(1)
                .ok_or(Error::Budget("parity report"))?;
            if result.operations.len() < input.len() {
                let operations = result
                    .operations
                    .into_iter()
                    .map(|value| operation(value, self.owner))
                    .collect::<Result<Vec<_>>>()?;
                let (replacement, rewrite) =
                    linear::replacement(window, operations, options.linear)?;
                output.extend(replacement);
                report.rewrites.push(rewrite);
                report.accepted_windows = report
                    .accepted_windows
                    .checked_add(1)
                    .ok_or(Error::Budget("parity report"))?;
            } else {
                output.extend_from_slice(window);
            }
            offset = end;
        }
        report.work = work.used;
        report.after_operations = output.len();
        let result = Self::from_parts(
            self.owner,
            self.num_qubits,
            self.num_bits,
            self.parameters,
            output,
            self.explicit_edges,
            self.limits,
        )?;
        Ok((result, report))
    }
}
