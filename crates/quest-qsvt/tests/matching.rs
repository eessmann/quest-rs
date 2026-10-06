use googletest::{expect_that, expect_true, gtest, matchers::eq};
use quest_numerics::{SparseFormat, SparseLimits, SparseMatrix};
use quest_qsvt::{Complex64, MatchingEncoding, NumericalPolicy, materialize_oracle};
use std::ops::Sub;
fn sparse(
	rows: usize,
	cols: usize,
	entries: Vec<(usize, usize, Complex64)>,
) -> quest_numerics::Result<SparseMatrix> {
	SparseMatrix::from_triplets(
		rows,
		cols,
		SparseFormat::Csr,
		entries,
		SparseLimits::default(),
	)
}
#[gtest]
fn history_adjoint_sign_permutation_changes_source_identity() -> googletest::Result<()> {
	// The two changed signs used to cancel when entire binary64 words were
	// XORed into FNV. This is the actual two-node, zero-generator DG1 history.
	let history = sparse(
		2,
		2,
		vec![
			(0, 0, Complex64::new(0.5, 0.0)),
			(0, 1, Complex64::new(0.5, 0.0)),
			(1, 0, Complex64::new(-0.5, 0.0)),
			(1, 1, Complex64::new(0.5, 0.0)),
		],
	)?;
	let adjoint = history.adjoint(SparseLimits::default())?;
	let policy = NumericalPolicy::default();
	let direct = MatchingEncoding::from_sparse(&history, policy)?;
	let inverse_source = MatchingEncoding::from_sparse(&adjoint, policy)?;
	expect_true!(direct.source_identity() != inverse_source.source_identity());
	let csc = SparseMatrix::from_triplets(
		2,
		2,
		SparseFormat::Csc,
		history.entries().collect(),
		SparseLimits::default(),
	)?;
	expect_that!(
		MatchingEncoding::from_sparse(&csc, policy)?.source_identity(),
		eq(direct.source_identity())
	);
	Ok(())
}
#[gtest]
fn complex_rectangular_matching_extracts_independent_matrix() -> googletest::Result<()> {
	let source = sparse(
		3,
		2,
		vec![
			(0, 0, Complex64::new(1.0, 2.0)),
			(0, 1, Complex64::new(-0.5, 0.0)),
			(1, 0, Complex64::new(0.0, -2.0)),
			(2, 1, Complex64::new(3.0, 0.5)),
		],
	)?;
	let encoding = MatchingEncoding::from_sparse(&source, NumericalPolicy::default())?;
	let projected = encoding.projected_encoding(NumericalPolicy::default())?;
	let block = projected.logical_matrix()?;
	for row in 0..3 {
		for col in 0..2 {
			let expected = source
				.row(row)
				.find(|&(c, _)| c == col)
				.map_or(Complex64::new(0.0, 0.0), |(_, v)| v);
			expect_true!(block[(row, col)].sub(expected).norm() < 1e-12);
		}
	}
	Ok(())
}
#[gtest]
fn permutation_completion_closes_paths_and_preserves_cycles() -> googletest::Result<()> {
	let source = sparse(
		5,
		5,
		vec![
			(1, 0, Complex64::new(1.0, 0.0)),
			(2, 1, Complex64::new(1.0, 0.0)),
			(4, 3, Complex64::new(1.0, 0.0)),
			(3, 4, Complex64::new(1.0, 0.0)),
		],
	)?;
	let encoding = MatchingEncoding::from_sparse(&source, NumericalPolicy::default())?;
	expect_that!(encoding.matchings().len(), eq(1));
	let matching = encoding
		.matchings()
		.first()
		.ok_or(quest_qsvt::Error::Encoding("missing matching"))?;
	expect_that!(matching.permutation_forward(0), eq(1));
	expect_that!(matching.permutation_forward(1), eq(2));
	expect_that!(matching.permutation_forward(2), eq(0));
	expect_that!(matching.permutation_forward(3), eq(4));
	expect_that!(matching.permutation_forward(4), eq(3));
	expect_that!(matching.permutation_forward(7), eq(7));
	for i in 0..8 {
		expect_that!(
			matching.permutation_inverse(matching.permutation_forward(i)),
			eq(i)
		);
	}
	Ok(())
}
#[gtest]
fn full_reference_matches_gates_and_adjoint_in_every_failure_sector() -> googletest::Result<()> {
	let source = sparse(
		2,
		3,
		vec![
			(0, 0, Complex64::new(1.0, 1.0)),
			(0, 1, Complex64::new(-1.0, 0.0)),
			(0, 2, Complex64::new(0.0, 2.0)),
			(1, 0, Complex64::new(0.2, 0.0)),
		],
	)?;
	let encoding = MatchingEncoding::from_sparse(&source, NumericalPolicy::default())?;
	expect_that!(encoding.num_colors(), eq(4));
	let oracle = encoding.to_oracle(NumericalPolicy::default())?;
	let unitary = materialize_oracle(&oracle, NumericalPolicy::default())?;
	for basis in 0..unitary.ncols() {
		let mut state = vec![Complex64::new(0.0, 0.0); unitary.nrows()];
		*state
			.get_mut(basis)
			.ok_or(quest_qsvt::Error::Encoding("basis"))? = Complex64::new(1.0, 0.0);
		encoding.apply_reference(&mut state, false, NumericalPolicy::default())?;
		for (row, &actual) in state.iter().enumerate() {
			expect_true!(actual.sub(unitary[(row, basis)]).norm() < 1e-12);
		}
		encoding.apply_reference(&mut state, true, NumericalPolicy::default())?;
		for (row, &actual) in state.iter().enumerate() {
			expect_true!(
				actual
					.sub(Complex64::new(f64::from(row == basis), 0.0))
					.norm()
					< 1e-12
			);
		}
	}
	// A column with no matching must be routed wholly into flag one.
	let zero = MatchingEncoding::from_sparse(&sparse(2, 3, vec![])?, NumericalPolicy::default())?;
	let block = zero
		.projected_encoding(NumericalPolicy::default())?
		.logical_matrix()?;
	expect_true!(block.col(0).iter().all(|value| value.norm() < 1e-12));
	Ok(())
}
#[gtest]
fn large_sparse_dimensions_reject_only_dense_expansion() -> googletest::Result<()> {
	let source = sparse(
		1_000_000,
		1_000_000,
		vec![(999_999, 500_000, Complex64::new(1.0, 0.0))],
	)?;
	let policy = NumericalPolicy { max_bytes: 16384 };
	let encoding = MatchingEncoding::from_sparse(&source, policy)?;
	let mut count = 0_usize;
	encoding.visit_gates(false, |_| {
		count = count
			.checked_add(1)
			.ok_or(quest_qsvt::Error::Budget("count"))?;
		Ok(())
	})?;
	expect_true!(count < 1000);
	expect_true!(
		materialize_oracle(&encoding.to_oracle(NumericalPolicy::default())?, policy).is_err()
	);
	expect_true!(MatchingEncoding::from_sparse(&source, NumericalPolicy { max_bytes: 0 }).is_err());
	Ok(())
}

