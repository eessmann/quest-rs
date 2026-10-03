use googletest::prelude::*;
#[path = "../examples/tutorials.rs"]
mod examples;

// ANCHOR: native_tutorial_tests
#[gtest]
fn executable_native_tutorials_pass_in_an_isolated_process() -> Result<()> {
	const NAME: &str = "executable_native_tutorials_pass_in_an_isolated_process";
	if std::env::var("QUEST_TUTORIAL_CHILD").as_deref() != Ok(NAME) {
		let status = std::process::Command::new(std::env::current_exe()?)
			.args(["--exact", NAME, "--nocapture", "--test-threads=1"])
			.env("QUEST_TUTORIAL_CHILD", NAME)
			.status()?;
		verify_that!(status.success(), eq(true))?;
		return Ok(());
	}
	let environment = quest::Environment::builder().build()?;
	let (zero, one) = examples::bell(&environment).or_fail()?;
	verify_that!(zero, near(0.5, 1.0e-13))?;
	verify_that!(one, near(0.5, 1.0e-13))?;
	verify_that!(
		examples::teleportation(&environment).or_fail()?,
		near(1.0, 1.0e-13)
	)?;
	verify_that!(
		examples::feedback(&environment).or_fail()?,
		near(1.0, 1.0e-13)
	)?;
	let (success, attempts) = examples::repeat_until_success(&environment).or_fail()?;
	verify_that!(success, eq(true))?;
	verify_that!(attempts, ge(1))?;
	verify_that!(attempts, le(2))?;
	verify_eq!(examples::captures_once(&environment).or_fail()?, (1, 4))?;
	verify_eq!(examples::array_arguments(&environment).or_fail()?, 6)?;
	Ok(())
}
// ANCHOR_END: native_tutorial_tests

#[cfg(feature = "workers")]
#[gtest]
fn optional_synthesis_tutorial_uses_an_explicit_worker_path() -> Result<()> {
	if let Some(path) = std::env::var_os("QUEST_TUTORIAL_WORKER") {
		verify_that!(
			examples::certified_synthesis(std::path::Path::new(&path)).or_fail()?,
			gt(0)
		)?;
		verify_eq!(
			examples::structured_certified_loop(std::path::Path::new(&path)).or_fail()?,
			1
		)?;
	}
	Ok(())
}
