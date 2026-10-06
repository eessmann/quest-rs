//! Independent unequal-cell full-space and admission checks.
#![allow(
	clippy::panic_in_result_fn,
	reason = "Independent numerical fixture assertions fail tests directly"
)]
use quest_cfd::physical_space::{
	AffineMeshView, DirichletFacet, PhysicalMeshLimits, PhysicalSpace,
};

#[test]
fn closed_unequal_triangles_keep_the_complete_kernel() -> Result<(), Box<dyn std::error::Error>> {
	let vertices = [[0., 0., 0.], [2., 0., 0.], [1., 1., 0.], [0., 1., 0.]];
	let cells: &[&[usize]] = &[&[0, 1, 2], &[0, 2, 3]];
	let walls = [
		DirichletFacet {
			vertices: &[0, 1],
			label: "bottom",
		},
		DirichletFacet {
			vertices: &[1, 2],
			label: "right",
		},
		DirichletFacet {
			vertices: &[2, 3],
			label: "top",
		},
		DirichletFacet {
			vertices: &[3, 0],
			label: "left",
		},
	];
	for (order, n, rank, m) in [(1, 12, 11, 1), (2, 24, 20, 4)] {
		let space = PhysicalSpace::from_mesh(
			AffineMeshView {
				dimension: 2,
				vertices: &vertices,
				cells,
				dirichlet: &walls,
				periodic: &[],
			},
			0.01,
			order,
			PhysicalMeshLimits::default(),
		)?;
		assert_eq!(space.diagnostics().local_velocity_dimension, n);
		assert_eq!(space.diagnostics().constraint_rank, rank);
		assert_eq!(space.dimension(), m);
		assert!(space.mesh_resources().is_some());
	}
	Ok(())
}

#[test]
fn count_shape_and_budget_reject_before_materialization() {
	let vertices = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
	let walls = [
		DirichletFacet {
			vertices: &[0, 1],
			label: "a",
		},
		DirichletFacet {
			vertices: &[1, 2],
			label: "b",
		},
		DirichletFacet {
			vertices: &[2, 0],
			label: "c",
		},
	];
	let view = AffineMeshView {
		dimension: 2,
		vertices: &vertices,
		cells: &[&[0, 1, 2]],
		dirichlet: &walls,
		periodic: &[],
	};
	assert!(
		PhysicalSpace::from_mesh(
			view,
			0.,
			2,
			PhysicalMeshLimits {
				max_bytes: 1,
				..Default::default()
			}
		)
		.is_err()
	);
	assert!(
		PhysicalSpace::from_mesh(
			view,
			0.,
			2,
			PhysicalMeshLimits {
				max_work: 1,
				..Default::default()
			}
		)
		.is_err()
	);
	assert!(
		PhysicalSpace::from_mesh(
			AffineMeshView {
				cells: &[&[0, 1, 1]],
				..view
			},
			0.,
			2,
			PhysicalMeshLimits::default()
		)
		.is_err()
	);
}

