//! Independently integrated full lifted stresses and quadratic observables.
#![allow(
	clippy::panic_in_result_fn,
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	clippy::suboptimal_flops,
	reason = "Tiny manufactured triangle fields have exact displayed mass, curl and weighted-normal integrals"
)]
use quest_cfd::physical_space::{
	BoundaryLimits, BoundaryTimeCoefficient, ExteriorCondition, ExteriorFacet,
	MechanicalTractionLimits, MixedAffineMeshView, PhysicalMeshLimits, PhysicalSpace,
	PolynomialBoundary,
};
#[test]
fn exact_label_force_and_full_lifted_energy_use_original_field()
-> Result<(), Box<dyn std::error::Error>> {
	let vertices = [[0., 0., 0.], [2., 0., 0.], [0., 1., 0.]];
	let exterior = [
		ExteriorFacet {
			vertices: &[0, 1],
			label: "wall",
			condition: ExteriorCondition::Dirichlet,
		},
		ExteriorFacet {
			vertices: &[0, 2],
			label: "wall",
			condition: ExteriorCondition::Dirichlet,
		},
		ExteriorFacet {
			vertices: &[1, 2],
			label: "natural",
			condition: ExteriorCondition::NaturalMechanicalTraction,
		},
	];
	for p in [1, 2] {
		let space = PhysicalSpace::from_mixed_mesh(
			MixedAffineMeshView {
				dimension: 2,
				vertices: &vertices,
				cells: &[&[0, 1, 2]],
				exterior: &exterior,
			},
			0.02,
			p,
			PhysicalMeshLimits::default(),
		)?;
		for rotation in [false, true] {
			let field = |x: [f64; 3]| {
				if rotation {
					[-x[1], x[0], 0.]
				} else {
					[x[0], -x[1], 0.]
				}
			};
			let mut c = BoundaryTimeCoefficient::zero(&space)?;
			let nodes = space.velocity_nodes()?;
			let scalar = nodes[0].len();
			for (i, x) in nodes[0].iter().enumerate() {
				let v = field(*x);
				c.lifting[i] = v[0];
				c.lifting[scalar + i] = v[1];
			}
			for (values, f) in c.prescribed.iter_mut().zip(space.dirichlet_facets()?) {
				for (v, x) in values.iter_mut().zip(f.nodes) {
					*v = field(x);
				}
			}
			let flow = PolynomialBoundary::new(&space, vec![c], BoundaryLimits::default())?;
			let a = vec![0.; space.dimension()];
			assert!((flow.energy(0., &a)? - 5. / 12.).abs() < 1e-11);
			assert!((flow.enstrophy(0., &a)? - if rotation { 2. } else { 0. }).abs() < 1e-11);
			let pressure = vec![vec![2.5; if p == 1 { 1 } else { 3 }]];
			let force = flow.boundary_force_on_label_with_limits(
				0.,
				&a,
				&pressure,
				"natural",
				MechanicalTractionLimits::default(),
			)?;
			let expected = if rotation {
				[2.54, 4.98, 0.]
			} else {
				[2.48, 5.04, 0.]
			};
			assert!(
				force
					.iter()
					.zip(expected)
					.all(|(x, y)| (x - y).abs() < 1e-11)
			);
			assert!(
				flow.boundary_force_on_label_with_limits(
					0.,
					&a,
					&pressure,
					"missing",
					MechanicalTractionLimits::default()
				)
				.is_err()
			);
			assert!(
				flow.boundary_force_on_label_with_limits(
					0.,
					&a,
					&pressure,
					"natural",
					MechanicalTractionLimits {
						max_work: 1,
						..Default::default()
					}
				)
				.is_err()
			);
		}
	}
	Ok(())
}
