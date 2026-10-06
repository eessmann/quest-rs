//! Fixed-count simplicial incidence and vertex/edge link validation.
#![allow(
	clippy::too_many_lines,
	reason = "Complete link/incidence validation remains one auditable bounded pass"
)]
#![allow(
	clippy::large_stack_arrays,
	reason = "A fixed charged 128 KiB topology workspace avoids allocation before admission"
)]
use super::mesh::{AffineMeshView, Counts, PhysicalMeshLimits, add, invalid, mul};
use super::{Cell, CfdError, Face};
#[derive(Clone, Copy)]
struct Incidence {
	cell: usize,
	nodes: [usize; 3],
	ids: [usize; 3],
}
const EMPTY: Incidence = Incidence {
	cell: 0,
	nodes: [0; 3],
	ids: [usize::MAX; 3],
};
#[derive(Clone, Copy)]
pub(super) struct FacetPlan {
	pub left: usize,
	pub right: Option<usize>,
	pub left_nodes: [usize; 3],
	pub right_nodes: [usize; 3],
	pub label: Option<usize>,
	pub periodic: bool,
	pub natural: bool,
}
fn key(ids: &[usize]) -> [usize; 3] {
	let mut k = [usize::MAX; 3];
	k[..ids.len()].copy_from_slice(ids);
	k.sort_unstable();
	k
}
fn key_inc(f: Incidence, d: usize) -> [usize; 3] {
	key(&f.ids[..d])
}
fn graph(edges: &[(usize, usize)], boundary: bool) -> Result<(), CfdError> {
	if edges.is_empty() {
		return Err(invalid());
	}
	let mut degrees = [0usize; 512];
	let mut seen = [false; 512];
	for &(a, b) in edges {
		if a == b {
			return Err(invalid());
		}
		degrees[a] += 1;
		degrees[b] += 1;
	}
	let ends = degrees.iter().filter(|&&v| v == 1).count();
	if degrees.iter().any(|&v| v > 2) || (boundary && ends != 2) || (!boundary && ends != 0) {
		return Err(invalid());
	}
	let mut queue = [0usize; 512];
	let mut head = 0;
	let mut tail = 1;
	queue[0] = edges[0].0;
	seen[queue[0]] = true;
	while head < tail {
		let x = queue[head];
		head += 1;
		for &(a, b) in edges {
			let y = if a == x {
				Some(b)
			} else if b == x {
				Some(a)
			} else {
				None
			};
			if let Some(y) = y
				&& !seen[y]
			{
				seen[y] = true;
				queue[tail] = y;
				tail += 1;
			}
		}
	}
	if degrees
		.iter()
		.zip(seen)
		.any(|(&degree, visited)| degree != 0 && !visited)
	{
		return Err(invalid());
	}
	Ok(())
}
fn links(view: AffineMeshView<'_>, raw: &[Incidence], exterior: &[bool]) -> Result<(), CfdError> {
	let d = view.dimension;
	let mut edges = [(0usize, 0usize); 128];
	for vertex in 0..view.vertices.len() {
		let boundary = raw
			.iter()
			.zip(exterior)
			.any(|(f, &outer)| outer && f.ids[..d].contains(&vertex));
		if d == 2 {
			let mut count = 0;
			for cell in view.cells {
				if cell.contains(&vertex) {
					let mut other = [0; 2];
					let mut j = 0;
					for &v in *cell {
						if v != vertex {
							other[j] = v;
							j += 1;
						}
					}
					edges[count] = other.into();
					count += 1;
				}
			}
			if raw
				.iter()
				.zip(exterior)
				.filter(|(f, outer)| **outer && f.ids[..d].contains(&vertex))
				.count()
				!= if boundary { 2 } else { 0 }
			{
				return Err(invalid());
			}
			graph(&edges[..count], boundary)?;
		} else {
			let mut triangles = [[0usize; 3]; 128];
			let mut nt = 0;
			for cell in view.cells {
				if cell.contains(&vertex) {
					let mut j = 0;
					for &v in *cell {
						if v != vertex {
							triangles[nt][j] = v;
							j += 1;
						}
					}
					nt += 1;
				}
			}
			let mut vertices = [false; 512];
			let mut all_edges = [(0usize, 0usize); 384];
			let mut multiplicity = [0usize; 384];
			let mut ne = 0;
			for tri in &triangles[..nt] {
				for &v in tri {
					vertices[v] = true;
				}
				for (a, b) in [(tri[0], tri[1]), (tri[0], tri[2]), (tri[1], tri[2])] {
					let e = (a.min(b), a.max(b));
					let i = all_edges[..ne]
						.iter()
						.position(|&old| old == e)
						.unwrap_or(ne);
					if i == ne {
						all_edges[ne] = e;
						ne += 1;
					}
					multiplicity[i] += 1;
					if multiplicity[i] > 2 {
						return Err(invalid());
					}
				}
			}
			let mut boundary_edges = [(0usize, 0usize); 384];
			let mut nb = 0;
			for i in 0..ne {
				if multiplicity[i] == 1 {
					boundary_edges[nb] = all_edges[i];
					nb += 1;
				}
			}
			let nv = vertices.iter().filter(|&&x| x).count();
			if nv + nt != ne + if boundary { 1 } else { 2 } {
				return Err(invalid());
			}
			if boundary {
				graph(&boundary_edges[..nb], false)?;
			} else if nb != 0 {
				return Err(invalid());
			}
			// Link triangles must connect through complete edges, not only vertices.
			let mut visited = [false; 128];
			visited[0] = true;
			let mut queue = [0usize; 128];
			let mut head = 0;
			let mut tail = 1;
			while head < tail {
				let i = queue[head];
				head += 1;
				for j in 0..nt {
					if !visited[j]
						&& triangles[i]
							.iter()
							.filter(|v| triangles[j].contains(v))
							.count()
							>= 2
					{
						visited[j] = true;
						queue[tail] = j;
						tail += 1;
					}
				}
			}

			if visited[..nt].iter().any(|&x| !x) {
				return Err(invalid());
			}
		}
	}
	if d == 3 {
		let mut unique = [(0usize, 0usize); 768];
		let mut total = 0;
		for cell in view.cells {
			for i in 0..4 {
				for j in i + 1..4 {
					let e = (cell[i].min(cell[j]), cell[i].max(cell[j]));
					if !unique[..total].contains(&e) {
						unique[total] = e;
						total += 1;
					}
				}
			}
		}
		for &(a, b) in &unique[..total] {
			let mut count = 0;
			for cell in view.cells {
				if cell.contains(&a) && cell.contains(&b) {
					let mut other = [0; 2];
					let mut j = 0;
					for &v in *cell {
						if v != a && v != b {
							other[j] = v;
							j += 1;
						}
					}
					edges[count] = other.into();
					count += 1;
				}
			}
			let outer = raw
				.iter()
				.zip(exterior)
				.filter(|(f, o)| **o && f.ids.contains(&a) && f.ids.contains(&b))
				.count();
			if outer != 0 && outer != 2 {
				return Err(invalid());
			}
			graph(&edges[..count], outer != 0)?;
		}
	}
	Ok(())
}
pub(super) fn validate(
	view: AffineMeshView<'_>,
	count: Counts,
	limits: PhysicalMeshLimits,
) -> Result<Vec<FacetPlan>, CfdError> {
	let d = count.d;
	let topology_work = add(
		1_000_000,
		add(
			mul(64, mul(count.i, count.i)?)?,
			mul(128, mul(count.i, mul(view.cells.len(), view.cells.len())?)?)?,
		)?,
	)?;
	if add(160 * 1024 * 1024, topology_work)? > limits.max_work {
		return Err(invalid());
	}
	for (i, v) in view.vertices.iter().enumerate() {
		if v.iter().any(|x| !x.is_finite())
			|| (d == 2 && v[2] != 0.)
			|| view.vertices[..i].contains(v)
			|| !view.cells.iter().any(|c| c.contains(&i))
		{
			return Err(invalid());
		}
	}
	for (ci, cell) in view.cells.iter().enumerate() {
		if cell.iter().any(|&v| v >= view.vertices.len())
			|| cell.iter().enumerate().any(|(i, v)| cell[..i].contains(v))
			|| view.cells[..ci]
				.iter()
				.any(|old| cell.iter().all(|v| old.contains(v)))
		{
			return Err(invalid());
		}
	}
	let mut raw = [EMPTY; 512];
	let mut nr = 0;
	for (ci, cell) in view.cells.iter().enumerate() {
		for omit in 0..=d {
			let mut f = EMPTY;
			f.cell = ci;
			let mut j = 0;
			for (local, &id) in cell.iter().enumerate() {
				if local != omit {
					f.nodes[j] = local;
					f.ids[j] = id;
					j += 1;
				}
			}
			raw[nr] = f;
			nr += 1;
		}
	}
	let raw = &raw[..nr];
	let mut mates = [None; 512];
	let mut exterior = [false; 512];
	for i in 0..nr {
		for j in 0..nr {
			if i != j && key_inc(raw[i], d) == key_inc(raw[j], d) {
				if mates[i].is_some() {
					return Err(invalid());
				}
				mates[i] = Some(j);
			}
		}
		exterior[i] = mates[i].is_none();
	}
	let mut visited = [false; 128];
	visited[0] = true;
	for _ in 0..view.cells.len() {
		for i in 0..nr {
			if visited[raw[i].cell]
				&& let Some(j) = mates[i]
			{
				visited[raw[j].cell] = true;
			}
		}
	}
	if visited[..view.cells.len()].iter().any(|&x| !x) {
		return Err(invalid());
	}
	links(view, raw, &exterior[..nr])?;
	let mut assigned = [false; 512];
	let mut plans = super::reserved(nr)?;
	for i in 0..nr {
		if let Some(j) = mates[i]
			&& i < j
		{
			let mut right = [0; 3];
			for k in 0..d {
				right[k] = raw[j].nodes[raw[j].ids[..d]
					.iter()
					.position(|x| *x == raw[i].ids[k])
					.ok_or_else(invalid)?];
			}
			plans.push(FacetPlan {
				left: raw[i].cell,
				right: Some(raw[j].cell),
				left_nodes: raw[i].nodes,
				right_nodes: right,
				label: None,
				periodic: false,
				natural: false,
			});
		}
	}
	let locate = |ids: &[usize]| -> Result<usize, CfdError> {
		if ids.iter().any(|&v| v >= view.vertices.len())
			|| ids.iter().enumerate().any(|(i, v)| ids[..i].contains(v))
		{
			return Err(invalid());
		}
		raw.iter()
			.enumerate()
			.find(|(i, f)| exterior[*i] && key_inc(**f, d) == key(ids))
			.map(|(i, _)| i)
			.ok_or_else(invalid)
	};
	for (label, f) in view.dirichlet.iter().enumerate() {
		let i = locate(f.vertices)?;
		if assigned[i] {
			return Err(invalid());
		}
		assigned[i] = true;
		plans.push(FacetPlan {
			left: raw[i].cell,
			right: None,
			left_nodes: raw[i].nodes,
			right_nodes: [0; 3],
			label: Some(label),
			periodic: false,
			natural: false,
		});
	}
	for f in view.periodic {
		let i = locate(f.left)?;
		let j = locate(f.right)?;
		if i == j || assigned[i] || assigned[j] {
			return Err(invalid());
		}
		assigned[i] = true;
		assigned[j] = true;
		let mut left = [0; 3];
		let mut right = [0; 3];
		for k in 0..d {
			left[k] = view.cells[raw[i].cell]
				.iter()
				.position(|&v| v == f.left[k])
				.ok_or_else(invalid)?;
			right[k] = view.cells[raw[j].cell]
				.iter()
				.position(|&v| v == f.right[k])
				.ok_or_else(invalid)?;
		}
		plans.push(FacetPlan {
			left: raw[i].cell,
			right: Some(raw[j].cell),
			left_nodes: left,
			right_nodes: right,
			label: None,
			periodic: true,
			natural: false,
		});
	}
	if exterior[..nr]
		.iter()
		.zip(assigned)
		.any(|(&outer, used)| outer && !used)
		|| plans.len() > limits.max_facets
		|| plans.capacity() > nr
	{
		return Err(invalid());
	}
	Ok(plans)
}
pub(super) fn materialize(
	view: AffineMeshView<'_>,
	count: Counts,
	plan: &[FacetPlan],
) -> Result<(Vec<Cell>, Vec<Face>), CfdError> {
	let mut cells = super::reserved(view.cells.len())?;
	for ids in view.cells {
		let mut points = super::reserved(count.d + 1)?;
		for &id in *ids {
			points.push(view.vertices[id]);
		}
		cells.push(crate::simplex::physical_cell(
			points,
			vec![[0; 3]; count.d + 1],
			count.d,
		)?);
	}
	let mut faces = super::reserved(plan.len())?;
	for f in plan {
		let nodes = f.left_nodes[..count.d].to_vec();
		let mut face = crate::simplex::make_face(&cells, &(f.left, nodes), None, count.d, 1)?;
		face.right = f.right;
		face.right_nodes = if f.right.is_some() {
			f.right_nodes[..count.d].to_vec()
		} else {
			Vec::new()
		};
		face.outflow = f.natural;
		face.prescribed = if f.right.is_none() && !f.natural {
			Some(vec![[0.; 3]; count.d])
		} else {
			None
		};
		face.lid = false;
		face.label = f
			.label
			.map_or_else(String::new, |i| view.dirichlet[i].label.to_owned());
		if !face.measure.is_finite()
			|| face.measure <= 0.
			|| face.normal.iter().any(|x| !x.is_finite())
		{
			return Err(invalid());
		}
		faces.push(face);
	}
	Ok((cells, faces))
}

