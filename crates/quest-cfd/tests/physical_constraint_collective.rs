#![cfg(feature = "distributed")]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::too_many_lines,
	clippy::panic_in_result_fn,
	clippy::suboptimal_flops,
	reason = "Small independent dense physical projector and MPI admission regressions"
)]
use quest::{
	collective::{CollectiveEnvironment, MpiRuntime},
	distributed_constraints::{ConstraintLimits, RankPolicy},
};
use quest_cfd::{
	physical_space::{
		BoxConstraintOutcome, BoxConstraintRecipe, ConstraintRecipeLimits, PhysicalSpace,
	},
	simplex::BoxBoundary,
};
#[test]
fn implicit_physical_chart_matches_full_dense_projection() -> Result<(), Box<dyn std::error::Error>>
{
	if std::env::var_os("QUEST_PHYSICAL_CHART_CHILD").is_none() {
		for (ranks, split) in [(1, 0), (2, 0), (4, 0), (8, 0), (4, 1)] {
			let status = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(180),
			)?
			.args([
				"--exact",
				"implicit_physical_chart_matches_full_dense_projection",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_PHYSICAL_CHART_CHILD", "1")
			.env("QUEST_PHYSICAL_CHART_SPLIT", split.to_string())
			.status()?;
			assert!(
				status.success(),
				"physical chart MPI ranks={ranks} split={split}"
			);
		}
		return Ok(());
	}
	let rt = MpiRuntime::initialize()?;
	let mut world = rt.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_PHYSICAL_CHART_SPLIT").as_deref() == Ok("1") {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let env = CollectiveEnvironment::builder(&comm)?.build()?;
	for d in [2, 3] {
		for p in [1, 2] {
			for boundary in [BoxBoundary::Periodic, BoxBoundary::Cavity { lid_speed: 1. }] {
				let recipe = BoxConstraintRecipe::new(
					d,
					1,
					1.,
					boundary,
					p,
					ConstraintRecipeLimits::default(),
				)?;
				let before = env.view().allocated_bytes();
				let prepared = recipe.prepare_collective(
					&env,
					ConstraintLimits::default(),
					RankPolicy::default(),
				)?;
				let BoxConstraintOutcome::Prepared(prepared) = prepared else {
					return Err("ambiguous physical rank".into());
				};
				let chart = prepared.chart();
				let dense = PhysicalSpace::box_mesh(d, 1, 1., 0., boundary, p)?;
				assert_eq!(chart.nullity(), dense.dimension());
				assert_eq!(
					chart.numerical_rank() + chart.nullity(),
					recipe.cell_count() * recipe.local_velocity_dimension()
				);
				assert!(
					env.view().allocated_bytes()
						>= before
							+ prepared.source_costs().live_bytes
							+ chart.resources().retained_bytes
				);
				let local = recipe.local_velocity_dimension();
				let total = local * recipe.cell_count();
				let input = (0..total)
					.map(|i| u32::try_from(i + 1).map(|j| (f64::from(j) * 0.731).sin()))
					.collect::<Result<Vec<_>, _>>()?;
				let mut mass = vec![0.; total];
				for cell in 0..recipe.cell_count() {
					let m = dense.cell_mass_matrix(cell)?;
					for i in 0..local {
						for j in 0..local {
							mass[cell * local + i] += m[i * local + j] * input[cell * local + j];
						}
					}
				}
				let coordinates = dense
					.chart()
					.iter()
					.map(|q| q.iter().zip(&mass).map(|(a, b)| a * b).sum::<f64>())
					.collect::<Vec<_>>();
				let expected = dense.coefficients(&coordinates)?;
				let owned = chart.local_row_range();
				let lower = chart.lower_velocity(&input[owned.clone()])?;
				let projected = chart.lift_null(lower.as_slice())?;
				for (actual, reference) in projected.as_slice().iter().zip(&expected[owned]) {
					assert!(
						(actual - reference).abs() < 2e-9,
						"d={d} p={p}: {actual} != {reference}"
					);
				}
				let residual = chart.constraint_values(projected.as_slice())?;
				assert!(residual.as_slice().iter().all(|x| x.abs() < 2e-9));
				drop(residual);
				drop(projected);
				drop(lower);
				drop(prepared);
				assert_eq!(env.view().allocated_bytes(), before);
			}
		}
	}
	let recipe = BoxConstraintRecipe::new(
		2,
		1,
		1.,
		BoxBoundary::Periodic,
		2,
		ConstraintRecipeLimits::default(),
	)?;
	let source = recipe.factory_source_costs(usize::try_from(env.size()?)?)?;
	let before = env.view().allocated_bytes();
	assert!(
		recipe
			.prepare_collective(
				&env,
				ConstraintLimits {
					max_construct_work: source.work_per_rank - 1,
					..ConstraintLimits::default()
				},
				RankPolicy::default()
			)
			.is_err()
	);
	assert_eq!(env.view().allocated_bytes(), before);
	// One rank differs in geometry while all dimensions/counts agree.
	if env.size()? > 1 {
		let mismatched = BoxConstraintRecipe::new(
			2,
			1,
			if env.rank()? == 0 { 1. } else { 2. },
			BoxBoundary::Periodic,
			2,
			ConstraintRecipeLimits::default(),
		)?;
		assert!(
			mismatched
				.prepare_collective(&env, ConstraintLimits::default(), RankPolicy::default())
				.is_err()
		);
		assert_eq!(env.view().allocated_bytes(), before);
	}
	// Live source storage must participate in native rank capacity admission.
	assert!(
		recipe
			.prepare_collective(
				&env,
				ConstraintLimits {
					max_local_bytes: source.live_bytes - 1,
					..ConstraintLimits::default()
				},
				RankPolicy::default()
			)
			.is_err()
	);
	assert_eq!(env.view().allocated_bytes(), before);
	assert!(
		recipe
			.prepare_collective(
				&env,
				ConstraintLimits {
					max_transport_bytes: source.coordination_bytes - 1,
					..ConstraintLimits::default()
				},
				RankPolicy::default()
			)
			.is_err()
	);
	assert_eq!(env.view().allocated_bytes(), before);
	let ambiguous = recipe.prepare_collective(
		&env,
		ConstraintLimits::default(),
		RankPolicy {
			zero_at_most: 0.,
			nonzero_at_least: 1e9,
			..RankPolicy::default()
		},
	)?;
	assert!(matches!(ambiguous, BoxConstraintOutcome::Ambiguous(_)));
	assert_eq!(env.view().allocated_bytes(), before);
	// A policy which calls every column zero cannot silently turn the full broken space
	// into the constrained space: its decided rank contradicts independent topology.
	assert!(
		recipe
			.prepare_collective(
				&env,
				ConstraintLimits::default(),
				RankPolicy {
					zero_at_most: 1e9,
					nonzero_at_least: 2e9,
					..RankPolicy::default()
				}
			)
			.is_err()
	);
	assert_eq!(env.view().allocated_bytes(), before);

	Ok(())
}
