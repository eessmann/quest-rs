use faer::Mat;
use googletest::Result;
use googletest::prelude::*;
use num_complex::Complex64 as C;
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::*;

fn fixture() -> Mat<C> {
	Mat::from_fn(2, 2, |r, c| match (r, c) {
		(0, 0) => C::new(1.0, 0.0),
		(0, _) => C::new(2.0, -2.0),
		(_, 0) => C::new(3.0, 1.0),
		_ => C::new(4.0, -1.0),
	})
}

#[gtest]
fn logical_views_are_copied_including_transpose_and_conjugation() -> Result<()> {
	let a = fixture();
	let x = NumericalOperator::from_view(a.adjoint(), MatrixPolicy::default())?;
	for r in 0..2 {
		for c in 0..2 {
			expect_eq!(x.view()[(r, c)], a[(c, r)].conj());
		}
	}
	Ok(())
}

#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Independent floating point fixtures and bounded test indices cannot overflow integers"
)]
fn faer_product_matches_independent_scalar_complex_fixture() -> Result<()> {
	let a = fixture();
	let b = Mat::from_fn(2, 2, |r, c| {
		C::new(
			2.0 + num_traits::ToPrimitive::to_f64(&r).unwrap_or(f64::NAN)
				+ num_traits::ToPrimitive::to_f64(&c).unwrap_or(f64::NAN),
			2.0f64.mul_add(
				num_traits::ToPrimitive::to_f64(&r).unwrap_or(f64::NAN),
				num_traits::ToPrimitive::to_f64(&c).unwrap_or(f64::NAN),
			),
		)
	});
	let x = NumericalOperator::from_view(a.as_ref(), MatrixPolicy::default())?;
	let y = NumericalOperator::from_view(b.as_ref(), MatrixPolicy::default())?;
	let product = x.product(&y, MatrixPolicy::default())?;
	for r in 0..2 {
		for c in 0..2 {
			let expected = (0..2).map(|k| a[(r, k)] * b[(k, c)]).sum::<C>();
			expect_lt!((product.view()[(r, c)] - expected).norm(), 1e-12);
		}
	}
	Ok(())
}

#[gtest]
#[expect(
	clippy::unnecessary_wraps,
	reason = "The googletest harness requires a Result return"
)]
fn matrix_admission_checks_finiteness_shape_and_budget() -> Result<()> {
	let a = fixture();
	expect_true!(NumericalOperator::from_view(a.as_ref(), MatrixPolicy { max_bytes: 1 }).is_err());
	expect_true!(
		NumericalOperator::from_view(Mat::<C>::zeros(3, 3).as_ref(), MatrixPolicy::default())
			.is_err()
	);
	let bad = Mat::from_fn(2, 2, |_, _| C::new(f64::NAN, 0.0));
	expect_true!(NumericalOperator::from_view(bad.as_ref(), MatrixPolicy::default()).is_err());
	Ok(())
}

#[gtest]
fn padding_is_counted_and_noncontiguous_views_keep_logical_coordinates() -> Result<()> {
	let a = fixture();
	expect_true!(NumericalOperator::from_view(a.as_ref(), MatrixPolicy { max_bytes: 64 }).is_err());
	let padded = Mat::from_fn(7, 5, |r, c| {
		C::new(
			10.0f64.mul_add(
				num_traits::ToPrimitive::to_f64(&r).unwrap_or(f64::NAN),
				num_traits::ToPrimitive::to_f64(&c).unwrap_or(f64::NAN),
			),
			num_traits::ToPrimitive::to_f64(&r).unwrap_or(f64::NAN)
				+ num_traits::ToPrimitive::to_f64(&c).unwrap_or(f64::NAN),
		)
	});
	let view = padded.submatrix(2, 1, 2, 2).reverse_rows().conjugate();
	let admitted = NumericalOperator::from_view(view, MatrixPolicy::default())?;
	expect_eq!(admitted.view()[(0, 0)], padded[(3, 1)].conj());
	expect_eq!(admitted.view()[(1, 1)], padded[(2, 2)].conj());
	let rows = [
		C::new(1.0, 2.0),
		C::new(3.0, 4.0),
		C::new(5.0, 6.0),
		C::new(7.0, 8.0),
	];
	let row_view = faer::MatRef::from_row_major_slice(&rows, 2, 2);
	let row_admitted = NumericalOperator::from_view(row_view, MatrixPolicy::default())?;
	expect_eq!(row_admitted.view()[(0, 1)], rows[1]);
	expect_eq!(row_admitted.view()[(1, 0)], rows[2]);
	Ok(())
}

