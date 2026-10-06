//! Bounded complete BDM1/P0 DG on Cartesian simplex meshes in two and three dimensions.
//!
//! Every coefficient index is bounded by validated mesh dimensions. Floating-point
//! arithmetic follows the displayed quadrature formulas without forced FMA changes.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	clippy::manual_midpoint
)]

use crate::{AssemblyDiagnostics, CfdError};
use std::collections::BTreeMap;

type Point = [f64; 3];
type GridPoint = [u32; 3];
type FacetIncidence = (usize, Vec<usize>);
type PeriodicIncidences = BTreeMap<(usize, Vec<GridPoint>), Vec<FacetIncidence>>;

/// Physical conditions on the exterior of the box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoxBoundary {
	/// Identify opposite faces in every physical direction.
	Periodic,
	/// Explicit mixed boundary facets, including stationary normal lifting and natural outflow.
	Mixed,
	/// No penetration everywhere, with SIP imposition of a tangential x-directed lid at y=L.
	Cavity {
		/// Lid speed; zero gives a stationary no-slip box.
		lid_speed: f64,
	},
}

#[derive(Clone, Debug)]
pub(crate) struct Cell {
	pub(crate) vertices: Vec<Point>,
	pub(crate) grid: Vec<GridPoint>,
	pub(crate) gradients: Vec<Point>,
	pub(crate) volume: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct Face {
	pub(crate) left: usize,
	pub(crate) right: Option<usize>,
	pub(crate) left_nodes: Vec<usize>,
	pub(crate) right_nodes: Vec<usize>,
	pub(crate) normal: Point,
	pub(crate) measure: f64,
	pub(crate) lid: bool,
	pub(crate) prescribed: Option<Vec<Point>>,
	pub(crate) outflow: bool,
	pub(crate) label: String,
}

/// Full BDM1/P0 ODE with all independent modes retained on a small box mesh.
///
/// The dense chart is deliberately limited to 768 local coefficients. This is a
/// verification/reference implementation, not the distributed large-mesh path.
#[derive(Clone, Debug)]
pub struct SimplexBdm {
	dimension: usize,
	viscosity: f64,
	boundary: BoxBoundary,
	cells: Vec<Cell>,
	faces: Vec<Face>,
	chart: Vec<Vec<f64>>,
	sip: Vec<Vec<f64>>,
	boundary_force: Vec<f64>,
	lifting: Vec<f64>,
	diagnostics: AssemblyDiagnostics,
}

impl SimplexBdm {
	/// Actual retained Vec/String capacities of the complete bounded physical source.
	/// Includes geometry, chart, dense viscosity, lifting and boundary payloads;
	/// excludes allocator/operating-system metadata and transient query storage.
	/// # Errors
	/// Rejects checked capacity-byte overflow.
	pub fn retained_bytes(&self) -> Result<usize, CfdError> {
		fn payload<T>(values: &Vec<T>) -> Result<usize, CfdError> {
			values
				.capacity()
				.checked_mul(size_of::<T>())
				.ok_or(CfdError::InvalidInput("simplex retained capacity overflow"))
		}
		let mut bytes = size_of::<Self>();
		let mut charge = |n: usize| -> Result<(), CfdError> {
			bytes = bytes
				.checked_add(n)
				.ok_or(CfdError::InvalidInput("simplex retained capacity overflow"))?;
			Ok(())
		};
		for rows in [&self.chart, &self.sip] {
			charge(payload(rows)?)?;
			for row in rows {
				charge(payload(row)?)?;
			}
		}
		charge(payload(&self.boundary_force)?)?;
		charge(payload(&self.lifting)?)?;
		charge(payload(&self.cells)?)?;
		for cell in &self.cells {
			charge(payload(&cell.vertices)?)?;
			charge(payload(&cell.grid)?)?;
			charge(payload(&cell.gradients)?)?;
		}
		charge(payload(&self.faces)?)?;
		for face in &self.faces {
			charge(payload(&face.left_nodes)?)?;
			charge(payload(&face.right_nodes)?)?;
			charge(face.label.capacity())?;
			if let Some(values) = &face.prescribed {
				charge(payload(values)?)?;
			}
		}
		Ok(bytes)
	}
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
	a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn sub(a: Point, b: Point) -> Point {
	std::array::from_fn(|k| a[k] - b[k])
}
fn cross(a: Point, b: Point) -> Point {
	[
		a[1] * b[2] - a[2] * b[1],
		a[2] * b[0] - a[0] * b[2],
		a[0] * b[1] - a[1] * b[0],
	]
}
fn centroid(points: &[Point]) -> Point {
	let scale = match points.len() {
		2 => 0.5,
		3 => 1. / 3.,
		4 => 0.25,
		_ => 0.,
	};
	std::array::from_fn(|k| points.iter().map(|p| p[k]).sum::<f64>() * scale)
}

fn cell(grid: Vec<GridPoint>, scale: f64, dimension: usize) -> Result<Cell, CfdError> {
	let vertices: Vec<Point> = grid
		.iter()
		.map(|p| p.map(|v| f64::from(v) * scale))
		.collect();
	physical_cell(vertices, grid, dimension)
}

pub(crate) fn physical_cell(
	vertices: Vec<Point>,
	grid: Vec<GridPoint>,
	dimension: usize,
) -> Result<Cell, CfdError> {
	let e1 = sub(vertices[1], vertices[0]);
	let e2 = sub(vertices[2], vertices[0]);
	let mut gradients = vec![[0.; 3]; dimension + 1];
	let volume;
	if dimension == 2 {
		let det = e1[0] * e2[1] - e1[1] * e2[0];
		volume = det.abs() * 0.5;
		gradients[1] = [e2[1] / det, -e2[0] / det, 0.];
		gradients[2] = [-e1[1] / det, e1[0] / det, 0.];
	} else {
		let e3 = sub(vertices[3], vertices[0]);
		let det = dot(&e1, &cross(e2, e3));
		volume = det.abs() / 6.;
		gradients[1] = cross(e2, e3).map(|v| v / det);
		gradients[2] = cross(e3, e1).map(|v| v / det);
		gradients[3] = cross(e1, e2).map(|v| v / det);
	}
	if !volume.is_finite() || volume <= 0. || gradients.iter().flatten().any(|v| !v.is_finite()) {
		return Err(CfdError::Assembly("degenerate simplex geometry"));
	}
	gradients[0] = std::array::from_fn(|k| -gradients.iter().skip(1).map(|g| g[k]).sum::<f64>());
	Ok(Cell {
		vertices,
		grid,
		gradients,
		volume,
	})
}

pub(crate) fn mesh(
	dimension: usize,
	n: u32,
	extent: f64,
	boundary: BoxBoundary,
) -> Result<(Vec<Cell>, Vec<Face>), CfdError> {
	let mut cells = Vec::new();
	let permutations: &[&[usize]] = if dimension == 2 {
		&[&[0, 1], &[1, 0]]
	} else {
		&[
			&[0, 1, 2],
			&[0, 2, 1],
			&[1, 0, 2],
			&[1, 2, 0],
			&[2, 0, 1],
			&[2, 1, 0],
		]
	};
	let z_count = if dimension == 2 { 1 } else { n };
	for z in 0..z_count {
		for y in 0..n {
			for x in 0..n {
				for axes in permutations {
					let mut p = [x, y, z];
					let mut points = vec![p];
					for &axis in *axes {
						p[axis] += 1;
						points.push(p);
					}
					cells.push(cell(points, extent / f64::from(n), dimension)?);
				}
			}
		}
	}
	let mut internal: BTreeMap<Vec<GridPoint>, Vec<(usize, Vec<usize>)>> = BTreeMap::new();
	for (ci, c) in cells.iter().enumerate() {
		for omitted in 0..=dimension {
			let nodes: Vec<_> = (0..=dimension).filter(|&i| i != omitted).collect();
			let mut key: Vec<_> = nodes.iter().map(|&i| c.grid[i]).collect();
			key.sort_unstable();
			internal.entry(key).or_default().push((ci, nodes));
		}
	}
	let mut faces = Vec::new();
	let mut periodic: PeriodicIncidences = BTreeMap::new();
	for pair in internal.into_values() {
		if pair.len() == 2 {
			faces.push(make_face(&cells, &pair[0], Some(&pair[1]), dimension, n)?);
		} else {
			let (ci, nodes) = &pair[0];
			if boundary == BoxBoundary::Periodic {
				let axis = (0..dimension)
					.find(|&axis| {
						nodes.iter().all(|&i| cells[*ci].grid[i][axis] == 0)
							|| nodes.iter().all(|&i| cells[*ci].grid[i][axis] == n)
					})
					.ok_or(CfdError::Assembly("box boundary facet lacks axis"))?;
				let mut key: Vec<_> = nodes
					.iter()
					.map(|&i| {
						let mut p = cells[*ci].grid[i];
						p[axis] = 0;
						p
					})
					.collect();
				key.sort_unstable();
				periodic
					.entry((axis, key))
					.or_default()
					.push(pair[0].clone());
			} else {
				faces.push(make_face(&cells, &pair[0], None, dimension, n)?);
			}
		}
	}
	for pair in periodic.into_values() {
		if pair.len() != 2 {
			return Err(CfdError::Assembly("unmatched periodic facet"));
		}
		faces.push(make_face(&cells, &pair[0], Some(&pair[1]), dimension, n)?);
	}
	Ok((cells, faces))
}

pub(crate) fn make_face(
	cells: &[Cell],
	left: &(usize, Vec<usize>),
	right: Option<&(usize, Vec<usize>)>,
	dimension: usize,
	n: u32,
) -> Result<Face, CfdError> {
	let vertices: Vec<_> = left.1.iter().map(|&i| cells[left.0].vertices[i]).collect();
	let edge = sub(vertices[1], vertices[0]);
	let mut normal = if dimension == 2 {
		[edge[1], -edge[0], 0.]
	} else {
		cross(edge, sub(vertices[2], vertices[0]))
	};
	let norm = dot(&normal, &normal).sqrt();
	let measure = if dimension == 2 { norm } else { norm * 0.5 };
	normal = normal.map(|v| v / norm);
	if dot(
		&normal,
		&sub(centroid(&vertices), centroid(&cells[left.0].vertices)),
	) < 0.
	{
		normal = normal.map(|v| -v);
	}
	let right_nodes = if let Some(right) = right {
		let rv: Vec<_> = right
			.1
			.iter()
			.map(|&i| cells[right.0].vertices[i])
			.collect();
		let translation = sub(centroid(&rv), centroid(&vertices));
		let mut mapping = Vec::new();
		for &i in &left.1 {
			let p = cells[left.0].vertices[i];
			let j = right
				.1
				.iter()
				.find(|&&j| {
					sub(sub(cells[right.0].vertices[j], p), translation)
						.iter()
						.all(|x| x.abs() < 1e-10)
				})
				.copied()
				.ok_or(CfdError::Assembly("facet vertex correspondence failed"))?;
			mapping.push(j);
		}
		mapping
	} else {
		Vec::new()
	};
	let lid = right.is_none() && left.1.iter().all(|&i| cells[left.0].grid[i][1] == n);
	Ok(Face {
		left: left.0,
		right: right.map(|r| r.0),
		left_nodes: left.1.clone(),
		right_nodes,
		normal,
		measure,
		lid,
		prescribed: right.is_none().then(|| vec![[0.; 3]; dimension]),
		outflow: false,
		label: String::new(),
	})
}

fn mass_apply(cells: &[Cell], dimension: usize, x: &[f64]) -> Vec<f64> {
	let nodes = dimension + 1;
	let local = dimension * nodes;
	let denominator = if dimension == 2 { 12. } else { 20. };
	let mut result = vec![0.; x.len()];
	for (cell, c) in cells.iter().enumerate() {
		for component in 0..dimension {
			let offset = cell * local + component * nodes;
			let sum = x[offset..offset + nodes].iter().sum::<f64>();
			for node in 0..nodes {
				result[offset + node] = c.volume / denominator * (sum + x[offset + node]);
			}
		}
	}
	result
}

fn constraints(cells: &[Cell], faces: &[Face], dimension: usize) -> Vec<Vec<f64>> {
	let nodes = dimension + 1;
	let local = dimension * nodes;
	let size = cells.len() * local;
	let mut rows = Vec::new();
	for face in faces {
		if face.outflow {
			continue;
		}
		for (point, &ln) in face.left_nodes.iter().enumerate() {
			let mut row = vec![0.; size];
			for k in 0..dimension {
				row[face.left * local + k * nodes + ln] = face.normal[k];
				if let Some(right) = face.right {
					row[right * local + k * nodes + face.right_nodes[point]] -= face.normal[k];
				}
			}
			rows.push(row);
		}
	}
	for (ci, c) in cells.iter().enumerate() {
		let mut row = vec![0.; size];
		for k in 0..dimension {
			for node in 0..nodes {
				row[ci * local + k * nodes + node] = c.volume * c.gradients[node][k];
			}
		}
		rows.push(row);
	}
	rows
}

fn chart(rows: &[Vec<f64>], cells: &[Cell], dimension: usize) -> Result<Vec<Vec<f64>>, CfdError> {
	let size = cells.len() * dimension * (dimension + 1);
	let mut rref = rows.to_vec();
	let mut pivots = Vec::new();
	for col in 0..size {
		let rank = pivots.len();
		let Some(row) =
			(rank..rref.len()).max_by(|&i, &j| rref[i][col].abs().total_cmp(&rref[j][col].abs()))
		else {
			break;
		};
		if rref[row][col].abs() < 1e-10 {
			continue;
		}
		rref.swap(rank, row);
		let scale = rref[rank][col];
		for value in &mut rref[rank] {
			*value /= scale;
		}
		let pivot = rref[rank].clone();
		for (i, r) in rref.iter_mut().enumerate() {
			if i != rank {
				let f = r[col];
				for (v, p) in r.iter_mut().zip(&pivot) {
					*v -= f * p;
				}
			}
		}
		pivots.push(col);
	}
	let mut chart: Vec<Vec<f64>> = Vec::new();
	let mut mass_chart: Vec<Vec<f64>> = Vec::new();
	for free in (0..size).filter(|i| !pivots.contains(i)) {
		let mut q = vec![0.; size];
		q[free] = 1.;
		for (row, &pivot) in pivots.iter().enumerate() {
			q[pivot] = -rref[row][free];
		}
		for _ in 0..2 {
			for (old, mold) in chart.iter().zip(&mass_chart) {
				let f = dot(mold, &q);
				for (v, p) in q.iter_mut().zip(old) {
					*v -= f * p;
				}
			}
		}
		let mq = mass_apply(cells, dimension, &q);
		let norm = dot(&q, &mq).sqrt();
		if !norm.is_finite() || norm < 1e-12 {
			return Err(CfdError::Assembly("singular full simplex chart"));
		}
		for v in &mut q {
			*v /= norm;
		}
		mass_chart.push(mq.iter().map(|v| v / norm).collect());
		chart.push(q);
	}
	if chart.is_empty() {
		return Err(CfdError::Assembly("mesh has no independent velocity modes"));
	}
	Ok(chart)
}

fn volume_quadrature(dimension: usize) -> Vec<(Vec<f64>, f64)> {
	if dimension == 2 {
		vec![
			(vec![2. / 3., 1. / 6., 1. / 6.], 1. / 3.),
			(vec![1. / 6., 2. / 3., 1. / 6.], 1. / 3.),
			(vec![1. / 6., 1. / 6., 2. / 3.], 1. / 3.),
		]
	} else {
		let a = (5. + 3. * 5_f64.sqrt()) / 20.;
		let b = (5. - 5_f64.sqrt()) / 20.;
		(0..4)
			.map(|j| ((0..4).map(|i| if i == j { a } else { b }).collect(), 0.25))
			.collect()
	}
}

fn face_quadrature(dimension: usize) -> Vec<(Vec<f64>, f64)> {
	if dimension == 2 {
		let a = 0.5 - 0.5 / 3_f64.sqrt();
		vec![(vec![a, 1. - a], 0.5), (vec![1. - a, a], 0.5)]
	} else {
		vec![
			(vec![1. / 3.; 3], -27. / 48.),
			(vec![0.6, 0.2, 0.2], 25. / 48.),
			(vec![0.2, 0.6, 0.2], 25. / 48.),
			(vec![0.2, 0.2, 0.6], 25. / 48.),
		]
	}
}

fn face_basis(face: &Face, bary: &[f64], dimension: usize) -> Vec<(usize, usize, f64, Point)> {
	let nodes = dimension + 1;
	let local = dimension * nodes;
	let mut basis = Vec::new();
	for (cell, face_nodes, sign) in [
		(Some(face.left), &face.left_nodes, 1.),
		(face.right, &face.right_nodes, -1.),
	] {
		if let Some(cell) = cell {
			for component in 0..dimension {
				for node in 0..nodes {
					let value = face_nodes
						.iter()
						.position(|&i| i == node)
						.map_or(0., |i| bary[i]);
					basis.push((
						cell * local + component * nodes + node,
						component,
						sign * value,
						[0.; 3],
					));
				}
			}
		}
	}
	basis
}

fn operators(cells: &[Cell], faces: &[Face], dimension: usize) -> (Vec<Vec<f64>>, Vec<f64>) {
	let nodes = dimension + 1;
	let local = dimension * nodes;
	let size = cells.len() * local;
	let mut sip = vec![vec![0.; size]; size];
	let mut force = vec![0.; size];
	for (ci, c) in cells.iter().enumerate() {
		for component in 0..dimension {
			for i in 0..nodes {
				for j in 0..nodes {
					sip[ci * local + component * nodes + i][ci * local + component * nodes + j] +=
						c.volume * dot(&c.gradients[i], &c.gradients[j]);
				}
			}
		}
	}
	for face in faces {
		if face.outflow {
			continue;
		}
		let df = if dimension == 2 { 2. } else { 3. };
		let min_volume = face.right.map_or(cells[face.left].volume, |r| {
			cells[r].volume.min(cells[face.left].volume)
		});
		let penalty = 40. * face.measure / (df * min_volume);
		let average = if face.right.is_some() { 0.5 } else { 1. };
		for (bary, weight) in face_quadrature(dimension) {
			let basis = face_basis(face, &bary, dimension);
			for &(i, component, ji, _) in &basis {
				let dni = average * dot(&cells[i / local].gradients[i % nodes], &face.normal);
				for &(j, cj, jj, _) in &basis {
					if component == cj {
						let dnj =
							average * dot(&cells[j / local].gradients[j % nodes], &face.normal);
						sip[i][j] +=
							face.measure * weight * (-dni * jj - dnj * ji + penalty * ji * jj);
					}
				}
				if let Some(values) = &face.prescribed {
					let prescribed = values
						.iter()
						.zip(&bary)
						.map(|(v, l)| v[component] * l)
						.sum::<f64>();
					force[i] += face.measure * weight * (-dni + penalty * ji) * prescribed;
				}
			}
		}
	}
	(sip, force)
}

impl SimplexBdm {
	/// Assemble the full BDM1/P0 DG system on a uniformly triangulated square/cube.
	///
	/// # Errors
	/// Rejects invalid scales, unsupported dimension, more than 768 local coefficients,
	/// or a failed rank/mass/continuity certificate. No mode truncation is performed.
	pub fn box_mesh(
		dimension: usize,
		subdivisions: u32,
		extent: f64,
		viscosity: f64,
		boundary: BoxBoundary,
	) -> Result<Self, CfdError> {
		if ![2, 3].contains(&dimension)
			|| boundary == BoxBoundary::Mixed
			|| subdivisions == 0
			|| !extent.is_finite()
			|| extent <= 0.
			|| !viscosity.is_finite()
			|| viscosity < 0.
		{
			return Err(CfdError::InvalidInput("invalid simplex box parameters"));
		}
		if let BoxBoundary::Cavity { lid_speed } = boundary
			&& !lid_speed.is_finite()
		{
			return Err(CfdError::InvalidInput("nonfinite cavity lid"));
		}
		let count = u64::from(subdivisions)
			.checked_pow(
				u32::try_from(dimension)
					.map_err(|_| CfdError::InvalidInput("dimension overflow"))?,
			)
			.and_then(|v| v.checked_mul(if dimension == 2 { 12 } else { 72 }))
			.ok_or(CfdError::InvalidInput("mesh count overflow"))?;
		if count > 768 {
			return Err(CfdError::Unsupported(
				"dense full-chart reference limited to 768 local velocity coefficients".into(),
			));
		}
		let (cells, mut faces) = mesh(dimension, subdivisions, extent, boundary)?;
		if let BoxBoundary::Cavity { lid_speed } = boundary {
			for face in &mut faces {
				if face.lid {
					face.prescribed = Some(vec![[lid_speed, 0., 0.]; dimension]);
				}
			}
		}
		Self::assemble_parts(dimension, viscosity, boundary, cells, faces)
	}

