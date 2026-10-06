#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	clippy::suboptimal_flops,
	reason = "Bounded independent full-pressure and traction comparisons use explicit physical formulas and assertions"
)]
use quest_cfd::{
	CfdError,
	physical_observation::{
		PhysicalObservableKind, PhysicalObservableLimits, PreparedPhysicalObservable,
	},
	physical_space::{
		BoundaryLimits, BoundaryTimeCoefficient, BoxBoundarySide, MechanicalTractionLimits,
		PhysicalSpace, PolynomialBoundary,
	},
	simplex::{BoxBoundary, PressureProbeLimits},
};

#[test]
fn complete_box_pressure_capture_is_supported() -> Result<(), CfdError> {
	let model = PhysicalSpace::box_mesh(2, 1, 1., 0.02, BoxBoundary::Cavity { lid_speed: 0. }, 2)?;
	let prepared = PreparedPhysicalObservable::box_space(
		&model,
		PhysicalObservableKind::PressureDifference {
			first: [0.2, 0.1, 0.],
			second: [0.8, 0.9, 0.],
		},
		PhysicalObservableLimits::default(),
	)?;
	assert_eq!(prepared.dimension(), model.dimension());
	assert!(prepared.value(&vec![0.; model.dimension()])?.abs() < 1e-12);
	Ok(())
}

#[test]
fn query_and_fixed_time_capture_failures_are_explicit() -> Result<(), CfdError> {
	let space = PhysicalSpace::box_mesh(2, 1, 1., 0.03, BoxBoundary::Cavity { lid_speed: 0. }, 2)?;
	let problem = manufactured(&space, 0.7)?;
	let state = vec![0.; space.dimension()];
	let pressure = vec![vec![0.; 3]; space.cell_count()];
	let kind = PhysicalObservableKind::PressureDifference {
		first: [0.2, 0.3, 0.],
		second: [0.8, 0.7, 0.],
	};
	for time in [f64::NAN, f64::INFINITY] {
		assert!(
			PreparedPhysicalObservable::polynomial_boundary(
				&problem,
				time,
				kind.clone(),
				PhysicalObservableLimits::default()
			)
			.is_err()
		);
		assert!(
			problem
				.boundary_force(time, &state, &pressure, BoxBoundarySide::XMax)
				.is_err()
		);
	}
	for limits in [
		PhysicalObservableLimits {
			max_bytes: 0,
			..Default::default()
		},
		PhysicalObservableLimits {
			max_prepare_bytes: problem.resources().borrowed_space_bytes,
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
			max_coordinates: space.dimension() - 1,
			..Default::default()
		},
	] {
		assert!(
			PreparedPhysicalObservable::polynomial_boundary(&problem, 0.3, kind.clone(), limits)
				.is_err()
		);
	}
	Ok(())
}

#[test]
fn pressure_traction_shapes_sides_and_full_3d_capacity_are_checked() -> Result<(), CfdError> {
	let space = PhysicalSpace::box_mesh(2, 1, 1., 0.03, BoxBoundary::Cavity { lid_speed: 0. }, 2)?;
	let problem = manufactured(&space, 0.7)?;
	let state = vec![0.; space.dimension()];
	let pressure = vec![vec![0.; 3]; space.cell_count()];
	let kind = PhysicalObservableKind::PressureDifference {
		first: [0.2, 0.3, 0.],
		second: [0.8, 0.7, 0.],
	};
	for bad_pressure in [
		vec![],
		vec![vec![0.]; space.cell_count()],
		vec![vec![f64::NAN; 3]; space.cell_count()],
	] {
		assert!(space.sample_pressures(&bad_pressure, &[[0.; 3]]).is_err());
		assert!(
			space
				.boundary_force(&state, &bad_pressure, BoxBoundarySide::XMax)
				.is_err()
		);
		assert!(
			problem
				.boundary_force(0.3, &state, &bad_pressure, BoxBoundarySide::XMax)
				.is_err()
		);
	}
	assert!(
		space
			.boundary_force(&state, &pressure, BoxBoundarySide::ZMax)
			.is_err()
	);
	assert!(
		space
			.sample_pressures(&pressure, &[[f64::NAN, 0., 0.]])
			.is_err()
	);
	assert!(
		space
			.boundary_force_with_limits(
				&state,
				&pressure,
				BoxBoundarySide::XMax,
				MechanicalTractionLimits {
					max_work: 0,
					..Default::default()
				}
			)
			.is_err()
	);
	assert!(
		problem
			.boundary_force_with_limits(
				0.3,
				&state,
				&pressure,
				BoxBoundarySide::XMax,
				MechanicalTractionLimits {
					max_bytes: 0,
					..Default::default()
				}
			)
			.is_err()
	);
	assert!(BoxBoundarySide::from_label("wall").is_err());
	let periodic = PhysicalSpace::box_mesh(2, 1, 1., 0.03, BoxBoundary::Periodic, 2)?;
	let periodic_p = vec![vec![0.; 3]; periodic.cell_count()];
	assert!(
		periodic
			.boundary_force(
				&vec![0.; periodic.dimension()],
				&periodic_p,
				BoxBoundarySide::XMax
			)
			.is_err()
	);
	// A complete 3D P2 owner remains 49-dimensional even when default cubic capture
	// admission rejects its thousands of pressure solves. No reduced fallback.
	let full = PhysicalSpace::box_mesh(3, 1, 1., 0.03, BoxBoundary::Cavity { lid_speed: 0. }, 2)?;
	assert_eq!(full.dimension(), 49);
	let mut complete_state = vec![0.; full.dimension()];
	complete_state[48] = 0.04;
	let complete_pressure = full.reconstruct_pressure(&complete_state)?;
	assert!(complete_pressure.momentum_residual < 1e-9);
	assert!(complete_pressure.gauge_residual < 1e-10);
	assert!(
		full.sample_pressures(&complete_pressure.pressure_coefficients, &[[0.2, 0.3, 0.4]])?[0]
			.is_finite()
	);
	assert!(
		full.boundary_force(
			&complete_state,
			&complete_pressure.pressure_coefficients,
			BoxBoundarySide::XMax
		)?
		.iter()
		.all(|x| x.is_finite())
	);
	assert!(
		PreparedPhysicalObservable::box_space(&full, kind, PhysicalObservableLimits::default())
			.is_err()
	);
	Ok(())
}

