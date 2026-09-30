use googletest::prelude::*;
use quest::{
    Angle, BoundGate, Complex64, Control, ControlState, Environment, Gate, MatrixPolicy,
    MemoryBudget, NumericalOperator, Operation, OracleFragment, QuantumRegionBuilder, QubitCount,
    RunInputs, circuit,
};

fn isolated(name: &str, body: impl FnOnce() -> googletest::Result<()>) -> googletest::Result<()> {
    if std::env::var("QUEST_ORACLE_TEST").as_deref() == Ok(name) {
        return body();
    }
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("QUEST_ORACLE_TEST", name)
        .status()?;
    expect_true!(status.success());
    Ok(())
}
fn fragment() -> quest::Result<OracleFragment> {
    let matrix = faer::Mat::from_fn(2, 2, |r, c| match (r, c) {
        (0, 1) => Complex64::new(0.0, 1.0),
        (1, 0) => Complex64::new(1.0, 0.0),
        _ => Complex64::new(0.0, 0.0),
    });
    let matrix = NumericalOperator::from_view(&matrix, MatrixPolicy::default())?;
    let mut builder = QuantumRegionBuilder::new(2, 0)?;
    builder.numerical(matrix, &[builder.qubit(0)?], &[])?;
    builder.gate(Gate::H, &[builder.qubit(1)?], &[])?;
    builder.global_phase(Angle::radians(0.37)?, &[])?;
    Ok(OracleFragment::builder(builder.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?)
}
fn append_expanded(
    builder: &mut QuantumRegionBuilder,
    body: &OracleFragment,
    targets: &[quest::QubitId],
    controls: &[Control],
) -> quest::Result<()> {
    for operation in body.decompose(targets, controls, MatrixPolicy::default())? {
        match operation {
            Operation::Numerical {
                matrix,
                targets,
                controls,
            } => {
                builder.numerical(matrix, &targets, &controls)?;
            }
            Operation::Gate {
                gate: BoundGate::H,
                targets,
                controls,
            } => {
                builder.gate(Gate::H, &targets, &controls)?;
            }
            Operation::GlobalPhase { radians, controls } => {
                builder.global_phase(Angle::radians(radians)?, &controls)?;
            }
            _ => {
                return Err(quest::Error::Value(
                    "unexpected reference fixture operation",
                ));
            }
        }
    }
    Ok(())
}
#[gtest]
fn retained_oracles_match_expanded_phase_target_and_density_semantics() -> googletest::Result<()> {
    isolated(
        "retained_oracles_match_expanded_phase_target_and_density_semantics",
        || {
            let environment = Environment::builder().build()?;
            let body = fragment()?;
            let build = |retained: bool| -> quest::Result<_> {
                let mut builder = QuantumRegionBuilder::new(3, 0)?;
                for (adjoint, order, positive) in [
                    (false, [2, 0], false),
                    (true, [0, 2], true),
                    (false, [0, 2], false),
                ] {
                    let fragment = if adjoint {
                        body.adjoint()
                    } else {
                        body.clone()
                    };
                    let targets = [builder.qubit(order[0])?, builder.qubit(order[1])?];
                    let controls = [Control::new(
                        builder.qubit(1)?,
                        if positive {
                            ControlState::One
                        } else {
                            ControlState::Zero
                        },
                    )];
                    if retained {
                        builder.oracle(&fragment, &targets, &controls)?;
                    } else {
                        append_expanded(&mut builder, &fragment, &targets, &controls)?;
                    }
                }
                Ok(builder.finish()?.bind(&[])?.plan()?)
            };
            let mut retained = environment.prepare(
                quest::Program::from_bound_region((build(true)?).into_region())?
                    .verify()?
                    .lower()?
                    .plan()?,
            )?;
            let mut expanded = environment.prepare(
                quest::Program::from_bound_region((build(false)?).into_region())?
                    .verify()?
                    .lower()?
                    .plan()?,
            )?;
            expect_eq!(retained.prepared_oracle_bodies(), 1);
            expect_eq!(retained.prepared_oracle_matrix_variants(), 2);
            let mut actual = environment.state_vector(QubitCount::new(3)?)?;
            let mut expected = environment.state_vector(QubitCount::new(3)?)?;
            actual.init_plus()?;
            expected.init_plus()?;
            let bytes = environment.allocated_bytes();
            retained.run(&mut actual, &quest::RunInputs::default())?;
            expanded.run(&mut expected, &quest::RunInputs::default())?;
            expect_eq!(environment.allocated_bytes(), bytes);
            let a = actual.snapshot()?;
            let b = expected.snapshot()?;
            for row in 0..8 {
                expect_that!(a[(row, 0)].re, near(b[(row, 0)].re, 1e-13));
                expect_that!(a[(row, 0)].im, near(b[(row, 0)].im, 1e-13));
            }
            let mut actual = environment.density_matrix(QubitCount::new(3)?)?;
            let mut expected = environment.density_matrix(QubitCount::new(3)?)?;
            actual.init_plus()?;
            expected.init_plus()?;
            retained.run(&mut actual, &quest::RunInputs::default())?;
            expanded.run(&mut expected, &quest::RunInputs::default())?;
            let a = actual.snapshot()?;
            let b = expected.snapshot()?;
            for row in 0..8 {
                for col in 0..8 {
                    expect_that!(a[(row, col)].re, near(b[(row, col)].re, 1e-13));
                    expect_that!(a[(row, col)].im, near(b[(row, col)].im, 1e-13));
                }
            }
            Ok(())
        },
    )
}
#[gtest]
fn structured_oracle_profiles_and_nested_adjoint_reuse_prepared_bodies() -> googletest::Result<()> {
    isolated(
        "structured_oracle_profiles_and_nested_adjoint_reuse_prepared_bodies",
        || {
            let environment = Environment::builder().build()?;
            let body = fragment()?;
            let program = circuit! {
                oracle block[2] = ${body.clone()};
                gate wrapper a,b { block a,b; }
                qubit[3] q;
                h q[1];
                negctrl @ wrapper q[1],q[2],q[0];
                adjoint @ negctrl @ wrapper q[1],q[2],q[0];
            }?;
            let mut prepared = environment.prepare((program).verify()?.lower()?.plan()?)?;
            expect_eq!(prepared.prepared_oracle_bodies(), 1);
            expect_eq!(prepared.prepared_oracle_matrix_variants(), 1);
            let mut state = environment.state_vector(QubitCount::new(3)?)?;
            let result = prepared.run(&mut state, &RunInputs::default())?;
            expect_eq!(result.completed_quantum, 3);
            expect_that!(state.amplitude(0)?.re, near(2f64.sqrt().recip(), 1e-13));
            expect_that!(state.amplitude(2)?.re, near(2f64.sqrt().recip(), 1e-13));
            expect_that!(state.amplitude(0)?.im, near(0.0, 1e-13));
            Ok(())
        },
    )
}
#[gtest]
fn oracle_control_variants_respect_transactional_preparation_budget() -> googletest::Result<()> {
    isolated(
        "oracle_control_variants_respect_transactional_preparation_budget",
        || {
            let environment = Environment::builder()
                .memory_budget(MemoryBudget::new(4096))
                .build()?;
            let body = fragment()?;
            let mut builder = QuantumRegionBuilder::new(8, 0)?;
            let targets = [builder.qubit(7)?, builder.qubit(6)?];
            let controls = (0..6)
                .map(|q| Ok(Control::new(builder.qubit(q)?, ControlState::One)))
                .collect::<quest_circuit::Result<Vec<_>>>()?;
            builder.oracle(&body, &targets, &controls)?;
            expect_true!(
                environment
                    .prepare(
                        quest::Program::from_region(builder.finish()?, &[])?
                            .verify()?
                            .lower()?
                            .plan()?
                    )
                    .is_err()
            );
            expect_eq!(environment.allocated_bytes(), 0);
            Ok(())
        },
    )
}

#[gtest]
fn unused_oracle_capture_needs_no_native_cache() -> googletest::Result<()> {
    isolated("unused_oracle_capture_needs_no_native_cache", || {
        let environment = Environment::builder().build()?;
        let body = fragment()?;
        let program = circuit! { oracle unused[2] = ${body}; qubit q; x q; }?;
        let mut prepared = environment.prepare((program).verify()?.lower()?.plan()?)?;
        expect_eq!(prepared.prepared_oracle_bodies(), 0);
        expect_eq!(prepared.prepared_oracle_matrix_variants(), 0);
        let mut register = environment.state_vector(QubitCount::new(1)?)?;
        prepared.run(&mut register, &RunInputs::default())?;
        expect_that!(register.amplitude(1)?.re, near(1.0, 1e-13));
        Ok(())
    })
}

#[gtest]
fn nested_shared_fragment_orientation_uses_one_native_matrix_pair() -> googletest::Result<()> {
    isolated(
        "nested_shared_fragment_orientation_uses_one_native_matrix_pair",
        || {
            let environment = Environment::builder().build()?;
            let leaf = fragment()?;
            let mut builder = QuantumRegionBuilder::new(2, 0)?;
            builder.oracle(
                &leaf.adjoint(),
                &[builder.qubit(1)?, builder.qubit(0)?],
                &[],
            )?;
            let nested = OracleFragment::builder(builder.finish()?.bind(&[])?)
                .matrix_tolerance(1e-12)?
                .build()?;
            let mut builder = QuantumRegionBuilder::new(3, 0)?;
            let targets = [builder.qubit(2)?, builder.qubit(0)?];
            let controls = [Control::new(builder.qubit(1)?, ControlState::Zero)];
            builder.oracle(&nested, &targets, &controls)?;
            builder.oracle(&nested.adjoint(), &targets, &controls)?;
            let mut prepared = environment.prepare(
                quest::Program::from_region(builder.finish()?, &[])?
                    .verify()?
                    .lower()?
                    .plan()?,
            )?;
            expect_eq!(prepared.prepared_oracle_bodies(), 2);
            expect_eq!(prepared.prepared_oracle_matrix_variants(), 1);
            let mut register = environment.state_vector(QubitCount::new(3)?)?;
            register.init_plus()?;
            let before = register.snapshot()?;
            prepared.run(&mut register, &quest::RunInputs::default())?;
            let after = register.snapshot()?;
            for row in 0..8 {
                expect_that!(after[(row, 0)].re, near(before[(row, 0)].re, 1e-13));
                expect_that!(after[(row, 0)].im, near(before[(row, 0)].im, 1e-13));
            }
            Ok(())
        },
    )
}

#[gtest]
fn oracle_snapshot_outlives_cached_native_resources() -> googletest::Result<()> {
    isolated("oracle_snapshot_outlives_cached_native_resources", || {
        let snapshot = {
            let environment = Environment::builder().build()?;
            let mut builder = QuantumRegionBuilder::new(2, 0)?;
            builder.oracle(&fragment()?, &[builder.qubit(0)?, builder.qubit(1)?], &[])?;
            let mut prepared = environment.prepare(
                quest::Program::from_region(builder.finish()?, &[])?
                    .verify()?
                    .lower()?
                    .plan()?,
            )?;
            let mut state = environment.state_vector(QubitCount::new(2)?)?;
            prepared.run(&mut state, &quest::RunInputs::default())?;
            state.snapshot()?
        };
        expect_false!(quest_sys::is_quest_env_init());
        expect_that!(
            snapshot[(1, 0)].re,
            near(0.37f64.cos() / 2f64.sqrt(), 1e-13)
        );
        expect_that!(
            snapshot[(1, 0)].im,
            near(0.37f64.sin() / 2f64.sqrt(), 1e-13)
        );
        Ok(())
    })
}
