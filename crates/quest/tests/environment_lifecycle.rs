use googletest::prelude::*;
use quest::{Environment, Error, ExecutionMode, QubitCount};

fn isolated(name: &str, body: impl FnOnce() -> Result<()>) -> Result<()> {
	if std::env::var("QUEST_ENVIRONMENT_LIFECYCLE_TEST").as_deref() == Ok(name) {
		return body();
	}
	let output = std::process::Command::new(std::env::current_exe()?)
		.args(["--exact", name, "--nocapture", "--test-threads=1"])
		.env("QUEST_ENVIRONMENT_LIFECYCLE_TEST", name)
		.output()?;
	if !output.status.success() {
		return fail!(
			"child status: {}\n{}\n{}",
			output.status,
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
	}
	Ok(())
}

fn completed_finalization() -> Result<()> {
	verify_false!(quest_sys::is_quest_env_init())?;
	// Inactive alone also describes failed retirement. Only completed cleanup
	// permits this idempotent low-level finalization probe to succeed.
	quest_sys::finalize_quest_env()?;
	verify_true!(Environment::builder().build().is_err())?;
	verify_true!(quest_sys::init_custom_quest_env_modes(0, 0, 0).is_err())?;
	Ok(())
}

#[gtest]
fn orphaned_native_handle_retires_runtime_without_panicking() -> Result<()> {
	isolated(
		"orphaned_native_handle_retires_runtime_without_panicking",
		|| {
			let orphan = {
				let _environment = Environment::builder().build()?;
				quest_sys::create_qureg(1)?
			};
			verify_false!(quest_sys::is_quest_env_init())?;
			verify_true!(quest_sys::finalize_quest_env().is_err())?;
			verify_true!(quest_sys::get_quest_env().is_err())?;
			verify_true!(quest_sys::create_qureg(1).is_err())?;
			verify_true!(Environment::builder().build().is_err())?;
			verify_true!(quest_sys::init_custom_quest_env_modes(0, 0, 0).is_err())?;
			// This deliberately outlived the owner, so its native allocation must
			// remain untouched. Destruction of the Rust handle still cannot panic.
			drop(orphan);
			verify_true!(quest_sys::finalize_quest_env().is_err())?;
			Ok(())
		},
	)
}

#[gtest]
fn scope_exit_finalizes_and_owned_snapshot_survives() -> Result<()> {
	isolated("scope_exit_finalizes_and_owned_snapshot_survives", || {
		let snapshot = {
			let environment = Environment::builder().build()?;
			let mut register = environment.state_vector(QubitCount::new(1)?)?;
			register.x(0)?;
			register.snapshot()?
		};
		completed_finalization()?;
		verify_eq!(snapshot.nrows(), 2)?;
		verify_eq!(snapshot[(1, 0)], quest::Complex64::new(1., 0.))?;
		Ok(())
	})
}

#[gtest]
fn early_error_return_finalizes_borrowing_resources() -> Result<()> {
	fn run() -> quest::Result<()> {
		let environment = Environment::builder().build()?;
		let _prepared = environment.prepare(
			(quest::circuit! {qubit q; reset q;}?)
				.verify()?
				.lower()?
				.plan()?,
		)?;
		let mut register = environment.state_vector(QubitCount::new(1)?)?;
		register.h(1)?;
		Ok(())
	}
	isolated("early_error_return_finalizes_borrowing_resources", || {
		verify_true!(matches!(run(), Err(Error::Index { index: 1, bound: 1 })))?;
		completed_finalization()
	})
}

#[gtest]
fn duplicate_initialization_leaves_existing_owner_usable() -> Result<()> {
	isolated(
		"duplicate_initialization_leaves_existing_owner_usable",
		|| {
			{
				let environment = Environment::builder().build()?;
				let mut register = environment.state_vector(QubitCount::new(1)?)?;
				verify_true!(Environment::builder().build().is_err())?;
				verify_true!(quest_sys::init_custom_quest_env_modes(0, 0, 0).is_err())?;
				verify_true!(quest_sys::is_quest_env_init())?;
				register.x(0)?;
				verify_eq!(register.amplitude(1)?, quest::Complex64::new(1., 0.))?;
			}
			completed_finalization()
		},
	)
}

#[gtest]
fn configuration_rejection_does_not_consume_native_attempt() -> Result<()> {
	isolated(
		"configuration_rejection_does_not_consume_native_attempt",
		|| {
			verify_true!(matches!(
				Environment::builder()
					.distribution(ExecutionMode::Enabled)
					.build(),
				Err(Error::Unsupported(_))
			))?;
			{
				let _environment = Environment::builder().build()?;
				verify_true!(quest_sys::is_quest_env_init())?;
			}
			completed_finalization()
		},
	)
}

#[gtest]
#[expect(
	clippy::panic_in_result_fn,
	reason = "A caught sentinel panic is required to verify cleanup during unwinding"
)]
fn unwinding_destroys_resources_without_a_second_panic() -> Result<()> {
	isolated(
		"unwinding_destroys_resources_without_a_second_panic",
		|| {
			let outcome = std::panic::catch_unwind(|| {
				let environment = Environment::builder().build().unwrap();
				let _register = environment
					.state_vector(QubitCount::new(1).unwrap())
					.unwrap();
				panic!("lifecycle unwind sentinel");
			});
			let payload = outcome.expect_err("the sentinel must unwind");
			verify_eq!(
				payload.downcast_ref::<&str>(),
				Some(&"lifecycle unwind sentinel")
			)?;
			completed_finalization()
		},
	)
}

