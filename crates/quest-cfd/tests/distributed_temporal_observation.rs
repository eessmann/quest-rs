#![cfg(feature = "distributed")]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	clippy::suboptimal_flops,
	clippy::too_many_lines,
	reason = "Small complete-register MPI composition fixtures and independent complex interpolation assertions"
)]
use quest::{
	MemoryBudget,
	collective::{CollectiveEnvironment, MpiRuntime},
};
use quest_cfd::{
	CfdError,
	configuration::ConfigurationGrid,
	distributed_history::{
		DistributedHistoryLimits, DistributedHistoryOutcome, TemporalReadoutLimits,
		prepare_history_inverse,
	},
	kvn_recipe::KvnRecipeLimits,
	physical_observation::{
		PhysicalObservableKind, PhysicalObservableLimits, PreparedPhysicalObservable,
	},
	physical_space::PhysicalSpace,
	simplex::BoxBoundary,
	stream_history::{HistoryRowDynamics, HistoryStreamLimits, TemporalHistoryRecipe},
	temporal_encoding::{TemporalEncoding, TemporalEncodingLimits},
	temporal_observation::{TemporalInterpolation, TemporalInterpolationLimits},
};
use quest_numerics::Complex64;
use quest_qsvt::reciprocal::{SpectralBounds, SpectralEvidence};
struct Zero;
impl HistoryRowDynamics for Zero {
	fn dimension(&self) -> usize {
		3
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
impl quest_cfd::history::HistoryDynamics for Zero {
	fn dimension(&self) -> usize {
		3
	}
	fn max_generator_entries(&self) -> usize {
		0
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(0)
	}
	fn visit_generator(
		&self,
		_: f64,
		_: &mut dyn FnMut(usize, usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		Ok(())
	}
	fn source(&self, _: f64, out: &mut [Complex64]) -> Result<(), CfdError> {
		out.fill(Complex64::new(0., 0.));
		Ok(())
	}
}
#[test]
fn projected_temporal_composition_preserves_interference_scaling_and_rejects_before_mutation()
-> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_TEMPORAL_OBSERVATION_CHILD").is_none() {
		for (ranks, split) in [(1, 0), (2, 0), (4, 0), (8, 0), (4, 1)] {
			let status = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(120),
			)?
			.args([
				"--exact",
				"projected_temporal_composition_preserves_interference_scaling_and_rejects_before_mutation",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_TEMPORAL_OBSERVATION_CHILD", "1")
			.env("QUEST_TEMPORAL_OBSERVATION_SPLIT", split.to_string())
			.status()?;
			assert!(
				status.success(),
				"temporal composition MPI{ranks}/split{split}"
			);
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_TEMPORAL_OBSERVATION_SPLIT").as_deref() == Ok("1") {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(16 * 1024 * 1024))
		.build()?;
	let model = PhysicalSpace::box_mesh(2, 1, 1., 0.02, BoxBoundary::Cavity { lid_speed: 0. }, 1)?;
	assert_eq!(model.dimension(), 1);
	let grid = ConfigurationGrid::uniform(1, -0.3, 0.3, 1, 2, 3)?;
	let prepared_observable = PreparedPhysicalObservable::box_space(
		&model,
		PhysicalObservableKind::VelocityComponent {
			point: [0.37, 0.23, 0.],
			component: 0,
		},
		PhysicalObservableLimits::default(),
	)?;
	let observable = prepared_observable.configuration(&grid, KvnRecipeLimits::default())?;
	let history = TemporalHistoryRecipe::new(&Zero, 0.1, 1, 1, HistoryStreamLimits::default())?;
	let interpolation =
		TemporalInterpolation::new(&history, 0, 0.5, TemporalInterpolationLimits::default())?;
	let encoding = TemporalEncoding::new(&interpolation, TemporalEncodingLimits::default())?;
	let spectrum = SpectralBounds::new(
		0.49,
		1.,
		SpectralEvidence::CallerPremise {
			description:
				"zero-dynamics DG1 blocks [[0.5,0.5],[-0.5,0.5]] have both singular values1/sqrt(2); bounds[0.49,1] are conservative; fixture only".into(),
		},
	)?;
	let initial = [
		Complex64::new(0.2, 0.1),
		Complex64::new(-0.3, 0.2),
		Complex64::new(0.2, -0.3),
	];
	let DistributedHistoryOutcome::Prepared(mut inverse) = prepare_history_inverse(
		&env,
		&history,
		|i| Ok(initial[i]),
		&spectrum,
		DistributedHistoryLimits {
			approximation_tolerance: 1e-3,
			..Default::default()
		},
	)?
	else {
		return Err("unexpected zero RHS".into());
	};
	let mut state = env.state_vector_local(inverse.qubit_count())?;
	let local = state.deployment().local_amplitudes();
	let start = state.deployment().rank() * local;
	let history_values = [
		Complex64::new(0.3, 0.1),
		Complex64::new(-0.1, 0.2),
		Complex64::new(0.2, -0.3),
		Complex64::new(-0.2, 0.4),
		Complex64::new(0.4, -0.1),
		Complex64::new(0.1, 0.2),
	];
	let value = |index: usize| {
		if index.is_multiple_of(2) && index / 2 < 6 {
			history_values[index / 2]
		} else if index == 12 {
			Complex64::new(0.31, -0.17)
		} else if index == 1 {
			Complex64::new(0.2, 0.1)
		} else if index == 16 {
			Complex64::new(-0.13, 0.21)
		} else {
			Complex64::new(0., 0.)
		}
	};
	let input = (start..start + local).map(value).collect::<Vec<_>>();
	state.write_local_amplitudes(0, &input)?;
	let total = state.total_probability()?;
	for limits in [
		TemporalReadoutLimits {
			max_work: 0,
			source_identity: 7,
			..Default::default()
		},
		TemporalReadoutLimits {
			max_bytes: 0,
			source_identity: 7,
			..Default::default()
		},
		TemporalReadoutLimits {
			max_transport_bytes: 0,
			source_identity: 7,
			..Default::default()
		},
		TemporalReadoutLimits {
			max_temporal_gates: 0,
			source_identity: 7,
			..Default::default()
		},
		TemporalReadoutLimits {
			source_identity: if comm.rank()? == 0 { 0 } else { 7 },
			..Default::default()
		},
	] {
		assert!(
			inverse
				.observe_temporal_postselected(&mut state, &encoding, &observable, limits)
				.is_err()
		);
		assert_eq!(state.read_local_amplitudes(0, local)?, input);
	}
	let different_history =
		TemporalHistoryRecipe::new(&Zero, 0.2, 1, 1, HistoryStreamLimits::default())?;
	let wrong = TemporalEncoding::new(
		&TemporalInterpolation::new(
			&different_history,
			0,
			0.5,
			TemporalInterpolationLimits::default(),
		)?,
		TemporalEncodingLimits::default(),
	)?;
	assert!(
		inverse
			.observe_temporal_postselected(
				&mut state,
				&wrong,
				&observable,
				TemporalReadoutLimits {
					source_identity: 7,
					..Default::default()
				}
			)
			.is_err()
	);
	assert_eq!(state.read_local_amplitudes(0, local)?, input);
	let report = inverse.observe_temporal_postselected(
		&mut state,
		&encoding,
		&observable,
		TemporalReadoutLimits {
			source_identity: 7,
			..Default::default()
		},
	)?;
	let expected = (0..3)
		.map(|i| 0.5 * (history_values[i] + history_values[i + 3]))
		.collect::<Vec<_>>();
	let norm = expected.iter().map(Complex64::norm_sqr).sum::<f64>();
	let first = (0..3)
		.map(|i| {
			Ok(expected[i].norm_sqr()
				* model.sample_velocity(&grid.point(i).ok_or("grid")?, [0.37, 0.23, 0.])?[0])
		})
		.collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?
		.iter()
		.sum::<f64>();
	let alpha = encoding.resources().normalization;
	assert!((report.original_total_mass - total).abs() < 1e-12);
	assert!(report.original_workspace_failure_mass > 0.);
	assert!(report.original_clean_padding_mass > 0.);
	assert!(
		(report.joint_temporal_success_probability - norm / (alpha * alpha * total)).abs() < 1e-12
	);
	assert!(
		(report.conditional_expectation.ok_or("nonzero expected")? - first / norm).abs() < 1e-10
	);
	assert!(
		(report.physical_scaled_quadratic_functional
			- first * inverse.report().physical_rescaling.powi(2))
		.abs()
			< 1e-10
	);
	assert!(!report.quantum_measurements_executed);
	assert_eq!(report.temporal_slab, 0);
	assert_eq!(
		report.temporal_side,
		quest_cfd::probability_observation::TemporalNodeSide::Interior
	);
	assert_ne!(report.temporal_source_identity, 0);
	assert_ne!(report.temporal_construction_identity, 0);
	assert!(report.resources.temporal_gates > 0);
	for (i, amplitude) in state.read_local_amplitudes(0, local)?.iter().enumerate() {
		let physical = start + i;
		let target = if physical.is_multiple_of(2) && physical / 2 < 3 {
			expected[physical / 2] / alpha
		} else {
			Complex64::new(0., 0.)
		};
		assert!((*amplitude - target).norm() < 1e-12);
	}
	// Execute the prepared inverse and then recover its constant-in-time physical field.
	inverse.initialize_rhs(&mut state)?;
	inverse.apply_inverse(&mut state, false)?;
	let actual = inverse.observe_temporal_postselected(
		&mut state,
		&encoding,
		&observable,
		TemporalReadoutLimits {
			source_identity: 7,
			..Default::default()
		},
	)?;
	let direct = (0..3)
		.map(|i| Ok(initial[i].norm_sqr() * observable.value(i)?))
		.collect::<Result<Vec<_>, CfdError>>()?
		.iter()
		.sum::<f64>();
	assert!(
		direct.abs() > 1e-4,
		"nonzero full physical observable signal required"
	);
	assert!((actual.physical_scaled_quadratic_functional - direct).abs() < 0.01 * direct.abs());
	// Opposite amplitudes cancel; no probability-mixture fallback is allowed.
	let cancelled = (start..start + local)
		.map(|i| {
			if i == 0 {
				Complex64::new(0.5, 0.2)
			} else if i == 6 {
				Complex64::new(-0.5, -0.2)
			} else {
				Complex64::new(0., 0.)
			}
		})
		.collect::<Vec<_>>();
	state.write_local_amplitudes(0, &cancelled)?;
	let zero = inverse.observe_temporal_postselected(
		&mut state,
		&encoding,
		&observable,
		TemporalReadoutLimits {
			source_identity: 7,
			..Default::default()
		},
	)?;
	assert!(zero.joint_temporal_success_probability < 1e-25);
	assert!(zero.physical_selected_squared_norm < 1e-25);
	let only_failure = (start..start + local)
		.map(|i| {
			if i == 1 {
				Complex64::new(0.2, 0.)
			} else {
				Complex64::new(0., 0.)
			}
		})
		.collect::<Vec<_>>();
	state.write_local_amplitudes(0, &only_failure)?;
	let empty = inverse.observe_temporal_postselected(
		&mut state,
		&encoding,
		&observable,
		TemporalReadoutLimits {
			source_identity: 7,
			..Default::default()
		},
	)?;
	assert!(empty.exact_zero_projected_branch);
	assert!(empty.conditional_expectation.is_none());
	assert!(empty.original_workspace_failure_mass > 0.);
	assert!(
		inverse
			.observe_temporal_postselected(
				&mut state,
				&encoding,
				&observable,
				TemporalReadoutLimits {
					source_identity: 7,
					..Default::default()
				}
			)
			.is_err()
	);
	assert!(state.total_probability()?.abs() < f64::MIN_POSITIVE);
	if comm.size()? == 8 {
		return Ok(());
	}
	// DG2 negative weight: native amplitudes must equal independent quadratic-in-time interpolation.
	let dg2_history = TemporalHistoryRecipe::new(&Zero, 0.1, 1, 2, HistoryStreamLimits::default())?;
	let dg2_interpolation = TemporalInterpolation::new(
		&dg2_history,
		0,
		0.25,
		TemporalInterpolationLimits::default(),
	)?;
	let dg2_encoding =
		TemporalEncoding::new(&dg2_interpolation, TemporalEncodingLimits::default())?;
	let stored = quest_cfd::history::HistorySystem::assemble_dynamics(
		&Zero,
		&initial,
		0.1,
		1,
		2,
		quest_numerics::SparseLimits::default(),
	)?;
	let dg2_spectrum = stored.spectral_bounds(quest_numerics::SparseLimits::default())?;
	let DistributedHistoryOutcome::Prepared(dg2_inverse) = prepare_history_inverse(
		&env,
		&dg2_history,
		|i| Ok(initial[i]),
		&dg2_spectrum,
		DistributedHistoryLimits {
			approximation_tolerance: 1e-3,
			..Default::default()
		},
	)?
	else {
		return Err("unexpected DG2 zero RHS".into());
	};
	let mut dg2_state = env.state_vector_local(dg2_inverse.qubit_count())?;
	let dg2_local = dg2_state.deployment().local_amplitudes();
	let dg2_start = dg2_state.deployment().rank() * dg2_local;
	let analytic = |i: usize, t: f64| {
		let a = f64::from(u32::try_from(i + 1).unwrap_or(0));
		Complex64::new(0.2 * a + t * t, 0.1 * a - t)
	};
	let nodal = [0., 0.5, 1.];
	let dg2_input = (dg2_start..dg2_start + dg2_local)
		.map(|physical| {
			if physical.is_multiple_of(2) && physical / 2 < 9 {
				let logical = physical / 2;
				analytic(logical % 3, nodal[logical / 3])
			} else if physical == 18 || physical == 1 {
				Complex64::new(0.31, -0.17)
			} else {
				Complex64::new(0., 0.)
			}
		})
		.collect::<Vec<_>>();
	dg2_state.write_local_amplitudes(0, &dg2_input)?;
	let dg2_report = dg2_inverse.observe_temporal_postselected(
		&mut dg2_state,
		&dg2_encoding,
		&observable,
		TemporalReadoutLimits {
			source_identity: 7,
			..Default::default()
		},
	)?;
	assert!(
		dg2_report.original_workspace_failure_mass > 0.
			&& dg2_report.original_clean_padding_mass > 0.
	);
	let dg2_alpha = dg2_encoding.resources().normalization;
	for (i, amplitude) in dg2_state
		.read_local_amplitudes(0, dg2_local)?
		.iter()
		.enumerate()
	{
		let physical = dg2_start + i;
		let expected = if physical.is_multiple_of(2) && physical / 2 < 3 {
			analytic(physical / 2, 0.25) / dg2_alpha
		} else {
			Complex64::new(0., 0.)
		};
		assert!(
			(*amplitude - expected).norm() < 2e-12,
			"DG2 phase/interference mismatch at {physical}"
		);
	}

	Ok(())
}

#[test]
fn zero_generator_dg1_block_supports_the_spectral_premise() -> Result<(), CfdError> {
	let history = TemporalHistoryRecipe::new(&Zero, 0.1, 1, 1, HistoryStreamLimits::default())?;
	let mut matrix = [[Complex64::new(0., 0.); 6]; 6];
	for entry in history.rows(0..6)? {
		let entry = entry?;
		matrix[entry.row][entry.column] += entry.value;
	}
	for (row, expected) in [(0, [0.5, 0.5]), (3, [-0.5, 0.5])] {
		for (column, value) in [0, 3].into_iter().zip(expected) {
			assert!((matrix[row][column] - Complex64::new(value, 0.)).norm() < 1e-15);
		}
	}
	let norm = matrix[0][0].norm_sqr() + matrix[3][0].norm_sqr();
	let cross = matrix[0][0].conj() * matrix[0][3] + matrix[3][0].conj() * matrix[3][3];
	assert!((norm - 0.5).abs() < 1e-15);
	assert!(cross.norm() < 1e-15);
	Ok(())
}
