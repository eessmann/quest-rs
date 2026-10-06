#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::unwrap_used,
	clippy::suboptimal_flops,
	clippy::too_many_lines,
	reason = "Tiny complete independent physical references"
)]
use quest_cfd::{
	physical_space::{
		BoundaryLimits, BoundaryTimeCoefficient, BoxConstraintRecipe, BoxForceRecipe,
		BoxTimeCoefficient, BoxTimeDataLimits, BoxTimeDataShard, ConstraintRecipeLimits,
		ForceRecipeLimits, PhysicalSpace, PolynomialBoundary,
	},
	simplex::BoxBoundary,
};

#[test]
fn complete_time_source_and_force_match_independent_boundary_reference() {
	for d in [2, 3] {
		for p in [1, 2] {
			let boundary = BoxBoundary::Cavity { lid_speed: 0. };
			let geo =
				BoxConstraintRecipe::new(d, 1, 1., boundary, p, ConstraintRecipeLimits::default())
					.unwrap();
			let space = PhysicalSpace::box_mesh(d, 1, 1., 0.03, boundary, p).unwrap();
			let n = geo.local_velocity_dimension();
			let scalar = n / d;
			let cells = geo.cell_count();
			let nodes = space.velocity_nodes().unwrap();
			let facets = space.boundary_facets().unwrap();
			let field = |x: [f64; 3]| [x[0], -x[1], 0.];
			let mut coefficients = vec![BoxTimeCoefficient::zero(); cells * 3];
			let mut reference = (0..3)
				.map(|_| BoundaryTimeCoefficient::zero(&space).unwrap())
				.collect::<Vec<_>>();
			for k in 0..3 {
				let scale = [0.1, -0.2, 0.3][k];
				for cell in 0..cells {
					let c = &mut coefficients[k * cells + cell];
					for axis in 0..d {
						for node in 0..scalar {
							c.lifting[axis * scalar + node] =
								scale * field(nodes[cell][node])[axis];
							c.body_force[axis * scalar + node] =
								scale * f64::from(u32::try_from(axis + 1).unwrap());
						}
					}
					for face in 0..=d {
						if geo.facet_is_exterior(cell, face).unwrap() {
							for mode in 0..geo.facet_mode_count() {
								let node = geo.facet_velocity_node(face, mode).unwrap();
								for axis in 0..d {
									c.prescribed[face][axis * scalar + node] =
										c.lifting[axis * scalar + node];
								}
							}
						}
					}
					reference[k].lifting[cell * n..(cell + 1) * n].copy_from_slice(&c.lifting[..n]);
					reference[k].body_force[cell * n..(cell + 1) * n]
						.copy_from_slice(&c.body_force[..n]);
				}
				for (i, face) in facets.iter().enumerate() {
					for (out, &point) in reference[k].prescribed[i].iter_mut().zip(&face.nodes) {
						let v = field(point);
						for axis in 0..d {
							out[axis] = scale * v[axis];
						}
					}
				}
			}
			let source = BoxTimeDataShard::new(
				&geo,
				0..cells,
				2,
				[-1., 1.],
				coefficients,
				BoxTimeDataLimits::default(),
			)
			.unwrap();
			for k in 0..3 {
				for cell in 0..cells {
					let defect = source
						.validate_cell(k, cell, &mut |other| {
							Ok(source.coefficient(k, other).unwrap().lifting)
						})
						.unwrap();
					assert!(defect < 1e-10);
				}
			}
			let bounded =
				PolynomialBoundary::new(&space, reference, BoundaryLimits::default()).unwrap();
			let recipe = BoxForceRecipe::new(&geo, 0.03, 0., ForceRecipeLimits::default()).unwrap();
			let state = (0..space.dimension())
				.map(|i| 0.007 * f64::from(u32::try_from(i + 1).unwrap()).sin())
				.collect::<Vec<_>>();
			for time in [-0.3, 0.17, 0.8] {
				let full = bounded.coefficients_at(time, &state).unwrap();
				let expected = bounded.momentum_force(time, &state).unwrap();
				let mut effective = Vec::new();
				for cell in 0..cells {
					let data = source.evaluate_cell(time, cell).unwrap();
					let force = recipe
						.cell_force_with_data(
							cell,
							&mut |i| {
								let mut a = [0.; 30];
								a[..n].copy_from_slice(&full[i * n..(i + 1) * n]);
								Ok(a)
							},
							&data.prescribed,
							&data.body_force,
						)
						.unwrap();
					for i in 0..n {
						assert!((force[i] - expected[cell * n + i]).abs() < 3e-9);
						let derivative = (0..n)
							.map(|j| {
								geo.mass_value(cell, i, j).unwrap() * data.lifting_derivative[j]
							})
							.sum::<f64>();
						effective.push(force[i] - derivative);
					}
				}
				let drift = bounded.drift(time, &state).unwrap();
				for (q, want) in space.chart().iter().zip(drift) {
					assert!(
						(q.iter().zip(&effective).map(|(a, b)| a * b).sum::<f64>() - want).abs()
							< 3e-9
					);
				}
			}
		}
	}
}

