//! Independent original-momentum manufactured traction and time-source checks.
#![allow(
	clippy::panic_in_result_fn,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Bounded manufactured fixture algebra and test assertions"
)]
use quest_cfd::physical_space::{
	BoundaryLimits, BoundaryTimeCoefficient, ExteriorCondition, ExteriorFacet, MixedAffineMeshView,
	NaturalTractionTimeCoefficient, PhysicalMeshLimits, PhysicalSpace, PolynomialBoundary,
	PressureNormalization,
};
fn space(d: usize, p: usize) -> Result<PhysicalSpace, quest_cfd::CfdError> {
	space_with_limits(d, p, PhysicalMeshLimits::default())
}
fn space_with_limits(
	d: usize,
	p: usize,
	limits: PhysicalMeshLimits,
) -> Result<PhysicalSpace, quest_cfd::CfdError> {
	let vertices = [[0., 0., 0.], [2., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
	let ids: &[&[usize]] = if d == 2 {
		&[&[0, 1], &[0, 2], &[1, 2]]
	} else {
		&[&[0, 1, 2], &[0, 1, 3], &[0, 2, 3], &[1, 2, 3]]
	};
	let exterior: Vec<_> = ids
		.iter()
		.enumerate()
		.map(|(i, v)| ExteriorFacet {
			vertices: v,
			label: if i == d { "natural" } else { "wall" },
			condition: if i == d {
				ExteriorCondition::NaturalMechanicalTraction
			} else {
				ExteriorCondition::Dirichlet
			},
		})
		.collect();
	PhysicalSpace::from_mixed_mesh(
		MixedAffineMeshView {
			dimension: d,
			vertices: &vertices[..=d],
			cells: if d == 2 {
				&[&[0, 1, 2]]
			} else {
				&[&[0, 1, 2, 3]]
			},
			exterior: &exterior,
		},
		0.02,
		p,
		limits,
	)
}
#[test]
fn prescribed_traction_fixes_nonzero_constant_pressure_without_mean_subtraction()
-> Result<(), Box<dyn std::error::Error>> {
	for d in [2, 3] {
		for p in [1, 2] {
			let s = space(d, p)?;
			let modes = vec![BoundaryTimeCoefficient::zero(&s)?];
			let mut tau = NaturalTractionTimeCoefficient::zero(&s)?;
			for (values, face) in tau.values.iter_mut().zip(s.natural_traction_facets()?) {
				for value in values {
					*value = face.normal.map(|n| -2.5 * n);
				}
			}
			let flow = PolynomialBoundary::with_natural_traction(
				&s,
				modes,
				vec![tau],
				BoundaryLimits::default(),
			)?;
			let a = vec![0.; s.dimension()];
			assert!(flow.drift(0.37, &a)?.iter().all(|x| x.abs() < 1e-10));
			let report = flow.reconstruct_pressure_general(0.37, &a)?;
			assert_eq!(
				report.normalization,
				PressureNormalization::PrescribedMechanicalTraction
			);
			assert!(report.normalization_residual.is_none());
			assert!(report.pressure_integral > 0.);
			assert!(
				report
					.pressure_coefficients
					.iter()
					.flatten()
					.all(|x| (x - 2.5).abs() < 1e-9)
			);
			assert!(report.momentum_residual < 1e-10);
			assert!(flow.reconstruct_pressure(0.37, &a).is_err());
		}
	}
	Ok(())
}
#[test]
fn polynomial_extension_uses_original_acceleration_and_all_natural_load_modes()
-> Result<(), Box<dyn std::error::Error>> {
	for d in [2, 3] {
		for p in [1, 2] {
			let s = space(d, p)?;
			let nodes = s.velocity_nodes()?;
			let width = nodes[0].len();
			let mut c = vec![BoundaryTimeCoefficient::zero(&s)?; 3];
			for (i, x) in nodes[0].iter().enumerate() {
				c[1].lifting[i] = x[0];
				c[1].lifting[width + i] = -x[1];
				c[0].body_force[i] = x[0];
				c[0].body_force[width + i] = -x[1];
				c[2].body_force[i] = x[0];
				c[2].body_force[width + i] = x[1];
			}
			for (values, f) in c[1].prescribed.iter_mut().zip(s.dirichlet_facets()?) {
				for (v, x) in values.iter_mut().zip(f.nodes) {
					*v = [x[0], -x[1], 0.];
				}
			}
			let mut tau = vec![NaturalTractionTimeCoefficient::zero(&s)?; 3];
			for (values, f) in tau[0].values.iter_mut().zip(s.natural_traction_facets()?) {
				for v in values {
					*v = f.normal.map(|n| -2. * n);
				}
			}
			for (values, f) in tau[1].values.iter_mut().zip(s.natural_traction_facets()?) {
				for v in values {
					*v = [
						0.02 * f.normal[0] - 3. * f.normal[0],
						-0.02 * f.normal[1] - 3. * f.normal[1],
						-3. * f.normal[2],
					];
				}
			}
			let flow =
				PolynomialBoundary::with_natural_traction(&s, c, tau, BoundaryLimits::default())?;
			for time in [0.13, 0.71] {
				let mut a = vec![0.; s.dimension()];
				let drift = flow.drift(time, &a)?;
				assert!(
					drift.iter().all(|v| v.abs() < 1e-9),
					"d={d},p={p}: {drift:?}"
				);
				let pressure = flow.reconstruct_pressure_general(time, &a)?;
				assert!(
					pressure
						.pressure_coefficients
						.iter()
						.flatten()
						.all(|v| (v - (2. + 3. * time)).abs() < 1e-8)
				);
				assert!(pressure.momentum_residual < 1e-9);
				assert!(pressure.continuity_residual < 1e-10);
				let last = a.len() - 1;
				a[last] = 0.17;
				let u = flow.coefficients_at(time, &a)?;
				let zero = flow.coefficients_at(time, &vec![0.; a.len()])?;
				assert!(
					u.iter()
						.zip(zero)
						.zip(&s.chart()[last])
						.all(|((x, y), q)| (x - y - 0.17 * q).abs() < 1e-10)
				);
			}
		}
	}
	Ok(())
}
#[test]
fn affine_pressure_gradient_is_recovered_without_closed_gauge()
-> Result<(), Box<dyn std::error::Error>> {
	for d in [2, 3] {
		for p in [1, 2] {
			let s = space(d, p)?;
			let mut c = BoundaryTimeCoefficient::zero(&s)?;
			let width = s.velocity_nodes()?[0].len();
			for axis in 0..d {
				for i in 0..width {
					c.body_force[axis * width + i] = if axis == 0 { 0.5 } else { 1. };
				}
			}
			let flow = PolynomialBoundary::new(&s, vec![c], BoundaryLimits::default())?;
			let a = vec![0.; s.dimension()];
			assert!(flow.drift(0., &a)?.iter().all(|v| v.abs() < 1e-10));
			let pressure = flow.reconstruct_pressure_general(0., &a)?;
			let expected = if p == 1 {
				vec![-1. / f64::from(u32::try_from(d + 1)?)]
			} else {
				let mut x = vec![0.; d + 1];
				x[0] = -1.;
				x
			};
			assert!(
				pressure.pressure_coefficients[0]
					.iter()
					.zip(expected)
					.all(|(x, y)| (x - y).abs() < 1e-9)
			);
		}
	}
	Ok(())
}
#[test]
fn canonical_batch_lifting_solves_full_constraints_and_is_mass_orthogonal()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::physical_space::CanonicalLiftingLimits;
	for d in [2, 3] {
		for p in [1, 2] {
			let s = space(d, p)?;
			let mut traces = vec![BoundaryTimeCoefficient::zero(&s)?.prescribed; 2];
			for (mode, facets) in traces.iter_mut().enumerate() {
				let scale = if mode == 0 { 0.3 } else { -0.7 };
				for (values, f) in facets.iter_mut().zip(s.dirichlet_facets()?) {
					for (v, x) in values.iter_mut().zip(f.nodes) {
						*v = [scale * (1. + x[0]), -scale * x[1], 0.];
					}
				}
			}
			let result = s.canonical_liftings(&traces, CanonicalLiftingLimits::default())?;
			assert_eq!(result.coefficients.len(), 2);
			assert!(result.constraint_residual < 1e-10);
			assert!(result.mass_orthogonality_residual < 1e-10);
			let mut modes = Vec::new();
			for (lifting, prescribed) in result.coefficients.into_iter().zip(&traces) {
				let n = lifting.len();
				let mass = s.cell_mass_matrix(0)?;
				let ml: Vec<_> = mass
					.chunks(n)
					.map(|r| r.iter().zip(&lifting).map(|(x, y)| x * y).sum::<f64>())
					.collect();
				assert!(
					s.chart().iter().all(|q| q
						.iter()
						.zip(&ml)
						.map(|(x, y)| x * y)
						.sum::<f64>()
						.abs()
						< 1e-10)
				);
				modes.push(BoundaryTimeCoefficient {
					lifting,
					prescribed: prescribed.clone(),
					body_force: vec![0.; s.diagnostics().local_velocity_dimension],
				});
			}
			let flow = PolynomialBoundary::new(&s, modes, BoundaryLimits::default())?;
			assert!(flow.continuity_residual(0.39, &vec![0.; s.dimension()])? < 1e-10);
			assert!(
				s.canonical_liftings(
					&traces,
					CanonicalLiftingLimits {
						max_work: result.resources.work - 1,
						..Default::default()
					}
				)
				.is_err()
			);
			assert!(
				s.canonical_liftings(
					&traces,
					CanonicalLiftingLimits {
						max_bytes: result.resources.peak_bytes - 1,
						..Default::default()
					}
				)
				.is_err()
			);
			let mut excess = traces.clone();
			excess[0][0].reserve(1000);
			let tight = CanonicalLiftingLimits {
				max_bytes: result.resources.peak_bytes,
				..Default::default()
			};
			assert!(s.canonical_liftings(&excess, tight).is_err());
		}
	}
	Ok(())
}
#[test]
fn natural_capacity_shapes_and_pressure_limits_are_enforced()
-> Result<(), Box<dyn std::error::Error>> {
	let s = space(2, 2)?;
	let c = vec![BoundaryTimeCoefficient::zero(&s)?];
	let tau = vec![NaturalTractionTimeCoefficient::zero(&s)?];
	let baseline = PolynomialBoundary::with_natural_traction(
		&s,
		c.clone(),
		tau.clone(),
		BoundaryLimits::default(),
	)?;
	let cap = baseline.resources().peak_bytes;
	let mut excess = tau.clone();
	excess[0].values[0].reserve(10_000);
	assert!(
		PolynomialBoundary::with_natural_traction(
			&s,
			c.clone(),
			excess,
			BoundaryLimits {
				max_bytes: cap,
				..Default::default()
			}
		)
		.is_err()
	);
	let mut malformed = tau.clone();
	malformed[0].values[0].pop();
	assert!(
		PolynomialBoundary::with_natural_traction(
			&s,
			c.clone(),
			malformed,
			BoundaryLimits::default()
		)
		.is_err()
	);
	let mut nonfinite = tau.clone();
	nonfinite[0].values[0][0][0] = f64::NAN;
	assert!(
		PolynomialBoundary::with_natural_traction(
			&s,
			c.clone(),
			nonfinite,
			BoundaryLimits::default()
		)
		.is_err()
	);
	assert!(
		PolynomialBoundary::with_natural_traction(
			&s,
			c.clone(),
			vec![tau[0].clone(); 2],
			BoundaryLimits::default()
		)
		.is_err()
	);
	assert!(
		PolynomialBoundary::with_natural_traction(
			&s,
			c.clone(),
			tau,
			BoundaryLimits {
				max_pressure_work: 1,
				..Default::default()
			}
		)
		.is_err()
	);
	let mut traces = vec![c[0].prescribed.clone()];
	traces[0][0][0][0] = f64::INFINITY;
	assert!(
		s.canonical_liftings(
			&traces,
			quest_cfd::physical_space::CanonicalLiftingLimits::default()
		)
		.is_err()
	);
	let closed =
		PhysicalSpace::box_mesh(2, 1, 1., 0.01, quest_cfd::simplex::BoxBoundary::Periodic, 2)?;
	let a = vec![0.01; closed.dimension()];
	let old = closed.reconstruct_pressure(&a)?;
	let general = closed.reconstruct_pressure_general(&a)?;
	assert_eq!(old.pressure_coefficients, general.pressure_coefficients);
	assert_eq!(general.normalization, PressureNormalization::ZeroVolumeMean);
	assert_eq!(general.normalization_residual, Some(old.gauge_residual));
	Ok(())
}
#[test]
fn mixed_time_loads_extract_to_complete_shared_polynomial_drift()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::{physical_space::BoundaryExtractionLimits, polynomial::PolynomialOde};
	let s = space(2, 2)?;
	let mut modes = vec![BoundaryTimeCoefficient::zero(&s)?; 2];
	modes[1].body_force = s.chart()[s.dimension() - 1].clone();
	let mut tau = vec![NaturalTractionTimeCoefficient::zero(&s)?; 2];
	for v in &mut tau[1].values[0] {
		*v = [0.2, -0.1, 0.];
	}
	let flow =
		PolynomialBoundary::with_natural_traction(&s, modes, tau, BoundaryLimits::default())?;
	let ode = PolynomialOde::from_polynomial_boundary(&flow, BoundaryExtractionLimits::default())?;
	assert_eq!(ode.dynamics.dimension(), s.dimension());
	let a = vec![0.013; s.dimension()];
	for t in [0.13, 0.71] {
		let direct = flow.drift(t, &a)?;
		let extracted = ode.dynamics.drift(t, &a)?;
		assert!(
			direct
				.iter()
				.zip(extracted)
				.all(|(a, b)| (a - b).abs() < 1e-9)
		);
	}
	Ok(())
}

