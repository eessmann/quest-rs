#![cfg(feature = "distributed")]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	clippy::too_many_lines,
	reason = "Small independent full-register probability and collective failure checks"
)]
use quest::collective::{CollectiveEnvironment, MpiRuntime};
use quest_cfd::{
	CfdError,
	distributed_history::{
		DistributedHistoryLimits, DistributedHistoryOutcome, ProbabilityReadoutLimits,
		ProbabilityReadoutRequest, prepare_history_inverse,
	},
	probability_observation::{DiagonalRange, HistoryProjection, TemporalNodeSelection},
	stream_history::{HistoryRowDynamics, HistoryStreamLimits, TemporalHistoryRecipe},
};
use quest_numerics::Complex64;
struct Zero(usize);
impl HistoryRowDynamics for Zero {
	fn dimension(&self) -> usize {
		self.0
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
		1
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
		Ok(Complex64::new(0., 0.))
	}
}
#[test]
fn probability_observation_preserves_all_branches_and_collective_admission()
-> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_PROBABILITY_CHILD").is_none() {
		for (ranks, split) in [(1, 0), (2, 0), (4, 0), (4, 1)] {
			let status = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(120),
			)?
			.args([
				"--exact",
				"probability_observation_preserves_all_branches_and_collective_admission",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_PROBABILITY_CHILD", "1")
			.env("QUEST_PROBABILITY_SPLIT", split.to_string())
			.status()?;
			assert!(status.success(), "ranks={ranks} split={split}");
		}
		return Ok(());
	}
	let rt = MpiRuntime::initialize()?;
	let mut world = rt.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_PROBABILITY_SPLIT").as_deref() == Ok("1") {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(quest::MemoryBudget::new(16 * 1024 * 1024))
		.build()?;
	let history = TemporalHistoryRecipe::new(&Zero(3), 0.1, 1, 1, HistoryStreamLimits::default())?;
	let spectral = quest_qsvt::reciprocal::SpectralBounds::new(
		0.5,
		1.,
		quest_qsvt::reciprocal::SpectralEvidence::CallerPremise {
			description: "test zero-generator temporal matrix only".into(),
		},
	)?;
	let DistributedHistoryOutcome::Prepared(prepared) = prepare_history_inverse(
		&env,
		&history,
		|_| Ok(Complex64::new(1., 0.)),
		&spectral,
		DistributedHistoryLimits {
			approximation_tolerance: 0.1,
			certification: None,
			..Default::default()
		},
	)?
	else {
		return Err("zero".into());
	};
	let mut state = env.state_vector_local(prepared.qubit_count())?;
	let local = state.deployment().local_amplitudes();
	let start = state.deployment().rank() * local;
	let special = [
		(0, 0.1_f64),
		(6, 0.2),
		(8, 0.3),
		(12, 0.1),
		(1, 0.15),
		(32, 0.15),
	];
	let values = (start..start + local)
		.map(|i| {
			Complex64::new(
				special
					.iter()
					.find(|(j, _)| *j == i)
					.map_or(0., |(_, p)| p.sqrt()),
				0.,
			)
		})
		.collect::<Vec<_>>();
	state.write_local_amplitudes(0, &values)?;
	let node = TemporalNodeSelection::new(&history, 0, 1)?;
	let request = ProbabilityReadoutRequest {
		range: DiagonalRange::new(1., 3.)?,
		projection: HistoryProjection::TemporalNode(node),
		source_identity: 719,
		limits: ProbabilityReadoutLimits {
			chunk_amplitudes: 7,
			..Default::default()
		},
	};
	let select = |i| Ok((true, if i == 3 { 1. } else { 3. }));
	let before = env.view().allocated_bytes();
	let got = prepared.reduce_probability_observable(&state, request, select)?;
	assert!((got.total_probability_mass - 1.).abs() < 1e-13);
	assert!((got.inverse_success_probability - 0.6).abs() < 1e-13);
	assert!((got.selected_probability - 0.5).abs() < 1e-13);
	assert!((got.selected_probability_given_inverse.ok_or("inverse")? - 5. / 6.).abs() < 1e-13);
	assert!((got.weighted_first_moment - 1.1).abs() < 1e-13);
	assert!((got.weighted_second_moment - 2.9).abs() < 1e-13);
	assert!((got.conditional_expectation.ok_or("mean")? - 2.2).abs() < 1e-13);
	assert!((got.conditional_variance.ok_or("variance")? - 0.96).abs() < 1e-13);
	let scale = prepared.report().physical_rescaling;
	assert!((got.physical_scaled_quadratic_functional / (scale * scale) - 1.1).abs() < 1e-13);
	assert_eq!(env.view().allocated_bytes(), before);
	assert_eq!(state.read_local_amplitudes(0, local)?, values);
	for limits in [
		ProbabilityReadoutLimits {
			max_work: got.resources.work_bound - 1,
			..request.limits
		},
		ProbabilityReadoutLimits {
			max_transport_bytes: got.resources.transport_byte_bound - 1,
			..request.limits
		},
		ProbabilityReadoutLimits {
			max_bytes: got.resources.local_extra_bytes - 1,
			..request.limits
		},
	] {
		assert!(
			prepared
				.reduce_probability_observable(
					&state,
					ProbabilityReadoutRequest { limits, ..request },
					select
				)
				.is_err()
		);
		assert_eq!(env.view().allocated_bytes(), before);
	}
	assert!(
		prepared
			.reduce_probability_observable(&state, request, |i| Ok((
				true,
				if i == 3 { 4. } else { 2. }
			)))
			.is_err()
	);
	assert!(
		prepared
			.reduce_probability_observable(&state, request, |i| if i == 3 {
				Err(CfdError::InvalidInput("one owner error"))
			} else {
				Ok((true, 2.))
			})
			.is_err()
	);
	let wrong = TemporalHistoryRecipe::new(&Zero(2), 0.1, 1, 2, HistoryStreamLimits::default())?;
	assert!(
		prepared
			.reduce_probability_observable(
				&state,
				ProbabilityReadoutRequest {
					projection: HistoryProjection::TemporalNode(TemporalNodeSelection::new(
						&wrong, 0, 1
					)?),
					..request
				},
				select
			)
			.is_err()
	);
	if env.size()? > 1 {
		assert!(
			prepared
				.reduce_probability_observable(
					&state,
					ProbabilityReadoutRequest {
						source_identity: if env.rank()? == 0 { 1 } else { 2 },
						..request
					},
					select
				)
				.is_err()
		);
	}

	// Changing a callback between the two passes is collectively rejected.
	let changed = std::cell::Cell::new(false);
	assert!(
		prepared
			.reduce_probability_observable(&state, request, |i| {
				let value = if i == 3 {
					let previous = changed.replace(true);
					if previous { 2. } else { 1. }
				} else {
					2.
				};
				Ok((true, value))
			})
			.is_err()
	);
	// Probability conditioning is separate from raw amplitudes and inverse scale.
	let doubled = values.iter().map(|z| 2. * z).collect::<Vec<_>>();
	state.write_local_amplitudes(0, &doubled)?;
	let scaled = prepared.reduce_probability_observable(&state, request, select)?;
	assert!((scaled.total_probability_mass - 4.).abs() < 1e-12);
	assert!((scaled.selected_probability - got.selected_probability).abs() < 1e-13);
	assert!((scaled.conditional_expectation.ok_or("scaled mean")? - 2.2).abs() < 1e-13);
	assert!(
		(scaled.physical_scaled_quadratic_functional / got.physical_scaled_quadratic_functional
			- 4.)
			.abs()
			< 1e-12
	);
	state.write_local_amplitudes(0, &values)?;
	let plan = quest_cfd::probability_observation::plan_sampling(
		quest_cfd::probability_observation::SamplingRequest {
			range: request.range,
			absolute_error: 0.1,
			failure_probability: 0.05,
			joint_success_lower_bound: 0.1,
			success_bound_provenance: "independent hypothetical caller bound; not this simulator result",
			max_selected_shots: 1_000_000,
			max_attempted_shots: 100_000_000,
			max_provenance_bytes: 4096,
			systematic_bias_bound: None,
		},
	)?;
	let cost = prepared.sampling_cost(&plan, 5, 17)?;
	assert_eq!(cost.attempted_state_preparations, plan.attempted_shots);
	assert_eq!(
		cost.inverse_matching_oracle_calls,
		plan.attempted_shots * 2 * u64::try_from(prepared.report().polynomial_degree)?
	);
	assert_eq!(
		cost.classical_selected_sample_work,
		plan.selected_shots * 17
	);
	assert!(!cost.quantum_measurements_executed);
	let empty = prepared.reduce_probability_observable(&state, request, |_| Ok((false, 2.)))?;
	assert!(empty.conditional_expectation.is_none() && empty.conditional_variance.is_none());
	assert_eq!(env.view().allocated_bytes(), before);
	Ok(())
}
