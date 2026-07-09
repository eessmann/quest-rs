use googletest::prelude::*;

#[gtest]
fn finalize_succeeds_after_raii_handles_drop() -> googletest::Result<()> {
    quest_sys::init_quest_env()?;

    {
        let _qureg = quest_sys::create_qureg(1)?;
    }

    quest_sys::finalize_quest_env()?;
    verify_that!(quest_sys::is_quest_env_init(), eq(false))?;
    Ok(())
}
