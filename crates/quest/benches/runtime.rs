//! Preparation and warm execution are distinct measurements. Failed samples abort the run.
use criterion::{BatchSize, Criterion};
use quest::{Environment, QubitCount, RunInputs, circuit};
use std::hint::black_box;
#[expect(
    clippy::panic,
    reason = "A failed sample must stop the benchmark immediately"
)]
fn checked<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("Runtime benchmark failed: {error}"),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let environment = Environment::builder().build()?;
    let program = circuit! { qubit[10] q; h q[0]; cx q[0],q[9]; rz(0.17) q[4]; }?;
    let plan = program.verify()?.lower()?.plan()?;
    let mut criterion = Criterion::default().configure_from_args();
    criterion.bench_function("runtime/preparation", |b| {
        b.iter_batched(
            || plan.clone(),
            |plan| black_box(checked(environment.prepare_structured_plan(plan))),
            BatchSize::PerIteration,
        );
    });
    let mut prepared = environment.prepare_structured_plan(plan)?;
    let mut register = environment.state_vector(QubitCount::new(10)?)?;
    let inputs = RunInputs::default();
    criterion.bench_function("runtime/repeated_state_vector_execution", |b| {
        b.iter(|| {
            black_box(checked(prepared.run(&mut register, &inputs)));
        });
    });
    criterion.final_summary();
    drop(criterion);
    Ok(())
}