#[cfg(test)]
#[allow(
	clippy::unwrap_used,
	clippy::panic_in_result_fn,
	reason = "Fixed independent abstract simplicial link counterexamples"
)]
mod tests {
	use super::*;
	fn rejected_links(d: usize, cells: &[&[usize]], vertices: &[[f64; 3]]) {
		let view = AffineMeshView {
			dimension: d,
			vertices,
			cells,
			dirichlet: &[],
			periodic: &[],
		};
		let mut raw = Vec::new();
		for (ci, cell) in cells.iter().enumerate() {
			for omitted in 0..=d {
				let mut f = EMPTY;
				f.cell = ci;
				let mut k = 0;
				for (local, &id) in cell.iter().enumerate() {
					if local != omitted {
						f.ids[k] = id;
						f.nodes[k] = local;
						k += 1;
					}
				}
				raw.push(f);
			}
		}
		let exterior = raw
			.iter()
			.map(|f| {
				raw.iter()
					.filter(|g| key_inc(**g, d) == key_inc(*f, d))
					.count()
					== 1
			})
			.collect::<Vec<_>>();
		assert!(raw.iter().all(|f| {
			raw.iter()
				.filter(|g| key_inc(**g, d) == key_inc(*f, d))
				.count()
				<= 2
		}));
		let mut reached = vec![false; cells.len()];
		reached[0] = true;
		for _ in 0..cells.len() {
			for a in &raw {
				for b in &raw {
					if reached[a.cell] && key_inc(*a, d) == key_inc(*b, d) {
						reached[b.cell] = true;
					}
				}
			}
		}
		assert!(reached.iter().all(|&v| v));
		assert!(links(view, &raw, &exterior).is_err());
	}
	#[test]
	fn connected_facet_complex_can_still_have_a_pinched_vertex_or_edge() {
		let mut vertices = Vec::new();
		for x in 0..4 {
			for y in 0..4 {
				vertices.push([f64::from(x), f64::from(y), 0.]);
			}
		}
		vertices.pop();
		let mut cells = Vec::new();
		for x in 0..3 {
			for y in 0..3 {
				let a = x * 4 + y;
				let b = (x + 1) * 4 + y;
				let c = (x + 1) * 4 + y + 1;
				let d = x * 4 + y + 1;
				cells.push([a, b, c].map(|i| if i == 15 { 0 } else { i }));
				cells.push([a, c, d].map(|i| if i == 15 { 0 } else { i }));
			}
		}
		let refs = cells.iter().map(<[usize; 3]>::as_slice).collect::<Vec<_>>();
		rejected_links(2, &refs, &vertices);
		let tetra: &[&[usize]] = &[
			&[0, 1, 2, 3],
			&[0, 1, 3, 4],
			&[0, 1, 4, 2],
			&[0, 1, 5, 6],
			&[0, 1, 6, 7],
			&[0, 1, 7, 5],
			&[0, 2, 3, 5],
			&[0, 3, 5, 6],
		];
		rejected_links(3, tetra, &[[0.; 3]; 8]);
	}
}
