#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	clippy::suboptimal_flops,
	reason = "Bounded independent full physical boundary identities preserve the displayed reference arithmetic"
)]
use quest_cfd::{
	CfdError,
	carleman_recipe::{CarlemanRecipeLimits, StatelessCarleman},
	physical_space::{BoundaryLimits, BoundaryTimeCoefficient, PhysicalSpace, PolynomialBoundary},
	simplex::BoxBoundary,
	stream_history::{HistoryStreamLimits, TemporalHistoryRecipe},
};

#[test]
#[allow(
	clippy::too_many_lines,
	reason = "Keep the complete extraction, actual temporal RHS and failure-admission checks on one shared fixture"
)]
fn full_polynomial_time_extraction_matches_direct_drift_and_temporal_quadrature()
-> Result<(), Box<dyn std::error::Error>> {
	use mathcore::multivariate::PolynomialLimits;
	use quest_cfd::{physical_space::BoundaryExtractionLimits, polynomial::PolynomialOde};
	let space = PhysicalSpace::box_mesh(2, 1, 1., 0.02, BoxBoundary::Cavity { lid_speed: 0. }, 2)?;
	let mut modes = vec![BoundaryTimeCoefficient::zero(&space)?; 4];
	for (k, c) in modes.iter_mut().enumerate() {
		if k > 0 {
			c.lifting = space.chart()[0]
				.iter()
				.map(|v| 0.1 * f64::from(u32::try_from(k).unwrap_or(0)) * v)
				.collect();
		}
		if k == 2 {
			c.body_force = space.chart()[1].iter().map(|v| 0.3 * v).collect();
		}
		for (values, face) in c.prescribed.iter_mut().zip(space.boundary_facets()?) {
			if face.normal[1] > 0.9 {
				for v in values {
					v[0] = [0.2, 0.1, -0.2, 0.05][k];
				}
			}
		}
	}
	let problem = PolynomialBoundary::new(&space, modes, BoundaryLimits::default())?;
	let snapshot =
		PolynomialOde::from_polynomial_boundary(&problem, BoundaryExtractionLimits::default())?;
	assert_eq!(snapshot.dynamics.dimension(), space.dimension());
	assert_eq!(snapshot.dynamics.symbols().len(), space.dimension() + 1);
	assert!(snapshot.evidence.independent_probe_scaled_error < 1e-9);
	let state = (0..space.dimension())
		.map(|i| 0.03 * f64::from(u32::try_from(i + 1).unwrap_or(0)).cos())
		.collect::<Vec<_>>();
	// Includes actual DG1 endpoints and DG2 midpoint, with separate physical time.
	for t in [-0.5, 0., 0.25, 0.5, 0.75, 1.3] {
		let a = problem.drift(t, &state)?;
		let b = snapshot.dynamics.drift(t, &state)?;
		assert!(
			a.iter()
				.zip(&b)
				.map(|(x, y)| (x - y).abs())
				.fold(0., f64::max)
				< 2e-10
		);
	}

	let hierarchy = StatelessCarleman::new(
		std::sync::Arc::new(snapshot.dynamics),
		1,
		1.,
		CarlemanRecipeLimits::default(),
	)?;
	for (order, times, masses) in [
		(1, vec![0., 0.3], vec![0.15, 0.15]),
		(2, vec![0., 0.15, 0.3], vec![0.05, 0.2, 0.05]),
	] {
		let history =
			TemporalHistoryRecipe::new(&hierarchy, 0.3, 1, order, HistoryStreamLimits::default())?;
		for (a, (&time, &mass)) in times.iter().zip(&masses).enumerate() {
			let direct = problem.drift(time, &vec![0.; space.dimension()])?;
			for (axis, value) in direct.iter().enumerate() {
				let row = a * space.dimension() + hierarchy.physical_coordinate_index(axis)?;
				assert!(
					(history
						.rhs_value(row, |_| Ok(quest_numerics::Complex64::default()))?
						.re
						- mass * value)
						.abs()
						< 2e-10
				);
			}
		}
	}
	assert!(
		PolynomialOde::from_polynomial_boundary(
			&problem,
			BoundaryExtractionLimits {
				max_work: 1,
				..Default::default()
			}
		)
		.is_err()
	);
	assert!(
		PolynomialOde::from_polynomial_boundary(
			&problem,
			BoundaryExtractionLimits {
				max_bytes: 1,
				..Default::default()
			}
		)
		.is_err()
	);
	assert!(
		PolynomialOde::from_polynomial_boundary(
			&problem,
			BoundaryExtractionLimits {
				polynomial: PolynomialLimits {
					max_terms: 1,
					..Default::default()
				},
				..Default::default()
			}
		)
		.is_err()
	);
	Ok(())
}

