//! Hardened adapter from exact project targets to the isolated grid synthesizer.

use quest_math::{
    AngleTarget, Axis, Gate, Limits, Operation, Rational, Sequence, Target, certify_rotation,
    dyadic_from_bits,
};
use rsgridsynth::{
    config::{SearchLimits, config_from_exact_strings},
    gridsynth::try_gridsynth_gates,
};

const MAX_INPUT_BITS: u64 = 4_096;
const MIN_WORKING_PRECISION: usize = 256;
const MAX_WORKING_PRECISION: usize = 1_024;
const SEARCH_MARGIN: u32 = 8;

/// Synthesize and independently certify a full-phase single-qubit rotation.
///
/// The returned precision is the binary working precision installed before the
/// vendored engine constructs any floating constants or caches.
///
/// # Errors
/// Rejects malformed or over-budget inputs, exhausted bounded search, malformed
/// candidates, and candidates whose full matrix cannot be certified within the
/// requested tolerance.
pub fn synthesize(
    target: &Target,
    epsilon_bits: u64,
    seed: u64,
) -> Result<(Sequence, usize), String> {
    let epsilon_float = f64::from_bits(epsilon_bits);
    if !epsilon_float.is_finite() || epsilon_float <= 0.0 || epsilon_float > 1.0 {
        return Err("epsilon must be finite and in (0, 1]".to_string());
    }
    let limits = Limits {
        precision_bits: MAX_WORKING_PRECISION,
        ..Limits::default()
    };
    let epsilon = dyadic_from_bits(epsilon_bits, limits)
        .map_err(|error| format!("invalid epsilon identity: {error}"))?;
    let search_epsilon = Rational::new(
        epsilon.numer().clone(),
        std::ops::Mul::mul(epsilon.denom(), SEARCH_MARGIN),
    );
    let precision = working_precision(&search_epsilon)?;
    let (theta, theta_is_pi_multiple) = exact_theta(target, limits)?;
    let search_limits = SearchLimits {
        max_grid_exponent: i64::try_from(precision)
            .map_err(|_| "working precision does not fit grid exponent".to_string())?,
        max_candidates: 1_000_000,
        max_output_gates: limits
            .gates
            .checked_sub(4)
            .ok_or_else(|| "gate limit is too small for basis changes".to_string())?,
    };
    let mut config = config_from_exact_strings(
        &theta.numer().to_string(),
        &theta.denom().to_string(),
        theta_is_pi_multiple,
        &search_epsilon.numer().to_string(),
        &search_epsilon.denom().to_string(),
        seed,
        precision,
        search_limits,
    )?;
    let result = try_gridsynth_gates(&mut config)?;
    if result.global_phase {
        return Err("exact-phase synthesis unexpectedly omitted a phase".to_string());
    }
    let rz = decode_chronological(&result.gates, search_limits.max_output_gates)?;
    let candidate = change_basis(target.axis, rz, limits.gates)?;
    certify_rotation(&candidate, target, epsilon_bits, limits)
        .map_err(|error| format!("independent full-phase certificate failed: {error}"))?;
    Ok((candidate, precision))
}

fn exact_theta(target: &Target, limits: Limits) -> Result<(Rational, bool), String> {
    match &target.angle {
        AngleTarget::DyadicRadians { bits } => {
            let value = f64::from_bits(*bits);
            if !value.is_finite() {
                return Err("rotation angle must be finite".to_string());
            }
            dyadic_from_bits(*bits, limits)
                .map(|value| (value, false))
                .map_err(|error| format!("invalid dyadic target: {error}"))
        }
        AngleTarget::RationalPi {
            numerator,
            denominator,
        } => {
            if denominator.bits() == 0 {
                return Err("rational-pi denominator cannot be zero".to_string());
            }
            if numerator.bits() > MAX_INPUT_BITS || denominator.bits() > MAX_INPUT_BITS {
                return Err("rational-pi input exceeds 4096 bits".to_string());
            }
            Ok((Rational::new(numerator.clone(), denominator.clone()), true))
        }
        AngleTarget::AffinePi { .. } => Err(
            "affine-pi target requires an engine interface accepting exact radians plus exact pi coefficient"
                .to_string(),
        ),
    }
}

fn working_precision(epsilon: &Rational) -> Result<usize, String> {
    if epsilon.numer().to_string().starts_with('-') || epsilon.numer().to_string() == "0" {
        return Err("search epsilon must be positive".to_string());
    }
    let numerator_bits = epsilon.numer().bits();
    let denominator_bits = epsilon.denom().bits();
    let mut logarithm = denominator_bits.saturating_sub(numerator_bits);
    let shift = usize::try_from(logarithm)
        .map_err(|_| "epsilon exponent does not fit memory index".to_string())?;
    if std::ops::Shl::shl(epsilon.numer(), shift) < *epsilon.denom() {
        logarithm = logarithm
            .checked_add(1)
            .ok_or_else(|| "epsilon exponent overflow".to_string())?;
    }
    let requested = usize::try_from(logarithm)
        .ok()
        .and_then(|value| value.checked_mul(4))
        .and_then(|value| value.checked_add(64))
        .ok_or_else(|| "working precision overflow".to_string())?;
    let precision = requested.max(MIN_WORKING_PRECISION);
    if precision > MAX_WORKING_PRECISION {
        return Err("requested epsilon requires more than 1024 working bits".to_string());
    }
    Ok(precision)
}

