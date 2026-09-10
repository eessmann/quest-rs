use googletest::prelude::*;
use quest::{Complex64, Environment, MemoryBudget, QubitCount};

fn isolated(name: &str, body: impl FnOnce() -> googletest::Result<()>) -> googletest::Result<()> {
    if std::env::var("QUEST_RUNTIME_TEST").as_deref() == Ok(name) {
        return body();
    }
    let status = std::process::Command::new(std::env::current_exe().or_fail()?)
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("QUEST_RUNTIME_TEST", name)
        .status()
        .or_fail()?;
    expect_true!(status.success());
    Ok(())
}

#[gtest]
fn bell_state_and_snapshot_survive_teardown() -> googletest::Result<()> {
    isolated("bell_state_and_snapshot_survive_teardown", || {
        let env = Environment::builder().build().or_fail()?;
        let mut register = env.state_vector(QubitCount::new(2).or_fail()?).or_fail()?;
        register.h(0).or_fail()?;
        register.cx(0, 1).or_fail()?;
        let snapshot = register.snapshot().or_fail()?;
        expect_that!(snapshot[(0, 0)].re, near(2f64.sqrt().recip(), 1e-14));
        expect_that!(snapshot[(3, 0)].re, near(2f64.sqrt().recip(), 1e-14));
        expect_that!(snapshot[(1, 0)], eq(Complex64::new(0., 0.)));
        drop(register);
        env.close().map_err(|e| e.to_string()).or_fail()?;
        expect_that!(snapshot.nrows(), eq(4));
        Ok(())
    })
}

#[gtest]
fn dimension_and_budget_reject_before_native_allocation() -> googletest::Result<()> {
    isolated(
        "dimension_and_budget_reject_before_native_allocation",
        || {
            expect_true!(QubitCount::new(usize::MAX).is_err());
            let env = Environment::builder()
                .memory_budget(MemoryBudget::new(256))
                .build()
                .or_fail()?;
            expect_true!(env.state_vector(QubitCount::new(10).or_fail()?).is_err());
            expect_that!(env.allocated_bytes(), eq(0));
            env.close().map_err(|e| e.to_string()).or_fail()?;
            Ok(())
        },
    )
}

#[gtest]
fn density_imaginary_entries_and_rectangular_blocks_keep_layout() -> googletest::Result<()> {
    isolated(
        "density_imaginary_entries_and_rectangular_blocks_keep_layout",
        || {
            let env = Environment::builder().build().or_fail()?;
            let mut density = env
                .density_matrix(QubitCount::new(2).or_fail()?)
                .or_fail()?;
            let block = faer::Mat::from_fn(2, 3, |r, c| {
                Complex64::new((r + 3 * c) as f64, (r as f64) - (c as f64))
            });
            density.write_block(1, 0, block.as_ref()).or_fail()?;
            let snapshot = density.snapshot().or_fail()?;
            for r in 0..2 {
                for c in 0..3 {
                    expect_that!(snapshot[(r + 1, c)], eq(block[(r, c)]));
                }
            }
            let tall = faer::Mat::from_fn(3, 1, |r, _| Complex64::new(r as f64, 0.25));
            density.write_block(0, 3, tall.as_ref()).or_fail()?;
            expect_that!(density.entry(2, 3).or_fail()?, eq(tall[(2, 0)]));
            drop(density);
            env.close().map_err(|e| e.to_string()).or_fail()?;
            Ok(())
        },
    )
}

#[gtest]
fn builder_execution_samples_each_shot_from_zero() -> googletest::Result<()> {
    isolated("builder_execution_samples_each_shot_from_zero", || {
        use quest::{Gate, ProgramBuilder, Shots};
        let mut builder = ProgramBuilder::new(1, 1).or_fail()?;
        let q = builder.qubit(0).or_fail()?;
        let c = builder.bit(0).or_fail()?;
        builder.gate(Gate::X, &[q], &[]).or_fail()?;
        builder.measure(q, c).or_fail()?;
        let env = Environment::builder().build().or_fail()?;
        let mut prepared = env.prepare(builder.finish().or_fail()?).or_fail()?;
        let samples = prepared
            .sample_zeroed(Shots::new(16).or_fail()?, &[17, 23])
            .or_fail()?;
        expect_that!(samples.counts.get(&vec![true]), some(eq(&16)));
        drop(prepared);
        env.close().map_err(|e| e.to_string()).or_fail()?;
        Ok(())
    })
}

