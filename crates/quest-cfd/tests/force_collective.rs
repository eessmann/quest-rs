#![cfg(feature = "distributed")]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::cast_precision_loss,
	clippy::too_many_lines,
	clippy::panic_in_result_fn,
	reason = "Bounded independent full force/chart/pressure MPI regressions"
)]
use quest::{
	collective::{CollectiveEnvironment, MpiRuntime},
	distributed_constraints::{ConstraintLimits, RankPolicy},
};
use quest_cfd::{
	physical_space::{
		BoxConstraintOutcome, BoxConstraintRecipe, BoxForceRecipe, ConstraintRecipeLimits,
		DistributedForceLimits, DistributedPressureLimits, ForceRecipeLimits, PhysicalSpace,
	},
	simplex::BoxBoundary,
};
fn sum(env: &CollectiveEnvironment<'_, '_>, value: f64) -> Result<f64, Box<dyn std::error::Error>> {
	let mut lane = env.communicator().collective_lane()?;
	let mut result = 0.;
	for peer in 0..env.size()? {
		let mut bytes = value.to_le_bytes();
		lane.broadcast_bytes(peer, &mut bytes)?;
		result += f64::from_le_bytes(bytes);
	}
	Ok(result)
}
#[test]
fn generated_force_collective_matches_full_dense_state() -> Result<(), Box<dyn std::error::Error>> {
	if std::env::var_os("QUEST_FORCE_CHILD").is_none() {
		for (ranks, split) in [(1, 0), (2, 0), (4, 0), (8, 0), (4, 1)] {
			let status = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(180),
			)?
			.args([
				"--exact",
				"generated_force_collective_matches_full_dense_state",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_FORCE_CHILD", "1")
			.env("QUEST_FORCE_SPLIT", split.to_string())
			.status()?;
			assert!(status.success(), "force ranks={ranks} split={split}");
		}
		return Ok(());
	}
	let rt = MpiRuntime::initialize()?;
	let mut world = rt.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_FORCE_SPLIT").as_deref() == Ok("1") {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let env = CollectiveEnvironment::builder(&comm)?.build()?;
	for d in [2, 3] {
		for p in [1, 2] {
			for boundary in [
				BoxBoundary::Periodic,
				BoxBoundary::Cavity { lid_speed: 0.7 },
			] {
				let subdivisions = if d == 2 && p == 1 { 2 } else { 1 };
				let geometry = BoxConstraintRecipe::new(
					d,
					subdivisions,
					1.7,
					boundary,
					p,
					ConstraintRecipeLimits::default(),
				)?;
				let force_source = BoxForceRecipe::new(
					&geometry,
					0.023,
					if boundary == BoxBoundary::Periodic {
						0.
					} else {
						0.7
					},
					ForceRecipeLimits::default(),
				)?;
				let before = env.view().allocated_bytes();
				let BoxConstraintOutcome::Prepared(prepared) = geometry.prepare_collective(
					&env,
					ConstraintLimits::default(),
					RankPolicy::default(),
				)?
				else {
					return Err("ambiguous complete box chart".into());
				};
				let chart = prepared.chart();
				let rows = chart.local_row_range();
				// Global objects below are bounded independent test references only.
				let dense = PhysicalSpace::box_mesh(d, subdivisions, 1.7, 0.023, boundary, p)?;
				let state = (0..dense.dimension())
					.map(|i| {
						0.02 * f64::from(
							u32::try_from(i + 1).expect("bounded reference coordinate"),
						)
						.sin()
					})
					.collect::<Vec<_>>();
				let velocity = dense.coefficients(&state)?;
				let expected_force = dense.momentum_force(&velocity)?;
				let local_state = chart.lower_velocity(&velocity[rows.clone()])?;
				let retained = env.view().allocated_bytes();
				let owner =
					prepared.prepare_force(&force_source, DistributedForceLimits::default())?;
				let force_live = env.view().allocated_bytes();
				assert!(force_live > retained);
				assert_eq!(owner.dimension(), chart.nullity());
				assert_eq!(owner.local_coordinate_range(), chart.local_null_range());
				let admitted = owner.admit_drift()?;
				assert_eq!(
					admitted.two_native_query_work,
					2 * chart.resources().query_work
				);
				for coordinate in chart.local_null_range() {
					assert_eq!(owner.coordinate_owner(coordinate)?, env.rank()?);
				}
				assert!(owner.coordinate_owner(chart.nullity()).is_err());
				let result = owner.drift(local_state.as_slice())?;
				let energy = sum(
					&env,
					0.5 * local_state.as_slice().iter().map(|v| v * v).sum::<f64>(),
				)?;
				assert!((energy - dense.energy(&state)?).abs() < 1e-11);
				let work = sum(
					&env,
					local_state
						.as_slice()
						.iter()
						.zip(result.as_slice())
						.map(|(a, b)| a * b)
						.sum::<f64>(),
				)?;
				let dense_work = state
					.iter()
					.zip(dense.drift(&state)?)
					.map(|(a, b)| a * b)
					.sum::<f64>();
				assert!((work - dense_work).abs() < 2e-9);
				assert_eq!(result.force_range(), rows);
				assert_eq!(result.global_range(), chart.local_null_range());
				for (&actual, &expected) in result
					.local_force()
					.iter()
					.zip(&expected_force[rows.clone()])
				{
					assert!(
						(actual - expected).abs() < 5e-8,
						"d={d},p={p},{boundary:?}: force {actual} != {expected}"
					);
				}
				let acceleration = chart.lift_null(result.as_slice())?;
				let expected_acceleration = dense.coefficients(&dense.drift(&state)?)?;
				for (&actual, &expected) in acceleration
					.as_slice()
					.iter()
					.zip(&expected_acceleration[rows])
				{
					assert!((actual - expected).abs() < 2e-7);
				}
				drop(acceleration);
				let pressure = prepared
					.recover_pressure(result.local_force(), DistributedPressureLimits::default())?;
				let reference = dense.reconstruct_pressure(&state)?;
				let reference_pressure = reference
					.pressure_coefficients
					.into_iter()
					.flatten()
					.collect::<Vec<_>>();
				for (index, &actual) in pressure
					.pressure_range()
					.zip(pressure.pressure_coefficients())
				{
					assert!((actual - reference_pressure[index]).abs() < 3e-7);
				}
				assert!(pressure.relative_momentum_residual < 1e-8);
				drop(pressure);
				let resources = result.resources;
				assert_eq!(
					resources.two_native_query_work,
					2 * chart.resources().query_work
				);
				assert_eq!(
					resources.total_work,
					resources.two_native_query_work + resources.source_work
				);
				let repeated = owner.drift(local_state.as_slice())?;
				assert_eq!(repeated.local_force(), result.local_force());
				assert_eq!(repeated.as_slice(), result.as_slice());
				drop(repeated);
				drop(result);
				assert_eq!(env.view().allocated_bytes(), force_live);
				assert!(
					owner
						.drift(if env.rank()? == 0 {
							&[f64::NAN]
						} else {
							local_state.as_slice()
						})
						.is_err()
				);
				assert_eq!(env.view().allocated_bytes(), force_live);
				let invalid_shape = vec![0.; local_state.as_slice().len() + 1];
				let bad_shape = if env.rank()? == 0 {
					&invalid_shape[..]
				} else {
					local_state.as_slice()
				};
				assert!(owner.drift(bad_shape).is_err());
				let huge = vec![1e300; local_state.as_slice().len()];
				if chart.nullity() > 0 {
					assert!(owner.drift(&huge).is_err());
				}
				assert_eq!(env.view().allocated_bytes(), force_live);
				drop(owner);
				assert_eq!(env.view().allocated_bytes(), retained);
				for limits in [
					DistributedForceLimits {
						max_work: resources.total_work - 1,
						..DistributedForceLimits::default()
					},
					DistributedForceLimits {
						max_transport_bytes: resources.total_transport_bytes - 1,
						..DistributedForceLimits::default()
					},
					DistributedForceLimits {
						max_local_bytes: 1,
						..DistributedForceLimits::default()
					},
					DistributedForceLimits {
						max_local_bytes: resources.maximum_rank_peak_bytes - 1,
						..DistributedForceLimits::default()
					},
					DistributedForceLimits {
						max_node_bytes: resources.node_peak_bytes - 1,
						..DistributedForceLimits::default()
					},
					DistributedForceLimits {
						max_transport_bytes: 0,
						..DistributedForceLimits::default()
					},
					DistributedForceLimits {
						ranks_per_node: 0,
						..DistributedForceLimits::default()
					},
					DistributedForceLimits {
						max_node_bytes: 1,
						..DistributedForceLimits::default()
					},
					DistributedForceLimits {
						max_prepare_work: 1,
						..DistributedForceLimits::default()
					},
				] {
					if let Ok(owner) = prepared.prepare_force(&force_source, limits) {
						assert!(owner.drift(local_state.as_slice()).is_err());
					}
					assert_eq!(env.view().allocated_bytes(), retained);
				}
				let other = BoxConstraintRecipe::new(
					d,
					subdivisions,
					1.7,
					boundary,
					p,
					ConstraintRecipeLimits::default(),
				)?;
				let other_force = BoxForceRecipe::new(
					&other,
					0.023,
					if boundary == BoxBoundary::Periodic {
						0.
					} else {
						0.7
					},
					ForceRecipeLimits::default(),
				)?;
				assert!(
					prepared
						.prepare_force(&other_force, DistributedForceLimits::default())
						.is_err()
				);
				assert_eq!(env.view().allocated_bytes(), retained);
				if env.size()? > 1 {
					let mismatched = DistributedForceLimits {
						max_work: if env.rank()? == 0 {
							999_999_999
						} else {
							1_000_000_000
						},
						..DistributedForceLimits::default()
					};
					assert!(prepared.prepare_force(&force_source, mismatched).is_err());
					let mismatch = BoxForceRecipe::new(
						&geometry,
						if env.rank()? == 0 { 0.024 } else { 0.023 },
						if boundary == BoxBoundary::Periodic {
							0.
						} else {
							0.7
						},
						ForceRecipeLimits::default(),
					)?;
					assert!(
						prepared
							.prepare_force(&mismatch, DistributedForceLimits::default())
							.is_err()
					);
					assert_eq!(env.view().allocated_bytes(), retained);
				}
				drop(local_state);
				drop(prepared);
				assert_eq!(env.view().allocated_bytes(), before);
			}
		}
	}
	Ok(())
}
