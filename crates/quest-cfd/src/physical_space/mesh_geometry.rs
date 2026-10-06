//! Shared exact pair predicates plus outward floating conditioning admission.
#![allow(
	clippy::float_cmp,
	reason = "Box planes and paired source coordinates require exact represented-value equality, not tolerance inference"
)]
#![allow(
	clippy::too_many_lines,
	reason = "Aggregate geometry admission and exact queries retain their explicit ordering"
)]

use super::mesh::{
	AffineMeshView, Counts, PhysicalMeshLimits, PhysicalMeshResources, add, invalid, mul,
};
use super::mesh_topology::FacetPlan;
use crate::CfdError;
use mathcore::geometry::{DyadicScale, GeometryLimits};
use quest_numerics::Interval;
fn quality(points: &[[f64; 3]], d: usize) -> Result<f64, CfdError> {
	let z = Interval::point(0.)?;
	let mut edges = [[z; 3]; 3];
	for i in 0..d {
		for j in 0..d {
			edges[i][j] =
				Interval::point(points[i + 1][j])?.checked_sub(Interval::point(points[0][j])?)?;
		}
	}
	let determinant = if d == 2 {
		edges[0][0]
			.checked_mul(edges[1][1])?
			.checked_sub(edges[0][1].checked_mul(edges[1][0])?)?
	} else {
		let mut sum = z;
		for (perm, positive) in [
			([0, 1, 2], true),
			([1, 2, 0], true),
			([2, 0, 1], true),
			([2, 1, 0], false),
			([1, 0, 2], false),
			([0, 2, 1], false),
		] {
			let term = edges[0][perm[0]]
				.checked_mul(edges[1][perm[1]])?
				.checked_mul(edges[2][perm[2]])?;
			sum = if positive {
				sum.checked_add(term)?
			} else {
				sum.checked_sub(term)?
			};
		}
		sum
	};
	let absolute = if determinant.lower() > 0. {
		determinant.lower()
	} else if determinant.upper() < 0. {
		-determinant.upper()
	} else {
		return Err(invalid());
	};
	let mut longest = 0_f64;
	for i in 0..points.len() {
		for j in i + 1..points.len() {
			let mut length = z;
			for axis in 0..d {
				let diff = Interval::point(points[i][axis])?
					.checked_sub(Interval::point(points[j][axis])?)?;
				length = length
					.checked_add(Interval::point(diff.lower().abs().max(diff.upper().abs()))?)?;
			}
			longest = longest.max(length.upper());
		}
	}
	let mut denominator = Interval::point(1.)?;
	for _ in 0..d {
		denominator = denominator.checked_mul(Interval::point(longest)?)?;
	}
	if denominator.upper() <= 0. || !denominator.upper().is_finite() {
		return Err(invalid());
	}
	Ok(Interval::point(absolute)?
		.checked_div(Interval::point(denominator.upper())?)?
		.lower())
}
fn points(view: AffineMeshView<'_>, cell: usize) -> [[f64; 3]; 4] {
	let mut out = [[0.; 3]; 4];
	for (i, &id) in view.cells[cell].iter().enumerate() {
		out[i] = view.vertices[id];
	}
	out
}
fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
	for &b in bytes {
		*hash ^= u64::from(b);
		*hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
	}
}
fn hash_count(hash: &mut u64, count: usize) -> Result<(), CfdError> {
	hash_bytes(
		hash,
		&u64::try_from(count).map_err(|_| invalid())?.to_le_bytes(),
	);
	Ok(())
}
// Versioned geometry-only provenance: excludes physical order/viscosity and preserves
// original coordinate bits (including signed zero). It is not an operator identity.
fn fingerprint(view: AffineMeshView<'_>) -> Result<u64, CfdError> {
	let mut hash = 0xcbf2_9ce4_8422_2325_u64;
	hash_bytes(&mut hash, b"quest-cfd-affine-geometry-v1\0");
	hash_count(&mut hash, view.dimension)?;
	hash_bytes(&mut hash, b"vertices\0");
	hash_count(&mut hash, view.vertices.len())?;
	for p in view.vertices {
		for x in p {
			hash_bytes(&mut hash, &x.to_bits().to_le_bytes());
		}
	}
	hash_bytes(&mut hash, b"cells\0");
	hash_count(&mut hash, view.cells.len())?;
	for cell in view.cells {
		hash_count(&mut hash, cell.len())?;
		for &i in *cell {
			hash_count(&mut hash, i)?;
		}
	}
	hash_bytes(&mut hash, b"dirichlet\0");
	hash_count(&mut hash, view.dirichlet.len())?;
	for f in view.dirichlet {
		hash_count(&mut hash, f.vertices.len())?;
		for &i in f.vertices {
			hash_count(&mut hash, i)?;
		}
		hash_count(&mut hash, f.label.len())?;
		hash_bytes(&mut hash, f.label.as_bytes());
	}
	hash_bytes(&mut hash, b"periodic\0");
	hash_count(&mut hash, view.periodic.len())?;
	for f in view.periodic {
		hash_count(&mut hash, f.left.len())?;
		for &i in f.left {
			hash_count(&mut hash, i)?;
		}
		hash_count(&mut hash, f.right.len())?;
		for &i in f.right {
			hash_count(&mut hash, i)?;
		}
	}
	Ok(hash)
}

