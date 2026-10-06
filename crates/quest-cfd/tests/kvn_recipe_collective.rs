#![cfg(feature = "distributed")]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	clippy::too_many_lines,
	clippy::suboptimal_flops,
	reason = "Explicitly bounded five-coordinate distributed integration smoke and scalar invariants"
)]
use quest::{
	MemoryBudget,
	collective::{CollectiveEnvironment, MpiRuntime},
};
use quest_cfd::{
	CfdError, PeriodicBdm1,
	configuration::ConfigurationGrid,
	distributed_history::{
		DistributedHistoryLimits, DistributedHistoryOutcome, prepare_history_inverse,
	},
	kvn_recipe::{CompactBumpRecipe, KvnHistoryRecipe, KvnRecipeLimits},
	polynomial::PolynomialOde,
	stream_history::{HistoryRowDynamics, HistoryStreamLimits, TemporalHistoryRecipe},
};
use quest_numerics::Complex64;
use quest_qsvt::reciprocal::{SpectralBounds, SpectralEvidence};
#[test]
fn full_five_coordinate_generated_kvn_reaches_distributed_history_inverse()
-> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_KVN_RECIPE_CHILD").is_none() {
		for ranks in [1, 2] {
			let status = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(180),
			)?
			.args([
				"--exact",
				"full_five_coordinate_generated_kvn_reaches_distributed_history_inverse",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_KVN_RECIPE_CHILD", "1")
			.status()?;
			assert!(status.success(), "generated KvN MPI ranks={ranks}");
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(16 * 1024 * 1024))
		.build()?;
	let flow = PeriodicBdm1::assemble(0.01)?;
	let ode = PolynomialOde::from_periodic_bdm1(
		&flow,
		mathcore::multivariate::PolynomialLimits::default(),
	)?
	.dynamics;
	let grid = ConfigurationGrid::uniform(5, -0.3, 0.3, 1, 2, 243)?;
	let energy = quest_cfd::probability_observation::KvnKineticEnergy::periodic_bdm1(
		&grid,
		&flow,
		KvnRecipeLimits::default(),
	)?;
	drop(flow);
	let dynamics = KvnHistoryRecipe::new(&grid, &ode, KvnRecipeLimits::default())?;
	let bump = CompactBumpRecipe::new(&grid, vec![0.; 5], 0.25, KvnRecipeLimits::default())?;
	let time = 0.001;
	let history = TemporalHistoryRecipe::new(
		&dynamics,
		time,
		1,
		1,
		HistoryStreamLimits {
			max_work_per_visit: 2_000_000_000,
			..Default::default()
		},
	)?;
	// Scalar-only spectral/initial diagnostics. This remains a caller premise about
	// floating evaluation, not a newly claimed interval certificate. Exact DG1 with
	// skew L has Hermitian part I/2; an induced-norm bound is 1 + dt*max_row_sum(L)/2.
	let diagnostic_work = grid
		.dimension()
		.checked_mul(dynamics.row_query_work() + bump.query_work())
		.ok_or("diagnostic work overflow")?;
	assert!(diagnostic_work < 2_000_000_000);
	let mut largest = 0_f64;
	let mut initial_norm = 0_f64;
	for row in 0..grid.dimension() {
		let mut sum = 0.;
		dynamics.visit_row(0., row, &mut |_, v| {
			sum += v.norm();
			Ok(())
		})?;
		largest = largest.max(sum);
		initial_norm = initial_norm.hypot(bump.amplitude(row)?.norm());
	}
	assert!(largest > 1e-3, "nonzero full nonlinear generator required");
	assert!(initial_norm > 0.);
	let spectrum=SpectralBounds::new(0.49,1.01+time*largest/2.,SpectralEvidence::CallerPremise{description:"tiny generated KvN smoke: DG1 skew-generator coercivity/norm with conservative floating margins; not a new interval proof".into()})?;
	let outcome = prepare_history_inverse(
		&env,
		&history,
		|i| bump.amplitude(i),
		&spectrum,
		DistributedHistoryLimits {
			max_local_bytes: 512 * 1024 * 1024,
			max_node_bytes: 1024 * 1024 * 1024,
			initial_retained_bytes: bump.retained_bytes(),
			initial_query_bytes: bump.query_bytes(),
			initial_query_work: bump.query_work(),
			max_rhs_query_work: 2_000_000_000,
			approximation_tolerance: 1e-3,
			producer: quest::qsvt::matching::preprocess::ProducerLimits {
				stream: quest_numerics::sparse_stream::StreamLimits {
					buffer_entries: 16_384,
					..Default::default()
				},
				..Default::default()
			},
			..Default::default()
		},
	)?;
	let DistributedHistoryOutcome::Prepared(mut prepared) = outcome else {
		return Err("resolved bump unexpectedly zero".into());
	};
	assert_eq!(prepared.report().history_dimension, 486);
	assert!(prepared.report().projector_response_bound.is_some());
	let mut state = env.state_vector_local(prepared.qubit_count())?;
	prepared.initialize_rhs(&mut state)?;
	prepared.apply_inverse(&mut state, false)?;
	let final_mass =
		prepared.reduce_observable(&state, |i| Ok((i / 243 == 1, Complex64::new(0., 0.))))?;
	assert!((final_mass.total_probability - 1.).abs() < 1e-9);
	// Continuum skew transport preserves mass. For this tiny time window, the
	// DG1 endpoint contraction plus 1e-3 inverse approximation is below this margin.
	let ratio = final_mass.physical_selected_squared_norm / (initial_norm * initial_norm);
	assert!((ratio - 1.).abs() < 0.04, "endpoint mass ratio={ratio}");

	// Probability readout observes the right nodal trace of this actual full nonlinear KvN inverse.
	let node = quest_cfd::probability_observation::TemporalNodeSelection::new(&history, 0, 1)?;
	let observed = prepared.reduce_probability_observable(
		&state,
		quest_cfd::distributed_history::ProbabilityReadoutRequest {
			range: energy.range(),
			projection: quest_cfd::probability_observation::HistoryProjection::TemporalNode(node),
			source_identity: 0x4b56_4e45_4e45_5247,
			limits: quest_cfd::distributed_history::ProbabilityReadoutLimits {
				callback_retained_bytes: energy.retained_bytes(),
				callback_query_work: energy.query_work(),
				..Default::default()
			},
		},
		|i| {
			Ok((
				true,
				energy.value(
					node.configuration_index(i)
						.ok_or(CfdError::InvalidInput("energy node selection"))?,
				)?,
			))
		},
	)?;
	assert!(
		(observed.physical_selected_squared_norm - final_mass.physical_selected_squared_norm).abs()
			< 1e-12
	);
	assert!(
		observed
			.conditional_expectation
			.is_some_and(|x| energy.range().contains(x))
	);
	assert!(observed.conditional_variance.is_some_and(|x| x >= 0.));
	if comm.rank()? == 0 {
		println!(
			"full KvN history={} grid={} physical=5 degree={} row_work={} peak={} mass_ratio={ratio}",
			history.dimension(),
			grid.dimension(),
			prepared.report().polynomial_degree,
			dynamics.row_query_work(),
			prepared.report().managed_local_peak_bytes
		);
	}
	Ok(())
}
