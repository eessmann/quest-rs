#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Independent bounded state-vector comparisons"
)]
use quest_qsvt::{
	Complex64, ReplayGate, ReplayKind,
	state_preparation::{AmplitudePreparation, PreparationLimits},
};
fn apply(state: &mut [Complex64], gate: ReplayGate) -> quest_qsvt::Result<()> {
	let target = gate.target.map_or(0, |n| 1 << n);
	for i in 0..state.len() {
		if i & gate.control_mask != gate.control_value {
			continue;
		}
		if let ReplayKind::Phase(angle) = gate.kind {
			state[i] *= Complex64::from_polar(1.0, angle);
			continue;
		}
		if i & target != 0 {
			continue;
		}
		let (a, b) = (state[i], state[i | target]);
		match gate.kind {
			ReplayKind::Ry(angle) => {
				let (s, c) = (0.5 * angle).sin_cos();
				state[i] = c * a - s * b;
				state[i | target] = s * a + c * b;
			}
			ReplayKind::X => {
				state[i] = b;
				state[i | target] = a;
			}
			_ => return Err(quest_qsvt::Error::Encoding("unexpected preparation gate")),
		}
	}
	Ok(())
}
#[googletest::gtest]
fn coherent_multiplexors_prepare_complex_padded_rhs_and_invert_on_every_branch()
-> googletest::Result<()> {
	for rhs in [
		vec![Complex64::new(-2.0, 0.0)],
		vec![
			Complex64::new(1.0, -2.0),
			Complex64::new(0.0, 0.0),
			Complex64::new(-3.0, 0.25),
		],
		vec![
			Complex64::new(0.0, 1.0),
			Complex64::new(0.0, -1.0),
			Complex64::new(1.0, 0.0),
			Complex64::new(-1.0, 0.0),
			Complex64::new(0.0, 0.0),
		],
	] {
		let preparation = AmplitudePreparation::new(&rhs, PreparationLimits::default())?;
		let size = 1 << preparation.qubits();
		let mut prepared = vec![Complex64::default(); size];
		prepared[0] = 1.0.into();
		let mut count = 0;
		preparation.visit_gates(false, &mut |gate| {
			count += 1;
			apply(&mut prepared, gate)
		})?;
		googletest::expect_eq!(count, preparation.resources().elementary_gates);
		googletest::expect_true!(count <= 5 * size);
		for (i, value) in prepared.iter().enumerate() {
			let expected = rhs.get(i).copied().unwrap_or_default() / preparation.norm();
			googletest::expect_true!((*value - expected).norm() < 2e-14);
		}
		let arbitrary: Vec<_> = (0..size)
			.map(|i| Complex64::new(f64::from(u32::try_from(i).unwrap_or(0)).sin(), 0.2))
			.collect();
		let mut state = arbitrary.clone();
		preparation.visit_gates(false, &mut |gate| apply(&mut state, gate))?;
		preparation.visit_gates(true, &mut |gate| apply(&mut state, gate))?;
		for (a, b) in state.iter().zip(&arbitrary) {
			googletest::expect_true!((*a - *b).norm() < 3e-14);
		}
	}
	Ok(())
}
#[googletest::gtest]
fn preparation_rejects_zero_and_precharges_table_work_and_storage() {
	googletest::expect_true!(
		AmplitudePreparation::new(&[Complex64::default(); 4], PreparationLimits::default())
			.is_err()
	);
	for limits in [
		PreparationLimits {
			max_bytes: 1,
			..Default::default()
		},
		PreparationLimits {
			max_compile_work: 1,
			..Default::default()
		},
		PreparationLimits {
			max_gates: 1,
			..Default::default()
		},
	] {
		googletest::expect_true!(
			AmplitudePreparation::new(&[Complex64::new(1.0, 0.0); 4], limits).is_err()
		);
	}
}

#[googletest::gtest]
fn coherent_preparation_retains_phase_under_negative_outer_controls() -> googletest::Result<()> {
	let rhs = [
		Complex64::new(-0.25, 0.8),
		Complex64::new(0.0, 0.0),
		Complex64::new(0.7, -0.5),
	];
	let preparation = AmplitudePreparation::new(&rhs, PreparationLimits::default())?;
	let initial: Vec<_> = (0..16)
		.map(|i| Complex64::new(f64::from(i).sin(), f64::from(i).cos()))
		.collect();
	let mut state = initial.clone();
	preparation.visit_mapped_gates(&[2, 0], 1 << 3, 0, false, &mut |gate| {
		apply(&mut state, gate)
	})?;
	for i in 8..16 {
		googletest::expect_eq!(state[i], initial[i]);
	}
	preparation.visit_mapped_gates(&[2, 0], 1 << 3, 0, true, &mut |gate| {
		apply(&mut state, gate)
	})?;
	for (a, b) in state.iter().zip(&initial) {
		googletest::expect_true!((*a - *b).norm() < 3e-14);
	}
	let mut normalized = vec![Complex64::default(); 4];
	normalized[0] = 1.0.into();
	preparation.visit_gates(false, &mut |gate| apply(&mut normalized, gate))?;
	let mut mapped = vec![Complex64::default(); 16];
	mapped[2] = 1.0.into(); // spectator bit1 remains1
	preparation.visit_mapped_gates(&[2, 0], 1 << 3, 0, false, &mut |gate| {
		apply(&mut mapped, gate)
	})?;
	for i in 0..4 {
		googletest::expect_true!(
			(mapped[2 | ((i & 1) << 2) | ((i & 2) >> 1)] - normalized[i]).norm() < 3e-14
		);
	}
	Ok(())
}
