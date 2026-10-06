#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	clippy::panic_in_result_fn,
	reason = "Independent analytic flux fixtures use admitted small tensor indices"
)]
use mathcore::{
	RBig,
	exact::{Owner, Symbol},
	multivariate::{PolynomialLimits, SparsePolynomial},
};
use quest_cfd::{
	configuration::ConfigurationGrid,
	configuration_flux::{ConfigurationFluxLimits, boundary_flux},
	polynomial::PolynomialOde,
};
use quest_numerics::Complex64;

fn constant_ode(values: &[i32]) -> Result<PolynomialOde, quest_cfd::CfdError> {
	let symbols: Vec<_> = (0..values.len())
		.map(|i| {
			u64::try_from(i)
				.map(|word| Symbol::new(Owner::new(975), word))
				.map_err(|_| quest_cfd::CfdError::InvalidInput("fixture symbol index"))
		})
		.collect::<Result<Vec<_>, _>>()?;
	let forms = values
		.iter()
		.map(|&v| {
			SparsePolynomial::from_terms(
				symbols.clone(),
				[(vec![0; values.len()], RBig::from(v))],
				PolynomialLimits::default(),
			)
		})
		.collect::<Result<Vec<_>, _>>()?;
	PolynomialOde::from_polynomials(forms, values.len(), PolynomialLimits::default())
}
fn uniform_density(grid: &ConfigurationGrid) -> Result<Vec<Complex64>, quest_cfd::CfdError> {
	(0..grid.dimension())
		.map(|i| {
			grid.weight(i)
				.ok_or(quest_cfd::CfdError::Assembly("fixture tensor weight"))
				.map(|w| Complex64::new(w.sqrt(), 0.))
		})
		.collect()
}
fn near(actual: f64, expected: f64) {
	assert!(
		(actual - expected).abs() < 2e-12,
		"actual={actual}, expected={expected}"
	);
}
fn ode_from_terms(
	m: usize,
	timed: bool,
	terms: Vec<Vec<(Vec<u32>, i32)>>,
) -> Result<PolynomialOde, quest_cfd::CfdError> {
	let symbols: Vec<_> = (0..m + usize::from(timed))
		.map(|i| {
			u64::try_from(i)
				.map(|word| Symbol::new(Owner::new(976), word))
				.map_err(|_| quest_cfd::CfdError::InvalidInput("fixture symbol index"))
		})
		.collect::<Result<Vec<_>, _>>()?;
	let forms = terms
		.into_iter()
		.map(|component| {
			SparsePolynomial::from_terms(
				symbols.clone(),
				component.into_iter().map(|(p, c)| (p, RBig::from(c))),
				PolynomialLimits::default(),
			)
		})
		.collect::<Result<Vec<_>, _>>()?;
	PolynomialOde::from_polynomials(forms, m, PolynomialLimits::default())
}

#[test]
fn constant_density_has_exterior_flux_not_outer_cell_occupation()
-> Result<(), Box<dyn std::error::Error>> {
	let ode = constant_ode(&[2, -3])?;
	for (cells, order) in [(1, 1), (1, 2), (3, 1), (3, 2)] {
		let grid = ConfigurationGrid::uniform(2, -1., 1., cells, order, 128)?;
		let z = uniform_density(&grid)?;
		let result = boundary_flux(&grid, &ode, 0., &z, ConfigurationFluxLimits::default())?;
		near(result.probability, 4.);
		near(result.outward_rate, 2.5);
		near(result.inward_rate, 2.5);
		near(result.net_rate, 0.);
		near(result.axes[0].lower.inward_rate, 1.);
		near(result.axes[0].upper.outward_rate, 1.);
		near(result.axes[1].lower.outward_rate, 1.5);
		near(result.axes[1].upper.inward_rate, 1.5);
		for axis in &result.axes {
			near(axis.lower.normalized_trace, 0.5);
			near(axis.upper.normalized_trace, 0.5);
		}
		let n = grid.axis_dimension();
		assert_eq!(
			result.resources.drift_calls,
			grid.dimension() - (n - 2).pow(2)
		);
		assert_eq!(result.resources.face_samples, 4 * n);
		near(
			result.outer_cell_occupation,
			grid.boundary_mass(&z)? / result.probability,
		);
	}
	Ok(())
}

#[test]
fn explicit_caps_reject_before_any_drift() -> Result<(), Box<dyn std::error::Error>> {
	let grid = ConfigurationGrid::uniform(2, -1., 1., 2, 2, 36)?;
	let ode = constant_ode(&[2, -3])?;
	let z = uniform_density(&grid)?;
	for limits in [
		ConfigurationFluxLimits {
			max_bytes: 1,
			..ConfigurationFluxLimits::default()
		},
		ConfigurationFluxLimits {
			max_work: 1,
			..ConfigurationFluxLimits::default()
		},
		ConfigurationFluxLimits {
			max_drift_calls: 1,
			..ConfigurationFluxLimits::default()
		},
	] {
		assert!(boundary_flux(&grid, &ode, 0., &z, limits).is_err());
	}
	Ok(())
}