fn mapped_matrix(
	encoding: &MatchingEncoding,
	targets: &[usize],
	mask: usize,
	value: usize,
	adjoint: bool,
	width: usize,
) -> quest_qsvt::Result<faer::Mat<Complex64>> {
	use quest_compile::{Angle, Control, ControlState, Gate, QuantumRegionBuilder};
	use quest_qsvt::ReplayKind;
	let mut body = QuantumRegionBuilder::new(width, 0)?;
	encoding.visit_mapped_gates(targets, mask, value, adjoint, |primitive| {
		let mut controls = Vec::new();
		for bit in 0..width {
			let flag = 1usize
				.checked_shl(
					u32::try_from(bit).map_err(|_| quest_qsvt::Error::Budget("test width"))?,
				)
				.ok_or(quest_qsvt::Error::Budget("test width"))?;
			if primitive.control_mask & flag != 0 {
				controls.push(Control::new(
					body.qubit(bit)?,
					if primitive.control_value & flag != 0 {
						ControlState::One
					} else {
						ControlState::Zero
					},
				));
			}
		}
		match primitive.kind {
			ReplayKind::Phase(angle) => {
				body.global_phase(Angle::radians(angle)?, &controls)?;
			}
			kind => {
				let gate = match kind {
					ReplayKind::H => Gate::H,
					ReplayKind::X => Gate::X,
					ReplayKind::Ry(angle) => Gate::Ry(Angle::radians(angle)?),
					ReplayKind::Phase(_) => return Err(quest_qsvt::Error::Encoding("test gate")),
				};
				body.gate(
					gate,
					&[body.qubit(
						primitive
							.target
							.ok_or(quest_qsvt::Error::Encoding("test target"))?,
					)?],
					&controls,
				)?;
			}
		}
		Ok(())
	})?;
	quest_qsvt::materialize_program(&body.finish()?.bind(&[])?, NumericalPolicy::default())
}

