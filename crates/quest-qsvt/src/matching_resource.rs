//! Resource-backed whole-unitary matching replay with constant cycle workspace.
use crate::matching::bit;
use crate::{
	Complex64, EncodingDescriptor, Error, MatchingColumn, MatchingHeader, MatchingShard,
	ReplayEncoding, ReplayGate, ReplayKind, Result,
};
use std::{
	ops::{Neg, Sub},
	sync::Arc,
};

/// Frozen column recipe, independent of serialization and MPI transport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResourceMatchingRecord {
	pub column: MatchingColumn,
	pub theta: f64,
	pub phase_angle: f64,
	pub is_edge: bool,
}
impl ResourceMatchingRecord {
	/// # Errors
	/// Rejects domain, unitarity, phase/rotation or completion-word inconsistencies.
	pub fn validate(self, header: MatchingHeader) -> Result<()> {
		let c = self.column;
		if c.color >= header.num_colors
			|| c.source >= header.system_dimension()?
			|| c.destination >= header.system_dimension()?
			|| !c.cosine.is_finite()
			|| !c.sine.is_finite()
			|| c.cosine < 0.0
			|| c.sine < 0.0
			|| c.cosine.mul_add(c.cosine, c.sine * c.sine).sub(1.0).abs() > 1e-12
			|| !c.phase.re.is_finite()
			|| !c.phase.im.is_finite()
			|| c.phase.norm_sqr().sub(1.0).abs() > 1e-12
			|| !self.theta.is_finite()
			|| !self.phase_angle.is_finite()
		{
			return Err(Error::Encoding("invalid resource matching record"));
		}
		if self.is_edge {
			if c.source >= header.cols
				|| c.destination >= header.rows
				|| !(0.0..=std::f64::consts::PI).contains(&self.theta)
				|| (0.5 * self.theta).cos().sub(c.cosine).abs() > 1e-12
				|| (0.5 * self.theta).sin().sub(c.sine).abs() > 1e-12
				|| self.phase_angle.cos().sub(c.phase.re).abs() > 1e-12
				|| self.phase_angle.sin().sub(c.phase.im).abs() > 1e-12
			{
				return Err(Error::Encoding(
					"resource frozen angles disagree with column",
				));
			}
		} else if c.cosine.to_bits() != 0.0_f64.to_bits()
			|| c.sine.to_bits() != 1.0_f64.to_bits()
			|| c.phase != Complex64::new(1.0, 0.0)
			|| self.theta.to_bits() != std::f64::consts::PI.to_bits()
			|| self.phase_angle.to_bits() != 0
		{
			return Err(Error::Encoding("resource noncanonical completion"));
		}
		Ok(())
	}
}
/// Immutable lookup resource. Ordered traversal enumerates touched records only.
///
/// The resource owns its data or an immutable verified snapshot. All methods must
/// preserve the same header, frozen identity and records for its entire lifetime.
/// Distributed implementations may borrow an exclusive transport context.
pub trait MatchingResource: std::fmt::Debug {
	fn header(&self) -> MatchingHeader;
	fn frozen_identity(&self) -> u64;
	/// # Errors
	/// Rejects retained allocation accounting overflow.
	fn retained_bytes(&self) -> Result<usize>;
	/// Conservative computational work per lookup, including local searches.
	fn query_work_bound(&self) -> usize;
	/// Conservative application payload wire bytes per lookup; excludes MPI internals.
	fn query_communication_bound(&self) -> usize;
	/// First touched record strictly beyond the bound in the requested order.
	/// # Errors
	/// Propagates resource/transport failures.
	fn next_record(
		&self,
		color: usize,
		bound: Option<usize>,
		reverse: bool,
	) -> Result<Option<ResourceMatchingRecord>>;
	/// # Errors
	/// Propagates resource/transport failures. Untouched columns return None.
	fn forward(&self, color: usize, source: usize) -> Result<Option<ResourceMatchingRecord>>;
	/// # Errors
	/// Propagates resource/transport failures. Untouched destinations return None.
	fn reverse(&self, color: usize, destination: usize) -> Result<Option<usize>>;
}
/// Resource limits for admission and one subsequent replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceReplayLimits {
	pub max_bytes: usize,
	pub max_queries: usize,
	pub max_work: usize,
	pub max_communication_bytes: usize,
	pub max_gates: usize,
}
impl Default for ResourceReplayLimits {
	fn default() -> Self {
		Self {
			max_bytes: 67_108_864,
			max_queries: 16_777_216,
			max_work: 268_435_456,
			max_communication_bytes: 1_073_741_824,
			max_gates: 16_777_216,
		}
	}
}
/// Modeled admission plus one replay; no measured RSS or timing claim is implied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceReplayStatistics {
	pub queries: usize,
	pub work: usize,
	pub communication_bytes: usize,
	/// Maximum primitives in either actual orientation; dry gate visits are work.
	pub gates: usize,
	pub retained_bytes: usize,
}
impl ResourceReplayStatistics {
	const fn check(self, limits: ResourceReplayLimits) -> Result<()> {
		if self.queries > limits.max_queries
			|| self.work > limits.max_work
			|| self.communication_bytes > limits.max_communication_bytes
			|| self.gates > limits.max_gates
			|| self.retained_bytes > limits.max_bytes
		{
			return Err(Error::Budget("resource matching replay"));
		}
		Ok(())
	}
}
/// Compact, admitted replay recipe; it retains no columns, cycles or gate list.
#[derive(Clone, Debug)]
pub struct AdmittedMatchingReplay {
	header: MatchingHeader,
	frozen_identity: u64,
	descriptor: EncodingDescriptor,
	statistics: ResourceReplayStatistics,
	limits: ResourceReplayLimits,
}
impl AdmittedMatchingReplay {
	/// Validate every touched record and directory closure, then count both streams.
	/// No user gate visitor runs until every admission succeeds.
	/// # Errors
	/// Rejects integrity, malformed directories and byte/query/work/wire/gate limits.
	pub fn admit<R: MatchingResource>(resource: &R, limits: ResourceReplayLimits) -> Result<Self> {
		let header = resource.header();
		header.validate()?;
		let minimum = header
			.num_colors
			.checked_add(
				header
					.color_qubits
					.checked_mul(2)
					.ok_or(Error::Budget("resource gate count"))?,
			)
			.ok_or(Error::Budget("resource gate count"))?;
		if minimum > limits.max_gates {
			return Err(Error::Budget("resource gate count"));
		}
		let retained_bytes = resource
			.retained_bytes()?
			.checked_add(size_of::<Self>())
			.and_then(|n| n.checked_add(4096))
			.ok_or(Error::Budget("resource replay workspace"))?;
		if retained_bytes > limits.max_bytes {
			return Err(Error::Budget("resource replay workspace"));
		}
		let mut cursor = Cursor::new(resource, limits);
		let mut count = 0usize;
		let mut digest = 0u64;
		for color in 0..header.num_colors {
			let mut bound = None;
			while let Some(record) = cursor.next(color, bound, false)? {
				// Charge the fixed SHA payload verification before performing it.
				cursor.statistics.work = cursor
					.statistics
					.work
					.checked_add(crate::record_fingerprint_work(7)?)
					.ok_or(Error::Budget("resource fingerprint work"))?;
				cursor.statistics.check(limits)?;
				record.validate(header)?;
				let own = MatchingShard::summarize_records(std::slice::from_ref(&record.column))?;
				count = count
					.checked_add(own.0)
					.ok_or(Error::Budget("resource record count"))?;
				if count > header.record_count {
					return Err(Error::Encoding("excess resource records"));
				}
				digest = digest.wrapping_add(own.1);
				bound = Some(record.column.source);
			}
		}
		if count != header.record_count || digest != header.record_digest {
			return Err(Error::Encoding("resource payload differs from manifest"));
		}
		let scan = cursor.statistics;
		let forward = stream(resource, header, false, limits, &mut |_| Ok(()))?;
		let reverse = stream(resource, header, true, limits, &mut |_| Ok(()))?;
		let statistics = ResourceReplayStatistics {
			queries: scan
				.queries
				.checked_add(forward.queries)
				.and_then(|n| n.checked_add(reverse.queries))
				.and_then(|n| n.checked_add(forward.queries.max(reverse.queries)))
				.ok_or(Error::Budget("resource query count"))?,
			work: scan
				.work
				.checked_add(forward.work)
				.and_then(|n| n.checked_add(reverse.work))
				.and_then(|n| n.checked_add(forward.work.max(reverse.work)))
				.ok_or(Error::Budget("resource work"))?,
			communication_bytes: scan
				.communication_bytes
				.checked_add(forward.communication_bytes)
				.and_then(|n| n.checked_add(reverse.communication_bytes))
				.and_then(|n| {
					n.checked_add(forward.communication_bytes.max(reverse.communication_bytes))
				})
				.ok_or(Error::Budget("resource communication"))?,
			gates: forward.gates.max(reverse.gates),
			retained_bytes,
		};
		statistics.check(limits)?;
		let mut descriptor = EncodingDescriptor::from_matching_header(header)?;
		descriptor.construction_identity = descriptor
			.construction_identity
			.wrapping_add(resource.frozen_identity().rotate_left(23))
			.wrapping_add(0x5245_534f_5552_4331);
		Ok(Self {
			header,
			frozen_identity: resource.frozen_identity(),
			descriptor,
			statistics,
			limits,
		})
	}
	#[must_use]
	pub const fn descriptor(&self) -> &EncodingDescriptor {
		&self.descriptor
	}
	#[must_use]
	pub const fn statistics(&self) -> ResourceReplayStatistics {
		self.statistics
	}
	/// Emit one gate at a time from an unchanged admitted resource.
	/// # Errors
	/// Propagates visitor/resource errors; rejects changed metadata before emission.
	pub fn visit_gates<R: MatchingResource>(
		&self,
		resource: &R,
		adjoint: bool,
		mut visitor: impl FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		if resource.header() != self.header || resource.frozen_identity() != self.frozen_identity {
			return Err(Error::Encoding("resource replay identity mismatch"));
		}
		stream(resource, self.header, adjoint, self.limits, &mut visitor)?;
		Ok(())
	}
	/// Map all source bits and signed outer controls without expanding storage.
	/// # Errors
	/// Rejects operand overlaps and propagates resource/visitor errors.
	pub fn visit_mapped_gates<R: MatchingResource>(
		&self,
		resource: &R,
		targets: &[usize],
		mask: usize,
		value: usize,
		adjoint: bool,
		mut visitor: impl FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		crate::owned_replay::validate_mapping(self.header.num_qubits()?, targets, mask, value)?;
		self.visit_gates(resource, adjoint, |gate| {
			visitor(gate.mapped(targets, mask, value)?)
		})
	}
}
/// Owns a resource and compact recipe. Clones share immutable resource storage.
#[derive(Debug)]
pub struct ResourceMatchingEncoding<R: MatchingResource> {
	resource: Arc<R>,
	recipe: AdmittedMatchingReplay,
}
impl<R: MatchingResource> Clone for ResourceMatchingEncoding<R> {
	fn clone(&self) -> Self {
		Self {
			resource: Arc::clone(&self.resource),
			recipe: self.recipe.clone(),
		}
	}
}
impl<R: MatchingResource> ResourceMatchingEncoding<R> {
	/// # Errors
	/// Rejects resource integrity and retained/work/query/wire/gate admission.
	pub fn new(resource: R, limits: ResourceReplayLimits) -> Result<Self> {
		let bytes = resource
			.retained_bytes()?
			.checked_add(size_of::<Self>())
			.and_then(|n| n.checked_add(2_usize.checked_mul(size_of::<usize>())?))
			.and_then(|n| n.checked_add(4096))
			.ok_or(Error::Budget("owning resource bytes"))?;
		if bytes > limits.max_bytes {
			return Err(Error::Budget("owning resource bytes"));
		}
		let mut recipe = AdmittedMatchingReplay::admit(&resource, limits)?;
		recipe.statistics.retained_bytes = bytes;
		Ok(Self {
			resource: Arc::new(resource),
			recipe,
		})
	}
	#[must_use]
	pub const fn recipe(&self) -> &AdmittedMatchingReplay {
		&self.recipe
	}
}
impl<R: MatchingResource> ReplayEncoding for ResourceMatchingEncoding<R> {
	fn descriptor(&self) -> Result<EncodingDescriptor> {
		Ok(self.recipe.descriptor.clone())
	}
	fn retained_bytes(&self) -> Result<usize> {
		Ok(self.recipe.statistics.retained_bytes)
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		self.recipe
			.visit_gates(self.resource.as_ref(), adjoint, visitor)
	}
}
struct Cursor<'a, R> {
	resource: &'a R,
	limits: ResourceReplayLimits,
	statistics: ResourceReplayStatistics,
}
impl<'a, R: MatchingResource> Cursor<'a, R> {
	fn new(resource: &'a R, limits: ResourceReplayLimits) -> Self {
		Self {
			resource,
			limits,
			statistics: ResourceReplayStatistics::default(),
		}
	}
	fn query(&mut self) -> Result<()> {
		self.statistics.queries = self
			.statistics
			.queries
			.checked_add(1)
			.ok_or(Error::Budget("resource query count"))?;
		self.statistics.work = self
			.statistics
			.work
			.checked_add(self.resource.query_work_bound().max(1))
			.ok_or(Error::Budget("resource work"))?;
		self.statistics.communication_bytes = self
			.statistics
			.communication_bytes
			.checked_add(self.resource.query_communication_bound())
			.ok_or(Error::Budget("resource communication"))?;
		self.statistics.check(self.limits)
	}
	fn next(
		&mut self,
		color: usize,
		bound: Option<usize>,
		reverse: bool,
	) -> Result<Option<ResourceMatchingRecord>> {
		self.query()?;
		let record = self.resource.next_record(color, bound, reverse)?;
		if record.is_some_and(|r| {
			r.column.color != color
				|| bound.is_some_and(|b| {
					if reverse {
						r.column.source >= b
					} else {
						r.column.source <= b
					}
				})
		}) {
			return Err(Error::Encoding("resource ordered traversal"));
		}
		Ok(record)
	}
	fn forward(&mut self, color: usize, source: usize) -> Result<usize> {
		self.query()?;
		let record = self
			.resource
			.forward(color, source)?
			.ok_or(Error::Encoding("resource cycle missing forward column"))?;
		if record.column.color != color || record.column.source != source {
			return Err(Error::Encoding("resource forward lookup key"));
		}
		Ok(record.column.destination)
	}
	fn reverse(&mut self, color: usize, destination: usize) -> Result<usize> {
		self.query()?;
		self.resource
			.reverse(color, destination)?
			.ok_or(Error::Encoding("resource cycle missing inverse"))
	}
	fn emit(
		&mut self,
		gate: ReplayGate,
		visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		self.statistics.gates = self
			.statistics
			.gates
			.checked_add(1)
			.ok_or(Error::Budget("resource gate count"))?;
		self.statistics.work = self
			.statistics
			.work
			.checked_add(1)
			.ok_or(Error::Budget("resource work"))?;
		self.statistics.check(self.limits)?;
		visitor(gate)
	}
}
fn stream<R: MatchingResource>(
	resource: &R,
	header: MatchingHeader,
	adjoint: bool,
	limits: ResourceReplayLimits,
	visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<ResourceReplayStatistics> {
	let mut cursor = Cursor::new(resource, limits);
	hadamards(&mut cursor, header, adjoint, visitor)?;
	for ordinal in 0..header.num_colors {
		let color = if adjoint {
			header
				.num_colors
				.checked_sub(ordinal)
				.and_then(|n| n.checked_sub(1))
				.ok_or(Error::Budget("resource color order"))?
		} else {
			ordinal
		};
		let start = header
			.system_qubits
			.checked_add(1)
			.ok_or(Error::Budget("resource color width"))?;
		let mask = bit(header.num_qubits()?)?
			.checked_sub(1)
			.ok_or(Error::Budget("resource color mask"))?
			& !bit(start)?
				.checked_sub(1)
				.ok_or(Error::Budget("resource color mask"))?;
		let value = color
			.checked_shl(u32::try_from(start).map_err(|_| Error::Budget("resource color width"))?)
			.ok_or(Error::Budget("resource color value"))?;
		let default = ReplayGate {
			kind: ReplayKind::Ry(if adjoint {
				-std::f64::consts::PI
			} else {
				std::f64::consts::PI
			}),
			target: Some(0),
			control_mask: mask,
			control_value: value,
		};
		if adjoint {
			permutations(&mut cursor, header, color, mask, value, true, visitor)?;
			corrections(&mut cursor, header, color, mask, value, true, visitor)?;
			cursor.emit(default, visitor)?;
		} else {
			cursor.emit(default, visitor)?;
			corrections(&mut cursor, header, color, mask, value, false, visitor)?;
			permutations(&mut cursor, header, color, mask, value, false, visitor)?;
		}
	}
	hadamards(&mut cursor, header, adjoint, visitor)?;
	Ok(cursor.statistics)
}
fn hadamards<R: MatchingResource>(
	cursor: &mut Cursor<'_, R>,
	header: MatchingHeader,
	adjoint: bool,
	visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<()> {
	for ordinal in 0..header.color_qubits {
		let local = if adjoint {
			header
				.color_qubits
				.checked_sub(ordinal)
				.and_then(|n| n.checked_sub(1))
				.ok_or(Error::Budget("resource H order"))?
		} else {
			ordinal
		};
		cursor.emit(
			ReplayGate {
				kind: ReplayKind::H,
				target: Some(
					header
						.system_qubits
						.checked_add(1)
						.and_then(|n| n.checked_add(local))
						.ok_or(Error::Budget("resource H bit"))?,
				),
				control_mask: 0,
				control_value: 0,
			},
			visitor,
		)?;
	}
	Ok(())
}
fn corrections<R: MatchingResource>(
	cursor: &mut Cursor<'_, R>,
	header: MatchingHeader,
	color: usize,
	mask: usize,
	value: usize,
	adjoint: bool,
	visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<()> {
	let mask = mask
		| header
			.system_dimension()?
			.checked_sub(1)
			.and_then(|n| n.checked_mul(2))
			.ok_or(Error::Budget("resource system mask"))?;
	let mut bound = None;
	while let Some(record) = cursor.next(color, bound, adjoint)? {
		bound = Some(record.column.source);
		if !record.is_edge {
			continue;
		}
		let value = value
			| record
				.column
				.source
				.checked_mul(2)
				.ok_or(Error::Budget("resource edge predicate"))?;
		let correction = ReplayGate {
			kind: ReplayKind::Ry(if adjoint {
				std::f64::consts::PI.sub(record.theta)
			} else {
				record.theta.sub(std::f64::consts::PI)
			}),
			target: Some(0),
			control_mask: mask,
			control_value: value,
		};
		let phase = ReplayGate {
			kind: ReplayKind::Phase(if adjoint {
				record.phase_angle.neg()
			} else {
				record.phase_angle
			}),
			target: None,
			control_mask: mask | 1,
			control_value: value,
		};
		if adjoint {
			cursor.emit(phase, visitor)?;
			cursor.emit(correction, visitor)?;
		} else {
			cursor.emit(correction, visitor)?;
			cursor.emit(phase, visitor)?;
		}
	}
	Ok(())
}
fn permutations<R: MatchingResource>(
	cursor: &mut Cursor<'_, R>,
	header: MatchingHeader,
	color: usize,
	mask: usize,
	value: usize,
	adjoint: bool,
	visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<()> {
	let mut bound = None;
	while let Some(record) = cursor.next(color, bound, adjoint)? {
		let pivot = record.column.source;
		bound = Some(pivot);
		let mut current = pivot;
		let mut minimum = pivot;
		let mut steps = 0usize;
		loop {
			let next = cursor.forward(color, current)?;
			if cursor.reverse(color, next)? != current {
				return Err(Error::Encoding("resource forward/reverse disagreement"));
			}
			minimum = minimum.min(next);
			steps = steps
				.checked_add(1)
				.ok_or(Error::Budget("resource cycle length"))?;
			if steps > header.record_count {
				return Err(Error::Encoding("resource permutation is not closed"));
			}
			current = next;
			if current == pivot {
				break;
			}
		}
		if minimum != pivot {
			continue;
		}
		current = if adjoint {
			cursor.reverse(color, pivot)?
		} else {
			cursor.forward(color, pivot)?
		};
		while current != pivot {
			transposition(cursor, header, pivot, current, mask, value, visitor)?;
			current = if adjoint {
				cursor.reverse(color, current)?
			} else {
				cursor.forward(color, current)?
			};
		}
	}
	Ok(())
}
fn transposition<R: MatchingResource>(
	cursor: &mut Cursor<'_, R>,
	header: MatchingHeader,
	a: usize,
	b: usize,
	color_mask: usize,
	color_value: usize,
	visitor: &mut dyn FnMut(ReplayGate) -> Result<()>,
) -> Result<()> {
	let difference = a ^ b;
	if difference == 0 {
		return Ok(());
	}
	let system_mask = header
		.system_dimension()?
		.checked_sub(1)
		.and_then(|n| n.checked_mul(2))
		.ok_or(Error::Budget("resource swap mask"))?;
	let mut current = a;
	let mut last = None;
	let gate = |local: usize, current: usize| -> Result<ReplayGate> {
		let target = local
			.checked_add(1)
			.ok_or(Error::Budget("resource swap target"))?;
		let target_mask = bit(target)?;
		Ok(ReplayGate {
			kind: ReplayKind::X,
			target: Some(target),
			control_mask: color_mask | (system_mask & !target_mask),
			control_value: color_value
				| (current
					.checked_mul(2)
					.ok_or(Error::Budget("resource swap value"))?
					& !target_mask),
		})
	};
	for local in 0..header.system_qubits {
		if difference & bit(local)? != 0 {
			cursor.emit(gate(local, current)?, visitor)?;
			current ^= bit(local)?;
			last = Some(local);
		}
	}
	let last = last.ok_or(Error::Encoding("resource swap difference"))?;
	current ^= bit(last)?;
	for local in (0..last).rev() {
		if difference & bit(local)? != 0 {
			cursor.emit(gate(local, current)?, visitor)?;
			current ^= bit(local)?;
		}
	}
	Ok(())
}
