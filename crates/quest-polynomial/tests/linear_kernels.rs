#![allow(
    clippy::panic_in_result_fn,
    reason = "Analytic regression fixtures fail immediately on violated mathematical assertions"
)]
#![allow(
    clippy::arithmetic_side_effects,
    reason = "Small bounded test fixtures"
)]
use quest_numerics::arithmetic::{
    Backend, Budget, BudgetedBackend, ExactConstant, F64Backend, MpBackend, PointBackend, Precision,
};
use quest_polynomial::{Error, Limits, LinearSolver, MpHouseholder, PivotedQr};

#[test]
fn both_kernels_solve_a_pivoted_alternation_system() -> Result<(), Box<dyn std::error::Error>> {
    // Degree two on four nodes: columns T0,T1,T2,alternating error.
    let matrix = [
        1.0, -1.0, 1.0, 1.0, 1.0, -0.5, -0.5, -1.0, 1.0, 0.5, -0.5, 1.0, 1.0, 1.0, 1.0, -1.0,
    ];
    let expected = [2.0, -3.0, 4.0, 0.125];
    let rhs: Vec<f64> = matrix
        .as_chunks::<4>()
        .0
        .iter()
        .map(|row| row.iter().zip(expected).map(|(a, x)| a * x).sum())
        .collect();
    let mut backend = F64Backend;
    let faer = PivotedQr.solve(&mut backend, &matrix, &rhs, 4, Limits::default())?;
    let generic = MpHouseholder.solve(&mut backend, &matrix, &rhs, 4, Limits::default())?;
    for result in [faer, generic] {
        assert_eq!(result.rank, 4);
        assert!(result.residual <= result.relative_threshold);
        for (actual, wanted) in result.values.into_iter().zip(expected) {
            assert!((actual - wanted).abs() < 1e-12);
        }
    }
    Ok(())
}

#[test]
fn scaled_norms_handle_extreme_common_scales() -> Result<(), Box<dyn std::error::Error>> {
    for scale in [1e-300, 1e300] {
        let matrix = [scale, 0.25 * scale, -0.5 * scale, scale];
        let rhs = [0.5 * scale, 0.875 * scale];
        let mut backend = F64Backend;
        let a = PivotedQr.solve(&mut backend, &matrix, &rhs, 2, Limits::default())?;
        let b = MpHouseholder.solve(&mut backend, &matrix, &rhs, 2, Limits::default())?;
        for result in [a, b] {
            for (actual, wanted) in result.values.into_iter().zip([0.25, 1.0]) {
                assert!((actual - wanted).abs() < 1e-12);
            }
        }
    }
    Ok(())
}

#[test]
fn mp_rank_decision_uses_selected_precision() -> Result<(), Box<dyn std::error::Error>> {
    let mut backend = MpBackend::new(Precision::default())?;
    let one = backend.point(1.0)?;
    let two = backend.point(2.0)?;
    let delta = backend.constant(&ExactConstant::Decimal("1e-40".into()))?;
    let perturbed = backend.add(one.clone(), delta.clone())?;
    let rhs2 = backend.add(two.clone(), delta)?;
    let matrix = [one.clone(), one.clone(), one.clone(), perturbed];
    let solution =
        MpHouseholder.solve(&mut backend, &matrix, &[two, rhs2], 2, Limits::default())?;
    let tolerance = backend.constant(&ExactConstant::Decimal("1e-30".into()))?;
    for value in solution.values {
        let difference = backend.sub(value, one.clone())?;
        let squared = backend.mul(difference.clone(), difference)?;
        let error = backend.sqrt(squared)?;
        assert!(backend.compare(&error, &tolerance)?.is_lt());
    }
    assert!(
        PivotedQr
            .solve(&mut F64Backend, &[1.0; 4], &[2.0; 2], 2, Limits::default())
            .is_err()
    );
    Ok(())
}

