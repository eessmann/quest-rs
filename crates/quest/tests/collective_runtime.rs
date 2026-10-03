#![cfg(all(feature = "mpi", quest_native_mpi))]
use googletest::prelude::*;
use quest::collective::{CollectiveEnvironment, MpiRuntime};
use quest::{
	Angle, Complex64, Control, ControlState, Gate, MatrixPolicy, MemoryBudget, NumericalOperator,
	OracleFragment, Outcome, QuantumRegionBuilder, QubitCount,
};
fn ranks(
	name: &str,
	count: &str,
	body: impl FnOnce() -> googletest::Result<()>,
) -> googletest::Result<()> {
	if std::env::var("QUEST_COLLECTIVE_TEST").as_deref() == Ok(name) {
		return body();
	}
	let output = std::process::Command::new("timeout")
		.args(["60s", "mpiexec", "-n", count])
		.arg(std::env::current_exe()?)
		.args(["--exact", name, "--nocapture", "--test-threads=1"])
		.env("QUEST_COLLECTIVE_TEST", name)
		.output()?;
	verify_that!(output.status.success(), eq(true)).with_failure_message(|| {
		format!(
			"{}\n{}\n{}",
			output.status,
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		)
	})?;
	Ok(())
}

#[gtest]
fn collective_rejects_different_classical_results_and_step_counts() -> googletest::Result<()> {
	ranks(
		"collective_rejects_different_classical_results_and_step_counts",
		"2",
		|| {
			let runtime = MpiRuntime::initialize()?;
			let mut comm = runtime.world()?;
			let different = comm.rank()? == 1;
			{
				let env = CollectiveEnvironment::builder(&comm)?.build()?;
				for source in [
					if different {
						"qubit[2] q; h q[0]; output int result=2;"
					} else {
						"qubit[2] q; h q[0]; output int result=1;"
					},
					if different {
						"qubit[2] q; h q[0]; int ignored=2; output int result=1;"
					} else {
						"qubit[2] q; h q[0]; output int result=1;"
					},
				] {
					let program = quest::Program::parse(source, "collective result")?
						.verify()?
						.lower()?
						.plan()?;
					verify_that!(env.prepare(program).is_err(), eq(true))?;
					verify_that!(env.view().allocated_bytes(), eq(0))?;
				}
				let too_small = quest::Program::parse("qubit q; h q;", "mixed amplitudes")?
					.verify()?
					.lower()?
					.plan()?;
				verify_that!(env.prepare(too_small).is_err(), eq(true))?;
				let program = quest::Program::parse(
					"qubit[2] q; h q[0]; output int result=7;",
					"common result",
				)?
				.verify()?
				.lower()?
				.plan()?;
				let mut prepared = env.prepare(program)?;
				let mut register = env.state_vector(QubitCount::new(2)?)?;
				let result = prepared.run(&mut register)?;
				verify_that!(
					result.outputs["result"].as_scalar().or_fail()?.to_i128()?,
					eq(7)
				)?;
			}
			verify_that!(comm.all_agree(true)?, eq(true))?;
			Ok(())
		},
	)
}
fn bell() -> quest::Result<quest::RegionPlan> {
	let mut p = QuantumRegionBuilder::new(2, 0)?;
	let a = p.qubit(0)?;
	let b = p.qubit(1)?;
	p.gate(Gate::H, &[a], &[])?;
	p.gate(Gate::X, &[b], &[Control::new(a, ControlState::One)])?;
	Ok(p.finish()?.bind(&[])?.plan()?)
}
fn oracle(phase: f64) -> quest::Result<quest::RegionPlan> {
	let mut body = QuantumRegionBuilder::new(1, 0)?;
	let x = faer::Mat::from_fn(2, 2, |r, c| Complex64::new(f64::from(r != c), 0.));
	body.numerical(
		NumericalOperator::from_view(&x, MatrixPolicy::default())?,
		&[body.qubit(0)?],
		&[],
	)?;
	body.global_phase(Angle::radians(phase)?, &[])?;
	let fragment = OracleFragment::builder(body.finish()?.bind(&[])?)
		.matrix_tolerance(1e-12)?
		.build()?;
	let mut outer = QuantumRegionBuilder::new(2, 0)?;
	outer.oracle(&fragment, &[outer.qubit(1)?], &[])?;
	Ok(outer.finish()?.bind(&[])?.plan()?)
}
#[gtest]
fn collective_prepared_bell_oracle_and_projection_preserve_mpi() -> googletest::Result<()> {
	ranks(
		"collective_prepared_bell_oracle_and_projection_preserve_mpi",
		"2",
		|| {
			let runtime = MpiRuntime::initialize()?;
			let mut comm = runtime.world()?;
			{
				let env = CollectiveEnvironment::builder(&comm)?.build()?;
				let mut register = env.state_vector(QubitCount::new(2)?)?;
				let deployment = register.deployment();
				expect_true!(deployment.is_distributed());
				expect_false!(deployment.is_density_matrix());
				expect_eq!(deployment.rank(), usize::try_from(env.rank()?)?);
				expect_eq!(deployment.nodes(), usize::try_from(env.size()?)?);
				expect_eq!(deployment.local_amplitudes(), 2);
				expect_eq!(deployment.compiler_snapshot()?.nodes(), 2);
				let mut prepared = env.prepare(
					quest::Program::from_bound_region((bell()?).into_region())?
						.verify()?
						.lower()?
						.plan()?,
				)?;
				register.init_zero()?;
				prepared.run(&mut register)?;
				verify_that!(
					register.probability(0, Outcome::One)?.get(),
					near(0.5, 1e-13)
				)?;
				verify_that!(register.total_probability()?, near(1., 1e-13))?;
				register.project(0, Outcome::One)?;
				verify_that!(register.total_probability()?, near(0.5, 1e-13))?;
				let values = [
					Complex64::new(1., 0.),
					Complex64::new(0., 0.),
					Complex64::new(0., 0.),
					Complex64::new(0., 0.),
				];
				register.init_pure_from_root(0, (env.rank()? == 0).then_some(values.as_slice()))?;
				let mut call = env.prepare(
					quest::Program::from_bound_region((oracle(0.37)?).into_region())?
						.verify()?
						.lower()?
						.plan()?,
				)?;
				verify_that!(call.prepared_oracle_bodies(), eq(1))?;
				call.run(&mut register)?;
				verify_that!(
					register.probability(1, Outcome::One)?.get(),
					near(1., 1e-13)
				)?;
			}
			verify_that!(comm.all_agree(true)?, eq(true))?;
			verify_that!(
				CollectiveEnvironment::builder(&comm)?.build().is_err(),
				eq(true)
			)?;
			verify_that!(runtime.is_active()?, eq(true))?;
			Ok(())
		},
	)
}
#[gtest]
fn collective_mismatched_payload_and_preflight_recover_together() -> googletest::Result<()> {
	ranks(
		"collective_mismatched_payload_and_preflight_recover_together",
		"2",
		|| {
			let runtime = MpiRuntime::initialize()?;
			let mut comm = runtime.world()?;
			let rank = comm.rank()?;
			{
				let env = CollectiveEnvironment::builder(&comm)?.build()?;
				verify_that!(
					env.prepare(
						quest::Program::from_bound_region(
							(oracle(if rank == 0 { 0.37 } else { 0.38 })?).into_region()
						)?
						.verify()?
						.lower()?
						.plan()?
					)
					.is_err(),
					eq(true)
				)?;
				let mut register = env.state_vector(QubitCount::new(2)?)?;
				verify_that!(
					register
						.probability(if rank == 0 { 0 } else { 9 }, Outcome::One)
						.is_err(),
					eq(true)
				)?;
				verify_that!(register.init_pure_from_root(0, None).is_err(), eq(true))?;
				let mut plan = QuantumRegionBuilder::new(2, 1)?;
				plan.measure(plan.qubit(0)?, plan.bit(0)?)?;
				verify_that!(
					env.prepare(
						quest::Program::from_region(plan.finish()?, &[])?
							.verify()?
							.lower()?
							.plan()?
					)
					.is_err(),
					eq(true)
				)?;
				let mut prepared = env.prepare(
					quest::Program::from_bound_region((bell()?).into_region())?
						.verify()?
						.lower()?
						.plan()?,
				)?;
				register.init_zero()?;
				prepared.run(&mut register)?;
				verify_that!(register.total_probability()?, near(1., 1e-13))?;
			}
			verify_that!(comm.all_agree(true)?, eq(true))?;
			Ok(())
		},
	)
}
#[gtest]
fn collective_budget_failure_on_one_rank_precedes_native_allocation() -> googletest::Result<()> {
	ranks(
		"collective_budget_failure_on_one_rank_precedes_native_allocation",
		"2",
		|| {
			let runtime = MpiRuntime::initialize()?;
			let mut comm = runtime.world()?;
			{
				let budget = if comm.rank()? == 0 { 128 } else { 1_000_000 };
				let env = CollectiveEnvironment::builder(&comm)?
					.memory_budget(MemoryBudget::new(budget))
					.build()?;
				verify_that!(env.state_vector(QubitCount::new(4)?).is_err(), eq(true))?;
				verify_that!(env.view().allocated_bytes(), eq(0))?;
				verify_that!(
					env.prepare(
						quest::Program::from_bound_region((bell()?).into_region())?
							.verify()?
							.lower()?
							.plan()?
					)
					.is_err(),
					eq(true)
				)?;
				verify_that!(env.view().allocated_bytes(), eq(0))?;
			}
			verify_that!(comm.all_agree(true)?, eq(true))?;
			Ok(())
		},
	)
}
#[gtest]
fn collective_subgroups_execute_independent_schedules() -> googletest::Result<()> {
	ranks(
		"collective_subgroups_execute_independent_schedules",
		"4",
		|| {
			let runtime = MpiRuntime::initialize()?;
			let mut world = runtime.world()?;
			let group = world.rank()? / 2;
			let mut comm = world.split_power_of_two(2)?;
			{
				let env = CollectiveEnvironment::builder(&comm)?.build()?;
				let mut register = env.state_vector(QubitCount::new(2)?)?;
				register.init_zero()?;
				if group == 0 {
					let mut prepared = env.prepare(
						quest::Program::from_bound_region((bell()?).into_region())?
							.verify()?
							.lower()?
							.plan()?,
					)?;
					prepared.run(&mut register)?;
					verify_that!(
						register.probability(0, Outcome::One)?.get(),
						near(0.5, 1e-13)
					)?;
				} else {
					let mut prepared = env.prepare(
						quest::Program::from_bound_region((oracle(0.2)?).into_region())?
							.verify()?
							.lower()?
							.plan()?,
					)?;
					prepared.run(&mut register)?;
					prepared.run(&mut register)?;
					verify_that!(
						register.probability(1, Outcome::One)?.get(),
						near(0., 1e-13)
					)?;
				}
			}
			verify_that!(comm.all_agree(true)?, eq(true))?;
			verify_that!(world.all_agree(true)?, eq(true))?;
			Ok(())
		},
	)
}