fn closed_space(
	d: usize,
	order: usize,
	limits: PhysicalMeshLimits,
) -> Result<PhysicalSpace, quest_cfd::CfdError> {
	if d == 2 {
		let v = [[0., 0., 0.], [2., 0., 0.], [1., 1., 0.], [0., 1., 0.]];
		let f = [
			DirichletFacet {
				vertices: &[0, 1],
				label: "a",
			},
			DirichletFacet {
				vertices: &[1, 2],
				label: "b",
			},
			DirichletFacet {
				vertices: &[2, 3],
				label: "c",
			},
			DirichletFacet {
				vertices: &[3, 0],
				label: "d",
			},
		];
		PhysicalSpace::from_mesh(
			AffineMeshView {
				dimension: 2,
				vertices: &v,
				cells: &[&[0, 1, 2], &[0, 2, 3]],
				dirichlet: &f,
				periodic: &[],
			},
			0.01,
			order,
			limits,
		)
	} else {
		let v = [
			[0., 0., 0.],
			[1., 0., 0.],
			[0., 1., 0.],
			[0., 0., 1.],
			[0., 0., -2.],
		];
		let f = [
			DirichletFacet {
				vertices: &[0, 1, 3],
				label: "a",
			},
			DirichletFacet {
				vertices: &[0, 2, 3],
				label: "b",
			},
			DirichletFacet {
				vertices: &[1, 2, 3],
				label: "c",
			},
			DirichletFacet {
				vertices: &[0, 1, 4],
				label: "d",
			},
			DirichletFacet {
				vertices: &[0, 2, 4],
				label: "e",
			},
			DirichletFacet {
				vertices: &[1, 2, 4],
				label: "f",
			},
		];
		PhysicalSpace::from_mesh(
			AffineMeshView {
				dimension: 3,
				vertices: &v,
				cells: &[&[0, 1, 2, 3], &[0, 2, 1, 4]],
				dirichlet: &f,
				periodic: &[],
			},
			0.01,
			order,
			limits,
		)
	}
}
#[allow(
	clippy::many_single_char_names,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Independent rational mass table uses bounded triangle/tetrahedron node indices"
)]
fn scalar_mass(d: usize, p: usize, i: usize, j: usize) -> f64 {
	if p == 1 {
		return if i == j { 2. } else { 1. } / if d == 2 { 12. } else { 20. };
	}
	let n = d + 1;
	let mut edges = Vec::new();
	for a in 0..n {
		for b in a + 1..n {
			edges.push((a, b));
		}
	}
	if i < n && j < n {
		return match (d, i == j) {
			(2, true) => 1. / 30.,
			(2, false) => -1. / 180.,
			(_, true) => 1. / 70.,
			(_, false) => 1. / 420.,
		};
	}
	if i < n || j < n {
		let vertex = i.min(j);
		let edge = edges[i.max(j) - n];
		let incident = edge.0 == vertex || edge.1 == vertex;
		return match (d, incident) {
			(2, true) => 0.,
			(2, false) => -1. / 45.,
			(_, true) => -1. / 105.,
			(_, false) => -1. / 70.,
		};
	}
	if i == j {
		return if d == 2 { 8. / 45. } else { 8. / 105. };
	}
	let a = edges[i - n];
	let b = edges[j - n];
	if <[usize; 2]>::from(a)
		.iter()
		.any(|vertex| <[usize; 2]>::from(b).contains(vertex))
	{
		if d == 2 { 4. / 45. } else { 4. / 105. }
	} else {
		2. / 105.
	}
}
#[test]
fn unequal_mass_and_complete_ranks_use_independent_barycentric_integrals()
-> Result<(), Box<dyn std::error::Error>> {
	for d in [2, 3] {
		for p in [1, 2] {
			let space = closed_space(d, p, PhysicalMeshLimits::default())?;
			let expected = match (d, p) {
				(2, 1) => (12, 11, 1),
				(2, 2) => (24, 20, 4),
				(3, 1) => (24, 22, 2),
				_ => (60, 49, 11),
			};
			assert_eq!(
				(
					space.diagnostics().local_velocity_dimension,
					space.diagnostics().constraint_rank,
					space.dimension()
				),
				expected
			);
			let state = (0..space.dimension())
				.map(|i| {
					f64::from(u32::try_from(i + 1).expect("bounded coordinate count")).sin() * 0.01
				})
				.collect::<Vec<_>>();
			let drift = space.drift(&state)?;
			let power = state.iter().zip(drift).map(|(a, b)| a * b).sum::<f64>();
			assert!(
				power < 0.,
				"homogeneous closed SIP must dissipate a nonzero state"
			);
			let scalar = space.local_velocity_per_cell() / d;
			let volumes = if d == 2 {
				[1., 0.5]
			} else {
				[1. / 6., 1. / 3.]
			};
			for (a, q) in space.chart().iter().enumerate() {
				for (b, r) in space.chart().iter().enumerate() {
					let mut mass = 0.;
					for (cell, volume) in volumes.iter().enumerate() {
						for axis in 0..d {
							let offset = (cell * d + axis) * scalar;
							for i in 0..scalar {
								for j in 0..scalar {
									mass = (volume * q[offset + i] * scalar_mass(d, p, i, j))
										.mul_add(r[offset + j], mass);
								}
							}
						}
					}
					assert!((mass - f64::from(a == b)).abs() < 2e-10);
				}
				let recovered = space.coefficients(&space.coordinates(q)?)?;
				assert!(q.iter().zip(recovered).all(|(x, y)| (x - y).abs() < 1e-9));
			}
			assert!(
				space
					.mesh_resources()
					.and_then(|r| r.minimum_quality_lower)
					.is_some_and(|x| x > 0.)
			);
		}
	}
	Ok(())
}
#[test]
fn affine_body_gradient_recovers_signed_volume_weighted_pressure()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::physical_space::{
		BoundaryLimits, BoundaryTimeCoefficient, BoxBoundarySide, PolynomialBoundary,
	};
	for d in [2, 3] {
		for p in [1, 2] {
			let space = closed_space(d, p, PhysicalMeshLimits::default())?;
			let mut source = BoundaryTimeCoefficient::zero(&space)?;
			let scalar = space.local_velocity_per_cell() / d;
			let axis = if d == 2 { 0 } else { 2 };
			for cell in 0..2 {
				let start = cell * space.local_velocity_per_cell() + axis * scalar;
				source.body_force[start..start + scalar].fill(1.);
			}
			let flow = PolynomialBoundary::new(&space, vec![source], BoundaryLimits::default())?;
			let state = vec![0.; space.dimension()];
			assert!(flow.drift(0.37, &state)?.iter().all(|x| x.abs() < 1e-10));
			let pressure = flow.reconstruct_pressure(0.37, &state)?;
			assert!(pressure.momentum_residual < 1e-10);
			assert!(pressure.gauge_residual < 1e-12);
			assert!(pressure.mesh_resources.is_some());
			let expected = if d == 2 {
				if p == 1 {
					vec![vec![2. / 9.], vec![-4. / 9.]]
				} else {
					vec![
						vec![-7. / 9., 11. / 9., 2. / 9.],
						vec![-7. / 9., 2. / 9., -7. / 9.],
					]
				}
			} else if p == 1 {
				vec![vec![0.5], vec![-0.25]]
			} else {
				vec![vec![0.25, 0.25, 0.25, 1.25], vec![0.25, 0.25, 0.25, -1.75]]
			};
			for (actual, expected) in pressure
				.pressure_coefficients
				.iter()
				.flatten()
				.zip(expected.iter().flatten())
			{
				assert!(
					(actual - expected).abs() < 1e-9,
					"d={d} p={p}: {actual} vs {expected}"
				);
			}
			assert!(
				space
					.boundary_force(
						&state,
						&pressure.pressure_coefficients,
						BoxBoundarySide::XMin
					)
					.is_err()
			);
		}
	}
	let limited = closed_space(
		2,
		2,
		PhysicalMeshLimits {
			max_pressure_work: 1,
			..Default::default()
		},
	)?;
	assert!(
		limited
			.reconstruct_pressure(&vec![0.; limited.dimension()])
			.is_err()
	);
	Ok(())
}

