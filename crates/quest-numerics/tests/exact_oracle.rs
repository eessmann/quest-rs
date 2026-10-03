#![allow(
	clippy::unwrap_used,
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	clippy::float_cmp,
	clippy::panic,
	clippy::panic_in_result_fn,
	clippy::needless_pass_by_value,
	reason = "Bounded independent exact-rational oracle fixtures"
)]
//! Independent transcendental bounds from rational Taylor series and inequalities.
//! The floating-point library only supplies the exact representation of endpoints.
use dashu_int::{IBig, UBig};
use dashu_ratio::RBig;
use quest_numerics::arithmetic::*;

type Bounds = (RBig, RBig);

fn ratio(numerator: i64, denominator: i64) -> RBig {
	RBig::from_parts_signed(IBig::from(numerator), IBig::from(denominator))
}
fn integer(value: i64) -> RBig {
	ratio(value, 1)
}
fn represented(value: &Binary) -> RBig {
	let significand = value.repr().significand();
	// Zero's special exponent is not a dyadic scale; return before any shift.
	if significand.is_zero() {
		return integer(0);
	}
	let exponent = value.repr().exponent();
	if exponent >= 0 {
		RBig::from(significand << usize::try_from(exponent).unwrap())
	} else {
		RBig::from_parts(significand.clone(), UBig::ONE << exponent.unsigned_abs())
	}
}
fn encloses(value: &MpInterval, (lower, upper): Bounds) {
	assert!(represented(value.lower()) <= lower, "lower endpoint");
	assert!(represented(value.upper()) >= upper, "upper endpoint");
}

// For x >= 0, the omitted positive terms have ratio at most x/(N+2).
// Their sum is bounded by a geometric series starting at term N+1.
fn exp_bounds(value: &RBig) -> Bounds {
	if value < &integer(0) {
		let (lower, upper) = exp_bounds(&-value);
		return (integer(1) / upper, integer(1) / lower);
	}
	let mut term = integer(1);
	let mut sum = term.clone();
	for index in 1..=512 {
		term = term * value / integer(index);
		sum += &term;
	}
	let first_omitted = term * value / integer(513);
	let ratio = value / integer(514);
	assert!(ratio < integer(1));
	let tail = first_omitted / (integer(1) - ratio);
	(sum.clone(), sum + tail)
}

// Taylor's theorem bounds the remainder by |x|^(degree+1)/(degree+1)!:
// every derivative of sine and cosine has magnitude at most one.
fn trig_bounds(value: &RBig, sine: bool) -> Bounds {
	let square = value * value;
	let mut term = if sine { value.clone() } else { integer(1) };
	let mut sum = term.clone();
	for index in 1..=256 {
		let degree = if sine { 2 * index + 1 } else { 2 * index };
		term = -term * &square / integer(degree * (degree - 1));
		sum += &term;
	}
	let next_degree = if sine { 514 } else { 513 };
	let mut remainder = term * value / integer(next_degree);
	if remainder < integer(0) {
		remainder = -remainder;
	}
	(&sum - &remainder, sum + remainder)
}

// log(x) = 2 atanh((x-1)/(x+1)). With x in [1,2], z is in [0,1/3]
// and the tail after z^(2N-1)/(2N-1) is at most
// 2 z^(2N+1) / ((2N+1)(1-z^2)).
fn reduced_log_bounds(value: &RBig) -> Bounds {
	assert!(value >= &integer(1) && value <= &integer(2));
	let z = (value - integer(1)) / (value + integer(1));
	let square = &z * &z;
	let mut power = z;
	let mut sum = integer(0);
	for index in 0..192 {
		sum += &power / integer(2 * index + 1);
		power *= &square;
	}
	let tail = integer(2) * power / (integer(385) * (integer(1) - square));
	let sum = integer(2) * sum;
	(sum.clone(), sum + tail)
}
fn log_bounds(value: &RBig) -> Bounds {
	assert!(value > &integer(0));
	let mut reduced = value.clone();
	let mut exponent = 0;
	while reduced < integer(1) {
		reduced *= integer(2);
		exponent -= 1;
	}
	while reduced >= integer(2) {
		reduced /= integer(2);
		exponent += 1;
	}
	let (lower, upper) = reduced_log_bounds(&reduced);
	let (log_two_lower, log_two_upper) = reduced_log_bounds(&integer(2));
	let scale = integer(exponent);
	if exponent >= 0 {
		(
			lower + &scale * log_two_lower,
			upper + scale * log_two_upper,
		)
	} else {
		(
			lower + &scale * log_two_upper,
			upper + scale * log_two_lower,
		)
	}
}

