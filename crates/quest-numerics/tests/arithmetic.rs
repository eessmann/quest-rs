#![allow(
	clippy::many_single_char_names,
	reason = "Conventional symbols in bounded arithmetic fixtures"
)]
#![allow(
	clippy::unwrap_used,
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	clippy::float_cmp,
	clippy::panic,
	clippy::panic_in_result_fn,
	reason = "Bounded analytic fixtures intentionally use exact assertions and fail immediately"
)]
use quest_numerics::arithmetic::*;
#[test]
fn exact_constants_and_second_order_ad() {
	let mut b = F64Backend;
	let mut ad = JetBackend(&mut b);
	let x = ad.variable(2.0).unwrap();
	let y = ad.mul(x.clone(), x).unwrap();
	assert_eq!((y.value, y.first, y.second), (4.0, 4.0, 2.0));
	let z = ad.variable(0.0).unwrap();
	assert!(ad.sqrt(z).is_err());
}
#[test]
fn mp_interval_retains_decimal_and_binary64_distinction() {
	let mut b = MpIntervalBackend::new(Precision::default()).unwrap();
	let decimal = b.constant(&ExactConstant::Decimal("0.1".into())).unwrap();
	let binary = b.point(0.1).unwrap();
	assert!(b.intersection(&decimal, &binary).unwrap().is_none());
	let one = b.point(1.0).unwrap();
	let three = b.point(3.0).unwrap();
	let third = b.div(one, three).unwrap();
	let three = b.point(3.0).unwrap();
	let result = b.mul(third, three).unwrap();
	let one = b.point(1.0).unwrap();
	assert!(b.intersection(&result, &one).unwrap().is_some());
}
#[test]
fn endpoint_extrema_and_domains() {
	let mut b = MpIntervalBackend::new(Precision::default()).unwrap();
	let a = b.point(1.0).unwrap();
	let c = b.point(2.0).unwrap();
	let x = b.hull(&a, &c).unwrap();
	let s = b.sin(x).unwrap();
	let one = b.point(1.0).unwrap();
	assert!(b.intersection(&s, &one).unwrap().is_some());
	let zero = b.point(0.0).unwrap();
	assert!(b.div(one, zero).is_err());
}
#[test]
fn invalid_mp_operands_cannot_pass_zero_shortcuts() {
	let mut b = MpBackend::new(Precision::default()).unwrap();
	let z = b.point(0.0).unwrap();
	let invalid = Binary::INFINITY;
	assert!(b.mul(z, invalid).is_err());
	let tiny = f64::from_bits(1);
	let x = b.point(tiny).unwrap();
	assert_eq!(b.to_f64(&x).unwrap().to_bits(), 1);
}
#[test]
fn first_and_vector_derivatives_are_structural() {
	let mut b = F64Backend;
	let mut g = GradientBackend::<_, 2>(&mut b);
	let x = g.variable(2.0, 0).unwrap();
	let y = g.variable(3.0, 1).unwrap();
	let z = g.mul(x, y).unwrap();
	assert_eq!(z.value, 6.0);
	assert_eq!(z.gradient.as_slice(), [3.0, 2.0]);
	let mut first = FirstBackend(&mut b);
	let x = first.variable(2.0).unwrap();
	let y = first.ln(x).unwrap();
	assert_eq!(y.first, 0.5);
}
#[test]
fn arbitrary_size_ratio_import_and_exponent_admission() {
	let mut b = MpIntervalBackend::new(Precision::default()).unwrap();
	let c = ExactConstant::Ratio {
		numerator: "1000000000000000000000000000000000000000001".into(),
		denominator: "1000000000000000000000000000000000000000000".into(),
	};
	let x = b.constant(&c).unwrap();
	let one = b.point(1.0).unwrap();
	assert!(b.intersection(&x, &one).unwrap().is_none());
	let mut b = MpBackend::new(Precision {
		max_abs_exponent: 10,
		..Default::default()
	})
	.unwrap();
	assert!(b.point(1e30).is_err());
	let mut b = MpBackend::new(Precision {
		max_operations: 0,
		..Default::default()
	})
	.unwrap();
	assert!(b.point(0.0).is_err());
}
#[test]
fn binary64_exact_import_avoids_double_rounding() {
	let mut b = F64Backend;
	// Midpoint between 1 and nextafter(1,+inf), plus 1e-100.
	let value=ExactConstant::Decimal("1.0000000000000001110223024625156540423631668090820312500000000000000000000000000000000000000000000001".into());
	assert_eq!(b.constant(&value).unwrap().to_bits(), 1.0f64.to_bits() + 1);
}
#[test]
fn underflow_is_not_an_exact_zero_constant() {
	let mut b = MpIntervalBackend::new(Precision::default()).unwrap();
	assert!(
		b.constant(&ExactConstant::Decimal("1e-10000000000".into()))
			.is_err()
	);
}
#[test]
fn subnormal_midpoint_stays_inside() {
	let mut b = Interval64Backend;
	let x = quest_numerics::Interval::point(f64::from_bits(1)).unwrap();
	let m = b.midpoint(&x).unwrap();
	assert!(x.contains(m.lower()));
}
#[test]
fn exponential_underflow_is_rejected_before_zero_can_escape() {
	let mut b = MpBackend::new(Precision::default()).unwrap();
	let x = b.point(-1e100).unwrap();
	assert!(b.exp(x).is_err());
	let mut b = MpIntervalBackend::new(Precision::default()).unwrap();
	let x = b.point(-1e100).unwrap();
	assert!(b.exp(x).is_err());
}
#[test]
fn mp_midpoint_of_higher_precision_endpoints_stays_inside() {
	let mut precise = MpBackend::new(Precision {
		bits: 1024,
		..Default::default()
	})
	.unwrap();
	let x=precise.constant(&ExactConstant::Decimal("1.0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001".into())).unwrap();
	let mut b = MpIntervalBackend::new(Precision {
		bits: 128,
		..Default::default()
	})
	.unwrap();
	let input = b.singleton(&x).unwrap();
	let middle = b.midpoint(&input).unwrap();
	assert!(b.same(&input, &middle).unwrap());
	assert!(b.storage_bytes(&input).unwrap() > b.working_scalar_bytes());
}
#[test]
fn value_and_derivative_domains_differ_at_square_root_zero() {
	let mut b = MpIntervalBackend::new(Precision::default()).unwrap();
	let zero = b.point(0.0).unwrap();
	assert!(b.sqrt(zero.clone()).is_ok());
	let mut first = FirstBackend(&mut b);
	let seed = first.variable(zero.clone()).unwrap();
	assert!(first.sqrt(seed).is_err());
	let mut second = JetBackend(&mut b);
	let seed = second.variable(zero).unwrap();
	assert!(second.sqrt(seed).is_err());
	let negative = b.point(-1.0).unwrap();
	let positive = b.point(1.0).unwrap();
	let crossing = b.hull(&negative, &positive).unwrap();
	assert!(b.ln(crossing.clone()).is_err());
	assert!(b.sqrt(crossing).is_err());
}
const fn audited<B: CertifyingBackend>(_: &B) {}
#[test]
fn operation_budget_is_shared_and_preserves_enclosure_admission() {
	let work = Budget::new(2);
	let mut b = F64Backend;
	{
		let mut metered = BudgetedBackend::new(&mut b, &work);
		let one = metered.point(1.0).unwrap();
		assert_eq!(metered.add(one, one).unwrap(), 2.0);
		assert!(metered.point(0.0).is_err());
	}
	assert_eq!(work.used(), 2);
	let work = Budget::new(1);
	let mut interval = Interval64Backend;
	let mut metered = BudgetedBackend::new(&mut interval, &work);
	audited(&metered);
	let x = metered.point(1.0).unwrap();
	assert!(metered.contains_zero(&x).is_err());
}
#[test]
fn rectangular_jacobian_uses_one_structural_evaluation() {
	let mut b = F64Backend;
	let result = quest_numerics::ad::jacobian::<_, _, 2, 3>(
		&mut b,
		[2.0, 3.0],
		quest_numerics::ad::JacobianLimits::default(),
		|g, x| {
			let product = g.mul(x[0].clone(), x[1].clone())?;
			let sum = g.add(x[0].clone(), x[1].clone())?;
			Ok(vec![product, sum, x[0].clone()])
		},
	)
	.unwrap();
	assert_eq!(result.value, [6.0, 5.0, 2.0]);
	assert_eq!(result.derivative.as_slice(), [3.0, 2.0, 1.0, 1.0, 1.0, 0.0]);
}
#[test]
fn jacobian_limits_precede_callback_and_output_shapes_are_checked() {
	use quest_numerics::ad::{JacobianLimits, jacobian};
	let mut backend = F64Backend;
	let work = Budget::new(0);
	let mut b = BudgetedBackend::new(&mut backend, &work);
	let result = jacobian::<_, _, 128, 1>(
		&mut b,
		[0.0; 128],
		JacobianLimits {
			max_bytes: 32,
			..Default::default()
		},
		|_, _| panic!("callback must not execute"),
	);
	assert!(result.is_err());
	assert_eq!(work.used(), 0);
	let result = jacobian::<_, _, 2, 1>(
		&mut backend,
		[1.0, 2.0],
		JacobianLimits::default(),
		|_, _| Ok(Vec::new()),
	);
	assert!(result.is_err());
}
#[test]
fn gradient_storage_is_heap_owned_independent_of_static_dimension() {
	assert_eq!(
		std::mem::size_of::<Gradient<f64, 2>>(),
		std::mem::size_of::<Gradient<f64, 4096>>()
	);
	let mut b = MpBackend::new(Precision {
		bits: 512,
		..Default::default()
	})
	.unwrap();
	let input = [b.point(1.0).unwrap(), b.point(2.0).unwrap()];
	let result = quest_numerics::ad::jacobian::<_, _, 2, 1>(
		&mut b,
		input,
		quest_numerics::ad::JacobianLimits::default(),
		|g, x| Ok(vec![g.mul(x[0].clone(), x[1].clone())?]),
	)
	.unwrap();
	assert_eq!(b.to_f64(result.derivative.get(0, 0).unwrap()).unwrap(), 2.0);
}