#[gtest]
fn nonsorted_numerical_targets_preserve_complex_operator_order() -> googletest::Result<()> {
    isolated(
        "nonsorted_numerical_targets_preserve_complex_operator_order",
        || {
            use quest::{MatrixPolicy, NumericalOperator, ProgramBuilder};
            let env = Environment::builder().build().or_fail()?;
            let input: Vec<_> = (0..8)
                .map(|i| Complex64::new((i + 1) as f64, (7 - i) as f64))
                .collect();
            let norm = input.iter().map(|v| v.norm_sqr()).sum::<f64>().sqrt();
            let input: Vec<_> = input.into_iter().map(|v| v / norm).collect();
            let mut matrix = faer::Mat::from_fn(4, 4, |r, c| {
                Complex64::new((1 + r + 2 * c) as f64 / 10., (r as f64 - c as f64) / 7.)
            });
            let operator =
                NumericalOperator::from_view(matrix.as_ref(), MatrixPolicy::default()).or_fail()?;
            let mut builder = ProgramBuilder::new(3, 0).or_fail()?;
            let targets = [builder.qubit(2).or_fail()?, builder.qubit(0).or_fail()?];
            builder.numerical(operator, &targets, &[]).or_fail()?;
            let mut prepared = env.prepare(builder.finish().or_fail()?).or_fail()?;
            let mut register = env.state_vector(QubitCount::new(3).or_fail()?).or_fail()?;
            register.init_pure(&input).or_fail()?;
            let expected: Vec<_> = (0..8)
                .map(|out| {
                    let row = ((out >> 2) & 1) | ((out & 1) << 1);
                    (0..4)
                        .map(|col| {
                            let i = (out & 2) | ((col & 1) << 2) | ((col >> 1) & 1);
                            matrix[(row, col)] * input[i]
                        })
                        .sum::<Complex64>()
                })
                .collect();
            matrix.fill(Complex64::new(0., 0.));
            prepared.run(&mut register).or_fail()?;
            let snapshot = register.snapshot().or_fail()?;
            for (i, expect) in expected.into_iter().enumerate() {
                expect_that!((snapshot[(i, 0)] - expect).norm(), le(1e-12));
            }
            drop(register);
            drop(prepared);
            env.close().map_err(|e| e.to_string()).or_fail()?;
            Ok(())
        },
    )
}

#[gtest]
fn numerical_density_evolution_uses_adjoint_and_signed_control() -> googletest::Result<()> {
    isolated(
        "numerical_density_evolution_uses_adjoint_and_signed_control",
        || {
            use quest::{Control, ControlState, MatrixPolicy, NumericalOperator, ProgramBuilder};
            let env = Environment::builder().build().or_fail()?;
            let mut density = env
                .density_matrix(QubitCount::new(2).or_fail()?)
                .or_fail()?;
            let input = [
                Complex64::new(0.5, 0.),
                Complex64::new(0., 0.5),
                Complex64::new(-0.5, 0.),
                Complex64::new(0., -0.5),
            ];
            density.init_pure(&input).or_fail()?;
            let values = [
                [Complex64::new(0.2, 0.3), Complex64::new(0.5, -0.7)],
                [Complex64::new(-0.1, 0.4), Complex64::new(0.9, 0.2)],
            ];
            let matrix = faer::Mat::from_fn(2, 2, |r, c| values[r][c]);
            let mut builder = ProgramBuilder::new(2, 0).or_fail()?;
            let target = builder.qubit(1).or_fail()?;
            let control = builder.qubit(0).or_fail()?;
            builder
                .numerical(
                    NumericalOperator::from_view(matrix.as_ref(), MatrixPolicy::default())
                        .or_fail()?,
                    &[target],
                    &[Control::new(control, ControlState::Zero)],
                )
                .or_fail()?;
            let mut prepared = env.prepare(builder.finish().or_fail()?).or_fail()?;
            prepared.run(&mut density).or_fail()?;
            let evolved: Vec<_> = (0..4)
                .map(|i| {
                    if i & 1 == 1 {
                        input[i]
                    } else {
                        (0..2).map(|j| values[i >> 1][j] * input[j << 1]).sum()
                    }
                })
                .collect();
            let snapshot = density.snapshot().or_fail()?;
            for r in 0..4 {
                for c in 0..4 {
                    expect_that!(
                        (snapshot[(r, c)] - evolved[r] * evolved[c].conj()).norm(),
                        le(1e-12)
                    );
                }
            }
            drop(density);
            drop(prepared);
            env.close().map_err(|e| e.to_string()).or_fail()?;
            Ok(())
        },
    )
}

