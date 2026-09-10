use crate::common::{ib_to_bf_prec, pi};
use crate::common::{reset_prec_bits, set_prec_bits};
use dashu_base::BitTest;
use dashu_float::round::mode::HalfEven;
use dashu_float::FBig;
use dashu_int::{IBig, UBig};
use rand::{rngs::StdRng, SeedableRng};
use std::str::FromStr;

#[derive(Debug)]
pub struct DiophantineData {
    pub diophantine_timeout: u128,
    pub factoring_timeout: u128,
    pub rng: StdRng,
}

#[derive(Debug)]
pub struct GridSynthConfig {
    pub theta: FBig<HalfEven>,
    pub epsilon: FBig<HalfEven>,
    pub verbose: bool,
    pub measure_time: bool,
    pub diophantine_data: DiophantineData,
    pub up_to_phase: bool,
    pub compute_error: bool,
    pub limits: SearchLimits,
}

#[derive(Debug, Clone, Copy)]
pub struct SearchLimits {
    pub max_grid_exponent: i64,
    pub max_candidates: usize,
    pub max_output_gates: usize,
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            max_grid_exponent: 1024,
            max_candidates: 1_000_000,
            max_output_gates: 16_384,
        }
    }
}

impl GridSynthConfig {
    /// Turns on or off checking solutions at the end of the run
    pub fn with_compute_error(self, compute_error: bool) -> Self {
        Self {
            compute_error,
            ..self
        }
    }
}

/// The result of running the gridsynth algorithm
pub struct GridSynthResult {
    /// List of gates.
    pub gates: String,

    /// The global phase factor.
    pub global_phase: bool,

    /// If error is computed, stores the error.
    pub error: Option<f64>,

    /// If error is computed, stores whether approximation is correct.
    pub is_correct: Option<bool>,
}

pub fn parse_decimal_with_exponent(input: &str) -> Option<(IBig, IBig)> {
    let input = input.trim();
    let (sign, body) = if let Some(s) = input.strip_prefix('-') {
        (-1, s)
    } else if let Some(s) = input.strip_prefix('+') {
        (1, s)
    } else {
        (1, input)
    };

    let (base_str, exp_str) = match body.split_once(['e', 'E']) {
        Some((b, e)) => (b, e),
        None => (body, "0"),
    };

    let mut parts = base_str.split('.');
    let int_part = match parts.next() {
        Some(part) => part,
        _ => return None,
    };
    let frac_part: &str = parts.next().unwrap_or_default();
    if parts.next().is_some() {
        return None;
    }
    let digits = format!("{}{}", int_part, frac_part);
    let decimal_digits = frac_part.len() as i32;

    let exponent: i32 = exp_str.parse().ok()?;
    let scale = exponent - decimal_digits;

    let mut numerator = IBig::from_str(&digits).ok()? * sign;
    let mut denominator = IBig::from(1);

    match scale.cmp(&0) {
        std::cmp::Ordering::Greater => {
            numerator *= IBig::from(10u8).pow(scale as usize);
        }
        std::cmp::Ordering::Less => {
            denominator = IBig::from(10u8).pow((-scale) as usize);
        }
        std::cmp::Ordering::Equal => {}
    }

    Some((numerator, denominator))
}

/// Creates the default config to easily call the code from other rust packages.
/// `seed` is used to set single RNG that is used through the call to `gridsynth`.
pub fn config_from_theta_epsilon(
    theta: f64,
    epsilon: f64,
    seed: u64,
    verbose: bool,
    up_to_phase: bool,
) -> GridSynthConfig {
    let (theta_num, theta_den) = parse_decimal_with_exponent(&theta.to_string()).unwrap();

    // The desired floating precision is initialized in module common.
    // It has the initial value the first time this function is called.
    // But, on subsequent calls, the precision has changed.
    // We reset it so that the precison is the same at the beginning of every synthesis.
    reset_prec_bits();
    let theta = ib_to_bf_prec(theta_num) / ib_to_bf_prec(theta_den);
    let (epsilon_num, epsilon_den) = parse_decimal_with_exponent(&epsilon.to_string()).unwrap();
    // The magic number 12 safely overapproximates the bits of precision.
    let calculated_prec_bits =
        12 * (epsilon_den.ilog(&UBig::from(10u8)) - epsilon_num.ilog(&UBig::from(10u8)));
    let prec_bits: usize = calculated_prec_bits;
    // Using precision that is too low can cause errors. For example stack overflow and sigabrt.
    // We don't actually need our target precision tied to working precision.
    let prec_bits = if prec_bits < 16 { 16 } else { prec_bits };
    set_prec_bits(prec_bits);
    let epsilon = ib_to_bf_prec(epsilon_num) / ib_to_bf_prec(epsilon_den);
    let diophantine_timeout = 200u128;
    let factoring_timeout = 50u128;
    let time = false;

    let rng: StdRng = SeedableRng::seed_from_u64(seed);
    let diophantine_data = DiophantineData {
        diophantine_timeout,
        factoring_timeout,
        rng,
    };

    GridSynthConfig {
        theta,
        epsilon,
        verbose,
        measure_time: time,
        diophantine_data,
        up_to_phase,
        compute_error: false,
        limits: SearchLimits::default(),
    }
}