	fn assemble_parts(
		dimension: usize,
		viscosity: f64,
		boundary: BoxBoundary,
		cells: Vec<Cell>,
		faces: Vec<Face>,
	) -> Result<Self, CfdError> {
		let rows = constraints(&cells, &faces, dimension);
		let chart = chart(&rows, &cells, dimension)?;
		let size = cells.len() * dimension * (dimension + 1);
		let constraint_residual = rows
			.iter()
			.flat_map(|c| chart.iter().map(move |q| dot(c, q).abs()))
			.fold(0., f64::max);
		let mut mass_orthogonality_residual = 0_f64;
		for (j, q) in chart.iter().enumerate() {
			let mq = mass_apply(&cells, dimension, q);
			for (i, p) in chart.iter().enumerate() {
				mass_orthogonality_residual =
					mass_orthogonality_residual.max((dot(p, &mq) - f64::from(i == j)).abs());
			}
		}
		if constraint_residual > 1e-8 || mass_orthogonality_residual > 1e-10 {
			return Err(CfdError::Assembly("full simplex chart certificate failed"));
		}
		let diagnostics = AssemblyDiagnostics {
			local_velocity_dimension: size,
			constraint_rank: size - chart.len(),
			independent_dimension: chart.len(),
			constraint_residual,
			mass_orthogonality_residual,
		};
		let (sip, boundary_force) = operators(&cells, &faces, dimension);
		let lifting = boundary_lifting(&cells, &faces, dimension, &rows, &chart)?;
		Ok(Self {
			dimension,
			viscosity,
			boundary,
			cells,
			faces,
			chart,
			sip,
			boundary_force,
			lifting,
			diagnostics,
		})
	}

