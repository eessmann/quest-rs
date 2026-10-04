use googletest::prelude::*;
use quest_qsvt::{Left, LogicalSpace, NumericalPolicy, Right};
use std::ops::Sub;

#[gtest]
fn constrained_ranges_preserve_packed_free_bit_order() -> googletest::Result<()> {
	let policy = NumericalPolicy::default();
	// Fix bit 1 to one and bit 3 to zero; free bits are 0, 2, 4.
	let space = LogicalSpace::<Left>::constrained_range(32, 0b01010, 0b00010, 1..6, policy)?;
	let expected = [3, 6, 7, 18, 19];
	expect_eq!(space.logical_dimension(), 5);
	for (logical, physical) in expected.into_iter().enumerate() {
		expect_eq!(space.coordinate_at(logical), Some(physical));
	}
	expect_eq!(space.coordinate_at(5), None);
	let basis = space.isometry_snapshot(policy)?;
	for row in 0..32 {
		for col in 0..5 {
			expect_eq!(basis[(row, col)].re, f64::from(row == expected[col]));
		}
	}
	Ok(())
}

#[gtest]
fn compact_ranges_partition_into_disjoint_exact_cubes() -> googletest::Result<()> {
	for mask in [0usize, 0b00010, 0b01010, 31] {
		let free_dimension = 1usize
			<< (5u32
				.checked_sub(mask.count_ones())
				.ok_or(quest_qsvt::Error::Budget("free width"))?);
		for start in 0..free_dimension {
			for end in start
				.checked_add(1)
				.ok_or(quest_qsvt::Error::Budget("range start"))?
				..=free_dimension
			{
				let space = LogicalSpace::<Right>::constrained_range(
					32,
					mask,
					mask,
					start..end,
					NumericalPolicy::default(),
				)?;
				let cubes = space.coordinate_cubes()?.unwrap();
				expect_true!(cubes.len() <= 10);
				for physical in 0..32 {
					let count = cubes.iter().filter(|&&(m, v)| physical & m == v).count();
					let packed = (0..5)
						.filter(|bit| mask & (1 << bit) == 0)
						.enumerate()
						.fold(0, |value, (local, bit)| {
							value | (((physical >> bit) & 1) << local)
						});
					let selected = physical & mask == mask && packed >= start && packed < end;
					expect_eq!(count, usize::from(selected));
					expect_eq!(space.contains_coordinate(physical), Some(selected));
				}
			}
		}
	}
	Ok(())
}

#[gtest]
fn large_spaces_remain_compact_and_cold_snapshots_obey_budget() -> googletest::Result<()> {
	let policy = NumericalPolicy { max_bytes: 128 };
	let dimension = 1usize << 40;
	let space = LogicalSpace::<Left>::bit_constraints(dimension, 0b111, 0, policy)?;
	expect_eq!(space.logical_dimension(), 1usize << 37);
	expect_true!(space.storage_bytes()? <= 128);
	expect_eq!(space.coordinate_cubes()?.unwrap(), vec![(7, 0)]);
	expect_true!(space.isometry_snapshot(policy).is_err());
	expect_true!(space.projector_matrix(policy).is_err());
	let range = LogicalSpace::<Left>::logical_range(dimension, 3..dimension - 5, policy)?;
	expect_true!(range.storage_bytes()? <= 128);
	expect_true!(range.coordinate_cubes()?.unwrap().len() <= 80);
	Ok(())
}

#[gtest]
fn malformed_constraints_and_ranges_are_rejected() {
	let policy = NumericalPolicy::default();
	expect_true!(LogicalSpace::<Left>::bit_constraints(8, 8, 0, policy).is_err());
	expect_true!(LogicalSpace::<Left>::bit_constraints(8, 2, 1, policy).is_err());
	expect_true!(LogicalSpace::<Left>::logical_range(8, 3..3, policy).is_err());
	expect_true!(LogicalSpace::<Left>::logical_range(8, 3..9, policy).is_err());
	expect_true!(LogicalSpace::<Left>::constrained_range(8, 2, 0, 0..5, policy).is_err());
}

