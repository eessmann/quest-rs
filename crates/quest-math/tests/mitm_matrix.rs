use googletest::{Result, prelude::*};
use quest_math::{ExactMatrix, Gate, Limits, Operation, Sequence, reconstruct};

fn operation(gate: Gate) -> Operation {
	Operation {
		gate,
		targets: if gate == Gate::W { vec![] } else { vec![0] },
		controls: vec![],
	}
}

#[gtest]
fn checked_small_matrix_product_matches_chronological_reconstruction() -> Result<()> {
	let limits = Limits::default();
	let h = ExactMatrix::for_operation(1, &operation(Gate::H), limits)?;
	let t = ExactMatrix::for_operation(1, &operation(Gate::T), limits)?;
	let chronological = Sequence {
		qubits: 1,
		operations: vec![operation(Gate::H), operation(Gate::T)],
	};
	expect_eq!(
		t.multiply(&h, limits)?,
		reconstruct(&chronological, limits)?
	);
	expect_eq!(
		h.adjoint(limits)?.multiply(&h, limits)?,
		ExactMatrix::identity(1, limits)?
	);
	Ok(())
}

#[gtest]
fn exact_matrix_key_preserves_global_phase() -> Result<()> {
	let limits = Limits::default();
	let identity = ExactMatrix::identity(1, limits)?;
	let negative = ExactMatrix::for_operation(1, &operation(Gate::W), limits)?
		.multiply(
			&ExactMatrix::for_operation(1, &operation(Gate::W), limits)?,
			limits,
		)?
		.multiply(
			&ExactMatrix::for_operation(1, &operation(Gate::W), limits)?,
			limits,
		)?
		.multiply(
			&ExactMatrix::for_operation(1, &operation(Gate::W), limits)?,
			limits,
		)?;
	let omega = ExactMatrix::for_operation(1, &operation(Gate::W), limits)?;
	expect_ne!(
		identity.full_phase_key(limits)?,
		negative.full_phase_key(limits)?
	);
	expect_ne!(
		identity.full_phase_key(limits)?,
		omega.full_phase_key(limits)?
	);
	Ok(())
}
