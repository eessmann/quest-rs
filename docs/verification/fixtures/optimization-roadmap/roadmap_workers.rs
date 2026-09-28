//! Optional worker ablations. Parent certificates, timeouts and exhaustion are observable.
#[allow(dead_code)]
#[path = "baseline_compiler.rs"]
mod baseline;
use baseline::{Error, fixture, measure_report};
use quest_circuit::*;
use quest_optimizer_client::{Client, MitmResult, WorkerLimits};
use quest_optimizer_protocol::MitmLimits;
use serde_json::json;
use std::result::Result;
fn status<T>(result: &MitmResult<T>) -> String {
    match result {
        MitmResult::Candidate(_) => "Candidate".into(),
        MitmResult::NoCandidate { explored } => format!("NoCandidate({explored})"),
        MitmResult::Incomplete { reason, explored } => format!("Incomplete({reason},{explored})"),
        MitmResult::Exhausted { explored } => format!("Exhausted({explored})"),
        MitmResult::Unresolved {
            precision_bits,
            explored,
        } => format!("Unresolved({precision_bits},{explored})"),
    }
}
fn main() -> Result<(), Error> {
    let client = Client::new(
        std::env::var("QUEST_ROADMAP_WORKER")?,
        WorkerLimits::default(),
    )?;
    let limits = quest_math::Limits::default();
    let mut failures = 0usize;
    for case in [
        "qft6",
        "pauli_evolution6",
        "qsvt_oracle6",
        "clifford_t6",
        "signed_controls6",
    ] {
        let original = fixture(case)?;
        for stage in ["zx_baseline", "zx_expanded", "beam_workers"] {
            for sample in 0..5 {
                let program = original.clone();
                let passed = match stage {
                    "zx_baseline" => measure_report(
                        case,
                        stage,
                        sample,
                        || Ok(program.zx_candidate(&client, 0, limits, 1, 4096)?),
                        |out| json!({"operations":out.0.schedule().len(),"certificates":out.1.accepted.len(),"window":out.1.candidate_window}),
                    ),
                    "zx_expanded" => measure_report(
                        case,
                        stage,
                        sample,
                        || Ok(program.zx_expanded_candidate_from(0, &client, 0, limits, 1, 4096)?),
                        |out| json!({"operations":out.0.schedule().len(),"certificates":out.1.accepted.len(),"window":out.1.candidate_window}),
                    ),
                    _ => measure_report(
                        case,
                        stage,
                        sample,
                        || {
                            let options = OptimizationOptions::new(
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
                                ApproximationMode::local(BigRational::new(1.into(), 8.into()))?,
                            )?;
                            Ok(
                                Optimizer::from_ideal(program, &[], options)?.search_with_workers(
                                    BeamOptions::new(2, 2, 16, 2)?,
                                    &client,
                                    0,
                                    limits,
                                )?,
                            )
                        },
                        |out| json!({"completion":format!("{:?}",out.stop_reason()),"evidence":format!("{:?}",out.evidence()),"logical_work":out.budget().work,"reserved_bytes":out.budget().retained_bytes,"search":format!("{:?}",out.search_report())}),
                    ),
                };
                failures += usize::from(!passed);
            }
        }
    }
    for qubits in [1, 2] {
        let mut operations = vec![
            quest_math::Operation {
                gate: quest_math::Gate::H,
                targets: vec![0],
                controls: vec![],
            },
            quest_math::Operation {
                gate: quest_math::Gate::T,
                targets: vec![0],
                controls: vec![],
            },
        ];
        if qubits == 2 {
            operations.push(quest_math::Operation {
                gate: quest_math::Gate::Cx,
                targets: vec![0, 1],
                controls: vec![],
            });
        }
        let target = quest_math::Sequence { qubits, operations };
        let mut search = MitmLimits::for_qubits(qubits)?;
        search.max_depth = 4;
        search.max_states = 2048;
        for sample in 0..5 {
            failures += usize::from(!measure_report(
                &format!("mitm_exact{qubits}"),
                "mitm_exact",
                sample,
                || Ok(client.exact_mitm(&target, 0, search, limits)?),
                |out| json!({"completion":status(out),"max_depth":4,"max_states":2048}),
            ));
        }
    }
    for (name, angle) in [
        (
            "mitm_dyadic",
            quest_math::AngleTarget::DyadicRadians {
                bits: 0.3f64.to_bits(),
            },
        ),
        (
            "mitm_pi",
            quest_math::AngleTarget::RationalPi {
                numerator: 1.into(),
                denominator: 37.into(),
            },
        ),
        (
            "mitm_affine",
            quest_math::AngleTarget::AffinePi {
                radians_numerator: 1.into(),
                radians_denominator: 10.into(),
                pi_numerator: 1.into(),
                pi_denominator: 37.into(),
            },
        ),
    ] {
        let target = quest_math::Target {
            axis: quest_math::Axis::Z,
            angle,
        };
        let mut search = MitmLimits::for_qubits(1)?;
        search.max_depth = 4;
        search.max_states = 2048;
        for sample in 0..5 {
            failures += usize::from(!measure_report(
                name,
                "mitm_approximate",
                sample,
                || Ok(client.approx_mitm(&target, 0.2f64.to_bits(), 0, search, limits)?),
                |out| json!({"completion":status(out),"epsilon":0.2,"max_depth":4,"max_states":2048}),
            ));
        }
    }
    if failures != 0 {
        return Err(format!("{failures} failed samples retained").into());
    }
    Ok(())
}
