//! Scope-based native QSVT, consuming postselection and reusable Hadamard resources.
use quest::{Complex64, Environment, QubitCount};
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::{DenseEncodingBuilder, NumericalPolicy, TransformBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ANCHOR: construct
    let matrix = faer::Mat::from_fn(1, 1, |_, _| Complex64::new(0.3, 0.4));
    let encoding = DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
        .normalization(1.0)?
        .build()?;
    let phases =
        PhaseSequence::<WxSymmetric>::builder(vec![std::f64::consts::FRAC_PI_4; 2]).build()?;
    let transform = TransformBuilder::new()
        .encoding(encoding)
        .standard(phases)
        .build()?;
    // ANCHOR_END: construct
    let snapshot = {
        // ANCHOR: execute
        let environment = Environment::builder().build()?;
        let mut register =
            environment.state_vector(QubitCount::new(transform.operands().num_qubits())?)?;
        // Admission lowers/validates the immutable circuit and reserves the full
        // budget. Preparation alone allocates and transfers native resources.
        let admitted = environment.qsvt().transform(transform.clone()).admit()?;
        let mut prepared = admitted.prepare()?;
        register.init_zero()?;
        let result = prepared.run(&mut register)?;
        println!("absolute retained mass: {}", result.mass().retained());
        // A copied mass is information; only this exclusive result can condition.
        let conditioned = result.condition()?;
        let snapshot = conditioned.logical_snapshot()?;
        let _ = conditioned.release();
        // ANCHOR_END: execute
        // ANCHOR: overlap
        let mut overlap = environment
            .qsvt()
            .transform(transform)
            .overlap()
            .input(vec![Complex64::new(1.0, 0.0)])
            .reference(vec![Complex64::new(0.0, 1.0)])
            .prepare()?;
        for _ in 0..3 {
            let observed = overlap.run()?;
            println!(
                "complex overlap: {}; retained mass: {}",
                observed.overlap(),
                observed.retained_mass()
            );
        }
        // ANCHOR_END: overlap
        snapshot
    };
    // All borrowing native resources have gone; this faer snapshot is independent.
    println!("owned logical snapshot after runtime retirement: {snapshot:?}");
    Ok(())
}