#[test]
fn rational_operands_obey_exponent_policy_before_cancellation() {
	let precision = Precision {
		max_abs_exponent: 10,
		..Precision::default()
	};
	for value in [
		ExactConstant::Rational(0, u64::MAX),
		ExactConstant::Rational(i64::MAX, i64::MAX.unsigned_abs()),
	] {
		let mut point = MpBackend::new(precision).unwrap();
		let mut interval = MpIntervalBackend::new(precision).unwrap();
		assert!(point.constant(&value).is_err());
		assert!(interval.constant(&value).is_err());
	}
}

#[test]
fn identity_ad_seeds_reject_invalid_values_without_rounding() {
	let mut backend = F64Backend;
	assert!(FirstBackend(&mut backend).variable(f64::NAN).is_err());
	assert!(JetBackend(&mut backend).variable(f64::INFINITY).is_err());
	assert!(
		GradientBackend::<_, 1>(&mut backend)
			.variable(f64::NAN, 0)
			.is_err()
	);
	assert!(
		quest_numerics::ad::jacobian::<_, _, 1, 1>(
			&mut backend,
			[f64::NAN],
			quest_numerics::ad::JacobianLimits::default(),
			|_, seeds| Ok(seeds.to_vec())
		)
		.is_err()
	);
	let mut inner = FirstBackend(&mut backend);
	assert!(
		FirstBackend(&mut inner)
			.variable(First {
				value: 1.0,
				first: f64::NAN
			})
			.is_err()
	);
	let mut wide = MpBackend::new(Precision::default()).unwrap();
	let value = wide.point(1024.0).unwrap();
	let mut narrow = MpBackend::new(Precision {
		max_abs_exponent: 2,
		..Precision::default()
	})
	.unwrap();
	assert!(FirstBackend(&mut narrow).variable(value).is_err());
	let invalid = Binary::INFINITY;
	assert!(JetBackend(&mut narrow).variable(invalid).is_err());
}

