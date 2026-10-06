//! Versioned rectangle source construction; coalescing is an angular approximation policy.
use super::{BoundaryFacet, CfdError, PlanarMesh, ring_cells};
pub(super) const POLICY: &str = "explicit-rectangle-corner-priority-v1";
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct ExplicitGeometryEvidence {
	pub source_policy: &'static str,
	pub requested_sectors: u32,
	pub retained_segments: usize,
	pub coalesced_rays: usize,
	pub maximum_coalesced_angle: f64,
	pub represented_rectangle_residual: f64,
}
#[derive(Clone, Copy)]
struct Ray {
	angle: f64,
	direction: [f64; 2],
	corner: Option<[f64; 2]>,
	ordinal: usize,
}
const fn invalid() -> CfdError {
	CfdError::InvalidInput("explicit rectangle ray construction")
}
#[allow(
	clippy::too_many_lines,
	clippy::float_cmp,
	reason = "One bounded versioned construction sequence; exact represented-side identity is required, never a tolerance label"
)]
pub fn explicit_planar(
	angular: u32,
	layers: u32,
) -> Result<(PlanarMesh, ExplicitGeometryEvidence), CfdError> {
	if !(4..=64).contains(&angular) || layers == 0 || layers > 8 {
		return Err(invalid());
	}
	let center = [0.2, 0.2];
	let radius = 0.05;
	let bounds = [0., 2.2, 0., 0.41];
	let tau = std::f64::consts::TAU;
	let mut rays = Vec::new();
	for i in 0..angular {
		let angle = tau * f64::from(i) / f64::from(angular);
		let direction = if (8 * i) % angular == 0 {
			match (8 * i / angular) % 8 {
				0 => [1., 0.],
				1 => [1., 1.],
				2 => [0., 1.],
				3 => [-1., 1.],
				4 => [-1., 0.],
				5 => [-1., -1.],
				6 => [0., -1.],
				_ => [1., -1.],
			}
		} else {
			[angle.cos(), angle.sin()]
		};
		rays.push(Ray {
			angle,
			direction,
			corner: None,
			ordinal: usize::try_from(i).map_err(|_| invalid())?,
		});
	}
	for x in [bounds[0], bounds[1]] {
		for y in [bounds[2], bounds[3]] {
			let direction = [x - center[0], y - center[1]];
			rays.push(Ray {
				angle: f64::atan2(direction[1], direction[0]).rem_euclid(tau),
				direction,
				corner: Some([x, y]),
				ordinal: rays.len(),
			});
		}
	}
	rays.sort_by(|a, b| {
		a.angle
			.total_cmp(&b.angle)
			.then_with(|| a.corner.is_none().cmp(&b.corner.is_none()))
			.then(a.ordinal.cmp(&b.ordinal))
	});
	let mut retained: Vec<Ray> = Vec::new();
	let mut removed = 0;
	let mut maximum = 0_f64;
	for ray in rays {
		if let Some(previous) = retained.last_mut() {
			let separation = ray.angle - previous.angle;
			if separation <= 1e-12 {
				if previous.corner.is_some() && ray.corner.is_some() {
					return Err(invalid());
				}
				removed += 1;
				maximum = maximum.max(separation);
				if ray.corner.is_some() {
					*previous = ray;
				}
				continue;
			}
		}
		retained.push(ray);
	}
	if retained
		.first()
		.zip(retained.last())
		.is_some_and(|(a, b)| a.angle + tau - b.angle <= 1e-12)
	{
		return Err(invalid());
	}
	let count = retained.len();
	let mut inner = Vec::new();
	let mut outer = Vec::new();
	for ray in &retained {
		let direction = ray.direction;
		let length = direction[0].hypot(direction[1]);
		inner.push([
			center[0] + radius * direction[0] / length,
			center[1] + radius * direction[1] / length,
			0.,
		]);
		let point = if let Some(corner) = ray.corner {
			corner
		} else {
			let hit_x = if direction[0] > 0. {
				bounds[1]
			} else {
				bounds[0]
			};
			let hit_y = if direction[1] > 0. {
				bounds[3]
			} else {
				bounds[2]
			};
			let rx = if direction[0] == 0. {
				f64::INFINITY
			} else {
				(hit_x - center[0]) / direction[0]
			};
			let ry = if direction[1] == 0. {
				f64::INFINITY
			} else {
				(hit_y - center[1]) / direction[1]
			};
			if rx < ry {
				[hit_x, center[1] + rx * direction[1]]
			} else if ry < rx {
				[center[0] + ry * direction[0], hit_y]
			} else {
				return Err(invalid());
			}
		};
		if point.iter().any(|v| !v.is_finite())
			|| point[0] < bounds[0]
			|| point[0] > bounds[1]
			|| point[1] < bounds[2]
			|| point[1] > bounds[3]
		{
			return Err(invalid());
		}
		outer.push([point[0], point[1], 0.]);
	}
	let mut vertices = Vec::new();
	for layer in 0..=layers {
		let t = f64::from(layer) / f64::from(layers);
		for (i, o) in inner.iter().zip(&outer) {
			vertices.push(if layer == 0 {
				*i
			} else if layer == layers {
				*o
			} else {
				[i[0] * (1. - t) + o[0] * t, i[1] * (1. - t) + o[1] * t, 0.]
			});
		}
	}
	let layers = usize::try_from(layers).map_err(|_| invalid())?;
	let mut boundaries = Vec::new();
	let mut deviation = 0_f64;
	for i in 0..count {
		let next = (i + 1) % count;
		let delta = (retained[next].angle - retained[i].angle).rem_euclid(tau);
		deviation = deviation.max(radius * (1. - (delta * 0.5).cos()));
		boundaries.push(BoundaryFacet {
			vertices: vec![i, next],
			velocity: Some(vec![[0.; 3]; 2]),
			label: "cylinder".into(),
		});
		let a = outer[i];
		let b = outer[next];
		let label = if a[0] == bounds[0] && b[0] == bounds[0] {
			"inlet"
		} else if a[0] == bounds[1] && b[0] == bounds[1] {
			"outlet"
		} else if (a[1] == bounds[2] && b[1] == bounds[2])
			|| (a[1] == bounds[3] && b[1] == bounds[3])
		{
			"far-wall"
		} else {
			return Err(invalid());
		};
		boundaries.push(BoundaryFacet {
			vertices: vec![layers * count + i, layers * count + next],
			velocity: if label == "outlet" {
				None
			} else {
				Some(vec![[0.; 3]; 2])
			},
			label: label.into(),
		});
	}
	Ok((
		PlanarMesh {
			vertices,
			cells: ring_cells(count, layers),
			boundaries,
			deviation,
			segments: count,
		},
		ExplicitGeometryEvidence {
			source_policy: POLICY,
			requested_sectors: angular,
			retained_segments: count,
			coalesced_rays: removed,
			maximum_coalesced_angle: maximum,
			represented_rectangle_residual: 0.,
		},
	))
}
#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	#[allow(
		clippy::float_cmp,
		clippy::panic_in_result_fn,
		reason = "The source policy promises exact represented side assignment and unchanged legacy bits"
	)]
	fn explicit_sides_and_corner_priority_preserve_legacy_source() -> Result<(), CfdError> {
		for (sectors, expected) in [
			(4, 17_304_938_616_133_229_079),
			(8, 8_025_335_988_921_218_776),
			(16, 17_361_619_031_758_027_556),
		] {
			let old = super::super::planar("shedding2d", sectors, 1)?;
			assert_eq!(
				crate::cylinder_high_order::fingerprint(&old, "legacy-planar-v1")?,
				expected
			);
		}
		for (sectors, count, removed) in [(4, 8, 0), (8, 11, 1)] {
			let (mesh, evidence) = explicit_planar(sectors, 1)?;
			assert_eq!((mesh.segments, evidence.coalesced_rays), (count, removed));
			let outer = &mesh.vertices[count..];
			for p in [[0., 0., 0.], [0., 0.41, 0.], [2.2, 0., 0.], [2.2, 0.41, 0.]] {
				assert!(outer.contains(&p));
			}
			for f in &mesh.boundaries {
				if f.label == "cylinder" {
					continue;
				}
				let a = mesh.vertices[f.vertices[0]];
				let b = mesh.vertices[f.vertices[1]];
				assert!(
					a[0] == 0. && b[0] == 0.
						|| a[0] == 2.2 && b[0] == 2.2
						|| a[1] == 0. && b[1] == 0.
						|| a[1] == 0.41 && b[1] == 0.41
				);
			}
		}
		Ok(())
	}
}
