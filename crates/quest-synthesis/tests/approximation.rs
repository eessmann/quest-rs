use quest_math::*;
use quest_synthesis::*;
#[test]
fn quarter_pi_full_phase_candidate_at_twelve_digits() {
	let target = Target {
		axis: Axis::Z,
		angle: AngleTarget::RationalPi {
			numerator: 1.into(),
			denominator: 4.into(),
		},
	};
	let result = approximate_rotation(
		&target,
		1e-12_f64.to_bits(),
		SynthesisOptions {
			seed: 1234,
			..SynthesisOptions::default()
		},
	)
	.unwrap();
	let repeated = approximate_rotation(
		&target,
		1e-12_f64.to_bits(),
		SynthesisOptions {
			seed: 1234,
			..SynthesisOptions::default()
		},
	)
	.unwrap();
	assert_eq!(result.sequence(), repeated.sequence());
	assert_eq!(result.work(), repeated.work());
	certify_rotation(
		result.sequence(),
		&target,
		1e-12_f64.to_bits(),
		Limits::default(),
	)
	.unwrap();
	let mut wrong_phase = result.sequence().clone();
	wrong_phase.operations.push(Operation {
		gate: Gate::W,
		targets: vec![],
		controls: vec![],
	});
	assert!(
		certify_rotation(
			&wrong_phase,
			&target,
			1e-12_f64.to_bits(),
			Limits::default()
		)
		.is_err()
	);
	assert!(matches!(
		approximate_rotation(
			&target,
			1e-12_f64.to_bits(),
			SynthesisOptions {
				seed: 1234,
				max_work: result.work().saturating_sub(1),
				..SynthesisOptions::default()
			},
		),
		Err(SynthesisError::WorkExhausted { .. })
	));
}
fn options() -> SynthesisOptions {
	SynthesisOptions {
		limits: Limits {
			gates: 20_000,
			bytes: 300_000_000,
			..Limits::default()
		},
		max_work: 5_000_000,
		..SynthesisOptions::default()
	}
}
#[test]
fn affine_and_dyadic_candidates_are_independently_certified() {
	for axis in [Axis::X, Axis::Y, Axis::Z] {
		let target = Target {
			axis,
			angle: AngleTarget::AffinePi {
				radians_numerator: 1.into(),
				radians_denominator: 3.into(),
				pi_numerator: 1.into(),
				pi_denominator: 2.into(),
			},
		};
		let candidate = approximate_rotation(&target, 0.25_f64.to_bits(), options()).unwrap();
		certify_rotation(
			candidate.sequence(),
			&target,
			0.25_f64.to_bits(),
			options().limits,
		)
		.unwrap();
		let empty: [quest_math::Operation; 0] = [];
		assert_ne!(candidate.sequence().operations, empty);
	}
	let target = Target {
		axis: Axis::Z,
		angle: AngleTarget::DyadicRadians {
			bits: 1.1_f64.to_bits(),
		},
	};
	let first = approximate_rotation(&target, 0.25_f64.to_bits(), options()).unwrap();
	let second = approximate_rotation(&target, 0.25_f64.to_bits(), options()).unwrap();
	assert_eq!(first.sequence(), second.sequence());
}
#[test]
fn pi_over_four_is_not_exactly_t_and_work_is_request_owned() {
	let target = Target {
		axis: Axis::Z,
		angle: AngleTarget::RationalPi {
			numerator: 1.into(),
			denominator: 4.into(),
		},
	};
	let candidate = approximate_rotation(&target, 0.2_f64.to_bits(), options()).unwrap();
	certify_rotation(
		candidate.sequence(),
		&target,
		0.2_f64.to_bits(),
		options().limits,
	)
	.unwrap();
	let t = Sequence {
		qubits: 1,
		operations: vec![Operation {
			gate: Gate::T,
			targets: vec![0],
			controls: vec![],
		}],
	};
	assert!(certify_rotation(&t, &target, 0.2_f64.to_bits(), options().limits).is_err());
	assert!(matches!(
		approximate_rotation(
			&target,
			0.2_f64.to_bits(),
			SynthesisOptions {
				max_work: 0,
				..options()
			}
		),
		Err(SynthesisError::WorkExhausted { .. })
	));
}

#[test]
fn twelve_digit_rational_pi_and_affine_rotations_are_certified() {
	for angle in [
		AngleTarget::RationalPi {
			numerator: 1.into(),
			denominator: 7.into(),
		},
		AngleTarget::AffinePi {
			radians_numerator: 1.into(),
			radians_denominator: 3.into(),
			pi_numerator: 1.into(),
			pi_denominator: 5.into(),
		},
	] {
		let target = Target {
			axis: Axis::Z,
			angle,
		};
		let options = SynthesisOptions::default();
		let candidate =
			approximate_rotation(&target, 1.0e-12_f64.to_bits(), options.clone()).unwrap();
		certify_rotation(
			candidate.sequence(),
			&target,
			1.0e-12_f64.to_bits(),
			options.limits,
		)
		.unwrap();
	}
}

#[test]
fn twelve_digit_basis_changes_and_signed_control_lift_preserve_the_certificate() {
	let cases = [
		(
			Target {
				axis: Axis::X,
				angle: AngleTarget::RationalPi {
					numerator: (-2).into(),
					denominator: 11.into(),
				},
			},
			17,
		),
		(
			Target {
				axis: Axis::Y,
				angle: AngleTarget::DyadicRadians {
					bits: 0.23_f64.to_bits(),
				},
			},
			31,
		),
	];
	for (target, seed) in cases {
		let result = approximate_rotation(
			&target,
			1.0e-12_f64.to_bits(),
			SynthesisOptions {
				seed,
				..SynthesisOptions::default()
			},
		)
		.unwrap();
		let lifted = lift_controlled_rotation(
			result.certificate(),
			3,
			2,
			&[
				Control {
					qubit: 1,
					positive: false,
				},
				Control {
					qubit: 0,
					positive: true,
				},
			],
			Limits::default(),
		)
		.unwrap();
		assert_eq!(lifted.bound_squared(), result.certificate().bound_squared());
	}
}
