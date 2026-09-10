// Copyright (c) IBM
// Licensed under the MIT License. See LICENSE file in the project root for full license information.

use dashu_base::RemEuclid;
use dashu_float::{round::mode::HalfEven, FBig};
use dashu_int::IBig;
use std::sync::atomic::{AtomicUsize, Ordering};

const PREC_BITS_INITIAL: usize = 1000;
pub static PREC_BITS: AtomicUsize = AtomicUsize::new(PREC_BITS_INITIAL);

// Reset precision to the initial value
pub fn reset_prec_bits() {
    PREC_BITS.store(PREC_BITS_INITIAL, Ordering::Relaxed);
}

pub fn set_prec_bits(bits: usize) {
    PREC_BITS.store(bits, Ordering::Relaxed);
}

pub fn get_prec_bits() -> usize {
    PREC_BITS.load(Ordering::Relaxed)
}

pub fn pi() -> FBig<HalfEven> {
    let pi_str = "3141592653589793238462643383279502884197169399375105820974944592307816406286208998628034825342117067982148086513282306647093844609550582231725359408128481117450284102701938521105559644622948954930381964428810975665933446128475648233786783165271201909145648566923460348610454326648213393607260249141273724587006606315588174881520920962829254091715364367892590360011330530548820466521384146951941511609";
    let decimals = IBig::from(10u8).pow(399);
    ib_to_bf_prec(IBig::from_str_radix(pi_str, 10).unwrap()) / ib_to_bf_prec(decimals)
}

fn reduce_to_pi_range(mut x: FBig<HalfEven>) -> FBig<HalfEven> {
    let pi = pi();
    let tau: FBig<HalfEven> = 2 * pi.clone();
    x = x.rem_euclid(tau.clone());
    if x > pi {
        x -= tau;
    }
    x
}

pub fn cos_fbig(x: &FBig<HalfEven>) -> FBig<HalfEven> {
    let t = reduce_to_pi_range(x.clone());

    let mut term = ib_to_bf_prec(IBig::ONE);
    let mut sum = term.clone();
    let t2 = &t * &t;

    for i in 1..get_prec_bits() {
        let denom = IBig::from((2 * i - 1) * (2 * i));
        term = -term * &t2 / denom;
        sum += &term;

        if term == ib_to_bf_prec(IBig::ZERO) {
            break;
        }
    }
    sum
}

pub fn sin_fbig(x: &FBig<HalfEven>) -> FBig<HalfEven> {
    let t = reduce_to_pi_range(x.clone());

    let mut term = t.clone();
    let mut sum = term.clone();
    let t2 = &t * &t;

    for i in 1..get_prec_bits() {
        let denom = IBig::from((2 * i) * (2 * i + 1));
        term = -term * &t2 / denom;
        sum += &term;

        if term == ib_to_bf_prec(IBig::ZERO) {
            break;
        }
    }
    sum
}

pub fn ib_to_bf_prec(x: IBig) -> FBig<HalfEven> {
    FBig::from(x).with_precision(get_prec_bits()).value()
}

pub fn fb_with_prec(x: FBig<HalfEven>) -> FBig<HalfEven> {
    x.with_precision(get_prec_bits()).value()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashu_float::round::mode::HalfEven;
    use dashu_float::FBig;
    use dashu_int::ops::Abs;
    use rand::Rng;
    use std::f64::consts::PI as PI_F64;

    fn to_fbig(x: f64) -> FBig<HalfEven> {
        FBig::<HalfEven>::try_from(x)
            .unwrap()
            .with_precision(get_prec_bits())
            .value()
    }

    fn approx_eq(a: &FBig<HalfEven>, b: &FBig<HalfEven>, tol_bits: usize) -> bool {
        let diff = (a - b).abs();
        let tol = ib_to_bf_prec(IBig::ONE)
            .with_precision(get_prec_bits())
            .value()
            / FBig::from(1u64 << tol_bits)
                .with_precision(get_prec_bits())
                .value();
        diff <= tol
    }

    #[test]
    fn test_sin_fbig_random() {
        let mut rng = rand::rng();
        for _ in 0..100 {
            let x_f64 = rng.random_range(-10.0 * PI_F64..=10.0 * PI_F64);
            let x = to_fbig(x_f64);
            let expected = to_fbig(x_f64.sin());
            let result = sin_fbig(&x);
            assert!(
                approx_eq(&result, &expected, 50),
                "sin({}) = {}, expected {}, diff = {}",
                x_f64,
                result,
                expected,
                (&result - &expected).abs()
            );
        }
    }

    #[test]
    fn test_cos_fbig_random() {
        let mut rng = rand::rng();
        for _ in 0..100 {
            let x_f64 = rng.random_range(-10.0 * PI_F64..=10.0 * PI_F64);
            let x = to_fbig(x_f64);
            let expected = to_fbig(x_f64.cos());
            let result = cos_fbig(&x);
            assert!(
                approx_eq(&result, &expected, 50),
                "cos({}) = {}, expected {}, diff = {}",
                x_f64,
                result,
                expected,
                (&result - &expected).abs()
            );
        }
    }
}
