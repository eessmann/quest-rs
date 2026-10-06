#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Small independent whole-unitary references"
)]
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::{
	Complex64, NumericalPolicy, OperandLayout, ReplayEncoding, ShiftRegister,
	StructuredStencilEncoding, TensorShiftEncoding, TransformBuilder, materialize_program,
	replay_transform::{ReplayTransform, TransformSchedule},
};

fn shift(width: usize, offset: usize) -> quest_qsvt::Result<TensorShiftEncoding> {
	TensorShiftEncoding::new(
		width,
		vec![ShiftRegister::new(0, width, offset)?],
		NumericalPolicy::default(),
	)
}

// Catches replacing general projectors/source replay with matching-only conventions,
// incorrect phases on failed sectors, and reversing the product incorrectly.
#[googletest::gtest]
fn arithmetic_stencil_schedule_matches_portable_whole_unitary_in_both_orientations()
-> googletest::Result<()> {
	let policy = NumericalPolicy::default();
	let encoding = StructuredStencilEncoding::new(
		1,
		vec![
			(Complex64::new(0.3, 0.4), shift(1, 1)?),
			(Complex64::new(-0.8, 0.1), shift(1, 0)?),
		],
		policy,
	)?;
	for phases in [vec![0.17, -0.23, -0.23, 0.17], vec![0.21, -0.3, 0.21]] {
		let sequence = PhaseSequence::<WxSymmetric>::builder(phases).build()?;
		let replay = ReplayTransform::new(encoding.clone(), sequence.clone(), policy)?;
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
		let matrix = materialize_program(portable.main(), policy)?;
		let initial: Vec<_> = (0..matrix.nrows())
			.map(|i| Complex64::new(f64::from(u32::try_from(i).unwrap_or(0)).sin(), 0.2))
			.collect();
		for adjoint in [false, true] {
			let mut actual = initial.clone();
			replay.apply_reference(&mut actual, adjoint, policy)?;
			for row in 0..actual.len() {
				let expected: Complex64 = (0..actual.len())
					.map(|col| {
						let value = if adjoint {
							matrix[(col, row)].conj()
						} else {
							matrix[(row, col)]
						};
						value * initial[col]
					})
					.sum();
				googletest::expect_true!((actual[row] - expected).norm() < 1e-11);
			}
		}
	}
	Ok(())
}

// Catches unconditional phases or H gates leaking across a signed outer control,
// wrong physical operand order, and loss of spectators.
#[googletest::gtest]
fn generic_transform_remaps_all_steps_and_signed_controls() -> googletest::Result<()> {
	let policy = NumericalPolicy::default();
	let encoding = shift(2, 1)?;
	let sequence = PhaseSequence::<WxSymmetric>::builder(vec![0.17, -0.23, -0.23, 0.17]).build()?;
	let replay = ReplayTransform::new(encoding.clone(), sequence.clone(), policy)?;
	let portable = TransformBuilder::new()
		.encoding(encoding.projected_encoding(policy)?)
		.operands(OperandLayout::new(3, vec![0, 1], 2, None)?)
		.standard(sequence)
		.build()?;
	let matrix = materialize_program(portable.main(), policy)?;
	let targets = [2, 0, 3]; // physical bit one is a spectator; bit four is control zero.
	let initial: Vec<_> = (0..32)
		.map(|i| {
			Complex64::new(
				f64::from(u32::try_from(i).unwrap_or(0)).cos(),
				f64::from(u32::try_from(i).unwrap_or(0)).sin(),
			)
		})
		.collect();
	for adjoint in [false, true] {
		let mut actual = initial.clone();
		replay.apply_mapped_reference(&mut actual, &targets, 16, 0, adjoint, policy)?;
		for row in 0..32 {
			let expected = if row & 16 != 0 {
				initial[row]
			} else {
				let local_row = targets.iter().enumerate().fold(0, |v, (local, physical)| {
					v | (((row >> physical) & 1) << local)
				});
				(0..8)
					.map(|col| {
						let physical_col = targets
							.iter()
							.enumerate()
							.fold(row & 0b1_0010, |v, (local, physical)| {
								v | (((col >> local) & 1) << physical)
							});
						let entry = if adjoint {
							matrix[(col, local_row)].conj()
						} else {
							matrix[(local_row, col)]
						};
						entry * initial[physical_col]
					})
					.sum()
			};
			googletest::expect_true!((actual[row] - expected).norm() < 1e-11);
		}
	}
	Ok(())
}

