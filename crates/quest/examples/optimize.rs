//! Price bounded optimization using the actual register deployment.
use quest::{
    ApproximationMode, BeamOptions, CostProfile, Environment, Gate, OptimizationLimits,
    OptimizationOptions, OptimizationTarget, Optimizer, OptimizerInput, QuantumRegionBuilder,
    QubitCount,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let environment = Environment::builder().build()?;
    let mut register = environment.state_vector(QubitCount::new(2)?)?;
    let mut builder = QuantumRegionBuilder::new(2, 0)?;
    let q = builder.qubit(0)?;
    builder.gate(Gate::H, &[q], &[])?;
    builder.gate(Gate::H, &[q], &[])?;
    builder.gate(Gate::X, &[q], &[])?;
    let options = OptimizationOptions::new(
        OptimizationTarget::once(
            register.deployment().compiler_snapshot()?,
            CostProfile::NativeV1,
        )?,
        OptimizationLimits::default(),
        ApproximationMode::Disabled,
    )?;
    let outcome =
        Optimizer::from_region(builder.finish()?, &[], options)?.search(BeamOptions::default())?;
    println!(
        "status={:?}, evidence={:?}, rounding={:?}, budget={:?}",
        outcome.stop_reason(),
        outcome.evidence(),
        outcome.rounding(),
        outcome.budget()
    );
    let OptimizerInput::Region { bound, .. } = outcome.into_input() else {
        return Err("unexpected optimizer input kind".into());
    };
    let mut prepared = environment.prepare(
        quest::Program::from_bound_region((bound.plan()?).into_region())?
            .verify()?
            .lower()?
            .plan()?,
    )?;
    register.init_zero()?;
    prepared.run(&mut register, &quest::RunInputs::default())?;
    println!("amplitude |01>: {:?}", register.amplitude(1)?);
    Ok(())
}
