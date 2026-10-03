//! Rotation synthesis includes its acceptance certificate; replay is measured separately.
#![allow(clippy::arithmetic_side_effects)] // Admitted benchmark digit range.
use quest_math::{AngleTarget, Axis, Gate, Target, certify_rotation};
use quest_synthesis::{SynthesisOptions, approximate_rotation};
use std::{hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
	let digits: i32 = std::env::args()
		.nth(1)
		.unwrap_or_else(|| "6".into())
		.parse()?;
	if !(1..=12).contains(&digits) {
		return Err("digits must be 1 through 12".into());
	}
	let epsilon = 10.0_f64.powi(-digits);
	let target = Target {
		axis: Axis::Z,
		angle: AngleTarget::RationalPi {
			numerator: 1.into(),
			denominator: 7.into(),
		},
	};
	let options = SynthesisOptions::default();
	let started = Instant::now();
	let result = approximate_rotation(&target, epsilon.to_bits(), options.clone())?;
	let synthesis_and_acceptance_ns = started.elapsed().as_nanos();
	let started = Instant::now();
	let certificate = certify_rotation(
		result.sequence(),
		&target,
		epsilon.to_bits(),
		options.limits,
	)?;
	let independent_replay_ns = started.elapsed().as_nanos();
	black_box(certificate);
	let t_count = result
		.sequence()
		.operations
		.iter()
		.filter(|op| matches!(op.gate, Gate::T | Gate::Tdg))
		.count();
	let matrix_output_reservation = quest_math::admit_synthesis_storage(
		1,
		0,
		result.sequence().operations.len(),
		options.limits,
	)?;
	let mut output = csv::WriterBuilder::new()
		.terminator(csv::Terminator::Any(b'\n'))
		.from_writer(std::io::stdout().lock());
	output.write_record([
		"digits",
		"epsilon_bits",
		"gates",
		"t_count",
		"synthesis_and_acceptance_ns",
		"independent_replay_ns",
		"logical_work",
		"working_precision_bits",
		"grid_exponent",
		"modeled_matrix_output_reservation_bytes",
		"request_max_bytes",
		"seed",
	])?;
	output.serialize((
		digits,
		epsilon.to_bits(),
		result.sequence().operations.len(),
		t_count,
		synthesis_and_acceptance_ns,
		independent_replay_ns,
		result.work(),
		result.working_precision_bits(),
		result.grid_exponent(),
		matrix_output_reservation,
		options.limits.bytes,
		result.seed(),
	))?;
	output.flush()?;
	Ok(())
}
