use googletest::{Result, prelude::*};
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::{
	AffinePhaseOperation as A, Cnot, ParityOptions, RBig, fold_parity, fold_parity_candidate,
};

#[gtest]
fn complemented_rz_keeps_the_exact_scalar_phase() -> Result<()> {
	let one = RBig::from(1);
	let source = [
		A::X { target: 0 },
		A::Rz {
			target: 0,
			coefficient: one.clone(),
		},
		A::X { target: 0 },
		A::Rz {
			target: 0,
			coefficient: one,
		},
	];
	expect_true!(
		fold_parity(1, &source, ParityOptions::default())?
			.operations()
			.is_empty()
	);
	let source = [A::Rz {
		target: 0,
		coefficient: RBig::from(2),
	}; 1];
	let result = fold_parity(1, &source, ParityOptions::default())?;
	expect_eq!(result.operations(), &source);
	Ok(())
}

#[gtest]
fn repeated_parity_phase_and_affine_x_network_fold_exactly() -> Result<()> {
	let phase = A::Phase {
		target: 0,
		coefficient: RBig::from_parts_signed(1.into(), 4.into()),
	};
	let cx = A::Cnot(Cnot {
		control: 1,
		target: 0,
	});
	let source = [cx.clone(), phase.clone(), cx.clone(), cx.clone(), phase, cx];
	let result = fold_parity(2, &source, ParityOptions::default())?;
	expect_eq!(result.operations().len(), 3);
	expect_true!(
		fold_parity(
			2,
			&source,
			ParityOptions {
				max_coefficient_bits: 0,
				..ParityOptions::default()
			}
		)
		.is_err()
	);
	Ok(())
}

#[gtest]
fn shared_pivot_reuses_symmetric_difference_and_restores_the_wire() -> Result<()> {
	let cx10 = A::Cnot(Cnot {
		control: 1,
		target: 0,
	});
	let cx20 = A::Cnot(Cnot {
		control: 2,
		target: 0,
	});
	let phase = A::Phase {
		target: 0,
		coefficient: RBig::from_parts_signed(1.into(), 4.into()),
	};
	let source = [
		cx10.clone(),
		phase.clone(),
		cx10.clone(),
		cx10.clone(),
		cx20.clone(),
		phase,
		cx20,
		cx10,
	];
	let result = fold_parity(3, &source, ParityOptions::default())?;
	expect_eq!(result.operations().len(), 6);
	for basis in 0..8 {
		expect_eq!(
			evaluate(&source, basis),
			evaluate(result.operations(), basis)
		);
	}
	Ok(())
}

#[gtest]
fn parity_candidate_can_be_longer_while_the_existing_pass_retains_input() -> Result<()> {
	let phase = A::Phase {
		target: 0,
		coefficient: RBig::from_parts_signed(1.into(), 4.into()),
	};
	let source = [
		phase.clone(),
		A::Cnot(Cnot {
			control: 1,
			target: 0,
		}),
		phase,
	];
	let candidate = fold_parity_candidate(2, &source, ParityOptions::default(), 5)?;
	expect_eq!(candidate.operations().len(), 5);
	for basis in 0..4 {
		expect_eq!(
			evaluate(&source, basis),
			evaluate(candidate.operations(), basis)
		);
	}
	expect_true!(fold_parity_candidate(2, &source, ParityOptions::default(), 4).is_err());
	expect_eq!(
		fold_parity(2, &source, ParityOptions::default())?.operations(),
		&source
	);
	Ok(())
}

#[gtest]
fn program_candidate_expands_a_window_after_a_fence_with_fresh_ids() -> Result<()> {
	use quest_compile::{Gate, QuantumRegionBuilder};
	let mut builder = QuantumRegionBuilder::new(2, 0)?;
	let q = builder.qubit(0)?;
	let r = builder.qubit(1)?;
	builder.gate(Gate::H, &[q], &[])?;
	builder.gate(Gate::T, &[q], &[])?;
	builder.gate(
		Gate::X,
		&[q],
		&[quest_compile::Control::new(
			r,
			quest_compile::ControlState::One,
		)],
	)?;
	builder.gate(Gate::T, &[q], &[])?;
	let original = builder.finish()?;
	let (candidate, report) = original
		.clone()
		.parity_candidate(ParityOptions::default(), 6)?;
	expect_eq!(candidate.schedule().len(), 6);
	expect_eq!(report.accepted_windows, 1);
	let ids = candidate
		.schedule()
		.iter()
		.copied()
		.collect::<std::collections::BTreeSet<_>>();
	expect_eq!(ids.len(), 6);
	candidate.bind(&[])?.plan()?;
	let (unchanged, _) = original.optimize_parity(ParityOptions::default())?;
	expect_eq!(unchanged.schedule().len(), 4);
	Ok(())
}