fn identity_encoding(
	width: usize,
	left: LogicalSpace<Left>,
	right: LogicalSpace<Right>,
) -> quest_qsvt::Result<quest_qsvt::ProjectedEncoding> {
	use quest_compile::{OracleFragment, QuantumRegionBuilder};
	use quest_qsvt::{EncodingBuilder, ExplicitUnitaryPremise};
	let oracle = OracleFragment::builder(QuantumRegionBuilder::new(width, 0)?.finish()?.bind(&[])?)
		.matrix_tolerance(1e-12)?
		.build()?;
	EncodingBuilder::new()
		.oracle(oracle)
		.left(left)
		.right(right)
		.normalization(1.0)?
		.unitarity_assumption(ExplicitUnitaryPremise::new("empty circuit is identity")?)
		.build()
}

#[gtest]
fn coherent_compact_phases_match_ordered_coordinates_including_failure_sectors()
-> googletest::Result<()> {
	use quest_qsp::{PhaseSequence, WxSymmetric};
	use quest_qsvt::{OperandLayout, TransformBuilder, materialize_program};
	let policy = NumericalPolicy::default();
	let compact = identity_encoding(
		3,
		LogicalSpace::constrained_range(8, 2, 2, 1..4, policy)?,
		LogicalSpace::logical_range(8, 1..6, policy)?,
	)?;
	let explicit = identity_encoding(
		3,
		LogicalSpace::coordinates(8, &[3, 6, 7], policy)?,
		LogicalSpace::coordinates(8, &[1, 2, 3, 4, 5], policy)?,
	)?;
	let build = |encoding| {
		TransformBuilder::new()
			.encoding(encoding)
			.operands(OperandLayout::new(4, vec![3, 0, 2], 1, None).unwrap())
			.standard(
				PhaseSequence::<WxSymmetric>::builder(vec![0.17, -0.23, 0.17])
					.build()
					.unwrap(),
			)
			.build()
	};
	let compact = build(compact)?;
	let explicit = build(explicit)?;
	let a = materialize_program(compact.main(), policy)?;
	let b = materialize_program(explicit.main(), policy)?;
	for row in 0..16 {
		for col in 0..16 {
			expect_true!(a[(row, col)].sub(b[(row, col)]).norm() < 1e-12);
		}
	}
	let a = compact.materialize_block()?;
	let b = explicit.materialize_block()?;
	expect_eq!((a.nrows(), a.ncols()), (5, 5));
	for row in 0..5 {
		for col in 0..5 {
			expect_true!(a[(row, col)].sub(b[(row, col)]).norm() < 1e-12);
		}
	}
	Ok(())
}

#[gtest]
fn coherent_reflections_admit_large_logical_spaces_without_dense_work() -> googletest::Result<()> {
	use quest_qsp::{PhaseSequence, WxSymmetric};
	use quest_qsvt::TransformBuilder;
	let policy = NumericalPolicy::default();
	for range in [false, true] {
		let dimension = 1usize << 40;
		let encoding = identity_encoding(
			40,
			LogicalSpace::bit_constraints(dimension, 7, 0, policy)?,
			if range {
				LogicalSpace::logical_range(dimension, 3..dimension - 5, policy)?
			} else {
				LogicalSpace::bit_constraints(dimension, 7, 0, policy)?
			},
		)?;
		let transform = TransformBuilder::new()
			.encoding(encoding)
			.standard(PhaseSequence::<WxSymmetric>::builder(vec![0.21]).build()?)
			.build()?;
		expect_true!(transform.main().instructions().len() < 200);
		expect_true!(transform.materialize_block().is_err());
	}
	Ok(())
}

#[gtest]
fn compact_joint_spaces_keep_left_then_right_basis_order() -> googletest::Result<()> {
	use quest_qsvt::ProjectionSpace;
	let policy = NumericalPolicy::default();
	let joint = ProjectionSpace::Joint {
		left: LogicalSpace::logical_range(8, 3..5, policy)?,
		right: LogicalSpace::bit_constraints(8, 4, 0, policy)?,
	};
	let expected = [3, 4, 8, 9, 10, 11];
	for (logical, physical) in expected.into_iter().enumerate() {
		expect_eq!(joint.coordinate_at(logical), Some(physical));
	}
	let cubes = joint.coordinate_cubes()?.unwrap();
	let basis = joint.isometry_snapshot(policy)?;
	for physical in 0..16 {
		expect_eq!(
			cubes
				.iter()
				.filter(|&&(mask, value)| physical & mask == value)
				.count(),
			usize::from(expected.contains(&physical))
		);
		for logical in 0..6 {
			expect_eq!(
				basis[(physical, logical)].re,
				f64::from(physical == expected[logical])
			);
		}
	}
	Ok(())
}
