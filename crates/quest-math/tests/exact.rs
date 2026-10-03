use googletest::{Result, prelude::*};
use quest_math::{
	Control, Cyclotomic, Gate, Limits, Operation, Sequence, reconstruct, recover_eighth_root_phase,
	verify_exact,
};
fn op(gate: Gate, targets: &[usize]) -> Operation {
	Operation {
		gate,
		targets: targets.to_vec(),
		controls: vec![],
	}
}
const fn sequence(qubits: usize, operations: Vec<Operation>) -> Sequence {
	Sequence { qubits, operations }
}
#[gtest]
fn exact_matrices_preserve_product_order_and_scalar_phase() -> Result<()> {
	let limits = Limits::default();
	let id = sequence(1, vec![]);
	verify_exact(
		&sequence(1, vec![op(Gate::H, &[0]), op(Gate::H, &[0])]),
		&id,
		limits,
	)?;
	let ht = sequence(1, vec![op(Gate::H, &[0]), op(Gate::T, &[0])]);
	let th = sequence(1, vec![op(Gate::T, &[0]), op(Gate::H, &[0])]);
	expect_true!(verify_exact(&ht, &th, limits).is_err());
	expect_true!(verify_exact(&sequence(1, vec![op(Gate::W, &[])]), &id, limits).is_err());
	let restored = recover_eighth_root_phase(&sequence(1, vec![op(Gate::W, &[])]), &id, limits)?;
	expect_eq!(restored.phase_power(), 7);
	verify_exact(restored.sequence(), &id, limits)?;
	Ok(())
}
#[gtest]
fn signed_controls_and_nonsorted_targets_match_independent_basis_maps() -> Result<()> {
	let limits = Limits::default();
	let mut gate = op(Gate::Cx, &[2, 0]);
	gate.controls = vec![Control {
		qubit: 1,
		positive: false,
	}];
	let matrix = reconstruct(&sequence(3, vec![gate]), limits)?;
	for row in 0..8usize {
		for column in 0..8usize {
			let expected_row = if column & 4 != 0 && column & 2 == 0 {
				column ^ 1
			} else {
				column
			};
			let offset = row
				.checked_mul(8)
				.and_then(|v| v.checked_add(column))
				.ok_or_else(|| std::io::Error::other("fixture offset"))?;
			let expected = if row == expected_row {
				Cyclotomic::one()
			} else {
				Cyclotomic::zero()
			};
			expect_eq!(matrix.entries().get(offset), Some(&expected));
		}
	}
	Ok(())
}
#[gtest]
fn bounds_and_duplicate_operands_are_checked_before_matrix_work() {
	let mut repeated = op(Gate::H, &[0]);
	repeated.controls = vec![Control {
		qubit: 0,
		positive: true,
	}];
	expect_true!(reconstruct(&sequence(1, vec![repeated]), Limits::default()).is_err());
	expect_true!(reconstruct(&sequence(5, vec![]), Limits::default()).is_err());
	expect_true!(
		reconstruct(
			&sequence(1, vec![op(Gate::X, &[0])]),
			Limits {
				gates: 0,
				..Limits::default()
			}
		)
		.is_err()
	);
	expect_true!(
		reconstruct(
			&sequence(4, vec![]),
			Limits {
				bytes: 16,
				..Limits::default()
			}
		)
		.is_err()
	);
}

#[gtest]
fn hadamard_then_t_has_independent_exact_coefficients() -> Result<()> {
	let limits = Limits::default();
	let matrix = reconstruct(
		&sequence(1, vec![op(Gate::H, &[0]), op(Gate::T, &[0])]),
		limits,
	)?;
	let expected = [[0, 1, 0, -1], [0, 1, 0, -1], [1, 0, 1, 0], [-1, 0, -1, 0]]
		.into_iter()
		.map(|coefficients| Cyclotomic::new(coefficients.map(dashu_int::IBig::from), 1, limits))
		.collect::<quest_math::Result<Vec<_>>>()?;
	expect_eq!(matrix.entries(), expected.as_slice());
	Ok(())
}
#[gtest]
fn all_primitive_inverses_and_controlled_scalar_phases_are_exact() -> Result<()> {
	let limits = Limits::default();
	for (left, right, width, targets) in [
		(Gate::Y, Gate::Y, 1, vec![0]),
		(Gate::S, Gate::Sdg, 1, vec![0]),
		(Gate::T, Gate::Tdg, 1, vec![0]),
		(Gate::Cx, Gate::Cx, 2, vec![0, 1]),
		(Gate::Cz, Gate::Cz, 2, vec![1, 0]),
		(Gate::Swap, Gate::Swap, 2, vec![1, 0]),
	] {
		verify_exact(
			&sequence(width, vec![op(left, &targets), op(right, &targets)]),
			&sequence(width, vec![]),
			limits,
		)?;
	}
	verify_exact(
		&sequence(1, vec![op(Gate::X, &[0]), op(Gate::Y, &[0])]),
		&sequence(
			1,
			vec![
				op(Gate::W, &[]),
				op(Gate::W, &[]),
				op(Gate::W, &[]),
				op(Gate::W, &[]),
				op(Gate::W, &[]),
				op(Gate::W, &[]),
				op(Gate::Z, &[0]),
			],
		),
		limits,
	)?;
	for (positive, expected) in [
		(true, vec![op(Gate::T, &[0])]),
		(false, vec![op(Gate::W, &[]), op(Gate::Tdg, &[0])]),
	] {
		let mut phase = op(Gate::W, &[]);
		phase.controls.push(Control { qubit: 0, positive });
		verify_exact(&sequence(1, vec![phase]), &sequence(1, expected), limits)?;
	}
	expect_true!(
		recover_eighth_root_phase(
			&sequence(1, vec![op(Gate::H, &[0])]),
			&sequence(1, vec![]),
			limits
		)
		.is_err()
	);
	Ok(())
}

#[gtest]
fn sequence_and_matrix_allocations_share_one_budget() {
	let limits = Limits {
		coefficient_bits: 8,
		bytes: 11_700,
		..Limits::default()
	};
	expect_true!(reconstruct(&sequence(1, vec![op(Gate::X, &[0])]), limits).is_err());
}
