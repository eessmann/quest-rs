use quest_numerics::{Complex64, SparseFormat, SparseLimits, SparseMatrix};

const fn c(re: f64, im: f64) -> Complex64 {
	Complex64::new(re, im)
}

#[googletest::gtest]
fn canonical_segments_sum_duplicates_in_input_order_and_remove_zeros() -> googletest::Result<()> {
	let matrix = SparseMatrix::builder(2, 3)
		.entries(
			vec![c(1.0, 2.0), c(3.0, 0.0), c(-1.0, -2.0), c(4.0, -1.0)],
			vec![2, 0, 2, 1],
			vec![0, 3, 4],
		)
		.build(SparseLimits::default())?;
	googletest::expect_that!(
		matrix.entries().collect::<Vec<_>>(),
		googletest::matchers::eq(&vec![(0, 0, c(3.0, 0.0)), (1, 1, c(4.0, -1.0))])
	);
	googletest::expect_that!(
		matrix.major_segment(0),
		googletest::matchers::eq(Some((&[0][..], &[c(3.0, 0.0)][..])))
	);
	googletest::expect_that!(matrix.major_segment(2), googletest::matchers::eq(None));
	Ok(())
}

#[googletest::gtest]
fn csc_one_based_adjoint_and_reference_products_agree() -> googletest::Result<()> {
	let matrix = SparseMatrix::builder(2, 3)
		.format(SparseFormat::Csc)
		.one_based()
		.entries(
			vec![c(2.0, 1.0), c(-1.0, 0.0), c(3.0, -2.0)],
			vec![1, 2, 1],
			vec![1, 2, 3, 4],
		)
		.build(SparseLimits::default())?;
	googletest::expect_that!(
		matrix.matvec(
			&[c(1.0, 0.0), c(2.0, 0.0), c(0.0, 1.0)],
			SparseLimits::default()
		)?,
		googletest::matchers::eq(&vec![c(4.0, 4.0), c(-2.0, 0.0)])
	);
	let adjoint = matrix.adjoint(SparseLimits::default())?;
	googletest::expect_that!(
		(adjoint.rows(), adjoint.cols(), adjoint.format()),
		googletest::matchers::eq((3, 2, SparseFormat::Csr))
	);
	googletest::expect_that!(
		adjoint.matvec(&[c(1.0, 0.0), c(2.0, 0.0)], SparseLimits::default())?,
		googletest::matchers::eq(&vec![c(2.0, -1.0), c(-2.0, 0.0), c(3.0, 2.0)])
	);
	googletest::expect_that!(
		matrix.row(0).collect::<Vec<_>>(),
		googletest::matchers::eq(&vec![(0, c(2.0, 1.0)), (2, c(3.0, -2.0))])
	);
	googletest::expect_that!(
		matrix.column(1).collect::<Vec<_>>(),
		googletest::matchers::eq(&vec![(1, c(-1.0, 0.0))])
	);
	Ok(())
}

#[googletest::gtest]
fn norms_bound_a_rectangular_operator() -> googletest::Result<()> {
	let matrix = SparseMatrix::from_triplets(
		2,
		3,
		SparseFormat::Csr,
		vec![
			(0, 0, c(3.0, 4.0)),
			(0, 1, c(2.0, 0.0)),
			(1, 1, c(-1.0, 0.0)),
		],
		SparseLimits::default(),
	)?;
	let norms = matrix.norms(SparseLimits::default())?;
	googletest::expect_that!(
		(norms.max_abs, norms.max_row_sum, norms.max_col_sum),
		googletest::matchers::eq((5.0, 7.0, 5.0))
	);
	googletest::expect_true!(norms.spectral_upper_bound >= 35.0_f64.sqrt());
	Ok(())
}

#[googletest::gtest]
fn malformed_storage_and_nonfinite_duplicates_fail() {
	let build = |data, indices, indptr| {
		SparseMatrix::builder(1, 2)
			.entries(data, indices, indptr)
			.build(SparseLimits::default())
	};
	googletest::expect_true!(build(vec![c(1.0, 0.0)], vec![0], vec![0, 2]).is_err());
	googletest::expect_true!(build(vec![c(1.0, 0.0)], vec![2], vec![0, 1]).is_err());
	googletest::expect_true!(
		build(
			vec![c(f64::MAX, 0.0), c(f64::MAX, 0.0)],
			vec![0, 0],
			vec![0, 2]
		)
		.is_err()
	);
	googletest::expect_true!(build(vec![c(f64::NAN, 0.0)], vec![0], vec![0, 1]).is_err());
}

