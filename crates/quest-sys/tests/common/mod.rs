use googletest::prelude::*;

/// Keep native lifecycle tests independent under both Cargo and Nextest.
pub fn isolated(
    name: &str,
    body: impl FnOnce() -> googletest::Result<()>,
) -> googletest::Result<()> {
    if std::env::var("QUEST_SYS_TEST_CASE").as_deref() == Ok(name) {
        return body();
    }
    let output = std::process::Command::new(std::env::current_exe().or_fail()?)
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("QUEST_SYS_TEST_CASE", name)
        .output()
        .or_fail()?;
    if !output.status.success() {
        return fail!(
            "native test child failed: {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

// Shared by the legacy adapter suite; other binaries initialize explicitly to
// exercise initialization/finalization failures and thread ownership.
#[allow(dead_code)]
pub fn isolated_with_environment(
    name: &str,
    body: impl FnOnce() -> googletest::Result<()>,
) -> googletest::Result<()> {
    isolated(name, || {
        quest_sys::init_quest_env()?;
        let result = body();
        // The closure's native handles have dropped on this same owner thread,
        // including when the body returned an error.
        let finalized = quest_sys::finalize_quest_env();
        result?;
        finalized?;
        Ok(())
    })
}