#[test]
fn periodic_time_body_source_and_last_complete_coordinate_are_preserved()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_cfd::{physical_space::BoundaryExtractionLimits, polynomial::PolynomialOde};
	let space = PhysicalSpace::box_mesh(3, 1, 1., 0.01, BoxBoundary::Periodic, 2)?;
	assert_eq!(space.dimension(), 85);
	let last = space.dimension() - 1;
	let mut data = vec![BoundaryTimeCoefficient::zero(&space)?; 4];
	data[1].body_force = space.chart()[last].iter().map(|v| 0.3 * v).collect();
	data[3].lifting = space.chart()[last].iter().map(|v| 0.2 * v).collect();
	let problem = PolynomialBoundary::new(&space, data, BoundaryLimits::default())?;
	let state = (0..space.dimension())
		.map(|i| 0.01 * f64::from(u32::try_from(i + 1).unwrap_or(0)).sin())
		.collect::<Vec<_>>();
	let t = 0.4;
	let mut shifted = state.clone();
	shifted[last] += 0.2 * t * t * t;
	let mut expected = space.drift(&shifted)?;
	expected[last] += 0.3 * t - 0.6 * t * t;
	let actual = problem.drift(t, &state)?;
	assert!(
		actual
			.iter()
			.zip(&expected)
			.map(|(a, b)| (a - b).abs())
			.fold(0., f64::max)
			< 1e-10
	);
	assert!(problem.reconstruct_pressure(t, &state)?.momentum_residual < 1e-9);
	// Complete numerical queries remain supported; no reduced chart is substituted
	// when the explicitly bounded polynomial snapshot cannot admit all85 coordinates.
	assert!(
		PolynomialOde::from_polynomial_boundary(&problem, BoundaryExtractionLimits::default())
			.is_err()
	);
	Ok(())
}

fn linear_mode(space: &PhysicalSpace) -> Result<BoundaryTimeCoefficient, CfdError> {
	let velocity = |p: [f64; 3]| [p[0] + 0.3, -p[1], 0.];
	let mut lifting = Vec::new();
	for nodes in space.velocity_nodes()? {
		for component in 0..space.physical_dimension() {
			lifting.extend(nodes.iter().map(|&p| velocity(p)[component]));
		}
	}
	let prescribed = space
		.boundary_facets()?
		.iter()
		.map(|face| face.nodes.iter().map(|&p| velocity(p)).collect())
		.collect();
	Ok(BoundaryTimeCoefficient {
		body_force: vec![0.; lifting.len()],
		lifting,
		prescribed,
	})
}