#[test]
fn geometric_rejection_preserves_attempted_work_without_numerical_claims() {
	let v = [[0., 0., 0.], [2., 0., 0.], [1., 1., 0.], [1., 0.5, 0.]];
	let f = [
		DirichletFacet {
			vertices: &[0, 2],
			label: "a",
		},
		DirichletFacet {
			vertices: &[1, 2],
			label: "b",
		},
		DirichletFacet {
			vertices: &[0, 3],
			label: "c",
		},
		DirichletFacet {
			vertices: &[1, 3],
			label: "d",
		},
	];
	let attempt = PhysicalSpace::from_mesh_with_receipt(
		AffineMeshView {
			dimension: 2,
			vertices: &v,
			cells: &[&[0, 1, 2], &[0, 1, 3]],
			dirichlet: &f,
			periodic: &[],
		},
		0.,
		2,
		PhysicalMeshLimits::default(),
	);
	assert!(attempt.outcome.is_err());
	let receipt = attempt
		.resources
		.expect("topology admitted before exact overlap failure");
	assert!(receipt.geometry_work > 0);
	assert!(receipt.geometry_peak_bytes > 0);
	assert_eq!(receipt.completed_phase, "geometry admitted");
	assert!(receipt.constraint_rank.is_none());
	assert!(receipt.minimum_quality_lower.is_none());
}
#[test]
fn missing_boundary_nonmanifold_incidence_and_hidden_external_capacity_reject() {
	let v = [[0., 0., 0.], [2., 0., 0.], [1., 1., 0.], [0., 1., 0.]];
	let f = [
		DirichletFacet {
			vertices: &[0, 1],
			label: "a",
		},
		DirichletFacet {
			vertices: &[1, 2],
			label: "b",
		},
		DirichletFacet {
			vertices: &[2, 3],
			label: "c",
		},
		DirichletFacet {
			vertices: &[3, 0],
			label: "d",
		},
	];
	let view = AffineMeshView {
		dimension: 2,
		vertices: &v,
		cells: &[&[0, 1, 2], &[0, 2, 3]],
		dirichlet: &f,
		periodic: &[],
	};
	assert!(
		PhysicalSpace::from_mesh(
			AffineMeshView {
				dirichlet: &f[..3],
				..view
			},
			0.,
			2,
			PhysicalMeshLimits::default()
		)
		.is_err()
	);
	assert!(
		PhysicalSpace::from_mesh(
			AffineMeshView {
				cells: &[&[0, 1, 2], &[0, 2, 3], &[0, 2, 1]],
				..view
			},
			0.,
			2,
			PhysicalMeshLimits::default()
		)
		.is_err()
	);
	assert!(
		PhysicalSpace::from_mesh(
			view,
			0.,
			2,
			PhysicalMeshLimits {
				external_retained_bytes: usize::MAX,
				..Default::default()
			}
		)
		.is_err()
	);
	assert!(
		PhysicalSpace::from_mesh(
			view,
			0.,
			2,
			PhysicalMeshLimits {
				minimum_scaled_quality: 0.9,
				..Default::default()
			}
		)
		.is_err()
	);
}

