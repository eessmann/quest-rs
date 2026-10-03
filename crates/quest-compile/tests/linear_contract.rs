use googletest::{Result, prelude::*};
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::{Cnot, LinearOptions, synthesize_cnot};

#[gtest]
fn candidate_generation_is_independent_of_local_shortening() -> Result<()> {
	use quest_compile::{
		Control, ControlState, Gate, LinearCandidateStrategy, QuantumRegionBuilder,
	};
	let input = [
		Cnot {
			control: 0,
			target: 1,
		},
		Cnot {
			control: 1,
			target: 2,
		},
		Cnot {
			control: 2,
			target: 0,
		},
	];
	let mut b = QuantumRegionBuilder::new(3, 0)?;
	for gate in &input {
		b.gate(
			Gate::X,
			&[b.qubit(gate.target)?],
			&[Control::new(b.qubit(gate.control)?, ControlState::One)],
		)?;
	}
	let p = b.finish()?;
	let old_ids = p.schedule().to_vec();
	let (candidate, report) = p.resynthesize_linear_candidate(
		LinearOptions::default(),
		LinearCandidateStrategy::Gaussian,
		64,
	)?;
	expect_eq!(report.accepted_windows, 1);
	expect_true!(candidate.schedule().iter().all(|id| !old_ids.contains(id)));
	let sequence = candidate
		.bind(&[])?
		.instructions()
		.iter()
		.map(|i| {
			if let quest_compile::Operation::Gate {
				targets, controls, ..
			} = i.operation()
			{
				Ok(Cnot {
					control: controls
						.first()
						.ok_or(quest_compile::Error::InvalidId)?
						.qubit()
						.index(),
					target: targets
						.first()
						.ok_or(quest_compile::Error::InvalidId)?
						.index(),
				})
			} else {
				Err(quest_compile::Error::NotUnitary)
			}
		})
		.collect::<quest_compile::Result<Vec<_>>>()?;
	expect_gt!(sequence.len(), input.len());
	for basis in 0..8 {
		expect_eq!(apply(&input, basis), apply(&sequence, basis));
	}
	Ok(())
}

fn apply(sequence: &[Cnot], mut input: u64) -> u64 {
	for gate in sequence {
		if (input >> gate.control) & 1 != 0 {
			input ^= 1u64 << gate.target;
		}
	}
	input
}

#[gtest]
fn gaussian_and_pmh_preserve_independent_basis_permutations_and_order() -> Result<()> {
	let source = [
		Cnot {
			control: 2,
			target: 0,
		},
		Cnot {
			control: 0,
			target: 1,
		},
		Cnot {
			control: 2,
			target: 0,
		},
	];
	let result = synthesize_cnot(3, &source, LinearOptions::default())?;
	for basis in 0..8 {
		expect_eq!(apply(&source, basis), apply(result.gates(), basis));
		expect_eq!(apply(&source, basis), apply(result.gaussian(), basis));
		expect_eq!(apply(&source, basis), apply(result.pmh(), basis));
	}
	expect_eq!(
		result.gates(),
		synthesize_cnot(3, &source, LinearOptions::default())?.gates()
	);
	Ok(())
}

#[gtest]
fn cnot_cancellations_and_preflight_bounds_are_checked() -> Result<()> {
	let source = [Cnot {
		control: 1,
		target: 0,
	}; 2];
	expect_true!(
		synthesize_cnot(2, &source, LinearOptions::default())?
			.gates()
			.is_empty()
	);
	expect_true!(synthesize_cnot(65, &source, LinearOptions::default()).is_err());
	expect_true!(
		synthesize_cnot(
			2,
			&[Cnot {
				control: 1,
				target: 1
			}],
			LinearOptions::default()
		)
		.is_err()
	);
	for options in [
		LinearOptions {
			max_work: 0,
			..LinearOptions::default()
		},
		LinearOptions {
			max_bytes: 0,
			..LinearOptions::default()
		},
		LinearOptions {
			block_size: 0,
			..LinearOptions::default()
		},
	] {
		expect_true!(synthesize_cnot(2, &source, options).is_err());
	}
	Ok(())
}