#[gtest]
fn all_native_preparation_kinds_drop_before_environment() -> Result<()> {
	use quest::{Complex64, MatrixPolicy, NumericalOperator, QuantumRegionBuilder, RunInputs};
	isolated(
		"all_native_preparation_kinds_drop_before_environment",
		|| {
			// These independent Rust payloads are intentionally kept beyond the
			// native environment. Only prepared transfers borrow the owner.
			let dense = NumericalOperator::from_view(
				faer::mat![
					[Complex64::new(0., 0.), Complex64::new(1., 0.)],
					[Complex64::new(1., 0.), Complex64::new(0., 0.)]
				]
				.as_ref(),
				MatrixPolicy::default(),
			)?;
			let diagonal = NumericalOperator::from_view(
				faer::mat![
					[Complex64::new(1., 0.), Complex64::new(0., 0.)],
					[Complex64::new(0., 0.), Complex64::new(-1., 0.)]
				]
				.as_ref(),
				MatrixPolicy::default(),
			)?;
			let snapshot = {
				let environment = Environment::builder().build()?;
				let snapshot = {
					let mut builder = QuantumRegionBuilder::new(1, 0)?;
					let q = builder.qubit(0)?;
					builder.numerical(dense.clone(), &[q], &[])?;
					builder.numerical(diagonal.clone(), &[q], &[])?;
					builder.channel(vec![dense.clone()], &[q], 1e-12)?;
					let mut prepared = environment.prepare(
						quest::Program::from_region(builder.finish()?, &[])?
							.verify()?
							.lower()?
							.plan()?,
					)?;
					let mut structured = environment.prepare(
						(quest::circuit! {qubit q; reset q;}?)
							.verify()?
							.lower()?
							.plan()?,
					)?;
					let _state = environment.state_vector(QubitCount::new(1)?)?;
					let mut density = environment.density_matrix(QubitCount::new(1)?)?;
					// Observe actual native allocations, rather than proving only
					// that preparation objects existed in Rust.
					let blocked = quest_sys::finalize_quest_env().unwrap_err();
					verify_true!(matches!(blocked, quest_sys::QuestError::Lifecycle(_)))?;
					let message = blocked.to_string();
					for resource in ["Qureg=2", "CompMatr=2", "DiagMatr=2", "KrausMap=2"] {
						verify_true!(message.contains(resource))?;
					}
					verify_true!(quest_sys::is_quest_env_init())?;
					prepared.run(&mut density, &quest::RunInputs::default())?;
					structured.run(&mut density, &RunInputs::default())?;
					density.snapshot()?
				};
				verify_eq!(environment.allocated_bytes(), 0)?;
				verify_true!(quest_sys::is_quest_env_init())?;
				snapshot
			};
			completed_finalization()?;
			verify_eq!(snapshot[(0, 0)], Complex64::new(1., 0.))?;
			verify_eq!(dense.view()[(0, 1)], Complex64::new(1., 0.))?;
			verify_eq!(diagonal.view()[(1, 1)], Complex64::new(-1., 0.))?;
			Ok(())
		},
	)
}

#[gtest]
#[expect(
	clippy::panic_in_result_fn,
	reason = "A caught sentinel panic is required to verify terminal cleanup during unwinding"
)]
fn retirement_during_unwinding_preserves_the_original_panic() -> Result<()> {
	isolated(
		"retirement_during_unwinding_preserves_the_original_panic",
		|| {
			let mut orphan = None;
			let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
				let _environment = Environment::builder().build().unwrap();
				orphan = Some(quest_sys::create_qureg(1).unwrap());
				panic!("retirement unwind sentinel");
			}));
			let payload = outcome.expect_err("the sentinel must unwind");
			verify_eq!(
				payload.downcast_ref::<&str>(),
				Some(&"retirement unwind sentinel")
			)?;
			verify_false!(quest_sys::is_quest_env_init())?;
			verify_true!(quest_sys::finalize_quest_env().is_err())?;
			drop(orphan);
			verify_true!(quest_sys::finalize_quest_env().is_err())?;
			Ok(())
		},
	)
}