#[test]
fn periodic_unequal_square_preserves_means_mapping_and_convection()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::physical_space::{
		BoundaryLimits, BoundaryTimeCoefficient, PolynomialBoundary, TranslationalPeriodicFacet,
	};
	let v = [
		[0., 0., 0.],
		[1., 0., 0.],
		[1., 1., 0.],
		[0., 1., 0.],
		[0.3, 0.4, 0.],
	];
	let periodic = [
		TranslationalPeriodicFacet {
			left: &[3, 0],
			right: &[2, 1],
		},
		TranslationalPeriodicFacet {
			left: &[1, 0],
			right: &[2, 3],
		},
	];
	let view = AffineMeshView {
		dimension: 2,
		vertices: &v,
		cells: &[&[0, 1, 4], &[1, 2, 4], &[2, 3, 4], &[3, 0, 4]],
		dirichlet: &[],
		periodic: &periodic,
	};
	for (p, m) in [(1, 9), (2, 19)] {
		let space = PhysicalSpace::from_mesh(view, 0., p, PhysicalMeshLimits::default())?;
		assert_eq!(space.dimension(), m);
		let scalar = space.local_velocity_per_cell() / 2;
		let mut u = vec![0.; space.diagnostics().local_velocity_dimension];
		for cell in u.chunks_mut(2 * scalar) {
			cell[..scalar].fill(0.2);
			cell[scalar..].fill(-0.1);
		}
		let a = space.coordinates(&u)?;
		let recovered = space.coefficients(&a)?;
		assert!(u.iter().zip(recovered).all(|(x, y)| (x - y).abs() < 1e-10));
		assert!((space.energy(&a)? - 0.025).abs() < 1e-11);
		let seed = (0..m)
			.map(|i| f64::from(u32::try_from(i + 1).expect("small dimension")).sin() * 0.01)
			.collect::<Vec<_>>();
		let drift = space.drift(&seed)?;
		assert!(
			seed.iter()
				.zip(&drift)
				.map(|(a, b)| a * b)
				.sum::<f64>()
				.abs()
				< 1e-10
		);
		assert!(drift.iter().map(|x| x * x).sum::<f64>() > 1e-8);
		let mut data = BoundaryTimeCoefficient::zero(&space)?;
		for cell in data.body_force.chunks_mut(2 * scalar) {
			cell[..scalar].fill(1.);
		}
		let flow = PolynomialBoundary::new(&space, vec![data], BoundaryLimits::default())?;
		let zero = vec![0.; m];
		let acceleration = space.coefficients(&flow.drift(0.31, &zero)?)?;
		for cell in acceleration.chunks(2 * scalar) {
			assert!(cell[..scalar].iter().all(|x| (x - 1.).abs() < 1e-9));
			assert!(cell[scalar..].iter().all(|x| x.abs() < 1e-9));
		}
	}
	let wrong = [
		TranslationalPeriodicFacet {
			left: &[3, 0],
			right: &[1, 2],
		},
		periodic[1],
	];
	assert!(
		PhysicalSpace::from_mesh(
			AffineMeshView {
				periodic: &wrong,
				..view
			},
			0.,
			2,
			PhysicalMeshLimits::default()
		)
		.is_err()
	);
	Ok(())
}