pub(super) fn validate(
	view: AffineMeshView<'_>,
	count: Counts,
	plan: &[FacetPlan],
	limits: PhysicalMeshLimits,
	receipt: &mut PhysicalMeshResources,
	adapter_bytes: usize,
) -> Result<(), CfdError> {
	if plan.iter().filter(|f| f.periodic).count() != view.periodic.len() {
		return Err(invalid());
	}
	let exact = GeometryLimits {
		max_coordinate_bits: limits.max_coordinate_bits,
		max_coefficient_bits: limits.max_coefficient_bits,
		max_bytes: limits.max_geometry_bytes,
		max_work: u64::try_from(limits.max_geometry_work.min(limits.max_work))
			.map_err(|_| invalid())?,
	};
	let scale = DyadicScale::admit(count.d, view.vertices, exact).map_err(|_| invalid())?;
	let c = view.cells.len();
	let pairs = mul(c, c - 1)? / 2;
	let slots = add(add(c, pairs)?, mul(view.periodic.len(), count.d)?)?;
	let work = add(
		usize::try_from(scale.source_work()).map_err(|_| invalid())?,
		add(
			mul(
				slots,
				usize::try_from(scale.pair_work()).map_err(|_| invalid())?,
			)?,
			mul(c, 10_000)?,
		)?,
	)?;
	let geometry_peak = add(
		add(
			add(count.input, limits.external_retained_bytes)?,
			adapter_bytes,
		)?,
		add(
			mul(count.i, size_of::<FacetPlan>())?,
			add(
				131_072,
				add(scale.retained_bytes(), scale.pair_peak_bytes())?,
			)?,
		)?,
	)?;
	receipt.geometry_work = work;
	receipt.geometry_peak_bytes = geometry_peak;
	receipt.construction_work = add(receipt.construction_work, work)?;
	receipt.constructor_peak_bytes = receipt.constructor_peak_bytes.max(geometry_peak);
	receipt.completed_phase = "geometry planned";
	receipt.source_identity = fingerprint(view)?;
	if plan.iter().any(|f| f.natural) {
		hash_bytes(&mut receipt.source_identity, b"mixed-natural-kinds\0");
		for f in plan {
			hash_bytes(&mut receipt.source_identity, &[u8::from(f.natural)]);
		}
	}
	if work > limits.max_geometry_work
		|| receipt.construction_work > limits.max_work
		|| geometry_peak > limits.max_bytes
	{
		return Err(invalid());
	}
	receipt.completed_phase = "geometry admitted";
	let mut minimum = f64::INFINITY;
	for ci in 0..c {
		let p = points(view, ci);
		scale
			.validate_simplex(&p[..=count.d], view.cells[ci], exact)
			.map_err(|_| invalid())?;
		let q = quality(&p[..=count.d], count.d)?;
		if q < limits.minimum_scaled_quality {
			return Err(invalid());
		}
		minimum = minimum.min(q);
	}
	for left in 0..c {
		let a = points(view, left);
		for right in left + 1..c {
			let b = points(view, right);
			scale
				.validate_pair(
					&a[..=count.d],
					view.cells[left],
					&b[..=count.d],
					view.cells[right],
					exact,
				)
				.map_err(|_| invalid())?;
		}
	}
	if !view.periodic.is_empty() {
		let lower: [f64; 3] = std::array::from_fn(|axis| {
			view.vertices
				.iter()
				.map(|p| p[axis])
				.fold(f64::INFINITY, f64::min)
		});
		let upper: [f64; 3] = std::array::from_fn(|axis| {
			view.vertices
				.iter()
				.map(|p| p[axis])
				.fold(f64::NEG_INFINITY, f64::max)
		});
		for face in view.periodic {
			let axis = (0..count.d)
				.find(|&axis| {
					let a = view.vertices[face.left[0]][axis];
					let b = view.vertices[face.right[0]][axis];
					((a == lower[axis] && b == upper[axis])
						|| (a == upper[axis] && b == lower[axis]))
						&& a != b
						&& face.left.iter().all(|&i| view.vertices[i][axis] == a)
						&& face.right.iter().all(|&i| view.vertices[i][axis] == b)
				})
				.ok_or_else(invalid)?;
			let a = &view.vertices[face.left[0]];
			let b = &view.vertices[face.right[0]];
			if (0..count.d).any(|i| i != axis && a[i] != b[i]) {
				return Err(invalid());
			}
			for (&left, &right) in face.left.iter().zip(face.right) {
				if !scale
					.matches_displacement(a, b, &view.vertices[left], &view.vertices[right], exact)
					.map_err(|_| invalid())?
				{
					return Err(invalid());
				}
			}
		}
	}
	receipt.minimum_quality_lower = Some(minimum);
	receipt.completed_phase = "geometry validated";
	Ok(())
}
