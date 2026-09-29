use googletest::prelude::*;
use quest::{
    Complex64, Control, ControlState, Environment, Error, Gate, MatrixPolicy, MemoryBudget,
    NumericalOperator, ProgramBuilder, QubitCount,
};

fn isolated(name: &str, body: impl FnOnce() -> googletest::Result<()>) -> googletest::Result<()> {
    if std::env::var("QUEST_SHARED_MATRIX_TEST").as_deref() == Ok(name) {
        return body();
    }
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("QUEST_SHARED_MATRIX_TEST", name)
        .status()?;
    expect_true!(status.success());
    Ok(())
}

fn dense_swap() -> quest::Result<NumericalOperator> {
    let matrix = faer::Mat::from_fn(32, 32, |row, col| {
        let destination = match col {
            0 => 1,
            1 => 0,
            _ => col,
        };
        Complex64::new(f64::from(row == destination), 0.0)
    });
    Ok(NumericalOperator::from_view(
        matrix.as_ref(),
        MatrixPolicy::default(),
    )?)
}

#[gtest]
fn cloned_matrices_on_different_targets_share_one_native_budget() -> googletest::Result<()> {
    isolated(
        "cloned_matrices_on_different_targets_share_one_native_budget",
        || {
            let environment = Environment::builder()
                .memory_budget(MemoryBudget::new(300_000))
                .build()?;
            let matrix = dense_swap()?;
            let mut builder = ProgramBuilder::new(6, 1)?;
            let first = (0..5)
                .map(|i| builder.qubit(i))
                .collect::<Result<Vec<_>, _>>()?;
            let second = (1..6)
                .map(|i| builder.qubit(i))
                .collect::<Result<Vec<_>, _>>()?;
            for targets in [&first, &first, &second, &second] {
                builder.numerical(matrix.clone(), targets, &[])?;
            }
            // Conditional wrappers still retain their own instruction storage.
            let bit = builder.bit(0)?;
            let qubit = builder.qubit(5)?;
            for _ in 0..2 {
                builder.gate_if(bit, false, Gate::X, &[qubit], &[])?;
            }
            let mut prepared = environment.prepare(builder.finish()?)?;
            let mut register = environment.state_vector(QubitCount::new(6)?)?;
            prepared.run(&mut register)?;
            expect_that!(register.amplitude(0)?, eq(Complex64::new(1.0, 0.0)));
            drop(register);
            drop(prepared);
            expect_eq!(environment.allocated_bytes(), 0);
            Ok(())
        },
    )
}

#[gtest]
fn matching_signed_controls_share_one_native_pair_across_target_orders() -> googletest::Result<()> {
    isolated(
        "matching_signed_controls_share_one_native_pair_across_target_orders",
        || {
            let environment = Environment::builder()
                .memory_budget(MemoryBudget::new(1_000_000))
                .build()?;
            let matrix = dense_swap()?;
            let mut builder = ProgramBuilder::new(6, 0)?;
            let first = (0..5)
                .map(|i| builder.qubit(i))
                .collect::<Result<Vec<_>, _>>()?;
            let second = [1, 0, 2, 3, 4]
                .into_iter()
                .map(|i| builder.qubit(i))
                .collect::<Result<Vec<_>, _>>()?;
            let control = Control::new(builder.qubit(5)?, ControlState::One);
            for targets in [&first, &first, &second, &second] {
                builder.numerical(matrix.clone(), targets, &[control])?;
            }
            let mut prepared = environment.prepare(builder.finish()?)?;
            let mut register = environment.state_vector(QubitCount::new(6)?)?;
            register.x(5)?;
            prepared.run(&mut register)?;
            expect_that!(register.amplitude(32)?, eq(Complex64::new(1.0, 0.0)));
            drop(register);
            drop(prepared);
            expect_eq!(environment.allocated_bytes(), 0);
            Ok(())
        },
    )
}

#[gtest]
fn different_signed_profiles_exceed_budget_without_disturbing_prepared_program()
-> googletest::Result<()> {
    isolated(
        "different_signed_profiles_exceed_budget_without_disturbing_prepared_program",
        || {
            let environment = Environment::builder()
                .memory_budget(MemoryBudget::new(1_200_000))
                .build()?;
            let mut existing = ProgramBuilder::new(6, 0)?;
            existing.gate(Gate::H, &[existing.qubit(0)?], &[])?;
            let mut existing = environment.prepare(existing.finish()?)?;
            let before = environment.allocated_bytes();

            let matrix = dense_swap()?;
            let mut builder = ProgramBuilder::new(6, 0)?;
            let targets = (0..5)
                .map(|i| builder.qubit(i))
                .collect::<Result<Vec<_>, _>>()?;
            let control = builder.qubit(5)?;
            for state in [ControlState::Zero, ControlState::One] {
                builder.numerical(matrix.clone(), &targets, &[Control::new(control, state)])?;
            }
            expect_true!(matches!(
                environment.prepare(builder.finish()?),
                Err(Error::Budget { .. })
            ));
            expect_eq!(environment.allocated_bytes(), before);

            let mut register = environment.state_vector(QubitCount::new(6)?)?;
            existing.run(&mut register)?;
            expect_that!(register.amplitude(0)?.re, near(2f64.sqrt().recip(), 1e-12));
            expect_that!(register.amplitude(1)?.re, near(2f64.sqrt().recip(), 1e-12));
            drop(register);
            drop(existing);
            expect_eq!(environment.allocated_bytes(), 0);
            Ok(())
        },
    )
}
