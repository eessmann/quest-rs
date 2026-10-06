#![cfg(feature = "distributed")]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::too_many_lines,
	clippy::panic_in_result_fn,
	clippy::suboptimal_flops,
	reason = "Bounded independent dense pressure, mass and MPI admission regressions"
)]
use quest::{
	collective::{CollectiveEnvironment, MpiRuntime},
	distributed_constraints::{ConstraintLimits, RankPolicy},
};
use quest_cfd::{
	physical_space::{
		BoxConstraintOutcome, BoxConstraintRecipe, ConstraintRecipeLimits,
		DistributedPressureLimits, PhysicalSpace,
	},
	simplex::BoxBoundary,
};
#[test]
fn physical_pressure_sign_gauge_and_original_momentum() -> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_PRESSURE_CHILD").is_none() {
		for (ranks, split) in [(1, 0), (2, 0), (4, 0), (8, 0), (4, 1)] {
			let status = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(180),
			)?
			.args([
				"--exact",
				"physical_pressure_sign_gauge_and_original_momentum",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_PRESSURE_CHILD", "1")
			.env("QUEST_PRESSURE_SPLIT", split.to_string())
			.status()?;
			assert!(status.success(), "pressure MPI ranks={ranks} split={split}");
		}
		return Ok(());
	}
	let rt = MpiRuntime::initialize()?;
	let mut world = rt.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_PRESSURE_SPLIT").as_deref() == Ok("1") {
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
					1.7,
					boundary,
					p,
					ConstraintRecipeLimits::default(),
				)?;
				let before = env.view().allocated_bytes();
				let BoxConstraintOutcome::Prepared(prepared) = recipe.prepare_collective(
					&env,
					ConstraintLimits::default(),
					RankPolicy::default(),
				)?
				else {
					return Err("ambiguous physical rank".into());
				};
				let chart = prepared.chart();
				let dense = PhysicalSpace::box_mesh(d, 1, 1.7, 0., boundary, p)?;
				assert_eq!(chart.nullity(), dense.dimension());
				let count = dense.cell_count() * dense.pressure_modes_per_cell();
				let pressure = (0..count)
					.map(|i| u32::try_from(i).map(|j| 0.3 + 0.07 * f64::from(j)))
					.collect::<Result<Vec<_>, _>>()?;
				// Uniform simplex volume and P0/P1 basis integrals give equal weights.
				let mean = pressure.iter().sum::<f64>() / f64::from(u32::try_from(count)?);
				let local = recipe.local_velocity_dimension();
				let dense_pressure_start = dense.constraint_count() - count;
				let rows = chart.local_row_range();
				let mut force = vec![0.; rows.len()];
				for cell in rows.start / local..rows.end / local {
					// This reference uses independently assembled unique-face constraints,
					// quadrature mass, and the complete dense mass-orthonormal null chart.
					let ct = dense.cell_constraint_transpose(cell)?;
					let mass = dense.cell_mass_matrix(cell)?;
					for i in 0..local {
						let index = cell * local + i - rows.start;
						for (j, &value) in pressure.iter().enumerate() {
							force[index] -=
								ct[i * dense.constraint_count() + dense_pressure_start + j] * value;
						}
						if let Some(q) = dense.chart().first() {
							for j in 0..local {
								force[index] += 0.13 * mass[i * local + j] * q[cell * local + j];
							}
						}
					}
				}
				let retained = env.view().allocated_bytes();
				let recovered =
					prepared.recover_pressure(&force, DistributedPressureLimits::default())?;
				for (index, &actual) in recovered
					.pressure_range()
					.zip(recovered.pressure_coefficients())
				{
					assert!(
						(actual - (pressure[index] - mean)).abs() < 3e-8,
						"d={d} p={p} boundary={boundary:?}: pressure[{index}]={actual}, expected {}",
						pressure[index] - mean
					);
				}
				assert!(recovered.relative_momentum_residual < 1e-8);
				assert!(recovered.relative_gauge_residual < 1e-12);
				assert_eq!(
					recovered.normal_range().len(),
					recovered.normal_multipliers().len()
				);
				let receipt = recovered.resources;
				assert_eq!(
					receipt.four_native_query_work,
					4 * chart.resources().query_work
				);
				assert_eq!(
					receipt.total_work,
					receipt.four_native_query_work + receipt.source_work
				);
				assert_eq!(
					env.view().allocated_bytes(),
					retained + receipt.local_external_bytes
				);
				drop(recovered);
				assert_eq!(env.view().allocated_bytes(), retained);
				for limit in [
					DistributedPressureLimits {
						max_work: receipt.total_work - 1,
						..DistributedPressureLimits::default()
					},
					DistributedPressureLimits {
						max_transport_bytes: receipt.total_transport_bytes - 1,
						..DistributedPressureLimits::default()
					},
					DistributedPressureLimits {
						max_local_bytes: receipt.maximum_rank_peak_bytes - 1,
						..DistributedPressureLimits::default()
					},
					DistributedPressureLimits {
						max_node_bytes: receipt.node_peak_bytes - 1,
						..DistributedPressureLimits::default()
					},
				] {
					assert!(prepared.recover_pressure(&force, limit).is_err());
					assert_eq!(env.view().allocated_bytes(), retained);
				}
				// One rank supplies bad input; all ranks must leave without leaked guards.
				if env.rank()? == 0 {
					force[0] = f64::NAN;
				}
				assert!(
					prepared
						.recover_pressure(&force, DistributedPressureLimits::default())
						.is_err()
				);
				assert_eq!(env.view().allocated_bytes(), retained);
				force.fill(0.);
				let bad_shape = if env.rank()? == 0 {
					&force[1..]
				} else {
					&force[..]
				};
				assert!(
					prepared
						.recover_pressure(bad_shape, DistributedPressureLimits::default())
						.is_err()
				);
				assert_eq!(env.view().allocated_bytes(), retained);
				drop(prepared);
				assert_eq!(env.view().allocated_bytes(), before);
			}
		}
	}
	Ok(())
}
