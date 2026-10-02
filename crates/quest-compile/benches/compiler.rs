//! Stage timings exclude cloning immutable inputs via per-iteration setup.
use criterion::{BatchSize, Criterion};
use quest_compile::{Constructed, LanguageError, Lowered, Program, Verified, circuit};
use std::hint::black_box;
fn construct() -> Result<Program<Constructed>, LanguageError> {
    circuit! {
        qubit[4] q;
        int count = 4;
        for int i in [0:count] { h q[0]; cx q[0],q[1]; rz(0.3) q[2]; }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut criterion = Criterion::default().configure_from_args();
    let mut failure = None;
    criterion.bench_function("compiler/construction_and_admission", |b| {
        b.iter_batched(
            || (),
            |()| match construct() {
                Ok(value) => Some(black_box(value)),
                Err(error) => {
                    failure = Some(error);
                    None
                }
            },
            BatchSize::PerIteration,
        );
    });
    if let Some(error) = failure.take() {
        return Err(error.into());
    }
    let program = construct()?;
    criterion.bench_function("compiler/verification_lowering_planning", |b| {
        b.iter_batched(
            || program.clone(),
            |program| match program
                .verify()
                .and_then(quest_compile::Program::<Verified>::lower)
                .and_then(quest_compile::Program::<Lowered>::plan)
            {
                Ok(plan) => Some(black_box(plan)),
                Err(error) => {
                    failure = Some(error);
                    None
                }
            },
            BatchSize::PerIteration,
        );
    });
    if let Some(error) = failure.take() {
        return Err(error.into());
    }
    let verified = program.verify()?;
    criterion.bench_function("compiler/classical_optimization", |b| {
        b.iter_batched(
            || verified.clone(),
            |program| match program
                .optimize_classical(quest_compile::classical::OptimizationLimits::default())
            {
                Ok(value) => Some(black_box(value)),
                Err(error) => {
                    failure = Some(error);
                    None
                }
            },
            BatchSize::PerIteration,
        );
    });
    if let Some(error) = failure.take() {
        return Err(error.into());
    }
    let mut quantum_failure = None;
    criterion.bench_function("compiler/exact_quantum_optimization", |b| {
        b.iter_batched(
            || verified.clone(),
            |program| match program
                .optimize_quantum(quest_compile::StructuredQuantumOptions::default())
            {
                Ok(value) => Some(black_box(value)),
                Err(error) => {
                    quantum_failure = Some(error);
                    None
                }
            },
            BatchSize::PerIteration,
        );
    });
    if let Some(error) = quantum_failure {
        return Err(error.into());
    }
    criterion.final_summary();
    drop(criterion);
    Ok(())
}