#[gtest]
fn program_candidate_can_start_after_an_earlier_affine_window() -> Result<()> {
	use quest_compile::{Gate, QuantumRegionBuilder};
	let mut builder = QuantumRegionBuilder::new(2, 0)?;
	let q = builder.qubit(0)?;
	let r = builder.qubit(1)?;
	builder.gate(Gate::T, &[q], &[])?;
	builder.gate(Gate::H, &[q], &[])?;
	builder.gate(Gate::T, &[q], &[])?;
	builder.gate(
		Gate::X,
		&[q],
		&[quest_compile::Control::new(
			r,
			quest_compile::ControlState::One,
		)],
	)?;
	builder.gate(Gate::T, &[q], &[])?;
	let original = builder.finish()?;
	let retained = original.schedule()[0..2].to_vec();
	let (candidate, report) = original.parity_candidate_from(2, ParityOptions::default(), 7)?;
	expect_eq!(report.accepted_windows, 1);
	expect_eq!(report.candidate_window, Some((2, 5)));
	expect_eq!(candidate.schedule().len(), 7);
	expect_eq!(&candidate.schedule()[0..2], retained.as_slice());
	candidate.bind(&[])?.plan()?;
	Ok(())
}

#[gtest]
fn bound_symbolic_parity_cancels_only_after_original_binding_and_keeps_source_identity()
-> Result<()> {
	use quest_compile::{Angle, Gate, QuantumRegionBuilder};
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let p = builder.parameter("p")?;
	let q = builder.qubit(0)?;
	let angle = Angle::parameter(p)?;
	builder.gate(Gate::Phase(angle.clone()), &[q], &[])?;
	builder.gate(Gate::Phase(angle.negated()?), &[q], &[])?;
	let ideal = builder.finish()?;
	let bound = ideal.clone().bind(&[(p, 0.25)])?;
	let (candidate, report) =
		ideal.parity_bound_candidate_from(&bound, 0, ParityOptions::default(), 2)?;
	expect_eq!(report.accepted_windows, 1);
	expect_eq!(candidate.instructions().len(), 0);
	expect_eq!(candidate.source_snapshot_id(), bound.source_snapshot_id());
	candidate.plan()?;
	Ok(())
}

#[gtest]
fn bound_symbolic_parity_falls_back_on_new_overflow_and_rejects_foreign_bound_input() -> Result<()>
{
	use quest_compile::{Angle, Gate, QuantumRegionBuilder};
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let p = builder.parameter("p")?;
	let q = builder.qubit(0)?;
	let angle = Angle::parameter(p)?;
	builder.gate(Gate::Phase(angle.clone()), &[q], &[])?;
	builder.gate(Gate::Phase(angle), &[q], &[])?;
	let ideal = builder.finish()?;
	let bound = ideal.clone().bind(&[(p, f64::MAX)])?;
	let (candidate, report) =
		ideal.parity_bound_candidate_from(&bound, 0, ParityOptions::default(), 2)?;
	expect_eq!(report.accepted_windows, 0);
	expect_eq!(candidate.snapshot_id(), bound.snapshot_id());
	let mut foreign_builder = QuantumRegionBuilder::new(1, 0)?;
	foreign_builder.gate(Gate::X, &[foreign_builder.qubit(0)?], &[])?;
	let foreign = foreign_builder.finish()?.bind(&[])?;
	expect_true!(
		ideal
			.parity_bound_candidate_from(&foreign, 0, ParityOptions::default(), 2)
			.is_err()
	);
	Ok(())
}

#[gtest]
fn bound_parity_keeps_rz_two_pi_scalar_and_signed_zero_source() -> Result<()> {
	use quest_compile::{Angle, Gate, Operation, QuantumRegionBuilder};
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let q = builder.qubit(0)?;
	builder.gate(Gate::Rz(Angle::pi(2, 1)?), &[q], &[])?;
	let ideal = builder.finish()?;
	let bound = ideal.clone().bind(&[])?;
	let (candidate, report) =
		ideal.parity_bound_candidate_from(&bound, 0, ParityOptions::default(), 1)?;
	expect_eq!(report.accepted_windows, 1);
	let [phase] = candidate.instructions() else {
		return Err(std::io::Error::other("expected scalar phase").into());
	};
	expect_true!(
		matches!(phase.operation(), Operation::GlobalPhase { radians, controls }
        if radians.to_bits() == std::f64::consts::PI.to_bits() && controls.is_empty())
	);

	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let p = builder.parameter("p")?;
	let q = builder.qubit(0)?;
	builder.gate(Gate::Phase(Angle::parameter(p)?), &[q], &[])?;
	let ideal = builder.finish()?;
	let bound = ideal.clone().bind(&[(p, -0.0)])?;
	let (candidate, report) =
		ideal.parity_bound_candidate_from(&bound, 0, ParityOptions::default(), 1)?;
	expect_eq!(report.accepted_windows, 0);
	expect_eq!(candidate.snapshot_id(), bound.snapshot_id());
	Ok(())
}

