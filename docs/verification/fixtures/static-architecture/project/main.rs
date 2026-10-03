//! Matched whole-project workloads. All correctness checks precede measured loops.
include!("../allocator.rs");
use std::hint::black_box;
type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;

fn qsp() -> AnyResult<()> {
    use quest_polynomial::{Laurent, Limits, Polynomial};
    use quest_qsp::{Complex64, Policy, SynthesisAlgorithm, SynthesisBuilder};
    header()?;
    for degree in [256_i32, 1024] {
        let denominator = f64::from(degree) * 4.0;
        let coefficients = (0..=degree)
            .map(|k| {
                Complex64::new(
                    f64::from(k.rem_euclid(3) - 1) / denominator,
                    f64::from(k.rem_euclid(5) - 2) / (2.0 * denominator),
                )
            })
            .collect::<Vec<_>>();
        let target = Polynomial::new(Laurent::new(0), coefficients.clone(), Limits::default())?;
        let policy = Policy {
            algorithm: SynthesisAlgorithm::InverseNlftDivideConquer,
            response_tolerance: 1e-11,
            contractivity_margin: 1e-12,
            max_completion_grid: 1_048_576,
            ..Policy::default()
        };
        let admitted = SynthesisBuilder::new()
            .unit_circle_response(&target)?
            .policy(policy)
            .admit()?;
        let candidate = admitted.clone().complete()?.synthesize()?;
        assert_eq!(candidate.source_storage(), (0, coefficients.len()));
        assert_eq!(
            candidate.algorithm(),
            SynthesisAlgorithm::InverseNlftDivideConquer
        );
        assert!(
            candidate
                .reconstruction_residual()
                .is_some_and(|r| r <= 1e-11)
        );
        for theta in [-2.4, -0.3, 0.0, 1.7] {
            let signal = Complex64::from_polar(1.0, theta);
            let mut power = Complex64::new(1.0, 0.0);
            let mut expected = Complex64::new(0.0, 0.0);
            for coefficient in &coefficients {
                expected += coefficient * power;
                power *= signal;
            }
            let [[actual, _], _] = candidate.evaluate(signal)?;
            assert!(
                (actual - expected).norm() <= 2e-10,
                "unit-circle response mismatch"
            );
        }
        eprintln!(
            "qsp degree={degree} source_count={} controls={} grid={} completion_residual={:.17e} reconstruction_residual={:.17e} tolerance=1e-11 margin=1e-12 algorithm=inverse-nlft-divide-conquer",
            coefficients.len(),
            candidate.controls().len(),
            candidate.completion_grid(),
            candidate.completion_residual(),
            candidate
                .reconstruction_residual()
                .ok_or("missing reconstruction diagnostic")?
        );
        let iterations = if degree == 256 { 12 } else { 4 };
        measure(
            &format!("qsp_complete_inverse_nlft_degree{degree}"),
            iterations,
            || {
                let frozen = black_box(admitted.clone()).complete()?.synthesize()?;
                Ok(black_box(frozen.controls().len()))
            },
        )?;
    }
    Ok(())
}

fn construct()
-> Result<quest_compile::Program<quest_compile::Constructed>, quest_compile::LanguageError> {
    quest_compile::circuit! {
        qubit[4] q;
        int count = 63;
        for int i in [0:count] { h q[0]; cx q[0],q[1]; rz(0.3) q[2]; }
    }
}
fn finite_source() -> AnyResult<quest_compile::QuantumRegion> {
    use quest_compile::{Angle, Gate, QuantumRegionBuilder};
    let mut builder = QuantumRegionBuilder::new(4, 0)?;
    for _ in 0..128 {
        builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
        builder.gate(Gate::Rz(Angle::pi(1, 7)?), &[builder.qubit(2)?], &[])?;
    }
    Ok(builder.finish()?)
}
fn compiler() -> AnyResult<()> {
    use quest_compile::{Constructed, Program};
    header()?;
    let constructed = construct()?;
    let plan = constructed.clone().verify()?.lower()?.plan()?;
    assert_eq!(plan.num_qubits(), 4);
    let count = plan
        .ssa()
        .blocks()
        .iter()
        .map(|b| b.instructions.len())
        .sum::<usize>();
    assert!(count > 0);
    eprintln!("compiler structured_width=4 loop_iterations=64 ssa_instructions={count}");
    measure("compiler_construction_admission", 128, || {
        let p = black_box(construct()?);
        black_box(p);
        Ok(4)
    })?;
    measure("compiler_verify_lower_plan", 128, || {
        let p = black_box(constructed.clone()).verify()?.lower()?.plan()?;
        Ok(black_box(
            p.ssa().blocks().iter().map(|b| b.instructions.len()).sum(),
        ))
    })?;
    let source = finite_source()?;
    let finite = Program::<Constructed>::from_region(source.clone(), &[])?
        .verify()?
        .lower()?
        .plan()?;
    assert_eq!(finite.captures().len(), 128);
    assert_eq!(finite.exact_captures().len(), 128);
    eprintln!("compiler finite_width=4 finite_operations=256 captures=128 exact_captures=128");
    measure(
        "compiler_finite_insert_verify_lower_plan_256ops",
        64,
        || {
            let p = Program::<Constructed>::from_region(black_box(source.clone()), &[])?
                .verify()?
                .lower()?
                .plan()?;
            Ok(black_box(p.captures().len()))
        },
    )?;
    Ok(())
}