#[gtest]
fn density_channel_reset_and_classical_branch_are_effectful() -> googletest::Result<()> {
    isolated(
        "density_channel_reset_and_classical_branch_are_effectful",
        || {
            use quest::{Gate, MatrixPolicy, NumericalOperator, ProgramBuilder};
            let env = Environment::builder().build().or_fail()?;
            let mut density = env
                .density_matrix(QubitCount::new(1).or_fail()?)
                .or_fail()?;
            let k0 = faer::Mat::from_fn(2, 2, |r, c| {
                Complex64::new(
                    if r == c {
                        if r == 0 { 1. } else { 0.75f64.sqrt() }
                    } else {
                        0.
                    },
                    0.,
                )
            });
            let k1 = faer::Mat::from_fn(2, 2, |r, c| {
                Complex64::new(if r == 0 && c == 1 { 0.5 } else { 0. }, 0.)
            });
            let mut builder = ProgramBuilder::new(1, 0).or_fail()?;
            let q = builder.qubit(0).or_fail()?;
            builder.gate(Gate::X, &[q], &[]).or_fail()?;
            builder
                .channel(
                    vec![
                        NumericalOperator::from_view(k0.as_ref(), MatrixPolicy::default())
                            .or_fail()?,
                        NumericalOperator::from_view(k1.as_ref(), MatrixPolicy::default())
                            .or_fail()?,
                    ],
                    &[q],
                    1e-12,
                )
                .or_fail()?;
            let mut prepared = env.prepare(builder.finish().or_fail()?).or_fail()?;
            prepared.run(&mut density).or_fail()?;
            expect_that!(density.entry(0, 0).or_fail()?.re, near(0.25, 1e-12));
            expect_that!(density.entry(1, 1).or_fail()?.re, near(0.75, 1e-12));
            drop(prepared);
            let mut reset = ProgramBuilder::new(1, 1).or_fail()?;
            let q = reset.qubit(0).or_fail()?;
            let bit = reset.bit(0).or_fail()?;
            reset.reset(q).or_fail()?;
            reset.measure(q, bit).or_fail()?;
            reset.gate_if(bit, false, Gate::X, &[q], &[]).or_fail()?;
            let mut prepared = env.prepare(reset.finish().or_fail()?).or_fail()?;
            let result = prepared.run(&mut density).or_fail()?;
            expect_false!(result.bits[0]);
            expect_that!(density.entry(1, 1).or_fail()?.re, near(1., 1e-12));
            drop(density);
            drop(prepared);
            env.close().map_err(|e| e.to_string()).or_fail()?;
            Ok(())
        },
    )
}

