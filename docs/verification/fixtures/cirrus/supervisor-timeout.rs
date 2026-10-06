//! Real Slurm timeout probe, linked against the selected snapshot's supervisor.
//! Run once as the batch coordinator with the same launcher settings as test-rust.sh.
use quest_test_support::mpi::MpiTest;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let start = Instant::now();
	let stopped = MpiTest::new(2, Duration::from_secs(5))?
		.executable("/bin/sh")
		.args(["-c", "printf 'timeout-child-ready\\n'; exec sleep 60"])
		.output()?;
	assert!(stopped.status.timed_out, "probe did not reach its deadline");
	assert!(
		String::from_utf8_lossy(&stopped.stdout).contains("timeout-child-ready"),
		"child never started; launch delay is not timeout-path evidence"
	);
	let cancellation = stopped
		.cancellation
		.as_ref()
		.expect("missing owned-step cancellation");
	assert!(cancellation.success, "scheduler rejected cancellation");
	assert!(
		cancellation.termination_confirmed,
		"step termination unconfirmed"
	);
	assert_eq!(stopped.slurm_step.as_ref(), Some(&cancellation.step));
	assert!(
		start.elapsed() < Duration::from_secs(20),
		"cleanup exceeded bound"
	);

	// Only the owned step may be cancelled: the same allocation must remain usable.
	let next = MpiTest::new(2, Duration::from_secs(15))?
		.executable("/bin/true")
		.output()?;
	assert!(
		next.status.success(),
		"allocation did not survive step cancellation"
	);
	assert!(next.slurm_step.is_some());
	assert_ne!(next.slurm_step, stopped.slurm_step);
	println!(
		"owned_step={} cancellation_accepted=true termination_confirmed=true allocation_reused=true elapsed_seconds={:.3}",
		cancellation.step,
		start.elapsed().as_secs_f64()
	);
	Ok(())
}
