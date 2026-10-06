#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Bounded independent interpolation matrices and whole-register assertions"
)]
#[path = "../../quest-qsvt/tests/portfolio_support/mod.rs"]
mod whole_unitary;
use quest_cfd::{
	CfdError,
	stream_history::{HistoryRowDynamics, HistoryStreamLimits, TemporalHistoryRecipe},
	temporal_encoding::{TemporalEncoding, TemporalEncodingLimits},
	temporal_observation::{TemporalInterpolation, TemporalInterpolationLimits},
};
use quest_numerics::{Complex64, SparseFormat, SparseLimits, SparseMatrix};
use quest_qsvt::{MatchingEncoding, NumericalPolicy, ReplayEncoding};
struct Zero(usize);
impl HistoryRowDynamics for Zero {
	fn dimension(&self) -> usize {
		self.0
	}
	fn max_row_entries(&self) -> usize {
		0
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(size_of::<Self>())
	}
	fn row_query_bytes(&self) -> usize {
		0
	}
	fn row_query_work(&self) -> usize {
		1
	}
	fn source_entry(&self, _: f64, _: usize) -> Result<Complex64, CfdError> {
		Ok(Complex64::default())
	}
	fn visit_row(
		&self,
		_: f64,
		_: usize,
		_: &mut dyn FnMut(usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		Ok(())
	}
}
fn interpolation(
	n: usize,
	order: usize,
	slab: usize,
	fraction: f64,
) -> Result<TemporalInterpolation, CfdError> {
	let source = Zero(n);
	let history =
		TemporalHistoryRecipe::new(&source, 0.1, 2, order, HistoryStreamLimits::default())?;
	TemporalInterpolation::new(
		&history,
		slab,
		fraction,
		TemporalInterpolationLimits::default(),
	)
}
#[test]
fn structured_time_map_matches_independent_completed_unitary_and_complex_interference()
-> Result<(), Box<dyn std::error::Error>> {
	let policy = NumericalPolicy::default();
	let interpolation = interpolation(3, 2, 1, 0.25)?;
	let encoding = TemporalEncoding::new(&interpolation, TemporalEncodingLimits::default())?;
	let weights = [0.375, 0.75, -0.125];
	let entries = (0..3)
		.flat_map(|i| {
			weights
				.into_iter()
				.enumerate()
				.map(move |(a, w)| (i, 9 + a * 3 + i, Complex64::new(w, 0.)))
		})
		.collect();
	let matrix =
		SparseMatrix::from_triplets(3, 18, SparseFormat::Csr, entries, SparseLimits::default())?;
	let reference = MatchingEncoding::from_sparse(&matrix, policy)?;
	let actual = quest_qsvt::materialize_oracle(&encoding.replay_oracle(policy)?, policy)?;
	let expected = quest_qsvt::materialize_oracle(&reference.to_oracle(policy)?, policy)?;
	assert_eq!(actual.shape(), expected.shape());
	for i in 0..actual.nrows() {
		for j in 0..actual.ncols() {
			assert!((actual[(i, j)] - expected[(i, j)]).norm() < 2e-12);
		}
	}
	whole_unitary::whole_register(&encoding)?;
	let descriptor = encoding.descriptor()?;
	let left = descriptor
		.left
		.logical_space::<quest_qsvt::Left>(descriptor.layout.num_qubits, policy)?;
	let right = descriptor
		.right
		.logical_space::<quest_qsvt::Right>(descriptor.layout.num_qubits, policy)?;
	for row in 0..3 {
		for col in 0..18 {
			let value = if col >= 9 && col % 3 == row {
				Complex64::new(weights[(col - 9) / 3], 0.)
			} else {
				Complex64::default()
			};
			assert!(
				(actual[(
					left.coordinate_at(row).unwrap(),
					right.coordinate_at(col).unwrap()
				)] * descriptor.normalization
					- value)
					.norm()
					< 2e-12
			);
		}
	}
	// Complex nodal amplitudes interfere before taking probabilities.
	let history = [
		Complex64::new(1., 2.),
		Complex64::new(-0.5, 0.25),
		Complex64::new(0.3, -0.7),
	];
	let coherent = weights
		.into_iter()
		.zip(history)
		.map(|(w, z)| w * z)
		.sum::<Complex64>();
	let mut projected = Complex64::default();
	for a in 0..3 {
		projected += actual[(
			left.coordinate_at(1).unwrap(),
			right.coordinate_at(10 + 3 * a).unwrap(),
		)] * history[a];
	}
	assert!((projected * descriptor.normalization - coherent).norm() < 1e-12);
	let incoherent = weights
		.into_iter()
		.zip(history)
		.map(|(w, z)| w * w * z.norm_sqr())
		.sum::<f64>();
	assert!((coherent.norm_sqr() - incoherent).abs() > 0.1);
	assert_eq!(encoding.resources().record_count, 18);
	assert_eq!(descriptor.source_identity, reference.source_identity());
	Ok(())
}
#[test]
fn time_recipe_storage_is_constant_and_zero_weights_keep_dummy_failure_sectors()
-> Result<(), Box<dyn std::error::Error>> {
	let small = TemporalEncoding::new(
		&interpolation(2, 1, 0, 0.)?,
		TemporalEncodingLimits::default(),
	)?;
	let large = TemporalEncoding::new(
		&interpolation(257, 2, 1, 0.25)?,
		TemporalEncodingLimits::default(),
	)?;
	assert_eq!(small.retained_bytes()?, large.retained_bytes()?);
	assert!(large.retained_bytes()? < 16_384);
	assert_eq!(small.slab(), 0);
	assert_eq!(
		small.side(),
		quest_cfd::probability_observation::TemporalNodeSide::SlabLeft
	);
	assert_eq!(large.slab(), 1);
	assert_eq!(
		large.side(),
		quest_cfd::probability_observation::TemporalNodeSide::Interior
	);
	let left_trace = TemporalEncoding::new(
		&interpolation(2, 1, 0, 1.)?,
		TemporalEncodingLimits::default(),
	)?;
	let right_trace = TemporalEncoding::new(
		&interpolation(2, 1, 1, 0.)?,
		TemporalEncodingLimits::default(),
	)?;
	assert_eq!(
		left_trace.physical_time().to_bits(),
		right_trace.physical_time().to_bits()
	);
	assert_eq!(
		left_trace.side(),
		quest_cfd::probability_observation::TemporalNodeSide::SlabRight
	);
	assert_eq!(
		right_trace.side(),
		quest_cfd::probability_observation::TemporalNodeSide::SlabLeft
	);
	assert_ne!(
		left_trace.descriptor()?.source_identity,
		right_trace.descriptor()?.source_identity
	);
	let descriptor = small.descriptor()?;
	let unitary = quest_qsvt::materialize_oracle(
		&small.replay_oracle(NumericalPolicy::default())?,
		NumericalPolicy::default(),
	)?;
	for i in 0..2 {
		for j in 0..8 {
			let expected = if i == j { 1. } else { 0. };
			assert!(
				(unitary[(2 * i, 2 * j)] * descriptor.normalization - Complex64::new(expected, 0.))
					.norm()
					< 1e-12
			);
		}
	}
	whole_unitary::whole_register(&small)?;
	let mut forward = 0;
	let mut reverse = 0;
	large.visit_replay(false, &mut |_| {
		forward += 1;
		Ok(())
	})?;
	large.visit_replay(true, &mut |_| {
		reverse += 1;
		Ok(())
	})?;
	assert_eq!(forward, reverse);
	assert_eq!(forward, large.resources().replay_gates);
	assert!(large.resources().preparation_work > 257);
	Ok(())
}
#[test]
fn time_encoding_rejects_before_unbounded_preparation_and_stops_failed_visitors()
-> Result<(), Box<dyn std::error::Error>> {
	let t = interpolation(3, 2, 1, 0.25)?;
	assert!(
		TemporalEncoding::new(
			&t,
			TemporalEncodingLimits {
				max_preparation_work: 1,
				..Default::default()
			}
		)
		.is_err()
	);
	let mut limits = TemporalEncodingLimits::default();
	limits.replay.max_gates = 1;
	assert!(TemporalEncoding::new(&t, limits).is_err());
	limits = TemporalEncodingLimits::default();
	limits.replay.max_bytes = 1;
	assert!(TemporalEncoding::new(&t, limits).is_err());
	let e = TemporalEncoding::new(&t, TemporalEncodingLimits::default())?;
	let mut calls = 0;
	assert!(
		e.visit_replay(false, &mut |_| {
			calls += 1;
			Err(quest_qsvt::Error::Encoding("visitor rejection"))
		})
		.is_err()
	);
	assert_eq!(calls, 1);
	Ok(())
}

#[cfg(feature = "quantum")]
#[test]
fn native_temporal_projection_replays_arbitrary_whole_register_states()
-> Result<(), Box<dyn std::error::Error>> {
	let source = TemporalEncoding::new(
		&interpolation(3, 2, 1, 0.25)?,
		TemporalEncodingLimits::default(),
	)?;
	let width = source.descriptor()?.layout.num_qubits;
	let dimension = 1usize << width;
	let targets = (2..width + 2).rev().collect::<Vec<_>>();
	let physical = |index: usize, control: usize, spectator: usize| {
		targets
			.iter()
			.enumerate()
			.fold(control | (spectator << 1), |v, (b, &t)| {
				v | (((index >> b) & 1) << t)
			})
	};
	let matrix = quest_qsvt::materialize_oracle(
		&source.replay_oracle(NumericalPolicy::default())?,
		NumericalPolicy::default(),
	)?;
	let mut initial = (0..dimension * 4)
		.map(|i| {
			let x = f64::from(u32::try_from(i).unwrap_or(0));
			Complex64::new((0.13 * x).sin(), (0.17 * x).cos())
		})
		.collect::<Vec<_>>();
	let norm = initial.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	for z in &mut initial {
		*z /= norm;
	}
	let environment = quest::Environment::builder()
		.memory_budget(quest::MemoryBudget::new(64 * 1024 * 1024))
		.build()?;
	let mut register = environment.state_vector(quest::QubitCount::new(width + 2)?)?;
	let mut executor = quest::qsvt::replay_native::ReplayGateExecutor::new(&register)?;
	for control in 0..2 {
		let mut expected = initial.clone();
		for spectator in 0..2 {
			for row in 0..dimension {
				expected[physical(row, control, spectator)] = (0..dimension)
					.map(|column| {
						matrix[(row, column)] * initial[physical(column, control, spectator)]
					})
					.sum();
			}
		}
		register.init_pure(&initial)?;
		let allocated = environment.allocated_bytes();
		source.visit_mapped_replay(&targets, 1, control, false, &mut |g| {
			executor
				.apply(&mut register, g)
				.map_err(|_| quest_qsvt::Error::Encoding("native temporal gate"))
		})?;
		for (a, b) in register.amplitudes(0, dimension * 4)?.iter().zip(&expected) {
			assert!((*a - *b).norm() < 2e-12);
		}
		source.visit_mapped_replay(&targets, 1, control, true, &mut |g| {
			executor
				.apply(&mut register, g)
				.map_err(|_| quest_qsvt::Error::Encoding("native temporal adjoint gate"))
		})?;
		for (a, b) in register.amplitudes(0, dimension * 4)?.iter().zip(&initial) {
			assert!((*a - *b).norm() < 2e-12);
		}
		assert_eq!(allocated, environment.allocated_bytes());
	}
	Ok(())
}

#[cfg(feature = "distributed")]
#[test]
#[allow(
	clippy::too_many_lines,
	reason = "Keep parent timeout and whole-register MPI/adjoint assertions together for each communicator"
)]
fn mpi_temporal_projection_preserves_whole_register_and_split_communicators()
-> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_TEMPORAL_ENCODING_CHILD").is_none() {
		for (ranks, split) in [(1, false), (2, false), (4, false), (8, false), (4, true)] {
			let status = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(60),
			)?
			.args([
				"--exact",
				"mpi_temporal_projection_preserves_whole_register_and_split_communicators",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_TEMPORAL_ENCODING_CHILD", "1")
			.env(
				"QUEST_TEMPORAL_ENCODING_SPLIT",
				if split { "1" } else { "0" },
			)
			.status()?;
			assert!(
				status.success(),
				"temporal replay ranks={ranks} split={split}"
			);
		}
		return Ok(());
	}
	let runtime = quest::collective::MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_TEMPORAL_ENCODING_SPLIT").as_deref() == Ok("1") {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let environment = quest::collective::CollectiveEnvironment::builder(&comm)?
		.memory_budget(quest::MemoryBudget::new(64 * 1024 * 1024))
		.build()?;
	let source = TemporalEncoding::new(
		&interpolation(3, 2, 1, 0.25)?,
		TemporalEncodingLimits::default(),
	)?;
	let width = source.descriptor()?.layout.num_qubits;
	let dimension = 1usize << width;
	let targets = (2..width + 2).rev().collect::<Vec<_>>();
	let index_of = |physical: usize| {
		targets
			.iter()
			.enumerate()
			.fold(0, |v, (b, &t)| v | (((physical >> t) & 1) << b))
	};
	let physical_of = |index: usize, low: usize| {
		targets
			.iter()
			.enumerate()
			.fold(low, |v, (b, &t)| v | (((index >> b) & 1) << t))
	};
	let initial = |i: usize| {
		let x = f64::from(u32::try_from(i).unwrap_or(0));
		Complex64::new((0.13 * x).sin(), (0.17 * x).cos())
	};
	let norm = (0..dimension * 4)
		.map(|i| initial(i).norm_sqr())
		.sum::<f64>()
		.sqrt();
	// Only this tiny independent reference materializes U; production replay owns its arithmetic recipe.
	let matrix = quest_qsvt::materialize_oracle(
		&source.replay_oracle(NumericalPolicy::default())?,
		NumericalPolicy::default(),
	)?;
	let mut register = environment.state_vector_local(quest::QubitCount::new(width + 2)?)?;
	let mut executor = quest::qsvt::replay_native::CollectiveReplayGateExecutor::new(&register)?;
	let local = register.deployment().local_amplitudes();
	let start = register.deployment().rank() * local;
	let values = (start..start + local)
		.map(|i| initial(i) / norm)
		.collect::<Vec<_>>();
	for control in 0..2 {
		register.write_local_amplitudes(0, &values)?;
		let allocated = environment.view().allocated_bytes();
		source.visit_mapped_replay(&targets, 1, control, false, &mut |g| {
			executor
				.apply(&mut register, g)
				.map_err(|_| quest_qsvt::Error::Encoding("MPI temporal gate"))
		})?;
		for (i, actual) in register.read_local_amplitudes(0, local)?.iter().enumerate() {
			let physical = start + i;
			let expected = if physical & 1 == control {
				(0..dimension)
					.map(|column| {
						matrix[(index_of(physical), column)]
							* initial(physical_of(column, physical & 3))
							/ norm
					})
					.sum()
			} else {
				initial(physical) / norm
			};
			assert!((*actual - expected).norm() < 2e-12);
		}
		source.visit_mapped_replay(&targets, 1, control, true, &mut |g| {
			executor
				.apply(&mut register, g)
				.map_err(|_| quest_qsvt::Error::Encoding("MPI temporal adjoint gate"))
		})?;
		for (a, b) in register
			.read_local_amplitudes(0, local)?
			.iter()
			.zip(&values)
		{
			assert!((*a - *b).norm() < 2e-12);
		}
		assert_eq!(allocated, environment.view().allocated_bytes());
	}
	Ok(())
}