#[gtest]
fn controlled_rotation_phase_matches_fused_and_unfused_execution() -> googletest::Result<()> {
    isolated(
        "controlled_rotation_phase_matches_fused_and_unfused_execution",
        || {
            use quest::{Angle, Control, ControlState, FusionOptions, Gate, ProgramBuilder};
            let env = Environment::builder().build().or_fail()?;
            let mut builder = ProgramBuilder::new(2, 0).or_fail()?;
            let q = builder.qubit(1).or_fail()?;
            let c = builder.qubit(0).or_fail()?;
            let controls = [Control::new(c, ControlState::One)];
            builder
                .gate(Gate::Rz(Angle::pi(2, 1).or_fail()?), &[q], &controls)
                .or_fail()?;
            builder.gate(Gate::Sx, &[q], &controls).or_fail()?;
            let validated = builder.finish().or_fail()?;
            let (optimized, _) = validated.clone().optimize_exact().or_fail()?;
            let (fused, _) = optimized
                .bind(&[])
                .or_fail()?
                .fuse(FusionOptions::default())
                .or_fail()?;
            let mut direct = env.prepare(validated).or_fail()?;
            let mut optimized = env
                .prepare_plan(fused.lower().or_fail()?.plan().or_fail()?)
                .or_fail()?;
            let mut left = env.state_vector(QubitCount::new(2).or_fail()?).or_fail()?;
            left.init_plus().or_fail()?;
            let mut right = left.try_clone().or_fail()?;
            direct.run(&mut left).or_fail()?;
            optimized.run(&mut right).or_fail()?;
            for i in 0..4 {
                expect_that!(
                    (left.amplitude(i).or_fail()? - right.amplitude(i).or_fail()?).norm(),
                    le(1e-12)
                );
            }
            // Sx leaves |+> invariant; controlled Rz(2pi) changes only the control-one branch's phase.
            expect_that!(right.amplitude(0).or_fail()?.re, near(0.5, 1e-12));
            expect_that!(right.amplitude(1).or_fail()?.re, near(-0.5, 1e-12));
            drop(left);
            drop(right);
            drop(direct);
            drop(optimized);
            env.close().map_err(|e| e.to_string()).or_fail()?;
            Ok(())
        },
    )
}

#[gtest]
fn execution_failure_reports_completed_prefix_and_keeps_partial_state() -> googletest::Result<()> {
    isolated(
        "execution_failure_reports_completed_prefix_and_keeps_partial_state",
        || {
            use quest::{Error, MatrixPolicy, NumericalOperator, ProgramBuilder};
            let env = Environment::builder().build().or_fail()?;
            let matrix = faer::Mat::<Complex64>::zeros(2, 2);
            let mut builder = ProgramBuilder::new(1, 1).or_fail()?;
            let q = builder.qubit(0).or_fail()?;
            let c = builder.bit(0).or_fail()?;
            builder
                .numerical(
                    NumericalOperator::from_view(matrix.as_ref(), MatrixPolicy::default())
                        .or_fail()?,
                    &[q],
                    &[],
                )
                .or_fail()?;
            builder.measure(q, c).or_fail()?;
            let mut prepared = env.prepare(builder.finish().or_fail()?).or_fail()?;
            let mut register = env.state_vector(QubitCount::new(1).or_fail()?).or_fail()?;
            let error = prepared
                .run(&mut register)
                .expect_err("zero-probability measurement must fail");
            expect_true!(matches!(
                error,
                Error::Execution {
                    instruction: 1,
                    completed: 1,
                    ..
                }
            ));
            expect_that!(register.total_probability().or_fail()?, eq(0.));
            drop(register);
            drop(prepared);
            expect_that!(env.allocated_bytes(), eq(0));
            env.close().map_err(|e| e.to_string()).or_fail()?;
            Ok(())
        },
    )
}

