#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	clippy::panic_in_result_fn,
	reason = "Bounded complete-coordinate independent physical comparisons use explicit test assertions"
)]
use quest_cfd::{
	CfdError,
	configuration::ConfigurationGrid,
	kvn_recipe::KvnRecipeLimits,
	physical_observation::{
		PhysicalObservableKind, PhysicalObservableLimits, PreparedPhysicalObservable,
	},
	physical_space::PhysicalSpace,
	simplex::{BoxBoundary, SimplexBdm},
};
#[test]
fn prepared_enstrophy_and_probes_keep_every_bdm_coordinate() -> Result<(), CfdError> {
	for dimension in [2, 3] {
		for order in [1, 2] {
			let model =
				PhysicalSpace::box_mesh(dimension, 1, 1.7, 0.03, BoxBoundary::Periodic, order)?;
			let point = [0.37, 0.61, if dimension == 3 { 0.29 } else { 0. }];
			let energy = PreparedPhysicalObservable::box_space(
				&model,
				PhysicalObservableKind::Enstrophy,
				PhysicalObservableLimits {
					max_prepare_work: 10_000_000_000_000,
					..Default::default()
				},
			)?;
			let probe = PreparedPhysicalObservable::box_space(
				&model,
				PhysicalObservableKind::VelocityComponent {
					point,
					component: 0,
				},
				PhysicalObservableLimits {
					max_prepare_work: 10_000_000_000_000,
					..Default::default()
				},
			)?;
			assert_eq!(energy.dimension(), model.dimension());
			assert_eq!(probe.dimension(), model.dimension());
			for sign in [-1., 1.] {
				let state = (0..model.dimension())
					.map(|i| {
						Ok(sign
							* 0.13
							* f64::from(
								u32::try_from(i + 3)
									.map_err(|_| CfdError::InvalidInput("test index"))?,
							)
							.cos())
					})
					.collect::<Result<Vec<_>, CfdError>>()?;
				let expected = model.enstrophy(&state)?;
				assert!((energy.value(&state)? - expected).abs() < 1e-9 * (1. + expected));
				assert!(
					(probe.value(&state)? - model.sample_velocity(&state, point)?[0]).abs() < 1e-10
				);
			}
			assert!(energy.value(&vec![0.; model.dimension() - 1]).is_err());
		}
	}
	Ok(())
}
#[test]
fn full_configuration_recipe_matches_physical_field_and_rejects_missing_coordinates()
-> Result<(), CfdError> {
	let model = PhysicalSpace::box_mesh(2, 1, 1., 0.02, BoxBoundary::Periodic, 1)?;
	let prepared = PreparedPhysicalObservable::box_space(
		&model,
		PhysicalObservableKind::Enstrophy,
		PhysicalObservableLimits {
			max_prepare_work: 10_000_000_000_000,
			..Default::default()
		},
	)?;
	let grid = ConfigurationGrid::uniform(model.dimension(), -0.2, 0.2, 1, 2, 1024)?;
	let recipe = prepared.configuration(&grid, KvnRecipeLimits::default())?;
	for index in 0..grid.dimension() {
		let state = grid
			.point(index)
			.ok_or(CfdError::InvalidInput("test grid index"))?;
		let value = recipe.value(index)?;
		assert!((value - model.enstrophy(&state)?).abs() < 1e-10);
		assert!(recipe.range().contains(value));
	}
	assert!(recipe.value(grid.dimension()).is_err());
	let smaller = ConfigurationGrid::uniform(model.dimension() - 1, -0.2, 0.2, 1, 1, 1024)?;
	assert!(
		prepared
			.configuration(&smaller, KvnRecipeLimits::default())
			.is_err()
	);
	assert!(
		PreparedPhysicalObservable::box_space(
			&model,
			PhysicalObservableKind::Enstrophy,
			PhysicalObservableLimits {
				max_reference_evaluations: 0,
				..Default::default()
			}
		)
		.is_err()
	);
	assert!(
		PreparedPhysicalObservable::box_space(
			&model,
			PhysicalObservableKind::VelocityComponent {
				point: [f64::NAN, 0., 0.],
				component: 0
			},
			PhysicalObservableLimits {
				max_prepare_work: 10_000_000_000_000,
				..Default::default()
			}
		)
		.is_err()
	);
	assert!(
		PreparedPhysicalObservable::box_space(
			&model,
			PhysicalObservableKind::VelocityComponent {
				point: [0.3, 0.2, 0.],
				component: 2
			},
			PhysicalObservableLimits {
				max_prepare_work: 10_000_000_000_000,
				..Default::default()
			}
		)
		.is_err()
	);
	Ok(())
}
#[test]
fn pressure_difference_and_solid_force_preserve_original_gauge_and_sign() -> Result<(), CfdError> {
	let case = quest_cfd::cylinder::reference("shedding2d", 100, 4, 1, 1)?;
	let model = &case.model;
	let first = [0.15, 0.2, 0.];
	let second = [0.25, 0.2, 0.];
	let pressure = PreparedPhysicalObservable::simplex(
		model,
		PhysicalObservableKind::PressureDifference { first, second },
		PhysicalObservableLimits {
			max_prepare_work: 10_000_000_000_000,
			..Default::default()
		},
	)?;
	let force = PreparedPhysicalObservable::simplex(
		model,
		PhysicalObservableKind::BoundaryForceComponent {
			label: "cylinder".into(),
			component: 0,
		},
		PhysicalObservableLimits {
			max_prepare_work: 10_000_000_000_000,
			..Default::default()
		},
	)?;
	let state = (0..model.dimension())
		.map(|i| {
			Ok(0.03
				* f64::from(
					u32::try_from(i + 1).map_err(|_| CfdError::InvalidInput("test index"))?,
				)
				.sin())
		})
		.collect::<Result<Vec<_>, CfdError>>()?;
	let p = model.reconstruct_pressure(&state)?;
	let probes = model.sample_pressures(&p.cell_pressure, &[first, second])?;
	assert!((pressure.value(&state)? - (probes[0] - probes[1])).abs() < 1e-8);
	assert!(
		(force.value(&state)? - model.boundary_force(&state, &p.cell_pressure, "cylinder")?[0])
			.abs()
			< 1e-8
	);
	let periodic = SimplexBdm::box_mesh(2, 1, 1., 0.01, BoxBoundary::Periodic)?;
	assert!(
		PreparedPhysicalObservable::simplex(
			&periodic,
			PhysicalObservableKind::BoundaryForceComponent {
				label: "missing".into(),
				component: 0
			},
			PhysicalObservableLimits {
				max_prepare_work: 10_000_000_000_000,
				..Default::default()
			}
		)
		.is_err()
	);
	Ok(())
}
#[test]
#[allow(
	clippy::too_many_lines,
	reason = "One small reference supports independent field, storage and work rejection assertions"
)]
fn resource_rejections_and_constant_field_reference_are_explicit() -> Result<(), CfdError> {
	let model = PhysicalSpace::box_mesh(2, 1, 1., 0.02, BoxBoundary::Periodic, 1)?;
	for limits in [
		PhysicalObservableLimits {
			max_coordinates: 0,
			..Default::default()
		},
		PhysicalObservableLimits {
			max_prepare_bytes: 0,
			..Default::default()
		},
		PhysicalObservableLimits {
			max_prepare_work: 0,
			..Default::default()
		},
		PhysicalObservableLimits {
			max_query_work: 0,
			..Default::default()
		},
		PhysicalObservableLimits {
			validation_tolerance: f64::NAN,
			..Default::default()
		},
	] {
		assert!(
			PreparedPhysicalObservable::box_space(
				&model,
				PhysicalObservableKind::Enstrophy,
				limits
			)
			.is_err()
		);
	}
	let probe = PreparedPhysicalObservable::box_space(
		&model,
		PhysicalObservableKind::VelocityComponent {
			point: [0.3, 0.2, 0.],
			component: 1,
		},
		PhysicalObservableLimits::default(),
	)?;
	let state = model.project_velocity(|_| [0.2, -0.4, 0.])?;
	assert!((probe.value(&state)? + 0.4).abs() < 1e-12);
	let enstrophy = PreparedPhysicalObservable::box_space(
		&model,
		PhysicalObservableKind::Enstrophy,
		PhysicalObservableLimits::default(),
	)?;
	assert!(enstrophy.value(&state)?.abs() < 1e-12);
	let resources = enstrophy.resources();
	assert!(resources.retained_bytes > size_of::<PreparedPhysicalObservable>());
	assert!(resources.prepare_peak_bytes > resources.retained_bytes);
	assert!(resources.reference_evaluations > model.dimension());
	assert!(enstrophy.value(&vec![f64::NAN; model.dimension()]).is_err());
	let grid = ConfigurationGrid::uniform(model.dimension(), -0.2, 0.2, 1, 1, 1024)?;
	for limits in [
		KvnRecipeLimits {
			max_dimension: 0,
			..Default::default()
		},
		KvnRecipeLimits {
			max_bytes: 0,
			..Default::default()
		},
		KvnRecipeLimits {
			max_query_work: 0,
			..Default::default()
		},
	] {
		assert!(enstrophy.configuration(&grid, limits).is_err());
	}
	let recipe = enstrophy.configuration(&grid, KvnRecipeLimits::default())?;
	assert!(recipe.retained_bytes() > resources.retained_bytes);
	assert!(
		enstrophy
			.configuration(
				&grid,
				KvnRecipeLimits {
					max_bytes: recipe.retained_bytes() + recipe.query_bytes() - 1,
					..Default::default()
				}
			)
			.is_err()
	);
	assert!(
		enstrophy
			.configuration(
				&grid,
				KvnRecipeLimits {
					max_query_work: recipe.query_work() - 1,
					..Default::default()
				}
			)
			.is_err()
	);
	let mut label = String::with_capacity(2_000_000);
	label.push_str("missing");
	assert!(
		PreparedPhysicalObservable::simplex(
			&SimplexBdm::box_mesh(2, 1, 1., 0.01, BoxBoundary::Periodic)?,
			PhysicalObservableKind::BoundaryForceComponent {
				label,
				component: 0
			},
			PhysicalObservableLimits {
				max_prepare_bytes: 1_000_000,
				..Default::default()
			}
		)
		.is_err()
	);
	Ok(())
}