fn differing_semantics(case: usize, different: bool) -> quest::Result<quest::RegionPlan> {
	let mut p = QuantumRegionBuilder::new(3, 0)?;
	let a = p.qubit(0)?;
	let b = p.qubit(1)?;
	let c = p.qubit(2)?;
	match case {
		0 => {
			let sign = if different { -1. } else { 1. };
			let matrix = faer::Mat::from_fn(2, 2, |r, col| {
				Complex64::new(0., if r == col { sign } else { 0. })
			});
			p.numerical(
				NumericalOperator::from_view(&matrix, MatrixPolicy::default())?,
				&[a],
				&[],
			)?;
		}
		1 => {
			p.gate(
				Gate::X,
				&[a],
				&[Control::new(
					b,
					if different {
						ControlState::Zero
					} else {
						ControlState::One
					},
				)],
			)?;
		}
		2 => {
			let targets = if different { [a, b] } else { [b, a] };
			p.gate(Gate::Swap, &targets, &[])?;
		}
		3 => {
			p.global_phase(
				Angle::radians(if different { 0.4 } else { 0.3 })?,
				&[Control::new(c, ControlState::Zero)],
			)?;
		}
		4 => {
			let mut body = QuantumRegionBuilder::new(1, 0)?;
			body.gate(Gate::S, &[body.qubit(0)?], &[])?;
			let inner = OracleFragment::builder(body.finish()?.bind(&[])?)
				.matrix_tolerance(1e-12)?
				.build()?;
			let mut outer = QuantumRegionBuilder::new(1, 0)?;
			outer.oracle(&inner, &[outer.qubit(0)?], &[])?;
			let outer = OracleFragment::builder(outer.finish()?.bind(&[])?)
				.matrix_tolerance(1e-12)?
				.build()?;
			p.oracle(&if different { outer.adjoint() } else { outer }, &[a], &[])?;
		}
		_ => {
			p.gate(
				Gate::Rz(Angle::radians(if different { 0.1 } else { 0.2 })?),
				&[a],
				&[],
			)?;
		}
	}
	Ok(p.finish()?.bind(&[])?.plan()?)
}
#[gtest]
fn collective_compares_full_semantics_and_prepared_identity() -> googletest::Result<()> {
	ranks(
		"collective_compares_full_semantics_and_prepared_identity",
		"2",
		|| {
			let runtime = MpiRuntime::initialize()?;
			let mut comm = runtime.world()?;
			let different = comm.rank()? == 1;
			{
				let env = CollectiveEnvironment::builder(&comm)?.build()?;
				for case in 0..6 {
					verify_that!(
						env.prepare(
							quest::Program::from_bound_region(
								(differing_semantics(case, different)?).into_region()
							)?
							.verify()?
							.lower()?
							.plan()?
						)
						.is_err(),
						eq(true)
					)?;
					verify_that!(env.view().allocated_bytes(), eq(0))?;
				}
				let mut register = env.state_vector(QubitCount::new(2)?)?;
				let mut first = env.prepare(
					quest::Program::from_bound_region((bell()?).into_region())?
						.verify()?
						.lower()?
						.plan()?,
				)?;
				let mut second = env.prepare(
					quest::Program::from_bound_region((oracle(0.4)?).into_region())?
						.verify()?
						.lower()?
						.plan()?,
				)?;
				register.init_zero()?;
				let rejected = if different {
					first.run(&mut register)
				} else {
					second.run(&mut register)
				};
				verify_that!(rejected.is_err(), eq(true))?;
				first.run(&mut register)?;
				verify_that!(register.total_probability()?, near(1., 1e-13))?;
			}
			verify_that!(comm.all_agree(true)?, eq(true))?;
			Ok(())
		},
	)
}