#[test]
#[allow(
	clippy::too_many_lines,
	clippy::float_cmp,
	reason = "One explicit twelve-tetrahedron fixture declares its cap and exact shared vertex identities"
)]
fn periodic_unequal_tetrahedra_use_declared_full_work_allowance()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::physical_space::TranslationalPeriodicFacet;
	let mut vertices = Vec::new();
	for x in [0., 1. / 3., 1.] {
		for y in [0., 1.] {
			for z in [0., 1.] {
				vertices.push([x, y, z]);
			}
		}
	}
	let permutations = [
		[0, 1, 2],
		[0, 2, 1],
		[1, 0, 2],
		[1, 2, 0],
		[2, 0, 1],
		[2, 1, 0],
	];
	let mut cells = Vec::new();
	for strip in 0..2 {
		for perm in permutations {
			let mut index = [strip, 0, 0];
			let mut cell = vec![index[0] * 4 + index[1] * 2 + index[2]];
			for axis in perm {
				index[axis] += 1;
				cell.push(index[0] * 4 + index[1] * 2 + index[2]);
			}
			cells.push(cell);
		}
	}
	let mut exterior = Vec::<Vec<usize>>::new();
	for cell in &cells {
		for omit in 0..4 {
			let mut face = cell
				.iter()
				.enumerate()
				.filter_map(|(i, &v)| (i != omit).then_some(v))
				.collect::<Vec<_>>();
			face.sort_unstable();
			if let Some(old) = exterior.iter().position(|f| f == &face) {
				exterior.remove(old);
			} else {
				exterior.push(face);
			}
		}
	}
	let mut pairs = Vec::<(Vec<usize>, Vec<usize>)>::new();
	for face in &exterior {
		if let Some(axis) = (0..3).find(|&axis| face.iter().all(|&i| vertices[i][axis] == 0.)) {
			let mut right = Vec::new();
			for &i in face {
				let mut point = vertices[i];
				point[axis] = 1.;
				right.push(
					vertices
						.iter()
						.position(|p| *p == point)
						.ok_or("periodic counterpart")?,
				);
			}
			pairs.push((face.clone(), right));
		}
	}
	let records = pairs
		.iter()
		.map(|(l, r)| TranslationalPeriodicFacet { left: l, right: r })
		.collect::<Vec<_>>();
	let cells = cells.iter().map(Vec::as_slice).collect::<Vec<_>>();
	let view = AffineMeshView {
		dimension: 3,
		vertices: &vertices,
		cells: &cells,
		dirichlet: &[],
		periodic: &records,
	};
	let rejected =
		PhysicalSpace::from_mesh_with_receipt(view, 0., 2, PhysicalMeshLimits::default());
	assert!(rejected.outcome.is_err());
	let limits = PhysicalMeshLimits {
		max_work: 2_000_000_000,
		max_pressure_work: 2_000_000_000,
		external_retained_bytes: 16_384,
		..Default::default()
	};
	for (p, n, r, m) in [(1, 144, 83, 61), (2, 360, 191, 169)] {
		let space = PhysicalSpace::from_mesh(view, 0., p, limits)?;
		assert_eq!(
			(
				space.diagnostics().local_velocity_dimension,
				space.diagnostics().constraint_rank,
				space.dimension()
			),
			(n, r, m)
		);
		let scalar = space.local_velocity_per_cell() / 3;
		for axis in 0..3 {
			let mut c = vec![0.; n];
			for cell in c.chunks_mut(3 * scalar) {
				cell[axis * scalar..(axis + 1) * scalar].fill(1.);
			}
			let a = space.coordinates(&c)?;
			assert!((space.energy(&a)? - 0.5).abs() < 1e-10);
			assert!(
				space
					.coefficients(&a)?
					.iter()
					.zip(c)
					.all(|(x, y)| (x - y).abs() < 1e-9)
			);
		}
		let pressure = space.reconstruct_pressure(&vec![0.; m])?;
		assert!(pressure.mesh_resources.is_some());
		println!(
			"affine periodic p={p} resources={:?}",
			space.mesh_resources()
		);
	}
	Ok(())
}