#[gtest]
fn bound_parity_precharges_original_binding_and_independent_affine_replay() -> Result<()> {
	use quest_compile::{Angle, Gate, LinearOptions, QuantumRegionBuilder};
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let p = builder.parameter("p")?;
	let q = builder.qubit(0)?;
	let mixed = Angle::parameter(p)?.added(&Angle::affine(
		RBig::from_parts_signed(1.into(), 8.into()),
		RBig::from_parts_signed(1.into(), 4.into()),
	)?)?;
	builder.gate(Gate::Phase(mixed.clone()), &[q], &[])?;
	builder.gate(Gate::Phase(mixed.negated()?), &[q], &[])?;
	let ideal = builder.finish()?;
	let bound = ideal.clone().bind(&[(p, 0.5)])?;
	let options = ParityOptions {
		linear: LinearOptions {
			max_work: 3,
			..LinearOptions::default()
		},
		..ParityOptions::default()
	};
	expect_true!(
		ideal
			.parity_bound_candidate_from(&bound, 0, options, 2)
			.is_err()
	);
	let (candidate, report) =
		ideal.parity_bound_candidate_from(&bound, 0, ParityOptions::default(), 2)?;
	expect_eq!(report.accepted_windows, 1);
	expect_true!(candidate.instructions().is_empty());
	Ok(())
}

fn evaluate(operations: &[A], mut basis: u64) -> (u64, RBig) {
	let mut phase = RBig::from(0);
	for operation in operations {
		let addition = match operation {
			A::X { target } => {
				basis ^= 1u64 << target;
				continue;
			}
			A::Cnot(gate) => {
				if basis & (1u64 << gate.control) != 0 {
					basis ^= 1u64 << gate.target;
				}
				continue;
			}
			A::Phase {
				target,
				coefficient,
			} => {
				if basis & (1u64 << target) != 0 {
					coefficient.clone()
				} else {
					RBig::from(0)
				}
			}
			A::Rz {
				target,
				coefficient,
			} => {
				let sign = if basis & (1u64 << target) != 0 { 1 } else { -1 };
				std::ops::Mul::mul(coefficient, RBig::from_parts_signed(sign.into(), 2.into()))
			}
			A::GlobalPhase { coefficient } => coefficient.clone(),
		};
		phase = std::ops::Add::add(phase, addition);
	}
	let period = std::ops::Mul::mul(phase.denominator(), dashu_int::IBig::from(2));
	let remainder = std::ops::Rem::rem(phase.numerator(), &period);
	let numerator = std::ops::Rem::rem(std::ops::Add::add(remainder, &period), period);
	(
		basis,
		RBig::from_parts(numerator, phase.denominator().clone()),
	)
}
const fn random(state: &mut u64) -> u64 {
	*state ^= *state << 13;
	*state ^= *state >> 7;
	*state ^= *state << 17;
	*state
}
#[gtest]
fn seeded_affine_phase_windows_preserve_every_basis_column_and_scalar_phase() -> Result<()> {
	let mut state = 0xcafe_5eedu64;
	let mut accepted = false;
	for width in 1..=4 {
		for _ in 0..40 {
			let mut source = vec![];
			for _ in 0..80 {
				let target = usize::try_from(
					random(&mut state)
						.checked_rem(u64::try_from(width)?)
						.ok_or_else(|| std::io::Error::other("fixture width"))?,
				)?;
				let coefficient = RBig::from_parts_signed(
					i64::try_from(random(&mut state) % 17)?
						.checked_sub(8)
						.ok_or_else(|| std::io::Error::other("fixture numerator"))?
						.into(),
					4.into(),
				);
				source.push(match random(&mut state) % 5 {
					0 => A::X { target },
					1 if width > 1 => {
						let control = target
							.checked_add(1)
							.and_then(|value| value.checked_rem(width))
							.ok_or_else(|| std::io::Error::other("fixture target"))?;
						A::Cnot(Cnot { control, target })
					}
					2 => A::Rz {
						target,
						coefficient,
					},
					3 => A::GlobalPhase { coefficient },
					_ => A::Phase {
						target,
						coefficient,
					},
				});
			}
			let result = fold_parity(width, &source, ParityOptions::default())?;
			accepted |= result.operations().len() < source.len();
			for basis in 0..(1u64 << width) {
				expect_eq!(
					evaluate(&source, basis),
					evaluate(result.operations(), basis)
				);
			}
		}
	}
	expect_true!(accepted);
	Ok(())
}

