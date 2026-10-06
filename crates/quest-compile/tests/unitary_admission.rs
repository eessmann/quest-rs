use num_complex::Complex64;
use quest_compile::{MatrixPolicy, NumericalOperator};

#[test]
fn explicit_unitary_evidence_is_bounded_and_separate_from_general_storage() {
	let source = faer::Mat::from_fn(2, 2, |row, col| match (row, col) {
		(0, 1) => Complex64::new(0.0, 1.0),
		(1, 0) => Complex64::new(1.0, 0.0),
		_ => Complex64::new(0.0, 0.0),
	});
	let general = NumericalOperator::from_view(&source, MatrixPolicy::default()).unwrap();
	assert!(general.unitary_evidence().is_none());
	assert!(
		general
			.clone()
			.admit_unitary(1e-12, MatrixPolicy::default(), 0)
			.is_err()
	);
	let admitted = general
		.clone()
		.admit_unitary(1e-12, MatrixPolicy::default(), 64)
		.unwrap();
	assert_eq!(general.view().as_ptr(), admitted.view().as_ptr());
	assert_eq!(admitted.unitary_evidence().unwrap().residual(), 0.0);
	assert_ne!(general.evidence_identity(), admitted.evidence_identity());
	assert!(
		admitted
			.clone()
			.admit_unitary(1e-12, MatrixPolicy::default(), 0)
			.is_err()
	);
	assert!(
		general
			.clone()
			.admit_unitary(1e-12, MatrixPolicy { max_bytes: 1 }, 64)
			.is_err()
	);
	assert!(
		general
			.admit_unitary(f64::NAN, MatrixPolicy::default(), 64)
			.is_err()
	);
	assert!(
		admitted
			.conjugate_transpose(MatrixPolicy::default())
			.unwrap()
			.unitary_evidence()
			.is_none()
	);
	let recipe =
		quest_compile::dispatch_recipe::MatrixRecipe::new(&admitted, &[false, true, true]).unwrap();
	assert_eq!(recipe.dimension(), 16);
	assert_eq!(recipe.native_dimension(), 2);
	assert_eq!(recipe.native_apply_calls(true), 1);
	assert_eq!(recipe.payload_bytes().unwrap(), 128);
	let nonunitary = faer::Mat::from_fn(2, 2, |r, c| {
		Complex64::new(if r == c { 2.0 } else { 1.0 }, 0.0)
	});
	assert!(
		NumericalOperator::from_view(&nonunitary, MatrixPolicy::default())
			.unwrap()
			.admit_unitary(1e-12, MatrixPolicy::default(), 64)
			.is_err()
	);
}