#[test]
fn source_checks_capacity_inactive_slots_and_time_domain() {
	let geo = BoxConstraintRecipe::new(
		2,
		1,
		1.,
		BoxBoundary::Periodic,
		1,
		ConstraintRecipeLimits::default(),
	)
	.unwrap();
	let mut c = vec![BoxTimeCoefficient::zero(); 2];
	c[0].lifting[29] = 1.;
	assert!(
		BoxTimeDataShard::new(&geo, 0..2, 0, [0., 1.], c, BoxTimeDataLimits::default()).is_err()
	);
	let mut c = vec![BoxTimeCoefficient::zero(); 2];
	c.reserve_exact(1000);
	assert!(
		BoxTimeDataShard::new(
			&geo,
			0..2,
			0,
			[0., 1.],
			c,
			BoxTimeDataLimits {
				max_bytes: 65536,
				..Default::default()
			}
		)
		.is_err()
	);
	let s = BoxTimeDataShard::new(
		&geo,
		0..2,
		0,
		[0., 1.],
		vec![BoxTimeCoefficient::zero(); 2],
		BoxTimeDataLimits::default(),
	)
	.unwrap();
	assert!(s.evaluate_cell(-0.1, 0).is_err());
	assert!(s.evaluate_cell(f64::NAN, 0).is_err());
	assert!(s.evaluate_cell(0., 2).is_err());
}

#[test]
fn full_constraint_validation_rejects_divergence_and_normal_mismatch() {
	let geo = BoxConstraintRecipe::new(
		2,
		1,
		1.,
		BoxBoundary::Cavity { lid_speed: 0. },
		2,
		ConstraintRecipeLimits::default(),
	)
	.unwrap();
	let mut c = vec![BoxTimeCoefficient::zero(); 2];
	c[0].lifting[0] = 0.25;
	let s =
		BoxTimeDataShard::new(&geo, 0..2, 0, [0., 1.], c, BoxTimeDataLimits::default()).unwrap();
	assert!(
		s.validate_cell(0, 0, &mut |other| Ok(s
			.coefficient(0, other)
			.unwrap()
			.lifting))
			.is_err()
	);
	let mut c = vec![BoxTimeCoefficient::zero(); 2];
	for (face, trace) in c[0].prescribed.iter_mut().enumerate().take(3) {
		if geo.facet_is_exterior(0, face).unwrap() {
			let node = geo.facet_velocity_node(face, 0).unwrap();
			trace[node] = 1.;
			trace[6 + node] = 1.;
		}
	}
	let s =
		BoxTimeDataShard::new(&geo, 0..2, 0, [0., 1.], c, BoxTimeDataLimits::default()).unwrap();
	assert!(
		s.validate_cell(0, 0, &mut |other| Ok(s
			.coefficient(0, other)
			.unwrap()
			.lifting))
			.is_err()
	);
}

#[test]
fn degree_eight_owned_subrange_and_empty_shard_keep_complete_modes() {
	let geo = BoxConstraintRecipe::new(
		2,
		1,
		1.,
		BoxBoundary::Periodic,
		1,
		ConstraintRecipeLimits::default(),
	)
	.unwrap();
	let mut c = vec![BoxTimeCoefficient::zero(); 9];
	for i in 0..3 {
		c[8].lifting[i] = 1.;
		c[8].body_force[i] = 2.;
	}
	let source =
		BoxTimeDataShard::new(&geo, 1..2, 8, [-1., 1.], c, BoxTimeDataLimits::default()).unwrap();
	let value = source.evaluate_cell(0.5, 1).unwrap();
	assert!((value.lifting[0] - 0.5_f64.powi(8)).abs() < 1e-15);
	assert!((value.lifting_derivative[0] - 8. * 0.5_f64.powi(7)).abs() < 1e-15);
	assert!((value.body_force[0] - 2. * 0.5_f64.powi(8)).abs() < 1e-15);
	assert!(
		source
			.validate_cell(8, 1, &mut |_| {
				let mut a = [0.; 30];
				a[..3].fill(1.);
				Ok(a)
			})
			.unwrap()
			< 1e-12
	);
	let empty = BoxTimeDataShard::new(
		&geo,
		2..2,
		8,
		[-1., 1.],
		vec![],
		BoxTimeDataLimits::default(),
	)
	.unwrap();
	assert!(empty.coefficient(0, 2).is_none());
	assert!(empty.evaluate_cell(0., 2).is_err());
}