// Catches accepting phases prepared for a different source, layout, or whole unitary.
#[googletest::gtest]
fn schedules_reject_descriptor_mismatches_and_preserve_compact_large_spaces()
-> googletest::Result<()> {
	let policy = NumericalPolicy::default();
	let encoding = shift(2, 1)?;
	let descriptor = encoding.descriptor()?;
	for field in 0..3 {
		let mut mismatch = descriptor.clone();
		match field {
			0 => mismatch.source_identity ^= 1,
			1 => mismatch.construction_identity ^= 1,
			_ => {
				mismatch.layout.system_mask = 0;
				mismatch.layout.workspace_mask = 3;
			}
		}
		let schedule = TransformSchedule::from_parts(mismatch, vec![0.1, 0.2], 0.0, policy)?;
		googletest::expect_true!(
			ReplayTransform::from_schedule(encoding.clone(), schedule, policy).is_err()
		);
	}
	let large = shift(40, 19)?;
	let transform = ReplayTransform::new(
		large,
		PhaseSequence::<WxSymmetric>::builder(vec![0.1]).build()?,
		NumericalPolicy { max_bytes: 8192 },
	)?;
	googletest::expect_true!(transform.retained_bytes()? < 8192);
	let mut count = 0;
	transform.visit_gates(false, |_| {
		count += 1;
		Ok(())
	})?;
	googletest::expect_true!(count < 20);
	googletest::expect_true!(transform.apply_reference(&mut [], false, policy).is_err());
	Ok(())
}

#[derive(Clone, Debug)]
struct RestrictedShift(TensorShiftEncoding);
impl ReplayEncoding for RestrictedShift {
	fn descriptor(&self) -> quest_qsvt::Result<quest_qsvt::EncodingDescriptor> {
		let mut result = self.0.descriptor()?;
		result.rows = 2;
		result.cols = 3;
		result.left.logical_range = 1..3;
		result.right.logical_range = 0..3;
		result.source_identity ^= 0xabc;
		Ok(result)
	}
	fn retained_bytes(&self) -> quest_qsvt::Result<usize> {
		self.0.retained_bytes()
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		visitor: &mut dyn FnMut(quest_qsvt::ReplayGate) -> quest_qsvt::Result<()>,
	) -> quest_qsvt::Result<()> {
		self.0.visit_gates(adjoint, visitor)
	}
}

// Catches treating logical embedding order/ranges as flag-zero matching coordinates.
#[googletest::gtest]
fn external_source_with_rectangular_offset_projectors_drives_shared_schedule()
-> googletest::Result<()> {
	let policy = NumericalPolicy::default();
	let encoding = RestrictedShift(shift(2, 1)?);
	let descriptor = encoding.descriptor()?;
	let projected = quest_qsvt::EncodingBuilder::new()
		.oracle(encoding.replay_oracle(policy)?)
		.left(
			descriptor
				.left
				.logical_space::<quest_qsvt::Left>(2, policy)?,
		)
		.right(
			descriptor
				.right
				.logical_space::<quest_qsvt::Right>(2, policy)?,
		)
		.normalization(descriptor.normalization)?
		.policy(policy)
		.unitarity_assumption(quest_qsvt::ExplicitUnitaryPremise::new(
			"Exact reversible modular shift",
		)?)
		.build()?;
	let sequence = PhaseSequence::<WxSymmetric>::builder(vec![0.17, -0.23, -0.23, 0.17]).build()?;
	let portable = TransformBuilder::new()
		.encoding(projected)
		.operands(OperandLayout::new(3, vec![0, 1], 2, None)?)
		.standard(sequence.clone())
		.build()?;
	let expected = materialize_program(portable.main(), policy)?;
	let replay = ReplayTransform::new(encoding, sequence, policy)?;
	let actual = quest_qsvt::materialize_oracle(&replay.to_oracle(policy)?, policy)?;
	for row in 0..8 {
		for col in 0..8 {
			googletest::expect_true!((actual[(row, col)] - expected[(row, col)]).norm() < 1e-11);
		}
	}
	Ok(())
}