	/// Physical space dimension.
	#[must_use]
	pub const fn physical_dimension(&self) -> usize {
		self.dimension
	}
	/// Full independent physical ODE dimension.
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.chart.len()
	}
	/// Complete assembly certificate.
	#[must_use]
	pub const fn diagnostics(&self) -> &AssemblyDiagnostics {
		&self.diagnostics
	}
	/// Number of simplices.
	#[must_use]
	pub const fn cell_count(&self) -> usize {
		self.cells.len()
	}
	/// The physical box boundary contract.
	#[must_use]
	pub const fn boundary(&self) -> BoxBoundary {
		self.boundary
	}

	/// Recover all local nodal velocity coefficients from complete mass coordinates.
	///
	/// # Errors
	/// Rejects malformed/nonfinite states or numerical overflow.
	pub fn coefficients(&self, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		self.coefficients_with_boundary_scale(state, 1.)
	}

	/// Reconstruct `u = Q a + g l` for a scalar time-dependent prescribed trace.
	/// Every homogeneous coordinate and the original minimum-mass lifting is retained.
	/// # Errors
	/// Rejects invalid states, nonfinite scale or numerical overflow.
	pub fn coefficients_with_boundary_scale(
		&self,
		state: &[f64],
		scale: f64,
	) -> Result<Vec<f64>, CfdError> {
		if state.len() != self.dimension()
			|| state.iter().any(|v| !v.is_finite())
			|| !scale.is_finite()
		{
			return Err(CfdError::InvalidInput("invalid full simplex state"));
		}
		let c: Vec<_> = (0..self.diagnostics.local_velocity_dimension)
			.map(|i| {
				self.chart
					.iter()
					.zip(state)
					.map(|(q, s)| q[i] * s)
					.sum::<f64>()
					+ scale * self.lifting[i]
			})
			.collect();
		if c.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("simplex coefficient overflow"));
		}
		Ok(c)
	}

	/// Full constrained L2 projection of an analytic velocity into the mass chart.
	/// Uses fourth-order tensor Gauss quadrature under a Duffy map; nonpolynomial inputs still require mesh refinement.
	///
	/// # Errors
	/// Rejects a velocity callback producing nonfinite values.
	pub fn project_velocity(&self, field: impl Fn(Point) -> Point) -> Result<Vec<f64>, CfdError> {
		let nodes = self.dimension + 1;
		let local = self.dimension * nodes;
		let mut load = vec![0.; self.diagnostics.local_velocity_dimension];
		for (ci, c) in self.cells.iter().enumerate() {
			for (bary, weight) in projection_quadrature(self.dimension) {
				let point = std::array::from_fn(|k| {
					c.vertices.iter().zip(&bary).map(|(p, l)| p[k] * l).sum()
				});
				let velocity = field(point);
				if velocity.iter().any(|v| !v.is_finite()) {
					return Err(CfdError::InvalidInput("nonfinite analytic velocity"));
				}
				for component in 0..self.dimension {
					for node in 0..nodes {
						load[ci * local + component * nodes + node] +=
							c.volume * weight * velocity[component] * bary[node];
					}
				}
			}
		}
		let state: Vec<_> = self.chart.iter().map(|q| dot(q, &load)).collect();
		if state.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("projected velocity overflow"));
		}
		Ok(state)
	}

	/// Complete nonlinear conservative central-flux convection plus SIP viscosity and lid forcing.
	///
	/// # Errors
	/// Rejects malformed states or nonfinite drift values.
	pub fn drift(&self, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		self.drift_with_boundary_scale(state, 1., 0.)
	}

	/// Evaluate `a' = Q^T (r(Qa+g l,g) - M l g')`.
	///
	/// The scalar multiplies every prescribed trace, including tangential SIP
	/// data. The lifting derivative is retained explicitly even when its mass
	/// projection is zero to roundoff. Geometry and constraint rank are fixed.
	/// # Errors
	/// Rejects invalid state/scale/rate or nonfinite drift.
	pub fn drift_with_boundary_scale(
		&self,
		state: &[f64],
		scale: f64,
		rate: f64,
	) -> Result<Vec<f64>, CfdError> {
		if !rate.is_finite() {
			return Err(CfdError::InvalidInput("nonfinite boundary derivative"));
		}
		let c = self.coefficients_with_boundary_scale(state, scale)?;
		let mut force = self.force(&c, scale);
		let mass_lifting = mass_apply(&self.cells, self.dimension, &self.lifting);
		for (f, l) in force.iter_mut().zip(mass_lifting) {
			*f -= rate * l;
		}
		let result: Vec<_> = self.chart.iter().map(|q| dot(q, &force)).collect();
		if result.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("simplex drift overflow"));
		}
		Ok(result)
	}

	fn force(&self, c: &[f64], boundary_scale: f64) -> Vec<f64> {
		let nodes = self.dimension + 1;
		let local = self.dimension * nodes;
		let mut force: Vec<_> = self
			.sip
			.iter()
			.zip(&self.boundary_force)
			.map(|(row, b)| self.viscosity * (boundary_scale * b - dot(row, c)))
			.collect();
		for (ci, cell) in self.cells.iter().enumerate() {
			for (bary, weight) in volume_quadrature(self.dimension) {
				let u: Point = std::array::from_fn(|k| {
					if k < self.dimension {
						(0..nodes)
							.map(|node| c[ci * local + k * nodes + node] * bary[node])
							.sum()
					} else {
						0.
					}
				});
				for component in 0..self.dimension {
					for node in 0..nodes {
						force[ci * local + component * nodes + node] +=
							cell.volume * weight * u[component] * dot(&u, &cell.gradients[node]);
					}
				}
			}
		}
		for face in &self.faces {
			if let Some(right) = face.right {
				for (bary, weight) in face_quadrature(self.dimension) {
					let ul: Point = std::array::from_fn(|k| {
						if k < self.dimension {
							face.left_nodes
								.iter()
								.zip(&bary)
								.map(|(&node, l)| c[face.left * local + k * nodes + node] * l)
								.sum()
						} else {
							0.
						}
					});
					let ur: Point = std::array::from_fn(|k| {
						if k < self.dimension {
							face.right_nodes
								.iter()
								.zip(&bary)
								.map(|(&node, l)| c[right * local + k * nodes + node] * l)
								.sum()
						} else {
							0.
						}
					});
					let un = dot(&ul, &face.normal);
					for (i, component, jump, _) in face_basis(face, &bary, self.dimension) {
						force[i] -= face.measure
							* weight
							* un
							* 0.5
							* (ul[component] + ur[component])
							* jump;
					}
				}
			}
		}
		for face in &self.faces {
			if face.right.is_none() {
				for (bary, weight) in face_quadrature(self.dimension) {
					let u: Point = std::array::from_fn(|k| {
						if k < self.dimension {
							face.left_nodes
								.iter()
								.zip(&bary)
								.map(|(&node, l)| c[face.left * local + k * nodes + node] * l)
								.sum()
						} else {
							0.
						}
					});
					let un = dot(&u, &face.normal);
					// Central prescribed trace: a smooth quadratic drift without sign switches.
					let exterior = face.prescribed.as_ref().map_or(u, |values| {
						std::array::from_fn(|k| {
							0.5 * (u[k]
								+ boundary_scale
									* values.iter().zip(&bary).map(|(v, l)| v[k] * l).sum::<f64>())
						})
					});
					for (i, k, trace, _) in face_basis(face, &bary, self.dimension) {
						force[i] -= face.measure * weight * un * exterior[k] * trace;
					}
				}
			}
		}

		force
	}

	/// Integrated kinetic energy, not divided by domain volume.
	///
	/// # Errors
	/// Rejects malformed states or nonfinite energy.
	pub fn energy(&self, state: &[f64]) -> Result<f64, CfdError> {
		let c = self.coefficients(state)?;
		let energy = 0.5 * dot(&c, &mass_apply(&self.cells, self.dimension, &c));
		if !energy.is_finite() {
			return Err(CfdError::InvalidInput("simplex energy overflow"));
		}
		Ok(energy)
	}

	/// Maximum pointwise cell divergence of a complete chart velocity.
	///
	/// # Errors
	/// Rejects malformed states.
	pub fn divergence_residual(&self, state: &[f64]) -> Result<f64, CfdError> {
		let c = self.coefficients(state)?;
		let nodes = self.dimension + 1;
		let local = self.dimension * nodes;
		Ok(self
			.cells
			.iter()
			.enumerate()
			.map(|(ci, cell)| {
				(0..self.dimension)
					.flat_map(|k| (0..nodes).map(move |node| (k, node)))
					.map(|(k, node)| c[ci * local + k * nodes + node] * cell.gradients[node][k])
					.sum::<f64>()
					.abs()
			})
			.fold(0., f64::max))
	}

	/// Classical RK4 reference for this exact complete physical DG ODE.
	///
	/// # Errors
	/// Rejects invalid states, time steps or overflow during integration.
	pub fn integrate_rk4(
		&self,
		initial: &[f64],
		dt: f64,
		steps: usize,
	) -> Result<Vec<f64>, CfdError> {
		if !dt.is_finite() || dt <= 0. {
			return Err(CfdError::InvalidInput("invalid reference time step"));
		}
		self.coefficients(initial)?;
		let mut state = initial.to_vec();
		for _ in 0..steps {
			let k1 = self.drift(&state)?;
			let shifted = |k: &[f64], f: f64| {
				state
					.iter()
					.zip(k)
					.map(|(s, k)| s + dt * f * k)
					.collect::<Vec<_>>()
			};
			let k2 = self.drift(&shifted(&k1, 0.5))?;
			let k3 = self.drift(&shifted(&k2, 0.5))?;
			let k4 = self.drift(&shifted(&k3, 1.))?;
			for i in 0..state.len() {
				state[i] += dt * (k1[i] + 2. * k2[i] + 2. * k3[i] + k4[i]) / 6.;
			}
			self.coefficients(&state)?;
		}
		Ok(state)
	}
}

