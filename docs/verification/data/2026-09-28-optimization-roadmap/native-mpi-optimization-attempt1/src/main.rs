//! Multi-rank coherent-facade timings; aggregate elapsed time by maximum rank.
#[allow(dead_code)]
#[path = "baseline_compiler.rs"]
mod baseline;
use baseline::{Error, measure_keep};
use quest::{
    collective::{CollectiveEnvironment, MpiRuntime},
    *,
};
use serde_json::json;
use std::result::Result;
fn main() -> Result<(), Error> {
    let runtime = MpiRuntime::initialize()?;
    let communicator = runtime.world()?;
    let env = CollectiveEnvironment::builder(&communicator)?.build()?;
    let mut register = env.state_vector(QubitCount::new(12)?)?;
    let deployment = register.deployment();
    if !deployment.is_distributed() || ![2, 4].contains(&deployment.nodes()) {
        return Err("requires two/four MPI ranks".into());
    }
    println!(
        "{}",
        json!({"metadata":"MPI collective facade; SV only; structured/DM execution unsupported by facade", "rank":deployment.rank(),"nodes":deployment.nodes(),"deployment":format!("{deployment:?}"),"warm_batch":10})
    );
    for case in [
        "qft6",
        "pauli_evolution6",
        "qsvt_oracle6",
        "clifford_t6",
        "signed_controls6",
    ] {
        let label = format!("{case}/rank{}", deployment.rank());
        let mut reference_marginals = Vec::new();
        for stage in [
            "unchanged",
            "existing_combined",
            "terminal",
            "terminal_reuse100",
            "beam",
        ] {
            let (plan, completion) = measure_keep(
                &label,
                &format!("{stage}/search"),
                0,
                || {
                    let source = baseline::fixture_with_width(case, 12)?;
                    let options = OptimizationOptions::new(
                        OptimizationTarget::new(
                            deployment.compiler_snapshot()?,
                            BigRational::from_integer(if stage == "terminal_reuse100" {
                                100.into()
                            } else {
                                1.into()
                            }),
                            CostProfile::NativeV1,
                        )?,
                        OptimizationLimits::default(),
                        ApproximationMode::Disabled,
                    )?;
                    Ok(match stage {
                        "existing_combined" => (
                            source
                                .optimize_exact()?
                                .0
                                .optimize_linear(LinearOptions::default())?
                                .0
                                .optimize_parity(ParityOptions::default())?
                                .0
                                .bind(&[])?
                                .fuse(FusionOptions::default())?
                                .0
                                .plan()?,
                            "Complete".into(),
                        ),
                        "terminal" | "terminal_reuse100" => {
                            let out = source.bind(&[])?.schedule_and_fuse(
                                TerminalOptions::default(),
                                options.target(),
                                &BudgetLedger::new(options.limits()),
                            )?;
                            let status = format!("{:?}", out.report().status());
                            (out.into_program().plan()?, status)
                        }
                        "beam" => {
                            let out = Optimizer::from_ideal(source, &[], options)?
                                .search(BeamOptions::default())?;
                            let status = format!("{:?}", out.stop_reason());
                            let OptimizerInput::Ideal { bound, .. } = out.into_input() else {
                                return Err("beam kind".into());
                            };
                            (bound.plan()?, status)
                        }
                        _ => (source.bind(&[])?.plan()?, "Complete".into()),
                    })
                },
                |(_, status)| json!({"completion":status}),
            )?;
            for sample in 0..5 {
                let prepared = measure_keep(
                    &label,
                    &format!("{stage}/preparation"),
                    sample,
                    || Ok(env.prepare_plan(plan.clone())?),
                    |_| json!({"completion":completion}),
                )?;
                drop(prepared);
            }
            let mut prepared = env.prepare_plan(plan)?;
            for input in 0..2 {
                if input == 0 {
                    register.init_zero()?;
                } else {
                    register.init_plus()?;
                }
                prepared.run(&mut register)?;
                quest_sys::sync_quest_env()?;
                let marginals = (0..register.num_qubits().get())
                    .map(|qubit| {
                        register
                            .probability(qubit, Outcome::One)
                            .map(Probability::get)
                    })
                    .collect::<quest::Result<Vec<_>>>()?;
                if stage == "unchanged" {
                    reference_marginals.push(marginals);
                } else {
                    let reference = reference_marginals
                        .get(input)
                        .ok_or("missing MPI baseline")?;
                    let local_match = marginals.len() == reference.len()
                        && !marginals
                            .iter()
                            .zip(reference)
                            .any(|(actual, expected)| (actual - expected).abs() > 1e-10);
                    let mut lane = communicator.collective_lane()?;
                    let all_match = lane.all_agree(local_match)?;
                    drop(lane);
                    if !all_match
                    {
                        return Err("MPI one-qubit probability mismatch".into());
                    }
                }
            }
            register.init_plus()?;
            prepared.run(&mut register)?;
            quest_sys::sync_quest_env()?;
            for sample in 0..5 {
                quest_sys::sync_quest_env()?;
                measure_keep(
                    &label,
                    &format!("{stage}/warm_execution"),
                    sample,
                    || {
                        for _ in 0..10 {
                            prepared.run(&mut register)?;
                        }
                        quest_sys::sync_quest_env()?;
                        Ok(())
                    },
                    |_| json!({"completion":completion,"executions":10}),
                )?;
            }
            let probability = register.total_probability()?;
            if (probability - 1.0).abs() > 1e-9 {
                return Err(format!("probability drift {probability}").into());
            }
        }
    }
    Ok(())
}
