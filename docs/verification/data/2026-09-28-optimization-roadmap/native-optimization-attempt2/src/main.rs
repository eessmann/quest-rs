//! Synchronized native measurements; six active wires with equal SV/DM storage sizes.
#[allow(dead_code)]
#[path = "baseline_compiler.rs"]
mod baseline;
use baseline::{Error, measure_keep};
use quest::*;
use serde_json::json;
use std::result::Result;

#[derive(Clone)]
enum Plan {
    Ideal(ExecutablePlan),
    Structured(StructuredPlan),
}
enum Prepared<'a> {
    Ideal(PreparedProgram<'a>),
    Structured(PreparedStructuredProgram<'a>),
}
impl Plan {
    fn prepare<'a>(&self, env: &'a Environment) -> Result<Prepared<'a>, Error> {
        Ok(match self {
            Self::Ideal(plan) => Prepared::Ideal(env.prepare_plan(plan.clone())?),
            Self::Structured(plan) => {
                Prepared::Structured(env.prepare_structured_plan(plan.clone())?)
            }
        })
    }
}
impl Prepared<'_> {
    fn run<K: RegisterKind>(
        &mut self,
        register: &mut Register<'_, K>,
        inputs: &RunInputs,
    ) -> Result<(), Error> {
        match self {
            Self::Ideal(program) => {
                program.run(register)?;
            }
            Self::Structured(program) => {
                program.run(register, inputs)?;
            }
        }
        Ok(())
    }
}
trait Snapshot {
    fn snapshot_values(&self) -> Result<Vec<Complex64>, Error>;
}
impl Snapshot for Register<'_, StateVector> {
    fn snapshot_values(&self) -> Result<Vec<Complex64>, Error> {
        Ok(self.amplitudes(0, 1 << self.num_qubits().get())?)
    }
}
impl Snapshot for Register<'_, DensityMatrix> {
    fn snapshot_values(&self) -> Result<Vec<Complex64>, Error> {
        let matrix = self.snapshot()?;
        Ok((0..matrix.nrows())
            .flat_map(|r| (0..matrix.ncols()).map(move |c| (r, c)))
            .map(|(r, c)| matrix[(r, c)])
            .collect())
    }
}
fn settings(deployment: DeploymentSnapshot, reuse: i64) -> Result<OptimizationOptions, Error> {
    Ok(OptimizationOptions::new(
        OptimizationTarget::new(
            deployment,
            BigRational::from_integer(reuse.into()),
            CostProfile::NativeV1,
        )?,
        OptimizationLimits::default(),
        ApproximationMode::Disabled,
    )?)
}
fn selected(out: OptimizationOutcome) -> Result<Plan, Error> {
    Ok(match out.into_input() {
        OptimizerInput::Ideal { bound, .. } | OptimizerInput::Bound(bound) => {
            Plan::Ideal(bound.plan()?)
        }
        OptimizerInput::VerifiedStructured(program) => Plan::Structured(program.lower()?.plan()?),
    })
}
fn build(
    case: &str,
    stage: &str,
    width: usize,
    deployment: DeploymentSnapshot,
) -> Result<(Plan, String), Error> {
    let options = settings(
        deployment,
        if stage == "terminal_reuse100" { 100 } else { 1 },
    )?;
    if case == "branch_heavy6" {
        let source = format!(
            "input bool flag; qubit[{width}] q; int i=0; while(i<3) {{ {} i+=1; }}",
            "if(flag){h q[0];x q[2];h q[0];}else{cx q[1],q[3];cx q[1],q[3];} ".repeat(32)
        );
        let program = StructuredProgram::parse(&source, "native-corpus.qasm")?.verify()?;
        match stage {
            "existing_combined" => Ok((
                Plan::Structured(
                    program
                        .optimize_classical(
                            language::ssa::optimization::OptimizationLimits::default(),
                        )?
                        .0
                        .optimize_quantum(StructuredQuantumOptions::default())?
                        .0
                        .lower()?
                        .plan()?,
                ),
                "Complete".into(),
            )),
            "terminal" | "terminal_reuse100" => {
                let out = program.fuse_terminal(
                    TerminalOptions::default(),
                    &options,
                    &BudgetLedger::new(options.limits()),
                )?;
                let status = format!("{:?}", out.report().status());
                Ok((
                    Plan::Structured(out.into_program().lower()?.plan()?),
                    status,
                ))
            }
            "beam" => {
                let out = Optimizer::from_verified_structured(program, options)?
                    .search(BeamOptions::default())?;
                let status = format!("{:?}", out.stop_reason());
                Ok((selected(out)?, status))
            }
            _ => Ok((
                Plan::Structured(program.lower()?.plan()?),
                "Complete".into(),
            )),
        }
    } else {
        let program = baseline::fixture_with_width(case, width)?;
        match stage {
            "existing_combined" => Ok((
                Plan::Ideal(
                    program
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
                ),
                "Complete".into(),
            )),
            "terminal" | "terminal_reuse100" => {
                let out = program.bind(&[])?.schedule_and_fuse(
                    TerminalOptions::default(),
                    options.target(),
                    &BudgetLedger::new(options.limits()),
                )?;
                let status = format!("{:?}", out.report().status());
                Ok((Plan::Ideal(out.into_program().plan()?), status))
            }
            "beam" => {
                let out =
                    Optimizer::from_ideal(program, &[], options)?.search(BeamOptions::default())?;
                let status = format!("{:?}", out.stop_reason());
                Ok((selected(out)?, status))
            }
            _ => Ok((Plan::Ideal(program.bind(&[])?.plan()?), "Complete".into())),
        }
    }
}
fn run<K: RegisterKind>(
    env: &Environment,
    register: &mut Register<'_, K>,
    mode: &str,
    kind: &str,
) -> Result<(), Error>
where
    for<'a> Register<'a, K>: Snapshot,
{
    let deployment = register.deployment();
    if deployment.is_gpu_accelerated() != (mode == "gpu")
        || deployment.is_multithreaded() != (mode == "omp")
    {
        return Err(format!("requested native mode not selected: {deployment:?}").into());
    }
    println!(
        "{}",
        json!({"metadata":"actual allocated native register", "deployment":format!("{deployment:?}"),"mode":mode,"kind":kind,"warm_batch":10,"samples":5,"active_wires":6})
    );
    let mut inputs = RunInputs::default();
    inputs.insert(
        "flag",
        ClassicalValue::Scalar(language::classical::ScalarValue::boolean(true)),
    )?;
    let mut failures = 0;
    for case in [
        "qft6",
        "pauli_evolution6",
        "qsvt_oracle6",
        "clifford_t6",
        "signed_controls6",
        "branch_heavy6",
    ] {
        let mut references = Vec::new();
        for stage in [
            "unchanged",
            "existing_combined",
            "terminal",
            "terminal_reuse100",
            "beam",
        ] {
            let result = (|| -> Result<(), Error> {
                let (plan, completion) = measure_keep(
                    case,
                    &format!("{stage}/search"),
                    0,
                    || {
                        build(
                            case,
                            stage,
                            deployment.width(),
                            deployment.compiler_snapshot()?,
                        )
                    },
                    |(_, status)| json!({"completion":status}),
                )?;
                for sample in 0..5 {
                    let before = env.allocated_bytes();
                    let prepared = measure_keep(
                        case,
                        &format!("{stage}/preparation"),
                        sample,
                        || plan.prepare(env),
                        |_| json!({"native_admitted_bytes":env.allocated_bytes().saturating_sub(before),"completion":completion}),
                    )?;
                    drop(prepared);
                }
                let mut prepared = plan.prepare(env)?;
                for input in 0..2 {
                    if input == 0 {
                        register.init_zero()?;
                    } else {
                        register.init_plus()?;
                    }
                    prepared.run(register, &inputs)?;
                    quest_sys::sync_quest_env()?;
                    let actual = register.snapshot_values()?;
                    if stage == "unchanged" {
                        references.push(actual);
                    } else {
                        let expected = references.get(input).ok_or("baseline failed")?;
                        let max = actual
                            .iter()
                            .zip(expected)
                            .map(|(a, b)| (*a - *b).norm())
                            .fold(0.0, f64::max);
                        if actual.len() != expected.len() || max > 1e-10 {
                            return Err(format!("complex output mismatch {max}").into());
                        }
                    }
                }
                register.init_plus()?;
                prepared.run(register, &inputs)?;
                quest_sys::sync_quest_env()?;
                for sample in 0..5 {
                    measure_keep(
                        case,
                        &format!("{stage}/warm_execution"),
                        sample,
                        || {
                            for _ in 0..10 {
                                prepared.run(register, &inputs)?;
                            }
                            quest_sys::sync_quest_env()?;
                            Ok(())
                        },
                        |_| json!({"executions":10,"completion":completion}),
                    )?;
                }
                let probability = register.total_probability()?;
                if (probability - 1.0).abs() > 1e-9 {
                    return Err(format!("probability drift {probability}").into());
                }
                Ok(())
            })();
            if let Err(error) = result {
                failures += 1;
                println!(
                    "{}",
                    json!({"case":case,"stage":stage,"status":"failed","error":error.to_string()})
                );
            }
        }
    }
    if failures > 0 {
        return Err(format!("{failures} stage failures retained").into());
    }
    Ok(())
}
fn main() -> Result<(), Error> {
    let mode = std::env::args().nth(1).ok_or("mode")?;
    let kind = std::env::args().nth(2).ok_or("kind")?;
    let env = Environment::builder()
        .gpu(if mode == "gpu" {
            ExecutionMode::Enabled
        } else {
            ExecutionMode::Disabled
        })
        .multithreading(if mode == "omp" {
            ExecutionMode::Enabled
        } else {
            ExecutionMode::Disabled
        })
        .build()?;
    match kind.as_str() {
        "sv" => run(
            &env,
            &mut env.state_vector(QubitCount::new(12)?)?,
            &mode,
            &kind,
        ),
        "dm" => run(
            &env,
            &mut env.density_matrix(QubitCount::new(6)?)?,
            &mode,
            &kind,
        ),
        _ => Err("kind must be sv/dm".into()),
    }
}