fn native() -> AnyResult<()> {
    use quest::{Complex64, Environment, QubitCount, RunInputs};
    use quest_compile::{
        Angle, Constructed, Control, ControlState, Gate, MatrixPolicy, NumericalOperator,
        OracleFragment, Program, QuantumRegionBuilder,
    };
    let raw = faer::Mat::from_fn(2, 2, |r, c| {
        Complex64::new(if r == c { 0.0 } else { 1.0 }, 0.0)
    });
    let matrix = NumericalOperator::from_view(&raw, MatrixPolicy::default())?;
    let mut body = QuantumRegionBuilder::new(1, 0)?;
    body.numerical(matrix.clone(), &[body.qubit(0)?], &[])?;
    let oracle = OracleFragment::builder(body.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let mut builder = QuantumRegionBuilder::new(10, 0)?;
    builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
    builder.gate(
        Gate::X,
        &[builder.qubit(9)?],
        &[Control::new(builder.qubit(0)?, ControlState::One)],
    )?;
    for _ in 0..64 {
        for controls in [
            vec![],
            vec![Control::new(builder.qubit(7)?, ControlState::Zero)],
        ] {
            let targets = [builder.qubit(4)?];
            builder.numerical(matrix.clone(), &targets, &controls)?;
            builder.oracle(&oracle, &targets, &controls)?;
        }
    }
    builder.gate(Gate::Rz(Angle::radians(0.17)?), &[builder.qubit(4)?], &[])?;
    let plan = Program::<Constructed>::from_region(builder.finish()?, &[])?
        .verify()?
        .lower()?
        .plan()?;
    let environment = Environment::builder().build()?;
    let mut prepared = environment.prepare(plan.clone())?;
    let admitted_bytes = environment.allocated_bytes();
    let mut register = environment.state_vector(QubitCount::new(10)?)?;
    register.init_zero()?;
    prepared.run(&mut register, &RunInputs::default())?;
    let expected = Complex64::from_polar(std::f64::consts::FRAC_1_SQRT_2, -0.085);
    for index in 0..1024 {
        let value = if index == 0 || index == 513 {
            expected
        } else {
            Complex64::new(0.0, 0.0)
        };
        assert!(
            (register.amplitude(index)? - value).norm() <= 1e-12,
            "native Bell/full-phase mismatch"
        );
    }
    eprintln!(
        "native CPU width=10 operations=259 signed_matrix_profiles=2 alias_pairs=128 environment_prepared_admitted_bytes={admitted_bytes}; Rust allocator excludes native C++ allocations"
    );
    header()?;
    measure("native_shared_matrix_oracle_preparation", 128, || {
        let p = environment.prepare(black_box(plan.clone()))?;
        Ok(black_box(p.plan().num_qubits()))
    })?;
    measure("native_shared_matrix_oracle_zeroed_execution", 128, || {
        register.init_zero()?;
        let output = prepared.run(&mut register, &RunInputs::default())?;
        black_box(output);
        Ok(1024)
    })?;
    Ok(())
}
fn main() -> AnyResult<()> {
    match std::env::args().nth(1).as_deref() {
        Some("qsp") => qsp(),
        Some("compiler") => compiler(),
        Some("native") => native(),
        _ => Err("expected qsp, compiler, or native workload argument".into()),
    }
}