#[gtest]
fn remapped_outer_controls_preserve_inactive_states_and_adjoint() -> googletest::Result<()> {
	let source = sparse(
		2,
		2,
		vec![
			(0, 1, Complex64::new(1.0, 1.0)),
			(1, 0, Complex64::new(-0.2, 0.4)),
		],
	)?;
	let encoding = MatchingEncoding::from_sparse(&source, NumericalPolicy::default())?;
	let targets = [2, 0]; // local flag on physical 2, system on physical 0
	let base = materialize_oracle(
		&encoding.to_oracle(NumericalPolicy::default())?,
		NumericalPolicy::default(),
	)?;
	for adjoint in [false, true] {
		for control_value in [0, 2] {
			let mapped = mapped_matrix(&encoding, &targets, 2, control_value, adjoint, 3)?;
			for row in 0..8 {
				for col in 0..8 {
					let local_row = ((row >> 2) & 1) | ((row & 1) << 1);
					let local_col = ((col >> 2) & 1) | ((col & 1) << 1);
					let expected = if (row & 2) != (col & 2) {
						Complex64::new(0.0, 0.0)
					} else if row & 2 != control_value {
						Complex64::new(f64::from(row == col), 0.0)
					} else if adjoint {
						base[(local_col, local_row)].conj()
					} else {
						base[(local_row, local_col)]
					};
					expect_true!(mapped[(row, col)].sub(expected).norm() < 1e-12);
				}
			}
		}
	}
	expect_true!(
		encoding
			.visit_mapped_gates(&[0, 0], 0, 0, false, |_| Ok(()))
			.is_err()
	);
	expect_true!(
		encoding
			.visit_mapped_gates(&targets, 1, 1, false, |_| Ok(()))
			.is_err()
	);
	Ok(())
}

#[gtest]
fn callback_failure_stops_replay_and_expansion_obeys_budget() -> googletest::Result<()> {
	let source = sparse(
		2,
		3,
		vec![
			(0, 0, Complex64::new(1.0, 0.0)),
			(0, 1, Complex64::new(1.0, 0.0)),
		],
	)?;
	let encoding = MatchingEncoding::from_sparse(&source, NumericalPolicy::default())?;
	let mut count = 0_usize;
	let result = encoding.visit_gates(false, |_| {
		count = count
			.checked_add(1)
			.ok_or(quest_qsvt::Error::Budget("test count"))?;
		Err(quest_qsvt::Error::Budget("intentional visitor rejection"))
	});
	expect_true!(result.is_err());
	expect_that!(count, eq(1));
	expect_true!(
		encoding
			.to_oracle(NumericalPolicy { max_bytes: 1024 })
			.is_err()
	);
	Ok(())
}
