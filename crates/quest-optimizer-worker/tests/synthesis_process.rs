#![cfg(all(feature = "synthesis", target_os = "linux"))]

use googletest::prelude::*;
use quest_math::{
	AngleTarget, Axis, Control, Gate, Limits, Target, certify_rotation, lift_controlled_rotation,
};
use quest_optimizer_client::{Client, WorkerLimits};

const EPSILON: f64 = 1.0e-12;

fn worker() -> Result<Client> {
	Ok(Client::new(
		env!("CARGO_BIN_EXE_quest-optimizer-worker"),
		WorkerLimits::default(),
	)?)
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

const fn dyadic(axis: Axis, angle: f64) -> Target {
	Target {
		axis,
		angle: AngleTarget::DyadicRadians {
			bits: angle.to_bits(),
		},
	}
}

#[gtest]
fn fixed_exact_phase_sequence_survives_the_bounded_process() -> Result<()> {
	let target = rational_pi(Axis::Z, 1, 4);
	let certificate = worker()?.synthesize(&target, EPSILON, 1_234, Limits::default())?;
	verify_that!(certificate.target(), eq(&target))?;
	let mut wrong_phase = certificate.candidate().clone();
	wrong_phase.operations.push(quest_math::Operation {
		gate: Gate::W,
		targets: vec![],
		controls: vec![],
	});
	verify_that!(
		certify_rotation(&wrong_phase, &target, EPSILON.to_bits(), Limits::default()),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn seeded_targets_are_reproducible_and_independently_certified() -> Result<()> {
	let cases = [
		(rational_pi(Axis::Z, 1, 7), 0x5eed),
		(rational_pi(Axis::X, -2, 11), 17),
		(dyadic(Axis::Z, 0.17), 23),
		(dyadic(Axis::Z, -0.42), 29),
		(dyadic(Axis::Y, 0.23), 31),
	];
	let client = worker()?;
	for (target, seed) in cases {
		let first = client.synthesize(&target, EPSILON, seed, Limits::default())?;
		let second = client.synthesize(&target, EPSILON, seed, Limits::default())?;
		verify_that!(first.candidate(), eq(second.candidate()))?;
		verify_that!(first.target(), eq(&target))?;
		verify_that!(first.epsilon_bits(), eq(EPSILON.to_bits()))?;
	}
	Ok(())
}

#[gtest]
fn nontrivial_process_certificate_lifts_to_signed_controls_without_losing_its_bound() -> Result<()>
{
	let target = rational_pi(Axis::Z, 1, 7);
	let base = worker()?.synthesize(&target, EPSILON, 0x5eed, Limits::default())?;
	let controls = [
		Control {
			qubit: 1,
			positive: false,
		},
		Control {
			qubit: 0,
			positive: true,
		},
	];
	let lifted = lift_controlled_rotation(&base, 3, 2, &controls, Limits::default())?;
	verify_that!(lifted.bound_squared(), eq(base.bound_squared()))?;
	verify_that!(lifted.controls(), eq(controls.as_slice()))?;
	verify_that!(
		lifted.sequence().operations.iter().all(|operation| {
			operation.controls == controls
				&& (operation.targets.is_empty() || operation.targets.as_slice() == [2])
		}),
		eq(true)
	)?;
	Ok(())
}
