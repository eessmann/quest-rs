#![cfg(not(target_os = "linux"))]

use googletest::prelude::*;
use quest_optimizer_client::{Client, Error, WorkerLimits};

#[gtest]
fn non_linux_rejects_worker_processes_before_executable_lookup() -> googletest::Result<()> {
    verify_that!(
        Client::new("/nonexistent-worker", WorkerLimits::default()),
        err(pat!(Error::Capability(_)))
    )
}