#[gtest]
fn collective_rejects_different_cache_sharing_before_materialization() -> googletest::Result<()> {
	ranks(
		"collective_rejects_different_cache_sharing_before_materialization",
		"2",
		|| {
			let runtime = MpiRuntime::initialize()?;
			let mut comm = runtime.world()?;
			let shared = comm.rank()? == 0;
			{
				let env = CollectiveEnvironment::builder(&comm)?.build()?;
				let values = faer::Mat::from_fn(2, 2, |r, c| Complex64::new(f64::from(r != c), 0.));
				let first = NumericalOperator::from_view(&values, MatrixPolicy::default())?;
				let second = if shared {
					first.clone()
				} else {
					NumericalOperator::from_view(&values, MatrixPolicy::default())?
				};
				let mut p = QuantumRegionBuilder::new(2, 0)?;
				p.numerical(first, &[p.qubit(0)?], &[])?;
				p.numerical(second, &[p.qubit(1)?], &[])?;
				verify_that!(
					env.prepare(
						quest::Program::from_region(p.finish()?, &[])?
							.verify()?
							.lower()?
							.plan()?
					)
					.is_err(),
					eq(true)
				)?;
				verify_that!(env.view().allocated_bytes(), eq(0))?;
				let make_body = || -> quest::Result<OracleFragment> {
					let mut body = QuantumRegionBuilder::new(1, 0)?;
					body.gate(Gate::H, &[body.qubit(0)?], &[])?;
					Ok(OracleFragment::builder(body.finish()?.bind(&[])?)
						.matrix_tolerance(1e-12)?
						.build()?)
				};
				let first = make_body()?;
				let second = if shared { first.clone() } else { make_body()? };
				let mut p = QuantumRegionBuilder::new(2, 0)?;
				p.oracle(&first, &[p.qubit(0)?], &[])?;
				p.oracle(&second, &[p.qubit(1)?], &[])?;
				verify_that!(
					env.prepare(
						quest::Program::from_region(p.finish()?, &[])?
							.verify()?
							.lower()?
							.plan()?
					)
					.is_err(),
					eq(true)
				)?;
				let mut register = env.state_vector(QubitCount::new(2)?)?;
				register.init_zero()?;
				let mut prepared = env.prepare(
					quest::Program::from_bound_region((bell()?).into_region())?
						.verify()?
						.lower()?
						.plan()?,
				)?;
				prepared.run(&mut register)?;
				verify_that!(register.total_probability()?, near(1., 1e-13))?;
			}
			verify_that!(comm.all_agree(true)?, eq(true))?;
			Ok(())
		},
	)
}