#[test]
#[allow(
	clippy::too_many_lines,
	reason = "Independent interpolation, oriented face integrals and limits share each complete P1/P2 fixture"
)]
fn p0_p1_incident_pressure_and_physical_side_force_are_exact() -> Result<(), CfdError> {
	for dimension in [2, 3] {
		for order in [1, 2] {
			let extent = 1.7;
			let space = PhysicalSpace::box_mesh(
				dimension,
				1,
				extent,
				0.03,
				BoxBoundary::Cavity { lid_speed: 0. },
				order,
			)?;
			let nodes = space.velocity_nodes()?;
			let pressure = if order == 1 {
				(0..space.cell_count())
					.map(|i| vec![3. + f64::from(u32::try_from(i).unwrap())])
					.collect::<Vec<_>>()
			} else {
				nodes
					.iter()
					.map(|cell| {
						cell[..=dimension]
							.iter()
							.map(|p| 3. + p[0] + 2. * p[1] + 3. * p[2])
							.collect()
					})
					.collect()
			};
			let expected = if order == 1 {
				3. + 0.5 * f64::from(u32::try_from(space.cell_count() - 1).unwrap())
			} else {
				3.
			};
			assert!((space.sample_pressures(&pressure, &[[0.; 3]])?[0] - expected).abs() < 1e-12);
			let state = vec![0.; space.dimension()];
			let constant = vec![vec![2.; space.pressure_modes_per_cell()]; space.cell_count()];
			let area = if dimension == 2 {
				extent
			} else {
				extent * extent
			};
			for (axis, lower, upper) in [
				(0, BoxBoundarySide::XMin, BoxBoundarySide::XMax),
				(1, BoxBoundarySide::YMin, BoxBoundarySide::YMax),
				(2, BoxBoundarySide::ZMin, BoxBoundarySide::ZMax),
			]
			.into_iter()
			.take(dimension)
			{
				let a = space.boundary_force(&state, &constant, lower)?;
				let b = space.boundary_force(&state, &constant, upper)?;
				for component in 0..3 {
					assert!(
						(a[component] - if component == axis { -2. * area } else { 0. }).abs()
							< 2e-12
					);
					assert!(
						(b[component] - if component == axis { 2. * area } else { 0. }).abs()
							< 2e-12
					);
				}
				assert_eq!(BoxBoundarySide::from_label(upper.label())?, upper);
			}
			if order == 2 {
				let point = [0.31, 0.52, if dimension == 3 { 0.17 } else { 0. }];
				assert!(
					(space.sample_pressures(&pressure, &[point])?[0]
						- (3. + point[0] + 2. * point[1] + 3. * point[2]))
						.abs()
						< 1e-12
				);
				let force = space.boundary_force(&state, &pressure, BoxBoundarySide::XMax)?;
				let analytic =
					area * (3. + extent + extent + if dimension == 3 { 1.5 * extent } else { 0. });
				assert!((force[0] - analytic).abs() < 1e-11);
			}
			assert!(
				space
					.sample_pressures(&constant, &[[2. * extent, 0., 0.]])
					.is_err()
			);
			assert!(
				space
					.sample_pressures_with_limits(
						&constant,
						&[[0.; 3]],
						PressureProbeLimits {
							max_work: 0,
							..Default::default()
						}
					)
					.is_err()
			);
			assert!(
				space
					.boundary_force_with_limits(
						&state,
						&constant,
						BoxBoundarySide::XMax,
						MechanicalTractionLimits {
							max_bytes: 0,
							..Default::default()
						}
					)
					.is_err()
			);
		}
	}
	Ok(())
}