#[test]
fn polynomial_lifting_keeps_original_mass_derivative_and_last_coordinate()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::physical_space::{BoundaryLimits, BoundaryTimeCoefficient, PolynomialBoundary};
	for d in [2, 3] {
		for p in [1, 2] {
			let space = closed_space(d, p, PhysicalMeshLimits::default())?;
			let mut zeroth = BoundaryTimeCoefficient::zero(&space)?;
			let mut first = BoundaryTimeCoefficient::zero(&space)?;
			let nodes = space.velocity_nodes()?;
			let scalar = space.local_velocity_per_cell() / d;
			for (cell, points) in nodes.iter().enumerate() {
				for (j, point) in points.iter().enumerate() {
					let index = cell * space.local_velocity_per_cell() + j;
					zeroth.body_force[index] = point[1];
					first.lifting[index] = point[1];
				}
			}
			for (values, facet) in first.prescribed.iter_mut().zip(space.boundary_facets()?) {
				for (value, point) in values.iter_mut().zip(facet.nodes) {
					value[0] = point[1];
				}
			}
			let flow =
				PolynomialBoundary::new(&space, vec![zeroth, first], BoundaryLimits::default())?;
			let zero = vec![0.; space.dimension()];
			for time in [0.13, 0.37] {
				let coefficients = flow.coefficients_at(time, &zero)?;
				for (cell, values) in coefficients.chunks(d * scalar).enumerate() {
					for (j, point) in nodes[cell].iter().enumerate() {
						assert!(time.mul_add(-point[1], values[j]).abs() < 1e-10);
					}
					assert!(values[scalar..].iter().all(|x| x.abs() < 1e-10));
				}
				assert!(flow.drift(time, &zero)?.iter().all(|x| x.abs() < 2e-9));
				let pressure = flow.reconstruct_pressure(time, &zero)?;
				assert!(
					pressure
						.pressure_coefficients
						.iter()
						.flatten()
						.all(|x| x.abs() < 1e-8)
				);
				assert!(
					pressure
						.mesh_resources
						.as_ref()
						.is_some_and(|r| r.additional_owner_bytes > 0)
				);
			}
			let mut full = zero.clone();
			let last = full.last_mut().ok_or("nonempty complete chart")?;
			*last = 0.03;
			let a = flow.coefficients_at(0.37, &full)?;
			let b = flow.coefficients_at(0.37, &zero)?;
			let physical = space.coefficients(&full)?;
			assert!(
				a.iter()
					.zip(b)
					.zip(physical)
					.all(|((x, y), z)| (x - y - z).abs() < 1e-10)
			);
		}
	}
	Ok(())
}

