use quest_math::*;

#[test]
fn empty_identity_proof_still_charges_dense_replay_work() {
	let limits = Limits::default();
	let target = ExactMatrix::identity(1, limits).unwrap();
	let word = Sequence {
		qubits: 1,
		operations: vec![],
	};
	let proof = SynthesisProof {
		reduction: vec![],
		block_ends: vec![],
		clean_ancilla: false,
	};
	assert!(verify_synthesis(&target, &word, &proof, limits, 0).is_err());
	assert!(
		verify_synthesis(&target, &word, &proof, limits, 100)
			.unwrap()
			.work()
			>= 8
	);
}

#[test]
fn combined_synthesis_storage_is_admitted_before_proof_and_output_allocation() {
	let limits = Limits::default();
	let base = admit_synthesis_storage(1, 0, 0, limits).unwrap();
	let tight = Limits {
		bytes: usize::try_from(base).unwrap() + 1024,
		..limits
	};
	assert!(admit_synthesis_storage(1, 1, 0, tight).is_ok());
	assert!(admit_synthesis_storage(1, 1, 1, tight).is_err());
	assert!(admit_synthesis_storage(1, usize::MAX, 1, limits).is_err());
}

fn op(gate: Gate, targets: &[usize]) -> Operation {
	Operation {
		gate,
		targets: targets.to_vec(),
		controls: vec![],
	}
}

#[test]
fn independent_trace_checker_rejects_word_and_trace_corruption() {
	let limits = Limits::default();
	let word = Sequence {
		qubits: 1,
		operations: vec![op(Gate::H, &[0])],
	};
	let target = reconstruct(&word, limits).unwrap();
	let proof = SynthesisProof {
		reduction: vec![RowOperation::Hadamard(BasisIndex(0), BasisIndex(1))],
		block_ends: vec![1],
		clean_ancilla: false,
	};
	verify_synthesis(&target, &word, &proof, limits, 10000).unwrap();
	let wrong = Sequence {
		qubits: 1,
		operations: vec![op(Gate::X, &[0])],
	};
	assert!(verify_synthesis(&target, &wrong, &proof, limits, 10000).is_err());
	let mut corrupt = proof.clone();
	corrupt.reduction[0] = RowOperation::Swap(BasisIndex(0), BasisIndex(1));
	assert!(verify_synthesis(&target, &word, &corrupt, limits, 10000).is_err());
	let mut disguised = word.clone();
	disguised.operations[0].controls.push(Control {
		qubit: 0,
		positive: true,
	});
	assert!(verify_synthesis(&target, &disguised, &proof, limits, 10000).is_err());
	assert!(
		verify_synthesis(
			&target,
			&word,
			&proof,
			Limits {
				bytes: 100,
				..limits
			},
			10000
		)
		.is_err()
	);
}

#[test]
fn clean_contract_does_not_require_an_identity_action_on_occupied_ancilla() {
	let limits = Limits::default();
	let target = ExactMatrix::identity(1, limits).unwrap();
	let word = Sequence {
		qubits: 2,
		operations: vec![op(Gate::Z, &[1])],
	};
	// omega^0 on a row is an identity row operation, with a nontrivial extension.
	let proof = SynthesisProof {
		reduction: vec![RowOperation::Phase(BasisIndex(0), 0)],
		block_ends: vec![1],
		clean_ancilla: true,
	};
	verify_synthesis(&target, &word, &proof, limits, 10000).unwrap();
	let dirty_return = Sequence {
		qubits: 2,
		operations: vec![op(Gate::X, &[1])],
	};
	assert!(verify_synthesis(&target, &dirty_return, &proof, limits, 10000).is_err());
	assert!(verify_synthesis(&target, &word, &proof, limits, 0).is_err());
}
