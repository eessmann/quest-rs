use quest_math::*;
use quest_synthesis::*;

fn word(text: &str) -> std::result::Result<Sequence, &'static str> {
	Ok(Sequence {
		qubits: 1,
		operations: text
			.trim()
			.chars()
			.rev()
			.map(|c| {
				let gate = match c {
					'H' => Some(Gate::H),
					'S' => Some(Gate::S),
					'T' => Some(Gate::T),
					'X' => Some(Gate::X),
					'W' => Some(Gate::W),
					_ => None,
				}
				.ok_or("invalid pinned oracle fixture gate")?;
				Ok(Operation {
					gate,
					targets: if gate == Gate::W { vec![] } else { vec![0] },
					controls: vec![],
				})
			})
			.collect::<std::result::Result<_, _>>()?,
	})
}
fn cases() -> std::result::Result<[(Sequence, AngleTarget); 4], &'static str> {
	Ok([
		(
			word(include_str!("fixtures/newsynth-pi7.txt"))?,
			AngleTarget::RationalPi {
				numerator: 1.into(),
				denominator: 7.into(),
			},
		),
		(
			word(include_str!("fixtures/newsynth-affine.txt"))?,
			AngleTarget::AffinePi {
				radians_numerator: 1.into(),
				radians_denominator: 3.into(),
				pi_numerator: 1.into(),
				pi_denominator: 5.into(),
			},
		),
		(
			word(include_str!("fixtures/newsynth-pi4.txt"))?,
			AngleTarget::RationalPi {
				numerator: 1.into(),
				denominator: 4.into(),
			},
		),
		(
			word(include_str!("fixtures/newsynth-dyadic.txt"))?,
			AngleTarget::DyadicRadians {
				bits: 1.125_f64.to_bits(),
			},
		),
	])
}
#[test]
fn pinned_newsynth_outputs_pass_independent_full_phase_certification() {
	for (sequence, angle) in cases().unwrap() {
		let target = Target {
			axis: Axis::Z,
			angle,
		};
		certify_rotation(&sequence, &target, 1e-12_f64.to_bits(), Limits::default()).unwrap();
		let mut wrong = sequence;
		wrong.operations.push(Operation {
			gate: Gate::W,
			targets: vec![],
			controls: vec![],
		});
		assert!(certify_rotation(&wrong, &target, 1e-12_f64.to_bits(), Limits::default()).is_err());
	}
}
#[test]
fn oracle_exact_matrices_resynthesize_and_normalize_with_all_phase_retained() {
	let options = SynthesisOptions {
		limits: Limits {
			gates: 10_000,
			bytes: 100_000_000,
			..Limits::default()
		},
		max_work: 100_000_000,
		..SynthesisOptions::default()
	};
	for (sequence, _) in cases().unwrap() {
		let matrix = reconstruct(&sequence, options.limits).unwrap();
		let exact = synthesize_matrix(&matrix, options.clone()).unwrap();
		assert_eq!(
			reconstruct(exact.sequence(), options.limits).unwrap(),
			matrix
		);
		let normal = normalize_one_qubit(&sequence, options.clone()).unwrap();
		assert_eq!(
			reconstruct(normal.sequence(), options.limits).unwrap(),
			matrix
		);
	}
}
