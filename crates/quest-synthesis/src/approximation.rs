//! Ross--Selinger candidate and norm equations (arXiv:1403.2975).
//! Grid candidates use exact rational lattice reduction and bounded sphere
//! enumeration. Norm equations use bounded algebraic number theory, with direct
//! line-circle enumeration for tiny norms. Exhaustion is not nonexistence.
// Exact algebraic expressions use paper notation. IBig operands do not
// overflow; scalar exponents are admitted against coefficient limits first.
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::arithmetic_side_effects,
    clippy::redundant_pub_crate
)]
use crate::{Budget, Result, SynthesisError, SynthesisOptions, synthesize_matrix};
use dashu_base::{BitTest, Signed};
use dashu_int::IBig;
use quest_math::{
    ApproxCertificate, Axis, Cyclotomic, DyadicBox8, ExactMatrix, Gate, Operation, Sequence,
    Target, certify_rotation, dyadic_from_bits, reconstruct, rotation_enclosure,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approximation {
    certificate: ApproxCertificate,
    work: u64,
    grid_exponent: u32,
    seed: u64,
    limits: quest_math::Limits,
}
impl Approximation {
    #[must_use]
    pub const fn sequence(&self) -> &Sequence {
        self.certificate.candidate()
    }
    #[must_use]
    pub const fn certificate(&self) -> &ApproxCertificate {
        &self.certificate
    }
    #[must_use]
    pub const fn work(&self) -> u64 {
        self.work
    }
    #[must_use]
    pub const fn grid_exponent(&self) -> u32 {
        self.grid_exponent
    }
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }
    #[must_use]
    pub const fn limits(&self) -> quest_math::Limits {
        self.limits
    }
    #[must_use]
    pub const fn working_precision_bits(&self) -> usize {
        self.limits.precision_bits
    }
    #[must_use]
    pub const fn algorithm(&self) -> &'static str {
        crate::ROTATION_ALGORITHM
    }
}
// SplitMix64, with constants and wrapping behavior pinned as part of the algorithm.
struct Rng(u64);
impl Rng {
    const fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}
pub(crate) fn sqrt(n: &IBig, budget: &mut Budget) -> Result<IBig> {
    if n.is_negative() {
        return Err(SynthesisError::Invalid("negative integer square root"));
    }
    if n.is_zero() {
        return Ok(IBig::ZERO);
    }
    let mut x = IBig::ONE << n.bit_len().div_ceil(2);
    loop {
        budget.charge(1)?;
        let y = (&x + n / &x) >> 1;
        if y >= x {
            return Ok(x);
        }
        x = y;
    }
}
fn norm_solution(
    n: &IBig,
    m: &IBig,
    budget: &mut Budget,
    reverse: bool,
) -> Result<Option<[IBig; 4]>> {
    if n.is_zero() {
        return Ok(if m.is_zero() {
            Some(std::array::from_fn(|_| IBig::ZERO))
        } else {
            None
        });
    }
    let radius = sqrt(n, budget)?;
    let mut b = -&radius;
    while b <= radius {
        let mut d = -&radius;
        while d <= radius {
            budget.charge(1)?;
            let bb = if reverse { -&b } else { b.clone() };
            let dd = d.clone();
            let rest = n - &bb * &bb - &dd * &dd;
            if !rest.is_negative() {
                let p = &bb - &dd;
                let q = &bb + &dd;
                let denominator = &p * &p + &q * &q;
                if denominator.is_zero() && m.is_zero() {
                    let mut a = IBig::ZERO;
                    let bound = sqrt(&rest, budget)?;
                    while a <= bound {
                        budget.charge(1)?;
                        let c2 = &rest - &a * &a;
                        let c = sqrt(&c2, budget)?;
                        if &c * &c == c2 {
                            return Ok(Some([a, bb, c, dd]));
                        }
                        a += 1;
                    }
                } else if !denominator.is_zero() {
                    let squared_m = m * m;
                    let discriminant = &denominator * &rest - squared_m;
                    if !discriminant.is_negative() {
                        let root = sqrt(&discriminant, budget)?;
                        if &root * &root == discriminant {
                            for root in [root.clone(), -root] {
                                let an = &p * m + &q * &root;
                                let cn = &q * m - &p * &root;
                                if (&an % &denominator).is_zero() && (&cn % &denominator).is_zero()
                                {
                                    return Ok(Some([
                                        an / &denominator,
                                        bb,
                                        cn / &denominator,
                                        dd,
                                    ]));
                                }
                            }
                        }
                    }
                }
            }
            d += 1;
        }
        b += 1;
    }
    Ok(None)
}
fn scale_interval(lower: &IBig, upper: &IBig, value: &IBig) -> (IBig, IBig) {
    if value.is_negative() {
        (upper * value, lower * value)
    } else {
        (lower * value, upper * value)
    }
}
fn max_product(a: &IBig, b: &IBig, c: &IBig, d: &IBig) -> IBig {
    (a * c).max(a * d).max(b * c).max(b * d)
}
fn op(gate: Gate) -> Operation {
    Operation {
        gate,
        targets: vec![0],
        controls: vec![],
    }
}
fn basis(axis: Axis, sequence: Sequence, budget: &mut Budget) -> Result<Sequence> {
    let (prefix, suffix) = match axis {
        Axis::Z => (vec![], vec![]),
        Axis::X => (vec![op(Gate::H)], vec![op(Gate::H)]),
        Axis::Y => (
            vec![op(Gate::Sdg), op(Gate::H)],
            vec![op(Gate::H), op(Gate::S)],
        ),
    };
    budget.output(sequence.operations.len(), prefix.len() + suffix.len())?;
    let mut operations = prefix;
    operations.extend(sequence.operations);
    operations.extend(suffix);
    Ok(Sequence {
        qubits: 1,
        operations,
    })
}