/// Construct a bounded configuration from exact integer ratios.
///
/// Precision is installed and solver caches are cleared before any floating
/// target, epsilon, or pi value is constructed.
pub fn config_from_exact_ratios(
    theta_numerator: IBig,
    theta_denominator: IBig,
    theta_is_pi_multiple: bool,
    epsilon_numerator: IBig,
    epsilon_denominator: IBig,
    seed: u64,
    precision_bits: usize,
    limits: SearchLimits,
) -> Result<GridSynthConfig, String> {
    if !(256..=1024).contains(&precision_bits) {
        return Err("working precision must be in 256..=1024 bits".to_string());
    }
    if theta_denominator <= IBig::ZERO
        || epsilon_denominator <= IBig::ZERO
        || epsilon_numerator <= IBig::ZERO
    {
        return Err("ratios require positive denominators and epsilon".to_string());
    }
    if !(0..=1024).contains(&limits.max_grid_exponent)
        || limits.max_candidates == 0
        || limits.max_candidates > 1_000_000
        || limits.max_output_gates == 0
        || limits.max_output_gates > 16_384
    {
        return Err("search limits exceed hardened bounds".to_string());
    }
    set_prec_bits(precision_bits);
    crate::clear_caches();
    let theta_ratio = ib_to_bf_prec(theta_numerator) / ib_to_bf_prec(theta_denominator);
    let theta = if theta_is_pi_multiple {
        theta_ratio * pi()
    } else {
        theta_ratio
    };
    let epsilon = ib_to_bf_prec(epsilon_numerator) / ib_to_bf_prec(epsilon_denominator);
    Ok(GridSynthConfig {
        theta,
        epsilon,
        verbose: false,
        measure_time: false,
        diophantine_data: DiophantineData {
            diophantine_timeout: 200,
            factoring_timeout: 50,
            rng: SeedableRng::seed_from_u64(seed),
        },
        up_to_phase: false,
        compute_error: false,
        limits,
    })
}

/// Parse bounded base-ten integers and delegate to the exact-ratio constructor.
pub fn config_from_exact_strings(
    theta_numerator: &str,
    theta_denominator: &str,
    theta_is_pi_multiple: bool,
    epsilon_numerator: &str,
    epsilon_denominator: &str,
    seed: u64,
    precision_bits: usize,
    limits: SearchLimits,
) -> Result<GridSynthConfig, String> {
    for value in [
        theta_numerator,
        theta_denominator,
        epsilon_numerator,
        epsilon_denominator,
    ] {
        if value.len() > 1_235 {
            return Err("exact-ratio integer exceeds 4096-bit input limit".to_string());
        }
    }
    let parse = |value: &str, label: &str| {
        let integer = IBig::from_str(value).map_err(|_| format!("invalid {label}"))?;
        if integer.bit_len() > 4_096 {
            return Err("exact-ratio integer exceeds 4096-bit input limit".to_string());
        }
        Ok(integer)
    };
    config_from_exact_ratios(
        parse(theta_numerator, "theta numerator")?,
        parse(theta_denominator, "theta denominator")?,
        theta_is_pi_multiple,
        parse(epsilon_numerator, "epsilon numerator")?,
        parse(epsilon_denominator, "epsilon denominator")?,
        seed,
        precision_bits,
        limits,
    )
}

#[cfg(test)]
mod hardening_tests {
    use super::*;

    #[test]
    fn exact_ratio_rejects_value_over_bit_limit_even_when_decimal_length_fits() {
        let oversized = "9".repeat(1_235);
        let error = config_from_exact_strings(
            &oversized,
            "1",
            false,
            "1",
            "8",
            0,
            256,
            SearchLimits::default(),
        )
        .expect_err("more than 4096 integer bits must be rejected");
        assert_eq!(error, "exact-ratio integer exceeds 4096-bit input limit");
    }
}