/// Full independent dimension of a unit box fixture, certified by actual constraint assembly.
///
/// # Errors
/// Propagates mesh admission and assembly certificate errors.
pub fn box_chart_dimension(
	dimension: usize,
	subdivisions: u32,
	periodic: bool,
) -> Result<usize, CfdError> {
	Ok(SimplexBdm::box_mesh(
		dimension,
		subdivisions,
		1.,
		0.,
		if periodic {
			BoxBoundary::Periodic
		} else {
			BoxBoundary::Cavity { lid_speed: 0. }
		},
	)?
	.dimension())
}

fn projection_quadrature(dimension: usize) -> Vec<(Vec<f64>, f64)> {
	let gauss = [
		(0.069_431_844_202_973_71, 0.173_927_422_568_726_93),
		(0.330_009_478_207_571_87, 0.326_072_577_431_273_07),
		(0.669_990_521_792_428_1, 0.326_072_577_431_273_07),
		(0.930_568_155_797_026_2, 0.173_927_422_568_726_93),
	];
	let mut quadrature = Vec::new();
	for (r, wr) in gauss {
		for (s, ws) in gauss {
			if dimension == 2 {
				quadrature.push((vec![1. - r, r * (1. - s), r * s], 2. * r * wr * ws));
			} else {
				for (t, wt) in gauss {
					quadrature.push((
						vec![1. - r, r * (1. - s), r * s * (1. - t), r * s * t],
						6. * r * r * s * wr * ws * wt,
					));
				}
			}
		}
	}
	quadrature
}