// Catches accepting descriptor promises inconsistent with clean initialization,
// and mutation before physical operand/storage admission.
#[googletest::gtest]
fn owning_contract_checks_clean_workspace_and_admission_before_mutation() -> googletest::Result<()>
{
	let policy = NumericalPolicy::default();
	let source =
		StructuredStencilEncoding::new(1, vec![(Complex64::new(0.3, 0.4), shift(1, 1)?)], policy)?;
	let mut descriptor = source.descriptor()?;
	descriptor.layout.clean_workspace_value = 1;
	googletest::expect_true!(descriptor.validate().is_err());
	let transform = ReplayTransform::new(
		shift(2, 1)?,
		PhaseSequence::<WxSymmetric>::builder(vec![0.1]).build()?,
		policy,
	)?;
	let initial = vec![Complex64::new(0.3, -0.2); 16];
	let mut state = initial.clone();
	googletest::expect_true!(
		transform
			.apply_mapped_reference(&mut state, &[0, 0, 2], 8, 0, false, policy)
			.is_err()
	);
	googletest::expect_true!(state == initial);
	googletest::expect_true!(
		transform
			.apply_mapped_reference(
				&mut state,
				&[0, 1, 2],
				8,
				0,
				false,
				NumericalPolicy { max_bytes: 1 }
			)
			.is_err()
	);
	googletest::expect_true!(state == initial);
	let mut retained = Vec::with_capacity(1024);
	retained.extend([0.1, 0.2]);
	googletest::expect_true!(
		TransformSchedule::from_parts(
			shift(2, 1)?.descriptor()?,
			retained,
			0.0,
			NumericalPolicy { max_bytes: 4096 }
		)
		.is_err()
	);
	let mut visited = 0;
	googletest::expect_true!(
		transform
			.visit_gates(false, |_| {
				visited += 1;
				Err(quest_qsvt::Error::Budget("visitor stopped"))
			})
			.is_err()
	);
	googletest::expect_true!(visited == 1);
	Ok(())
}

// Catches overflowing 2*angle when lowering a finite compact projector phase.
#[googletest::gtest]
fn finite_large_projector_phases_do_not_overflow_during_lowering() -> googletest::Result<()> {
	let policy = NumericalPolicy::default();
	let source = shift(2, 1)?;
	let schedule =
		TransformSchedule::from_parts(source.descriptor()?, vec![f64::MAX], 0.0, policy)?;
	let transform = ReplayTransform::from_schedule(source, schedule, policy)?;
	let mut state = vec![Complex64::new(0.0, 0.0); 8];
	state[0] = Complex64::new(1.0, 0.0);
	transform.apply_reference(&mut state, false, policy)?;
	googletest::expect_true!((state[0] - Complex64::new(f64::MAX.cos(), 0.0)).norm() < 1e-12);
	googletest::expect_true!((state[4] - Complex64::new(0.0, f64::MAX.sin())).norm() < 1e-12);
	Ok(())
}

// Catches using term ordering (a unitary construction choice) as operator identity.
#[googletest::gtest]
fn stencil_operator_identity_is_separate_from_color_order_construction() -> googletest::Result<()> {
	let policy = NumericalPolicy::default();
	let first = (Complex64::new(0.3, 0.4), shift(2, 1)?);
	let second = (Complex64::new(-0.8, 0.1), shift(2, 3)?);
	let a = StructuredStencilEncoding::new(2, vec![first.clone(), second.clone()], policy)?;
	let b = StructuredStencilEncoding::new(2, vec![second, first], policy)?;
	let left = a.descriptor()?;
	let right = b.descriptor()?;
	googletest::expect_true!(left.source_identity == right.source_identity);
	googletest::expect_true!(left.construction_identity != right.construction_identity);
	let schedule = TransformSchedule::from_parts(left, vec![0.1, 0.2], 0.0, policy)?;
	googletest::expect_true!(ReplayTransform::from_schedule(b, schedule, policy).is_err());
	Ok(())
}

// Catches charging logical phase length when the owning import retains much more capacity.
#[googletest::gtest]
fn conversion_admission_charges_owned_phase_capacity_for_both_constructors()
-> googletest::Result<()> {
	let policy = NumericalPolicy { max_bytes: 8192 };
	let shift_source = shift(2, 1)?;
	let matching_source = quest_qsvt::MatchingEncoding::from_sparse(
		&quest_numerics::SparseMatrix::from_triplets(
			1,
			1,
			quest_numerics::SparseFormat::Csr,
			vec![(0, 0, Complex64::new(0.3, 0.4))],
			quest_numerics::SparseLimits::default(),
		)?,
		policy,
	)?;
	let small = PhaseSequence::<WxSymmetric>::builder(vec![0.2, 0.2]).build()?;
	googletest::expect_true!(
		ReplayTransform::new(shift_source.clone(), small.clone(), policy).is_ok()
	);
	googletest::expect_true!(
		quest_qsvt::replay_transform::MatchingTransform::new(
			matching_source.clone(),
			small,
			policy
		)
		.is_ok()
	);
	let mut oversized = Vec::with_capacity(131_072);
	oversized.extend([0.2, 0.2]);
	let retained = PhaseSequence::<WxSymmetric>::builder(oversized).build()?;
	googletest::expect_true!(matches!(
		ReplayTransform::new(shift_source, retained.clone(), policy),
		Err(quest_qsvt::Error::Budget(_))
	));
	googletest::expect_true!(matches!(
		quest_qsvt::replay_transform::MatchingTransform::new(matching_source, retained, policy),
		Err(quest_qsvt::Error::Budget(_))
	));
	Ok(())
}
