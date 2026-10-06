//! Snapshot proposal rounds. Endpoint owners admit only dual endpoint winners.
use super::transport::{Packet, Router, index, number};
use crate::{Error, Result};
use quest_numerics::sparse_stream::SparseEntry;
use std::cell::RefCell;

pub(super) struct Edge {
	pub entry: SparseEntry,
	pub color: Option<usize>,
	proposal: usize,
	ready: bool,
	row_winner: bool,
}
impl Edge {
	pub const fn new(entry: SparseEntry) -> Self {
		Self {
			entry,
			color: None,
			proposal: 0,
			ready: false,
			row_winner: false,
		}
	}
}
struct Vertex {
	index: usize,
	degree: usize,
	used: Vec<usize>,
}
fn vertex(vertices: &[Vertex], value: usize) -> Result<&Vertex> {
	vertices
		.binary_search_by_key(&value, |v| v.index)
		.ok()
		.and_then(|i| vertices.get(i))
		.ok_or(Error::Value("missing owned endpoint"))
}
fn vertex_mut(vertices: &mut [Vertex], value: usize) -> Result<&mut Vertex> {
	let i = vertices
		.binary_search_by_key(&value, |v| v.index)
		.map_err(|_| Error::Value("missing owned endpoint"))?;
	vertices.get_mut(i).ok_or(Error::Overflow)
}
fn used(vertices: &[Vertex], value: usize, color: usize) -> Result<bool> {
	Ok(vertex(vertices, value)?.used.binary_search(&color).is_ok())
}
fn commit(
	router: &mut Router<'_>,
	vertices: &mut [Vertex],
	value: usize,
	color: usize,
) -> Result<()> {
	let vertex = vertex_mut(vertices, value)?;
	let position = vertex
		.used
		.binary_search(&color)
		.err()
		.ok_or(Error::Value("endpoint color conflict"))?;
	router.work(vertex.used.len())?;
	router.push(&mut vertex.used, color, router.limits.max_vertex_degree)?;
	let last = vertex.used.len().checked_sub(1).ok_or(Error::Overflow)?;
	vertex
		.used
		.get_mut(position..=last)
		.ok_or(Error::Overflow)?
		.rotate_right(1);
	Ok(())
}
fn vertices(
	router: &mut Router<'_>,
	indices: impl IntoIterator<Item = usize>,
) -> Result<Vec<Vertex>> {
	let mut result: Vec<Vertex> = Vec::new();
	for index in indices {
		if let Some(previous) = result.last_mut().filter(|v| v.index == index) {
			previous.degree = previous.degree.checked_add(1).ok_or(Error::Overflow)?;
			if previous.degree > router.limits.max_vertex_degree {
				return Err(Error::Value("sparse producer vertex degree"));
			}
		} else {
			if router.limits.max_vertex_degree == 0 {
				return Err(Error::Value("sparse producer vertex degree"));
			}
			router.push(
				&mut result,
				Vertex {
					index,
					degree: 1,
					used: Vec::new(),
				},
				router.limits.max_endpoint_records,
			)?;
		}
	}
	Ok(result)
}
type EdgeSet<'a> = RefCell<&'a mut Vec<Edge>>;
type Proposal = (usize, usize, usize, usize);
#[derive(Clone, Copy)]
enum Selection {
	Unready,
	Uncolored,
	Committed,
}
fn packets<'a>(
	edges: &'a EdgeSet<'_>,
	parts: usize,
	selection: Selection,
) -> impl Iterator<Item = Result<Packet>> + 'a {
	(0..edges.borrow().len()).filter_map(move |i| {
		let edges = edges.borrow();
		let edge = edges.get(i)?;
		let selected = match selection {
			Selection::Unready => edge.color.is_none() && !edge.ready,
			Selection::Uncolored => edge.color.is_none(),
			Selection::Committed => edge.ready && edge.color == Some(edge.proposal),
		};
		selected.then(|| {
			Ok(Packet {
				destination: edge
					.entry
					.column
					.checked_rem(parts)
					.ok_or(Error::Overflow)?,
				words: [
					number(edge.entry.column)?,
					number(edge.proposal)?,
					number(edge.entry.row)?,
					number(i)?,
					0,
					0,
					0,
					0,
				],
			})
		})
	})
}
#[allow(
	clippy::indexing_slicing,
	reason = "Decoded transport packets are fixed eight-word arrays"
)]
fn endpoints(router: &mut Router<'_>, edges: &[Edge]) -> Result<(Vec<Vertex>, Vec<Vertex>, usize)> {
	let row_result = vertices(router, edges.iter().map(|e| e.entry.row));
	let rows = router.agree(row_result)?;
	let parts = router.parts;
	let mut raw = Vec::new();
	router.exchange(
		edges.iter().map(|e| {
			Ok(Packet {
				destination: e.entry.column.checked_rem(parts).ok_or(Error::Overflow)?,
				words: [
					number(e.entry.column)?,
					number(e.entry.row)?,
					0,
					0,
					0,
					0,
					0,
					0,
				],
			})
		}),
		|router, p| {
			router.push(
				&mut raw,
				(index(p[0])?, index(p[1])?),
				router.limits.max_endpoint_records,
			)?;
			Ok([0; 8])
		},
		|_, _| Ok(()),
	)?;
	let sorting = router.work(sort_work(raw.len())?);
	router.agree(sorting)?;
	raw.sort_unstable();
	let col_result = vertices(router, raw.iter().map(|&(col, _)| col));
	let columns = router.agree(col_result)?;
	router.drop_vector(raw)?;
	let dr = router.maximum(rows.iter().map(|v| v.degree).max().unwrap_or(0))?;
	let dc = router.maximum(columns.iter().map(|v| v.degree).max().unwrap_or(0))?;
	router.statistics.max_row_degree = dr;
	router.statistics.max_column_degree = dc;
	let palette = if dr == 0 && dc == 0 {
		0
	} else {
		dr.checked_add(dc)
			.and_then(|n| n.checked_sub(1))
			.ok_or(Error::Overflow)?
	};
	Ok((rows, columns, palette))
}
fn begin_round(router: &mut Router<'_>, edges: &EdgeSet<'_>) -> Result<()> {
	let admitted = (|| {
		router.statistics.coloring_rounds = router
			.statistics
			.coloring_rounds
			.checked_add(1)
			.ok_or(Error::Overflow)?;
		if router.statistics.coloring_rounds > router.limits.max_rounds {
			return Err(Error::Value("sparse producer coloring rounds"));
		}
		router.work(edges.borrow().len())?;
		for edge in edges.borrow_mut().iter_mut().filter(|e| e.color.is_none()) {
			edge.proposal = 0;
			edge.ready = false;
			edge.row_winner = false;
		}
		Ok(())
	})();
	router.agree(admitted)
}
#[allow(
	clippy::indexing_slicing,
	reason = "Decoded transport packets are fixed eight-word arrays"
)]
fn propose(
	router: &mut Router<'_>,
	edges: &EdgeSet<'_>,
	rows: &[Vertex],
	columns: &[Vertex],
	palette: usize,
) -> Result<()> {
	loop {
		let prepared = (|| {
			router.work(edges.borrow().len())?;
			for edge in edges
				.borrow_mut()
				.iter_mut()
				.filter(|e| e.color.is_none() && !e.ready)
			{
				while used(rows, edge.entry.row, edge.proposal)? {
					edge.proposal = edge.proposal.checked_add(1).ok_or(Error::Overflow)?;
					router.work(1)?;
				}
				if edge.proposal >= palette {
					return Err(Error::Value("sparse producer palette exhausted"));
				}
			}
			Ok(())
		})();
		router.agree(prepared)?;
		router.exchange(
			packets(edges, router.parts, Selection::Unready),
			|router, p| {
				router.work(
					usize::try_from(router.limits.max_vertex_degree.checked_ilog2().unwrap_or(0))
						.map_err(|_| Error::Overflow)?
						.saturating_add(2),
				)?;
				Ok([
					p[3],
					u64::from(used(columns, index(p[0])?, index(p[1])?)?),
					0,
					0,
					0,
					0,
					0,
					0,
				])
			},
			|router, p| {
				router.statistics.probes = router
					.statistics
					.probes
					.checked_add(1)
					.ok_or(Error::Overflow)?;
				if router.statistics.probes > router.limits.max_probes {
					return Err(Error::Value("sparse producer color probes"));
				}
				let mut edges = edges.borrow_mut();
				let edge = edges.get_mut(index(p[0])?).ok_or(Error::Overflow)?;
				if p[1] == 0 {
					edge.ready = true;
				} else {
					edge.proposal = edge.proposal.checked_add(1).ok_or(Error::Overflow)?;
				}
				Ok(())
			},
		)?;
		if router.all(edges.borrow().iter().all(|e| e.color.is_some() || e.ready))? {
			return Ok(());
		}
	}
}
fn row_winners(
	router: &mut Router<'_>,
	edges: &EdgeSet<'_>,
	order: &mut Vec<Proposal>,
) -> Result<()> {
	order.clear();
	let ordered = (|| {
		for (i, edge) in edges
			.borrow()
			.iter()
			.enumerate()
			.filter(|(_, e)| e.color.is_none())
		{
			router.push(
				order,
				(edge.entry.row, edge.proposal, edge.entry.column, i),
				router.limits.max_local_edges,
			)?;
		}
		router.work(sort_work(order.len())?)?;
		order.sort_unstable();
		let mut previous = None;
		for &(row, color, _, i) in order.iter() {
			if previous != Some((row, color)) {
				edges
					.borrow_mut()
					.get_mut(i)
					.ok_or(Error::Overflow)?
					.row_winner = true;
			}
			previous = Some((row, color));
		}
		Ok(())
	})();
	router.agree(ordered)
}
#[allow(
	clippy::indexing_slicing,
	reason = "Decoded transport packets are fixed eight-word arrays"
)]
fn column_winners(
	router: &mut Router<'_>,
	edges: &EdgeSet<'_>,
	winners: &mut Vec<Proposal>,
) -> Result<()> {
	winners.clear();
	router.exchange(
		packets(edges, router.parts, Selection::Uncolored),
		|router, p| {
			router.push(
				winners,
				(index(p[0])?, index(p[1])?, index(p[2])?, index(p[3])?),
				router.limits.max_endpoint_records,
			)?;
			Ok([0; 8])
		},
		|_, _| Ok(()),
	)?;
	let sorting = router.work(sort_work(winners.len())?);
	router.agree(sorting)?;
	winners.sort_unstable();
	router.exchange(
		packets(edges, router.parts, Selection::Uncolored),
		|_, p| {
			let key = (index(p[0])?, index(p[1])?);
			let position = winners.partition_point(|&(col, color, _, _)| (col, color) < key);
			let winner = winners.get(position).is_some_and(|&(col, color, row, _)| {
				(col, color) == key && number(row).is_ok_and(|row| row == p[2])
			});
			Ok([p[3], u64::from(winner), 0, 0, 0, 0, 0, 0])
		},
		|_, p| {
			let mut edges = edges.borrow_mut();
			let edge = edges.get_mut(index(p[0])?).ok_or(Error::Overflow)?;
			if p[1] == 1 && edge.row_winner {
				edge.color = Some(edge.proposal);
			}
			Ok(())
		},
	)
}
#[allow(
	clippy::indexing_slicing,
	reason = "Decoded transport packets are fixed eight-word arrays"
)]
fn commit_round(
	router: &mut Router<'_>,
	edges: &EdgeSet<'_>,
	rows: &mut [Vertex],
	columns: &mut [Vertex],
) -> Result<()> {
	let committed = (|| {
		for edge in edges
			.borrow()
			.iter()
			.filter(|e| e.ready && e.color == Some(e.proposal))
		{
			commit(router, rows, edge.entry.row, edge.proposal)?;
		}
		Ok(())
	})();
	router.agree(committed)?;
	router.exchange(
		packets(edges, router.parts, Selection::Committed),
		|router, p| {
			commit(router, columns, index(p[0])?, index(p[1])?)?;
			Ok([0; 8])
		},
		|_, _| Ok(()),
	)?;
	for edge in edges.borrow_mut().iter_mut().filter(|e| e.color.is_some()) {
		edge.ready = false;
	}
	Ok(())
}
pub(super) fn color(router: &mut Router<'_>, edges: &mut Vec<Edge>) -> Result<usize> {
	let (mut rows, mut columns, palette) = endpoints(router, edges)?;
	let mut remaining = router.sum(edges.len())?;
	let edges = RefCell::new(edges);
	let mut winners = Vec::new();
	let mut row_order = Vec::new();
	while remaining > 0 {
		begin_round(router, &edges)?;
		propose(router, &edges, &rows, &columns, palette)?;
		row_winners(router, &edges, &mut row_order)?;
		column_winners(router, &edges, &mut winners)?;
		commit_round(router, &edges, &mut rows, &mut columns)?;
		let next = router.sum(edges.borrow().iter().filter(|e| e.color.is_none()).count())?;
		if next >= remaining {
			return Err(Error::Value("sparse producer coloring made no progress"));
		}
		remaining = next;
	}
	let used = router.maximum(
		edges
			.borrow()
			.iter()
			.filter_map(|e| e.color)
			.max()
			.map_or(0, |c| c.saturating_add(1)),
	)?;
	router.statistics.used_colors = used;
	// Endpoint color lists are independently allocated and therefore released
	// before their owning vertex buffers. Proposal storage ends with coloring.
	for vertex in rows.iter_mut().chain(columns.iter_mut()) {
		router.drop_vector(std::mem::take(&mut vertex.used))?;
	}
	router.drop_vector(rows)?;
	router.drop_vector(columns)?;
	router.drop_vector(winners)?;
	router.drop_vector(row_order)?;
	Ok(used)
}
pub(super) fn sort_work(count: usize) -> Result<usize> {
	// All producer sorts are unstable, in-place sorts: no additional heap sort
	// buffer overlaps the vectors whose capacities the router already owns.
	count
		.checked_mul(
			usize::try_from(count.checked_ilog2().unwrap_or(0))
				.map_err(|_| Error::Overflow)?
				.checked_add(2)
				.ok_or(Error::Overflow)?,
		)
		.ok_or(Error::Overflow)
}
