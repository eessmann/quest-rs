#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Bounded independent full-unitary matrices and explicit failure assertions"
)]
use quest_qsvt::{
	Complex64, EncodingDescriptor, NumericalPolicy, ReplayEncoding, ReplayGate, ReplayKind,
	ShiftRegister, TensorShiftEncoding,
	portfolio::{LcuPlan, LcuPlanLimits, LcuStep, PortfolioLimits, WeightedLcu},
	state_preparation::{AmplitudePreparation, PreparationLimits},
};

fn child(offset: usize) -> quest_qsvt::Result<TensorShiftEncoding> {
	TensorShiftEncoding::new(
		1,
		vec![ShiftRegister::new(0, 1, offset)?],
		NumericalPolicy::default(),
	)
}
fn inputs() -> quest_qsvt::Result<Vec<(Complex64, EncodingDescriptor)>> {
	Ok(vec![
		(Complex64::new(0., 2.), child(1)?.descriptor()?),
		(Complex64::new(-0., 0.), child(0)?.descriptor()?),
		(Complex64::new(-0.25, 0.), child(0)?.descriptor()?),
		(Complex64::new(0.1, -0.2), child(1)?.descriptor()?),
	])
}
fn apply(state: &mut [Complex64], gate: ReplayGate) {
	for i in 0..state.len() {
		if i & gate.control_mask != gate.control_value {
			continue;
		}
		if let ReplayKind::Phase(angle) = gate.kind {
			state[i] *= Complex64::from_polar(1., angle);
			continue;
		}
		let bit = 1 << gate.target.unwrap_or(0);
		if i & bit != 0 {
			continue;
		}
		let (a, b) = (state[i], state[i | bit]);
		match gate.kind {
			ReplayKind::X => (state[i], state[i | bit]) = (b, a),
			ReplayKind::H => {
				state[i] = (a + b) * std::f64::consts::FRAC_1_SQRT_2;
				state[i | bit] = (a - b) * std::f64::consts::FRAC_1_SQRT_2;
			}
			ReplayKind::Ry(angle) => {
				let (s, c) = (angle * 0.5).sin_cos();
				state[i] = c * a - s * b;
				state[i | bit] = s * a + c * b;
			}
			ReplayKind::Phase(_) => {}
		}
	}
}