#[test]
fn validation_preserves_wide_seeds_and_checks_composite_components() {
	let mut wide = MpBackend::new(Precision::default()).unwrap();
	let value = wide
		.constant(&ExactConstant::Decimal(
			"1.0000000000000000000000000000000000000001".into(),
		))
		.unwrap();
	let mut narrow = MpBackend::new(Precision {
		bits: 64,
		..Precision::default()
	})
	.unwrap();
	let seed = FirstBackend(&mut narrow).variable(value.clone()).unwrap();
	assert_eq!(
		seed.value.partial_cmp(&value),
		Some(std::cmp::Ordering::Equal)
	);
	let mut point = F64Backend;
	assert!(
		JetBackend(&mut point)
			.validate(&Jet {
				value: 1.0,
				first: 1.0,
				second: f64::NAN
			})
			.is_err()
	);
	assert!(
		GradientBackend::<_, 1>(&mut point)
			.validate(&Gradient {
				value: 1.0,
				gradient: quest_numerics::shapes::Matrix::from_rows([[f64::NAN]]).unwrap()
			})
			.is_err()
	);
	let budget = Budget::new(0);
	assert!(
		BudgetedBackend::new(&mut point, &budget)
			.validate(&1.0)
			.is_err()
	);
}

#[test]
fn binary64_point_fast_paths_preserve_bits_and_reject_nonfinite() {
	let mut point = F64Backend;
	let mut interval = Interval64Backend;
	for value in [-0.0, 0.0, f64::from_bits(1), f64::MAX, -f64::MAX] {
		assert_eq!(point.point(value).unwrap().to_bits(), value.to_bits());
		assert_eq!(
			point
				.constant(&ExactConstant::Binary64(value))
				.unwrap()
				.to_bits(),
			value.to_bits()
		);
		let enclosure = interval.point(value).unwrap();
		assert_eq!(enclosure.lower(), value);
		assert_eq!(enclosure.upper(), value);
	}
	for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
		assert!(point.point(value).is_err());
		assert!(interval.point(value).is_err());
	}
}