#[test]
fn full_compressive_drift_and_explicit_time_are_preserved() -> Result<(), Box<dyn std::error::Error>>
{
	let m = 3;
	let ode = ode_from_terms(
		m,
		false,
		(0..m)
			.map(|axis| {
				let mut powers = vec![0; m];
				powers[axis] = 1;
				vec![(powers, -2)]
			})
			.collect(),
	)?;
	let grid = ConfigurationGrid::uniform(m, -1., 1., 2, 2, 216)?;
	let result = boundary_flux(
		&grid,
		&ode,
		0.,
		&uniform_density(&grid)?,
		ConfigurationFluxLimits::default(),
	)?;
	near(result.outward_rate, 0.);
	near(result.inward_rate, 6.);
	near(result.net_rate, -6.);
	assert_eq!(result.axes.len(), m);
	for axis in &result.axes {
		near(axis.inward_rate, 2.);
	}
	let timed = ode_from_terms(1, true, vec![vec![(vec![0, 0], 1), (vec![0, 2], 1)]])?;
	let grid = ConfigurationGrid::uniform(1, -1., 1., 1, 2, 3)?;
	for time in [0., 2.] {
		let result = boundary_flux(
			&grid,
			&timed,
			time,
			&uniform_density(&grid)?,
			ConfigurationFluxLimits::default(),
		)?;
		near(result.outward_rate, f64::midpoint(1., time * time));
		near(result.inward_rate, result.outward_rate);
		near(result.time, time);
	}
	Ok(())
}

#[test]
fn nonlinear_cross_coordinate_drift_reaches_the_fifth_axis()
-> Result<(), Box<dyn std::error::Error>> {
	let mut terms = vec![Vec::new(); 5];
	terms[4] = vec![(vec![2, 0, 0, 0, 0], 1), (vec![0, 0, 0, 0, 1], 2)];
	let ode = ode_from_terms(5, false, terms)?;
	let grid = ConfigurationGrid::uniform(5, -1., 1., 1, 2, 243)?;
	let result = boundary_flux(
		&grid,
		&ode,
		0.,
		&uniform_density(&grid)?,
		ConfigurationFluxLimits::default(),
	)?;
	assert_eq!(result.axes.len(), 5);
	for axis in &result.axes[..4] {
		near(axis.outward_rate, 0.);
		near(axis.inward_rate, 0.);
	}
	// Integrating a0² on each last-coordinate face gives 1/3.
	near(result.axes[4].lower.outward_rate, 5. / 6.);
	near(result.axes[4].upper.outward_rate, 7. / 6.);
	near(result.outward_rate, 2.);
	near(result.inward_rate, 0.);
	assert_eq!(result.resources.drift_calls, 242);
	assert_eq!(result.resources.face_samples, 810);
	Ok(())
}

#[test]
fn polynomial_density_uses_face_quadrature_and_ignores_complex_phase()
-> Result<(), Box<dyn std::error::Error>> {
	let grid = ConfigurationGrid::uniform(2, -1., 1., 3, 2, 81)?;
	let ode = constant_ode(&[2, -3])?;
	let z: Vec<_> = (0..grid.dimension())
		.map(|i| {
			let a = grid.point(i).unwrap();
			let rho = (1. + a[0] * a[0]) * (1. + 2. * a[1] * a[1]);
			Complex64::new((grid.weight(i).unwrap() * rho).sqrt(), 0.)
		})
		.collect();
	let expected = boundary_flux(&grid, &ode, 0., &z, ConfigurationFluxLimits::default())?;
	near(expected.probability, 80. / 9.);
	near(expected.axes[0].lower.normalized_trace, 0.75);
	near(expected.axes[0].upper.normalized_trace, 0.75);
	near(expected.axes[1].lower.normalized_trace, 0.9);
	near(expected.axes[1].upper.normalized_trace, 0.9);
	near(expected.outward_rate, 4.2);
	near(expected.inward_rate, 4.2);
	let phased: Vec<_> = z
		.iter()
		.enumerate()
		.map(|(i, &v)| {
			// Exact unit phases avoid a trigonometric normalization comparison.
			v * [
				Complex64::new(1., 0.),
				Complex64::new(0., 1.),
				Complex64::new(-1., 0.),
				Complex64::new(0., -1.),
			][i % 4]
		})
		.collect();
	let actual = boundary_flux(&grid, &ode, 0., &phased, ConfigurationFluxLimits::default())?;
	near(actual.probability, expected.probability);
	near(actual.outward_rate, expected.outward_rate);
	near(actual.inward_rate, expected.inward_rate);
	near(actual.outer_cell_occupation, expected.outer_cell_occupation);
	Ok(())
}

