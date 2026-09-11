#![cfg(all(feature = "mpi", quest_native_mpi))]

mod common;

use common::isolated;
use googletest::prelude::*;
use quest_sys::mpi::{MpiQuestEnvironmentBuilder, MpiRuntime};

#[gtest]
fn mpi_survives_quest_drop_and_reinitialization_is_rejected() -> googletest::Result<()> {
    isolated(
        "mpi_survives_quest_drop_and_reinitialization_is_rejected",
        || {
            if !MpiRuntime::is_available() {
                return Ok(());
            }
            let runtime = MpiRuntime::initialize()?;
            verify_that!(MpiRuntime::initialize().is_err(), eq(true))?;
            let mut world = runtime.world()?;
            verify_that!(runtime.thread_multiple(), eq(true))?;
            {
                let _environment = world.quest_environment()?.build()?;
                verify_that!(quest_sys::get_quest_env()?.is_mpi_user_owned, eq(true))?;
                let mut register = quest_sys::create_qureg(1)?;
                quest_sys::init_zero_state(register.pin_mut())?;
            }
            verify_that!(quest_sys::is_quest_env_init(), eq(false))?;
            verify_that!(world.all_agree(true)?, eq(true))?;
            verify_that!(
                world
                    .quest_environment()
                    .and_then(MpiQuestEnvironmentBuilder::build)
                    .is_err(),
                eq(true)
            )?;
            verify_that!(runtime.is_active()?, eq(true))?;
            drop(world);
            drop(runtime);
            verify_that!(MpiRuntime::is_finalized(), eq(true))?;
            verify_that!(MpiRuntime::initialize().is_err(), eq(true))
        },
    )
}

#[gtest]
fn mpi_threaded_views_exchange_and_collectives_agree() -> googletest::Result<()> {
    isolated("mpi_threaded_views_exchange_and_collectives_agree", || {
        if !MpiRuntime::is_available() {
            return Ok(());
        }
        let runtime = MpiRuntime::initialize()?;
        let mut world = runtime.world()?;
        let rank = world.rank()?;
        let view = world.threaded();
        std::thread::scope(|scope| -> googletest::Result<()> {
            let a = scope.spawn(move || {
                let mut received = [0; 3];
                view.send_receive(&[1, 2, 3], rank, 71, &mut received, rank, 72)?;
                Ok::<_, quest_sys::QuestError>(received)
            });
            let b = scope.spawn(move || {
                let mut received = [0; 3];
                view.send_receive(&[4, 5, 6], rank, 72, &mut received, rank, 71)?;
                Ok::<_, quest_sys::QuestError>(received)
            });
            verify_that!(
                a.join().map_err(|_| "worker panicked").or_fail()??,
                eq([4, 5, 6])
            )?;
            verify_that!(
                b.join().map_err(|_| "worker panicked").or_fail()??,
                eq([1, 2, 3])
            )?;
            Ok(())
        })?;
        let mut values = [0.0_f64; 2];
        let status = view.send_receive_typed(&[1.25_f64, -2.5], rank, 73, &mut values, rank, 73)?;
        verify_that!(values, eq([1.25, -2.5]))?;
        verify_that!(status.count, eq(2))?;
        verify_that!(status.source_rank, eq(rank))?;
        verify_that!(status.tag, eq(73))?;
        let mut bytes = [1, 2, 3];
        world.broadcast_bytes(0, &mut bytes)?;
        verify_that!(world.all_agree(false)?, eq(false))?;
        verify_that!(world.split_power_of_two(3).is_err(), eq(true))?;
        let subgroup = world.split_power_of_two(1)?;
        verify_that!(subgroup.size()?, eq(1))?;
        verify_that!(subgroup.rank()?, eq(0))
    })
}