// Alternating arctangent series: the two successive partial sums bracket atan(x).
fn atan_bounds(value: RBig) -> Bounds {
	assert!(value > integer(0) && value < integer(1));
	let square = &value * &value;
	let mut power = value;
	let mut sum = integer(0);
	for index in 0..192 {
		let term = &power / integer(2 * index + 1);
		if index % 2 == 0 {
			sum += term;
		} else {
			sum -= term;
		}
		power *= &square;
	}
	let next = &sum + power / integer(385);
	(sum, next)
}
fn pi_bounds() -> Bounds {
	let (a_lower, a_upper) = atan_bounds(ratio(1, 5));
	let (b_lower, b_upper) = atan_bounds(ratio(1, 239));
	(
		integer(16) * a_lower - integer(4) * b_upper,
		integer(16) * a_upper - integer(4) * b_lower,
	)
}

#[test]
fn exact_rational_transcendental_and_basic_arithmetic_bounds() {
	let mut backend = MpIntervalBackend::new(Precision {
		bits: 128,
		..Precision::default()
	})
	.unwrap();
	for value in [0.125, 0.5, 1.0, 1.5, 10.0, 100.0] {
		let input = backend.point(value).unwrap();
		let exact = represented(input.lower());
		encloses(&backend.exp(input.clone()).unwrap(), exp_bounds(&exact));
		encloses(&backend.ln(input.clone()).unwrap(), log_bounds(&exact));
		encloses(
			&backend.sin(input.clone()).unwrap(),
			trig_bounds(&exact, true),
		);
		encloses(
			&backend.cos(input.clone()).unwrap(),
			trig_bounds(&exact, false),
		);
		let root = backend.sqrt(input.clone()).unwrap();
		let lower = represented(root.lower());
		let upper = represented(root.upper());
		assert!(lower >= integer(0));
		assert!(&lower * &lower <= exact && &upper * &upper >= exact);
		let three = backend.point(3.0).unwrap();
		let expected = exact / integer(3);
		encloses(
			&backend.div(input, three).unwrap(),
			(expected.clone(), expected),
		);
	}
	for value in [-10.0, -0.5, 0.0] {
		let input = backend.point(value).unwrap();
		let exact = represented(input.lower());
		encloses(&backend.exp(input.clone()).unwrap(), exp_bounds(&exact));
		encloses(
			&backend.sin(input.clone()).unwrap(),
			trig_bounds(&exact, true),
		);
		encloses(&backend.cos(input).unwrap(), trig_bounds(&exact, false));
	}
}

#[test]
fn exact_rational_decimal_pi_and_derivative_bounds() {
	let mut backend = MpIntervalBackend::new(Precision::default()).unwrap();
	let big = UBig::from(10u8).pow(1000);
	for (text, exact) in [
		("0.1", ratio(1, 10)),
		(
			"-1.2345678901234567890123456789",
			RBig::from_parts(
				"-12345678901234567890123456789".parse().unwrap(),
				UBig::from(10u8).pow(28),
			),
		),
		("1e-1000", RBig::from_parts(IBig::ONE, big.clone())),
		("1e1000", RBig::from(big)),
	] {
		let value = backend
			.constant(&ExactConstant::Decimal(text.into()))
			.unwrap();
		encloses(&value, (exact.clone(), exact));
		let cancellation = backend.sub(value.clone(), value).unwrap();
		assert!(backend.contains_zero(&cancellation).unwrap());
	}
	let pi = backend.pi().unwrap();
	encloses(&pi, pi_bounds());
	let half = backend.point(0.5).unwrap();
	let half_pi = backend.mul(pi, half.clone()).unwrap();
	encloses(&backend.sin(half_pi).unwrap(), (integer(1), integer(1)));
	let mut jet = JetBackend(&mut backend);
	let seed = jet.variable(half).unwrap();
	let output = jet.sin(seed).unwrap();
	let sine = trig_bounds(&ratio(1, 2), true);
	encloses(&output.value, sine.clone());
	encloses(&output.first, trig_bounds(&ratio(1, 2), false));
	encloses(&output.second, (-sine.1, -sine.0));
}