#[test]
fn complete_time_lifting_retains_original_momentum_and_nonzero_normal_boundary_work()
-> Result<(), Box<dyn std::error::Error>> {
	for dimension in [2, 3] {
		for order in [1, 2] {
			let space = PhysicalSpace::box_mesh(
				dimension,
				1,
				1.,
				0.,
				BoxBoundary::Cavity { lid_speed: 0. },
				order,
			)?;
			let mut mode = linear_mode(&space)?;
			// This homogeneous part must remain in the supplied lifting; removing it
			// would hide the -Q^T M ell_dot term.
			for (ell, q) in mode.lifting.iter_mut().zip(&space.chart()[0]) {
				*ell += 0.2 * q;
			}
			let zero = BoundaryTimeCoefficient::zero(&space)?;
			let problem =
				PolynomialBoundary::new(&space, vec![zero, mode], BoundaryLimits::default())?;
			let state = vec![0.; space.dimension()];
			for time in [0., 0.25, 0.5] {
				assert!(problem.continuity_residual(time, &state)? < 1e-11);
				let report = problem.reconstruct_pressure(time, &state)?;
				assert!(report.momentum_residual < 1e-9);
				assert!(report.gauge_residual < 1e-11);
				assert_eq!(
					report.pressure_coefficients[0].len(),
					if order == 1 { 1 } else { dimension + 1 }
				);
				let power = problem.convection_power(time, &state)?;
				assert!((power.force_power - power.boundary_power).abs() < 1e-10);
			}
			assert!(problem.convection_power(0.5, &state)?.boundary_power.abs() > 1e-4);
			let at_zero = problem.drift(0., &state)?;
			assert!(at_zero[0].abs() > 0.1);
			let power = problem.convection_power(0.5, &state)?;
			eprintln!(
				"boundary d={dimension} p={order} complete={} power={:.12e} boundary={:.12e} ell_dot_coordinate0={:.12e} resources={}",
				problem.dimension(),
				power.force_power,
				power.boundary_power,
				at_zero[0],
				serde_json::to_string(&problem.resources())?
			);
			assert_eq!(problem.dimension(), space.dimension());
		}
	}
	Ok(())
}

#[test]
fn polynomial_lid_body_force_and_lifting_derivative_match_independent_complete_assemblies()
-> Result<(), Box<dyn std::error::Error>> {
	for d in [2, 3] {
		for p in [1, 2] {
			let space =
				PhysicalSpace::box_mesh(d, 1, 1., 0.02, BoxBoundary::Cavity { lid_speed: 9. }, p)?;
			let layout = space.boundary_facets()?;
			let mut modes = Vec::new();
			for k in 0..3 {
				let mut c = BoundaryTimeCoefficient::zero(&space)?;
				for (values, face) in c.prescribed.iter_mut().zip(&layout) {
					if face.normal[1] > 0.9 {
						for v in values {
							v[0] = [0.4, 0.3, 0.2][k];
						}
					}
				}
				if k == 1 {
					c.body_force = space.chart()[0].iter().map(|v| 0.7 * v).collect();
				}
				if k == 2 {
					c.lifting = space.chart()[0].iter().map(|v| 0.2 * v).collect();
				}
				modes.push(c);
			}
			let problem = PolynomialBoundary::new(&space, modes, BoundaryLimits::default())?;
			let state = (0..space.dimension())
				.map(|i| 0.02 * f64::from(u32::try_from(i + 1).unwrap_or(0)).sin())
				.collect::<Vec<_>>();
			for t in [0., 0.25, 0.5, 1.] {
				let reference = PhysicalSpace::box_mesh(
					d,
					1,
					1.,
					0.02,
					BoxBoundary::Cavity {
						lid_speed: 0.4 + 0.3 * t + 0.2 * t * t,
					},
					p,
				)?;
				let mut shifted = state.clone();
				shifted[0] += 0.2 * t * t;
				let mut expected = reference.drift(&shifted)?;
				expected[0] += 0.3 * t;
				let actual = problem.drift(t, &state)?;
				assert!(
					actual
						.iter()
						.zip(&expected)
						.map(|(a, b)| (a - b).abs())
						.fold(0., f64::max)
						< 2e-10,
					"d={d} p={p} t={t}"
				);
				let pressure = problem.reconstruct_pressure(t, &state)?;
				assert!(pressure.momentum_residual < 2e-9);
				assert!(pressure.continuity_residual < 2e-10);
				assert!(pressure.gauge_residual < 2e-11);
				assert_eq!(
					pressure.pressure_coefficients[0].len(),
					if p == 1 { 1 } else { d + 1 }
				);
			}
			assert!(problem.resources().borrowed_space_bytes >= space.retained_bytes()?);
			assert!(
				problem.resources().peak_bytes
					> problem.resources().retained_bytes + problem.resources().borrowed_space_bytes
			);
		}
	}
	Ok(())
}