fn finish_candidate(
    coefficients: [IBig; 4],
    t: [IBig; 4],
    exponent: u32,
    target: &Target,
    epsilon_bits: u64,
    budget: &mut Budget,
) -> Result<Option<Approximation>> {
    let options = budget.options.clone();
    let u = Cyclotomic::new(coefficients, exponent, options.limits)?;
    let t = Cyclotomic::new(t, exponent, options.limits)?;
    let matrix = ExactMatrix::from_entries(
        1,
        vec![
            u.clone(),
            t.conjugated(options.limits)?
                .checked_mul(&Cyclotomic::omega(4), options.limits)?,
            t,
            u.conjugated(options.limits)?,
        ],
        options.limits,
    )?;
    let remaining = options.max_work.saturating_sub(budget.used);
    let exact = synthesize_matrix(
        &matrix,
        SynthesisOptions {
            max_work: remaining,
            ..options.clone()
        },
    )?;
    budget.used = budget.used.saturating_add(exact.work());
    let sequence = basis(target.axis, exact.sequence().clone(), budget)?;
    let attempts = [64, 128, 256, 512, 1024, 2048, 4096]
        .iter()
        .filter(|&&p| p <= options.limits.precision_bits)
        .count();
    budget.rotation_certificate(sequence.operations.len(), attempts)?;
    match certify_rotation(&sequence, target, epsilon_bits, options.limits) {
        Ok(certificate) => {
            budget.charge(0)?;
            Ok(Some(Approximation {
                certificate,
                work: budget.used,
                grid_exponent: exponent * 2,
                seed: options.seed,
                limits: options.limits,
            }))
        }
        Err(quest_math::Error::NotCertified) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Enclose the Hadamard entries used for the grid's exact sqrt(2) coordinates.
fn hadamard_enclosure(bits: usize, limits: quest_math::Limits) -> Result<DyadicBox8> {
    Ok(DyadicBox8::from_exact(
        &reconstruct(
            &Sequence {
                qubits: 1,
                operations: vec![op(Gate::H)],
            },
            limits,
        )?,
        bits,
        limits,
    )?)
}

/// Generate SU(2) grid candidates, solve their exact norm equations, synthesize
/// their exact matrices, then independently certify the requested exact target.
///
/// The precision is request-owned and no global float context is used.
/// # Errors
/// Rejects malformed inputs, exhausted grid/work/output/precision resources, and
/// interval uncertainty; finite search exhaustion never asserts impossibility.
#[allow(clippy::needless_pass_by_value)] // Public entry points own request configuration.
pub fn approximate_rotation(
    target: &Target,
    epsilon_bits: u64,
    options: SynthesisOptions,
) -> Result<Approximation> {
    let mut budget = Budget {
        options: options.clone(),
        used: 0,
    };
    budget.charge(1)?;
    let epsilon = dyadic_from_bits(epsilon_bits, options.limits)?;
    if epsilon <= quest_math::RBig::ZERO || epsilon > quest_math::RBig::ONE {
        return Err(SynthesisError::Invalid("epsilon must be in (0,1]"));
    }
    let bits = options.limits.precision_bits;
    let rz = Target {
        axis: Axis::Z,
        angle: target.angle.clone(),
    };
    budget.rotation_certificate(0, 1)?;
    let enclosure = rotation_enclosure(&rz, bits, options.limits).map_err(|error| {
        if error == quest_math::Error::NotCertified {
            SynthesisError::PrecisionUnresolved {
                precision_bits: bits,
            }
        } else {
            SynthesisError::Math(error)
        }
    })?;
    let root = hadamard_enclosure(bits, options.limits)?;
    let (root_lo, root_hi) = root.coordinate(0)?;
    let (xlo, xhi) = enclosure.coordinate(0)?;
    let (ylo, yhi) = enclosure.coordinate(1)?;
    let grid = IBig::ONE << bits;
    let en = epsilon.numerator();
    let ed = &IBig::from(epsilon.denominator().clone());
    let cap_n = ed * ed * 4 - en * en;
    let cap_d = ed * ed * 4;
    let mut rng = Rng(options.seed);
    let reverse = rng.next() & 1 != 0;
    let lattice = crate::grid::Grid::new(&enclosure, &epsilon, &mut budget)?;
    let mut result = budget.with_reserved_bytes(lattice.reserved_bytes(), |budget| {
        for exponent in 0..=options.max_grid_exponent / 2 {
            budget.charge(1)?;
            if u64::from(exponent).saturating_mul(2).saturating_add(4)
                > options.limits.coefficient_bits
            {
                return Err(SynthesisError::Budget {
                    resource: "grid coefficient bits",
                });
            }
            let radius = IBig::ONE
                << usize::try_from(exponent).map_err(|_| SynthesisError::Budget {
                    resource: "synthesis exponent",
                })?;
            let norm = &radius * &radius;
            let result = lattice.enumerate(exponent, budget, |[a, b, c, d], budget| {
                budget.charge(1)?;
                let n = &norm - &a * &a - &b * &b - &c * &c - &d * &d;
                let m = -(&a * (&b - &d) + &c * (&b + &d));
                if n < IBig::ZERO || &n * &n < &m * &m * 2 {
                    return Ok(None);
                }
                let (rxlo, rxhi) = scale_interval(root_lo, root_hi, &(&b - &d));
                let (iylo, iyhi) = scale_interval(root_lo, root_hi, &(&b + &d));
                let uxlo = &a * &grid + rxlo;
                let uxhi = &a * &grid + rxhi;
                let uylo = &c * &grid + iylo;
                let uyhi = &c * &grid + iyhi;
                let dot = max_product(&uxlo, &uxhi, xlo, xhi) + max_product(&uylo, &uyhi, ylo, yhi);
                if dot * &cap_d < &cap_n * &grid * &grid * &radius {
                    return Ok(None);
                }
                let solution = if u64::try_from(n.bit_len()).unwrap_or(u64::MAX) <= 12 {
                    norm_solution(&n, &m, budget, reverse)?
                } else {
                    match crate::norm_equation::solve(&n, &m, budget, rng.next())? {
                        crate::norm_equation::NormOutcome::Solution(t) => Some(t),
                        crate::norm_equation::NormOutcome::NoSolution
                        | crate::norm_equation::NormOutcome::Unresolved => None,
                    }
                };
                let Some(t) = solution else {
                    return Ok(None);
                };
                finish_candidate([a, b, c, d], t, exponent, target, epsilon_bits, budget)
            })?;
            if let Some(result) = result {
                return Ok(result);
            }
        }
        Err(SynthesisError::Budget {
            resource: "grid exponent",
        })
    })?;
    result.limits = options.limits;
    Ok(result)
}