const fn random(state: &mut u64) -> u64 {
	*state ^= *state << 13;
	*state ^= *state >> 7;
	*state ^= *state << 17;
	*state
}

#[gtest]
fn seeded_binary_matrices_verify_both_algorithms_through_sixty_four_wires() -> Result<()> {
	let mut state = 0xd00d_5eedu64;
	let mut better = false;
	for width in [2usize, 3, 4, 8, 16, 32, 64] {
		for _ in 0..8 {
			let mut source = vec![];
			for _ in 0..256 {
				let control = usize::try_from(
					random(&mut state)
						.checked_rem(u64::try_from(width)?)
						.ok_or_else(|| std::io::Error::other("fixture width"))?,
				)?;
				let target = usize::try_from(
					random(&mut state)
						.checked_rem(u64::try_from(width)?)
						.ok_or_else(|| std::io::Error::other("fixture width"))?,
				)?;
				if control != target {
					source.push(Cnot { control, target });
				}
			}
			let result = synthesize_cnot(width, &source, LinearOptions::default())?;
			better |= result.pmh().len() < result.gaussian().len();
			for bit in 0..width {
				let basis = 1u64 << bit;
				expect_eq!(apply(&source, basis), apply(result.gaussian(), basis));
				expect_eq!(apply(&source, basis), apply(result.pmh(), basis));
				expect_eq!(apply(&source, basis), apply(result.gates(), basis));
			}
			expect_le!(result.gates().len(), source.len());
		}
	}
	expect_true!(better);
	Ok(())
}

#[gtest]
fn program_linear_windows_preserve_barriers_and_explicit_dependencies() -> Result<()> {
	use quest_compile::{Control, ControlState, Gate, QuantumRegionBuilder};
	let mut builder = QuantumRegionBuilder::new(2, 0)?;
	let zero = builder.qubit(0)?;
	let one = builder.qubit(1)?;
	let controls = [Control::new(one, ControlState::One)];
	for _ in 0..2 {
		builder.gate(Gate::X, &[zero], &controls)?;
	}
	builder.barrier(&[zero, one])?;
	for _ in 0..2 {
		builder.gate(Gate::X, &[zero], &controls)?;
	}
	let (optimized, report) = builder
		.finish()?
		.optimize_linear(LinearOptions::default())?;
	expect_eq!(report.accepted_windows, 2);
	expect_eq!(optimized.schedule().len(), 1);
	expect_eq!(report.rewrites.len(), 2);
	let mut builder = QuantumRegionBuilder::new(2, 0)?;
	let target = builder.qubit(0)?;
	let control = [Control::new(builder.qubit(1)?, ControlState::One)];
	let first = builder.gate(Gate::X, &[target], &control)?;
	let second = builder.gate(Gate::X, &[target], &control)?;
	builder.depend(first, second)?;
	let (unchanged, report) = builder
		.finish()?
		.optimize_linear(LinearOptions::default())?;
	expect_eq!(unchanged.schedule().len(), 2);
	expect_eq!(report.accepted_windows, 0);
	Ok(())
}

#[gtest]
fn whole_program_scan_and_output_copy_are_in_the_pass_budget() -> Result<()> {
	use quest_compile::QuantumRegionBuilder;
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let q = builder.qubit(0)?;
	for _ in 0..100 {
		builder.barrier(&[q])?;
	}
	let program = builder.finish()?;
	expect_true!(
		program
			.clone()
			.optimize_linear(LinearOptions {
				max_work: 0,
				..LinearOptions::default()
			})
			.is_err()
	);
	expect_true!(
		program
			.optimize_linear(LinearOptions {
				max_bytes: 5000,
				..LinearOptions::default()
			})
			.is_err()
	);
	Ok(())
}