fn manufactured(space: &PhysicalSpace, kappa: f64) -> Result<PolynomialBoundary<'_>, CfdError> {
	let nodes = space.velocity_nodes()?;
	let mut modes = vec![BoundaryTimeCoefficient::zero(space)?; 3];
	for (cell, points) in nodes.iter().enumerate() {
		for component in 0..space.physical_dimension() {
			for (node, p) in points.iter().enumerate() {
				let i = cell * space.local_velocity_per_cell() + component * points.len() + node;
				let w = [p[0] + 0.3, -p[1], 0.][component];
				modes[1].lifting[i] = w;
				modes[0].body_force[i] = w + if component == 0 { kappa } else { 0. };
				modes[1].body_force[i] = if component == 0 { kappa } else { 0. };
				modes[2].body_force[i] = [p[0] + 0.3, p[1], 0.][component];
			}
		}
	}
	for (trace, facet) in modes[1].prescribed.iter_mut().zip(space.boundary_facets()?) {
		for (v, p) in trace.iter_mut().zip(facet.nodes) {
			*v = [p[0] + 0.3, -p[1], 0.];
		}
	}
	PolynomialBoundary::new(space, modes, BoundaryLimits::default())
}

#[test]
fn timed_lifting_pressure_and_viscous_traction_use_original_acceleration() -> Result<(), CfdError> {
	for dimension in [2, 3] {
		for order in [1, 2] {
			let viscosity = 0.03;
			let space = PhysicalSpace::box_mesh(
				dimension,
				1,
				1.,
				viscosity,
				BoxBoundary::Cavity { lid_speed: 0. },
				order,
			)?;
			let kappa = if order == 2 { 0.7 } else { 0. };
			let problem = manufactured(&space, kappa)?;
			let state = vec![0.; space.dimension()];
			for time in [-0.2, 0., 0.3, 0.8] {
				let p = problem.reconstruct_pressure(time, &state)?;
				assert!(p.momentum_residual < 1e-9);
				assert!(p.gauge_residual < 1e-10);
				assert!(p.continuity_residual < 1e-10);
				let values = space.sample_pressures(
					&p.pressure_coefficients,
					&[
						[0.2, 0.3, if dimension == 3 { 0.4 } else { 0. }],
						[0.8, 0.7, if dimension == 3 { 0.6 } else { 0. }],
					],
				)?;
				assert!((values[0] - kappa * (1. + time) * -0.3).abs() < 1e-9);
				assert!((values[1] - kappa * (1. + time) * 0.3).abs() < 1e-9);
				let force = problem.boundary_force(
					time,
					&state,
					&p.pressure_coefficients,
					BoxBoundarySide::XMax,
				)?;
				assert!((force[0] - (0.5 * kappa * (1. + time) - viscosity * time)).abs() < 1e-9);
				assert!(force[1].abs() < 1e-10 && force[2].abs() < 1e-10);
			}
		}
	}
	Ok(())
}

#[test]
fn autonomous_and_fixed_time_captures_preserve_full_coordinates_and_provenance()
-> Result<(), CfdError> {
	for (dimension, order) in [(2, 1), (2, 2), (3, 1)] {
		let space = PhysicalSpace::box_mesh(
			dimension,
			1,
			1.,
			0.03,
			BoxBoundary::Cavity { lid_speed: 0. },
			order,
		)?;
		let problem = manufactured(&space, if order == 2 { 0.7 } else { 0. })?;
		for kind in [
			PhysicalObservableKind::PressureDifference {
				first: [0.2, 0.3, 0.],
				second: [0.8, 0.7, 0.],
			},
			PhysicalObservableKind::BoundaryForceComponent {
				label: "x-max".into(),
				component: 0,
			},
		] {
			let autonomous = PreparedPhysicalObservable::box_space(
				&space,
				kind.clone(),
				PhysicalObservableLimits::default(),
			)?;
			let prepared = PreparedPhysicalObservable::polynomial_boundary(
				&problem,
				0.3,
				kind.clone(),
				PhysicalObservableLimits::default(),
			)?;
			assert_eq!(prepared.dimension(), space.dimension());
			assert_eq!(prepared.provenance().snapshot_time, Some(0.3));
			assert_eq!(prepared.provenance().pressure_degree, order - 1);
			assert_eq!(autonomous.provenance().snapshot_time, None);
			for factor in [0., -0.2, 0.3] {
				let mut state = (0..space.dimension())
					.map(|i| factor * f64::from(u32::try_from(i + 1).unwrap()).sin())
					.collect::<Vec<_>>();
				state[space.dimension() - 1] += factor;
				let pressure = problem.reconstruct_pressure(0.3, &state)?;
				let expected = match &kind {
					PhysicalObservableKind::PressureDifference { first, second } => {
						let p = space.sample_pressures(
							&pressure.pressure_coefficients,
							&[*first, *second],
						)?;
						p[0] - p[1]
					}
					PhysicalObservableKind::BoundaryForceComponent { label, component } => problem
						.boundary_force(
							0.3,
							&state,
							&pressure.pressure_coefficients,
							BoxBoundarySide::from_label(label)?,
						)?[*component],
					_ => return Err(CfdError::InvalidInput("test observable kind")),
				};
				assert!((prepared.value(&state)? - expected).abs() < 1e-8 * (1. + expected.abs()));
			}
		}
	}
	Ok(())
}
