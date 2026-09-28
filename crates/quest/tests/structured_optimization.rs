use googletest::{Result, prelude::*};
use quest::{
    Complex64, Environment, QubitCount, RunInputs, StructuredProgram, StructuredQuantumOptions,
};

fn isolated(name: &str, body: impl FnOnce() -> Result<()>) -> Result<()> {
    if std::env::var("QUEST_STRUCTURED_OPT_TEST").as_deref() == Ok(name) {
        return body();
    }
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("QUEST_STRUCTURED_OPT_TEST", name)
        .status()?;
    expect_true!(status.success());
    Ok(())
}
fn initial(width: usize, input: usize) -> Result<Vec<Complex64>> {
    let size = 1usize
        .checked_shl(u32::try_from(width)?)
        .ok_or_else(|| std::io::Error::other("fixture width"))?;
    let mut values = vec![Complex64::new(0.0, 0.0); size];
    if input < size {
        *values
            .get_mut(input)
            .ok_or_else(|| std::io::Error::other("fixture input"))? = Complex64::new(1.0, 0.0);
    } else {
        *values
            .first_mut()
            .ok_or_else(|| std::io::Error::other("fixture first"))? =
            Complex64::new(1.0 / 2.0f64.sqrt(), 0.0);
        *values
            .last_mut()
            .ok_or_else(|| std::io::Error::other("fixture last"))? =
            Complex64::new(0.0, 1.0 / 2.0f64.sqrt());
    }
    Ok(values)
}
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Independent finite complex residuals are explicitly bounded at2e-13"
)]
fn compare(environment: &Environment, width: usize, source: &str, count: usize) -> Result<()> {
    let original = StructuredProgram::parse(source, "native-differential.qasm")?.verify()?;
    let (optimized, report) = original
        .clone()
        .optimize_quantum(StructuredQuantumOptions::default())?;
    expect_gt!(report.before_gates, report.after_gates);
    let mut before = environment.prepare_structured_plan(original.lower()?.plan()?)?;
    let mut after = environment.prepare_structured_plan(optimized.lower()?.plan()?)?;
    let mut state_before = environment.state_vector(QubitCount::new(width)?)?;
    let mut state_after = environment.state_vector(QubitCount::new(width)?)?;
    let mut density_before = environment.density_matrix(QubitCount::new(width)?)?;
    let mut density_after = environment.density_matrix(QubitCount::new(width)?)?;
    for input in 0..count {
        let values = initial(width, input)?;
        state_before.init_pure(&values)?;
        state_after.init_pure(&values)?;
        density_before.init_pure(&values)?;
        density_after.init_pure(&values)?;
        let before_output = before.run(&mut state_before, &RunInputs::default())?;
        let after_output = after.run(&mut state_after, &RunInputs::default())?;
        expect_eq!(before_output.outputs, after_output.outputs);
        let before_output = before.run(&mut density_before, &RunInputs::default())?;
        let after_output = after.run(&mut density_after, &RunInputs::default())?;
        expect_eq!(before_output.outputs, after_output.outputs);
        for row in 0..values.len() {
            expect_lt!(
                (state_before.amplitude(row)? - state_after.amplitude(row)?).norm(),
                2e-13
            );
            for column in 0..values.len() {
                expect_lt!(
                    (density_before.entry(row, column)? - density_after.entry(row, column)?).norm(),
                    2e-13
                );
            }
        }
    }
    Ok(())
}
#[gtest]
fn exact_structured_optimization_preserves_native_state_density_and_observations() -> Result<()> {
    isolated(
        "exact_structured_optimization_preserves_native_state_density_and_observations",
        || {
            let environment = Environment::builder().build().or_fail()?;
            compare(
                &environment,
                3,
                "qubit[3] q; output int i=0; while(i<3) { h q[0]; x q[2]; h q[0]; cx q[0],q[1]; cx q[0],q[1]; negctrl @ y q[1],q[2]; negctrl @ y q[1],q[2]; i+=1; }",
                9,
            )?;
            compare(
                &environment,
                1,
                &format!("qubit q; {}", "x q; t q; x q; t q; ".repeat(9)),
                3,
            )?;
            compare(
                &environment,
                2,
                "qubit[2] q; h q[0]; h q[0]; x q[0]; output bit b=measure q[0]; if(bool(b)) { x q[1]; x q[1]; } reset q[0];",
                1,
            )?;
            Ok(())
        },
    )
}

#[gtest]
fn terminal_structured_fusion_preserves_full_native_state_and_density_operators() -> Result<()> {
    isolated(
        "terminal_structured_fusion_preserves_full_native_state_and_density_operators",
        || {
            use quest_circuit::{
                ApproximationMode, BudgetLedger, CostProfile, OptimizationLimits,
                OptimizationOptions, OptimizationTarget, TerminalOptions,
            };
            let environment = Environment::builder().build()?;
            let mut a = environment.state_vector(QubitCount::new(2)?)?;
            let mut b = environment.state_vector(QubitCount::new(2)?)?;
            let mut da = environment.density_matrix(QubitCount::new(2)?)?;
            let mut db = environment.density_matrix(QubitCount::new(2)?)?;
            let source = "def mixed(qubit a, qubit b) { h a; negctrl @ x a,b; t b; } gate pair a { h a; t a; } qubit[2] q; mixed(q[1],q[0]); inv @ pair q[0]; pow(-2) @ pair q[1]; h q[0]; x q[0];";
            let original = StructuredProgram::parse(source, "terminal-native.qasm")?.verify()?;
            let options = OptimizationOptions::new(
                OptimizationTarget::once(
                    a.deployment().compiler_snapshot()?,
                    CostProfile::NativeV1,
                )?,
                OptimizationLimits::default(),
                ApproximationMode::Disabled,
            )?;
            let outcome = original.clone().fuse_terminal(
                TerminalOptions::default(),
                &options,
                &BudgetLedger::new(OptimizationLimits::default()),
            )?;
            expect_ge!(outcome.report().fused_windows(), 2);
            let mut before = environment.prepare_structured_plan(original.lower()?.plan()?)?;
            let mut after =
                environment.prepare_structured_plan(outcome.into_program().lower()?.plan()?)?;
            for input in 0..5 {
                let values = initial(2, input)?;
                a.init_pure(&values)?;
                b.init_pure(&values)?;
                da.init_pure(&values)?;
                db.init_pure(&values)?;
                before.run(&mut a, &RunInputs::default())?;
                after.run(&mut b, &RunInputs::default())?;
                before.run(&mut da, &RunInputs::default())?;
                after.run(&mut db, &RunInputs::default())?;
                for row in 0..4 {
                    expect_lt!(
                        std::ops::Sub::sub(a.amplitude(row)?, b.amplitude(row)?).norm(),
                        2e-13
                    );
                    for col in 0..4 {
                        expect_lt!(
                            std::ops::Sub::sub(da.entry(row, col)?, db.entry(row, col)?).norm(),
                            2e-13
                        );
                    }
                }
            }
            Ok(())
        },
    )
}
