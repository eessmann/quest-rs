use quest_numerics::constraint_chart::CellWhitening;
#[test]
fn whitening_roundtrip_and_dual_force() {
	let w = CellWhitening::new(2, vec![4., 2., 2., 5.], 32).unwrap();
	let u = [2., -3.];
	let mut z = u;
	w.velocity_to_coordinates(&mut z).unwrap();
	assert!((z[0] - 1.).abs() < 1e-14);
	assert!((z[1] + 6.).abs() < 1e-14);
	w.coordinates_to_velocity(&mut z).unwrap();
	assert!((z[0] - u[0]).abs() < 1e-14 && (z[1] - u[1]).abs() < 1e-14);
	let mut f = [7., -2.];
	w.force_to_coordinates(&mut f).unwrap();
	w.coordinates_to_force(&mut f).unwrap();
	assert!((f[0] - 7.).abs() < 1e-14 && (f[1] + 2.).abs() < 1e-14);
}
#[test]
fn rejects_non_spd_shape_symmetry_and_capacity() {
	for a in [
		vec![1., 2., 2., 1.],
		vec![1., 0., 1., 1.],
		vec![f64::NAN, 0., 0., 1.],
	] {
		assert!(CellWhitening::new(2, a, 32).is_err());
	}
	assert!(CellWhitening::new(2, vec![1., 0., 0., 1.], 31).is_err());
	let mut a = Vec::with_capacity(100);
	a.extend([1., 0., 0., 1.]);
	assert!(CellWhitening::new(2, a, 32).is_err());
}
