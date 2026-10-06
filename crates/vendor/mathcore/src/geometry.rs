//! Exact conformity of bounded affine triangles and tetrahedra.
//!
//! Binary64 coordinates are their exact dyadic values; zero has rational semantics.
//! The immutable scale is width metadata, not a source fingerprint. Mesh owners must
//! charge source admission once and the complete pair slot for every operation,
//! including repeated queries and AABB shortcuts. This checks a simplicial complex;
//! manifold topology, domain coverage and floating Jacobian conditioning are separate.
//!
//! Process arithmetic configuration must remain immutable throughout an operation.
//! Threshold overrides must be ASCII and at most32 bytes. Safe standard-library
//! environment discovery (also used by Dashu tuning) is outside the arithmetic
//! payload/work model: malformed oversized host configuration can allocate/scan
//! proportionally before rejection. These limits are not hard platform-memory caps.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Fixed dimension<=3 arrays and width<=128 are admitted before integer operations; resource arithmetic is checked"
)]
#![allow(
	clippy::float_cmp,
	reason = "Exact finite coordinate/zero identity defines geometry; no tolerance or snapping is permitted"
)]

use crate::dyadic;
use dashu_int::{IBig, Word};
use std::mem::size_of;

const PAIR_BYTES: usize = 64 * 1024;
const HARD_BITS: usize = 128;

/// Arithmetic/source-scan limits; the caller separately admits aggregate mesh work.
/// Environment retrieval/parse overhead and allocator/OS overhead are external.
#[derive(Clone, Copy, Debug)]
#[allow(
	clippy::struct_field_names,
	reason = "Fields are explicit upper resource bounds"
)]
pub struct GeometryLimits {
	pub max_coordinate_bits: usize,
	pub max_coefficient_bits: usize,
	pub max_bytes: usize,
	pub max_work: u64,
}
impl Default for GeometryLimits {
	fn default() -> Self {
		Self {
			max_coordinate_bits: 64,
			max_coefficient_bits: 4096,
			max_bytes: PAIR_BYTES,
			max_work: 1_000_000_000,
		}
	}
}
/// No approximate fallback is provided for rejected input or exhausted budgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GeometryError {
	#[error("geometry budget: {0}")]
	Budget(&'static str),
	#[error("invalid geometry shape")]
	Shape,
	#[error("nonfinite coordinate")]
	Nonfinite,
	#[error("point exceeds admitted dyadic scale")]
	OffScale,
	#[error("degenerate simplex")]
	Degenerate,
	#[error("inconsistent or duplicate vertex identifiers/coordinates")]
	Identifiers,
	#[error("intersection exceeds shared vertex hull")]
	Nonconforming,
	#[error("unsupported integer arithmetic profile")]
	ArithmeticProfile,
}
type Result<T> = std::result::Result<T, GeometryError>;

/// Exact legal pair intersection after consistency and nondegeneracy checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairRelation {
	Disjoint,
	Shared { vertices: usize },
}

/// Constant-storage common dyadic scale, admitted without big-integer allocations.
#[derive(Clone, Copy, Debug)]
pub struct DyadicScale {
	dimension: usize,
	exponent: i32,
	bits: usize,
	source_work: u64,
	pair_work: u64,
}