/// Full momentum/continuity and pressure gauge evidence on a box mesh.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SimplexPressureRecovery {
	/// One P0 pressure per cell, in the volume-weighted zero-mean gauge.
	pub cell_pressure: Vec<f64>,
	/// Mean-zero on closed/periodic domains; natural-outflow traction fixes pressure on open domains.
	pub gauge: String,
	/// Hybrid normal-trace constraint multipliers, in face-vertex order.
	pub normal_multipliers: Vec<f64>,
	/// Infinity norm of the full local momentum residual.
	pub momentum_residual: f64,
	/// Infinity norm of cellwise pointwise divergence.
	pub continuity_residual: f64,
	/// Absolute mean-pressure gauge residual on closed domains; zero when outflow fixes the gauge.
	pub pressure_mean_residual: f64,
}

impl SimplexBdm {
	/// Evaluate the DG velocity at a physical point. At interfaces the first containing
	/// cell supplies the trace, because tangential DG traces need not agree.
	///
	/// # Errors
	/// Rejects malformed states, nonfinite points or points outside the mesh.
	pub fn sample_velocity(&self, state: &[f64], point: Point) -> Result<Point, CfdError> {
		let c = self.coefficients(state)?;
		self.sample_coefficients(&c, point)
	}

	fn sample_coefficients(&self, c: &[f64], point: Point) -> Result<Point, CfdError> {
		if point.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("nonfinite velocity probe"));
		}
		let nodes = self.dimension + 1;
		let local = self.dimension * nodes;
		for (ci, cell) in self.cells.iter().enumerate() {
			let delta = sub(point, cell.vertices[0]);
			let bary: Vec<_> = cell
				.gradients
				.iter()
				.enumerate()
				.map(|(i, g)| dot(g, &delta) + f64::from(i == 0))
				.collect();
			if bary.iter().all(|&l| (-1e-10..=1. + 1e-10).contains(&l))
				&& (self.dimension == 3 || point[2].abs() < 1e-10)
			{
				return Ok(std::array::from_fn(|k| {
					if k < self.dimension {
						(0..nodes)
							.map(|node| c[ci * local + k * nodes + node] * bary[node])
							.sum()
					} else {
						0.
					}
				}));
			}
		}
		Err(CfdError::InvalidInput(
			"velocity probe lies outside the box",
		))
	}

	/// L2 velocity error integrated independently with tensor Gauss/Duffy quadrature.
	///
	/// # Errors
	/// Rejects malformed states or nonfinite analytic values.
	pub fn velocity_error_l2(
		&self,
		state: &[f64],
		field: impl Fn(Point) -> Point,
	) -> Result<f64, CfdError> {
		let c = self.coefficients(state)?;
		let nodes = self.dimension + 1;
		let local = self.dimension * nodes;
		let mut error = 0.;
		for (ci, cell) in self.cells.iter().enumerate() {
			for (bary, weight) in projection_quadrature(self.dimension) {
				let point = std::array::from_fn(|k| {
					cell.vertices.iter().zip(&bary).map(|(p, l)| p[k] * l).sum()
				});
				let exact = field(point);
				if exact.iter().any(|v| !v.is_finite()) {
					return Err(CfdError::InvalidInput("nonfinite analytic velocity"));
				}
				for k in 0..self.dimension {
					let value = (0..nodes)
						.map(|node| c[ci * local + k * nodes + node] * bary[node])
						.sum::<f64>();
					error += cell.volume * weight * (value - exact[k]).powi(2);
				}
			}
		}
		if !error.is_finite() {
			return Err(CfdError::InvalidInput("velocity error overflow"));
		}
		Ok(error.sqrt())
	}

	/// Integrated enstrophy 0.5 times the squared vorticity norm.
	///
	/// # Errors
	/// Rejects malformed states or overflow.
	pub fn enstrophy(&self, state: &[f64]) -> Result<f64, CfdError> {
		let c = self.coefficients(state)?;
		let nodes = self.dimension + 1;
		let local = self.dimension * nodes;
		let mut result = 0.;
		for (ci, cell) in self.cells.iter().enumerate() {
			let grad: [[f64; 3]; 3] = std::array::from_fn(|k| {
				std::array::from_fn(|j| {
					if k < self.dimension {
						(0..nodes)
							.map(|node| c[ci * local + k * nodes + node] * cell.gradients[node][j])
							.sum()
					} else {
						0.
					}
				})
			});
			let curl = [
				grad[2][1] - grad[1][2],
				grad[0][2] - grad[2][0],
				grad[1][0] - grad[0][1],
			];
			result += 0.5 * cell.volume * dot(&curl, &curl);
		}
		if !result.is_finite() {
			return Err(CfdError::InvalidInput("enstrophy overflow"));
		}
		Ok(result)
	}

	/// Reconstruct mean-zero P0 pressure and hybrid forces from every momentum equation.
	///
	/// # Errors
	/// Rejects malformed states or a singular pressure recovery system.
	pub fn reconstruct_pressure(&self, state: &[f64]) -> Result<SimplexPressureRecovery, CfdError> {
		self.reconstruct_pressure_with_boundary_scale(state, 1., 0.)
	}

	/// Reconstruct pressure from full momentum including the physical lifting acceleration.
	/// # Errors
	/// Rejects malformed state/boundary data or singular recovery.
	pub fn reconstruct_pressure_with_boundary_scale(
		&self,
		state: &[f64],
		scale: f64,
		rate: f64,
	) -> Result<SimplexPressureRecovery, CfdError> {
		let c = self.coefficients_with_boundary_scale(state, scale)?;
		let a = self.coefficients_with_boundary_scale(
			&self.drift_with_boundary_scale(state, scale, rate)?,
			rate,
		)?;
		let force = self.force(&c, scale);
		let ma = mass_apply(&self.cells, self.dimension, &a);
		let residual: Vec<_> = force.iter().zip(&ma).map(|(f, m)| f - m).collect();
		let mut rows = constraints(&self.cells, &self.faces, self.dimension);
		let normal_count = rows.len() - self.cells.len();
		let closed = !self.faces.iter().any(|f| f.outflow);
		if closed {
			let last = rows
				.pop()
				.ok_or(CfdError::Assembly("empty pressure constraints"))?;
			let last_volume = self
				.cells
				.last()
				.ok_or(CfdError::Assembly("empty mesh"))?
				.volume;
			for (row, cell) in rows.iter_mut().skip(normal_count).zip(&self.cells) {
				let gauge_weight = cell.volume / last_volume;
				for (v, l) in row.iter_mut().zip(&last) {
					*v -= gauge_weight * l;
				}
			}
		}
		if rows.len() != self.diagnostics.constraint_rank {
			return Err(CfdError::Assembly("pressure chart rank mismatch"));
		}
		let gram = rows
			.iter()
			.map(|a| rows.iter().map(|b| dot(a, b)).collect())
			.collect();
		let rhs = rows.iter().map(|r| dot(r, &residual)).collect();
		let multipliers = crate::bdm::solve(gram, rhs)?;
		let momentum_residual = residual
			.iter()
			.enumerate()
			.map(|(i, r)| {
				(r - rows
					.iter()
					.zip(&multipliers)
					.map(|(row, l)| row[i] * l)
					.sum::<f64>())
				.abs()
			})
			.fold(0., f64::max);
		let mut cell_pressure: Vec<_> = multipliers.iter().skip(normal_count).map(|p| -p).collect();
		if closed {
			let last_volume = self
				.cells
				.last()
				.ok_or(CfdError::Assembly("empty mesh"))?
				.volume;
			let weighted_sum = cell_pressure
				.iter()
				.zip(&self.cells)
				.map(|(p, c)| p * c.volume)
				.sum::<f64>();
			cell_pressure.push(-weighted_sum / last_volume);
		}
		let volume = self.cells.iter().map(|c| c.volume).sum::<f64>();
		let pressure_mean_residual = (cell_pressure
			.iter()
			.zip(&self.cells)
			.map(|(p, c)| p * c.volume)
			.sum::<f64>()
			/ volume)
			.abs();
		Ok(SimplexPressureRecovery {
			cell_pressure,
			gauge: if closed {
				"volume-weighted zero mean".into()
			} else {
				"natural outflow traction fixes pressure level".into()
			},
			normal_multipliers: multipliers[..normal_count].to_vec(),
			momentum_residual,
			continuity_residual: self.divergence_residual(state)?,
			pressure_mean_residual: if closed { pressure_mean_residual } else { 0. },
		})
	}
}

