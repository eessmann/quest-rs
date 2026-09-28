//! Run bounded optional workers with independent parent certificates.
use quest::{
    ApproximationMode, BeamOptions, CostProfile, Environment, Gate, OptimizationLimits,
    OptimizationOptions, OptimizationTarget, Optimizer, OptimizerInput, ProgramBuilder, QubitCount,
    certified, optimizer,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("pass the optimizer worker executable path")?;
    let client = optimizer::Client::new(path, optimizer::WorkerLimits::default())?;
    let environment = Environment::builder().build()?;
    let mut register = environment.state_vector(QubitCount::new(1)?)?;
    let mut builder = ProgramBuilder::new(1, 0)?;
    let q = builder.qubit(0)?;
    for gate in [Gate::H, Gate::X, Gate::H] {
        builder.gate(gate, &[q], &[])?;
    }
    let options = OptimizationOptions::new(
        OptimizationTarget::once(
            register.deployment().compiler_snapshot()?,
            CostProfile::NativeV1,
        )?,
        OptimizationLimits::default(),
        ApproximationMode::Disabled,
    )?;
    let outcome = Optimizer::from_ideal(builder.finish()?, &[], options)?.search_with_workers(
        BeamOptions::new(1, 1, 8, 2)?,
        &client,
        0,
        certified::Limits::default(),
    )?;
    println!(
        "status={:?}, evidence={:?}, rounding={:?}",
        outcome.stop_reason(),
        outcome.evidence(),
        outcome.rounding()
    );
    if let Some(report) = outcome.search_report() {
        println!(
            "request slots={}, exact certificates={}, MITM={:?}",
            report.worker_requests(),
            report.exact_regions().len(),
            report.exact_mitm_statuses()
        );
    }
    let OptimizerInput::Ideal { bound, .. } = outcome.into_input() else {
        return Err("unexpected optimizer input kind".into());
    };
    let mut prepared = environment.prepare_plan(bound.plan()?)?;
    register.init_plus()?;
    prepared.run(&mut register)?;
    println!(
        "amplitudes: {:?}, {:?}",
        register.amplitude(0)?,
        register.amplitude(1)?
    );
    Ok(())
}
