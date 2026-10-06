#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Explicit tiny reversible table and block reference"
)]
mod portfolio_support;
use quest_qsvt::{
	Complex64, NumericalPolicy, ReplayEncoding,
	portfolio::{PortfolioLimits, Qrom, SparseAccessEncoding},
};
#[googletest::gtest]
fn qrom_xor_lookup_unlookup_is_reversible_on_dirty_outputs() -> googletest::Result<()> {
	let q = Qrom::new(vec![1, 2, 3], 2, PortfolioLimits::default())?;
	for address in 0..4 {
		for output in 0..4 {
			let mut actual = address | (output << 2);
			q.visit_lookup(&[0, 1], &[2, 3], 0, 0, false, &mut |g| {
				if actual & g.control_mask == g.control_value {
					actual ^= 1 << g.target.unwrap();
				}
				Ok(())
			})?;
			googletest::expect_eq!(actual, address | ((output ^ ([1, 2, 3, 0][address])) << 2));
			q.visit_lookup(&[0, 1], &[2, 3], 0, 0, true, &mut |g| {
				if actual & g.control_mask == g.control_value {
					actual ^= 1 << g.target.unwrap();
				}
				Ok(())
			})?;
			googletest::expect_eq!(actual, address | (output << 2));
		}
	}
	Ok(())
}
#[googletest::gtest]
fn explicit_sparse_access_lookup_value_unlookup_extracts_quantized_complex_matrix()
-> googletest::Result<()> {
	let a = quest_numerics::SparseMatrix::from_triplets(
		2,
		2,
		quest_numerics::SparseFormat::Csr,
		vec![
			(0, 0, Complex64::new(1.0, 0.0)),
			(0, 1, Complex64::new(0.0, 0.5)),
			(1, 0, Complex64::new(-0.5, 0.0)),
		],
		quest_numerics::SparseLimits::default(),
	)?;
	let source = SparseAccessEncoding::new(&a, 2, PortfolioLimits::default())?;
	portfolio_support::whole_register(&source)?;
	let d = source.descriptor()?;
	let l = d
		.left
		.logical_space::<quest_qsvt::Left>(d.layout.num_qubits, NumericalPolicy::default())?;
	let r = d
		.right
		.logical_space::<quest_qsvt::Right>(d.layout.num_qubits, NumericalPolicy::default())?;
	for j in 0..2 {
		let mut state = vec![Complex64::default(); 1 << d.layout.num_qubits];
		state[r.coordinate_at(j).unwrap()] = 1.0.into();
		source.apply_reference(&mut state, false, 4_000_000)?;
		for i in 0..2 {
			let expected = match (i, j) {
				(0, 0) => Complex64::new(1.0, 0.0),
				(0, 1) => Complex64::new(0.0, 0.5),
				(1, 0) => Complex64::new(-0.5, 0.0),
				_ => Complex64::default(),
			};
			googletest::expect_true!(
				(state[l.coordinate_at(i).unwrap()] * d.normalization - expected).norm() < 5e-13
			);
		}
	}
	googletest::expect_eq!(source.resources().oracle_queries, 4);
	googletest::expect_eq!(source.resources().precision_bits, 2);
	Ok(())
}
#[googletest::gtest]
fn explicit_sparse_precision_is_observed_and_table_work_is_never_free() -> googletest::Result<()> {
	let matrix = quest_numerics::SparseMatrix::from_triplets(
		1,
		2,
		quest_numerics::SparseFormat::Csc,
		vec![
			(0, 1, Complex64::new(0.3, 0.4)),
			(0, 0, Complex64::new(1.0, 0.0)),
		],
		quest_numerics::SparseLimits::default(),
	)?;
	let coarse = SparseAccessEncoding::new(&matrix, 2, PortfolioLimits::default())?;
	let fine = SparseAccessEncoding::new(&matrix, 8, PortfolioLimits::default())?;
	googletest::expect_true!(
		fine.coefficient_error_estimate() < coarse.coefficient_error_estimate()
	);
	portfolio_support::whole_register(&coarse)?;
	for limits in [
		PortfolioLimits {
			max_bytes: 1,
			..Default::default()
		},
		PortfolioLimits {
			max_compile_work: 1,
			..Default::default()
		},
		PortfolioLimits {
			max_gates: 1,
			..Default::default()
		},
		PortfolioLimits {
			max_table_entries: 1,
			..Default::default()
		},
	] {
		googletest::expect_true!(SparseAccessEncoding::new(&matrix, 2, limits).is_err());
		googletest::expect_true!(Qrom::new(vec![1, 2, 3], 2, limits).is_err());
	}
	let mut words = Vec::with_capacity(131_072);
	words.push(1);
	googletest::expect_true!(
		Qrom::new(
			words,
			1,
			PortfolioLimits {
				max_bytes: 16_384,
				..Default::default()
			}
		)
		.is_err()
	);
	let zero = quest_numerics::SparseMatrix::from_triplets(
		1,
		1,
		quest_numerics::SparseFormat::Csr,
		vec![],
		quest_numerics::SparseLimits::default(),
	)?;
	let e = SparseAccessEncoding::new(&zero, 2, PortfolioLimits::default())?;
	portfolio_support::whole_register(&e)?;
	eprintln!(
		"PORTFOLIO_SPARSE alpha={} resources={:?}",
		coarse.descriptor()?.normalization,
		coarse.resources()
	);
	Ok(())
}