#[test]
#[allow(
	clippy::too_many_lines,
	reason = "One immutable model exercises independent malformed fields, actual capacities and query overflow admission"
)]
fn boundary_rejects_incompatible_data_and_resources_without_projection()
-> Result<(), Box<dyn std::error::Error>> {
	let space = PhysicalSpace::box_mesh(2, 1, 1., 0.01, BoxBoundary::Cavity { lid_speed: 0. }, 2)?;
	let mut c = linear_mode(&space)?;
	c.prescribed[0][0][0] += 0.5;
	assert!(PolynomialBoundary::new(&space, vec![c], BoundaryLimits::default()).is_err());
	let mut c = linear_mode(&space)?;
	c.lifting[0] += 0.2;
	assert!(PolynomialBoundary::new(&space, vec![c], BoundaryLimits::default()).is_err());
	let mut c = linear_mode(&space)?;
	c.body_force[0] = f64::NAN;
	assert!(PolynomialBoundary::new(&space, vec![c], BoundaryLimits::default()).is_err());
	for limits in [
		BoundaryLimits {
			max_bytes: 1,
			..Default::default()
		},
		BoundaryLimits {
			max_construction_work: 1,
			..Default::default()
		},
		BoundaryLimits {
			max_drift_work: 1,
			..Default::default()
		},
		BoundaryLimits {
			max_pressure_work: 1,
			..Default::default()
		},
		BoundaryLimits {
			max_time_degree: 0,
			..Default::default()
		},
	] {
		assert!(
			PolynomialBoundary::new(
				&space,
				vec![BoundaryTimeCoefficient::zero(&space)?, linear_mode(&space)?],
				limits
			)
			.is_err()
		);
	}
	let problem = PolynomialBoundary::new(
		&space,
		vec![BoundaryTimeCoefficient::zero(&space)?, linear_mode(&space)?],
		BoundaryLimits::default(),
	)?;
	let baseline = PolynomialBoundary::new(
		&space,
		vec![BoundaryTimeCoefficient::zero(&space)?],
		BoundaryLimits::default(),
	)?;
	let mut extra = BoundaryTimeCoefficient::zero(&space)?;
	extra.lifting.try_reserve_exact(1024)?;
	let increased =
		PolynomialBoundary::new(&space, vec![extra.clone()], BoundaryLimits::default())?;
	// Clone may shrink a Vec; admission must charge the supplied original capacity.
	let allocated = PolynomialBoundary::new(&space, vec![extra], BoundaryLimits::default())?;
	assert!(allocated.resources().retained_bytes >= baseline.resources().retained_bytes + 8192);
	assert!(allocated.resources().peak_bytes > baseline.resources().peak_bytes);
	assert!(increased.resources().peak_bytes <= allocated.resources().peak_bytes);
	let mut extra = BoundaryTimeCoefficient::zero(&space)?;
	extra.lifting.try_reserve_exact(1024)?;
	assert!(
		PolynomialBoundary::new(
			&space,
			vec![extra],
			BoundaryLimits {
				max_bytes: baseline.resources().peak_bytes,
				..Default::default()
			}
		)
		.is_err()
	);
	assert!(
		problem
			.drift(f64::NAN, &vec![0.; space.dimension()])
			.is_err()
	);
	assert!(problem.drift(0., &[]).is_err());
	let mut overflow = BoundaryTimeCoefficient::zero(&space)?;
	for (values, face) in overflow.prescribed.iter_mut().zip(space.boundary_facets()?) {
		if face.normal[1] > 0.9 {
			for v in values {
				v[0] = 1e308;
			}
		}
	}
	let overflow = PolynomialBoundary::new(
		&space,
		vec![BoundaryTimeCoefficient::zero(&space)?, overflow],
		BoundaryLimits::default(),
	)?;
	assert!(
		overflow
			.continuity_residual(2., &vec![0.; space.dimension()])
			.is_err()
	);
	Ok(())
}
