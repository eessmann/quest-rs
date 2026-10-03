#![allow(
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	clippy::float_cmp,
	clippy::panic,
	clippy::panic_in_result_fn,
	reason = "Small analytic fixtures deliberately use direct indices, exact expectations and failing assertions"
)]
use faer::Mat;
use quest_qsvt::{Complex64, PauliLimits, decompose_pauli};
const fn c(x: f64) -> Complex64 {
	Complex64::new(x, 0.0)
}
#[test]
fn decomposition_uses_trace_convention_for_complex_nonhermitian_matrix()
-> Result<(), Box<dyn std::error::Error>> {
	// A=[[1,2i],[3,4]] => I=2.5,X=1.5+i,Y=-1-1.5i,Z=-1.5.
	let a = Mat::from_fn(2, 2, |r, col| {
		[[c(1.0), Complex64::new(0.0, 2.0)], [c(3.0), c(4.0)]][r][col]
	});
	let d = decompose_pauli(a.as_ref(), PauliLimits::default())?;
	assert_eq!(
		d.coefficients(),
		&[
			c(2.5),
			Complex64::new(1.5, 1.0),
			Complex64::new(-1.0, -1.5),
			c(-1.5)
		]
	);
	assert_eq!(d.num_qubits(), 1);
	assert_eq!(d.label(2).as_deref(), Some("Y"));
	let reconstructed = d.reconstruct(PauliLimits::default())?;
	for r in 0..2 {
		for col in 0..2 {
			assert!((a[(r, col)] - reconstructed[(r, col)]).norm() < 1e-14);
		}
	}
	Ok(())
}
#[test]
fn two_qubit_tensor_order_is_little_endian_and_round_trip_is_complete()
-> Result<(), Box<dyn std::error::Error>> {
	// Z tensor X: coefficient index = 3*4+1 = 13.
	let a = Mat::from_fn(4, 4, |r, col| {
		if r == (col ^ 1) {
			if col & 2 == 0 { c(1.0) } else { c(-1.0) }
		} else {
			c(0.0)
		}
	});
	let d = decompose_pauli(a.as_ref(), PauliLimits::default())?;
	assert_eq!(d.label(13).as_deref(), Some("ZX"));
	for (i, z) in d.coefficients().iter().enumerate() {
		assert_eq!(*z, if i == 13 { c(1.0) } else { c(0.0) });
	}
	let a = Mat::from_fn(4, 4, |r, col| {
		Complex64::new(
			[
				0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0,
				15.0,
			][r * 4 + col]
				/ 7.0,
			([0.0, 1.0, 2.0, 3.0][r] - [0.0, 1.0, 2.0, 3.0][col]) / 3.0,
		)
	});
	let d = decompose_pauli(a.as_ref(), PauliLimits::default())?;
	let b = d.reconstruct(PauliLimits::default())?;
	for r in 0..4 {
		for col in 0..4 {
			assert!((a[(r, col)] - b[(r, col)]).norm() < 1e-13);
		}
	}
	Ok(())
}
#[test]
fn scalar_zero_shape_nonfinite_and_resource_boundaries_are_explicit()
-> Result<(), Box<dyn std::error::Error>> {
	let a = Mat::from_fn(1, 1, |_, _| c(3.0));
	let d = decompose_pauli(a.as_ref(), PauliLimits::default())?;
	assert_eq!(d.coefficients(), &[c(3.0)]);
	assert_eq!(d.label(0).as_deref(), Some(""));
	assert!(
		decompose_pauli(
			Mat::from_fn(0, 0, |_, _| c(0.0)).as_ref(),
			PauliLimits::default()
		)
		.is_err()
	);
	assert!(
		decompose_pauli(
			Mat::from_fn(3, 3, |_, _| c(0.0)).as_ref(),
			PauliLimits::default()
		)
		.is_err()
	);
	assert!(
		decompose_pauli(
			Mat::from_fn(2, 3, |_, _| c(0.0)).as_ref(),
			PauliLimits::default()
		)
		.is_err()
	);
	assert!(
		decompose_pauli(
			Mat::from_fn(2, 2, |_, _| c(f64::NAN)).as_ref(),
			PauliLimits::default()
		)
		.is_err()
	);
	assert!(
		decompose_pauli(
			a.as_ref(),
			PauliLimits {
				max_work: 0,
				..PauliLimits::default()
			}
		)
		.is_err()
	);
	Ok(())
}
#[test]
fn decomposition_preserves_subnormal_identity_and_finite_huge_averages()
-> Result<(), Box<dyn std::error::Error>> {
	for value in [f64::from_bits(1), f64::MAX] {
		let a = Mat::from_fn(2, 2, |r, col| if r == col { c(value) } else { c(0.0) });
		let d = decompose_pauli(a.as_ref(), PauliLimits::default())?;
		assert_eq!(d.coefficients()[0], c(value));
		assert!(d.coefficients()[1..].iter().all(|z| *z == c(0.0)));
	}
	Ok(())
}
