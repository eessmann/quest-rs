//! Fixed classical secondary-plane and reflection diagnostics for the unit cube.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Fixed 810-point stencil, checked sample counts and rejected nonfinite reductions"
)]
use crate::{CfdError, cases::VelocityProbe};

/// Finite sampled diagnostics, not a volume norm, steady-state or symmetry certificate.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Cavity3dObservations {
	/// Interior 9 by 9 yz-plane probes at x=1/2; retain all velocity components.
	pub x_midplane: Vec<VelocityProbe>,
	/// Interior 9 by 9 xz-plane probes at y=1/2; retain all velocity components.
	pub y_midplane: Vec<VelocityProbe>,
	/// Scalar velocity queries, including both ends of every reflected pair.
	pub sample_queries: usize,
	pub reflection_pairs: usize,
	/// RMS of vector defects `(u-u_reflected, v-v_reflected, w+w_reflected)`.
	pub reflection_rms_defect: f64,
	/// Maximum Euclidean vector defect on the same pairs.
	pub reflection_maximum_defect: f64,
	/// RMS w over the 162 plane probes, including the duplicated intersection line.
	pub sampled_spanwise_velocity_rms: f64,
}

/// Sample the unit-cube cavity on two transverse planes and across z reflection.
///
/// Points on each plane have their varying coordinates in {0.1,...,0.9}.
/// Reflection pairs have x,y in that set, z in {0.1,...,0.4}, and partner 1-z.
/// The callback runs once on the complete fixed stencil. Its physical-state,
/// DG trace convention, evaluation work, retained storage and scratch belong to
/// the caller. Production box references use their existing first-cell trace.
/// This helper adds fixed classical diagnostics and does not implement a coherent
/// observable or certify any physical symmetry, stationary state or circulation.
/// # Errors
/// Rejects allocation failure, callback failure, wrong sample count, nonfinite
/// velocities or unrepresentable reflection statistics.
pub fn cavity_3d_probes(
	sample: impl FnOnce(&[[f64; 3]]) -> Result<Vec<[f64; 3]>, CfdError>,
) -> Result<Cavity3dObservations, CfdError> {
	const PLANE: usize = 81;
	const PAIRS: usize = 324;
	const QUERIES: usize = 2 * PLANE + 2 * PAIRS;
	let mut points = Vec::new();
	points
		.try_reserve_exact(QUERIES)
		.map_err(|_| CfdError::Assembly("cavity probe allocation"))?;
	for plane in 0..2 {
		for i in 1..10 {
			for j in 1..10 {
				let a = f64::from(i) / 10.;
				let b = f64::from(j) / 10.;
				points.push(if plane == 0 { [0.5, a, b] } else { [a, 0.5, b] });
			}
		}
	}
	for i in 1..10 {
		for j in 1..10 {
			for k in 1..5 {
				let x = f64::from(i) / 10.;
				let y = f64::from(j) / 10.;
				let z = f64::from(k) / 10.;
				points.extend([[x, y, z], [x, y, 1. - z]]);
			}
		}
	}
	let velocities = sample(&points)?;
	if velocities.len() != QUERIES || velocities.iter().flatten().any(|v| !v.is_finite()) {
		return Err(CfdError::InvalidInput("cavity probe count or velocity"));
	}
	let mut planes = [Vec::new(), Vec::new()];
	let mut spanwise = 0_f64;
	for (plane, output) in planes.iter_mut().enumerate() {
		output
			.try_reserve_exact(PLANE)
			.map_err(|_| CfdError::Assembly("cavity plane allocation"))?;
		for offset in 0..PLANE {
			let index = plane * PLANE + offset;
			output.push(VelocityProbe {
				point: points[index],
				velocity: velocities[index],
			});
			spanwise = spanwise.hypot(velocities[index][2] / 162_f64.sqrt());
		}
	}
	let (mut rms, mut maximum) = (0_f64, 0_f64);
	for pair in velocities[2 * PLANE..].as_chunks::<2>().0 {
		let defect = [
			pair[0][0] - pair[1][0],
			pair[0][1] - pair[1][1],
			pair[0][2] + pair[1][2],
		];
		let mut norm = 0_f64;
		for component in defect {
			norm = norm.hypot(component);
			rms = rms.hypot(component / 18.);
		}
		maximum = maximum.max(norm);
	}
	if [spanwise, rms, maximum].iter().any(|v| !v.is_finite()) {
		return Err(CfdError::InvalidInput("cavity probe reduction overflow"));
	}
	let [x_midplane, y_midplane] = planes;
	Ok(Cavity3dObservations {
		x_midplane,
		y_midplane,
		sample_queries: QUERIES,
		reflection_pairs: PAIRS,
		reflection_rms_defect: rms,
		reflection_maximum_defect: maximum,
		sampled_spanwise_velocity_rms: spanwise,
	})
}
