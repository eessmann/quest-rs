#![cfg(feature = "workers")]
use googletest::prelude::*;
use quest::{Complex64, Environment, QubitCount, RunInputs, circuit};
#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Independent finite residuals of normalized two-qubit fixtures"
)]
fn certified_ssa_synthesis_preserves_native_state_and_density() -> Result<()> {
	const NAME: &str = "certified_ssa_synthesis_preserves_native_state_and_density";
	let Some(worker) = std::env::var_os("QUEST_TUTORIAL_WORKER") else {
		return Ok(());
	};
	if std::env::var("QUEST_SYNTHESIS_CHILD").as_deref() != Ok(NAME) {
		let status = std::process::Command::new(std::env::current_exe()?)
			.args(["--exact", NAME, "--nocapture", "--test-threads=1"])
			.env("QUEST_SYNTHESIS_CHILD", NAME)
			.status()?;
		expect_true!(status.success());
		return Ok(());
	}
	let client = quest::optimizer::Client::new(worker, quest::optimizer::WorkerLimits::default())?;
	let environment = Environment::builder().build()?;
	let original = circuit! {qubit[2] q; negctrl @ ry(${0.23_f64}) q[1],q[0];}?.verify()?;
	let (candidate, report) = original.clone().synthesize_rotations(
		&client,
		1e-12,
		2026,
		quest::certified::Limits::default(),
	)?;
	expect_eq!(report.rotations.len(), 1);
	let mut before = environment.prepare(original.lower()?.plan()?)?;
	let mut after = environment.prepare(candidate.lower()?.plan()?)?;
	let mut state_before = environment.state_vector(QubitCount::new(2)?)?;
	let mut state_after = environment.state_vector(QubitCount::new(2)?)?;
	let mut density_before = environment.density_matrix(QubitCount::new(2)?)?;
	let mut density_after = environment.density_matrix(QubitCount::new(2)?)?;
	for column in 0..5 {
		let mut values = [Complex64::new(0., 0.); 4];
		if let Some(value) = values.get_mut(column) {
			*value = Complex64::new(1., 0.);
		} else {
			values = [
				Complex64::new(0.5, 0.),
				Complex64::new(0., 0.5),
				Complex64::new(-0.5, 0.),
				Complex64::new(0., -0.5),
			];
		}
		state_before.init_pure(&values)?;
		state_after.init_pure(&values)?;
		density_before.init_pure(&values)?;
		density_after.init_pure(&values)?;
		before.run(&mut state_before, &RunInputs::default())?;
		after.run(&mut state_after, &RunInputs::default())?;
		before.run(&mut density_before, &RunInputs::default())?;
		after.run(&mut density_after, &RunInputs::default())?;
		for row in 0..4 {
			expect_lt!(
				(state_before.amplitude(row)? - state_after.amplitude(row)?).norm(),
				3e-12
			);
			for column in 0..4 {
				expect_lt!(
					(density_before.entry(row, column)? - density_after.entry(row, column)?).norm(),
					3e-12
				);
			}
		}
	}
	Ok(())
}
