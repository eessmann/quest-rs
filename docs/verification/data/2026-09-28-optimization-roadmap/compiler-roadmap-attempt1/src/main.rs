//! New-stage ablations over the unchanged version-one corpus.
#[allow(dead_code)]
#[path = "baseline_compiler.rs"]
mod baseline;
use baseline::{Error, fixture, measure_report};
use quest_circuit::*;
use serde_json::json;
use std::result::Result;

fn options() -> Result<OptimizationOptions, Error> {
    Ok(OptimizationOptions::new(
        OptimizationTarget::once(
            DeploymentSnapshot::new(
                DeploymentKind::StateVector,
                6,
                false,
                false,
                false,
                0,
                1,
                64,
            )?,
            CostProfile::NativeV1,
        )?,
        OptimizationLimits::default(),
        ApproximationMode::Disabled,
    )?)
}
fn shared_symbolic() -> Result<(ValidatedProgram, ParameterId), Error> {
    let mut builder = ProgramBuilder::new(6, 0)?;
    let p = builder.parameter("theta")?;
    let mut angle = Angle::parameter(p)?;
    for _ in 0..12 {
        angle = angle.added(&angle)?;
    }
    let angle = angle.added(&Angle::pi(1, 3)?)?;
    for index in 0..6 {
        builder.gate(Gate::Rz(angle.clone()), &[builder.qubit(index)?], &[])?;
    }
    Ok((builder.finish()?, p))
}
fn main() -> Result<(), Error> {
    let mut failures = 0usize;
    for sample in 0..5 {
        failures += usize::from(!measure_report(
            "shared_symbolic6",
            "construction",
            sample,
            shared_symbolic,
            |(program, _)| json!({"operations":program.schedule().len(),"shared_doublings":12}),
        ));
        let (program, p) = shared_symbolic()?;
        failures += usize::from(!measure_report(
            "shared_symbolic6",
            "binding",
            sample,
            || Ok(program.bind(&[(p, 0.03125)])?),
            |program| json!({"operations":program.instructions().len()}),
        ));
    }

    for case in [
        "qft6",
        "pauli_evolution6",
        "qsvt_oracle6",
        "clifford_t6",
        "signed_controls6",
    ] {
        let original = fixture(case)?;
        for stage in [
            "schedule",
            "terminal",
            "linear_candidate",
            "parity_candidate",
            "beam",
        ] {
            for sample in 0..5 {
                let input = original.clone();
                let limits = OptimizationLimits::default();
                let ledger = BudgetLedger::new(limits);
                let options = options()?;
                let passed = match stage {
                    "schedule" => measure_report(
                        case,
                        stage,
                        sample,
                        || {
                            Ok(input
                                .bind(&[])?
                                .commutation_schedule(TerminalOptions::default(), &ledger)?)
                        },
                        |out| json!({"operations":out.order().len(),"logical_work":ledger.usage().work,"reserved_bytes":ledger.usage().retained_bytes}),
                    ),
                    "terminal" => measure_report(
                        case,
                        stage,
                        sample,
                        || {
                            Ok(input.bind(&[])?.schedule_and_fuse(
                                TerminalOptions::default(),
                                options.target(),
                                &ledger,
                            )?)
                        },
                        |out| json!({"operations":out.program().instructions().len(),"completion":format!("{:?}",out.report().status()),"rounding_changed":out.report().rounding_changed(),"logical_work":ledger.usage().work,"reserved_bytes":ledger.usage().retained_bytes}),
                    ),
                    "linear_candidate" => measure_report(
                        case,
                        stage,
                        sample,
                        || {
                            Ok(input.resynthesize_linear_candidate(
                                LinearOptions::default(),
                                LinearCandidateStrategy::Gaussian,
                                4096,
                            )?)
                        },
                        |out| json!({"operations":out.0.schedule().len(),"logical_work":out.1.work}),
                    ),
                    "parity_candidate" => measure_report(
                        case,
                        stage,
                        sample,
                        || Ok(input.parity_candidate(ParityOptions::default(), 4096)?),
                        |out| json!({"operations":out.0.schedule().len(),"logical_work":out.1.work,"accepted_windows":out.1.accepted_windows}),
                    ),
                    _ => measure_report(
                        case,
                        stage,
                        sample,
                        || {
                            Ok(Optimizer::from_ideal(input, &[], options)?
                                .search(BeamOptions::default())?)
                        },
                        |out| json!({"completion":format!("{:?}",out.stop_reason()),"rounding":format!("{:?}",out.rounding()),"evidence":format!("{:?}",out.evidence()),"logical_work":out.budget().work,"reserved_bytes":out.budget().retained_bytes,"search":format!("{:?}",out.search_report())}),
                    ),
                };
                failures += usize::from(!passed);
            }
        }
    }
    let source = format!(
        "input bool flag; qubit[6] q; int i=0; while(i<3) {{ {} i+=1; }}",
        "if(flag){h q[0];x q[2];h q[0];}else{cx q[1],q[3];cx q[1],q[3];} ".repeat(32)
    );
    let original = StructuredProgram::parse(&source, "corpus.qasm")?.verify()?;
    for stage in ["quantum_flow", "terminal", "beam"] {
        for sample in 0..5 {
            let input = original.clone();
            let options = options()?;
            let ledger = BudgetLedger::new(options.limits());
            let passed = match stage {
                "quantum_flow" => measure_report(
                    "branch_heavy6",
                    stage,
                    sample,
                    || {
                        Ok(language::ssa::QuantumFlow::analyze(
                            input.ssa(),
                            language::ssa::QuantumFlowLimits::default(),
                        )?)
                    },
                    |out| json!({"logical_work":out.usage().work,"reserved_bytes":out.usage().retained_bytes,"facts":out.facts().len(),"edges":out.edges().len()}),
                ),
                "terminal" => measure_report(
                    "branch_heavy6",
                    stage,
                    sample,
                    || Ok(input.fuse_terminal(TerminalOptions::default(), &options, &ledger)?),
                    |out| json!({"completion":format!("{:?}",out.report().status()),"windows":out.report().fused_windows(),"logical_work":ledger.usage().work,"reserved_bytes":ledger.usage().retained_bytes}),
                ),
                _ => measure_report(
                    "branch_heavy6",
                    stage,
                    sample,
                    || {
                        Ok(Optimizer::from_verified_structured(input, options)?
                            .search(BeamOptions::default())?)
                    },
                    |out| json!({"completion":format!("{:?}",out.stop_reason()),"logical_work":out.budget().work,"reserved_bytes":out.budget().retained_bytes}),
                ),
            };
            failures += usize::from(!passed);
        }
    }
    if failures != 0 {
        return Err(format!("{failures} failed samples retained in JSONL").into());
    }
    Ok(())
}
