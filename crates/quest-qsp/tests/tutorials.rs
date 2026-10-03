#[path = "../examples/qsp_tutorials.rs"]
mod tutorials;
use googletest::prelude::*;
#[gtest]
fn book_binary64_polynomial_function_and_synthesis_examples_run() -> Result<()> {
	expect_that!(
		checked(tutorials::polynomial_and_interval())?,
		near(-0.72, 1e-14)
	);
	expect_that!(checked(tutorials::function_and_remez())?, lt(0.006));
	expect_that!(
		checked(tutorials::canonical_synthesis())?.response(0.3)?,
		near(0.18, 1e-11)
	);
	expect_that!(
		checked(tutorials::generalized_synthesis())?
			.controls()
			.len(),
		eq(2)
	);
	expect_that!(checked(tutorials::stage_observation())?, eq(1));
	Ok(())
}
#[cfg(feature = "certification")]
#[gtest]
fn book_independent_certification_example_runs() -> Result<()> {
	expect_that!(checked(tutorials::independent_certification())?, le(1e-11));
	Ok(())
}
#[cfg(feature = "offline-synthesis")]
#[gtest]
fn book_explicit_offline_examples_run() -> Result<()> {
	expect_that!(checked(tutorials::explicit_offline_synthesis())?, le(1e-11));
	expect_that!(
		checked(tutorials::explicit_multiprecision_approximation())?,
		le(0.006)
	);
	Ok(())
}

fn checked<T>(result: tutorials::TutorialResult<T>) -> Result<T> {
	result.map_err(|error| std::io::Error::other(error.to_string()).into())
}

#[cfg(feature = "rayon")]
#[gtest]
fn book_caller_pool_example_runs() -> Result<()> {
	expect_that!(
		checked(tutorials::parallel_synthesis())?.response(0.3)?,
		near(0.18, 1e-11)
	);
	Ok(())
}