#[test]
fn rank_shape_and_every_input_are_checked() {
    let mut backend = F64Backend;
    for result in [
        PivotedQr.solve(
            &mut backend,
            &[1.0, 2.0, 2.0, 4.0],
            &[0.0; 2],
            2,
            Limits::default(),
        ),
        MpHouseholder.solve(
            &mut backend,
            &[1.0, 2.0, 2.0, 4.0],
            &[0.0; 2],
            2,
            Limits::default(),
        ),
    ] {
        assert!(matches!(result, Err(Error::NotEstablished(_))));
    }
    assert!(matches!(
        MpHouseholder.solve(&mut backend, &[1.0], &[1.0, 2.0], 1, Limits::default()),
        Err(Error::Shape { .. })
    ));
    // The zero matrix does not hide a nonfinite RHS or later matrix entry.
    assert!(matches!(
        MpHouseholder.solve(
            &mut backend,
            &[0.0; 4],
            &[0.0, f64::NAN],
            2,
            Limits::default()
        ),
        Err(Error::Arithmetic(_))
    ));
    assert!(matches!(
        PivotedQr.solve(
            &mut backend,
            &[0.0, 0.0, 0.0, f64::INFINITY],
            &[0.0; 2],
            2,
            Limits::default()
        ),
        Err(Error::Arithmetic(_))
    ));
}

#[test]
fn both_kernels_obey_local_and_enclosing_work_budgets() -> Result<(), Box<dyn std::error::Error>> {
    let tiny = Limits {
        max_work: 2,
        ..Limits::default()
    };
    assert!(
        PivotedQr
            .solve(&mut F64Backend, &[1.0], &[2.0], 1, tiny)
            .is_err()
    );
    assert!(
        MpHouseholder
            .solve(&mut F64Backend, &[1.0], &[2.0], 1, tiny)
            .is_err()
    );
    let tiny = Limits {
        max_bytes: 1,
        ..Limits::default()
    };
    assert!(matches!(
        MpHouseholder.solve(&mut F64Backend, &[1.0], &[2.0], 1, tiny),
        Err(Error::Budget(_))
    ));
    let budget = Budget::new(10_000);
    let mut inner = F64Backend;
    let mut backend = BudgetedBackend::new(&mut inner, &budget);
    PivotedQr.solve(&mut backend, &[1.0], &[2.0], 1, Limits::default())?;
    assert!(budget.used() >= 16);
    let budget = Budget::new(8);
    let mut backend = BudgetedBackend::new(&mut inner, &budget);
    assert!(
        PivotedQr
            .solve(&mut backend, &[1.0], &[2.0], 1, Limits::default())
            .is_err()
    );
    Ok(())
}

#[test]
fn unrepresentable_solution_cannot_be_reported_as_zero() {
    assert!(
        PivotedQr
            .solve(&mut F64Backend, &[1e300], &[1e-300], 1, Limits::default())
            .is_err()
    );
    assert!(
        MpHouseholder
            .solve(&mut F64Backend, &[1e300], &[1e-300], 1, Limits::default())
            .is_err()
    );
}

#[test]
fn faer_padding_and_scratch_are_admitted_for_tiny_systems() -> Result<(), Box<dyn std::error::Error>>
{
    // Three 1x1 faer matrices each retain a 64-byte padded column, before
    // factor/solve scratch, permutations, or the scaled system are counted.
    let tight = Limits {
        max_bytes: 256,
        ..Limits::default()
    };
    assert!(matches!(
        PivotedQr.solve(&mut F64Backend, &[1.0], &[2.0], 1, tight),
        Err(Error::Budget(_))
    ));
    let admitted = Limits {
        max_bytes: 1024,
        ..Limits::default()
    };
    let solution = PivotedQr.solve(&mut F64Backend, &[1.0], &[2.0], 1, admitted)?;
    assert_eq!(solution.values.len(), 1);
    assert!(
        solution
            .values
            .iter()
            .all(|value| (value - 2.0).abs() < 1e-12)
    );
    Ok(())
}
