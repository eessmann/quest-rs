use googletest::prelude::*;
use quest::{Complex64, Environment, ExecutionMode, MemoryBudget, QubitCount};

// Run again with QUEST_REGISTER_TEST_MODE=threads, gpu, or auto. Each Cargo
// invocation gets a fresh process for QuEST's once-only environment lifecycle.
#[gtest]
fn requested_modes_and_native_density_conversion() -> googletest::Result<()> {
    let mode = std::env::var("QUEST_REGISTER_TEST_MODE").unwrap_or_default();
    if mode == "auto" {
        return auto_gpu_deployment_can_differ_between_register_kinds();
    }
    let (gpu, threads) = match mode.as_str() {
        "" => (ExecutionMode::Disabled, ExecutionMode::Disabled),
        "threads" => (ExecutionMode::Disabled, ExecutionMode::Enabled),
        "gpu" => (ExecutionMode::Enabled, ExecutionMode::Disabled),
        other => return fail!("unknown register test mode: {other}"),
    };
    // A two-qubit state vector and density matrix consume 256 + 1024 bytes
    // on CPU, doubled with GPU host/device copies. No export scratch fits.
    let budget = if gpu == ExecutionMode::Enabled {
        2560
    } else {
        1280
    };
    let env = Environment::builder()
        .gpu(gpu)
        .multithreading(threads)
        .memory_budget(MemoryBudget::new(budget))
        .build()
        .or_fail()?;
    let mut state = env.state_vector(QubitCount::new(2).or_fail()?).or_fail()?;
    expect_eq!(
        state.deployment().is_gpu_accelerated(),
        gpu == ExecutionMode::Enabled
    );
    expect_eq!(
        state.deployment().is_multithreaded(),
        threads == ExecutionMode::Enabled
    );
    expect_false!(state.deployment().is_distributed());

    let amplitudes = [
        Complex64::new(0.4, 0.2),
        Complex64::new(-0.1, 0.3),
        Complex64::new(0.2, -0.1),
        Complex64::new(-0.3, -0.2),
    ];
    state.init_pure(&amplitudes).or_fail()?;
    let cloned = state.try_clone().or_fail()?;
    expect_eq!(cloned.deployment(), state.deployment());
    for (index, expected) in amplitudes.iter().enumerate() {
        expect_complex_near(cloned.amplitude(index).or_fail()?, *expected);
    }
    drop(cloned);

    let density = state.to_density().or_fail()?;
    expect_true!(density.deployment().is_density_matrix());
    expect_eq!(
        density.deployment().is_gpu_accelerated(),
        gpu == ExecutionMode::Enabled
    );
    expect_eq!(
        density.deployment().is_multithreaded(),
        threads == ExecutionMode::Enabled
    );
    expect_eq!(env.allocated_bytes(), budget);
    for (row, left) in amplitudes.iter().enumerate() {
        for (column, right) in amplitudes.iter().enumerate() {
            let expected = std::ops::Mul::mul(*left, right.conj());
            expect_complex_near(density.entry(row, column).or_fail()?, expected);
        }
    }
    let before = state.amplitude(1).or_fail()?;
    expect_true!(state.to_density().is_err());
    expect_eq!(env.allocated_bytes(), budget);
    expect_complex_near(state.amplitude(1).or_fail()?, before);
    drop(density);
    let retry = state.to_density().or_fail()?;
    expect_complex_near(retry.entry(1, 2).or_fail()?, Complex64::new(-0.05, 0.05));
    drop(retry);
    drop(state);

    let mut density = env
        .density_matrix(QubitCount::new(1).or_fail()?)
        .or_fail()?;
    density.init_plus().or_fail()?;
    let cloned_density = density.try_clone().or_fail()?;
    expect_eq!(cloned_density.deployment(), density.deployment());
    expect_complex_near(
        cloned_density.entry(0, 1).or_fail()?,
        Complex64::new(0.5, 0.0),
    );
    Ok(())
}

fn auto_gpu_deployment_can_differ_between_register_kinds() -> googletest::Result<()> {
    // On this QuEST build, width six is below the GPU state-vector threshold
    // and reaches the density-matrix threshold. The host/device budget admits
    // both registers exactly, including a CPU-to-GPU native pure-state copy.
    let env = Environment::builder()
        .gpu(ExecutionMode::Auto)
        .memory_budget(MemoryBudget::new(532_480))
        .build()
        .or_fail()?;
    let mut state = env.state_vector(QubitCount::new(6).or_fail()?).or_fail()?;
    expect_false!(state.deployment().is_gpu_accelerated());
    state.init_plus().or_fail()?;
    let density = state.to_density().or_fail()?;
    expect_true!(density.deployment().is_gpu_accelerated());
    expect_eq!(env.allocated_bytes(), 532_480);
    expect_complex_near(
        density.entry(0, 1).or_fail()?,
        Complex64::new(0.015_625, 0.0),
    );
    Ok(())
}

fn expect_complex_near(actual: Complex64, expected: Complex64) {
    expect_that!(actual.re, near(expected.re, 1e-13));
    expect_that!(actual.im, near(expected.im, 1e-13));
}