fn decode_chronological(gates: &str, limit: usize) -> Result<Vec<Operation>, String> {
    if gates.len() > limit || !gates.is_ascii() {
        return Err("candidate output exceeds the gate or encoding limit".to_string());
    }
    let mut operations = Vec::new();
    operations
        .try_reserve_exact(gates.len())
        .map_err(|_| "candidate allocation failed".to_string())?;
    for gate in gates.chars().rev() {
        let gate = match gate {
            'I' => continue,
            'H' => Gate::H,
            'X' => Gate::X,
            'S' => Gate::S,
            'T' => Gate::T,
            'W' => Gate::W,
            _ => return Err(format!("unsupported rsgridsynth gate {gate:?}")),
        };
        operations.push(operation(gate));
    }
    Ok(operations)
}

fn change_basis(axis: Axis, rz: Vec<Operation>, limit: usize) -> Result<Sequence, String> {
    let extra = match axis {
        Axis::Z => 0,
        Axis::X => 2,
        Axis::Y => 4,
    };
    let count = rz
        .len()
        .checked_add(extra)
        .ok_or_else(|| "basis-change gate count overflow".to_string())?;
    if count > limit {
        return Err("certified candidate exceeds gate limit".to_string());
    }
    let mut operations = Vec::new();
    operations
        .try_reserve_exact(count)
        .map_err(|_| "basis-change allocation failed".to_string())?;
    match axis {
        Axis::Z => operations.extend(rz),
        Axis::X => {
            operations.push(operation(Gate::H));
            operations.extend(rz);
            operations.push(operation(Gate::H));
        }
        Axis::Y => {
            operations.push(operation(Gate::Sdg));
            operations.push(operation(Gate::H));
            operations.extend(rz);
            operations.push(operation(Gate::H));
            operations.push(operation(Gate::S));
        }
    }
    Ok(Sequence {
        qubits: 1,
        operations,
    })
}

fn operation(gate: Gate) -> Operation {
    Operation {
        targets: if gate == Gate::W { Vec::new() } else { vec![0] },
        gate,
        controls: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, PoisonError};

    const EPSILON_BITS: u64 = 1.0e-12_f64.to_bits();
    static SYNTHESIS: Mutex<()> = Mutex::new(());

    fn isolated_synthesis() -> MutexGuard<'static, ()> {
        SYNTHESIS.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn rational_pi(axis: Axis, numerator: i64, denominator: i64) -> Target {
        Target {
            axis,
            angle: AngleTarget::RationalPi {
                numerator: std::convert::From::from(numerator),
                denominator: std::convert::From::from(denominator),
            },
        }
    }

    #[test]
    fn affine_pi_requires_an_exact_two_term_engine_interface() {
        let target = Target {
            axis: Axis::Z,
            angle: AngleTarget::AffinePi {
                radians_numerator: 1.into(),
                radians_denominator: 3.into(),
                pi_numerator: 1.into(),
                pi_denominator: 5.into(),
            },
        };
        assert!(exact_theta(&target, Limits::default()).is_err());
    }

    fn independently_certify(sequence: &Sequence, target: &Target) {
        let certificate = certify_rotation(
            sequence,
            target,
            EPSILON_BITS,
            Limits {
                precision_bits: MAX_WORKING_PRECISION,
                ..Limits::default()
            },
        )
        .expect("synthesized sequence must carry a full-phase certificate");
        assert_eq!(certificate.candidate(), sequence);
        assert_eq!(certificate.target(), target);
        assert_eq!(certificate.epsilon_bits(), EPSILON_BITS);
    }

    #[test]
    fn exact_pi_over_four_preserves_scalar_phase_at_twelve_digits() {
        let _isolation = isolated_synthesis();
        let target = rational_pi(Axis::Z, 1, 4);
        let (sequence, precision) = synthesize(&target, EPSILON_BITS, 1_234)
            .expect("exact Clifford+T target must synthesize");
        assert_eq!(precision, MIN_WORKING_PRECISION);
        assert!(
            sequence
                .operations
                .iter()
                .any(|operation| operation.gate == Gate::W)
        );
        independently_certify(&sequence, &target);
        let missing_phase = Sequence {
            qubits: sequence.qubits,
            operations: sequence
                .operations
                .into_iter()
                .filter(|operation| operation.gate != Gate::W)
                .collect(),
        };
        assert!(
            certify_rotation(
                &missing_phase,
                &target,
                EPSILON_BITS,
                Limits {
                    precision_bits: MAX_WORKING_PRECISION,
                    ..Limits::default()
                },
            )
            .is_err()
        );
    }

    #[test]
    fn seeded_pi_over_seven_is_deterministic_and_certified_at_twelve_digits() {
        let _isolation = isolated_synthesis();
        let target = rational_pi(Axis::Z, 1, 7);
        let first = synthesize(&target, EPSILON_BITS, 0x5eed)
            .expect("seeded nontrivial target must synthesize");
        let second = synthesize(&target, EPSILON_BITS, 0x5eed)
            .expect("same seeded target must synthesize again");
        assert_eq!(first, second);
        independently_certify(&first.0, &target);
    }

    #[test]
    fn x_and_y_basis_changes_retain_full_phase() {
        let _isolation = isolated_synthesis();
        for axis in [Axis::X, Axis::Y] {
            let target = rational_pi(axis, 1, 4);
            let (sequence, _) = synthesize(&target, EPSILON_BITS, 9)
                .expect("basis-changed exact target must synthesize");
            independently_certify(&sequence, &target);
        }
    }

    #[test]
    fn impossible_precision_is_rejected_before_search() {
        let _isolation = isolated_synthesis();
        let target = rational_pi(Axis::Z, 1, 7);
        let error = synthesize(&target, f64::MIN_POSITIVE.to_bits(), 0)
            .expect_err("precision above the hard cap must fail");
        assert!(error.contains("more than 1024 working bits"));
    }
}
