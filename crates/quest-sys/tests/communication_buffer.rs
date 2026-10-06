#![cfg(all(feature = "mpi", quest_native_mpi))]
#![allow(
	clippy::panic_in_result_fn,
	reason = "Subprocess assertions verify checked native mutation boundaries"
)]

use quest_sys::{QuestComplex, mpi::MpiRuntime};

const fn value(re: f64, im: f64) -> QuestComplex {
	QuestComplex { re, im }
}

fn read(register: &quest_sys::Qureg, length: usize) -> quest_sys::QuestResult<Vec<QuestComplex>> {
	let mut output = vec![value(0.0, 0.0); length];
	quest_sys::read_local_qureg_amps(register, 0, &mut output)?;
	Ok(output)
}

fn launch() -> googletest::Result<()> {
	for ranks in [1, 2] {
		let output =
			quest_test_support::mpi::MpiTest::new(ranks, std::time::Duration::from_secs(60))?
				.args([
					"--exact",
					"communication_buffer_staging_is_isolated_and_checked",
					"--nocapture",
					"--test-threads=1",
				])
				.env("QUEST_SYS_COMMUNICATION_BUFFER_CHILD", "1")
				.output()?;
		assert!(
			output.status.success(),
			"MPI {ranks}: {}\n{}\n{}",
			output.status,
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
	}
	Ok(())
}

#[test]
fn communication_buffer_staging_is_isolated_and_checked() -> googletest::Result<()> {
	if std::env::var_os("QUEST_SYS_COMMUNICATION_BUFFER_CHILD").is_none() {
		return launch();
	}
	let runtime = MpiRuntime::initialize()?;
	let world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let parts = world.size()?;
	let rank = world.rank()?;
	let _environment = world.quest_environment()?.build()?;
	let mut register = quest_sys::create_custom_qureg(2, 0, 1, 0, 0)?;
	let deployment = quest_sys::get_qureg_deployment(&register)?;
	let length = usize::try_from(deployment.num_amps_per_node)?;
	let original = if parts == 1 {
		vec![
			value(1.0, -1.0),
			value(2.0, -2.0),
			value(3.0, -3.0),
			value(4.0, -4.0),
		]
	} else if rank == 0 {
		vec![value(1.0, -1.0), value(2.0, -2.0)]
	} else {
		vec![value(3.0, -3.0), value(4.0, -4.0)]
	};
	quest_sys::write_local_qureg_amps(register.pin_mut(), 0, &original)?;
	if parts == 1 {
		assert!(quest_sys::validate_cpu_communication_buffer(&register).is_err());
		assert!(quest_sys::stage_cpu_communication_buffer(register.pin_mut()).is_err());
		assert!(
			quest_sys::set_cpu_communication_buffer_indexed(register.pin_mut(), &[], &[]).is_err()
		);
		assert!(quest_sys::commit_cpu_communication_buffer(register.pin_mut()).is_err());
		assert_eq!(read(&register, length)?, original);
	} else {
		quest_sys::validate_cpu_communication_buffer(&register)?;
		quest_sys::stage_cpu_communication_buffer(register.pin_mut())?;
		quest_sys::set_cpu_communication_buffer_indexed(
			register.pin_mut(),
			&[0, 0, 1],
			&[value(5.0, 1.0), value(9.0, 2.0), value(7.0, 3.0)],
		)?;
		assert_eq!(
			read(&register, length)?,
			original,
			"staging must leave amplitudes intact"
		);
		quest_sys::commit_cpu_communication_buffer(register.pin_mut())?;
		assert_eq!(
			read(&register, length)?,
			vec![value(9.0, 2.0), value(7.0, 3.0)]
		);
		quest_sys::write_local_qureg_amps(register.pin_mut(), 0, &original)?;
		let past_end = i64::try_from(length)?;
		for (indices, values) in [
			(vec![0, past_end], vec![value(5.0, 0.0), value(6.0, 0.0)]),
			(vec![0, -1], vec![value(5.0, 0.0), value(6.0, 0.0)]),
			(vec![0, 1], vec![value(5.0, 0.0)]),
			(vec![0, 1], vec![value(5.0, 0.0), value(f64::NAN, 0.0)]),
			(vec![0, 1], vec![value(5.0, 0.0), value(0.0, f64::INFINITY)]),
		] {
			quest_sys::stage_cpu_communication_buffer(register.pin_mut())?;
			assert!(
				quest_sys::set_cpu_communication_buffer_indexed(
					register.pin_mut(),
					&indices,
					&values
				)
				.is_err()
			);
			quest_sys::commit_cpu_communication_buffer(register.pin_mut())?;
			assert_eq!(
				read(&register, length)?,
				original,
				"reject the entire write before mutation"
			);
		}
		quest_sys::stage_cpu_communication_buffer(register.pin_mut())?;
		quest_sys::set_cpu_communication_buffer_indexed(
			register.pin_mut(),
			&[0],
			&[value(8.0, 8.0)],
		)?;
		quest_sys::stage_cpu_communication_buffer(register.pin_mut())?;
		quest_sys::set_cpu_communication_buffer_indexed(register.pin_mut(), &[], &[])?;
		quest_sys::commit_cpu_communication_buffer(register.pin_mut())?;
		assert_eq!(
			read(&register, length)?,
			original,
			"restaging discards the prior scratch contents"
		);
	}
	assert_eq!(quest_sys::get_qureg_deployment(&register)?, deployment);
	let mut density = quest_sys::create_custom_qureg(2, 1, 1, 0, 0)?;
	assert!(quest_sys::validate_cpu_communication_buffer(&density).is_err());
	assert!(quest_sys::stage_cpu_communication_buffer(density.pin_mut()).is_err());
	assert!(quest_sys::set_cpu_communication_buffer_indexed(density.pin_mut(), &[], &[]).is_err());
	assert!(quest_sys::commit_cpu_communication_buffer(density.pin_mut()).is_err());
	Ok(())
}
