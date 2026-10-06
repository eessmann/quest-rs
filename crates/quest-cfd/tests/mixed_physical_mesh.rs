//! Independent complete-space and boundary-flux checks for natural traction.
#![allow(
	clippy::panic_in_result_fn,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Bounded independent one-simplex fixture algebra and assertions"
)]
use quest_cfd::physical_space::{
	ExteriorCondition, ExteriorFacet, MixedAffineMeshView, PhysicalMeshLimits, PhysicalSpace,
};
fn simplex(d: usize, p: usize, nu: f64) -> Result<PhysicalSpace, quest_cfd::CfdError> {
	let v = [[0., 0., 0.], [2., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
	let facets: &[&[usize]] = if d == 2 {
		&[&[0, 1], &[0, 2], &[1, 2]]
	} else {
		&[&[0, 1, 2], &[0, 1, 3], &[0, 2, 3], &[1, 2, 3]]
	};
	let exterior: Vec<_> = facets
		.iter()
		.enumerate()
		.map(|(i, ids)| ExteriorFacet {
			vertices: ids,
			label: if i == d { "natural" } else { "wall" },
			condition: if i == d {
				ExteriorCondition::NaturalMechanicalTraction
			} else {
				ExteriorCondition::Dirichlet
			},
		})
		.collect();
	let cells: &[&[usize]] = if d == 2 {
		&[&[0, 1, 2]]
	} else {
		&[&[0, 1, 2, 3]]
	};
	PhysicalSpace::from_mixed_mesh(
		MixedAffineMeshView {
			dimension: d,
			vertices: &v[..=d],
			cells,
			exterior: &exterior,
		},
		nu,
		p,
		PhysicalMeshLimits::default(),
	)
}
#[test]
fn all_open_constraint_rows_are_independent_and_every_coordinate_is_retained()
-> Result<(), Box<dyn std::error::Error>> {
	for (d, p, n, r, m) in [
		(2, 1, 6, 5, 1),
		(2, 2, 12, 9, 3),
		(3, 1, 12, 10, 2),
		(3, 2, 30, 22, 8),
	] {
		let s = simplex(d, p, 0.01)?;
		assert_eq!(
			(
				s.diagnostics().local_velocity_dimension,
				s.constraint_count(),
				s.diagnostics().constraint_rank,
				s.dimension()
			),
			(n, r, r, m)
		);
		for q in s.chart() {
			let a = s.coordinates(q)?;
			let b = s.coefficients(&a)?;
			assert!(b.iter().zip(q).all(|(x, y)| (x - y).abs() < 1e-10));
		}
		assert_eq!(s.dirichlet_facets()?.len(), d);
		assert_eq!(s.natural_traction_facets()?.len(), 1);
	}
	Ok(())
}
#[test]
fn natural_cubic_energy_flux_has_both_outflow_and_backflow_signs()
-> Result<(), Box<dyn std::error::Error>> {
	for p in [1, 2] {
		let s = simplex(2, p, 0.)?;
		let nodes = s.velocity_nodes()?;
		let width = nodes[0].len();
		for a in [0.2_f64, -0.2] {
			let mut u = vec![0.; width * 2];
			for (i, x) in nodes[0].iter().enumerate() {
				u[i] = a * x[0];
				u[width + i] = -a * x[1];
			}
			let state = s.coordinates(&u)?;
			assert!(
				s.normal_trace_residual(&state)? < 1e-10,
				"natural flux is not a homogeneous trace constraint"
			);
			assert!(s.reconstruct_pressure_general(&state)?.continuity_residual < 1e-10);
			let f = s.drift(&state)?;
			let adv = s.advective_drift_reference(&state)?;
			assert!(f.iter().zip(adv).all(|(x, y)| (x - y).abs() < 1e-11));
			let power = state.iter().zip(f).map(|(x, y)| x * y).sum::<f64>();
			assert!(
				(power + a.powi(3) / 2.).abs() < 1e-11,
				"p={p},a={a}:power={power}"
			);
		}
	}
	Ok(())
}
#[test]
#[allow(
	clippy::too_many_lines,
	clippy::many_single_char_names,
	reason = "One independent two-cell fixture compares complete rank, rational mass and the separate BDM1 engine"
)]
fn unequal_cells_keep_all_coordinates_and_match_independent_p1_assembly()
-> Result<(), Box<dyn std::error::Error>> {
	for d in [2, 3] {
		let vertices = if d == 2 {
			vec![[0., 0., 0.], [2., 0., 0.], [1., 1., 0.], [0., 1., 0.]]
		} else {
			vec![
				[0., 0., 0.],
				[1., 0., 0.],
				[0., 1., 0.],
				[0., 0., 1.],
				[0., 0., -2.],
			]
		};
		let cells: Vec<Vec<usize>> = if d == 2 {
			vec![vec![0, 1, 2], vec![0, 2, 3]]
		} else {
			vec![vec![0, 1, 2, 3], vec![0, 2, 1, 4]]
		};
		let facets: Vec<Vec<usize>> = if d == 2 {
			vec![vec![0, 1], vec![1, 2], vec![2, 3], vec![3, 0]]
		} else {
			vec![
				vec![0, 1, 3],
				vec![0, 2, 3],
				vec![1, 2, 3],
				vec![0, 1, 4],
				vec![0, 2, 4],
				vec![1, 2, 4],
			]
		};
		let exterior: Vec<_> = facets
			.iter()
			.enumerate()
			.map(|(i, v)| ExteriorFacet {
				vertices: v,
				label: if i == 0 { "natural" } else { "wall" },
				condition: if i == 0 {
					ExteriorCondition::NaturalMechanicalTraction
				} else {
					ExteriorCondition::Dirichlet
				},
			})
			.collect();
		let refs: Vec<_> = cells.iter().map(Vec::as_slice).collect();
		for p in [1, 2] {
			let s = PhysicalSpace::from_mixed_mesh(
				MixedAffineMeshView {
					dimension: d,
					vertices: &vertices,
					cells: &refs,
					exterior: &exterior,
				},
				0.03,
				p,
				PhysicalMeshLimits::default(),
			)?;
			let (r, m) = match (d, p) {
				(2, 1) => (10, 2),
				(2, 2) => (18, 6),
				(3, 1) => (20, 4),
				_ => (44, 16),
			};
			assert_eq!(
				(
					s.constraint_count(),
					s.diagnostics().constraint_rank,
					s.dimension()
				),
				(r, r, m)
			);
			let scalar = s.local_velocity_per_cell() / d;
			let volumes = if d == 2 {
				[1., 0.5]
			} else {
				[1. / 6., 1. / 3.]
			};
			for (a, q) in s.chart().iter().enumerate() {
				for (b, v) in s.chart().iter().enumerate() {
					let mut mass = 0.;
					for (c, volume) in volumes.iter().enumerate() {
						for axis in 0..d {
							let offset = (c * d + axis) * scalar;
							for i in 0..scalar {
								for j in 0..scalar {
									mass += volume
										* q[offset + i]
										* scalar_mass(d, p, i, j)
										* v[offset + j];
								}
							}
						}
					}
					assert!((mass - f64::from(a == b)).abs() < 2e-10);
				}
			}
			let coordinates = vec![0.01; m];
			let drift = s.drift(&coordinates)?;
			assert!(drift.iter().all(|x| x.is_finite()));
			let pressure = s.reconstruct_pressure_general(&coordinates)?;
			assert!(pressure.momentum_residual < 1e-9);
			assert!(pressure.normalization_residual.is_none());
			if p == 1 {
				let old_facets: Vec<_> = facets
					.iter()
					.enumerate()
					.map(|(i, v)| quest_cfd::simplex::BoundaryFacet {
						vertices: v.clone(),
						velocity: if i == 0 { None } else { Some(vec![[0.; 3]; d]) },
						label: if i == 0 {
							"natural".into()
						} else {
							"wall".into()
						},
					})
					.collect();
				let old = quest_cfd::simplex::SimplexBdm::from_mesh(
					d,
					&vertices,
					&cells,
					&old_facets,
					&[],
					0.03,
				)?;
				let a = vec![0.013; old.dimension()];
				let same = s.coordinates(&old.coefficients(&a)?)?;
				let x = s.coefficients(&s.drift(&same)?)?;
				let y = old.coefficients(&old.drift(&a)?)?;
				assert!(x.iter().zip(y).all(|(x, y)| (x - y).abs() < 1e-8));
				let x = s.reconstruct_pressure_general(&same)?;
				let y = old.reconstruct_pressure(&a)?;
				assert!(
					x.pressure_coefficients
						.iter()
						.zip(y.cell_pressure)
						.all(|(x, y)| (x[0] - y).abs() < 1e-8)
				);
			}
		}
	}
	Ok(())
}
#[test]
fn mixed_classification_and_attempt_admission_are_explicit()
-> Result<(), Box<dyn std::error::Error>> {
	let vertices = [[0., 0., 0.], [2., 0., 0.], [0., 1., 0.]];
	let cells: &[&[usize]] = &[&[0, 1, 2]];
	let mut exterior = [
		ExteriorFacet {
			vertices: &[0, 1],
			label: "a",
			condition: ExteriorCondition::Dirichlet,
		},
		ExteriorFacet {
			vertices: &[0, 2],
			label: "b",
			condition: ExteriorCondition::Dirichlet,
		},
		ExteriorFacet {
			vertices: &[1, 2],
			label: "n",
			condition: ExteriorCondition::NaturalMechanicalTraction,
		},
	];
	let view = MixedAffineMeshView {
		dimension: 2,
		vertices: &vertices,
		cells,
		exterior: &exterior,
	};
	let attempt = PhysicalSpace::from_mixed_mesh_with_receipt(
		view,
		0.01,
		2,
		PhysicalMeshLimits {
			max_geometry_work: 1,
			..Default::default()
		},
	);
	assert!(attempt.outcome.is_err());
	assert!(
		attempt
			.resources
			.is_some_and(|r| r.constraint_rows == 9 && r.constraint_rank.is_none())
	);
	let source = PhysicalSpace::from_mixed_mesh(
		view,
		0.01,
		2,
		PhysicalMeshLimits {
			max_pressure_work: 1,
			..Default::default()
		},
	)?;
	assert!(
		source
			.reconstruct_pressure_general(&vec![0.; source.dimension()])
			.is_err()
	);
	for kind in [
		ExteriorCondition::Dirichlet,
		ExteriorCondition::NaturalMechanicalTraction,
	] {
		for f in &mut exterior {
			f.condition = kind;
		}
		assert!(
			PhysicalSpace::from_mixed_mesh(
				MixedAffineMeshView {
					dimension: 2,
					vertices: &vertices,
					cells,
					exterior: &exterior
				},
				0.01,
				2,
				PhysicalMeshLimits::default()
			)
			.is_err()
		);
	}
	exterior[0].condition = ExteriorCondition::Dirichlet;
	exterior[1].vertices = exterior[0].vertices;
	assert!(
		PhysicalSpace::from_mixed_mesh(
			MixedAffineMeshView {
				dimension: 2,
				vertices: &vertices,
				cells,
				exterior: &exterior
			},
			0.01,
			2,
			PhysicalMeshLimits::default()
		)
		.is_err()
	);
	Ok(())
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
fn natural_face_has_no_spurious_sip_penalty() -> Result<(), Box<dyn std::error::Error>> {
	for p in [1, 2] {
		let s = simplex(2, p, 0.03)?;
		let scalar = s.local_velocity_per_cell() / 2;
		let mut u = vec![0.; 2 * scalar];
		let a = 0.2_f64;
		for (i, x) in s.velocity_nodes()?[0].iter().enumerate() {
			u[i] = a * x[0];
			u[scalar + i] = -a * x[1];
		}
		let state = s.coordinates(&u)?;
		let power = state
			.iter()
			.zip(s.drift(&state)?)
			.map(|(x, y)| x * y)
			.sum::<f64>();
		// Exact monomial integrals: volume grad energy=2a²; x/y-axis D penalties
		// integrate x² and y² to8/3 and1/3. Both consistency contractions vanish.
		let sip = if p == 1 { 346. / 3. } else { 257. };
		let expected = -a.powi(3) / 2. - 0.03 * sip * a * a;
		assert!(
			(power - expected).abs() < 1e-10,
			"p={p}: {power} vs {expected}"
		);
	}
	Ok(())
}
