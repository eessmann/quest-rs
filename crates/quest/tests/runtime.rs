use googletest::prelude::*;
use quest::{Complex64, Environment, Error, MemoryBudget, QubitCount};

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
fn facade_reports_preserve_exact_resource_usage() {
    let report = Error::Budget {
        requested: 17,
        available: 11,
    }
    .report();
    expect_eq!(report.stage, quest::language::Stage::Preparation);
    expect_eq!(
        report.cause,
        quest::language::DiagnosticCause::ResourceLimit(quest::language::ResourceUsage {
            resource: quest::language::ResourceKind::PreparationBytes,
            requested: 17,
            limit: 11,
        })
    );
}

#[gtest]
fn preparation_accounts_for_retained_exact_angle_coefficients() -> googletest::Result<()> {
    isolated(
        "preparation_accounts_for_retained_exact_angle_coefficients",
        || {
            let plan = |angle| -> quest::Result<quest::ExecutablePlan> {
                let mut builder = quest::ProgramBuilder::new(1, 0)?;
                builder.gate(quest::Gate::Rz(angle), &[builder.qubit(0)?], &[])?;
                Ok(builder.finish()?.bind(&[])?.plan()?)
            };
            let small = plan(quest::Angle::pi(1, 1)?)?;
            // Near-one ratio with coprime, individually large coefficients. Both
            // plans execute the same rounded radians but retain different proofs.
            let numerator = format!("1{}1", "0".repeat(398)).parse()?;
            let denominator = format!("1{}", "0".repeat(399)).parse()?;
            let large = plan(quest::Angle::rational_pi(quest::BigRational::new(
                numerator,
                denominator,
            ))?)?;
            let env = Environment::builder().build()?;
            let prepared = env.prepare_plan(small)?;
            let small_bytes = env.allocated_bytes();
            drop(prepared);
            expect_eq!(env.allocated_bytes(), 0);
            let prepared = env.prepare_plan(large)?;
            verify_that!(env.allocated_bytes().saturating_sub(small_bytes), gt(500))?;
            drop(prepared);
            expect_eq!(env.allocated_bytes(), 0);
            Ok(())
        },
    )
}

#[gtest]
fn bell_state_and_snapshot_survive_teardown() -> googletest::Result<()> {
    isolated("bell_state_and_snapshot_survive_teardown", || {
        let snapshot = {
            let env = Environment::builder().build().or_fail()?;
            let mut register = env.state_vector(QubitCount::new(2).or_fail()?).or_fail()?;
            register.h(0).or_fail()?;
            register.cx(0, 1).or_fail()?;
            register.snapshot().or_fail()?
        };
        expect_false!(quest_sys::is_quest_env_init());
        quest_sys::finalize_quest_env().or_fail()?;
        expect_that!(snapshot[(0, 0)].re, near(2f64.sqrt().recip(), 1e-14));
        expect_that!(snapshot[(3, 0)].re, near(2f64.sqrt().recip(), 1e-14));
        expect_that!(snapshot[(1, 0)], eq(Complex64::new(0., 0.)));
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
            Ok(())
        },
    )
}

#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Independent complex fixtures use bounded basis indices and finite nonzero norms"
)]
fn density_imaginary_entries_and_rectangular_blocks_keep_layout() -> googletest::Result<()> {
    isolated(
        "density_imaginary_entries_and_rectangular_blocks_keep_layout",
        || {
            let env = Environment::builder().build().or_fail()?;
            let mut density = env
                .density_matrix(QubitCount::new(2).or_fail()?)
                .or_fail()?;
            let block = faer::Mat::from_fn(2, 3, |r, c| {
                Complex64::new(
                    fixture_index(r + 3 * c),
                    fixture_index(r) - fixture_index(c),
                )
            });
            density.write_block(1, 0, block.as_ref()).or_fail()?;
            let snapshot = density.snapshot().or_fail()?;
            for r in 0..2 {
                for c in 0..3 {
                    expect_that!(snapshot[(r + 1, c)], eq(block[(r, c)]));
                }
            }
            let tall = faer::Mat::from_fn(3, 1, |r, _| Complex64::new(fixture_index(r), 0.25));
            density.write_block(0, 3, tall.as_ref()).or_fail()?;
            expect_that!(density.entry(2, 3).or_fail()?, eq(tall[(2, 0)]));
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
        Ok(())
    })
}

#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Independent complex fixtures use bounded basis indices and finite nonzero norms"
)]
fn nonsorted_numerical_targets_preserve_complex_operator_order() -> googletest::Result<()> {
    isolated(
        "nonsorted_numerical_targets_preserve_complex_operator_order",
        || {
            use quest::{MatrixPolicy, NumericalOperator, ProgramBuilder};
            let env = Environment::builder().build().or_fail()?;
            let input: Vec<_> = (0..8)
                .map(|i| Complex64::new(fixture_index(i + 1), fixture_index(7 - i)))
                .collect();
            let norm = input.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
            let input: Vec<_> = input.into_iter().map(|v| v / norm).collect();
            let mut matrix = faer::Mat::from_fn(4, 4, |r, c| {
                Complex64::new(
                    fixture_index(1 + r + 2 * c) / 10.,
                    (fixture_index(r) - fixture_index(c)) / 7.,
                )
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
            Ok(())
        },
    )
}

#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Independent complex fixtures use bounded basis indices and finite nonzero norms"
)]
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
            Ok(())
        },
    )
}