#[test]
fn bulk_work_preserves_nested_limits_and_default_visit_errors() {
	let outer = Budget::new(20);
	let inner = Budget::new(5);
	let mut scalar = F64Backend;
	let mut inner_backend = BudgetedBackend::new(&mut scalar, &inner);
	let mut backend = BudgetedBackend::new(&mut inner_backend, &outer);
	backend.charge(4).unwrap();
	assert_eq!((outer.used(), inner.used()), (4, 4));
	assert!(backend.charge(2).is_err());
	// Bulk work is reserved conservatively before the inner backend admits it.
	assert_eq!((outer.used(), inner.used()), (6, 4));
	assert!(backend.charge(usize::MAX).is_err());
	assert_eq!((outer.used(), inner.used()), (6, 4));
	backend.charge(1).unwrap();
	assert_eq!((outer.used(), inner.used()), (7, 5));

	// The default implementation retains visit's partial-progress semantics.
	let mut mp = MpBackend::new(Precision {
		max_operations: 3,
		..Precision::default()
	})
	.unwrap();
	mp.charge(2).unwrap();
	assert!(mp.charge(2).is_err());
	assert!(mp.visit().is_err());
	mp.charge(0).unwrap();
}

#[test]
fn mp_precision_is_bit_granular_and_preserves_native_endpoints() {
	let mut point = MpBackend::new(Precision {
		bits: 65,
		..Precision::default()
	})
	.unwrap();
	assert_eq!(point.precision_bits(), 65);
	let third = point.constant(&ExactConstant::Rational(1, 3)).unwrap();
	assert_eq!(third.precision(), 65);
	let epsilon = point.epsilon().unwrap();
	assert_eq!(epsilon, Binary::ONE.with_precision(65).value() >> 64);
	let mut interval = MpIntervalBackend::new(Precision {
		bits: 65,
		..Precision::default()
	})
	.unwrap();
	let third = interval.constant(&ExactConstant::Rational(1, 3)).unwrap();
	assert_eq!(
		(third.lower().precision(), third.upper().precision()),
		(65, 65)
	);
	assert!(third.lower() < third.upper());
	assert!(interval.storage_bytes(&third).unwrap() <= interval.working_scalar_bytes());
	for bits in [0, 53, 63, 1_048_577, usize::MAX] {
		assert!(
			MpBackend::new(Precision {
				bits,
				..Precision::default()
			})
			.is_err()
		);
	}
}

#[test]
fn mp_trigonometric_extrema_and_large_phase_widen_conservatively() {
	let mut backend = MpIntervalBackend::new(Precision::default()).unwrap();
	let huge = Binary::ONE.with_precision(256).value() << 4096;
	let input = backend.singleton(&huge).unwrap();
	for result in [
		backend.sin(input.clone()).unwrap(),
		backend.cos(input).unwrap(),
	] {
		assert_eq!(result.lower(), &Binary::NEG_ONE);
		assert_eq!(result.upper(), &Binary::ONE);
	}
	let lower = backend.point(-7.0).unwrap();
	let upper = backend.point(7.0).unwrap();
	let input = backend.hull(&lower, &upper).unwrap();
	for result in [
		backend.sin(input.clone()).unwrap(),
		backend.cos(input).unwrap(),
	] {
		assert_eq!(result.lower(), &Binary::NEG_ONE);
		assert_eq!(result.upper(), &Binary::ONE);
	}
}