#[test]
fn actual_owner_capacity_and_lifted_pressure_overlap_are_charged()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::physical_space::{BoundaryLimits, BoundaryTimeCoefficient, PolynomialBoundary};
	let space = closed_space(2, 2, PhysicalMeshLimits::default())?;
	let receipt = space.mesh_resources().ok_or("mesh receipt")?;
	assert_eq!(receipt.retained_bytes, Some(space.retained_bytes()?));
	assert_eq!(receipt.completed_phase, "complete");
	let same_geometry = closed_space(2, 1, PhysicalMeshLimits::default())?;
	assert_eq!(
		receipt.source_identity,
		same_geometry
			.mesh_resources()
			.ok_or("receipt")?
			.source_identity
	);
	let zero = vec![0.; space.dimension()];
	let base = space
		.reconstruct_pressure(&zero)?
		.mesh_resources
		.ok_or("pressure receipt")?;
	let plain = PolynomialBoundary::new(
		&space,
		vec![BoundaryTimeCoefficient::zero(&space)?],
		BoundaryLimits::default(),
	)?;
	let mut oversized = BoundaryTimeCoefficient::zero(&space)?;
	oversized.body_force.reserve_exact(4096);
	let capacity = oversized.body_force.capacity();
	let large = PolynomialBoundary::new(&space, vec![oversized], BoundaryLimits::default())?;
	let extra = (capacity - space.diagnostics().local_velocity_dimension) * size_of::<f64>();
	assert_eq!(
		large.resources().retained_bytes - plain.resources().retained_bytes,
		extra
	);
	let query = large
		.reconstruct_pressure(0.1, &zero)?
		.mesh_resources
		.ok_or("lifted receipt")?;
	assert_eq!(
		query.additional_owner_bytes,
		large.resources().retained_bytes
	);
	assert_eq!(
		query.peak_bytes,
		base.peak_bytes
			+ query.additional_owner_bytes
			+ 256 * space.diagnostics().local_velocity_dimension
	);
	let limited = closed_space(
		2,
		2,
		PhysicalMeshLimits {
			max_pressure_bytes: base.peak_bytes,
			..Default::default()
		},
	)?;
	assert!(limited.reconstruct_pressure(&zero).is_ok());
	let flow = PolynomialBoundary::new(
		&limited,
		vec![BoundaryTimeCoefficient::zero(&limited)?],
		BoundaryLimits::default(),
	)?;
	assert!(flow.reconstruct_pressure(0.1, &zero).is_err());
	Ok(())
}

#[test]
#[allow(
	clippy::too_many_lines,
	reason = "Independent red-refinement and nodal prolongation fixture keeps all topology and basis data explicit"
)]
fn refinement_and_order_elevation_preserve_every_coarse_coordinate()
-> Result<(), Box<dyn std::error::Error>> {
	let mut vertices = vec![[0., 0., 0.], [2., 0., 0.], [1., 1., 0.], [0., 1., 0.]];
	let edges = [(0, 1), (1, 2), (0, 2), (2, 3), (0, 3)];
	for (a, b) in edges {
		vertices.push(std::array::from_fn(|j| {
			f64::midpoint(vertices[a][j], vertices[b][j])
		}));
	}
	let children: &[&[usize]] = &[
		&[0, 4, 6],
		&[4, 1, 5],
		&[6, 5, 2],
		&[4, 5, 6],
		&[0, 6, 8],
		&[6, 2, 7],
		&[8, 7, 3],
		&[6, 7, 8],
	];
	let boundary: &[&[usize]] = &[
		&[0, 4],
		&[4, 1],
		&[1, 5],
		&[5, 2],
		&[2, 7],
		&[7, 3],
		&[3, 8],
		&[8, 0],
	];
	let walls: Vec<_> = boundary
		.iter()
		.map(|ids| DirichletFacet {
			vertices: ids,
			label: "wall",
		})
		.collect();
	let coarse = closed_space(2, 1, PhysicalMeshLimits::default())?;
	for order in [1, 2] {
		let fine = PhysicalSpace::from_mesh(
			AffineMeshView {
				dimension: 2,
				vertices: &vertices,
				cells: children,
				dirichlet: &walls,
				periodic: &[],
			},
			0.01,
			order,
			PhysicalMeshLimits::default(),
		)?;
		let (rank, dimension) = if order == 1 { (39, 9) } else { (71, 25) };
		assert_eq!(fine.diagnostics().constraint_rank, rank);
		assert_eq!(fine.dimension(), dimension);
		for coarse_q in coarse.chart() {
			let mut coefficients = vec![0.; fine.diagnostics().local_velocity_dimension];
			let scalar = fine.local_velocity_per_cell() / 2;
			for (cell, nodes) in fine.velocity_nodes()?.iter().enumerate() {
				let parent = cell / 4;
				for (node, point) in nodes.iter().enumerate() {
					let (x, y) = (point[0], point[1]);
					let bary = if parent == 0 {
						[(x - y).mul_add(-0.5, 1. - y), (x - y) * 0.5, y]
					} else {
						[1. - y, x, y - x]
					};
					for axis in 0..2 {
						let value = bary
							.iter()
							.enumerate()
							.map(|(j, b)| b * coarse_q[parent * 6 + axis * 3 + j])
							.sum::<f64>();
						coefficients[cell * scalar * 2 + axis * scalar + node] = value;
					}
				}
			}
			let state = fine.coordinates(&coefficients)?;
			let restored = fine.coefficients(&state)?;
			assert!(
				restored
					.iter()
					.zip(&coefficients)
					.all(|(x, y)| (x - y).abs() < 1e-9)
			);
			assert!((fine.energy(&state)? - 0.5).abs() < 1e-10);
		}
		assert_eq!(fine.chart().len(), dimension);
	}
	Ok(())
}