#[test]
fn time_owner_admission_includes_declared_live_mesh_external_storage()
-> Result<(), Box<dyn std::error::Error>> {
	let plain = space(2, 1)?;
	let plain_flow = PolynomialBoundary::new(
		&plain,
		vec![BoundaryTimeCoefficient::zero(&plain)?],
		BoundaryLimits::default(),
	)?;
	let external = 64 * 1024 * 1024;
	let with_external = space_with_limits(
		2,
		1,
		PhysicalMeshLimits {
			external_retained_bytes: external,
			..Default::default()
		},
	)?;
	let modes = vec![BoundaryTimeCoefficient::zero(&with_external)?];
	let flow = PolynomialBoundary::new(&with_external, modes.clone(), BoundaryLimits::default())?;
	assert_eq!(flow.resources().external_retained_bytes, Some(external));
	assert!(plain_flow.resources().external_retained_bytes.is_none());
	assert!(
		flow.resources().peak_bytes >= plain_flow.resources().peak_bytes + external,
		"declared external storage must remain live in the time owner peak"
	);
	assert!(
		PolynomialBoundary::new(
			&with_external,
			modes,
			BoundaryLimits {
				max_bytes: plain_flow.resources().peak_bytes,
				..Default::default()
			}
		)
		.is_err()
	);
	Ok(())
}
