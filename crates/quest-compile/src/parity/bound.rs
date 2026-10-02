//! Binding-specific exact affine parity candidates. The original ideal program
//! remains the authority for parameter domains and source conversion obligations.
use super::{ParityOptions, ParityReport};
use crate::linear::{self, Cnot, Work};
use dashu_base::BitTest;

use crate::{
    Angle, BoundAngleTarget, BoundRegion, Error, Gate, ParameterId, ProvenanceGraph, QuantumRegion,
    QubitId, RBig, Result,
};
use dashu_base::Signed;
use dashu_int::IBig;
use quest_language::quantum::model::{Instruction, Occurrence, Operation, SemanticOperation};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
enum AffineOp {
    X(usize),
    Cnot(Cnot),
    Phase(usize, Angle),
    Rz(usize, Angle),
    Global(Angle),
}
#[derive(Clone, PartialEq, Eq)]
struct Pair {
    radians: RBig,
    pi: RBig,
}
impl Pair {
    fn from_target(target: &BoundAngleTarget, options: ParityOptions) -> Result<Self> {
        let within = |values: &[&IBig]| {
            values
                .iter()
                .all(|value| value.bit_len() <= options.max_coefficient_bits)
        };
        let (radians, pi) = match target {
            BoundAngleTarget::DyadicRadians { bits } => {
                if *bits == (-0.0f64).to_bits() {
                    return Err(Error::Unsupported("signed-zero symbolic parity"));
                }
                (
                    quest_language::rational::dyadic_from_bits(*bits).ok_or(Error::NonFinite)?,
                    RBig::from(0),
                )
            }
            BoundAngleTarget::RationalPi {
                numerator,
                denominator,
            } => {
                if denominator.is_zero() {
                    return Err(Error::ZeroDenominator);
                }
                if !within(&[numerator, denominator]) {
                    return Err(Error::Budget("symbolic parity coefficient bits"));
                }
                (
                    RBig::from(0),
                    RBig::from_parts_signed(numerator.clone(), denominator.clone()),
                )
            }
            BoundAngleTarget::AffinePi {
                radians_numerator,
                radians_denominator,
                pi_numerator,
                pi_denominator,
            } => {
                if radians_denominator.is_zero() || pi_denominator.is_zero() {
                    return Err(Error::ZeroDenominator);
                }
                if !within(&[
                    radians_numerator,
                    radians_denominator,
                    pi_numerator,
                    pi_denominator,
                ]) {
                    return Err(Error::Budget("symbolic parity coefficient bits"));
                }
                (
                    RBig::from_parts_signed(radians_numerator.clone(), radians_denominator.clone()),
                    RBig::from_parts_signed(pi_numerator.clone(), pi_denominator.clone()),
                )
            }
        };
        Ok(Self { radians, pi })
    }
    fn bit_len(&self) -> usize {
        [&self.radians, &self.pi]
            .into_iter()
            .map(|v| v.numerator().bit_len().max(v.denominator().bit_len()))
            .max()
            .unwrap_or(0)
    }
    fn checked(self, options: ParityOptions) -> Result<Self> {
        if self.bit_len() > options.max_coefficient_bits {
            return Err(Error::Budget("symbolic parity coefficient bits"));
        }
        Ok(self)
    }
    fn normalized(mut self, options: ParityOptions, work: &mut Work) -> Result<Self> {
        self = self.checked(options)?;
        let period_bits = self
            .pi
            .denominator()
            .bit_len()
            .checked_add(1)
            .ok_or(Error::Budget("symbolic parity arithmetic"))?;
        Self::admit_forecast(period_bits, options, work)?;
        let period = std::ops::Mul::mul(self.pi.denominator(), IBig::from(2));
        let mut numerator = std::ops::Rem::rem(self.pi.numerator(), &period);
        if numerator.is_negative() {
            std::ops::AddAssign::add_assign(&mut numerator, period);
        }
        self.pi = RBig::from_parts(numerator, self.pi.denominator().clone());
        self.checked(options)
    }
    fn add(&self, other: &Self, options: ParityOptions, work: &mut Work) -> Result<Self> {
        Self {
            radians: std::ops::Add::add(&self.radians, &other.radians),
            pi: std::ops::Add::add(&self.pi, &other.pi),
        }
        .normalized(options, work)
    }
    fn forecast_add(&self, other: &Self, options: ParityOptions, work: &mut Work) -> Result<()> {
        let component = |a: &RBig, b: &RBig| -> Result<usize> {
            let left = a
                .numerator()
                .bit_len()
                .checked_add(b.denominator().bit_len())
                .ok_or(Error::Budget("symbolic parity arithmetic"))?;
            let right = b
                .numerator()
                .bit_len()
                .checked_add(a.denominator().bit_len())
                .ok_or(Error::Budget("symbolic parity arithmetic"))?;
            let numerator = left
                .max(right)
                .checked_add(1)
                .ok_or(Error::Budget("symbolic parity arithmetic"))?;
            let denominator = a
                .denominator()
                .bit_len()
                .checked_add(b.denominator().bit_len())
                .ok_or(Error::Budget("symbolic parity arithmetic"))?;
            Ok(numerator.max(denominator))
        };
        let bits = component(&self.radians, &other.radians)?.max(component(&self.pi, &other.pi)?);
        Self::admit_forecast(bits, options, work)
    }
    fn forecast_scale(
        &self,
        numerator: i64,
        denominator: i64,
        options: ParityOptions,
        work: &mut Work,
    ) -> Result<()> {
        let ratio_numerator = IBig::from(numerator).bit_len();
        let ratio_denominator = IBig::from(denominator).bit_len();
        let component = |value: &RBig| -> Result<usize> {
            let numerator = value
                .numerator()
                .bit_len()
                .checked_add(ratio_numerator)
                .ok_or(Error::Budget("symbolic parity arithmetic"))?;
            let denominator = value
                .denominator()
                .bit_len()
                .checked_add(ratio_denominator)
                .ok_or(Error::Budget("symbolic parity arithmetic"))?;
            Ok(numerator.max(denominator))
        };
        Self::admit_forecast(
            component(&self.radians)?.max(component(&self.pi)?),
            options,
            work,
        )
    }
    fn admit_forecast(bits: usize, options: ParityOptions, work: &mut Work) -> Result<()> {
        if bits > options.max_coefficient_bits {
            return Err(Error::Budget("symbolic parity coefficient bits"));
        }
        work.charge(bits)
    }
    fn scaled(
        &self,
        numerator: i64,
        denominator: i64,
        options: ParityOptions,
        work: &mut Work,
    ) -> Result<Self> {
        let ratio = RBig::from_parts_signed(numerator.into(), denominator.into());
        Self {
            radians: std::ops::Mul::mul(&self.radians, &ratio),
            pi: std::ops::Mul::mul(&self.pi, &ratio),
        }
        .normalized(options, work)
    }
    fn angle(&self) -> Result<Angle> {
        Ok(Angle::affine(self.radians.clone(), self.pi.clone())?)
    }
    const fn is_zero(&self) -> bool {
        self.radians.is_zero() && self.pi.is_zero()
    }
}
fn raw_from_angle(angle: &Angle, options: ParityOptions) -> Result<Pair> {
    if !angle.is_exact() {
        return Err(Error::Unsupported("opaque symbolic parity checker"));
    }
    let (radians, pi) = angle.independent_constant_summary().ok_or(Error::Binding)?;
    Pair {
        radians: radians.clone(),
        pi: pi.clone(),
    }
    .checked(options)
}
fn normalize_angle(angle: &Angle, options: ParityOptions, work: &mut Work) -> Result<Angle> {
    let pair = raw_from_angle(angle, options)?.normalized(options, work)?;
    work.charge(pair.bit_len())?;
    pair.angle()
}
fn charge_exact_source(angle: &Angle, work: &mut Work) -> Result<()> {
    work.charge(
        usize::try_from(angle.equivalence_work_estimate()?)
            .map_err(|_| Error::Budget("symbolic parity source work"))?,
    )
}
fn add_angle(
    phases: &mut BTreeMap<u64, Angle>,
    mask: u64,
    angle: &Angle,
    options: ParityOptions,
    work: &mut Work,
) -> Result<()> {
    if let Some(old) = phases.get(&mask) {
        raw_from_angle(old, options)?.forecast_add(
            &raw_from_angle(angle, options)?,
            options,
            work,
        )?;
    } else {
        Pair::admit_forecast(raw_from_angle(angle, options)?.bit_len(), options, work)?;
    }
    let merged = if let Some(old) = phases.get(&mask) {
        charge_exact_source(old, work)?;
        charge_exact_source(angle, work)?;
        old.added(angle)?
    } else {
        angle.clone()
    };
    let merged = normalize_angle(&merged, options, work)?;
    if raw_from_angle(&merged, options)?.is_zero() {
        phases.remove(&mask);
    } else {
        phases.insert(mask, merged);
    }
    Ok(())
}
fn eligible(operation: &SemanticOperation) -> bool {
    if linear::cnot(operation).is_some() {
        return true;
    }
    match operation {
        SemanticOperation::GlobalPhase { angle, controls } => {
            controls.is_empty() && angle.is_exact()
        }
        SemanticOperation::Gate { gate, controls, .. } if controls.is_empty() => match gate {
            Gate::X | Gate::Z | Gate::S | Gate::Sdg | Gate::T | Gate::Tdg => true,
            Gate::Phase(angle) | Gate::Rz(angle) => angle.is_exact(),
            _ => false,
        },
        _ => false,
    }
}
fn bound_angle(
    instruction: &Instruction,
    options: ParityOptions,
    work: &mut Work,
) -> Result<Angle> {
    let [Some(target)] = instruction.angle_targets() else {
        return Err(Error::InvalidId);
    };
    let pair = Pair::from_target(target, options)?.checked(options)?;
    Pair::admit_forecast(pair.bit_len(), options, work)?;
    pair.angle()
}
fn adapt(
    occurrence: &Occurrence,
    instruction: &Instruction,
    options: ParityOptions,
    work: &mut Work,
) -> Result<AffineOp> {
    if let Some(gate) = linear::cnot(&occurrence.operation) {
        return Ok(AffineOp::Cnot(gate));
    }
    match &occurrence.operation {
        SemanticOperation::GlobalPhase { controls, .. } if controls.is_empty() => {
            Ok(AffineOp::Global(bound_angle(instruction, options, work)?))
        }
        SemanticOperation::Gate {
            gate,
            targets,
            controls,
        } if controls.is_empty() => {
            let [target] = targets.as_ref() else {
                return Err(Error::InvalidId);
            };
            let index = target.index();
            Ok(match gate {
                Gate::X => AffineOp::X(index),
                Gate::Phase(_) => AffineOp::Phase(index, bound_angle(instruction, options, work)?),
                Gate::Rz(_) => AffineOp::Rz(index, bound_angle(instruction, options, work)?),
                Gate::Z => AffineOp::Phase(index, Angle::pi(1, 1)?),
                Gate::S => AffineOp::Phase(index, Angle::pi(1, 2)?),
                Gate::Sdg => AffineOp::Phase(index, Angle::pi(-1, 2)?),
                Gate::T => AffineOp::Phase(index, Angle::pi(1, 4)?),
                Gate::Tdg => AffineOp::Phase(index, Angle::pi(-1, 4)?),
                _ => return Err(Error::Unsupported("symbolic parity operation")),
            })
        }
        _ => Err(Error::Unsupported("symbolic parity operation")),
    }
}
struct AngleSignature {
    rows: Vec<u64>,
    offsets: u64,
    phases: BTreeMap<u64, Angle>,
}
fn angle_signature(
    width: usize,
    operations: &[AffineOp],
    options: ParityOptions,
    work: &mut Work,
) -> Result<AngleSignature> {
    let mut result = AngleSignature {
        rows: linear::identity(width)?,
        offsets: 0,
        phases: BTreeMap::new(),
    };
    for operation in operations {
        work.charge(1)?;
        match operation {
            AffineOp::X(target) => result.offsets ^= linear::bit(*target)?,
            AffineOp::Cnot(gate) => {
                linear::row_add(&mut result.rows, *gate)?;
                if result.offsets & linear::bit(gate.control)? != 0 {
                    result.offsets ^= linear::bit(gate.target)?;
                }
            }
            AffineOp::Global(angle) => add_angle(&mut result.phases, 0, angle, options, work)?,
            AffineOp::Phase(target, angle) | AffineOp::Rz(target, angle) => {
                if matches!(operation, AffineOp::Rz(..)) {
                    raw_from_angle(angle, options)?.forecast_scale(-1, 2, options, work)?;
                    charge_exact_source(angle, work)?;
                    add_angle(
                        &mut result.phases,
                        0,
                        &angle.scaled_ratio((-1).into(), 2.into())?,
                        options,
                        work,
                    )?;
                }
                let mut angle = angle.clone();
                if result.offsets & linear::bit(*target)? != 0 {
                    add_angle(&mut result.phases, 0, &angle, options, work)?;
                    raw_from_angle(&angle, options)?.forecast_scale(-1, 1, options, work)?;
                    charge_exact_source(&angle, work)?;
                    angle = angle.negated()?;
                }
                add_angle(
                    &mut result.phases,
                    *result.rows.get(*target).ok_or(Error::InvalidId)?,
                    &angle,
                    options,
                    work,
                )?;
            }
        }
    }
    Ok(result)
}
#[derive(PartialEq, Eq)]
struct PairSignature {
    rows: Vec<u64>,
    offsets: u64,
    phases: BTreeMap<u64, Pair>,
}
fn add_pair(
    phases: &mut BTreeMap<u64, Pair>,
    mask: u64,
    pair: &Pair,
    options: ParityOptions,
    work: &mut Work,
) -> Result<()> {
    if let Some(old) = phases.get(&mask) {
        old.forecast_add(pair, options, work)?;
    } else {
        Pair::admit_forecast(pair.bit_len(), options, work)?;
    }
    let next = if let Some(old) = phases.get(&mask) {
        old.add(pair, options, work)?
    } else {
        pair.clone().normalized(options, work)?
    };
    work.charge(next.bit_len())?;
    if next.is_zero() {
        phases.remove(&mask);
    } else {
        phases.insert(mask, next);
    }
    Ok(())
}
// Independent exact-rational replay. Candidate construction above uses Angle/
// quest-symbolic for every affine addition and scaling, while this checker
// recomputes both signatures without its normalizer.
fn pair_signature(
    width: usize,
    operations: &[AffineOp],
    options: ParityOptions,
    work: &mut Work,
) -> Result<PairSignature> {
    let mut result = PairSignature {
        rows: linear::identity(width)?,
        offsets: 0,
        phases: BTreeMap::new(),
    };
    for operation in operations {
        work.charge(1)?;
        match operation {
            AffineOp::X(target) => result.offsets ^= linear::bit(*target)?,
            AffineOp::Cnot(gate) => {
                linear::row_add(&mut result.rows, *gate)?;
                if result.offsets & linear::bit(gate.control)? != 0 {
                    result.offsets ^= linear::bit(gate.target)?;
                }
            }
            AffineOp::Global(angle) => add_pair(
                &mut result.phases,
                0,
                &raw_from_angle(angle, options)?,
                options,
                work,
            )?,
            AffineOp::Phase(target, angle) | AffineOp::Rz(target, angle) => {
                let mut pair = raw_from_angle(angle, options)?;
                if matches!(operation, AffineOp::Rz(..)) {
                    pair.forecast_scale(-1, 2, options, work)?;
                    add_pair(
                        &mut result.phases,
                        0,
                        &pair.scaled(-1, 2, options, work)?,
                        options,
                        work,
                    )?;
                }
                if result.offsets & linear::bit(*target)? != 0 {
                    add_pair(&mut result.phases, 0, &pair, options, work)?;
                    pair.forecast_scale(-1, 1, options, work)?;
                    pair = pair.scaled(-1, 1, options, work)?;
                }
                add_pair(
                    &mut result.phases,
                    *result.rows.get(*target).ok_or(Error::InvalidId)?,
                    &pair,
                    options,
                    work,
                )?;
            }
        }
    }
    Ok(result)
}
fn candidate(
    signature: &AngleSignature,
    limit: usize,
    options: ParityOptions,
    work: &mut Work,
) -> Result<Vec<AffineOp>> {
    let width = signature.rows.len();
    let mut output = Vec::new();
    if let Some(scalar) = signature.phases.get(&0) {
        output.push(AffineOp::Global(scalar.clone()));
    }
    for target in 0..width {
        let pivot = linear::bit(target)?;
        let mut current = pivot;
        for (&mask, coefficient) in signature.phases.iter().filter(|(mask, _)| {
            **mask != 0 && usize::try_from(mask.trailing_zeros()).ok() == Some(target)
        }) {
            let difference = current ^ mask;
            let extra = usize::try_from(difference.count_ones())
                .map_err(|_| Error::Budget("symbolic parity output"))?
                .checked_add(1)
                .ok_or(Error::Budget("symbolic parity output"))?;
            if output.len().checked_add(extra).is_none_or(|n| n >= limit) {
                return Err(Error::Budget("symbolic parity output"));
            }
            for control in 0..width {
                work.charge(1)?;
                if difference & linear::bit(control)? != 0 {
                    output.push(AffineOp::Cnot(Cnot { control, target }));
                }
            }
            output.push(AffineOp::Phase(target, coefficient.clone()));
            current = mask;
        }
        let restore = current ^ pivot;
        let extra = usize::try_from(restore.count_ones())
            .map_err(|_| Error::Budget("symbolic parity output"))?;
        if output.len().checked_add(extra).is_none_or(|n| n >= limit) {
            return Err(Error::Budget("symbolic parity output"));
        }
        for control in 0..width {
            work.charge(1)?;
            if restore & linear::bit(control)? != 0 {
                output.push(AffineOp::Cnot(Cnot { control, target }));
            }
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
        .ok_or(Error::Budget("symbolic parity output"))?;
    if total >= limit {
        return Err(Error::Budget("symbolic parity output"));
    }
    output.extend(linear.into_iter().map(AffineOp::Cnot));
    for target in 0..width {
        if signature.offsets & linear::bit(target)? != 0 {
            output.push(AffineOp::X(target));
        }
    }
    Ok(output)
}
fn semantic(operation: AffineOp, owner: u64) -> SemanticOperation {
    match operation {
        AffineOp::Cnot(gate) => linear::cnot_operation(gate, owner),
        AffineOp::Global(angle) => SemanticOperation::GlobalPhase {
            angle,
            controls: Arc::from([]),
        },
        AffineOp::X(target) => SemanticOperation::Gate {
            gate: Gate::X,
            targets: vec![QubitId {
                owner,
                index: target,
            }]
            .into(),
            controls: Arc::from([]),
        },
        AffineOp::Phase(target, angle) => SemanticOperation::Gate {
            gate: Gate::Phase(angle),
            targets: vec![QubitId {
                owner,
                index: target,
            }]
            .into(),
            controls: Arc::from([]),
        },
        AffineOp::Rz(target, angle) => SemanticOperation::Gate {
            gate: Gate::Rz(angle),
            targets: vec![QubitId {
                owner,
                index: target,
            }]
            .into(),
            controls: Arc::from([]),
        },
    }
}
fn same_affine_operation(expected: &Operation, actual: &Operation) -> bool {
    match (expected, actual) {
        (
            Operation::Gate {
                gate: first,
                targets: first_targets,
                controls: first_controls,
            },
            Operation::Gate {
                gate: second,
                targets: second_targets,
                controls: second_controls,
            },
        ) => {
            first == second && first_targets == second_targets && first_controls == second_controls
        }
        (
            Operation::GlobalPhase {
                radians: first,
                controls: first_controls,
            },
            Operation::GlobalPhase {
                radians: second,
                controls: second_controls,
            },
        ) => first.to_bits() == second.to_bits() && first_controls == second_controls,
        _ => false,
    }
}
#[expect(
    clippy::suspicious_operation_groupings,
    reason = "A bound source identity is compared to the original source publication identity"
)]
fn validate_source(
    source: &QuantumRegion,
    bound: &BoundRegion,
    options: ParityOptions,
) -> Result<Vec<(ParameterId, f64)>> {
    if bound.source_snapshot_id() != source.snapshot_id()
        || bound.num_qubits() != source.num_qubits()
        || bound.num_bits() != source.num_bits()
        || bound.instructions().len() != source.occurrences().len()
        || !source.explicit_edges().is_empty()
    {
        return Err(Error::InvalidId);
    }
    let overlap = source
        .retained_bytes()?
        .checked_add(bound.retained_bytes()?)
        .and_then(|n| n.checked_mul(3))
        .ok_or(Error::Budget("symbolic parity overlap"))?;
    if overlap > options.linear.max_bytes {
        return Err(Error::Budget("symbolic parity overlap"));
    }
    let mut pairs = Vec::new();
    pairs
        .try_reserve_exact(bound.binding_storage().len())
        .map_err(|_| Error::Budget("symbolic parity bindings"))?;
    for (&id, &value) in bound.binding_storage() {
        if id.owner != source.owner()
            || id.index >= source.parameter_storage().len()
            || !value.is_finite()
        {
            return Err(Error::Binding);
        }
        pairs.push((id, value));
    }
    if pairs.len() != source.parameter_storage().len() {
        return Err(Error::Binding);
    }
    let rebound = source.clone().bind(&pairs)?; // Replays every original source obligation.
    // BoundRegion has no public mutator. Its source snapshot identity and
    // binding map establish immutable lineage; rebind checks all original
    // finite-conversion obligations. Compare the eligible affine payloads
    // structurally, with scalar bit identity, before using them in a proof.
    for ((item, expected), actual) in source
        .occurrences()
        .iter()
        .zip(rebound.instructions())
        .zip(bound.instructions())
    {
        if expected.id() != actual.id()
            || expected.provenance() != actual.provenance()
            || expected.angle_targets() != actual.angle_targets()
            || (eligible(&item.operation)
                && !same_affine_operation(expected.operation(), actual.operation()))
        {
            return Err(Error::InvalidId);
        }
    }
    Ok(pairs)
}
fn fallback(error: &Error) -> bool {
    matches!(
        error,
        Error::NonFinite
            | Error::Budget(
                "rational pi precision"
                    | "exact angle coefficient bits"
                    | "symbolic parity coefficient bits"
            )
            | Error::Symbolic(quest_symbolic::Error::Exact(
                quest_symbolic::ExactError::CoefficientLimit
            ))
    )
}
/// Compiler extension over shared semantic capabilities.
pub trait BoundParityPasses: Sized {
    /// # Errors
    /// Rejects invalid semantic candidates, incompatible interfaces, and configured resource limits.
    fn parity_bound_candidate_from(
        &self,
        bound: &BoundRegion,
        start_offset: usize,
        options: ParityOptions,
        max_output_operations: usize,
    ) -> Result<(BoundRegion, ParityReport)>;
}
impl BoundParityPasses for QuantumRegion {
    /// Materialize one exact symbolic affine parity candidate at the admitted
    /// binding. The returned bound program keeps this ideal source's identity;
    /// the caller must retain the ideal source as the binding-domain authority.
    /// Candidate-only finite conversion failure retains the original bound input.
    /// # Errors
    /// Rejects foreign/malformed source, invalid binding, or exhausted search resources.
    #[expect(
        clippy::too_many_lines,
        reason = "Source replay, independent exact signature, and bound publication are one transaction"
    )]
    fn parity_bound_candidate_from(
        &self,
        bound: &BoundRegion,
        start_offset: usize,
        options: ParityOptions,
        max_output_operations: usize,
    ) -> Result<(BoundRegion, ParityReport)> {
        if start_offset > self.occurrences().len() {
            return Err(Error::InvalidId);
        }
        let narrowed = linear::program_preflight(self, options.linear)?;
        let mut work = linear::program_work(self, options.linear)?;
        work.charge(
            usize::try_from(self.binding_work_estimate()?)
                .map_err(|_| Error::Budget("symbolic parity binding work"))?,
        )?;
        let pairs = validate_source(self, bound, options)?;
        if bound.instructions().len() > max_output_operations {
            return Err(Error::Budget("symbolic parity output"));
        }
        let options = ParityOptions {
            linear: narrowed,
            ..options
        };
        let mut report = ParityReport {
            before_operations: bound.instructions().len(),
            after_operations: bound.instructions().len(),
            provenance: Arc::clone(bound.provenance_arc()),
            ..ParityReport::default()
        };
        let Some(offset) = self
            .occurrences()
            .iter()
            .enumerate()
            .skip(start_offset)
            .find_map(|(i, item)| eligible(&item.operation).then_some(i))
        else {
            report.work = work.used;
            return Ok((bound.clone(), report));
        };
        let count = self
            .occurrences()
            .iter()
            .skip(offset)
            .take(options.linear.max_window_operations)
            .take_while(|item| eligible(&item.operation))
            .count();
        if count == 0 {
            return Err(Error::Budget("symbolic parity window"));
        }
        // Two simultaneous exact phase tables, source/candidate summaries,
        // replay coefficients and the output sequence can coexist here.
        let coefficient_bytes = options.max_coefficient_bits.div_ceil(8);
        let scratch = coefficient_bytes
            .checked_mul(16)
            .and_then(|bytes| bytes.checked_add(2048))
            .and_then(|per_operation| per_operation.checked_mul(count))
            .ok_or(Error::Budget("symbolic parity scratch"))?;
        if scratch > options.linear.max_bytes {
            return Err(Error::Budget("symbolic parity scratch"));
        }
        work.charge(count)?;
        let end = offset
            .checked_add(count)
            .ok_or(Error::Budget("symbolic parity window"))?;
        report.candidate_window = Some((offset, end));
        report.considered_windows = 1;
        let window = self
            .occurrences()
            .get(offset..end)
            .ok_or(Error::InvalidId)?;
        let bound_window = bound
            .instructions()
            .get(offset..end)
            .ok_or(Error::InvalidId)?;
        let input = window
            .iter()
            .zip(bound_window)
            .map(|(item, instruction)| adapt(item, instruction, options, &mut work))
            .collect::<Result<Vec<_>>>();
        let input = match input {
            Ok(input) => input,
            Err(Error::Unsupported("signed-zero symbolic parity")) => {
                report.work = work.used;
                return Ok((bound.clone(), report));
            }
            Err(error) => return Err(error),
        };
        let outside = self
            .occurrences()
            .len()
            .checked_sub(count)
            .ok_or(Error::Budget("symbolic parity output"))?;
        let room = max_output_operations
            .checked_sub(outside)
            .ok_or(Error::Budget("symbolic parity output"))?;
        let expected = pair_signature(self.num_qubits(), &input, options, &mut work)?;
        let generated =
            (|| -> Result<(BoundRegion, crate::LinearRewrite, Arc<ProvenanceGraph>)> {
                let signature = angle_signature(self.num_qubits(), &input, options, &mut work)?;
                let limit = room
                    .min(options.linear.max_window_operations)
                    .checked_add(1)
                    .ok_or(Error::Budget("symbolic parity output"))?;
                let candidate = candidate(&signature, limit, options, &mut work)?;
                if pair_signature(self.num_qubits(), &candidate, options, &mut work)? != expected {
                    return Err(Error::NotUnitary);
                }
                let mut graph = ProvenanceGraph::edit(
                    Arc::clone(self.provenance_arc()),
                    options
                        .linear
                        .max_bytes
                        .min(self.limits().max_provenance_bytes),
                )?;
                let operations = candidate
                    .into_iter()
                    .map(|op| semantic(op, self.owner()))
                    .collect();
                let (replacement, rewrite) = linear::replacement_candidate(
                    window,
                    operations,
                    room,
                    options.linear,
                    &mut graph,
                )?;
                let size = outside
                    .checked_add(replacement.len())
                    .ok_or(Error::Budget("symbolic parity output"))?;
                let mut output = Vec::new();
                output
                    .try_reserve_exact(size)
                    .map_err(|_| Error::Budget("symbolic parity allocation"))?;
                output.extend_from_slice(self.occurrences().get(..offset).ok_or(Error::InvalidId)?);
                output.extend(replacement);
                output.extend_from_slice(self.occurrences().get(end..).ok_or(Error::InvalidId)?);
                let provenance = Arc::new(graph);
                let ideal = Self::from_parts(
                    self.owner(),
                    self.num_qubits(),
                    self.num_bits(),
                    self.parameter_storage().clone(),
                    output,
                    self.explicit_edges().clone(),
                    self.limits(),
                    Arc::clone(&provenance),
                )?;
                work.charge(
                    usize::try_from(ideal.binding_work_estimate()?)
                        .map_err(|_| Error::Budget("symbolic parity candidate binding work"))?,
                )?;
                let mut result = ideal.bind(&pairs)?;
                result.retain_source(self.snapshot_id(), result.binding_storage().clone())?;
                Ok((result, rewrite, provenance))
            })();
        match generated {
            Ok((result, rewrite, provenance)) => {
                report.accepted_windows = 1;
                report.after_operations = result.instructions().len();
                report.work = work.used;
                report.rewrites.push(rewrite);
                report.provenance = provenance;
                Ok((result, report))
            }
            Err(error) if fallback(&error) => {
                report.work = work.used;
                Ok((bound.clone(), report))
            }
            Err(error) => Err(error),
        }
    }
}
