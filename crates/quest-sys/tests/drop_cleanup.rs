mod common;

use common::isolated;
use googletest::prelude::*;
use quest_sys::QuestError;

fn verify_retired() -> googletest::Result<()> {
	verify_that!(quest_sys::is_quest_env_init(), eq(false))?;
	verify_that!(
		quest_sys::get_quest_env(),
		err(pat!(QuestError::Lifecycle(_)))
	)?;
	verify_that!(
		quest_sys::get_qu_est_validation_epsilon(),
		err(pat!(QuestError::Lifecycle(_)))
	)?;
	verify_that!(
		quest_sys::create_qureg(1).map(drop),
		err(pat!(QuestError::Lifecycle(_)))
	)?;
	verify_that!(
		quest_sys::init_custom_quest_env(false, false, false),
		err(pat!(QuestError::Lifecycle(_)))
	)?;
	verify_that!(
		quest_sys::finalize_quest_env(),
		err(pat!(QuestError::Lifecycle(_)))
	)
}

#[gtest]
fn drop_cleanup_finalizes_and_remains_idempotent() -> googletest::Result<()> {
	isolated("drop_cleanup_finalizes_and_remains_idempotent", || {
		quest_sys::init_custom_quest_env(false, false, false)?;
		{
			let mut register = quest_sys::create_qureg(1)?;
			quest_sys::init_zero_state(register.pin_mut())?;
		}
		quest_sys::finalize_quest_env_on_drop();
		verify_that!(quest_sys::is_quest_env_init(), eq(false))?;
		// Inactive alone also describes failed retirement. An ordinary finalizer
		// succeeds only when native cleanup actually completed.
		quest_sys::finalize_quest_env()?;
		quest_sys::finalize_quest_env_on_drop();
		quest_sys::finalize_quest_env()?;
		verify_that!(
			quest_sys::init_custom_quest_env(false, false, false),
			err(pat!(QuestError::Lifecycle(_)))
		)
	})
}

#[gtest]
fn drop_cleanup_before_initialization_does_not_consume_attempt() -> googletest::Result<()> {
	isolated(
		"drop_cleanup_before_initialization_does_not_consume_attempt",
		|| {
			quest_sys::finalize_quest_env_on_drop();
			verify_that!(quest_sys::is_quest_env_init(), eq(false))?;
			quest_sys::init_custom_quest_env(false, false, false)?;
			quest_sys::finalize_quest_env_on_drop();
			quest_sys::finalize_quest_env()?;
			Ok(())
		},
	)
}

#[gtest]
fn drop_cleanup_with_live_handle_permanently_retires_runtime() -> googletest::Result<()> {
	isolated(
		"drop_cleanup_with_live_handle_permanently_retires_runtime",
		|| {
			quest_sys::init_custom_quest_env(false, false, false)?;
			let mut orphan = quest_sys::create_qureg(1)?;
			// Ordinary finalization still allows the owner to release handles.
			verify_that!(
				quest_sys::finalize_quest_env(),
				err(pat!(QuestError::Lifecycle(_)))
			)?;
			verify_that!(quest_sys::is_quest_env_init(), eq(true))?;
			quest_sys::init_zero_state(orphan.pin_mut())?;

			quest_sys::finalize_quest_env_on_drop();
			verify_retired()?;
			verify_that!(
				quest_sys::init_zero_state(orphan.pin_mut()),
				err(pat!(QuestError::Lifecycle(_)))
			)?;
			// Its native storage must survive retirement. Rust handle destruction
			// returns normally, but can never reactivate native admission.
			drop(orphan);
			quest_sys::finalize_quest_env_on_drop();
			verify_retired()
		},
	)
}

#[gtest]
fn drop_cleanup_from_another_thread_retires_without_native_finalization() -> googletest::Result<()>
{
	isolated(
		"drop_cleanup_from_another_thread_retires_without_native_finalization",
		|| {
			quest_sys::init_custom_quest_env(false, false, false)?;
			std::thread::spawn(quest_sys::finalize_quest_env_on_drop)
				.join()
				.expect("cleanup must not panic");
			verify_retired()
		},
	)
}