#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Independent complex fixtures use bounded basis indices and finite nonzero norms"
)]
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
            let mut optimized = env.prepare_plan(fused.plan().or_fail()?).or_fail()?;
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
        Ok(())
    })
}

#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Independent complex fixtures use bounded basis indices and finite nonzero norms"
)]
fn openqasm31_u_phase_survives_native_adjoint_signed_controls_and_fusion() -> googletest::Result<()>
{
    isolated(
        "openqasm31_u_phase_survives_native_adjoint_signed_controls_and_fusion",
        || {
            use quest::{Angle, Control, ControlState, FusionOptions, Gate, ProgramBuilder};
            let env = Environment::builder().build().or_fail()?;
            // U(pi/2, 0, pi) = exp(i*pi/4) H, so |0> maps to
            // (1+i)/2 on both target states. The adjoint conjugates that phase.
            for inverse in [false, true] {
                for polarity in [None, Some(ControlState::Zero), Some(ControlState::One)] {
                    for fuse in [false, true] {
                        let mut builder = ProgramBuilder::new(3, 0).or_fail()?;
                        let target = builder.qubit(1).or_fail()?;
                        let controls = match polarity {
                            None => vec![],
                            Some(state) => vec![
                                Control::new(builder.qubit(2).or_fail()?, state),
                                Control::new(builder.qubit(0).or_fail()?, ControlState::Zero),
                            ],
                        };
                        let gate = Gate::U {
                            theta: Angle::pi(1, 2).or_fail()?,
                            phi: Angle::pi(0, 1).or_fail()?,
                            lambda: Angle::pi(1, 1).or_fail()?,
                        };
                        builder
                            .gate(
                                if inverse {
                                    gate.adjoint().or_fail()?
                                } else {
                                    gate
                                },
                                &[target],
                                &controls,
                            )
                            .or_fail()?;
                        // H after U makes the expected output a phase on target |0>,
                        // and provides a real two-gate fusion block.
                        builder.gate(Gate::H, &[target], &controls).or_fail()?;
                        let bound = builder.finish().or_fail()?.bind(&[]).or_fail()?;
                        let bound = if fuse {
                            let (bound, report) = bound.fuse(FusionOptions::default()).or_fail()?;
                            expect_eq!(report.after_operations, 1);
                            bound
                        } else {
                            bound
                        };
                        let mut prepared = env.prepare_plan(bound.plan().or_fail()?).or_fail()?;
                        let mut register =
                            env.state_vector(QubitCount::new(3).or_fail()?).or_fail()?;
                        let input = [
                            Complex64::new(0.5, 0.0),
                            Complex64::new(0.5, 0.0),
                            Complex64::new(0.0, 0.0),
                            Complex64::new(0.0, 0.0),
                            Complex64::new(0.5, 0.0),
                            Complex64::new(0.5, 0.0),
                            Complex64::new(0.0, 0.0),
                            Complex64::new(0.0, 0.0),
                        ];
                        register.init_pure(&input).or_fail()?;
                        prepared.run(&mut register).or_fail()?;
                        let snapshot = register.snapshot().or_fail()?;
                        for (basis, initial) in input.into_iter().enumerate() {
                            let selected = polarity.is_none_or(|state| {
                                basis & 1 == 0 && ((basis & 4 != 0) == (state == ControlState::One))
                            });
                            let phase = Complex64::new(
                                std::f64::consts::FRAC_1_SQRT_2,
                                if inverse {
                                    -std::f64::consts::FRAC_1_SQRT_2
                                } else {
                                    std::f64::consts::FRAC_1_SQRT_2
                                },
                            );
                            let expected = if selected { initial * phase } else { initial };
                            expect_lt!((snapshot[(basis, 0)] - expected).norm(), 1e-13);
                        }
                    }
                }
            }
            Ok(())
        },
    )
}

// Out-of-fixture indices produce NaN so numerical comparisons fail explicitly.
fn fixture_index(index: usize) -> f64 {
    u32::try_from(index).map_or(f64::NAN, f64::from)
}

#[gtest]
fn diagonal_preparation_retains_structure_under_a_small_native_budget() -> googletest::Result<()> {
    isolated(
        "diagonal_preparation_retains_structure_under_a_small_native_budget",
        || {
            let env = Environment::builder()
                .memory_budget(MemoryBudget::new(98_304))
                .build()?;
            let values = faer::Mat::from_fn(32, 32, |row, col| {
                if row == col {
                    Complex64::new(0.0, 1.0)
                } else {
                    Complex64::new(0.0, 0.0)
                }
            });
            let operator = quest::NumericalOperator::from_view(
                values.as_ref(),
                quest::MatrixPolicy::default(),
            )?;
            expect_true!(operator.is_diagonal());
            let mut builder = quest::ProgramBuilder::new(5, 0)?;
            let targets = (0..5)
                .rev()
                .map(|index| builder.qubit(index))
                .collect::<std::result::Result<Vec<_>, quest::CircuitError>>()?;
            builder.numerical(operator, &targets, &[])?;
            let mut prepared = env.prepare(builder.finish()?)?;
            let mut state = env.state_vector(QubitCount::new(5)?)?;
            prepared.run(&mut state)?;
            expect_eq!(state.snapshot()?[(0, 0)], Complex64::new(0.0, 1.0));
            drop(state);
            let mut density = env.density_matrix(QubitCount::new(5)?)?;
            prepared.run(&mut density)?;
            expect_eq!(density.entry(0, 0)?, Complex64::new(1.0, 0.0));
            Ok(())
        },
    )
}