#[gtest]
fn scalar_numerical_operator_and_preparation_budget_have_checked_admission()
-> googletest::Result<()> {
    isolated(
        "scalar_numerical_operator_and_preparation_budget_have_checked_admission",
        || {
            use quest::{MatrixPolicy, NumericalOperator, ProgramBuilder};
            let env = Environment::builder()
                .memory_budget(MemoryBudget::new(16384))
                .build()
                .or_fail()?;
            let scalar = Complex64::new(0.25, -0.5);
            let matrix = faer::Mat::from_fn(1, 1, |_, _| scalar);
            let mut builder = ProgramBuilder::new(1, 0).or_fail()?;
            builder
                .numerical(
                    NumericalOperator::from_view(matrix.as_ref(), MatrixPolicy::default())
                        .or_fail()?,
                    &[],
                    &[],
                )
                .or_fail()?;
            let mut prepared = env.prepare(builder.finish().or_fail()?).or_fail()?;
            let mut register = env.state_vector(QubitCount::new(1).or_fail()?).or_fail()?;
            prepared.run(&mut register).or_fail()?;
            expect_that!(register.amplitude(0).or_fail()?, eq(scalar));
            drop(register);
            drop(prepared);
            expect_that!(env.allocated_bytes(), eq(0));
            let matrix = faer::Mat::<Complex64>::identity(32, 32);
            let mut builder = ProgramBuilder::new(5, 0).or_fail()?;
            let targets = (0..5)
                .map(|i| builder.qubit(i))
                .collect::<std::result::Result<Vec<_>, _>>()
                .or_fail()?;
            builder
                .numerical(
                    NumericalOperator::from_view(matrix.as_ref(), MatrixPolicy::default())
                        .or_fail()?,
                    &targets,
                    &[],
                )
                .or_fail()?;
            expect_true!(env.prepare(builder.finish().or_fail()?).is_err());
            expect_that!(env.allocated_bytes(), eq(0));
            env.close().map_err(|e| e.to_string()).or_fail()?;
            Ok(())
        },
    )
}

#[gtest]
fn changed_numerical_policy_is_rejected_before_mutation_or_seeding() -> googletest::Result<()> {
    isolated(
        "changed_numerical_policy_is_rejected_before_mutation_or_seeding",
        || {
            use quest::{Error, Gate, ProgramBuilder, Shots};
            let env = Environment::builder().build().or_fail()?;
            let mut builder = ProgramBuilder::new(1, 0).or_fail()?;
            let q = builder.qubit(0).or_fail()?;
            builder.gate(Gate::X, &[q], &[]).or_fail()?;
            let mut prepared = env.prepare(builder.finish().or_fail()?).or_fail()?;
            let mut register = env.state_vector(QubitCount::new(1).or_fail()?).or_fail()?;
            let epsilon = quest_sys::get_qu_est_validation_epsilon().or_fail()?;
            quest_sys::set_qu_est_validation_epsilon(epsilon * 2.).or_fail()?;
            quest_sys::set_qu_est_seeds(&[31, 47]).or_fail()?;
            let original_seeds = quest_sys::get_qu_est_seeds().or_fail()?;
            expect_true!(matches!(
                prepared.run(&mut register),
                Err(Error::ConfigurationChanged)
            ));
            expect_that!(register.amplitude(0).or_fail()?, eq(Complex64::new(1., 0.)));
            expect_true!(matches!(
                prepared.sample_zeroed(Shots::new(2).or_fail()?, &[99]),
                Err(Error::ConfigurationChanged)
            ));
            expect_that!(
                quest_sys::get_qu_est_seeds().or_fail()?,
                eq(&original_seeds)
            );
            quest_sys::set_qu_est_validation_epsilon(epsilon).or_fail()?;
            drop(register);
            drop(prepared);
            env.close().map_err(|e| e.to_string()).or_fail()?;
            Ok(())
        },
    )
}

#[gtest]
fn density_promotion_owns_an_independent_native_state() -> googletest::Result<()> {
    isolated("density_promotion_owns_an_independent_native_state", || {
        let env = Environment::builder().build().or_fail()?;
        let mut original = env.state_vector(QubitCount::new(1).or_fail()?).or_fail()?;
        original
            .init_pure(&[
                Complex64::new(0.5f64.sqrt(), 0.),
                Complex64::new(0., 0.5f64.sqrt()),
            ])
            .or_fail()?;
        let density = original.to_density().or_fail()?;
        drop(original);
        expect_that!(density.entry(0, 1).or_fail()?.im, near(-0.5, 1e-12));
        drop(density);
        env.close().map_err(|e| e.to_string()).or_fail()?;
        Ok(())
    })
}
