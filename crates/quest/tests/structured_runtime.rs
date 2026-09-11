use googletest::prelude::*;
use quest::{
    Complex64, Environment, InterpreterLimits, QubitCount, RunInputs, StructuredProgram, circuit,
};

fn isolated(name: &str, body: impl FnOnce() -> Result<()>) -> Result<()> {
    if std::env::var("QUEST_STRUCTURED_TEST").as_deref() == Ok(name) {
        return body();
    }
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("QUEST_STRUCTURED_TEST", name)
        .status()?;
    expect_true!(status.success());
    Ok(())
}
#[gtest]
fn structured_feedback_and_density_reset_preserve_observations() -> Result<()> {
    isolated(
        "structured_feedback_and_density_reset_preserve_observations",
        || {
            let environment = Environment::builder().build().or_fail()?;
            let program = circuit! {
                qubit[2] q;
                output bit outcome;
                h q[0]; cx q[0],q[1];
                outcome = measure q[0];
                if (bool(outcome)) { x q[1]; }
                reset q[0];
            }?;
            let mut prepared = environment.prepare_structured(program).or_fail()?;
            let mut state = environment
                .state_vector(QubitCount::new(2).or_fail()?)
                .or_fail()?;
            let result = prepared.run(&mut state, &RunInputs::default()).or_fail()?;
            expect_true!(result.outputs.contains_key("outcome"));
            expect_that!(state.amplitude(0).or_fail()?.re, near(1., 1e-13));
            let mut density = environment
                .density_matrix(QubitCount::new(2).or_fail()?)
                .or_fail()?;
            prepared
                .run(&mut density, &RunInputs::default())
                .or_fail()?;
            expect_that!(density.entry(0, 0).or_fail()?.re, near(1., 1e-13));
            Ok(())
        },
    )
}
#[gtest]
fn runtime_loop_capture_once_and_step_failure_report_prefix() -> Result<()> {
    isolated(
        "runtime_loop_capture_once_and_step_failure_report_prefix",
        || {
            let mut captures = Vec::new();
            let program = circuit! {
                gate turn(a) q { rz(a) q; }
                qubit q; output int count=0;
                h q;
                while(count<3) { turn(${{captures.push(1); 0.25}}) q; count+=1; }
            }?;
            expect_eq!(captures, vec![1]);
            let (macro_diagnostic, source_diagnostic) = {
                let environment = Environment::builder().build().or_fail()?;
                let mut prepared = environment.prepare_structured(program).or_fail()?;
                let mut state = environment
                    .state_vector(QubitCount::new(1).or_fail()?)
                    .or_fail()?;
                let result = prepared.run(&mut state, &RunInputs::default()).or_fail()?;
                expect_eq!(
                    result
                        .outputs
                        .get("count")
                        .and_then(|value| value.as_scalar())
                        .map(quest::language::classical::ScalarValue::to_i128),
                    Some(Ok(3))
                );
                expect_that!(
                    state.amplitude(0).or_fail()?.im,
                    near(-0.375_f64.sin() / 2_f64.sqrt(), 1e-13)
                );
                let looping = circuit! {qubit q; while(true){x q;}}?;
                let mut loop_plan = environment.prepare_structured(looping).or_fail()?;
                let error = loop_plan
                    .run_with_limits(
                        &mut state,
                        &RunInputs::default(),
                        InterpreterLimits {
                            steps: 100,
                            ..InterpreterLimits::default()
                        },
                    )
                    .unwrap_err();
                let quest::Error::StructuredExecution(error) = error else {
                    fail!("structured error expected")?;
                    return Ok(());
                };
                expect_true!(matches!(
                    error.cause,
                    quest::language::vm::RuntimeCause::StepLimit
                ));
                expect_gt!(error.completed_quantum, 0);
                expect_false!(error.context.is_empty());
                let macro_diagnostic = error.diagnostic().clone();
                expect_true!(macro_diagnostic.labels.is_empty());
                expect_true!(
                    macro_diagnostic
                        .notes
                        .iter()
                        .any(|note| note.contains("Rust source location:"))
                );
                expect_true!(matches!(
                    macro_diagnostic.provenance.entity,
                    Some(quest::language::Entity::Operation(_))
                ));
                expect_false!(macro_diagnostic.provenance.execution.is_empty());

                let sourced = StructuredProgram::parse(
                    "qubit q; input int value; if (value > 0) { x q; }",
                    "runtime.qasm",
                )?;
                let mut source_plan = environment.prepare_structured(sourced).or_fail()?;
                let source_error = source_plan
                    .run(&mut state, &RunInputs::default())
                    .unwrap_err();
                let source_diagnostic = source_error.diagnostic().or_fail()?.clone();
                expect_eq!(source_diagnostic.labels.len(), 1);
                (macro_diagnostic, source_diagnostic)
            };
            expect_false!(quest_sys::is_quest_env_init());
            quest_sys::finalize_quest_env().or_fail()?;
            macro_diagnostic.validate_sources()?;
            source_diagnostic.validate_sources()?;
            #[cfg(feature = "codespan-reporting")]
            expect_true!(
                quest::language::render_plain(&source_diagnostic)?.contains("runtime.qasm")
            );
            expect_false!(
                source_diagnostic
                    .sources
                    .slice(source_diagnostic.labels[0].span)?
                    .is_empty()
            );
            expect_eq!(
                source_diagnostic
                    .sources
                    .get(quest::language::SourceId::new(1))
                    .or_fail()?
                    .name(),
                "runtime.qasm"
            );
            Ok(())
        },
    )
}
#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Independent finite two-qubit analytical matrix fixture"
)]
fn standard_cu_and_cx_preserve_full_phase_on_each_basis_column() -> Result<()> {
    isolated(
        "standard_cu_and_cx_preserve_full_phase_on_each_basis_column",
        || {
            let environment = Environment::builder().build().or_fail()?;
            let program = circuit! {
                include "stdgates.inc";
                qubit[2] q;
                cu(0.7,-0.4,0.2,0.3) q[1],q[0];
                CX q[1],q[0];
            }?;
            let mut prepared = environment.prepare_structured(program).or_fail()?;
            let mut state = environment
                .state_vector(QubitCount::new(2).or_fail()?)
                .or_fail()?;
            let cis = |x: f64| Complex64::new(x.cos(), x.sin());
            // The specification's active block is exp(i gamma) U3.1, then CX.
            // U3.1 includes exp(i theta/2) relative to the conventional Euler matrix.
            let c = 0.35_f64.cos();
            let s = 0.35_f64.sin();
            let u = [
                [cis(0.65) * c, -cis(0.85) * s],
                [cis(0.25) * s, cis(0.45) * c],
            ];
            for column in 0..4 {
                state.init_zero().or_fail()?;
                if column & 1 != 0 {
                    state.x(0).or_fail()?;
                }
                if column & 2 != 0 {
                    state.x(1).or_fail()?;
                }
                prepared.run(&mut state, &RunInputs::default()).or_fail()?;
                for row in 0..4 {
                    let expected = if column < 2 {
                        if row == column {
                            Complex64::new(1., 0.)
                        } else {
                            Complex64::new(0., 0.)
                        }
                    } else if row >= 2 {
                        u[(row - 2) ^ 1][column - 2]
                    } else {
                        Complex64::new(0., 0.)
                    };
                    expect_that!(
                        (state.amplitude(row).or_fail()? - expected).norm(),
                        near(0., 1e-13)
                    );
                }
            }
            Ok(())
        },
    )
}
