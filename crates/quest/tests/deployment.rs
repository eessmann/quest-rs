use googletest::prelude::*;
use quest::{Environment, QubitCount};

#[gtest]
fn allocated_register_reports_actual_local_deployment() -> googletest::Result<()> {
	let env = Environment::builder().build().or_fail()?;
	let sv = env.state_vector(QubitCount::new(2).or_fail()?).or_fail()?;
	let dm = env
		.density_matrix(QubitCount::new(2).or_fail()?)
		.or_fail()?;
	let sv_deployment = sv.deployment();
	let dm_deployment = dm.deployment();
	expect_false!(sv_deployment.is_density_matrix());
	expect_true!(dm_deployment.is_density_matrix());
	expect_false!(sv_deployment.is_distributed());
	expect_false!(dm_deployment.is_distributed());
	expect_eq!(sv_deployment.width(), 2);
	expect_eq!(dm_deployment.width(), 2);
	expect_eq!(sv_deployment.local_amplitudes(), 4);
	expect_eq!(dm_deployment.local_amplitudes(), 16);
	expect_eq!(sv_deployment.rank(), 0);
	expect_eq!(sv_deployment.nodes(), 1);
	expect_eq!(sv.try_clone().or_fail()?.deployment(), sv_deployment);
	let compiler = sv_deployment.compiler_snapshot().or_fail()?;
	expect_eq!(compiler.width(), sv_deployment.width());
	expect_eq!(compiler.local_amplitudes(), 4);
	expect_false!(compiler.distributed());
	Ok(())
}
