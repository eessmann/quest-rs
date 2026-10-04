#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Small independently materialized whole-circuit reference"
)]
use quest_numerics::{SparseFormat, SparseMatrix};
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::{
	Complex64, MatchingEncoding, NumericalPolicy, OperandLayout, TransformBuilder,
	materialize_program, replay_transform::MatchingTransform,
};
#[googletest::gtest]
fn lazy_matching_transform_matches_portable_whole_unitary() -> googletest::Result<()> {
	let policy = NumericalPolicy::default();
	let matrix = SparseMatrix::from_triplets(
		2,
		3,
		SparseFormat::Csr,
		vec![
			(0, 1, Complex64::new(0.3, 0.2)),
			(1, 0, Complex64::new(-0.7, 0.1)),
			(1, 2, Complex64::new(0.1, 0.0)),
		],
		quest_numerics::SparseLimits::default(),
	)?;
	let encoding = MatchingEncoding::from_sparse(&matrix, policy)?;
	for values in [vec![0.17, -0.23, -0.23, 0.17], vec![0.21, -0.3, 0.21]] {
		let sequence = PhaseSequence::<WxSymmetric>::builder(values).build()?;
		let replay = MatchingTransform::new(encoding.clone(), sequence.clone(), policy)?;
		let width = encoding.num_qubits();
		let portable = TransformBuilder::new()
			.encoding(encoding.projected_encoding(policy)?)
			.operands(OperandLayout::new(
				width + 1,
				(0..width).collect(),
				width,
				None,
			)?)
			.standard(sequence)
			.build()?;
		let unitary = materialize_program(portable.main(), policy)?;
		let initial: Vec<_> = (0..unitary.nrows())
			.map(|i| Complex64::new(f64::from(u32::try_from(i).unwrap_or(0)).sin(), 0.2))
			.collect();
		let mut actual = initial.clone();
		replay.apply_reference(&mut actual, false, policy)?;
		for row in 0..actual.len() {
			let expected: Complex64 = (0..actual.len())
				.map(|col| unitary[(row, col)] * initial[col])
				.sum();
			googletest::expect_true!((actual[row] - expected).norm() < 1e-11);
		}
		replay.apply_reference(&mut actual, true, policy)?;
		googletest::expect_true!(
			actual
				.iter()
				.zip(initial)
				.all(|(a, b)| (*a - b).norm() < 1e-11)
		);
	}
	Ok(())
}
