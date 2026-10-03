use quest_math::*;
use quest_synthesis::*;

fn operation(gate: Gate, targets: &[usize], controls: &[Control]) -> Operation {
	Operation {
		gate,
		targets: targets.to_vec(),
		controls: controls.to_vec(),
	}
}
fn options() -> SynthesisOptions {
	SynthesisOptions {
		limits: Limits {
			gates: 100_000,
			bytes: 1_000_000_000,
			..Limits::default()
		},
		max_work: 100_000_000,
		..SynthesisOptions::default()
	}
}
#[test]
fn exact_one_qubit_matrix_synthesis_retains_all_scalar_phases() {
	for gate in [Gate::H, Gate::T, Gate::Sdg, Gate::X, Gate::W] {
		let word = Sequence {
			qubits: 1,
			operations: vec![
				operation(Gate::H, &[0], &[]),
				operation(gate, if gate == Gate::W { &[] } else { &[0] }, &[]),
				operation(Gate::T, &[0], &[]),
			],
		};
		let target = reconstruct(&word, options().limits).unwrap();
		let result = synthesize_matrix(&target, options()).unwrap();
		assert_eq!(result.sequence().qubits, 1);
		assert_eq!(
			reconstruct(result.sequence(), options().limits).unwrap(),
			target
		);
	}
}

#[test]
fn controlled_h_and_phase_lower_to_elementary_gates_with_one_clean_wire() {
	for gate in [Gate::H, Gate::T, Gate::X] {
		let word = Sequence {
			qubits: 3,
			operations: vec![operation(
				gate,
				&[0],
				&[
					Control {
						qubit: 1,
						positive: false,
					},
					Control {
						qubit: 2,
						positive: true,
					},
				],
			)],
		};
		let target = reconstruct(&word, options().limits).unwrap();
		let result = synthesize_matrix(&target, options()).unwrap();
		assert_eq!(
			result.sequence().qubits,
			if gate == Gate::T { 4 } else { 3 }
		);
		assert!(
			result
				.sequence()
				.operations
				.iter()
				.all(|op| op.controls.is_empty())
		);
		verify_synthesis(
			&target,
			result.sequence(),
			result.proof(),
			options().limits,
			options().max_work,
		)
		.unwrap();
	}
}

#[test]
fn bad_unitaries_and_work_exhaustion_are_distinct() {
	let bad = ExactMatrix::from_entries(1, vec![Cyclotomic::one(); 4], options().limits).unwrap();
	assert!(matches!(
		synthesize_matrix(&bad, options()),
		Err(SynthesisError::NotUnitary)
	));
	let target = reconstruct(
		&Sequence {
			qubits: 1,
			operations: vec![operation(Gate::H, &[0], &[])],
		},
		options().limits,
	)
	.unwrap();
	assert!(matches!(
		synthesize_matrix(
			&target,
			SynthesisOptions {
				max_work: 0,
				..options()
			}
		),
		Err(SynthesisError::WorkExhausted { .. })
	));
}

#[test]
fn no_ancilla_obeys_determinant_conditions_and_synthesizes_admitted_gates() {
	let opt = SynthesisOptions {
		ancilla_policy: AncillaPolicy::NoAncilla,
		..options()
	};
	for qubits in 2..=4 {
		let allowed = Sequence {
			qubits,
			operations: vec![
				operation(Gate::T, &[0], &[]),
				operation(Gate::H, &[1], &[]),
				operation(Gate::Cx, &[0, 1], &[]),
			],
		};
		let matrix = reconstruct(&allowed, opt.limits).unwrap();
		let result = synthesize_matrix(&matrix, opt.clone()).unwrap();
		assert_eq!(result.sequence().qubits, qubits);
		assert_eq!(reconstruct(result.sequence(), opt.limits).unwrap(), matrix);
		let forbidden = Sequence {
			qubits,
			operations: vec![operation(
				Gate::T,
				&[0],
				&(1..qubits)
					.map(|qubit| Control {
						qubit,
						positive: true,
					})
					.collect::<Vec<_>>(),
			)],
		};
		assert!(matches!(
			synthesize_matrix(&reconstruct(&forbidden, opt.limits).unwrap(), opt.clone()),
			Err(SynthesisError::AncillaRequired { .. })
		));
	}
}

#[test]
fn matsumoto_amano_is_canonical_across_cancelling_words() {
	let base = Sequence {
		qubits: 1,
		operations: vec![
			operation(Gate::H, &[0], &[]),
			operation(Gate::T, &[0], &[]),
			operation(Gate::S, &[0], &[]),
			operation(Gate::H, &[0], &[]),
			operation(Gate::T, &[0], &[]),
		],
	};
	let mut redundant = base.clone();
	redundant
		.operations
		.extend([operation(Gate::H, &[0], &[]), operation(Gate::H, &[0], &[])]);
	let a = normalize_one_qubit(&base, options()).unwrap();
	let b = normalize_one_qubit(&redundant, options()).unwrap();
	assert_eq!(a, b);
	assert_eq!(
		reconstruct(a.sequence(), options().limits).unwrap(),
		reconstruct(&base, options().limits).unwrap()
	);
}

#[test]
fn nonadjacent_basis_rows_preserve_gray_path_phases() {
	let limits = options().limits;
	let h = Cyclotomic::new([0.into(), 1.into(), 0.into(), (-1).into()], 1, limits).unwrap();
	let mut entries = vec![Cyclotomic::zero(); 64];
	for row in 0..8 {
		entries[row * 8 + row] = Cyclotomic::one();
	}
	entries[0] = h.clone();
	entries[7] = h.clone();
	entries[56] = h.clone();
	entries[63] = h.checked_mul(&Cyclotomic::omega(4), limits).unwrap();
	let matrix = ExactMatrix::from_entries(3, entries, limits).unwrap();
	let result = synthesize_matrix(&matrix, options()).unwrap();
	assert_eq!(reconstruct(result.sequence(), limits).unwrap(), matrix);
	let mut proof = result.proof().clone();
	proof.block_ends[0] = usize::MAX;
	assert!(
		verify_synthesis(
			&matrix,
			result.sequence(),
			&proof,
			limits,
			options().max_work
		)
		.is_err()
	);
}
