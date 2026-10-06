#![cfg(feature = "distributed")]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	clippy::too_many_lines,
	clippy::suboptimal_flops,
	clippy::items_after_statements,
	reason = "Bounded independent stored history and complete small native partition checks"
)]
use quest::collective::{CollectiveEnvironment, MpiRuntime};
use quest_cfd::{
	CfdError,
	classical_history::{ReferenceBudget, solve_reference},
	distributed_history::{
		DistributedHistoryLimits, DistributedHistoryOutcome, prepare_history_inverse,
	},
	history::{HistoryDynamics, HistorySystem},
	stream_history::{HistoryRowDynamics, HistoryStreamLimits, TemporalHistoryRecipe},
};
use quest_numerics::{Complex64, SparseLimits};
struct Dynamics;
impl HistoryRowDynamics for Dynamics {
	fn dimension(&self) -> usize {
		3
	}
	fn max_row_entries(&self) -> usize {
		2
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(0)
	}
	fn row_query_bytes(&self) -> usize {
		0
	}
	fn row_query_work(&self) -> usize {
		8
	}
	fn visit_row(
		&self,
		t: f64,
		r: usize,
		v: &mut dyn FnMut(usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		v(r, Complex64::new(-0.2 - t, 0.3))?;
		if r == 0 {
			v(1, Complex64::new(0.1, 0.2))?;
		}
		Ok(())
	}
	fn source_entry(&self, t: f64, r: usize) -> Result<Complex64, CfdError> {
		Ok(Complex64::new(
			t,
			-t * f64::from(u32::try_from(r).map_err(|_| CfdError::InvalidInput("test index"))?),
		))
	}
}
impl HistoryDynamics for Dynamics {
	fn dimension(&self) -> usize {
		3
	}
	fn max_generator_entries(&self) -> usize {
		6
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(0)
	}
	fn visit_generator(
		&self,
		t: f64,
		v: &mut dyn FnMut(usize, usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		for r in 0..3 {
			self.visit_row(t, r, &mut |c, x| v(r, c, x))?;
		}
		Ok(())
	}
	fn source(&self, t: f64, out: &mut [Complex64]) -> Result<(), CfdError> {
		for (r, x) in out.iter_mut().enumerate() {
			*x = self.source_entry(t, r)?;
		}
		Ok(())
	}
}
#[test]
fn streamed_inverse_preserves_physical_scaling_and_whole_native_register()
-> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_CFD_MPI_CHILD").is_none() {
		for (ranks, split) in [(1, 0), (2, 0), (4, 0), (8, 0), (4, 1)] {
			let status = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(180),
			)?
			.args([
				"--exact",
				"streamed_inverse_preserves_physical_scaling_and_whole_native_register",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_CFD_MPI_CHILD", "1")
			.env("QUEST_CFD_MPI_SPLIT", split.to_string())
			.status()?;
			assert!(status.success(), "MPI {ranks} split={split} failed");
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let split = std::env::var("QUEST_CFD_MPI_SPLIT").as_deref() == Ok("1");
	let comm = if split {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let group = if split { world.rank()? / 2 } else { 0 };
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(quest::MemoryBudget::new(16 * 1024 * 1024))
		.build()?;
	let recipe = TemporalHistoryRecipe::new(&Dynamics, 0.1, 1, 1, HistoryStreamLimits::default())?;
	let initial = [
		Complex64::new(0.2 + f64::from(group) * 0.1, -0.1),
		Complex64::new(0.3, 0.4),
		Complex64::new(-0.1, 0.2),
	];
	let stored =
		HistorySystem::assemble_dynamics(&Dynamics, &initial, 0.1, 1, 1, SparseLimits::default())?;
	let reference = solve_reference(&stored, ReferenceBudget::default())?;
	let spectrum = stored.spectral_bounds(SparseLimits::default())?;
	let mut generated = 0;
	let outcome = prepare_history_inverse(
		&env,
		&recipe,
		|i| {
			generated += 1;
			Ok(initial[i])
		},
		&spectrum,
		DistributedHistoryLimits {
			approximation_tolerance: 1e-3,
			..Default::default()
		},
	)?;
	let DistributedHistoryOutcome::Prepared(mut prepared) = outcome else {
		return Err("unexpected zero".into());
	};
	assert!(prepared.report().projector_response_bound.is_some());
	let parts = usize::try_from(comm.size()?)?;
	let rank = usize::try_from(comm.rank()?)?;
	assert_eq!(
		generated,
		3usize.saturating_sub(rank * (8 / parts)).min(8 / parts)
	); // local callback is only queried for the initial temporal block.
	let mut state = env.state_vector_local(prepared.qubit_count())?;
	prepared.initialize_rhs(&mut state)?;
	prepared.apply_inverse(&mut state, false)?;
	let reduction =
		prepared.reduce_observable(&state, |i| Ok((i / 3 == 1, Complex64::new(1.0, 0.0))))?;
	let expected: Complex64 = reference.solution[3..6].iter().copied().sum();
	assert!((reduction.physical_linear_observable - expected).norm() < 0.02);
	let expected_norm = reference.solution[3..6]
		.iter()
		.map(Complex64::norm_sqr)
		.sum::<f64>();
	assert!((reduction.physical_selected_squared_norm - expected_norm).abs() < 0.02);
	assert!(
		reduction.inverse_success_probability > 0.0 && reduction.inverse_success_probability <= 1.0
	);
	assert!((reduction.total_probability - 1.0).abs() < 1e-10);
	let report = prepared.report();
	assert!(
		(report.physical_rescaling - report.rhs_norm / (report.alpha * report.reciprocal_scale))
			.abs()
			< 1e-13
	);
	if comm.rank()? == 0 {
		println!(
			"group={group} dimension={} degree={} alpha={} rhs_gates={} producer_edges={} success={} scale={} selected_mass={}",
			report.history_dimension,
			report.polynomial_degree,
			report.alpha,
			report.rhs_preparation.elementary_gates,
			report.producer.global_edges,
			reduction.inverse_success_probability,
			report.physical_rescaling,
			reduction.physical_selected_squared_norm
		);
	}
	// Every failure and padding sector contains a distinct nonzero amplitude.
	let local = state.deployment().local_amplitudes();
	let start = state.deployment().rank() * local;
	let values = (0..local)
		.map(|i| {
			let x = f64::from(u32::try_from(start + i).unwrap_or(0));
			Complex64::new((0.13 * x).sin(), (0.17 * x).cos())
		})
		.collect::<Vec<_>>();
	state.write_local_amplitudes(0, &values)?;
	for value in [0, 1] {
		prepared.prepare_rhs(&mut state, 1, value, false)?;
		prepared.prepare_rhs(&mut state, 1, value, true)?;
		for (a, b) in state.read_local_amplitudes(0, local)?.iter().zip(&values) {
			assert!((*a - *b).norm() < 2e-12);
		}
	}
	prepared.apply_inverse(&mut state, false)?;
	prepared.apply_inverse(&mut state, true)?;
	for (actual, expected) in state.read_local_amplitudes(0, local)?.iter().zip(&values) {
		assert!((*actual - *expected).norm() < 2e-10);
	}
	Ok(())
}
#[test]
fn collective_history_admission_and_zero_rhs_do_not_mutate_state()
-> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_CFD_MPI_ADMISSION").is_none() {
		let status = quest_test_support::mpi::MpiTest::new(4, std::time::Duration::from_secs(90))?
			.args([
				"--exact",
				"collective_history_admission_and_zero_rhs_do_not_mutate_state",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_CFD_MPI_ADMISSION", "1")
			.status()?;
		assert!(status.success());
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(quest::MemoryBudget::new(16 * 1024 * 1024))
		.build()?;
	let recipe = TemporalHistoryRecipe::new(&Dynamics, 0.1, 1, 1, HistoryStreamLimits::default())?;
	let spectrum = quest_qsvt::reciprocal::SpectralBounds::new(
		0.5,
		1.0,
		quest_qsvt::reciprocal::SpectralEvidence::CallerPremise {
			description: "test caller spectral premise".into(),
		},
	)?;
	let before = env.view().allocated_bytes();
	let rank_bytes = usize::try_from(comm.rank()?)?
		.checked_add(1)
		.and_then(|n| n.checked_mul(64))
		.ok_or("external bytes")?;
	let reservation = env.reserve_external_bytes(rank_bytes)?;
	assert_eq!(reservation.bytes(), rank_bytes);
	assert_eq!(env.view().allocated_bytes(), before + rank_bytes);
	drop(reservation);
	assert_eq!(env.view().allocated_bytes(), before);
	assert!(
		env.reserve_external_bytes(if comm.rank()? == 0 { usize::MAX } else { 64 })
			.is_err()
	);
	assert_eq!(env.view().allocated_bytes(), before);

	for limits in [
		DistributedHistoryLimits {
			max_local_bytes: 1,
			..Default::default()
		},
		DistributedHistoryLimits {
			max_rhs_query_work: 0,
			..Default::default()
		},
		DistributedHistoryLimits {
			max_degree: 1,
			..Default::default()
		},
	] {
		assert!(
			prepare_history_inverse(&env, &recipe, |_| Ok(1.0.into()), &spectrum, limits).is_err()
		);
		assert_eq!(env.view().allocated_bytes(), before);
	}
	assert!(
		prepare_history_inverse(
			&env,
			&recipe,
			|_| if comm.rank().unwrap_or(0) == 0 {
				Err(CfdError::InvalidInput("injected initial error"))
			} else {
				Ok(1.0.into())
			},
			&spectrum,
			DistributedHistoryLimits::default()
		)
		.is_err()
	);
	assert_eq!(env.view().allocated_bytes(), before);

	let grid = TemporalHistoryRecipe::new(
		&Dynamics,
		0.1,
		if comm.rank()? == 0 { 2 } else { 3 },
		if comm.rank()? == 0 { 2 } else { 1 },
		HistoryStreamLimits::default(),
	)?;
	let mut queried = 0;
	assert!(
		prepare_history_inverse(
			&env,
			&grid,
			|_| {
				queried += 1;
				Ok(1.0.into())
			},
			&spectrum,
			DistributedHistoryLimits {
				max_degree: 1,
				..Default::default()
			}
		)
		.is_err()
	);
	let no_query = comm.collective_lane()?.all_agree(queried == 0)?;
	assert!(
		no_query,
		"different temporal grids with equal dimension must reject before RHS generation"
	);
	let limits = DistributedHistoryLimits {
		certification: None,
		approximation_tolerance: 1e-2,
		preparation: quest::qsvt::amplitude_preparation::DistributedPreparationLimits {
			max_native_dispatches: 0,
			..Default::default()
		},
		..Default::default()
	};
	let DistributedHistoryOutcome::Prepared(mut inadmissible) =
		prepare_history_inverse(&env, &recipe, |_| Ok(1.0.into()), &spectrum, limits)?
	else {
		return Err("unexpected zero".into());
	};
	let mut state = env.state_vector_local(inadmissible.qubit_count())?;
	state.init_plus()?;
	let original = state.read_local_amplitudes(0, state.deployment().local_amplitudes())?;
	assert!(inadmissible.initialize_rhs(&mut state).is_err());
	assert_eq!(
		state.read_local_amplitudes(0, state.deployment().local_amplitudes())?,
		original
	);
	let wrong = env.state_vector_local(quest::QubitCount::new(
		inadmissible.qubit_count().get() + 1,
	)?)?;
	assert!(
		inadmissible
			.reduce_observable(&wrong, |_| Ok((true, 1.0.into())))
			.is_err()
	);
	assert!(
		inadmissible
			.reduce_observable(&state, |_| Ok((true, Complex64::new(f64::MAX / 2.0, 0.0))))
			.is_err(),
		"scaled finite observable must reject overflow collectively"
	);
	assert!(
		inadmissible
			.reduce_observable(&state, |_| if comm.rank().unwrap_or(0) == 0 {
				Err(CfdError::InvalidInput("injected observable error"))
			} else {
				Ok((true, 1.0.into()))
			})
			.is_err()
	);
	assert_eq!(
		state.read_local_amplitudes(0, state.deployment().local_amplitudes())?,
		original
	);
	drop(wrong);
	drop(state);
	drop(inadmissible);
	assert_eq!(env.view().allocated_bytes(), before);
	struct Zero;
	impl HistoryRowDynamics for Zero {
		fn dimension(&self) -> usize {
			4
		}
		fn max_row_entries(&self) -> usize {
			0
		}
		fn retained_bytes(&self) -> Result<usize, CfdError> {
			Ok(0)
		}
		fn row_query_bytes(&self) -> usize {
			0
		}
		fn row_query_work(&self) -> usize {
			0
		}
		fn visit_row(
			&self,
			_: f64,
			_: usize,
			_: &mut dyn FnMut(usize, Complex64) -> Result<(), CfdError>,
		) -> Result<(), CfdError> {
			Ok(())
		}
		fn source_entry(&self, _: f64, _: usize) -> Result<Complex64, CfdError> {
			Ok(0.0.into())
		}
	}
	let zero = TemporalHistoryRecipe::new(&Zero, 0.1, 1, 1, HistoryStreamLimits::default())?;
	assert!(matches!(
		prepare_history_inverse(
			&env,
			&zero,
			|_| Ok(0.0.into()),
			&spectrum,
			DistributedHistoryLimits::default()
		)?,
		DistributedHistoryOutcome::ZeroRhs { .. }
	));
	assert_eq!(env.view().allocated_bytes(), before);
	Ok(())
}
