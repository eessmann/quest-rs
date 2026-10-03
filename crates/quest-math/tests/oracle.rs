use dashu_int::IBig;
use googletest::{Result, prelude::*};
use quest_math::{
	AngleTarget, Axis, Gate, Limits, Operation, RBig, Sequence, Target, certify_rotation,
};

fn invalid() -> std::io::Error {
	std::io::Error::other("invalid committed oracle fixture")
}

fn next<'a>(fields: &mut impl Iterator<Item = &'a str>) -> Result<&'a str> {
	Ok(fields.next().ok_or_else(invalid)?)
}

fn sequence(gates: &str) -> Result<Sequence> {
	let operations = gates
		.chars()
		.map(|symbol| {
			let gate = match symbol {
				'H' => Gate::H,
				'X' => Gate::X,
				'Y' => Gate::Y,
				'Z' => Gate::Z,
				'S' => Gate::S,
				's' => Gate::Sdg,
				'T' => Gate::T,
				't' => Gate::Tdg,
				'W' => Gate::W,
				_ => return Err(invalid()),
			};
			Ok(Operation {
				gate,
				targets: if gate == Gate::W { vec![] } else { vec![0] },
				controls: vec![],
			})
		})
		.collect::<std::result::Result<Vec<_>, _>>()?;
	Ok(Sequence {
		qubits: 1,
		operations,
	})
}

fn target(axis: &str, angle: &str) -> Result<Target> {
	let axis = match axis {
		"X" => Axis::X,
		"Y" => Axis::Y,
		"Z" => Axis::Z,
		_ => return Err(invalid().into()),
	};
	let angle = if let Some(bits) = angle.strip_prefix('D') {
		AngleTarget::DyadicRadians {
			bits: bits.parse()?,
		}
	} else {
		let (numerator, denominator) = angle
			.strip_prefix('P')
			.and_then(|value| value.split_once('/'))
			.ok_or_else(invalid)?;
		AngleTarget::RationalPi {
			numerator: numerator.parse()?,
			denominator: denominator.parse()?,
		}
	};
	Ok(Target { axis, angle })
}

#[gtest]
fn independent_decimal_full_matrix_oracles_bracket_every_axis_and_angle_kind() -> Result<()> {
	let denominator = IBig::from(10u8).pow(50);
	let slack = RBig::from_parts_signed(IBig::from(1u8), IBig::from(10u8).pow(12));
	for line in include_str!("fixtures/rotation_oracle.txt")
		.lines()
		.filter(|line| !line.starts_with('#'))
	{
		let mut fields = line.split_whitespace();
		let candidate = sequence(next(&mut fields)?)?;
		let target = target(next(&mut fields)?, next(&mut fields)?)?;
		let pass = next(&mut fields)?.parse()?;
		let fail = next(&mut fields)?.parse()?;
		let lower = RBig::from_parts_signed(next(&mut fields)?.parse()?, denominator.clone());
		let upper = RBig::from_parts_signed(next(&mut fields)?.parse()?, denominator.clone());
		expect_true!(fields.next().is_none());
		let certificate = certify_rotation(&candidate, &target, pass, Limits::default())?;
		expect_true!(certificate.bound_squared() >= &lower);
		let upper_with_slack = std::ops::Add::add(upper, &slack);
		expect_true!(certificate.bound_squared() <= &upper_with_slack);
		expect_true!(certify_rotation(&candidate, &target, fail, Limits::default()).is_err());
	}
	Ok(())
}

#[gtest]
fn pinned_nonclifford_pi_over_seven_sequence_is_certified_at_one_e_minus_twelve() -> Result<()> {
	// rsgridsynth candidate, seed 0x5eed, 256 bits. Generation is not proof;
	// the project-owned verifier recomputes full phase and the rigorous bound.
	let candidate = sequence(
		"WWWWWSHTHSTHTHTHTHTHSTHTHSTHTHTHTHTHTHTHSTHSTHTHTHSTHTHSTHTHTHTHTHTHSTHSTHSTHTHTHTHTHTHTHTHTHTHSTHTHTHTHSTHTHTHSTHSTHSTHTHTHSTHTHSTHSTHSTHSTHSTHSTHSTHTHTHTHSTHSTHTHTHTHTHTHTHSTHTHTHSTHSTHSTHTHTHSTHSTHSTHTHSTHSTHTHSTHTHTHSTHTHTHTHSTHSTHSTHTHSTHSTHSTHSTHTHTHTHSTHSTHSTHTHSTHTHSTHTHSTHTHSTHSTHTHSTHTHTHTHTHSTHTHTHSTHTHTHTHTHS",
	)?;
	let target = target("Z", "P1/7")?;
	let limits = Limits::default();
	let certificate = certify_rotation(&candidate, &target, 1e-12_f64.to_bits(), limits)?;
	expect_eq!(certificate.precision_bits(), 64);
	let missing_phase = Sequence {
		qubits: 1,
		operations: candidate
			.operations
			.into_iter()
			.filter(|operation| operation.gate != Gate::W)
			.collect(),
	};
	expect_true!(certify_rotation(&missing_phase, &target, 1e-12_f64.to_bits(), limits).is_err());
	Ok(())
}