#[test]
fn native_binary64_interchange_preserves_ties_subnormals_and_overflow() {
	for precision in [53, 65, 128] {
		for value in [
			-0.0,
			0.0,
			f64::from_bits(1),
			-f64::from_bits(1),
			f64::MAX,
			-f64::MAX,
		] {
			let imported = exact_from_f64(value, precision).unwrap();
			assert_eq!(imported.precision(), usize::try_from(precision).unwrap());
			for round in [
				BinaryRounding::Down,
				BinaryRounding::Nearest,
				BinaryRounding::Up,
			] {
				assert_eq!(to_f64(&imported, round).unwrap().to_bits(), value.to_bits());
			}
		}
	}
	for precision in [0, 52, 1_048_577, u32::MAX] {
		assert!(exact_from_f64(1.0, precision).is_err());
	}
	let half_tiny = Binary::ONE.with_precision(128).value() >> 1075;
	assert_eq!(
		to_f64(&half_tiny, BinaryRounding::Down).unwrap().to_bits(),
		0
	);
	assert_eq!(
		to_f64(&half_tiny, BinaryRounding::Nearest)
			.unwrap()
			.to_bits(),
		0
	);
	assert_eq!(to_f64(&half_tiny, BinaryRounding::Up).unwrap().to_bits(), 1);
	let negative_half = -half_tiny;
	assert_eq!(
		to_f64(&negative_half, BinaryRounding::Down)
			.unwrap()
			.to_bits(),
		(-f64::from_bits(1)).to_bits()
	);
	assert_eq!(
		to_f64(&negative_half, BinaryRounding::Nearest)
			.unwrap()
			.to_bits(),
		(-0.0f64).to_bits()
	);
	assert_eq!(
		to_f64(&negative_half, BinaryRounding::Up)
			.unwrap()
			.to_bits(),
		(-0.0f64).to_bits()
	);
	let three_halves = Binary::from(3).with_precision(128).value() >> 1075;
	assert_eq!(
		to_f64(&three_halves, BinaryRounding::Nearest)
			.unwrap()
			.to_bits(),
		2
	);
	let half_ulp = Binary::ONE.with_precision(128).value() >> 53;
	let midpoint = Binary::ONE.with_precision(128).value() + half_ulp;
	assert_eq!(to_f64(&midpoint, BinaryRounding::Nearest).unwrap(), 1.0);
	assert_eq!(
		to_f64(&midpoint, BinaryRounding::Up).unwrap(),
		1.0f64.next_up()
	);
	let overflow_midpoint =
		exact_from_f64(f64::MAX, 128).unwrap() + (Binary::ONE.with_precision(128).value() << 970);
	assert_eq!(
		to_f64(&overflow_midpoint, BinaryRounding::Down).unwrap(),
		f64::MAX
	);
	assert_eq!(
		to_f64(&overflow_midpoint, BinaryRounding::Nearest).unwrap(),
		f64::INFINITY
	);
	assert_eq!(
		to_f64(&overflow_midpoint, BinaryRounding::Up).unwrap(),
		f64::INFINITY
	);
}

#[test]
fn native_mp_backends_keep_precision_local_to_parallel_workers() {
	let workers: Vec<_> = [65, 127, 256, 257]
		.into_iter()
		.map(|bits| {
			std::thread::spawn(move || {
				let mut backend = MpIntervalBackend::new(Precision {
					bits,
					..Precision::default()
				})
				.unwrap();
				for _ in 0..8 {
					let pi = backend.pi().unwrap();
					assert_eq!(pi.lower().precision(), bits);
					assert_eq!(pi.upper().precision(), bits);
					let half = backend.point(0.5).unwrap();
					let half_pi = backend.mul(pi, half).unwrap();
					assert_eq!(backend.sin(half_pi).unwrap().upper(), &Binary::ONE);
				}
			})
		})
		.collect();
	for worker in workers {
		worker.join().unwrap();
	}
}

