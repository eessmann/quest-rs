mod common;

use googletest::prelude::*;
use quest_sys::QuestComplex;

#[gtest]
fn local_partition_transfer_and_cube_union_are_checked() -> googletest::Result<()> {
	common::isolated(
		"local_partition_transfer_and_cube_union_are_checked",
		|| {
			quest_sys::init_custom_quest_env(false, false, false)?;
			let mut register = quest_sys::create_qureg(3)?;
			let values: Vec<_> = (0..8)
				.map(|index| QuestComplex {
					re: f64::from(index),
					im: -f64::from(index),
				})
				.collect();
			quest_sys::write_local_qureg_amps(register.pin_mut(), 0, &values)?;
			let mut copied = vec![QuestComplex { re: 0.0, im: 0.0 }; 3];
			quest_sys::read_local_qureg_amps(&register, 2, &mut copied)?;
			verify_that!(&copied, eq(&values.get(2..5).or_fail()?.to_vec()))?;
			verify_that!(
				quest_sys::write_local_qureg_amps(register.pin_mut(), 7, &values),
				err(anything())
			)?;
			verify_that!(
				quest_sys::read_local_qureg_amps(&register, -1, &mut copied),
				err(anything())
			)?;
			verify_that!(
				quest_sys::project_qureg_basis_cubes(register.pin_mut(), &[0, 0], &[0], &[0]),
				err(anything())
			)?;
			// Targets are intentionally not sorted. Keep packed values 0 and 3.
			quest_sys::project_qureg_basis_cubes(register.pin_mut(), &[2, 0], &[3, 3], &[0, 3])?;
			let mut result = vec![QuestComplex { re: 0.0, im: 0.0 }; 8];
			quest_sys::read_local_qureg_amps(&register, 0, &mut result)?;
			for (index, value) in result.iter().enumerate() {
				let expected = if ((index >> 2) & 1) == (index & 1) {
					*values.get(index).or_fail()?
				} else {
					QuestComplex { re: 0.0, im: 0.0 }
				};
				verify_that!(*value, eq(expected))?;
			}
			let mut density = quest_sys::create_density_qureg(1)?;
			verify_that!(
				quest_sys::write_local_qureg_amps(density.pin_mut(), 0, &values),
				err(anything())
			)?;
			drop(density);
			drop(register);
			quest_sys::finalize_quest_env()?;
			Ok(())
		},
	)
}