// Exact decimal parsing for frozen external fixtures, deliberately separate
// from the production floating-point importer.
fn fixture_decimal(text: &str) -> RBig {
	let mut fields = text.split(['e', 'E']);
	let mantissa = fields.next().unwrap();
	let exponent: isize = fields.next().map_or(0, |value| value.parse().unwrap());
	assert!(fields.next().is_none());
	let fractional = mantissa.split('.').nth(1).map_or(0, str::len);
	let significand: IBig = mantissa.replace('.', "").parse().unwrap();
	let scale = exponent - isize::try_from(fractional).unwrap();
	if scale >= 0 {
		RBig::from(significand * IBig::from(10u8).pow(scale.unsigned_abs()))
	} else {
		RBig::from_parts(significand, UBig::from(10u8).pow(scale.unsigned_abs()))
	}
}

#[test]
fn directed_transcendentals_enclose_frozen_libmpdec_reference_bounds() {
	let fixtures: serde_json::Value =
		serde_json::from_str(include_str!("data/decimal-reference.json")).unwrap();
	let cases = fixtures["cases"].as_array().unwrap();
	assert_ne!(cases.len(), 0);
	for bits in [128, 256] {
		let mut backend = MpIntervalBackend::new(Precision {
			bits,
			..Precision::default()
		})
		.unwrap();
		for case in cases {
			let input = backend
				.constant(&ExactConstant::Decimal(
					case["input"].as_str().unwrap().into(),
				))
				.unwrap();
			let result = match case["operation"].as_str().unwrap() {
				"exp" => backend.exp(input),
				"ln" => backend.ln(input),
				"sqrt" => backend.sqrt(input),
				_ => panic!("unknown reference operation"),
			}
			.unwrap();
			encloses(
				&result,
				(
					fixture_decimal(case["lower"].as_str().unwrap()),
					fixture_decimal(case["upper"].as_str().unwrap()),
				),
			);
		}
	}
}

#[test]
fn exact_dyadic_binary64_exports_obey_independent_rational_distance() {
	let mut state = 0xe128_5bf9_c631_2da7u64;
	for index in 0..2048u32 {
		state ^= state << 13;
		state ^= state >> 7;
		state ^= state << 17;
		let significand = (IBig::from(state) << 64) + IBig::from(state.rotate_left(23));
		let significand = if index % 2 == 0 {
			significand
		} else {
			-significand
		};
		let exponent = isize::try_from(index % 1900).unwrap() - 1250;
		let value = Binary::from_parts(significand, exponent);
		let exact = represented(&value);
		let lower = to_f64(&value, BinaryRounding::Down).unwrap();
		let upper = to_f64(&value, BinaryRounding::Up).unwrap();
		// Explicit MAX-boundary fixtures cover the two infinity cases separately.
		assert!(lower.is_finite() && upper.is_finite());
		let lower_exact = RBig::try_from(lower).unwrap();
		let upper_exact = RBig::try_from(upper).unwrap();
		assert!(lower_exact <= exact && exact <= upper_exact);
		if lower_exact != upper_exact {
			assert_eq!(lower.next_up(), upper);
		}
		let lower_distance = &exact - lower_exact;
		let upper_distance = upper_exact - exact;
		let expected = match lower_distance.cmp(&upper_distance) {
			std::cmp::Ordering::Less => lower,
			std::cmp::Ordering::Greater => upper,
			std::cmp::Ordering::Equal => {
				if lower.to_bits() & 1 == 0 {
					lower
				} else {
					upper
				}
			}
		};
		assert_eq!(
			to_f64(&value, BinaryRounding::Nearest).unwrap().to_bits(),
			expected.to_bits()
		);
	}
}