#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Independent floating point fixtures and bounded test indices cannot overflow integers"
)]
fn u_realization_keeps_finite_large_phases_without_overflowing_their_sum() -> Result<()> {
	let matrix = BoundGate::U {
		theta: 0.5,
		phi: 1e308,
		lambda: 1e308,
	}
	.matrix(MatrixPolicy::default())?;
	expect_lt!(matrix.unitarity_residual(MatrixPolicy::default())?, 1e-14);
	let phase = C::new(1e308f64.cos(), 1e308f64.sin());
	expect_lt!(
		(matrix.view()[(1, 1)] - C::from_polar(1.0, 0.25) * phase * phase * 0.25f64.cos()).norm(),
		1e-15
	);
	Ok(())
}

#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Independent floating point fixtures and bounded test indices cannot overflow integers"
)]
fn openqasm31_u_has_specification_phase_and_two_pi_periodicity() -> Result<()> {
	// Fixed entries of the specification's exponential matrix, independent of
	// the half-angle implementation. U(pi/2, 0, pi) = exp(i*pi/4) H.
	for theta in [
		std::f64::consts::FRAC_PI_2,
		5.0 * std::f64::consts::FRAC_PI_2,
	] {
		let matrix = BoundGate::U {
			theta,
			phi: 0.0,
			lambda: std::f64::consts::PI,
		}
		.matrix(MatrixPolicy::default())?;
		for (row, col, expected) in [
			(0, 0, C::new(0.5, 0.5)),
			(0, 1, C::new(0.5, 0.5)),
			(1, 0, C::new(0.5, 0.5)),
			(1, 1, C::new(-0.5, -0.5)),
		] {
			expect_lt!((matrix.view()[(row, col)] - expected).norm(), 1e-14);
		}
	}
	Ok(())
}

#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Independent floating point fixtures and bounded test indices cannot overflow integers"
)]
fn openqasm31_u_adjoint_keeps_the_conjugated_specification_phase() -> Result<()> {
	let gate = Gate::U {
		theta: Angle::pi(1, 2)?,
		phi: Angle::pi(0, 1)?,
		lambda: Angle::pi(1, 1)?,
	};
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	builder.gate(gate.adjoint()?, &[builder.qubit(0)?], &[])?;
	let bound = builder.finish()?.bind(&[])?;
	let Operation::Gate { gate, .. } = bound
		.instructions()
		.first()
		.ok_or(Error::InvalidId)?
		.operation()
	else {
		fail!("expected gate")?;
		return Ok(());
	};
	let matrix = gate.matrix(MatrixPolicy::default())?;
	expect_lt!((matrix.view()[(0, 0)] - C::new(0.5, -0.5)).norm(), 1e-14);
	expect_lt!((matrix.view()[(1, 1)] - C::new(-0.5, 0.5)).norm(), 1e-14);
	Ok(())
}

#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Independent finite complex fixture compares every matrix entry"
)]
fn registry_u_decomposition_reconstructs_the_full_specification_matrix() -> Result<()> {
	use quest_language::{Decomposition, GateKind};
	let Decomposition::Sequence(steps) = GateKind::U.definition().decomposition else {
		fail!("U must expose its complete primitive sequence")?;
		return Ok(());
	};
	let policy = MatrixPolicy::default();
	let parameters = [std::f64::consts::FRAC_PI_2; 3];
	let mut product = BoundGate::Id.matrix(policy)?;
	for step in steps {
		let expression = step.parameters.first().ok_or(Error::InvalidId)?;
		let angle = expression.terms.iter().try_fold(
			std::f64::consts::PI * f64::from(expression.pi_numerator)
				/ f64::from(expression.pi_denominator),
			|sum, term| {
				let input = *parameters.get(term.input).ok_or(Error::InvalidId)?;
				Ok::<_, Error>(
					sum + input * f64::from(term.numerator) / f64::from(term.denominator),
				)
			},
		)?;
		let operator = match step.gate {
			GateKind::Ry => BoundGate::Ry(angle).matrix(policy)?,
			GateKind::Rz => BoundGate::Rz(angle).matrix(policy)?,
			GateKind::Phase => BoundGate::Phase(angle).matrix(policy)?,
			GateKind::GlobalPhase => NumericalOperator::from_view(
				Mat::from_fn(2, 2, |row, col| {
					if row == col {
						C::from_polar(1.0, angle)
					} else {
						C::new(0.0, 0.0)
					}
				})
				.as_ref(),
				policy,
			)?,
			_ => {
				fail!("unexpected primitive in U decomposition")?;
				return Ok(());
			}
		};
		product = operator.product(&product, policy)?;
	}
	// Literal entries from the specification's exponential definition at
	// theta=phi=lambda=pi/2. Neither implementation supplies the expected phase.
	for (row, col, expected) in [
		(0, 0, C::new(0.5, 0.5)),
		(0, 1, C::new(0.5, -0.5)),
		(1, 0, C::new(-0.5, 0.5)),
		(1, 1, C::new(-0.5, -0.5)),
	] {
		expect_lt!((product.view()[(row, col)] - expected).norm(), 1e-14);
	}
	Ok(())
}
