//! Distributed path traversal closes each partial matching without a padded table.
use super::{
	ReplayEdge, ReverseRecord,
	color::{Edge, sort_work},
	transport::{Packet, Router, index, number},
};
use crate::{Error, Result};
use quest_qsvt::{Complex64, MatchingColumn};
use std::{
	cell::RefCell,
	ops::{Div, Mul},
};
struct Touch {
	color: usize,
	vertex: usize,
	next: Option<usize>,
	previous: Option<usize>,
}
struct Path {
	color: usize,
	initial: usize,
	cursor: usize,
	done: bool,
}
fn lookup(touched: &[Touch], color: usize, vertex: usize) -> Result<&Touch> {
	touched
		.binary_search_by_key(&(color, vertex), |t| (t.color, t.vertex))
		.ok()
		.and_then(|i| touched.get(i))
		.ok_or(Error::Value("missing completion endpoint"))
}
pub(super) struct Records {
	pub columns: Vec<MatchingColumn>,
	pub edges: Vec<ReplayEdge>,
	pub reverse: Vec<ReverseRecord>,
}
pub(super) fn complete(router: &mut Router<'_>, edges: &[Edge], beta: f64) -> Result<Records> {
	let (touched, paths) = directories(router, edges)?;
	let paths = walk(router, &touched, paths)?;
	router.drop_vector(touched)?;
	let mut result = forward_columns(router, edges, beta, &paths)?;
	router.drop_vector(paths)?;
	reverse_directory(router, &mut result)?;
	Ok(result)
}
#[allow(
	clippy::indexing_slicing,
	reason = "Decoded transport packets are fixed eight-word arrays"
)]
fn directories(router: &mut Router<'_>, edges: &[Edge]) -> Result<(Vec<Touch>, Vec<Path>)> {
	let parts = router.parts;
	let mut endpoints = Vec::new();
	router.exchange(
		edges
			.iter()
			.flat_map(|edge| [packet(edge, true, parts), packet(edge, false, parts)]),
		|router, p| {
			router.push(
				&mut endpoints,
				(index(p[0])?, index(p[1])?, p[2], index(p[3])?),
				router.limits.max_endpoint_records,
			)?;
			Ok([0; 8])
		},
		|_, _| Ok(()),
	)?;
	let sorting = router.work(sort_work(endpoints.len())?);
	router.agree(sorting)?;
	endpoints.sort_unstable();
	let mut touched: Vec<Touch> = Vec::new();
	let admitted = (|| {
		for &(color, vertex, kind, other) in &endpoints {
			if touched
				.last()
				.is_none_or(|t| (t.color, t.vertex) != (color, vertex))
			{
				router.push(
					&mut touched,
					Touch {
						color,
						vertex,
						next: None,
						previous: None,
					},
					router.limits.max_endpoint_records,
				)?;
			}
			let item = touched.last_mut().ok_or(Error::Overflow)?;
			let slot = if kind == 0 {
				&mut item.next
			} else {
				&mut item.previous
			};
			if slot.replace(other).is_some() {
				return Err(Error::Value("partial matching endpoint conflict"));
			}
		}
		Ok(())
	})();
	router.agree(admitted)?;
	router.drop_vector(endpoints)?;
	let mut paths = Vec::new();
	let admitted = (|| {
		for t in touched
			.iter()
			.filter(|t| t.previous.is_none() && t.next.is_some())
		{
			router.push(
				&mut paths,
				Path {
					color: t.color,
					initial: t.vertex,
					cursor: t.vertex,
					done: false,
				},
				router.limits.max_endpoint_records,
			)?;
		}
		Ok(())
	})();
	router.agree(admitted)?;
	Ok((touched, paths))
}
#[allow(
	clippy::indexing_slicing,
	reason = "Decoded transport packets are fixed eight-word arrays"
)]
fn walk(router: &mut Router<'_>, touched: &[Touch], paths: Vec<Path>) -> Result<Vec<Path>> {
	let parts = router.parts;
	let paths = RefCell::new(paths);
	while !router.all(paths.borrow().iter().all(|p| p.done))? {
		let admitted = (|| {
			router.statistics.completion_rounds = router
				.statistics
				.completion_rounds
				.checked_add(1)
				.ok_or(Error::Overflow)?;
			if router.statistics.completion_rounds > router.limits.max_completion_rounds
				|| router.statistics.completion_rounds
					> router
						.statistics
						.global_edges
						.checked_add(1)
						.ok_or(Error::Overflow)?
			{
				return Err(Error::Value("sparse producer completion rounds"));
			}
			router.work(paths.borrow().len())
		})();
		router.agree(admitted)?;
		let count = paths.borrow().len();
		router.exchange(
			(0..count).filter_map(|i| {
				let paths = paths.borrow();
				let p = paths.get(i)?;
				if p.done {
					None
				} else {
					Some((|| {
						Ok(Packet {
							destination: p.cursor.checked_rem(parts).ok_or(Error::Overflow)?,
							words: [
								number(i)?,
								number(p.color)?,
								number(p.cursor)?,
								0,
								0,
								0,
								0,
								0,
							],
						})
					})())
				}
			}),
			|router, p| {
				router.work(1)?;
				let next = lookup(touched, index(p[1])?, index(p[2])?)?.next;
				Ok([
					p[0],
					u64::from(next.is_some()),
					number(next.unwrap_or(0))?,
					0,
					0,
					0,
					0,
					0,
				])
			},
			|_, p| {
				let mut paths = paths.borrow_mut();
				let path = paths.get_mut(index(p[0])?).ok_or(Error::Overflow)?;
				if p[1] == 0 {
					path.done = true;
				} else {
					path.cursor = index(p[2])?;
				}
				Ok(())
			},
		)?;
	}
	Ok(paths.into_inner())
}
fn forward_columns(
	router: &mut Router<'_>,
	edges: &[Edge],
	beta: f64,
	paths: &[Path],
) -> Result<Records> {
	let parts = router.parts;
	let mut result = Records {
		columns: Vec::new(),
		edges: Vec::new(),
		reverse: Vec::new(),
	};
	router.exchange(
		edges.iter().map(|edge| {
			let value = edge.entry.value;
			let theta = 2.0_f64.mul(value.norm().div(beta).clamp(0.0, 1.0).acos());
			let phase = value.arg();
			Ok(Packet {
				destination: edge
					.entry
					.column
					.checked_rem(parts)
					.ok_or(Error::Overflow)?,
				words: [
					number(edge.color.ok_or(Error::Value("uncolored edge"))?)?,
					number(edge.entry.column)?,
					number(edge.entry.row)?,
					value.re.to_bits(),
					value.im.to_bits(),
					theta.to_bits(),
					phase.to_bits(),
					1,
				],
			})
		}),
		|router, p| {
			add_column(router, &mut result, p)?;
			Ok([0; 8])
		},
		|_, _| Ok(()),
	)?;
	router.exchange(
		paths.iter().map(|path| {
			Ok(Packet {
				destination: path.cursor.checked_rem(parts).ok_or(Error::Overflow)?,
				words: [
					number(path.color)?,
					number(path.cursor)?,
					number(path.initial)?,
					0,
					0,
					std::f64::consts::PI.to_bits(),
					0,
					0,
				],
			})
		}),
		|router, p| {
			add_column(router, &mut result, p)?;
			Ok([0; 8])
		},
		|_, _| Ok(()),
	)?;
	let sorting = (|| {
		router.work(sort_work(result.columns.len())?)?;
		router.work(sort_work(result.edges.len())?)
	})();
	router.agree(sorting)?;
	result.columns.sort_unstable_by_key(|c| (c.color, c.source));
	result.edges.sort_unstable_by_key(|e| (e.color, e.column));
	let valid = if result
		.columns
		.windows(2)
		.any(|pair| matches!(pair,[a,b] if (a.color,a.source)==(b.color,b.source)))
	{
		Err(Error::Value("completed source conflict"))
	} else {
		Ok(())
	};
	router.agree(valid)?;
	Ok(result)
}
#[allow(
	clippy::indexing_slicing,
	reason = "Decoded transport packets are fixed eight-word arrays"
)]
fn reverse_directory(router: &mut Router<'_>, result: &mut Records) -> Result<()> {
	let parts = router.parts;
	router.exchange(
		result.columns.iter().map(|c| {
			Ok(Packet {
				destination: c.destination.checked_rem(parts).ok_or(Error::Overflow)?,
				words: [
					number(c.color)?,
					number(c.destination)?,
					number(c.source)?,
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
				&mut result.reverse,
				ReverseRecord {
					color: index(p[0])?,
					destination: index(p[1])?,
					source: index(p[2])?,
				},
				router.limits.max_endpoint_records,
			)?;
			Ok([0; 8])
		},
		|_, _| Ok(()),
	)?;
	let sorting = router.work(sort_work(result.reverse.len())?);
	router.agree(sorting)?;
	result
		.reverse
		.sort_unstable_by_key(|r| (r.color, r.destination));
	let valid = if result
		.reverse
		.windows(2)
		.any(|pair| matches!(pair,[a,b] if (a.color,a.destination)==(b.color,b.destination)))
	{
		Err(Error::Value("completed destination conflict"))
	} else {
		Ok(())
	};
	router.agree(valid)?;
	Ok(())
}
fn packet(edge: &Edge, outgoing: bool, parts: usize) -> Result<Packet> {
	let (vertex, other, kind) = if outgoing {
		(edge.entry.column, edge.entry.row, 0)
	} else {
		(edge.entry.row, edge.entry.column, 1)
	};
	Ok(Packet {
		destination: vertex.checked_rem(parts).ok_or(Error::Overflow)?,
		words: [
			number(edge.color.ok_or(Error::Value("uncolored edge"))?)?,
			number(vertex)?,
			kind,
			number(other)?,
			0,
			0,
			0,
			0,
		],
	})
}
#[allow(
	clippy::indexing_slicing,
	reason = "Decoded transport packets are fixed eight-word arrays"
)]
fn add_column(router: &mut Router<'_>, result: &mut Records, p: [u64; 8]) -> Result<()> {
	let (color, source, destination) = (index(p[0])?, index(p[1])?, index(p[2])?);
	let theta = f64::from_bits(p[5]);
	let phase = f64::from_bits(p[6]);
	let (sine, cosine) = if p[7] == 0 {
		(1.0, 0.0)
	} else {
		theta.mul(0.5).sin_cos()
	};
	router.push(
		&mut result.columns,
		MatchingColumn {
			color,
			source,
			destination,
			cosine,
			sine,
			phase: if p[7] == 0 {
				Complex64::new(1.0, 0.0)
			} else {
				Complex64::from_polar(1.0, phase)
			},
		},
		router.limits.max_endpoint_records,
	)?;
	if p[7] == 1 {
		router.push(
			&mut result.edges,
			ReplayEdge {
				color,
				row: destination,
				column: source,
				value: Complex64::new(f64::from_bits(p[3]), f64::from_bits(p[4])),
				theta,
				phase,
			},
			router.limits.max_local_edges,
		)?;
	}
	Ok(())
}