#[googletest::gtest]
fn shared_selector_matches_independent_whole_unitary_and_standalone_adjoint()
-> googletest::Result<()> {
	let plan = LcuPlan::new(inputs()?, LcuPlanLimits::default())?;
	googletest::expect_eq!(plan.surviving_indices(), &[0, 2, 3]);
	googletest::expect_eq!(plan.child_width(), 1);
	googletest::expect_eq!(plan.selector_qubits(), 2);
	let weights = [
		Complex64::new(0., 2.),
		(-0.25).into(),
		Complex64::new(0.1, -0.2),
	];
	let amplitudes: Vec<_> = weights
		.iter()
		.map(|w| Complex64::new(w.norm().sqrt(), 0.))
		.collect();
	let prep = AmplitudePreparation::new(&amplitudes, PreparationLimits::default())?;
	let mut p = [[Complex64::default(); 4]; 4];
	for col in 0..4 {
		let mut state = [Complex64::default(); 4];
		state[col] = 1.0.into();
		prep.apply_reference(&mut state, &[0, 1], false, NumericalPolicy::default(), 1000)?;
		for row in 0..4 {
			p[row][col] = state[row];
		}
	}
	let mut expected = [[Complex64::default(); 8]; 8];
	for row in 0..8 {
		for col in 0..8 {
			for label in 0..4 {
				let flip = usize::from(label == 0 || label == 2);
				if row & 1 != (col & 1) ^ flip {
					continue;
				}
				let phase = if label < 3 {
					Complex64::from_polar(1., weights[label].arg())
				} else {
					1.0.into()
				};
				expected[row][col] += p[label][row >> 1].conj() * phase * p[label][col >> 1];
			}
		}
	}
	let targets = [3, 2, 1]; // spectator bit 4, signed outer control bit 0
	let extract = |physical: usize| {
		targets
			.iter()
			.enumerate()
			.fold(0, |v, (b, t)| v | (((physical >> t) & 1) << b))
	};
	let initial: Vec<_> = (0..32)
		.map(|i| {
			let x = f64::from(i);
			Complex64::new((0.13 * x).cos(), (0.17 * x).sin())
		})
		.collect();
	for adjoint in [false, true] {
		for value in [0, 1] {
			let mut state = initial.clone();
			let mut primitives = 0;
			let mut children = 0;
			plan.visit_mapped_steps(
				&targets,
				1,
				value,
				adjoint,
				|step| -> quest_qsvt::Result<()> {
					match step {
						LcuStep::Gate(gate) => {
							primitives += 1;
							apply(&mut state, gate);
						}
						LcuStep::Child {
							index,
							adjoint: direction,
							control_mask,
							control_value,
						} => {
							assert_eq!(direction, adjoint);
							children += 1;
							if index == 0 || index == 2 {
								apply(
									&mut state,
									ReplayGate {
										kind: ReplayKind::X,
										target: Some(targets[0]),
										control_mask,
										control_value,
									},
								);
							}
						}
					}
					Ok(())
				},
			)?;
			googletest::expect_eq!(primitives, plan.resources().primitive_gates);
			googletest::expect_eq!(children, 3);
			for row in 0..32 {
				let mut wanted = initial[row];
				if row & 1 == value {
					wanted = Complex64::default();
					for col in 0..32 {
						if col & 0b10001 != row & 0b10001 {
							continue;
						}
						let coefficient = if adjoint {
							expected[extract(col)][extract(row)].conj()
						} else {
							expected[extract(row)][extract(col)]
						};
						wanted += coefficient * initial[col];
					}
				}
				assert!((state[row] - wanted).norm() < 3e-12);
			}
		}
	}
	googletest::expect_eq!(plan.descriptor().errors.preparation, None);
	googletest::expect_eq!(plan.descriptor().errors.encoding, None);
	let owned = WeightedLcu::new(
		vec![
			(Complex64::new(0., 2.), child(1)?),
			(Complex64::new(-0., 0.), child(0)?),
			(Complex64::new(-0.25, 0.), child(0)?),
			(Complex64::new(0.1, -0.2), child(1)?),
		],
		PortfolioLimits::default(),
	)?;
	googletest::expect_eq!(owned.descriptor()?, plan.descriptor().clone());
	Ok(())
}

#[derive(Debug, PartialEq)]
enum VisitorError {
	Source,
	Stop,
}
impl From<quest_qsvt::Error> for VisitorError {
	fn from(_: quest_qsvt::Error) -> Self {
		Self::Source
	}
}

#[googletest::gtest]
fn mapped_selector_preserves_visitor_error_and_rejects_layout_before_emission()
-> googletest::Result<()> {
	let plan = LcuPlan::new(inputs()?, LcuPlanLimits::default())?;
	let mut emitted = 0;
	let result =
		plan.visit_mapped_steps(&[0, 0, 2], 0, 0, false, |_| -> Result<(), VisitorError> {
			emitted += 1;
			Ok(())
		});
	googletest::expect_eq!(result, Err(VisitorError::Source));
	googletest::expect_eq!(emitted, 0);
	let result =
		plan.visit_mapped_steps(&[0, 1, 2], 0, 0, false, |_| -> Result<(), VisitorError> {
			emitted += 1;
			Err(VisitorError::Stop)
		});
	googletest::expect_eq!(result, Err(VisitorError::Stop));
	googletest::expect_eq!(emitted, 1);
	Ok(())
}

#[googletest::gtest]
fn selector_retains_underflowing_nonzero_term_and_identity_of_zero_inputs() -> googletest::Result<()>
{
	let mut tiny = child(0)?.descriptor()?;
	tiny.normalization = 1e-200;
	let terms = vec![
		(1.0.into(), child(0)?.descriptor()?),
		(Complex64::new(1e-200, 0.), tiny),
	];
	let plan = LcuPlan::new(terms, LcuPlanLimits::default())?;
	googletest::expect_eq!(plan.resources().selected_terms, 2);
	let mut visits = 0;
	plan.visit_mapped_steps(&[0, 1], 0, 0, true, |step| -> quest_qsvt::Result<()> {
		visits += usize::from(matches!(step, LcuStep::Child { .. }));
		Ok(())
	})?;
	googletest::expect_eq!(visits, 2);
	let with_zero = LcuPlan::new(inputs()?, LcuPlanLimits::default())?;
	let mut changed = inputs()?;
	changed[1].1.source_identity ^= 1;
	let changed = LcuPlan::new(changed, LcuPlanLimits::default())?;
	googletest::expect_ne!(
		with_zero.descriptor().source_identity,
		changed.descriptor().source_identity
	);
	let mut invalid = inputs()?;
	invalid[1].1.rows = 3;
	googletest::expect_true!(LcuPlan::new(invalid, LcuPlanLimits::default()).is_err());
	Ok(())
}

