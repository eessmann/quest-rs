//! Reproducible stage receipt; no optimizer or discrete synthesis is implicit.
#![allow(clippy::arithmetic_side_effects)] // Fixed modest benchmark sizes and counters.
use quest::{Environment, Program, QubitCount, RunInputs};
use std::{fmt::Write, hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let qubits: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "8".into())
        .parse()?;
    if ![4, 8, 12].contains(&qubits) {
        return Err("qubits must be 4, 8, or 12".into());
    }
    let layers = qubits * 2;
    let repetitions = 100usize;
    let mut source = format!("qubit[{qubits}] q;\n");
    for _ in 0..layers {
        for q in 0..qubits {
            writeln!(
                source,
                "h q[{q}]; rz(0.17) q[{q}]; cx q[{q}],q[{}];",
                (q + 1) % qubits
            )?;
        }
    }
    let started = Instant::now();
    let plan = Program::parse(&source, "architecture-runtime.qasm")?
        .verify()?
        .lower()?
        .plan()?;
    let compilation_ns = started.elapsed().as_nanos();
    let modeled_plan_bytes = plan.resources().ir_bytes + plan.resources().source_bytes;
    let environment = Environment::builder().build()?;
    let mut register = environment.state_vector(QubitCount::new(qubits)?)?;
    let register_bytes = environment.allocated_bytes();
    let started = Instant::now();
    let mut prepared = environment.prepare(plan.clone())?;
    let preparation_ns = started.elapsed().as_nanos();
    let prepared_bytes = environment.allocated_bytes() - register_bytes;
    let native_prepared_gates = prepared.prepared_static_gates();
    let run_storage_cap = quest::InterpreterLimits::default().storage_bytes;
    let inputs = RunInputs::default();
    let mut execution_ns = 0u128;
    for _ in 0..repetitions {
        register.init_zero()?;
        let started = Instant::now();
        black_box(prepared.run(&mut register, &inputs)?);
        execution_ns += started.elapsed().as_nanos();
    }
    let warm = register.amplitudes(0, 1usize << qubits)?;
    drop(prepared);
    let mut prepare_per_run_ns = 0u128;
    for _ in 0..repetitions {
        register.init_zero()?;
        let started = Instant::now();
        let mut prepared = environment.prepare(plan.clone())?;
        black_box(prepared.run(&mut register, &inputs)?);
        drop(prepared);
        prepare_per_run_ns += started.elapsed().as_nanos();
    }
    let cold = register.amplitudes(0, 1usize << qubits)?;
    let max_difference = warm
        .iter()
        .zip(&cold)
        .map(|(a, b)| (*a - *b).norm())
        .fold(0.0_f64, f64::max);
    if max_difference != 0.0 {
        return Err("prepare-once and prepare-per-run outputs differ".into());
    }
    println!(
        "qubits,layers,gates,native_prepared_gates,repetitions,compilation_ns,preparation_ns,execution_total_ns,prepare_per_run_total_ns,modeled_plan_bytes,modeled_prepared_bytes,modeled_register_bytes,run_storage_cap_bytes,max_output_difference"
    );
    println!(
        "{qubits},{layers},{},{native_prepared_gates},{repetitions},{compilation_ns},{preparation_ns},{execution_ns},{prepare_per_run_ns},{modeled_plan_bytes},{prepared_bytes},{register_bytes},{run_storage_cap},{max_difference}",
        3 * qubits * layers
    );
    Ok(())
}