#[test]
#[allow(
	clippy::too_many_lines,
	reason = "Four independent explicit unsupported-closure fixtures keep their topology inline"
)]
fn mixed_natural_general_periodic_and_zero_chart_spaces_are_explicitly_unsupported() {
	use quest_cfd::physical_space::TranslationalPeriodicFacet;
	let vertices = [
		[0., 0., 0.],
		[1., 0., 0.],
		[1., 1., 0.],
		[0., 1., 0.],
		[0.5, 0.5, 0.],
	];
	let cells: &[&[usize]] = &[&[0, 1, 4], &[1, 2, 4], &[2, 3, 4], &[3, 0, 4]];
	let pairs = [
		TranslationalPeriodicFacet {
			left: &[0, 3],
			right: &[1, 2],
		},
		TranslationalPeriodicFacet {
			left: &[0, 1],
			right: &[3, 2],
		},
	];
	let view = AffineMeshView {
		dimension: 2,
		vertices: &vertices,
		cells,
		dirichlet: &[],
		periodic: &pairs,
	};
	let walls = [
		DirichletFacet {
			vertices: &[0, 1],
			label: "wall",
		},
		DirichletFacet {
			vertices: &[2, 3],
			label: "wall",
		},
	];
	assert!(
		PhysicalSpace::from_mesh(
			AffineMeshView {
				dirichlet: &walls,
				periodic: &pairs[..1],
				..view
			},
			0.01,
			1,
			PhysicalMeshLimits::default()
		)
		.is_err()
	);
	assert!(
		PhysicalSpace::from_mesh(
			AffineMeshView {
				periodic: &[],
				..view
			},
			0.01,
			1,
			PhysicalMeshLimits::default()
		)
		.is_err()
	);
	// A genuine translation-periodic parallelogram is outside the first box-only seam contract.
	let shear = vertices.map(|[x, y, z]| [0.25_f64.mul_add(y, x), y, z]);
	assert!(
		PhysicalSpace::from_mesh(
			AffineMeshView {
				vertices: &shear,
				..view
			},
			0.01,
			1,
			PhysicalMeshLimits::default()
		)
		.is_err()
	);
	let triangle_walls = [
		DirichletFacet {
			vertices: &[0, 1],
			label: "a",
		},
		DirichletFacet {
			vertices: &[1, 2],
			label: "b",
		},
		DirichletFacet {
			vertices: &[2, 0],
			label: "c",
		},
	];
	let triangle = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
	assert!(
		PhysicalSpace::from_mesh(
			AffineMeshView {
				dimension: 2,
				vertices: &triangle,
				cells: &[&[0, 1, 2]],
				dirichlet: &triangle_walls,
				periodic: &[]
			},
			0.01,
			1,
			PhysicalMeshLimits::default()
		)
		.is_err()
	);
}
