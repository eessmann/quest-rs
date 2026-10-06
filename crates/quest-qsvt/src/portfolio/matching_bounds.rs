//! Stored-matching per-color rescaling through immutable resource columns.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Explicit per-color records and simultaneous retained storage are bounded"
)]
use super::{PortfolioLimits, PortfolioResources, WeightedLcu, add, admit, bits, count, hash, mul};
use crate::{
	Complex64, EncodingDescriptor, Error, MatchingColumn, MatchingEncoding, MatchingHeader,
	MatchingShard, ReplayEncoding, ReplayGate, Result,
	matching_resource::{
		MatchingResource, ResourceMatchingEncoding, ResourceMatchingRecord, ResourceReplayLimits,
	},
};
use std::sync::Arc;
#[derive(Debug)]
struct ColorResource {
	header: MatchingHeader,
	records: Vec<ResourceMatchingRecord>,
	identity: u64,
}
impl MatchingResource for ColorResource {
	fn header(&self) -> MatchingHeader {
		self.header
	}
	fn frozen_identity(&self) -> u64 {
		self.identity
	}
	fn retained_bytes(&self) -> Result<usize> {
		add(
			size_of::<Self>(),
			mul(self.records.capacity(), size_of::<ResourceMatchingRecord>())?,
		)
	}
	fn query_work_bound(&self) -> usize {
		self.records.len().saturating_add(4)
	}
	fn query_communication_bound(&self) -> usize {
		0
	}
	fn next_record(
		&self,
		color: usize,
		bound: Option<usize>,
		reverse: bool,
	) -> Result<Option<ResourceMatchingRecord>> {
		if color != 0 {
			return Ok(None);
		}
		let i = if reverse {
			self.records
				.partition_point(|r| bound.is_none_or(|b| r.column.source < b))
				.checked_sub(1)
		} else {
			Some(
				self.records
					.partition_point(|r| bound.is_some_and(|b| r.column.source <= b)),
			)
		};
		Ok(i.and_then(|i| self.records.get(i)).copied())
	}
	fn forward(&self, color: usize, source: usize) -> Result<Option<ResourceMatchingRecord>> {
		if color != 0 {
			return Ok(None);
		}
		Ok(self
			.records
			.binary_search_by_key(&source, |r| r.column.source)
			.ok()
			.and_then(|i| self.records.get(i))
			.copied())
	}
	fn reverse(&self, color: usize, destination: usize) -> Result<Option<usize>> {
		if color != 0 {
			return Ok(None);
		}
		Ok(self
			.records
			.iter()
			.find(|r| r.column.destination == destination)
			.map(|r| r.column.source))
	}
}
#[derive(Clone, Debug)]
enum Source {
	Weighted(WeightedLcu<ResourceMatchingEncoding<ColorResource>>),
	Zero(MatchingEncoding),
}
#[derive(Debug)]
struct Data {
	source: Source,
	descriptor: EncodingDescriptor,
	resources: PortfolioResources,
	bounds: Vec<f64>,
}
/// Stored-matching LCU with alpha equal to the sum of each color's maximum magnitude.
///
/// This bounded local baseline derives independent owning records; it does not
/// change producer/persisted resource normalizations or require those interfaces.
#[derive(Clone, Debug)]
pub struct PerMatchingBounds(Arc<Data>);
impl PerMatchingBounds {
	/// # Errors
	/// Rejects retained parent/child records, preparation, query and work budgets.
	#[allow(
		clippy::too_many_lines,
		reason = "Ordered per-color rescaling bounds parent, independent records and child query construction together"
	)]
	pub fn new(base: &MatchingEncoding, l: PortfolioLimits) -> Result<Self> {
		let mut total = 0;
		for m in base.matchings() {
			total = add(total, m.permutation().len())?;
		}
		let peak = add(
			base.retained_bytes()?,
			add(mul(total, size_of::<ResourceMatchingRecord>() * 2)?, 4096)?,
		)?;
		if peak > l.max_bytes || total > l.max_table_entries {
			return Err(Error::Budget("per-matching construction records"));
		}
		let mut terms = crate::matching::reserve(base.matchings().len())?;
		let mut bounds = crate::matching::reserve(base.matchings().len())?;
		let mut query_work = 0;
		let mut query_count = 0;
		let mut replay_queries = 0;
		let mut bytes = base.retained_bytes()?;
		for (color, m) in base.matchings().iter().enumerate() {
			let beta = m.edges().iter().fold(0.0_f64, |b, e| b.max(e.magnitude));
			if beta == 0.0 {
				continue;
			}
			let mut records = crate::matching::reserve(m.permutation().len())?;
			for &(source, destination) in m.permutation() {
				let edge = m.entry(source);
				let ratio = edge.map_or(0.0, |e| e.magnitude / beta).clamp(0.0, 1.0);
				records.push(ResourceMatchingRecord {
					column: MatchingColumn {
						color: 0,
						source,
						destination,
						cosine: ratio,
						sine: ratio.mul_add(-ratio, 1.0).max(0.0).sqrt(),
						phase: edge.map_or(Complex64::new(1.0, 0.0), |e| {
							Complex64::from_polar(1.0, e.phase)
						}),
					},
					theta: 2.0 * ratio.acos(),
					phase_angle: edge.map_or(0.0, |e| e.phase),
					is_edge: edge.is_some(),
				});
			}
			let columns: Vec<_> = records.iter().map(|r| r.column).collect();
			let (record_count, record_digest) = MatchingShard::summarize_records(&columns)?;
			drop(columns);
			let header = MatchingHeader {
				rows: base.rows(),
				cols: base.cols(),
				system_qubits: base.system_qubits(),
				color_qubits: 0,
				num_colors: 1,
				beta,
				alpha: beta,
				source_identity: hash([
					base.source_identity(),
					u64::try_from(color).map_err(|_| Error::Budget("matching color identity"))?,
				]),
				record_count,
				record_digest,
			};
			let resource = ColorResource {
				header,
				records,
				identity: hash([record_digest, beta.to_bits()]),
			};
			let source = ResourceMatchingEncoding::new(
				resource,
				ResourceReplayLimits {
					max_bytes: l.max_bytes.saturating_sub(bytes),
					max_work: l.max_compile_work.saturating_sub(query_work),
					max_queries: l.max_compile_work,
					max_gates: l.max_gates,
					max_communication_bytes: 0,
				},
			)?;
			// Two record traversals, cycle validation for each vertex, and one pivot walk.
			let mut actual_queries = add(mul(m.permutation().len(), 5)?, 2)?;
			for cycle in m.cycles() {
				actual_queries = add(
					actual_queries,
					mul(mul(cycle.len(), cycle.len().saturating_sub(1))?, 2)?,
				)?;
			}
			let extra_work = add(
				mul(actual_queries, add(m.permutation().len(), 4)?)?,
				source.recipe().statistics().gates,
			)?;
			query_work = add(
				query_work,
				add(source.recipe().statistics().work, extra_work)?,
			)?;
			replay_queries = add(replay_queries, actual_queries)?;
			if query_work > l.max_compile_work {
				return Err(Error::Budget("per-matching directory construction work"));
			}
			query_count = add(
				query_count,
				add(source.recipe().statistics().queries, actual_queries)?,
			)?;
			bytes = add(bytes, source.retained_bytes()?)?;
			terms.push((Complex64::new(1.0, 0.0), source));
			bounds.push(beta);
		}
		let (source, mut descriptor, mut resources) = if terms.is_empty() {
			let resources = PortfolioResources {
				elementary_gates: count(base, l)?,
				retained_bytes: base.retained_bytes()?,
				construction_peak_bytes: peak,
				..PortfolioResources::default()
			};
			(Source::Zero(base.clone()), base.descriptor()?, resources)
		} else {
			let e = WeightedLcu::new(
				terms,
				PortfolioLimits {
					max_bytes: l.max_bytes.saturating_sub(base.retained_bytes()?),
					max_compile_work: l.max_compile_work.saturating_sub(query_work),
					..l
				},
			)?;
			let d = e.descriptor()?;
			let r = e.resources();
			(Source::Weighted(e), d, r)
		};
		descriptor.source_identity = base.source_identity();
		descriptor.construction_identity =
			hash([descriptor.construction_identity, 0x5045_5243_4f4c_4f52]);
		descriptor.validate()?;
		resources.directory_queries = replay_queries;
		resources.admission_queries = query_count;
		resources.compile_work = add(resources.compile_work, query_work)?;
		resources.table_entries = add(resources.table_entries, total)?;
		resources.retained_bytes = add(
			resources.retained_bytes,
			add(
				size_of::<Data>() + 64,
				mul(bounds.capacity(), size_of::<f64>())?,
			)?,
		)?;
		resources.construction_peak_bytes =
			add(resources.construction_peak_bytes, base.retained_bytes()?)?
				.max(add(resources.retained_bytes, 4096)?);
		admit(resources, l)?;
		let _ = bits(descriptor.layout.num_qubits)?;
		Ok(Self(Arc::new(Data {
			source,
			descriptor,
			resources,
			bounds,
		})))
	}
	#[must_use]
	pub fn bounds(&self) -> &[f64] {
		&self.0.bounds
	}
	#[must_use]
	pub fn resources(&self) -> PortfolioResources {
		self.0.resources
	}
}
impl ReplayEncoding for PerMatchingBounds {
	fn descriptor(&self) -> Result<EncodingDescriptor> {
		Ok(self.0.descriptor.clone())
	}
	fn retained_bytes(&self) -> Result<usize> {
		Ok(self.0.resources.retained_bytes)
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		match &self.0.source {
			Source::Weighted(e) => e.visit_replay(adjoint, v),
			Source::Zero(e) => e.visit_replay(adjoint, v),
		}
	}
}
