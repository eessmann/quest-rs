mod common;
use common::isolated;

use googletest::prelude::*;
use quest_sys::{QuestComplex, QuestError};

const fn complex(re: f64, im: f64) -> QuestComplex {
	QuestComplex { re, im }
}

#[gtest]
fn generated_global_calls_reject_another_thread() -> googletest::Result<()> {
	isolated("generated_global_calls_reject_another_thread", || {
		quest_sys::init_custom_quest_env(false, false, false)?;
		let result = std::thread::spawn(|| {
			(
				quest_sys::get_qu_est_validation_epsilon(),
				quest_sys::set_qu_est_validation_epsilon(1e-9),
				quest_sys::get_quest_env(),
				quest_sys::create_qureg(1).map(drop),
			)
		})
		.join()
		.expect("worker should not panic");
		expect_that!(result.0, err(pat!(QuestError::Lifecycle(_))));
		expect_that!(result.1, err(pat!(QuestError::Lifecycle(_))));
		expect_that!(result.2, err(pat!(QuestError::Lifecycle(_))));
		expect_that!(result.3, err(pat!(QuestError::Lifecycle(_))));
		quest_sys::finalize_quest_env()?;
		Ok(())
	})
}

#[gtest]
fn density_pure_initialization_uses_statevector_dimension() -> googletest::Result<()> {
	isolated(
		"density_pure_initialization_uses_statevector_dimension",
		|| {
			quest_sys::init_custom_quest_env(false, false, false)?;
			let mut density = quest_sys::create_density_qureg(1)?;
			let a = std::f64::consts::FRAC_1_SQRT_2;
			quest_sys::init_arbitrary_pure_state(
				density.pin_mut(),
				&[complex(a, 0.0), complex(0.0, a)],
			)?;
			let upper = quest_sys::get_density_qureg_amp(&density, 0, 1)?;
			let lower = quest_sys::get_density_qureg_amp(&density, 1, 0)?;
			expect_that!(upper.re, near(0.0, 1e-12));
			expect_that!(upper.im, near(-0.5, 1e-12));
			expect_that!(lower.im, near(0.5, 1e-12));
			expect_that!(
				quest_sys::init_arbitrary_pure_state(density.pin_mut(), &[complex(0.0, 0.0); 4]),
				err(anything())
			);
			drop(density);
			quest_sys::finalize_quest_env()?;
			Ok(())
		},
	)
}

#[gtest]
fn calls_before_initialization_return_lifecycle_errors() -> googletest::Result<()> {
	isolated(
		"calls_before_initialization_return_lifecycle_errors",
		|| {
			verify_that!(quest_sys::is_quest_env_init(), eq(false))?;
			verify_that!(
				quest_sys::get_quest_env(),
				err(pat!(QuestError::Lifecycle(_)))
			)?;
			verify_that!(
				quest_sys::get_qu_est_validation_epsilon(),
				err(pat!(QuestError::Lifecycle(_)))
			)?;
			let result = quest_sys::create_qureg(1).map(drop);
			verify_that!(result, err(pat!(QuestError::Lifecycle(_))))?;
			Ok(())
		},
	)
}

#[gtest]
fn duplicate_initialization_and_restart_are_lifecycle_errors() -> googletest::Result<()> {
	isolated(
		"duplicate_initialization_and_restart_are_lifecycle_errors",
		|| {
			quest_sys::init_custom_quest_env(false, false, false)?;
			verify_that!(
				quest_sys::init_custom_quest_env(false, false, false),
				err(pat!(QuestError::Lifecycle(_)))
			)?;
			quest_sys::finalize_quest_env()?;
			verify_that!(quest_sys::is_quest_env_init(), eq(false))?;
			verify_that!(
				quest_sys::init_quest_env(),
				err(pat!(QuestError::Lifecycle(_)))
			)?;
			Ok(())
		},
	)
}