#[test]
fn duplicate_internal_facets_are_not_exterior_traces() -> Result<(), Box<dyn std::error::Error>> {
	let grid = ConfigurationGrid::uniform(1, -1., 1., 3, 1, 6)?;
	let ode = constant_ode(&[2])?;
	let mut z = vec![Complex64::new(0., 0.); 6];
	z[1] = Complex64::new(1., 0.);
	z[2] = Complex64::new(0., 1.);
	near(grid.axis_node(1).unwrap(), grid.axis_node(2).unwrap());
	let result = boundary_flux(&grid, &ode, 0., &z, ConfigurationFluxLimits::default())?;
	near(result.probability, 2.);
	near(result.outer_cell_occupation, 0.5);
	near(result.outward_rate, 0.);
	near(result.inward_rate, 0.);
	near(result.axes[0].lower.normalized_trace, 0.);
	near(result.axes[0].upper.normalized_trace, 0.);
	assert_eq!(
		result.resources.drift_calls, 2,
		"zero-amplitude exterior nodes still evaluate complete drift"
	);
	assert_eq!(result.resources.face_samples, 2);
	Ok(())
}

#[test]
fn ledger_exact_caps_and_accessible_slice_capacity_policy() -> Result<(), Box<dyn std::error::Error>>
{
	let grid = ConfigurationGrid::uniform(2, -1., 1., 2, 2, 36)?;
	let ode = constant_ode(&[2, -3])?;
	let mut z = uniform_density(&grid)?;
	let initial = boundary_flux(&grid, &ode, 0., &z, ConfigurationFluxLimits::default())?;
	let r = initial.resources;
	assert_eq!(
		r.total_work,
		r.constructor_validation_work + r.traversal_work + r.query_work
	);
	assert_eq!(
		r.peak_bytes,
		r.retained_input_bytes + r.accessible_state_bytes + r.result_bytes + r.scratch_bytes
	);
	assert_eq!(r.accessible_state_bytes, z.len() * size_of::<Complex64>());
	let exact = ConfigurationFluxLimits {
		max_bytes: r.peak_bytes,
		max_work: r.total_work,
		max_drift_calls: r.drift_calls,
	};
	assert!(boundary_flux(&grid, &ode, 0., &z, exact).is_ok());
	for limits in [
		ConfigurationFluxLimits {
			max_bytes: r.peak_bytes - 1,
			..exact
		},
		ConfigurationFluxLimits {
			max_work: r.total_work - 1,
			..exact
		},
		ConfigurationFluxLimits {
			max_drift_calls: r.drift_calls - 1,
			..exact
		},
	] {
		assert!(boundary_flux(&grid, &ode, 0., &z, limits).is_err());
	}
	z.reserve(4096);
	assert!(z.capacity() > z.len());
	let spare = boundary_flux(&grid, &ode, 0., &z, exact)?;
	assert_eq!(
		spare.resources.peak_bytes, r.peak_bytes,
		"inaccessible Vec spare capacity belongs to caller accounting"
	);
	near(spare.outward_rate, initial.outward_rate);
	Ok(())
}

#[test]
fn malformed_norm_shape_time_and_kernel_errors_are_rejected()
-> Result<(), Box<dyn std::error::Error>> {
	let grid = ConfigurationGrid::uniform(1, -1., 1., 1, 2, 3)?;
	let ode = constant_ode(&[1])?;
	let z = uniform_density(&grid)?;
	let l = ConfigurationFluxLimits::default();
	for bad in [
		vec![Complex64::new(0., 0.); 3],
		vec![Complex64::new(f64::NAN, 0.); 3],
		vec![Complex64::new(f64::MAX, 0.); 3],
		vec![Complex64::new(1., 0.); 2],
	] {
		assert!(boundary_flux(&grid, &ode, 0., &bad, l).is_err());
	}
	assert!(boundary_flux(&grid, &ode, f64::NAN, &z, l).is_err());
	assert!(boundary_flux(&grid, &constant_ode(&[1, 2])?, 0., &z, l).is_err());
	let timed = ode_from_terms(1, true, vec![vec![(vec![0, 2], 1)]])?;
	assert!(boundary_flux(&grid, &timed, f64::MAX, &z, l).is_err());
	// This time would overflow prepared-kernel evaluation. The tiny admission
	// ceiling must reject before querying that kernel instead.
	let error = boundary_flux(
		&grid,
		&timed,
		f64::MAX,
		&z,
		ConfigurationFluxLimits { max_work: 1, ..l },
	)
	.unwrap_err();
	assert!(matches!(
		error,
		quest_cfd::CfdError::InvalidInput("configuration flux validation/traversal work budget")
	));
	for (limits, expected) in [
		(
			ConfigurationFluxLimits { max_bytes: 1, ..l },
			"KvN retained/query storage budget",
		),
		(
			ConfigurationFluxLimits {
				max_drift_calls: 1,
				..l
			},
			"configuration flux drift-call budget",
		),
	] {
		let error = boundary_flux(&grid, &timed, f64::MAX, &z, limits).unwrap_err();
		assert!(matches!(error, quest_cfd::CfdError::InvalidInput(message) if message == expected));
	}
	Ok(())
}