#[test]
fn scalar_storage_tracks_significand_instead_of_requested_precision() {
	let mut wide = MpBackend::new(Precision {
		bits: 1024,
		..Precision::default()
	})
	.unwrap();
	let mut narrow = MpBackend::new(Precision {
		bits: 64,
		..Precision::default()
	})
	.unwrap();
	let one_wide = wide.point(1.0).unwrap();
	let one_narrow = narrow.point(1.0).unwrap();
	assert_eq!(
		wide.storage_bytes(&one_wide).unwrap(),
		narrow.storage_bytes(&one_narrow).unwrap()
	);
	let dense = wide.constant(&ExactConstant::Rational(1, 3)).unwrap();
	assert!(wide.storage_bytes(&dense).unwrap() > wide.storage_bytes(&one_wide).unwrap());
}

#[test]
fn exact_binary64_export_rounds_values_between_maximum_and_overflow_midpoint() {
	let maximum = exact_from_f64(f64::MAX, 128).unwrap();
	let quarter_ulp = Binary::from_parts(dashu_int::IBig::ONE, 969);
	let below_midpoint = &maximum + &quarter_ulp;
	assert_eq!(
		to_f64(&below_midpoint, BinaryRounding::Nearest).unwrap(),
		f64::MAX
	);
	assert_eq!(
		to_f64(&below_midpoint, BinaryRounding::Down).unwrap(),
		f64::MAX
	);
	assert_eq!(
		to_f64(&below_midpoint, BinaryRounding::Up).unwrap(),
		f64::INFINITY
	);
	let negative = -below_midpoint;
	assert_eq!(
		to_f64(&negative, BinaryRounding::Nearest).unwrap(),
		-f64::MAX
	);
	assert_eq!(
		to_f64(&negative, BinaryRounding::Down).unwrap(),
		f64::NEG_INFINITY
	);
	assert_eq!(to_f64(&negative, BinaryRounding::Up).unwrap(), -f64::MAX);
}

#[test]
fn native_guard_digit_survives_backend_and_endpoint_transfer() {
	use dashu_base::BitTest;
	let mut backend = MpBackend::new(Precision {
		bits: 128,
		..Precision::default()
	})
	.unwrap();
	let dense = Binary::from_parts((dashu_int::IBig::ONE << 128) - dashu_int::IBig::ONE, -128);
	let tiny = Binary::from_parts(dashu_int::IBig::ONE, -129);
	let result = backend.sub(dense, tiny).unwrap();
	let exact = Binary::from_parts(
		(dashu_int::IBig::ONE << 129) - dashu_int::IBig::from(3),
		-129,
	);
	assert_eq!(result, exact);
	assert_eq!(result.repr().significand().bit_len(), 129);
	let mut enclosure = MpIntervalBackend::new(Precision {
		bits: 64,
		..Precision::default()
	})
	.unwrap();
	let imported = enclosure.singleton(&result).unwrap();
	assert_eq!(enclosure.lower_endpoint(&imported).unwrap(), exact);
	assert_eq!(enclosure.upper_endpoint(&imported).unwrap(), exact);
	assert!(enclosure.storage_bytes(&imported).unwrap() > enclosure.working_scalar_bytes());
}

#[test]
fn exact_binary64_import_export_round_trip_spans_finite_bit_patterns() {
	let mut state = 0x31f2_65dc_9720_a68bu64;
	for _ in 0..16384 {
		state ^= state << 13;
		state ^= state >> 7;
		state ^= state << 17;
		let value = f64::from_bits(state);
		if value.is_finite() {
			let exact = exact_from_f64(value, 65).unwrap();
			for direction in [
				BinaryRounding::Down,
				BinaryRounding::Nearest,
				BinaryRounding::Up,
			] {
				assert_eq!(
					to_f64(&exact, direction).unwrap().to_bits(),
					value.to_bits()
				);
			}
		}
	}
}

#[test]
fn exponent_admission_uses_value_magnitude_and_not_raw_native_scale() {
	let backend = MpBackend::new(Precision {
		max_abs_exponent: 2,
		..Precision::default()
	})
	.unwrap();
	// This wide significand stores a value close to one despite scale -256.
	let near_one = Binary::from_parts((dashu_int::IBig::ONE << 256) + dashu_int::IBig::ONE, -256);
	assert!(backend.validate(&near_one).is_ok());
	// The raw scale is zero, but the significand places the value near 2^63.
	let huge = Binary::from_parts((dashu_int::IBig::ONE << 64) - dashu_int::IBig::ONE, 0);
	assert!(backend.validate(&huge).is_err());
}
