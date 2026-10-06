#![allow(
	clippy::unwrap_used,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Bounded independent flux, viscosity and convergence references use direct indexed arithmetic; construction failures fail the test"
)]
use quest_cfd::burgers::BurgersDg;
#[test]
fn full_dg1_burgers_retains_eight_modes_and_conserves_convective_energy() {
	let dg = BurgersDg::new(4, 1, 0.).unwrap();
	assert_eq!(dg.dimension(), 8);
	let state = [0.2, -0.1, 0.4, 0.3, -0.2, 0.7, -0.4, 0.1];
	let drift = dg.drift(&state).unwrap();
	assert!(drift.iter().map(|v| v * v).sum::<f64>() > 1e-4);
	assert!(
		state
			.iter()
			.zip(drift)
			.map(|(a, b)| a * b)
			.sum::<f64>()
			.abs()
			< 1e-11
	);
}
#[test]
fn polynomial_matches_independent_split_flux_and_sip_in_both_orders() {
	for p in [1, 2] {
		let model = BurgersDg::new(3, p, 0.1).unwrap();
		let state = (0..model.dimension())
			.map(|i| f64::from(u32::try_from(i).unwrap() + 1).sin() * 0.03)
			.collect::<Vec<_>>();
		let compiled = model.drift(&state).unwrap();
		let direct = model.direct_drift(&state).unwrap();
		for (a, b) in compiled.iter().zip(direct) {
			assert!((a - b).abs() < 1e-12);
		}
		let jac = model
			.polynomial_ode()
			.jacobian(0., &vec![0.; model.dimension()])
			.unwrap();
		for i in 0..model.dimension() {
			for j in 0..model.dimension() {
				assert!((jac[i][j] - jac[j][i]).abs() < 1e-12);
			}
		}
		assert!(state.iter().zip(&compiled).map(|(a, b)| a * b).sum::<f64>() < 0.);
	}
}
#[test]
fn sip_quadratic_form_has_correct_left_and_right_boundary_normals() {
	let viscous = BurgersDg::new(3, 1, 0.1).unwrap();
	let inviscid = BurgersDg::new(3, 1, 0.).unwrap();
	let h = 1. / 3.;
	let u = [0.1, 0.3, -0.2, 0.4, 0.7, -0.5];
	let root = (h / 2_f64).sqrt();
	let state = u.iter().map(|v| v * root).collect::<Vec<_>>();
	let a = viscous.drift(&state).unwrap();
	let b = inviscid.drift(&state).unwrap();
	let contraction = -state
		.iter()
		.zip(a.iter().zip(b))
		.map(|(s, (x, y))| s * (x - y))
		.sum::<f64>()
		/ 0.1;
	let slopes = [(u[1] - u[0]) / h, (u[3] - u[2]) / h, (u[5] - u[4]) / h];
	let penalty = 16. / h;
	let mut expected = h * slopes.iter().map(|g| g * g).sum::<f64>();
	for (jump, gradient) in [
		(-u[0], slopes[0]),
		(u[1] - u[2], f64::midpoint(slopes[0], slopes[1])),
		(u[3] - u[4], f64::midpoint(slopes[1], slopes[2])),
		(u[5], slopes[2]),
	] {
		expected += (penalty * jump).mul_add(jump, -2. * gradient * jump);
	}
	assert!((contraction - expected).abs() < 1e-12);
	assert!(expected > 0.);
}
#[test]
fn cole_hopf_space_order_and_time_refinement_are_independent() {
	let mut errors = Vec::new();
	for (cells, p) in [(4, 1), (8, 1), (4, 2)] {
		let model = BurgersDg::new(cells, p, 0.1).unwrap();
		let initial = model.initial_state(0.01).unwrap();
		let result = model
			.polynomial_ode()
			.integrate_rk4(&initial, 0.00005, 2000)
			.unwrap();
		let error = model.l2_error(&result, 0.1, 0.01).unwrap();
		eprintln!("Burgers cells={cells} p={p} error={error}");
		errors.push(error);
	}
	assert!(errors[1] < errors[0] / 3.);
	assert!(errors[2] < errors[0] / 5.);
	let model = BurgersDg::new(4, 1, 0.1).unwrap();
	let ode = model.polynomial_ode();
	let initial = model.initial_state(0.01).unwrap();
	let reference = ode.integrate_rk4(&initial, 0.00005, 2000).unwrap();
	let mut temporal = Vec::new();
	for steps in [8u32, 16, 32] {
		let state = ode
			.integrate_rk4(&initial, 0.1 / f64::from(steps), steps)
			.unwrap();
		let error = state
			.iter()
			.zip(&reference)
			.map(|(a, b)| (a - b).powi(2))
			.sum::<f64>()
			.sqrt();
		eprintln!("Burgers steps={steps} time error={error}");
		temporal.push(error);
	}
	assert!(temporal[1] < temporal[0] / 8.);
	assert!(temporal[2] < temporal[1] / 8.);
}
