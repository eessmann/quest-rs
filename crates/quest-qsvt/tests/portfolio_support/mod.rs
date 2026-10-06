#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Independent finite bounded primitive state reference and full-sector assertions"
)]
use quest_qsvt::{Complex64, ReplayEncoding, ReplayGate, ReplayKind};
fn apply(state: &mut [Complex64], g: ReplayGate) -> quest_qsvt::Result<()> {
	assert_eq!(g.control_value & !g.control_mask, 0);
	if let Some(t) = g.target {
		assert_eq!(g.control_mask & (1 << t), 0);
	}
	for i in 0..state.len() {
		if i & g.control_mask != g.control_value {
			continue;
		}
		if let ReplayKind::Phase(a) = g.kind {
			state[i] *= Complex64::from_polar(1.0, a);
			continue;
		}
		let target = 1
			<< g.target
				.ok_or(quest_qsvt::Error::Encoding("reference target"))?;
		if i & target != 0 {
			continue;
		}
		let (a, b) = (state[i], state[i | target]);
		match g.kind {
			ReplayKind::X => {
				state[i] = b;
				state[i | target] = a;
			}
			ReplayKind::H => {
				state[i] = (a + b) * std::f64::consts::FRAC_1_SQRT_2;
				state[i | target] = (a - b) * std::f64::consts::FRAC_1_SQRT_2;
			}
			ReplayKind::Ry(angle) => {
				let (sin, cos) = (0.5 * angle).sin_cos();
				state[i] = cos * a - sin * b;
				state[i | target] = sin * a + cos * b;
			}
			ReplayKind::Phase(_) => {
				return Err(quest_qsvt::Error::Encoding("reference phase branch"));
			}
		}
	}
	Ok(())
}
/// Covers every failure/workspace/padding sector with arbitrary amplitudes,
/// controls of both signs, a spectator and reverse-order operand mapping.
pub fn whole_register<E: ReplayEncoding>(source: &E) -> quest_qsvt::Result<()> {
	let width = source.descriptor()?.layout.num_qubits;
	assert!(width <= 10);
	let dimension = 1 << width;
	let targets: Vec<_> = (2..width + 2).rev().collect();
	let initial: Vec<_> = (0..dimension * 4)
		.map(|i| {
			let x = f64::from(u32::try_from(i).unwrap_or(0));
			Complex64::new((0.17 * x).sin(), (0.13 * x).cos())
		})
		.collect();
	for control in 0..2 {
		let mut expected = initial.clone();
		for spectator in 0..2 {
			let mut branch = vec![Complex64::default(); dimension];
			for (index, v) in branch.iter_mut().enumerate() {
				let mut physical = control | (spectator << 1);
				for (b, t) in targets.iter().enumerate() {
					physical |= ((index >> b) & 1) << t;
				}
				*v = initial[physical];
			}
			source.visit_replay(false, &mut |g| {
				apply(&mut branch, g)?;
				Ok(())
			})?;
			for (index, v) in branch.iter().enumerate() {
				let mut physical = control | (spectator << 1);
				for (b, t) in targets.iter().enumerate() {
					physical |= ((index >> b) & 1) << t;
				}
				expected[physical] = *v;
			}
		}
		let mut actual = initial.clone();
		source.visit_mapped_replay(&targets, 1, control, false, &mut |g| {
			apply(&mut actual, g)?;
			Ok(())
		})?;
		for (a, e) in actual.iter().zip(&expected) {
			assert!((*a - *e).norm() < 3e-12);
		}
		source.visit_mapped_replay(&targets, 1, control, true, &mut |g| {
			apply(&mut actual, g)?;
			Ok(())
		})?;
		for (a, e) in actual.iter().zip(&initial) {
			assert!((*a - *e).norm() < 5e-12);
		}
	}
	Ok(())
}
