#![forbid(unsafe_code)]

use quest::collective::{CollectiveEnvironment, MpiRuntime};
use quest::{Outcome, Program, QubitCount};
use std::error::Error;
use std::time::Duration;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn worker(expected_ranks: i32) -> Result<()> {
	let runtime = MpiRuntime::initialize()?;
	let mut communicator = runtime.world()?;
	if !communicator.all_agree(communicator.size()? == expected_ranks)? {
		return Err("unexpected MPI rank count".into());
	}
	let correct = {
		let environment = CollectiveEnvironment::builder(&communicator)?
			.with_multithreading()
			.build()?;
		let program = Program::parse(
			"qubit[5] q; h q[0]; cx q[0], q[4];",
			"independent MPI consumer",
		)?
		.verify()?
		.lower()?
		.plan()?;
		let mut prepared = environment.prepare(program)?;
		let mut register = environment.state_vector(QubitCount::new(5)?)?;
		register.init_zero()?;
		prepared.run(&mut register)?;
		let deployment = register.deployment();
		let rank = usize::try_from(communicator.rank()?)?;
		let ranks = usize::try_from(expected_ranks)?;
		let local_count = 32 / ranks;
		let local_state = register.read_local_amplitudes(0, local_count)?;
		let deployment_correct = deployment.rank() == rank
			&& deployment.nodes() == ranks
			&& deployment.local_amplitudes() == local_count
			&& deployment.is_multithreaded()
			&& (ranks == 1 || deployment.is_distributed());
		let state_correct = local_state.iter().enumerate().all(|(index, value)| {
			let global_index = rank * local_count + index;
			let expected = if matches!(global_index, 0 | 17) {
				std::f64::consts::FRAC_1_SQRT_2
			} else {
				0.0
			};
			(value.re - expected).abs() < 1e-13 && value.im.abs() < 1e-13
		});
		let low = register.probability(0, Outcome::One)?.get();
		let high = register.probability(4, Outcome::One)?.get();
		let norm = register.total_probability()?;
		register.project(0, Outcome::One)?;
		register.project(4, Outcome::Zero)?;
		let forbidden_joint = register.total_probability()?;
		deployment_correct
			&& state_correct
			&& (low - 0.5).abs() < 1e-13
			&& (high - 0.5).abs() < 1e-13
			&& (norm - 1.0).abs() < 1e-13
			&& forbidden_joint.abs() < 1e-13
	};
	if !communicator.all_agree(correct && runtime.is_active()?)? {
		return Err("collective state or MPI lifetime check failed".into());
	}
	let rank = communicator.rank()?;
	drop(communicator);
	drop(runtime);
	if !MpiRuntime::is_finalized() {
		return Err("MPI runtime was not finalized by its owner".into());
	}
	if rank == 0 {
		println!("MPI_CONSUMER_VERIFIED ranks={expected_ranks}");
	}
	Ok(())
}

fn main() -> Result<()> {
	let arguments: Vec<_> = std::env::args().skip(1).collect();
	match arguments.as_slice() {
		[mode, count] if mode == "--worker" => worker(count.parse()?),
		[] => {
			for ranks in [1, 2, 4, 8] {
				let output = quest_test_support::mpi::MpiTest::new(ranks, Duration::from_secs(60))?
					.args(["--worker", &ranks.to_string()])
					.env("OMP_NUM_THREADS", "2")
					.output()?;
				let stdout = String::from_utf8(output.stdout)?;
				let witness = format!("MPI_CONSUMER_VERIFIED ranks={ranks}");
				if !output.status.success()
					|| stdout.lines().filter(|line| *line == witness).count() != 1
				{
					return Err(format!(
						"MPI consumer failed at {ranks} ranks: {}\n{stdout}\n{}",
						output.status,
						String::from_utf8_lossy(&output.stderr),
					)
					.into());
				}
				println!("{witness}");
			}
			Ok(())
		}
		_ => Err("usage: quest-independent-mpi-consumer [--worker RANKS]".into()),
	}
}
