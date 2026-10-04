use googletest::{expect_that, expect_true, gtest, matchers::eq};
use quest_qsvt::{NumericalPolicy, ShiftRegister, TensorShiftEncoding, materialize_oracle};
#[gtest]
fn tensor_shift_gate_arithmetic_matches_modular_indices() -> googletest::Result<()> {
	let plan = TensorShiftEncoding::new(
		5,
		vec![ShiftRegister::new(0, 3, 5)?, ShiftRegister::new(3, 2, 3)?],
		NumericalPolicy::default(),
	)?;
	let unitary = materialize_oracle(
		&plan.to_oracle(NumericalPolicy::default())?,
		NumericalPolicy::default(),
	)?;
	for col in 0_usize..32 {
		let destination = ((col & 7)
			.checked_add(5_usize)
			.ok_or(quest_qsvt::Error::Budget("test index"))?
			% 8)
			| (((col >> 3)
				.checked_add(3)
				.ok_or(quest_qsvt::Error::Budget("test index"))?
				% 4)
				<< 3);
		expect_that!(plan.map_index(col, false)?, eq(destination));
		expect_that!(plan.map_index(destination, true)?, eq(col));
		for row in 0_usize..32 {
			expect_true!(
				(std::ops::Sub::sub(unitary[(row, col)].re, f64::from(row == destination))).abs()
					< 1e-12
			);
			expect_that!(unitary[(row, col)].im, eq(0.0));
		}
	}
	Ok(())
}
#[gtest]
fn structured_shift_width_is_compact_and_invalid_ranges_fail() -> googletest::Result<()> {
	let plan = TensorShiftEncoding::new(
		40,
		vec![ShiftRegister::new(0, 40, 123)?],
		NumericalPolicy { max_bytes: 1024 },
	)?;
	let mut count = 0_usize;
	plan.visit_gates(false, |_| {
		count = count
			.checked_add(1)
			.ok_or(quest_qsvt::Error::Budget("test count"))?;
		Ok(())
	})?;
	expect_true!(count < 1600);
	expect_true!(
		TensorShiftEncoding::new(
			5,
			vec![ShiftRegister::new(0, 3, 1)?, ShiftRegister::new(2, 2, 1)?],
			NumericalPolicy::default()
		)
		.is_err()
	);
	expect_true!(ShiftRegister::new(0, 0, 0).is_err());
	expect_true!(ShiftRegister::new(0, 3, 8).is_err());
	Ok(())
}

#[gtest]
fn mapped_shift_replay_respects_controls_and_exact_inverse() -> googletest::Result<()> {
	let plan = TensorShiftEncoding::new(
		3,
		vec![ShiftRegister::new(0, 3, 5)?],
		NumericalPolicy::default(),
	)?;
	let targets = [2, 0, 3];
	for initial in 0_usize..16 {
		let mut result = initial;
		plan.visit_mapped_gates(&targets, 2, 2, false, |gate| {
			if result & gate.control_mask == gate.control_value {
				result ^= 1usize
					.checked_shl(
						u32::try_from(
							gate.target
								.ok_or(quest_qsvt::Error::Encoding("shift target"))?,
						)
						.map_err(|_| quest_qsvt::Error::Budget("shift target"))?,
					)
					.ok_or(quest_qsvt::Error::Budget("shift target"))?;
			}
			Ok(())
		})?;
		if initial & 2 == 0 {
			expect_that!(result, eq(initial));
		} else {
			let local = ((initial >> 2) & 1) | ((initial & 1) << 1) | ((initial >> 3) << 2);
			let mapped = plan.map_index(local, false)?;
			let expected = 2 | ((mapped & 1) << 2) | ((mapped >> 1) & 1) | ((mapped >> 2) << 3);
			expect_that!(result, eq(expected));
		}
		plan.visit_mapped_gates(&targets, 2, 2, true, |gate| {
			if result & gate.control_mask == gate.control_value {
				result ^= 1usize
					.checked_shl(
						u32::try_from(
							gate.target
								.ok_or(quest_qsvt::Error::Encoding("shift target"))?,
						)
						.map_err(|_| quest_qsvt::Error::Budget("shift target"))?,
					)
					.ok_or(quest_qsvt::Error::Budget("shift target"))?;
			}
			Ok(())
		})?;
		expect_that!(result, eq(initial));
	}
	Ok(())
}