/// Exterior simplex facet with an explicit stationary velocity trace or natural outflow.
#[derive(Clone, Debug)]
pub struct BoundaryFacet {
	/// Global vertex identifiers in trace-value order.
	pub vertices: Vec<usize>,
	/// One velocity value per facet vertex; `None` is natural outflow.
	pub velocity: Option<Vec<[f64; 3]>>,
	/// Named boundary for traction/flux measurements.
	pub label: String,
}

/// Explicit correspondence of opposite periodic exterior facets.
#[derive(Clone, Debug)]
pub struct PeriodicFacets {
	/// Global vertex identifiers on one periodic side.
	pub left: Vec<usize>,
	/// Corresponding global vertex identifiers on the opposite side.
	pub right: Vec<usize>,
}

fn boundary_values(cells: &[Cell], faces: &[Face], dimension: usize) -> Vec<f64> {
	let mut values = Vec::new();
	for face in faces {
		if !face.outflow {
			for node in 0..dimension {
				values.push(
					face.prescribed
						.as_ref()
						.map_or(0., |g| dot(&g[node], &face.normal)),
				);
			}
		}
	}
	values.resize(values.len() + cells.len(), 0.);
	values
}

fn boundary_lifting(
	cells: &[Cell],
	faces: &[Face],
	dimension: usize,
	rows: &[Vec<f64>],
	chart: &[Vec<f64>],
) -> Result<Vec<f64>, CfdError> {
	let values = boundary_values(cells, faces, dimension);
	let size = cells.len() * dimension * (dimension + 1);
	if values.iter().all(|v| v.abs() < 1e-15) {
		return Ok(vec![0.; size]);
	}
	let count = rows.len() - usize::from(!faces.iter().any(|f| f.outflow));
	let gram = rows[..count]
		.iter()
		.map(|a| rows[..count].iter().map(|b| dot(a, b)).collect())
		.collect();
	let lambda = crate::bdm::solve(gram, values[..count].to_vec())?;
	let mut lifting: Vec<_> = (0..size)
		.map(|i| {
			rows[..count]
				.iter()
				.zip(&lambda)
				.map(|(r, l)| r[i] * l)
				.sum()
		})
		.collect();
	let ml = mass_apply(cells, dimension, &lifting);
	for q in chart {
		let projection = dot(q, &ml);
		for (l, v) in lifting.iter_mut().zip(q) {
			*l -= projection * v;
		}
	}
	if rows
		.iter()
		.zip(&values)
		.any(|(r, b)| (dot(r, &lifting) - b).abs() > 1e-8)
	{
		return Err(CfdError::Assembly(
			"incompatible or inaccurate prescribed normal lifting",
		));
	}
	Ok(lifting)
}

