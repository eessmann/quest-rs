use googletest::{Result, prelude::*};
use quest::{
    Complex64, Control, ControlState, Environment, MatrixPolicy, NumericalOperator,
    QuantumRegionBuilder, QubitCount,
};

#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Independent 16-entry finite diagonal and outer-product oracle with explicit residual tolerance"
)]
fn diagonal_cache_preserves_order_signed_controls_and_density_adjoint() -> Result<()> {
    const NAME: &str = "diagonal_cache_preserves_order_signed_controls_and_density_adjoint";
    if std::env::var("QUEST_DIAGONAL_REVIEW").as_deref() != Ok(NAME) {
        let status = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env("QUEST_DIAGONAL_REVIEW", NAME)
            .status()?;
        expect_true!(status.success());
        return Ok(());
    }
    let diagonal = [
        Complex64::new(2.0, 0.0),
        Complex64::new(0.0, 1.0),
        Complex64::new(-0.5, 0.0),
        Complex64::new(1.0, 1.0),
    ];
    let matrix = faer::Mat::from_fn(4, 4, |r, c| {
        if r == c {
            diagonal.get(r).copied().unwrap_or_default()
        } else {
            Complex64::default()
        }
    });
    let operator = NumericalOperator::from_view(matrix.as_ref(), MatrixPolicy::default())?;
    let mut builder = QuantumRegionBuilder::new(4, 0)?;
    builder.numerical(
        operator.clone(),
        &[builder.qubit(2)?, builder.qubit(0)?],
        &[
            Control::new(builder.qubit(1)?, ControlState::Zero),
            Control::new(builder.qubit(3)?, ControlState::One),
        ],
    )?;
    // The shared payload/cache is mapped through a different ordered interface.
    builder.numerical(
        operator,
        &[builder.qubit(0)?, builder.qubit(2)?],
        &[
            Control::new(builder.qubit(3)?, ControlState::Zero),
            Control::new(builder.qubit(1)?, ControlState::One),
        ],
    )?;
    let environment = Environment::builder().build()?;
    let mut prepared = environment.prepare(
        quest::Program::from_region(builder.finish()?, &[])?
            .verify()?
            .lower()?
            .plan()?,
    )?;
    let initial = vec![Complex64::new(0.25, 0.0); 16];
    let mut expected = initial.clone();
    for (basis, value) in expected.iter_mut().enumerate() {
        let local = if basis & 0b1010 == 0b1000 {
            Some(((basis >> 2) & 1) | ((basis & 1) << 1))
        } else if basis & 0b1010 == 0b0010 {
            Some((basis & 1) | ((basis >> 1) & 2))
        } else {
            None
        };
        if let Some(local) = local {
            *value *= diagonal
                .get(local)
                .ok_or_else(|| std::io::Error::other("oracle local index"))?;
        }
    }
    let mut state = environment.state_vector(QubitCount::new(4)?)?;
    let mut density = environment.density_matrix(QubitCount::new(4)?)?;
    state.init_pure(&initial)?;
    density.init_pure(&initial)?;
    prepared.run(&mut state, &quest::RunInputs::default())?;
    prepared.run(&mut density, &quest::RunInputs::default())?;
    for (row, left) in expected.iter().enumerate() {
        expect_lt!((state.amplitude(row)? - left).norm(), 1e-14);
        for (column, right) in expected.iter().enumerate() {
            expect_lt!(
                (density.entry(row, column)? - left * right.conj()).norm(),
                1e-14
            );
        }
    }
    Ok(())
}