fn mpi_ranks(
    name: &str,
    ranks: &str,
    body: impl FnOnce() -> googletest::Result<()>,
) -> googletest::Result<()> {
    if !MpiRuntime::is_available() {
        return Ok(());
    }
    if std::env::var("QUEST_SYS_MPI_CASE").as_deref() == Ok(name) {
        return body();
    }
    let output = std::process::Command::new("timeout")
        .args(["60s", "mpiexec", "-n", ranks])
        .arg(std::env::current_exe().or_fail()?)
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("QUEST_SYS_MPI_CASE", name)
        .output()
        .or_fail()?;
    if !output.status.success() {
        return fail!(
            "MPI child failed: {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[gtest]
fn mpi_subgroups_and_excluded_ranks_preserve_contexts() -> googletest::Result<()> {
    mpi_ranks(
        "mpi_subgroups_and_excluded_ranks_preserve_contexts",
        "4",
        || {
            let runtime = MpiRuntime::initialize()?;
            let mut world = runtime.world()?;
            let rank = world.rank()?;
            verify_that!(world.size()?, eq(4))?;
            verify_that!(world.split(Some(-1), rank).is_err(), eq(true))?;
            verify_that!(
                world
                    .split_power_of_two(if rank == 0 { 1 } else { 2 })
                    .is_err(),
                eq(true)
            )?;
            let odd = world.split((rank < 3).then_some(0), rank)?;
            if let Some(comm) = odd {
                verify_that!(comm.size()?, eq(3))?;
                // Invalid subgroup size must not consume native initialization.
                verify_that!(
                    comm.quest_environment()
                        .and_then(MpiQuestEnvironmentBuilder::build)
                        .is_err(),
                    eq(true)
                )?;
            }
            verify_that!(world.all_agree(true)?, eq(true))?;
            let mut subgroup = world.split_power_of_two(2)?;
            let duplicate = subgroup.duplicate()?;
            verify_that!(duplicate.size()?, eq(2))?;
            let env = subgroup.quest_environment()?.build()?;
            verify_that!(quest_sys::get_quest_env()?.num_nodes, eq(2))?;
            verify_that!(quest_sys::get_quest_env()?.rank, eq(subgroup.rank()?))?;
            {
                let mut register = quest_sys::create_qureg(3)?;
                quest_sys::init_plus_state(register.pin_mut())?;
                verify_that!(quest_sys::calc_total_prob(&register)?, near(1.0, 1e-12))?;
                let mut lane = subgroup.collective_lane()?;
                verify_that!(subgroup.collective_lane().is_err(), eq(true))?;
                let mut bytes = if subgroup.rank()? == 0 {
                    [17, 4]
                } else {
                    [0, 0]
                };
                lane.broadcast_bytes(0, &mut bytes)?;
                verify_that!(bytes, eq([17, 4]))?;
                verify_that!(lane.all_agree(true)?, eq(true))?;
                let view = subgroup.threaded();
                let peer = 1_i32.checked_sub(subgroup.rank()?).or_fail()?;
                std::thread::scope(|scope| -> googletest::Result<()> {
                    let worker = scope.spawn(move || {
                        let mut received = [0; 2];
                        view.send_receive(&[9, 8], peer, 31, &mut received, peer, 31)?;
                        Ok::<_, quest_sys::QuestError>(received)
                    });
                    // Native operations can proceed while the independent message
                    // context is active; no lifecycle mutex protects application MPI.
                    quest_sys::init_zero_state(register.pin_mut())?;
                    verify_that!(
                        worker.join().map_err(|_| "worker panicked").or_fail()??,
                        eq([9, 8])
                    )?;
                    Ok(())
                })?;
            }
            drop(env);
            verify_that!(runtime.is_active()?, eq(true))?;
            let mut lane = subgroup.collective_lane()?;
            verify_that!(lane.all_agree(true)?, eq(true))?;
            verify_that!(
                subgroup
                    .quest_environment()
                    .and_then(MpiQuestEnvironmentBuilder::build)
                    .is_err(),
                eq(true)
            )
        },
    )
}

#[gtest]
fn mpi_collective_preflight_rejects_mismatch_on_every_rank() -> googletest::Result<()> {
    mpi_ranks(
        "mpi_collective_preflight_rejects_mismatch_on_every_rank",
        "2",
        || {
            let runtime = MpiRuntime::initialize()?;
            let mut world = runtime.world()?;
            let rank = world.rank()?;
            let mut bytes = [0; 2];
            verify_that!(world.broadcast_bytes(rank, &mut bytes).is_err(), eq(true))?;
            let length = usize::try_from(rank).or_fail()?;
            let slice = bytes.get_mut(..length).or_fail()?;
            verify_that!(world.broadcast_bytes(0, slice).is_err(), eq(true))?;
            verify_that!(world.broadcast_bytes(-1, &mut bytes).is_err(), eq(true))?;
            verify_that!(world.all_agree(rank == 0)?, eq(false))?;
            verify_that!(world.all_agree(true)?, eq(true))
        },
    )
}

#[gtest]
fn mpi_distributed_drop_with_live_resource_aborts_job() -> googletest::Result<()> {
    let name = "mpi_distributed_drop_with_live_resource_aborts_job";
    if !MpiRuntime::is_available() {
        return Ok(());
    }
    if std::env::var("QUEST_SYS_MPI_CASE").as_deref() == Ok(name) {
        let runtime = MpiRuntime::initialize()?;
        let world = runtime.world()?;
        let environment = world.quest_environment()?.build()?;
        let _live = quest_sys::create_qureg(3)?;
        drop(environment);
        return fail!("invalid distributed destruction order returned");
    }
    let output = std::process::Command::new("timeout")
        .args(["60s", "mpiexec", "-n", "2"])
        .arg(std::env::current_exe().or_fail()?)
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("QUEST_SYS_MPI_CASE", name)
        .output()
        .or_fail()?;
    verify_that!(output.status.success(), eq(false))?;
    verify_that!(output.status.code(), not(eq(Some(124))))?;
    verify_that!(
        String::from_utf8_lossy(&output.stderr).as_ref(),
        contains_substring("unrecoverable distributed MPI operation or cleanup failure")
    )
}
