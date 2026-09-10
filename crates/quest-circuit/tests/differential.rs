use googletest::{Result, prelude::*};
use num_complex::Complex64 as C;
use proptest::{
    prelude::*,
    test_runner::{Config, RngAlgorithm, TestRng, TestRunner},
};
use quest_circuit::*;

// Independent scalar oracle. The tested faer realization/product code is never
// used for the original gate path; basis wiring and amplitudes are explicit.
fn scalar_gate(gate: &BoundGate) -> Vec<Vec<C>> {
    let z = C::new(0.0, 0.0);
    let one = C::new(1.0, 0.0);
    let i = C::new(0.0, 1.0);
    match gate {
        BoundGate::X => vec![vec![z, one], vec![one, z]],
        BoundGate::Y => vec![vec![z, -i], vec![i, z]],
        BoundGate::Z => vec![vec![one, z], vec![z, -one]],
        BoundGate::H => {
            let h = one / 2.0f64.sqrt();
            vec![vec![h, h], vec![h, -h]]
        }
        BoundGate::S => vec![vec![one, z], vec![z, i]],
        BoundGate::Sdg => vec![vec![one, z], vec![z, -i]],
        BoundGate::Rx(a) => {
            let c = C::new((a / 2.0).cos(), 0.0);
            let s = -i * (a / 2.0).sin();
            vec![vec![c, s], vec![s, c]]
        }
        BoundGate::Rz(a) => vec![
            vec![(-i * (a / 2.0)).exp(), z],
            vec![z, (i * (a / 2.0)).exp()],
        ],
        BoundGate::Phase(a) => vec![vec![one, z], vec![z, (i * a).exp()]],
        other => panic!("fixture does not support {other:?}"),
    }
}
fn scalar_run(plan: &ExecutablePlan, mut state: Vec<C>) -> Vec<C> {
    for instruction in plan.instructions() {
        let (targets, controls, matrix) = match instruction.operation() {
            Operation::Gate {
                gate,
                targets,
                controls,
            } => (targets, controls, scalar_gate(gate)),
            Operation::Numerical {
                matrix,
                targets,
                controls,
            } => (
                targets,
                controls,
                (0..matrix.dimension())
                    .map(|r| {
                        (0..matrix.dimension())
                            .map(|c| matrix.view()[(r, c)])
                            .collect()
                    })
                    .collect(),
            ),
            Operation::GlobalPhase { radians, controls } => {
                for (index, x) in state.iter_mut().enumerate() {
                    if controls.iter().all(|c| {
                        ((index >> c.qubit().index()) & 1)
                            == usize::from(c.state() == ControlState::One)
                    }) {
                        *x *= C::new(0.0, *radians).exp();
                    }
                }
                continue;
            }
            Operation::Barrier { .. } => continue,
            _ => panic!("effect in unitary fixture"),
        };
        let mask = targets.iter().fold(0usize, |m, q| m | (1 << q.index()));
        for base in 0..state.len() {
            if base & mask != 0
                || !controls.iter().all(|c| {
                    ((base >> c.qubit().index()) & 1) == usize::from(c.state() == ControlState::One)
                })
            {
                continue;
            }
            let indices = (0..matrix.len())
                .map(|local| {
                    targets.iter().enumerate().fold(base, |index, (bit, q)| {
                        index | (((local >> bit) & 1) << q.index())
                    })
                })
                .collect::<Vec<_>>();
            let input = indices
                .iter()
                .map(|&index| state[index])
                .collect::<Vec<_>>();
            for (row, &index) in indices.iter().enumerate() {
                state[index] = matrix[row].iter().zip(&input).map(|(m, x)| m * x).sum();
            }
        }
    }
    state
}