#[googletest::gtest]
fn selector_admits_capacity_shared_work_and_returned_payload() -> googletest::Result<()> {
	let terms = inputs()?;
	let baseline = LcuPlan::new(terms.clone(), LcuPlanLimits::default())?;
	let resources = baseline.resources();
	googletest::expect_eq!(baseline.retained_bytes()?, resources.retained_bytes);
	googletest::expect_true!(
		resources.construction_peak_bytes
			>= resources.retained_bytes
				+ terms.capacity() * size_of::<(Complex64, EncodingDescriptor)>()
	);
	googletest::expect_eq!(
		baseline.preparation().retained_bytes()?,
		baseline.preparation().resources().retained_bytes
	);
	googletest::expect_eq!(
		resources.compile_work,
		resources.metadata_compile_work + resources.preparation_compile_work
	);
	googletest::expect_eq!(
		resources.primitive_gates,
		resources.preparation_gates + resources.selected_terms
	);
	googletest::expect_true!(
		LcuPlan::new(
			terms.clone(),
			LcuPlanLimits {
				max_compile_work: resources.compile_work - 1,
				..Default::default()
			}
		)
		.is_err()
	);
	googletest::expect_true!(
		LcuPlan::new(
			terms.clone(),
			LcuPlanLimits {
				max_primitives: resources.primitive_gates - 1,
				..Default::default()
			}
		)
		.is_err()
	);
	googletest::expect_true!(
		LcuPlan::new(
			terms.clone(),
			LcuPlanLimits {
				max_bytes: resources.construction_peak_bytes - 1,
				..Default::default()
			}
		)
		.is_err()
	);
	let mut oversized = Vec::with_capacity(4096);
	oversized.extend(terms);
	googletest::expect_true!(
		LcuPlan::new(
			oversized,
			LcuPlanLimits {
				max_bytes: resources.construction_peak_bytes,
				..Default::default()
			}
		)
		.is_err()
	);
	Ok(())
}

#[googletest::gtest]
fn selector_rejects_zero_nonfinite_and_incompatible_inputs() -> googletest::Result<()> {
	googletest::expect_true!(LcuPlan::new(vec![], LcuPlanLimits::default()).is_err());
	let mut zero = inputs()?;
	for (weight, _) in &mut zero {
		*weight = 0.0.into();
	}
	googletest::expect_true!(LcuPlan::new(zero, LcuPlanLimits::default()).is_err());
	for bad in [
		Complex64::new(f64::NAN, 0.),
		Complex64::new(0., f64::INFINITY),
	] {
		let mut terms = inputs()?;
		terms[1].0 = bad;
		googletest::expect_true!(LcuPlan::new(terms, LcuPlanLimits::default()).is_err());
	}
	let mut incompatible = inputs()?;
	incompatible[1].1.rows = 1;
	incompatible[1].1.left.logical_range = 0..1;
	googletest::expect_true!(LcuPlan::new(incompatible, LcuPlanLimits::default()).is_err());
	googletest::expect_true!(
		LcuPlan::new(
			inputs()?,
			LcuPlanLimits {
				max_terms: 3,
				..Default::default()
			}
		)
		.is_err()
	);
	let one = LcuPlan::new(
		vec![(Complex64::new(0., -0.5), child(1)?.descriptor()?)],
		LcuPlanLimits::default(),
	)?;
	googletest::expect_eq!(one.selector_qubits(), 0);
	googletest::expect_eq!(one.surviving_indices(), &[0]);
	let mut count = 0;
	one.visit_mapped_steps(&[2], 1, 0, false, |_| -> quest_qsvt::Result<()> {
		count += 1;
		Ok(())
	})?;
	googletest::expect_eq!(count, one.resources().primitive_gates + 1);
	Ok(())
}
