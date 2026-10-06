#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Small actual trace and independent one-shot full-state reference comparison"
)]
#[path = "../examples/cylinder_window.rs"]
mod window;
use clap::Parser;
use window::{Options, admit, experiment};
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
fn options(extra: &[&str]) -> Result<Options> {
	Ok(Options::try_parse_from(
		std::iter::once("cylinder_window").chain(extra.iter().copied()),
	)?)
}
#[test]
fn frozen_defaults_admit_full_window_without_claiming_convergence() -> Result {
	let admitted = admit(&options(&[])?)?;
	assert!(admitted.frozen_window);
	assert_eq!(admitted.observation_steps, [40_000, 80_000]);
	assert_eq!(admitted.sample_count, 401);
	assert!(admitted.aggregate_work > 80_000);
	Ok(())
}
#[test]
fn rejects_invalid_time_windows_and_full_aggregate_budgets() -> Result {
	for args in [
		vec!["--dt", "0"],
		vec!["--dt", "NaN"],
		vec!["--steps", "0"],
		vec!["--stride", "0"],
		vec!["--steps", "4"],
		vec!["--observation-start", "7", "--observation-end", "6"],
		vec!["--observation-start", "4.00001"],
		vec!["--max-work", "1"],
		vec!["--max-bytes", "1"],
		vec!["--max-trace-bytes", "1"],
		vec!["--radial-layers", "0"],
		vec!["--max-residual", "NaN"],
	] {
		assert!(
			admit(&options(&args)?).is_err(),
			"admitted invalid {args:?}"
		);
	}
	let mut o = options(&[])?;
	let a = admit(&o)?;
	o.max_work = a.aggregate_work - 1;
	assert!(admit(&o).is_err());
	o.max_work = a.aggregate_work;
	o.max_trace_bytes = a.trace_bytes - 1;
	assert!(admit(&o).is_err());
	Ok(())
}
#[test]
fn tiny_trace_advances_one_state_and_includes_unaligned_stride_endpoint() -> Result {
	let o = options(&[
		"--short-window",
		"--dt",
		"0.000001",
		"--steps",
		"5",
		"--stride",
		"2",
		"--observation-start",
		"0",
		"--observation-end",
		"0.000005",
	])?;
	let mut progress = window::Progress::default();
	let result = experiment(&o, &mut progress)?;
	assert!(!result.admission.frozen_window);
	assert_eq!(result.trace.len(), 4);
	for (sample, expected_time) in result
		.trace
		.iter()
		.zip([0., 0.000_002, 0.000_004, 0.000_005])
	{
		assert!((sample.time - expected_time).abs() < 1e-20);
	}
	assert_eq!(progress.completed_steps, 5);
	let reference = quest_cfd::cylinder::reference("shedding2d", 100, 4, 1, 1)?;
	let expected = reference.reference(o.dt, o.steps)?;
	let last = result.trace.last().ok_or("missing trace endpoint")?;
	assert!((last.drag - expected.drag_coefficient).abs() < 1e-10);
	assert!((last.lift - expected.lift_coefficient).abs() < 1e-10);
	assert!((last.pressure_difference - expected.pressure_difference).abs() < 1e-9);
	assert!(result.statistics.frequency.is_none());
	assert!(!result.statistics.periodicity_certified);
	assert!(result.maximum_geometry_deviation > 0.);
	Ok(())
}

#[test]
fn excessive_pressure_recovery_residual_is_rejected_without_dropping_a_sample() -> Result {
	let o = options(&[
		"--short-window",
		"--dt",
		"0.000001",
		"--steps",
		"5",
		"--stride",
		"2",
		"--observation-start",
		"0",
		"--observation-end",
		"0.000005",
		"--max-residual",
		"1e-30",
	])?;
	assert!(admit(&o).is_ok());
	let mut progress = window::Progress::default();
	assert!(experiment(&o, &mut progress).is_err());
	assert_eq!(progress.completed_steps, 0);
	Ok(())
}