#[googletest::gtest]
fn construction_and_execution_budgets_are_independent() -> googletest::Result<()> {
	let matrix = SparseMatrix::builder(1, 1)
		.entries(vec![c(1.0, 0.0)], vec![0], vec![0, 1])
		.build(SparseLimits::default())?;
	let no_work = SparseLimits {
		max_work: 0,
		..SparseLimits::default()
	};
	googletest::expect_true!(matrix.matvec(&[c(1.0, 0.0)], no_work).is_err());
	googletest::expect_true!(matrix.norms(no_work).is_err());
	googletest::expect_true!(
		matrix
			.adjoint(SparseLimits {
				max_bytes: 0,
				..SparseLimits::default()
			})
			.is_err()
	);
	googletest::expect_true!(matrix.matvec(&[], SparseLimits::default()).is_err());
	googletest::expect_true!(
		SparseMatrix::builder(1, 1)
			.entries(Vec::with_capacity(1000), vec![], vec![0, 0])
			.build(SparseLimits {
				max_bytes: 32,
				..SparseLimits::default()
			})
			.is_err()
	);
	Ok(())
}

#[googletest::gtest]
fn triplets_preserve_nonassociative_duplicate_order() -> googletest::Result<()> {
	for format in [SparseFormat::Csr, SparseFormat::Csc] {
		let matrix = SparseMatrix::from_triplets(
			2,
			2,
			format,
			vec![
				(1, 1, c(1e16, 0.0)),
				(0, 0, c(2.0, 0.0)),
				(1, 1, c(-1e16, 0.0)),
				(1, 1, c(1.0, 0.0)),
			],
			SparseLimits::default(),
		)?;
		googletest::expect_that!(
			matrix.row(1).collect::<Vec<_>>(),
			googletest::matchers::eq(&vec![(1, c(1.0, 0.0))])
		);
		googletest::expect_that!(
			matrix
				.adjoint(SparseLimits::default())?
				.adjoint(SparseLimits::default())?
				.entries()
				.collect::<Vec<_>>(),
			googletest::matchers::eq(&matrix.entries().collect::<Vec<_>>())
		);
	}
	Ok(())
}

#[googletest::gtest]
fn norm_bounds_scale_large_tiny_and_zero_operators() -> googletest::Result<()> {
	for value in [1e200, 1e-200, 0.0] {
		let matrix = SparseMatrix::from_triplets(
			1,
			1,
			SparseFormat::Csr,
			vec![(0, 0, c(value, 0.0))],
			SparseLimits::default(),
		)?;
		let norms = matrix.norms(SparseLimits::default())?;
		googletest::expect_true!(norms.spectral_upper_bound.is_finite());
		googletest::expect_true!(norms.spectral_upper_bound >= value);
		googletest::expect_true!(norms.max_abs >= value);
	}
	Ok(())
}

#[googletest::gtest]
fn limits_reject_shapes_work_entries_and_nonfinite_products() -> googletest::Result<()> {
	let tiny = SparseLimits {
		max_entries: 0,
		..SparseLimits::default()
	};
	googletest::expect_true!(
		SparseMatrix::from_triplets(1, 1, SparseFormat::Csr, vec![(0, 0, c(1.0, 0.0))], tiny)
			.is_err()
	);
	googletest::expect_true!(
		SparseMatrix::builder(usize::MAX, 1)
			.entries(vec![], vec![], vec![])
			.build(SparseLimits {
				max_dimension: usize::MAX,
				..SparseLimits::default()
			})
			.is_err()
	);
	googletest::expect_true!(
		SparseMatrix::builder(1, 1)
			.one_based()
			.entries(vec![c(1.0, 0.0)], vec![0], vec![1, 2])
			.build(SparseLimits::default())
			.is_err()
	);
	googletest::expect_true!(
		SparseMatrix::builder(1, 1)
			.entries(vec![c(1.0, 0.0)], vec![0], vec![0, 1])
			.build(SparseLimits {
				max_work: 0,
				..SparseLimits::default()
			})
			.is_err()
	);
	let matrix = SparseMatrix::from_triplets(
		1,
		1,
		SparseFormat::Csr,
		vec![(0, 0, c(f64::MAX, 0.0))],
		SparseLimits::default(),
	)?;
	googletest::expect_true!(
		matrix
			.matvec(&[c(2.0, 0.0)], SparseLimits::default())
			.is_err()
	);
	googletest::expect_true!(
		matrix
			.matvec(&[c(f64::NAN, 0.0)], SparseLimits::default())
			.is_err()
	);
	Ok(())
}

#[googletest::gtest]
fn construction_work_includes_validation_and_pointer_output_passes() {
	googletest::expect_true!(
		SparseMatrix::builder(2, 1)
			.entries(vec![], vec![], vec![0, 0, 0])
			.build(SparseLimits {
				max_work: 3,
				..SparseLimits::default()
			})
			.is_err()
	);
}
