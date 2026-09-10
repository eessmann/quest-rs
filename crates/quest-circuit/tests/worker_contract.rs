#![cfg(feature = "workers")]
use googletest::{Result, prelude::*};
use quest_circuit::{Gate, ProgramBuilder};
use quest_math::Limits;
use quest_optimizer_client::{Client, WorkerLimits};
#[gtest]
fn explicit_synthesis_validates_epsilon_even_without_any_rotation() -> Result<()> {
    let mut builder = ProgramBuilder::new(1, 0)?;
    builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
    let program = builder.finish()?;
    let client = Client::new("/nonexistent-worker", WorkerLimits::default())?;
    for epsilon in [0.0, -1.0, 1.0, 2.0, f64::NAN, f64::INFINITY] {
        expect_true!(
            program
                .clone()
                .synthesize_rotations(&client, epsilon, 0, Limits::default())
                .is_err()
        );
    }
    let (unchanged, report) = program.synthesize_rotations(&client, 1e-12, 0, Limits::default())?;
    expect_eq!(unchanged.schedule().len(), 1);
    expect_true!(report.rotations.is_empty());
    Ok(())
}

#[gtest]
fn unchanged_worker_output_obeys_aggregate_provenance_budget() -> Result<()> {
    let mut builder = ProgramBuilder::new(1, 0)?;
    builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
    let program = builder.finish()?;
    let client = Client::new("/nonexistent-worker", WorkerLimits::default())?;
    let limits = Limits {
        bytes: 0,
        ..Limits::default()
    };
    expect_true!(program.optimize_zx(&client, 0, limits).is_err());
    Ok(())
}
