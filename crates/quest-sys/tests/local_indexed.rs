mod common;
use googletest::prelude::*;
use quest_sys::QuestComplex;

#[gtest]
fn local_indexed_batches_validate_before_mutation() -> googletest::Result<()> {
	common::isolated("local_indexed_batches_validate_before_mutation", || {
		quest_sys::init_custom_quest_env(false, false, false)?;
		let mut register = quest_sys::create_qureg(3)?;
		let values = [
			QuestComplex { re: 3.0, im: -1.0 },
			QuestComplex { re: 2.0, im: 4.0 },
		];
		quest_sys::write_local_indexed_qureg_amps(register.pin_mut(), &[6, 1], &values)?;
		let mut actual = [QuestComplex { re: 0.0, im: 0.0 }; 2];
		quest_sys::read_local_indexed_qureg_amps(&register, &[1, 6], &mut actual)?;
		expect_eq!(actual, [values[1], values[0]]);
		expect_true!(
			quest_sys::write_local_indexed_qureg_amps(register.pin_mut(), &[1, 8], &values)
				.is_err()
		);
		expect_true!(
			quest_sys::write_local_indexed_qureg_amps(
				register.pin_mut(),
				&[1, 6],
				&[
					values[0],
					QuestComplex {
						re: f64::NAN,
						im: 0.0
					}
				]
			)
			.is_err()
		);
		quest_sys::read_local_indexed_qureg_amps(&register, &[1, 6], &mut actual)?;
		expect_eq!(actual, [values[1], values[0]]);
		expect_true!(
			quest_sys::read_local_indexed_qureg_amps(&register, &[-1, 6], &mut actual).is_err()
		);
		drop(register);
		quest_sys::finalize_quest_env()?;
		Ok(())
	})
}