#[gtest]
fn another_thread_cannot_finalize_an_active_environment() -> googletest::Result<()> {
	isolated(
		"another_thread_cannot_finalize_an_active_environment",
		|| {
			quest_sys::init_custom_quest_env(false, false, false)?;
			let result = std::thread::spawn(quest_sys::finalize_quest_env)
				.join()
				.expect("worker should not panic");
			expect_that!(result, err(pat!(QuestError::Lifecycle(_))));
			expect_that!(quest_sys::is_quest_env_init(), eq(true));
			quest_sys::finalize_quest_env()?;
			Ok(())
		},
	)
}

#[gtest]
fn validation_tolerance_cannot_disable_numerical_checks() -> googletest::Result<()> {
	isolated(
		"validation_tolerance_cannot_disable_numerical_checks",
		|| {
			quest_sys::init_custom_quest_env(false, false, false)?;
			let original = quest_sys::get_qu_est_validation_epsilon()?;
			for epsilon in [0.0, -1.0, f64::NAN, f64::INFINITY] {
				expect_that!(
					quest_sys::set_qu_est_validation_epsilon(epsilon),
					err(anything())
				);
			}
			expect_that!(quest_sys::get_qu_est_validation_epsilon()?, eq(original));
			quest_sys::finalize_quest_env()?;
			Ok(())
		},
	)
}

#[gtest]
fn concurrent_initializers_admit_exactly_one_owner() -> googletest::Result<()> {
	isolated("concurrent_initializers_admit_exactly_one_owner", || {
		let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
		let mut threads = Vec::with_capacity(4);
		for _ in 0..4 {
			let barrier = barrier.clone();
			threads.push(std::thread::spawn(move || {
				barrier.wait();
				match quest_sys::init_custom_quest_env(false, false, false) {
					Ok(()) => {
						let register = quest_sys::create_qureg(1)
							.map_err(|error| format!("winning thread cannot create: {error}"))?;
						drop(register);
						quest_sys::finalize_quest_env()
							.map_err(|error| format!("winning thread cannot finalize: {error}"))?;
						Ok(true)
					}
					Err(QuestError::Lifecycle(_)) => Ok(false),
					Err(error) => Err(format!("unexpected initialization failure: {error}")),
				}
			}));
		}
		let mut owners = 0_usize;
		for thread in threads {
			let owns_environment = thread
				.join()
				.map_err(|_| "worker should not panic")
				.or_fail()?
				.or_fail()?;
			owners = owners
				.checked_add(usize::from(owns_environment))
				.or_fail()?;
		}
		verify_that!(owners, eq(1))?;
		verify_that!(quest_sys::is_quest_env_init(), eq(false))
	})
}

#[gtest]
fn owner_identity_is_not_reused_after_its_thread_exits() -> googletest::Result<()> {
	isolated(
		"owner_identity_is_not_reused_after_its_thread_exits",
		|| {
			std::thread::spawn(|| quest_sys::init_custom_quest_env(false, false, false))
				.join()
				.expect("initialization thread")?;
			for _ in 0..8 {
				let result = std::thread::spawn(quest_sys::get_quest_env)
					.join()
					.expect("later thread");
				expect_that!(result, err(pat!(QuestError::Lifecycle(_))));
			}
			// The original thread is gone. Preserve the environment until process exit.
			verify_that!(
				quest_sys::finalize_quest_env(),
				err(pat!(QuestError::Lifecycle(_)))
			)
		},
	)
}

#[gtest]
fn custom_modes_accept_auto_and_reject_invalid_flags_before_native_init() -> googletest::Result<()>
{
	isolated(
		"custom_modes_accept_auto_and_reject_invalid_flags_before_native_init",
		|| {
			for mode in [-2, 2] {
				expect_that!(
					quest_sys::init_custom_quest_env_modes(mode, 0, 0),
					err(anything())
				);
			}
			quest_sys::init_custom_quest_env_modes(-1, -1, -1)?;
			expect_that!(quest_sys::is_quest_env_init(), eq(true));
			quest_sys::finalize_quest_env()?;
			Ok(())
		},
	)
}
