//! Price bounded optimization using the actual register deployment.
use quest::{
    ApproximationMode, BeamOptions, CostProfile, Environment, Gate, OptimizationLimits,
    OptimizationOptions, OptimizationTarget, Optimizer, OptimizerInput, ProgramBuilder, QubitCount,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let environment = Environment::builder().build()?;
    let mut register = environment.state_vector(QubitCount::new(2)?)?;
    let mut builder = ProgramBuilder::new(2, 0)?;
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
        Optimizer::from_ideal(builder.finish()?, &[], options)?.search(BeamOptions::default())?;
    println!(
        "status={:?}, evidence={:?}, rounding={:?}, budget={:?}",
        outcome.stop_reason(),
        outcome.evidence(),
        outcome.rounding(),
        outcome.budget()
    );
    let OptimizerInput::Ideal { bound, .. } = outcome.into_input() else {
        return Err("unexpected optimizer input kind".into());
    };
    let mut prepared = environment.prepare_plan(bound.plan()?)?;
    register.init_zero()?;
    prepared.run(&mut register)?;
    println!("amplitude |01>: {:?}", register.amplitude(1)?);
    Ok(())
}