impl SimplexBdm {
	/// Assemble an explicitly specified simplex mesh and its stationary boundary lifting.
	///
	/// # Errors
	/// Rejects malformed topology, incomplete boundary conditions, unbudgeted meshes,
	/// incompatible prescribed normal flux or failed rank certificates.
	pub fn from_mesh(
		dimension: usize,
		vertices: &[[f64; 3]],
		connectivity: &[Vec<usize>],
		boundaries: &[BoundaryFacet],
		periodic: &[PeriodicFacets],
		viscosity: f64,
	) -> Result<Self, CfdError> {
		if ![2, 3].contains(&dimension)
			|| !viscosity.is_finite()
			|| viscosity < 0.
			|| vertices.iter().flatten().any(|v| !v.is_finite())
			|| connectivity.is_empty()
		{
			return Err(CfdError::InvalidInput("invalid explicit mesh parameters"));
		}
		if connectivity
			.len()
			.checked_mul(dimension * (dimension + 1))
			.is_none_or(|n| n > 768)
		{
			return Err(CfdError::Unsupported(
				"dense full-chart reference limited to 768 local velocity coefficients".into(),
			));
		}
		let mut cells = Vec::new();
		let mut incidence: BTreeMap<Vec<usize>, Vec<(usize, Vec<usize>)>> = BTreeMap::new();
		for (ci, cell) in connectivity.iter().enumerate() {
			if cell.len() != dimension + 1 || cell.iter().any(|&v| v >= vertices.len()) {
				return Err(CfdError::InvalidInput("invalid simplex connectivity"));
			}
			let mut unique = cell.clone();
			unique.sort_unstable();
			unique.dedup();
			if unique.len() != cell.len() {
				return Err(CfdError::InvalidInput("repeated simplex vertex"));
			}
			cells.push(physical_cell(
				cell.iter().map(|&v| vertices[v]).collect(),
				vec![[0; 3]; dimension + 1],
				dimension,
			)?);
			for omitted in 0..=dimension {
				let nodes: Vec<_> = (0..=dimension).filter(|&i| i != omitted).collect();
				let mut key: Vec<_> = nodes.iter().map(|&i| cell[i]).collect();
				key.sort_unstable();
				incidence.entry(key).or_default().push((ci, nodes));
			}
		}
		let mut faces = Vec::new();
		let mut exterior = BTreeMap::new();
		for (key, pair) in incidence {
			if pair.len() == 2 {
				faces.push(make_face(&cells, &pair[0], Some(&pair[1]), dimension, 0)?);
			} else if pair.len() == 1 {
				exterior.insert(key, pair[0].clone());
			} else {
				return Err(CfdError::InvalidInput("nonmanifold simplex facet"));
			}
		}
		for pair in periodic {
			let mut left = pair.left.clone();
			left.sort_unstable();
			let mut right = pair.right.clone();
			right.sort_unstable();
			let l = exterior.remove(&left).ok_or(CfdError::InvalidInput(
				"periodic left facet missing or duplicated",
			))?;
			let r = exterior.remove(&right).ok_or(CfdError::InvalidInput(
				"periodic right facet missing or duplicated",
			))?;
			faces.push(make_face(&cells, &l, Some(&r), dimension, 0)?);
		}
		assign_boundaries(
			&mut faces,
			&mut exterior,
			boundaries,
			&cells,
			connectivity,
			dimension,
		)?;
		if !exterior.is_empty() {
			return Err(CfdError::InvalidInput("unassigned exterior mesh facet"));
		}
		Self::assemble_parts(dimension, viscosity, BoxBoundary::Mixed, cells, faces)
	}

	/// Maximum normal-trace, prescribed boundary and integrated divergence residual.
	///
	/// # Errors
	/// Rejects malformed states.
	pub fn boundary_residual(&self, state: &[f64]) -> Result<f64, CfdError> {
		self.boundary_residual_with_scale(state, 1.)
	}

	/// Maximum constraint residual for the prescribed trace multiplied by `scale`.
	/// # Errors
	/// Rejects malformed state or nonfinite scale.
	pub fn boundary_residual_with_scale(&self, state: &[f64], scale: f64) -> Result<f64, CfdError> {
		let c = self.coefficients_with_boundary_scale(state, scale)?;
		let rows = constraints(&self.cells, &self.faces, self.dimension);
		let values = boundary_values(&self.cells, &self.faces, self.dimension);
		Ok(rows
			.iter()
			.zip(values)
			.map(|(r, b)| (dot(r, &c) - scale * b).abs())
			.fold(0., f64::max))
	}

	/// Physical volume of the polygonal/polyhedral mesh.
	#[must_use]
	pub fn volume(&self) -> f64 {
		self.cells.iter().map(|c| c.volume).sum()
	}
}

impl SimplexBdm {
	/// Integrate physical pressure plus unsymmetrized viscous traction on a named solid
	/// boundary, with sign giving force exerted by the fluid on the solid.
	///
	/// # Errors
	/// Rejects malformed states/pressure or a label without exterior facets.
	pub fn boundary_force(
		&self,
		state: &[f64],
		pressure: &[f64],
		label: &str,
	) -> Result<[f64; 3], CfdError> {
		let c = self.coefficients(state)?;
		if pressure.len() != self.cells.len() || pressure.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("invalid boundary-traction pressure"));
		}
		let nodes = self.dimension + 1;
		let local = self.dimension * nodes;
		let mut force = [0.; 3];
		let mut found = false;
		for face in self
			.faces
			.iter()
			.filter(|f| f.right.is_none() && f.label == label)
		{
			found = true;
			let cell = &self.cells[face.left];
			for k in 0..self.dimension {
				let grad: Point = std::array::from_fn(|j| {
					(0..nodes)
						.map(|node| {
							c[face.left * local + k * nodes + node] * cell.gradients[node][j]
						})
						.sum()
				});
				force[k] += face.measure
					* (pressure[face.left] * face.normal[k]
						- self.viscosity * dot(&grad, &face.normal));
			}
		}
		if !found || force.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput(
				"boundary label missing or traction overflow",
			));
		}
		Ok(force)
	}
}

/// Admission limits for cellwise pressure probes, independent of caller allocation capacity.
#[derive(Clone, Copy, Debug)]
pub struct PressureProbeLimits {
	/// Number of requested points.
	pub max_points: usize,
	/// Conservative scalar work: pressure validation plus 64 units per point/cell pair.
	pub max_work: usize,
	/// Accessible pressure/point slices, output values and 256 bytes of scalar scratch.
	/// The already prepared mesh and unused capacity in caller buffers are separate.
	pub max_bytes: usize,
}
impl Default for PressureProbeLimits {
	fn default() -> Self {
		Self {
			max_points: 65_536,
			max_work: 100_000_000,
			max_bytes: 8 * 1024 * 1024,
		}
	}
}

impl SimplexBdm {
	/// Batch physical velocity probes while reconstructing the full coefficient vector once.
	///
	/// # Errors
	/// Rejects malformed states and nonfinite/out-of-domain points.
	pub fn sample_velocities(
		&self,
		state: &[f64],
		points: &[[f64; 3]],
	) -> Result<Vec<[f64; 3]>, CfdError> {
		let c = self.coefficients(state)?;
		points
			.iter()
			.map(|&p| self.sample_coefficients(&c, p))
			.collect()
	}

	/// Sample cellwise P0 pressure, averaging incident fluid-cell traces at facets/vertices.
	///
	/// The trace convention is explicit because discontinuous pressure has no unique
	/// point value at interfaces. This bounded physical probe does not perform global
	/// pressure interpolation or claim a continuum boundary-trace error bound.
	/// # Errors
	/// Rejects malformed pressure, nonfinite points and points outside the physical mesh.
	pub fn sample_pressures(
		&self,
		pressure: &[f64],
		points: &[[f64; 3]],
	) -> Result<Vec<f64>, CfdError> {
		self.sample_pressures_with_limits(pressure, points, PressureProbeLimits::default())
	}

	/// Pressure probes with checked work/storage admission before scanning points or allocating.
	///
	/// Uses the same incident-cell trace convention as [`Self::sample_pressures`].
	/// # Errors
	/// Rejects exceeded limits, allocation failure, invalid inputs and arithmetic overflow.
	pub fn sample_pressures_with_limits(
		&self,
		pressure: &[f64],
		points: &[[f64; 3]],
		limits: PressureProbeLimits,
	) -> Result<Vec<f64>, CfdError> {
		let work = points
			.len()
			.checked_mul(self.cells.len())
			.and_then(|n| n.checked_mul(64))
			.and_then(|n| n.checked_add(pressure.len()));
		let bytes = points
			.len()
			.checked_mul(32)
			.and_then(|n| pressure.len().checked_mul(8).and_then(|p| n.checked_add(p)))
			.and_then(|n| n.checked_add(256));
		if points.len() > limits.max_points
			|| work.is_none_or(|n| n > limits.max_work)
			|| bytes.is_none_or(|n| n > limits.max_bytes)
		{
			return Err(CfdError::InvalidInput(
				"pressure probe work or storage budget",
			));
		}
		if pressure.len() != self.cells.len() || pressure.iter().any(|p| !p.is_finite()) {
			return Err(CfdError::InvalidInput("invalid P0 pressure probes"));
		}
		let mut output = Vec::new();
		output
			.try_reserve_exact(points.len())
			.map_err(|_| CfdError::InvalidInput("pressure probe allocation"))?;
		for &point in points {
			if point.iter().any(|v| !v.is_finite())
				|| (self.dimension == 2 && point[2].abs() >= 1e-10)
			{
				return Err(CfdError::InvalidInput("invalid pressure probe location"));
			}
			let mut sum = 0.;
			let mut count = 0_u32;
			for (cell, &p) in self.cells.iter().zip(pressure) {
				let delta = sub(point, cell.vertices[0]);
				if cell.gradients.iter().enumerate().all(|(i, g)| {
					(-1e-10..=1. + 1e-10).contains(&(dot(g, &delta) + f64::from(i == 0)))
				}) {
					sum += p;
					count = count
						.checked_add(1)
						.ok_or(CfdError::InvalidInput("pressure probe cell count"))?;
				}
			}
			if count == 0 || !sum.is_finite() {
				return Err(CfdError::InvalidInput(
					"pressure probe outside mesh or trace overflow",
				));
			}
			output.push(sum / f64::from(count));
		}
		Ok(output)
	}