#[gtest]
fn program_folding_retains_rz_scalar_phase_and_stops_at_effects_and_opaque_angles() -> Result<()> {
	use quest_compile::{Angle, Gate, Operation, QuantumRegionBuilder};
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let q = builder.qubit(0)?;
	for _ in 0..2 {
		builder.gate(Gate::Rz(Angle::pi(1, 1)?), &[q], &[])?;
	}
	let (optimized, report) = builder
		.finish()?
		.optimize_parity(ParityOptions::default())?;
	expect_eq!(report.accepted_windows, 1);
	let bound = optimized.bind(&[])?;
	let [instruction] = bound.instructions() else {
		return Err(std::io::Error::other("expected retained scalar phase").into());
	};
	expect_true!(
		matches!(instruction.operation(), Operation::GlobalPhase { radians, controls } if radians.to_bits() == std::f64::consts::PI.to_bits() && controls.is_empty())
	);
	let mut builder = QuantumRegionBuilder::new(1, 1)?;
	let q = builder.qubit(0)?;
	let b = builder.bit(0)?;
	builder.gate(Gate::Rz(Angle::pi(1, 1)?), &[q], &[])?;
	builder.measure(q, b)?;
	builder.gate(Gate::Rz(Angle::pi(1, 1)?), &[q], &[])?;
	builder.reset(q)?;
	builder.gate(Gate::Phase(Angle::radians(0.3)?), &[q], &[])?;
	builder.gate(Gate::Phase(Angle::radians(-0.3)?), &[q], &[])?;
	let (unchanged, report) = builder
		.finish()?
		.optimize_parity(ParityOptions::default())?;
	expect_eq!(unchanged.schedule().len(), 6);
	expect_eq!(report.accepted_windows, 0);
	Ok(())
}

#[gtest]
fn negative_controls_and_explicit_edges_guard_parity_replacements() -> Result<()> {
	use quest_compile::{Control, ControlState, Gate, QuantumRegionBuilder};
	let mut builder = QuantumRegionBuilder::new(2, 0)?;
	let target = builder.qubit(0)?;
	let controls = [Control::new(builder.qubit(1)?, ControlState::Zero)];
	for _ in 0..2 {
		builder.gate(Gate::X, &[target], &controls)?;
	}
	let (unchanged, report) = builder
		.finish()?
		.optimize_parity(ParityOptions::default())?;
	expect_eq!(unchanged.schedule().len(), 2);
	expect_eq!(report.considered_windows, 0);
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let target = builder.qubit(0)?;
	let first = builder.gate(Gate::T, &[target], &[])?;
	let second = builder.gate(Gate::Tdg, &[target], &[])?;
	builder.depend(first, second)?;
	let (unchanged, report) = builder
		.finish()?
		.optimize_parity(ParityOptions::default())?;
	expect_eq!(unchanged.schedule().len(), 2);
	expect_eq!(report.considered_windows, 0);
	let mut oversized = dashu_int::UBig::ZERO;
	oversized.set_bit(4097);
	expect_true!(
		fold_parity(
			1,
			&[A::GlobalPhase {
				coefficient: RBig::from(oversized)
			}],
			ParityOptions::default()
		)
		.is_err()
	);
	Ok(())
}

#[gtest]
fn canonical_first_window_candidate_preserves_two_pi_scalar_and_output_limit() -> Result<()> {
	use quest_compile::{Angle, Gate, Operation, QuantumRegionBuilder};
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	builder.gate(Gate::Rz(Angle::pi(2, 1)?), &[builder.qubit(0)?], &[])?;
	let source = builder.finish()?;
	expect_true!(
		source
			.clone()
			.parity_candidate(ParityOptions::default(), 0)
			.is_err()
	);
	let (candidate, report) = source.parity_candidate(ParityOptions::default(), 1)?;
	expect_eq!(report.accepted_windows, 1);
	let bound = candidate.bind(&[])?;
	let [instruction] = bound.instructions() else {
		return fail!("expected exact scalar phase");
	};
	expect_true!(
		matches!(instruction.operation(), Operation::GlobalPhase { radians, controls }
        if radians.to_bits() == std::f64::consts::PI.to_bits() && controls.is_empty())
	);
	Ok(())
}