#[gtest]
fn randomized_exact_pass_and_fusion_preserve_complex_state_including_phase() -> Result<()> {
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: 96,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    let cases = proptest::collection::vec((0u8..9, -8i16..9, 0usize..3, any::<bool>()), 0..40);
    let result = runner.run(&cases, |gates| {
        let mut builder = ProgramBuilder::new(3, 0).unwrap();
        for (kind, n, target, controlled) in gates {
            let gate = match kind {
                0 => Gate::X,
                1 => Gate::Y,
                2 => Gate::Z,
                3 => Gate::H,
                4 => Gate::S,
                5 => Gate::Sdg,
                6 => Gate::Rx(Angle::pi(n.into(), 4).unwrap()),
                7 => Gate::Rz(Angle::pi(n.into(), 4).unwrap()),
                _ => Gate::Phase(Angle::pi(n.into(), 4).unwrap()),
            };
            let q = builder.qubit(target).unwrap();
            let controls = if controlled {
                vec![Control::new(
                    builder.qubit((target + 1) % 3).unwrap(),
                    ControlState::Zero,
                )]
            } else {
                vec![]
            };
            builder.gate(gate, &[q], &controls).unwrap();
        }
        let p = builder.finish().unwrap();
        let original = p
            .clone()
            .bind(&[])
            .unwrap()
            .lower()
            .unwrap()
            .plan()
            .unwrap();
        let optimized = p
            .optimize_exact()
            .unwrap()
            .0
            .bind(&[])
            .unwrap()
            .fuse(FusionOptions::default())
            .unwrap()
            .0
            .lower()
            .unwrap()
            .plan()
            .unwrap();
        let state = (0..8)
            .map(|i| C::new((i + 1) as f64 / 11.0, (3 * i + 1) as f64 / 19.0))
            .collect::<Vec<_>>();
        let a = scalar_run(&original, state.clone());
        let b = scalar_run(&optimized, state);
        for (a, b) in a.iter().zip(&b) {
            prop_assert!((*a - *b).norm() < 2e-12, "{a:?} != {b:?}");
        }
        Ok(())
    });
    expect_true!(result.is_ok(), "{result:?}");
    Ok(())
}

#[gtest]
fn controlling_a_global_phase_changes_only_the_active_branch() -> Result<()> {
    let mut builder = ProgramBuilder::new(2, 0)?;
    let control = builder.qubit(1)?;
    builder.global_phase(Angle::pi(1, 2)?, &[])?;
    let plan = builder
        .finish()?
        .into_unitary()?
        .controlled(&[Control::new(control, ControlState::One)])?
        .into_program()
        .bind(&[])?
        .lower()?
        .plan()?;
    let state = vec![C::new(1.0, 0.0); 4];
    let result = scalar_run(&plan, state);
    expect_eq!(result[0], C::new(1.0, 0.0));
    expect_eq!(result[1], C::new(1.0, 0.0));
    expect_lt!((result[2] - C::new(0.0, 1.0)).norm(), 1e-15);
    expect_lt!((result[3] - C::new(0.0, 1.0)).norm(), 1e-15);
    Ok(())
}

#[gtest]
fn numerical_approximate_unitarity_does_not_grant_exact_capability() -> Result<()> {
    let matrix = NumericalOperator::from_view(
        faer::Mat::from_fn(2, 2, |r, c| {
            C::new(if r == c { 1.0 + 1e-9 } else { 0.0 }, 0.0)
        })
        .as_ref(),
        MatrixPolicy::default(),
    )?;
    expect_true!(matrix.check_unitary(1e-7, MatrixPolicy::default()).is_ok());
    let mut b = ProgramBuilder::new(1, 0)?;
    b.numerical(matrix, &[b.qubit(0)?], &[])?;
    expect_true!(b.finish()?.into_unitary().is_err());
    Ok(())
}

#[gtest]
fn channels_are_effectful_and_completeness_is_checked() -> Result<()> {
    let identity = BoundGate::Id.matrix(MatrixPolicy::default())?;
    let mut b = ProgramBuilder::new(1, 0)?;
    let q = b.qubit(0)?;
    expect_true!(
        b.channel(vec![identity.clone(), identity.clone()], &[q], 1e-12)
            .is_err()
    );
    b.gate(Gate::X, &[q], &[])?;
    b.channel(vec![identity], &[q], 1e-12)?;
    b.gate(Gate::X, &[q], &[])?;
    let (p, _) = b.finish()?.optimize_exact()?;
    expect_eq!(p.schedule().len(), 3);
    let (bound, _) = p.bind(&[])?.fuse(FusionOptions::default())?;
    expect_eq!(bound.instructions().len(), 3);
    expect_true!(bound.lower()?.plan()?.requires_density_matrix());
    Ok(())
}