	/// L2 error of the recovered cellwise P0 pressure against a prescribed analytic gauge.
	///
	/// # Errors
	/// Rejects invalid pressure values or nonfinite analytic data.
	pub fn pressure_error_l2(
		&self,
		pressure: &[f64],
		field: impl Fn([f64; 3]) -> f64,
	) -> Result<f64, CfdError> {
		if pressure.len() != self.cells.len() || pressure.iter().any(|p| !p.is_finite()) {
			return Err(CfdError::InvalidInput("invalid P0 pressure coefficients"));
		}
		let mut error = 0.;
		for (cell, &p) in self.cells.iter().zip(pressure) {
			for (bary, weight) in projection_quadrature(self.dimension) {
				let point = std::array::from_fn(|k| {
					cell.vertices.iter().zip(&bary).map(|(p, l)| p[k] * l).sum()
				});
				let exact = field(point);
				if !exact.is_finite() {
					return Err(CfdError::InvalidInput("nonfinite analytic pressure"));
				}
				error += cell.volume * weight * (p - exact).powi(2);
			}
		}
		if !error.is_finite() {
			return Err(CfdError::InvalidInput("pressure error overflow"));
		}
		Ok(error.sqrt())
	}

	/// Physical gradient dissipation nu times the elementwise squared velocity gradient.
	/// SIP jump/consistency dissipation is not included in this continuum observable.
	///
	/// # Errors
	/// Rejects malformed states or overflow.
	pub fn gradient_dissipation(&self, state: &[f64]) -> Result<f64, CfdError> {
		let c = self.coefficients(state)?;
		let nodes = self.dimension + 1;
		let local = self.dimension * nodes;
		let mut result = 0.;
		for (ci, cell) in self.cells.iter().enumerate() {
			for k in 0..self.dimension {
				for j in 0..self.dimension {
					let derivative = (0..nodes)
						.map(|node| c[ci * local + k * nodes + node] * cell.gradients[node][j])
						.sum::<f64>();
					result += self.viscosity * cell.volume * derivative * derivative;
				}
			}
		}
		if !result.is_finite() {
			return Err(CfdError::InvalidInput("gradient dissipation overflow"));
		}
		Ok(result)
	}
}

/// Exact local velocity and constraint-rank counts for a connected box triangulation,
/// computed from BDM1 facet unisolvence and the one closed-domain pressure redundancy.
///
/// No dense chart or mesh allocation is required.
///
/// # Errors
/// Rejects unsupported dimensions, zero subdivisions or machine-size topology overflow.
pub fn box_chart_dimensions(
	dimension: usize,
	subdivisions: u32,
	periodic: bool,
) -> Result<(usize, usize), CfdError> {
	if ![2, 3].contains(&dimension) || subdivisions == 0 {
		return Err(CfdError::InvalidInput("invalid box topology"));
	}
	let n = usize::try_from(subdivisions)
		.map_err(|_| CfdError::InvalidInput("subdivision count overflow"))?;
	let power =
		u32::try_from(dimension).map_err(|_| CfdError::InvalidInput("dimension overflow"))?;
	let cells = n
		.checked_pow(power)
		.and_then(|v| v.checked_mul(if dimension == 2 { 2 } else { 6 }))
		.ok_or(CfdError::InvalidInput("cell count overflow"))?;
	let local = cells
		.checked_mul(dimension * (dimension + 1))
		.ok_or(CfdError::InvalidInput("velocity count overflow"))?;
	let boundary = if periodic {
		0
	} else {
		n.checked_pow(power - 1)
			.and_then(|v| v.checked_mul(if dimension == 2 { 4 } else { 12 }))
			.ok_or(CfdError::InvalidInput("boundary count overflow"))?
	};
	let trace = cells
		.checked_mul(dimension + 1)
		.and_then(|v| v.checked_add(boundary))
		.and_then(|v| v.checked_mul(dimension))
		.map(|v| v / 2)
		.ok_or(CfdError::InvalidInput("trace count overflow"))?;
	let rank = trace
		.checked_add(cells - 1)
		.ok_or(CfdError::InvalidInput("constraint count overflow"))?;
	Ok((local, rank))
}

fn assign_boundaries(
	faces: &mut Vec<Face>,
	exterior: &mut BTreeMap<Vec<usize>, FacetIncidence>,
	boundaries: &[BoundaryFacet],
	cells: &[Cell],
	connectivity: &[Vec<usize>],
	dimension: usize,
) -> Result<(), CfdError> {
	for boundary in boundaries {
		let mut key = boundary.vertices.clone();
		key.sort_unstable();
		let incidence = exterior.remove(&key).ok_or(CfdError::InvalidInput(
			"prescribed facet missing or duplicated",
		))?;
		let mut face = make_face(cells, &incidence, None, dimension, 0)?;
		face.lid = false;
		face.label.clone_from(&boundary.label);
		if let Some(values) = &boundary.velocity {
			if values.len() != dimension || values.iter().flatten().any(|v| !v.is_finite()) {
				return Err(CfdError::InvalidInput("invalid prescribed facet velocity"));
			}
			let ordered: Result<Vec<_>, _> = face
				.left_nodes
				.iter()
				.map(|&node| {
					boundary
						.vertices
						.iter()
						.position(|&id| id == connectivity[incidence.0][node])
						.map(|i| values[i])
						.ok_or(CfdError::InvalidInput("facet trace mapping failed"))
				})
				.collect();
			face.prescribed = Some(ordered?);
		} else {
			face.prescribed = None;
			face.outflow = true;
		}
		faces.push(face);
	}
	Ok(())
}

#[cfg(test)]
mod retained_source_tests {
	use super::*;
	use crate::physical_observation::{
		PhysicalObservableKind, PhysicalObservableLimits, PreparedPhysicalObservable,
	};
	#[test]
	#[allow(
		clippy::panic_in_result_fn,
		reason = "Assertions verify bounded source-capacity admission"
	)]
	fn excess_simplex_chart_capacity_is_rejected_by_capture_admission() -> Result<(), CfdError> {
		let mut model = SimplexBdm::box_mesh(2, 1, 1., 0.02, BoxBoundary::Periodic)?;
		let base = PreparedPhysicalObservable::simplex(
			&model,
			PhysicalObservableKind::Enstrophy,
			PhysicalObservableLimits::default(),
		)?;
		assert_eq!(
			base.resources().borrowed_source_bytes,
			model.retained_bytes()?
		);
		let previous_bytes = model.retained_bytes()?;
		let previous_capacity = model.chart[0].capacity();
		model.chart[0]
			.try_reserve_exact(100_000)
			.map_err(|_| CfdError::InvalidInput("test capacity allocation"))?;
		assert_eq!(
			model.retained_bytes()? - previous_bytes,
			8 * (model.chart[0].capacity() - previous_capacity)
		);
		assert!(
			PreparedPhysicalObservable::simplex(
				&model,
				PhysicalObservableKind::Enstrophy,
				PhysicalObservableLimits {
					max_prepare_bytes: base.resources().prepare_peak_bytes + 1024,
					..Default::default()
				}
			)
			.is_err()
		);
		Ok(())
	}
}
