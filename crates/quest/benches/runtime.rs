//! Preparation and warm execution are distinct measurements. Failed samples abort the run.
use criterion::{BatchSize, Criterion};
use quest::{Environment, QubitCount, RunInputs, circuit};
use std::hint::black_box;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let environment = Environment::builder().build()?;
    let program = circuit! { qubit[10] q; h q[0]; cx q[0],q[9]; rz(0.17) q[4]; }?;
    let plan = program.verify()?.lower()?.plan()?;
    let mut criterion = Criterion::default().configure_from_args();
    let mut failure = None;
    criterion.bench_function("runtime/preparation", |b| {
        b.iter_batched(
            || plan.clone(),
            |plan| match environment.prepare_structured_plan(plan) {
                Ok(prepared) => Some(black_box(prepared)),
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
    let mut prepared = environment.prepare_structured_plan(plan)?;
    let mut register = environment.state_vector(QubitCount::new(10)?)?;
    let inputs = RunInputs::default();
    criterion.bench_function("runtime/repeated_state_vector_execution", |b| {
        b.iter(|| match prepared.run(&mut register, &inputs) {
            Ok(output) => {
                black_box(output);
            }
            Err(error) => failure = Some(error),
        });
    });
    if let Some(error) = failure.take() {
        return Err(error.into());
    }
    criterion.final_summary();
    drop(criterion);
    Ok(())
}