impl DyadicScale {
	/// Admit a finite 2D/3D coordinate source; 2D unused z components must be zero.
	/// # Errors
	/// Rejects invalid dimensions, finite/width/profile violations and source budgets.
	pub fn admit(dimension: usize, points: &[[f64; 3]], limits: GeometryLimits) -> Result<Self> {
		if !(2..=3).contains(&dimension) || points.is_empty() {
			return Err(GeometryError::Shape);
		}
		check_limits(limits)?;
		let work = u64::try_from(points.len())
			.ok()
			.and_then(|n| n.checked_mul(if dimension == 2 { 384 } else { 512 }))
			.and_then(|n| n.checked_add(4096))
			.ok_or(GeometryError::Budget("source work overflow"))?;
		if work > limits.max_work || size_of::<Self>() > limits.max_bytes {
			return Err(GeometryError::Budget("source admission"));
		}
		let mut exponent = i32::MAX;
		for point in points {
			check_point_shape(point, dimension)?;
			for &coordinate in &point[..dimension] {
				let p = dyadic::parts(coordinate)
					.ok_or(GeometryError::Nonfinite)?
					.normalized();
				if p.mantissa != 0 {
					exponent = exponent.min(p.exponent);
				}
			}
		}
		if exponent == i32::MAX {
			exponent = 0;
		}
		let mut bits = 0;
		for point in points {
			for &coordinate in &point[..dimension] {
				bits = bits.max(width(coordinate, exponent)?);
			}
		}
		if bits > limits.max_coordinate_bits {
			return Err(GeometryError::Budget("coordinate width"));
		}
		let determinant_bits = dimension * (bits + 1) + if dimension == 3 { 3 } else { 1 };
		let n = u64::try_from(determinant_bits.div_ceil(32))
			.map_err(|_| GeometryError::Budget("word width"))?;
		let o = u64::try_from((2 * determinant_bits + 2).div_ceil(32))
			.map_err(|_| GeometryError::Budget("word width"))?;
		let pair_work = 1024 * 16 * (n + 1) * (n + 1) + 1024 * 8 * (o + 1) + 2048 * 16;
		let scale = Self {
			dimension,
			exponent,
			bits,
			source_work: work,
			pair_work,
		};
		if scale.coefficient_bits() > limits.max_coefficient_bits {
			return Err(GeometryError::Budget("coefficient width"));
		}
		profile()?;
		Ok(scale)
	}
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.dimension
	}
	#[must_use]
	pub const fn coordinate_bits(&self) -> usize {
		self.bits
	}
	#[must_use]
	pub const fn retained_bytes(&self) -> usize {
		size_of::<Self>()
	}
	#[must_use]
	pub const fn source_work(&self) -> u64 {
		self.source_work
	}
	#[must_use]
	pub const fn pair_peak_bytes(&self) -> usize {
		PAIR_BYTES
	}
	#[must_use]
	pub const fn coefficient_bits(&self) -> usize {
		2 * self.determinant_bits() + 2
	}
	const fn determinant_bits(&self) -> usize {
		self.dimension * (self.bits + 1) + if self.dimension == 3 { 3 } else { 1 }
	}
	/// Conservative modeled word operations, charged even for repeated/shortcut queries.
	#[must_use]
	pub const fn pair_work(&self) -> u64 {
		self.pair_work
	}
	fn query(&self, limits: GeometryLimits) -> Result<()> {
		check_limits(limits)?;
		if self.bits > limits.max_coordinate_bits
			|| self.coefficient_bits() > limits.max_coefficient_bits
			|| PAIR_BYTES > limits.max_bytes
			|| self.pair_work() > limits.max_work
		{
			return Err(GeometryError::Budget("pair admission"));
		}
		profile()?;
		Ok(())
	}
	fn point(&self, point: &[f64; 3]) -> Result<[IBig; 3]> {
		check_point_shape(point, self.dimension)?;
		// Width validation precedes every shift/allocation, even with cached scale.
		for &value in &point[..self.dimension] {
			if width(value, self.exponent)? > self.bits {
				return Err(GeometryError::OffScale);
			}
		}
		let mut output = std::array::from_fn(|_| IBig::ZERO);
		for (axis, &value) in point[..self.dimension].iter().enumerate() {
			let p = dyadic::parts(value)
				.ok_or(GeometryError::Nonfinite)?
				.normalized();
			if p.mantissa == 0 {
				continue;
			}
			let shift =
				usize::try_from(p.exponent - self.exponent).map_err(|_| GeometryError::OffScale)?;
			let integer = IBig::from(p.mantissa) << shift;
			output[axis] = if p.negative { -integer } else { integer };
		}
		Ok(output)
	}
	fn simplex(&self, points: &[[f64; 3]], ids: &[usize]) -> Result<[[IBig; 3]; 4]> {
		if points.len() != self.dimension + 1 || ids.len() != points.len() {
			return Err(GeometryError::Shape);
		}
		// Preflight all points before any integer allocation.
		for point in points {
			check_point_shape(point, self.dimension)?;
			for &x in &point[..self.dimension] {
				if width(x, self.exponent)? > self.bits {
					return Err(GeometryError::OffScale);
				}
			}
		}
		for i in 0..points.len() {
			for j in i + 1..points.len() {
				if ids[i] == ids[j] || points[i] == points[j] {
					return Err(GeometryError::Identifiers);
				}
			}
		}
		let mut result = std::array::from_fn(|_| std::array::from_fn(|_| IBig::ZERO));
		for (index, point) in points.iter().enumerate() {
			result[index] = self.point(point)?;
		}
		if orient(&result, self.dimension) == IBig::ZERO {
			return Err(GeometryError::Degenerate);
		}
		Ok(result)
	}
	/// Validate an individual cell even when a mesh contains no cell pairs.
	/// # Errors
	/// Rejects shape, coordinate/ID, degeneracy, profile and complete pair-slot budgets.
	pub fn validate_simplex(
		&self,
		points: &[[f64; 3]],
		ids: &[usize],
		limits: GeometryLimits,
	) -> Result<()> {
		if points.len() != self.dimension + 1 || ids.len() != points.len() {
			return Err(GeometryError::Shape);
		}
		self.query(limits)?;
		self.simplex(points, ids)?;
		Ok(())
	}
	/// Compare exact displacements without rounding either anchor difference to f64.
	/// # Errors
	/// Rejects invalid/off-scale points, profile and complete pair-slot budgets.
	pub fn matches_displacement(
		&self,
		reference_left: &[f64; 3],
		reference_right: &[f64; 3],
		left: &[f64; 3],
		right: &[f64; 3],
		limits: GeometryLimits,
	) -> Result<bool> {
		self.query(limits)?;
		let a = self.point(reference_left)?;
		let b = self.point(reference_right)?;
		let c = self.point(left)?;
		let d = self.point(right)?;
		Ok((0..self.dimension).all(|j| &b[j] - &a[j] == &d[j] - &c[j]))
	}
	/// Check that the intersection is exactly the hull of common vertex IDs.
	/// # Errors
	/// Rejects duplicate/inconsistent IDs or coordinates, degeneracy, illegal contact,
	/// off-scale points, arithmetic profile and full pair-slot budgets before shortcut.
	pub fn validate_pair(
		&self,
		left: &[[f64; 3]],
		left_ids: &[usize],
		right: &[[f64; 3]],
		right_ids: &[usize],
		limits: GeometryLimits,
	) -> Result<PairRelation> {
		if left.len() != self.dimension + 1
			|| right.len() != left.len()
			|| left_ids.len() != left.len()
			|| right_ids.len() != right.len()
		{
			return Err(GeometryError::Shape);
		}
		self.query(limits)?;
		// Validate the complete pair before integer allocation or a shortcut.
		for point in left.iter().chain(right) {
			check_point_shape(point, self.dimension)?;
			for &x in &point[..self.dimension] {
				if width(x, self.exponent)? > self.bits {
					return Err(GeometryError::OffScale);
				}
			}
		}
		let mut shared_left = [false; 4];
		let mut shared_right = [false; 4];
		let mut shared = 0;
		for i in 0..left.len() {
			for j in 0..right.len() {
				if left_ids[i] == right_ids[j] {
					if left[i] != right[j] {
						return Err(GeometryError::Identifiers);
					}
					shared_left[i] = true;
					shared_right[j] = true;
					shared += 1;
				} else if left[i] == right[j] {
					return Err(GeometryError::Identifiers);
				}
			}
		}
		let a = self.simplex(left, left_ids)?;
		let b = self.simplex(right, right_ids)?;
		if shared == left.len() {
			return Err(GeometryError::Identifiers);
		}
		for axis in 0..self.dimension {
			let amin = left.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
			let amax = left
				.iter()
				.map(|p| p[axis])
				.fold(f64::NEG_INFINITY, f64::max);
			let bmin = right.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
			let bmax = right
				.iter()
				.map(|p| p[axis])
				.fold(f64::NEG_INFINITY, f64::max);
			if amax < bmin || bmax < amin {
				return Ok(PairRelation::Disjoint);
			}
		}
		let ab = cross_barycentric(&b, &a, self.dimension);
		let ba = cross_barycentric(&a, &b, self.dimension);
		check_candidates(&ab, shared_left, self.dimension)?;
		check_candidates(&ba, shared_right, self.dimension)?;
		Ok(if shared == 0 {
			PairRelation::Disjoint
		} else {
			PairRelation::Shared { vertices: shared }
		})
	}
}
const fn check_limits(l: GeometryLimits) -> Result<()> {
	if l.max_coordinate_bits == 0
		|| l.max_coordinate_bits > HARD_BITS
		|| l.max_coefficient_bits == 0
		|| l.max_coefficient_bits > 4096
	{
		return Err(GeometryError::Budget("invalid limits"));
	}
	Ok(())
}
fn profile() -> Result<()> {
	if ![4, 8].contains(&size_of::<Word>()) {
		return Err(GeometryError::ArithmeticProfile);
	}
	for (name, minimum) in [
		("DASHU_THRESHOLD_SIMPLE_MUL", 24),
		("DASHU_THRESHOLD_SIMPLE_SQR", 30),
	] {
		// Safe std retrieval clones host configuration before its length is known.
		// It is explicitly outside the arithmetic payload/model. No numeric parsing
		// or Unicode scanning of an oversized retrieved value is performed here.
		if let Some(value) = std::env::var_os(name) {
			let bytes = value.as_encoded_bytes();
			if bytes.len() > 32
				|| !bytes.is_ascii()
				|| value
					.to_str()
					.and_then(|s| s.parse::<usize>().ok())
					.is_none_or(|v| v < minimum)
			{
				return Err(GeometryError::ArithmeticProfile);
			}
		}
	}
	Ok(())
}
fn check_point_shape(point: &[f64; 3], dimension: usize) -> Result<()> {
	if point.iter().any(|x| !x.is_finite()) {
		return Err(GeometryError::Nonfinite);
	}
	if dimension == 2 && point[2] != 0. {
		return Err(GeometryError::Shape);
	}
	Ok(())
}
fn width(value: f64, exponent: i32) -> Result<usize> {
	let p = dyadic::parts(value)
		.ok_or(GeometryError::Nonfinite)?
		.normalized();
	if p.mantissa == 0 {
		return Ok(0);
	}
	let shift = usize::try_from(p.exponent - exponent).map_err(|_| GeometryError::OffScale)?;
	(64 - usize::try_from(p.mantissa.leading_zeros())
		.map_err(|_| GeometryError::Budget("mantissa width"))?)
	.checked_add(shift)
	.ok_or(GeometryError::Budget("coordinate overflow"))
}
fn orient(points: &[[IBig; 3]; 4], dimension: usize) -> IBig {
	let m: [[IBig; 3]; 3] = std::array::from_fn(|row| {
		std::array::from_fn(|col| &points[col + 1][row] - &points[0][row])
	});
	if dimension == 2 {
		return &m[0][0] * &m[1][1] - &m[0][1] * &m[1][0];
	}
	&m[0][0] * &m[1][1] * &m[2][2] + &m[0][1] * &m[1][2] * &m[2][0] + &m[0][2] * &m[1][0] * &m[2][1]
		- &m[0][2] * &m[1][1] * &m[2][0]
		- &m[0][1] * &m[1][0] * &m[2][2]
		- &m[0][0] * &m[1][2] * &m[2][1]
}
fn cross_barycentric(
	simplex: &[[IBig; 3]; 4],
	other: &[[IBig; 3]; 4],
	dimension: usize,
) -> [[IBig; 4]; 4] {
	let negative = orient(simplex, dimension) < IBig::ZERO;
	let mut table = std::array::from_fn(|_| std::array::from_fn(|_| IBig::ZERO));
	for vertex in 0..=dimension {
		for facet in 0..=dimension {
			let mut replaced = simplex.clone();
			replaced[facet].clone_from(&other[vertex]);
			let numerator = orient(&replaced, dimension);
			table[vertex][facet] = if negative { -numerator } else { numerator };
		}
	}
	table
}
fn check_candidates(table: &[[IBig; 4]; 4], shared: [bool; 4], dimension: usize) -> Result<()> {
	for vertex in 0..=dimension {
		if table[vertex][..=dimension].iter().all(|n| *n >= IBig::ZERO) && !shared[vertex] {
			return Err(GeometryError::Nonconforming);
		}
	}
	for u in 0..=dimension {
		for v in u + 1..=dimension {
			for facet in 0..=dimension {
				let uk = &table[u][facet];
				let vk = &table[v][facet];
				let denominator = uk - vk;
				let positive = denominator > IBig::ZERO;
				if denominator == IBig::ZERO
					|| (positive && (*uk < IBig::ZERO || *vk > IBig::ZERO))
					|| (!positive && (*uk > IBig::ZERO || *vk < IBig::ZERO))
				{
					continue;
				}
				let feasible = (0..=dimension).filter(|&j| j != facet).all(|j| {
					let q = uk * &table[v][j] - vk * &table[u][j];
					if positive {
						q >= IBig::ZERO
					} else {
						q <= IBig::ZERO
					}
				});
				if feasible
					&& ((!shared[u] && *vk != IBig::ZERO) || (!shared[v] && *uk != IBig::ZERO))
				{
					return Err(GeometryError::Nonconforming);
				}
			}
		}
	}
	Ok(())
}
