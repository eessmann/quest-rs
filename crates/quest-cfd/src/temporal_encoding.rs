//! Explicit coherent temporal interpolation with constant retained recipe storage.
//!
//! The rectangular map sums DG amplitudes before probabilities. Arithmetic
//! matching directories replace a stored matrix, but their classical scans and
//! controlled primitive streams are still admitted and charged.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Checked dimensions bound fixed three-node arithmetic matching recipes"
)]
use crate::{
	CfdError, probability_observation::TemporalNodeSide,
	temporal_observation::TemporalInterpolation,
};
use quest_qsvt::{
	Complex64, EncodingDescriptor, MatchingColumn, MatchingHeader, MatchingShard, ReplayEncoding,
	ReplayGate,
	matching_resource::{
		MatchingResource, ResourceMatchingEncoding, ResourceMatchingRecord, ResourceReplayLimits,
	},
};

/// Classical construction and each complete replay have separate explicit caps.
#[derive(Clone, Copy, Debug)]
pub struct TemporalEncodingLimits {
	pub max_history_dimension: usize,
	/// Includes recipe/header construction and the shared replay admission model.
	pub max_preparation_work: usize,
	pub replay: ResourceReplayLimits,
}
impl Default for TemporalEncodingLimits {
	fn default() -> Self {
		Self {
			max_history_dimension: 1_048_576,
			max_preparation_work: 100_000_000,
			replay: ResourceReplayLimits::default(),
		}
	}
}
/// Counts describe construction plus one admitted replay, never a free oracle.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct TemporalEncodingResources {
	pub configuration_dimension: usize,
	pub history_dimension: usize,
	pub record_count: usize,
	pub padded_matching_labels: usize,
	pub normalization: f64,
	/// Conservative metadata scan plus shared admission/replay work units.
	pub preparation_work: usize,
	pub replay_gates: usize,
	pub resource_queries: usize,
	pub retained_bytes: usize,
	/// Norm bound for the stored rectangular interpolation map, before division by alpha.
	pub interpolation_norm_upper_bound: f64,
}
#[derive(Clone, Debug)]
struct Directory {
	header: MatchingHeader,
	starts: [usize; 3],
	weights: [f64; 3],
	theta: [f64; 3],
	phase: [f64; 3],
	nodes: usize,
	edge_cosine: [f64; 3],
	edge_sine: [f64; 3],
	edge_phase: [Complex64; 3],
	frozen: u64,
}
const fn invalid() -> CfdError {
	CfdError::InvalidInput("temporal encoding shape/work/storage overflow")
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(invalid)
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(invalid)
}
fn word(n: usize) -> Result<u64, CfdError> {
	u64::try_from(n).map_err(|_| invalid())
}
fn hash(mut current: u64, word: u64) -> u64 {
	for byte in word.to_le_bytes() {
		current = (current ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
	}
	current
}
impl Directory {
	fn active(&self, color: usize) -> bool {
		color < self.nodes && self.weights[color] != 0.
	}
	fn record(&self, color: usize, source: usize) -> Option<ResourceMatchingRecord> {
		if !self.active(color) {
			return None;
		}
		let start = self.starts[color];
		let n = self.header.rows;
		if source >= start && source - start < n {
			Some(ResourceMatchingRecord {
				column: MatchingColumn {
					color,
					source,
					destination: source - start,
					cosine: self.edge_cosine[color],
					sine: self.edge_sine[color],
					phase: self.edge_phase[color],
				},
				theta: self.theta[color],
				phase_angle: self.phase[color],
				is_edge: true,
			})
		} else if start > 0 && source < n {
			Some(ResourceMatchingRecord {
				column: MatchingColumn {
					color,
					source,
					destination: start + source,
					cosine: 0.,
					sine: 1.,
					phase: Complex64::new(1., 0.),
				},
				theta: std::f64::consts::PI,
				phase_angle: 0.,
				is_edge: false,
			})
		} else {
			None
		}
	}
}
impl MatchingResource for Directory {
	fn header(&self) -> MatchingHeader {
		self.header
	}
	fn frozen_identity(&self) -> u64 {
		self.frozen
	}
	fn retained_bytes(&self) -> quest_qsvt::Result<usize> {
		Ok(size_of::<Self>())
	}
	fn query_work_bound(&self) -> usize {
		512
	}
	fn query_communication_bound(&self) -> usize {
		0
	}
	fn next_record(
		&self,
		color: usize,
		bound: Option<usize>,
		reverse: bool,
	) -> quest_qsvt::Result<Option<ResourceMatchingRecord>> {
		if color >= self.header.num_colors {
			return Err(quest_qsvt::Error::Encoding("temporal matching color"));
		}
		if !self.active(color) {
			return Ok(None);
		}
		let n = self.header.rows;
		let start = self.starts[color];
		// The two source intervals are disjoint unless this is the identity block.
		let ranges = if start == 0 {
			[(0, n), (0, 0)]
		} else {
			[(0, n), (start, start + n)]
		};
		let mut candidate = None;
		for (lo, hi) in ranges {
			if lo == hi {
				continue;
			}
			let value = if reverse {
				let upper = bound.map_or(hi, |b| b.min(hi));
				(upper > lo).then(|| upper - 1)
			} else {
				let lower = bound.map_or(Some(lo), |b| b.checked_add(1).map(|x| x.max(lo)));
				lower.filter(|&x| x < hi)
			};
			if let Some(v) = value {
				candidate =
					Some(candidate.map_or(
						v,
						|old: usize| if reverse { old.max(v) } else { old.min(v) },
					));
			}
		}
		Ok(candidate.and_then(|i| self.record(color, i)))
	}
	fn forward(
		&self,
		color: usize,
		source: usize,
	) -> quest_qsvt::Result<Option<ResourceMatchingRecord>> {
		if color >= self.header.num_colors || source >= self.header.system_dimension()? {
			return Err(quest_qsvt::Error::Encoding("temporal forward index"));
		}
		Ok(self.record(color, source))
	}
	fn reverse(&self, color: usize, destination: usize) -> quest_qsvt::Result<Option<usize>> {
		// Every completion is an identity or a transposition, hence self-inverse.
		Ok(self
			.forward(color, destination)?
			.map(|r| r.column.destination))
	}
}

/// Owning block encoding of `C[i, history_index(i,a)] = weight[a]`.
///
/// Source storage is constant in the configuration dimension. The emitted circuit
/// explicitly enumerates its controlled rotations and completed transpositions;
/// no logarithmic query-cost claim is made. Successful output is `C z / alpha`.
/// Its probability is `||C z||²/alpha²` for a normalized input in the right
/// logical subspace. Joint inverse/workspace selection is an additional obligation.
#[derive(Clone, Debug)]
pub struct TemporalEncoding {
	encoding: ResourceMatchingEncoding<Directory>,
	resources: TemporalEncodingResources,
	semantics: [u64; 11],
	physical_time: f64,
	slab: usize,
	side: TemporalNodeSide,
}
impl TemporalEncoding {
	/// Construct the complete unitary including unused labels, failure sectors and padding.
	/// Coefficients are stored binary64 interpolation values; rounding/physical time
	/// consistency is not certified by the matching identity.
	/// # Errors
	/// Rejects shape, arithmetic, work/storage/query/gate caps before returning an owner.
	#[allow(
		clippy::too_many_lines,
		reason = "Keep pre-scan admission, frozen records and whole-stream admission together"
	)]
	pub fn new(
		interpolation: &TemporalInterpolation,
		limits: TemporalEncodingLimits,
	) -> Result<Self, CfdError> {
		let n = interpolation.configuration_dimension();
		let h = interpolation.history_dimension();
		let q = interpolation.node_count();
		if n == 0 || h > limits.max_history_dimension || !matches!(q, 2 | 3) {
			return Err(invalid());
		}
		let nodes = interpolation.weights();
		let mut starts = [0; 3];
		let mut weights = [0.; 3];
		let mut theta = [0.; 3];
		let mut phase = [0.; 3];
		let mut count = 0;
		let mut beta = 0_f64;
		for a in 0..q {
			starts[a] = interpolation.history_index(0, a)?;
			if add(starts[a], n)? > h || (starts[a] != 0 && starts[a] < n) || !nodes[a].is_finite()
			{
				return Err(invalid());
			}
			weights[a] = nodes[a];
			beta = beta.max(nodes[a].abs());
			if nodes[a] != 0. {
				count = add(count, mul(n, if starts[a] == 0 { 1 } else { 2 })?)?;
			}
		}
		if beta == 0. {
			return Err(invalid());
		}
		let metadata_work = add(4096, mul(add(count, mul(n, q)?)?, 4096)?)?;
		let remaining = limits
			.max_preparation_work
			.checked_sub(metadata_work)
			.ok_or_else(invalid)?;
		let mut replay = limits.replay;
		replay.max_work = replay.max_work.min(remaining);
		replay.max_bytes = replay
			.max_bytes
			.checked_sub(size_of::<Self>())
			.ok_or_else(invalid)?;
		let system = h.checked_next_power_of_two().ok_or_else(invalid)?;
		let labels = q.next_power_of_two();
		let alpha = beta * f64::from(u32::try_from(labels).map_err(|_| invalid())?);
		for a in 0..q {
			if weights[a] != 0. {
				theta[a] = 2. * (weights[a].abs() / beta).clamp(0., 1.).acos();
				phase[a] = if weights[a] < 0. {
					std::f64::consts::PI
				} else {
					0.
				};
			}
		}
		let header = MatchingHeader {
			rows: n,
			cols: h,
			system_qubits: usize::try_from(system.ilog2()).map_err(|_| invalid())?,
			color_qubits: usize::try_from(labels.ilog2()).map_err(|_| invalid())?,
			num_colors: labels,
			beta,
			alpha,
			source_identity: 0,
			record_count: count,
			record_digest: 0,
		};
		header.validate()?;
		let mut edge_cosine = [0.; 3];
		let mut edge_sine = [1.; 3];
		let mut edge_phase = [Complex64::new(1., 0.); 3];
		for a in 0..q {
			if weights[a] != 0. {
				(edge_sine[a], edge_cosine[a]) = (0.5 * theta[a]).sin_cos();
				edge_phase[a] = Complex64::from_polar(1., phase[a]);
			}
		}
		let mut directory = Directory {
			header,
			starts,
			weights,
			theta,
			phase,
			nodes: q,
			edge_cosine,
			edge_sine,
			edge_phase,
			frozen: 0,
		};
		// Same canonical row-major represented-matrix fingerprint as the stored baseline.
		let mut identity = hash(hash(0xcbf2_9ce4_8422_2325, word(n)?), word(h)?);
		for i in 0..n {
			for a in 0..q {
				if weights[a] != 0. {
					for w in [word(i)?, word(starts[a] + i)?, weights[a].to_bits(), 0] {
						identity = hash(identity, w);
					}
				}
			}
		}
		directory.header.source_identity = identity;
		let mut digest = 0_u64;
		for color in 0..q {
			let mut previous = None;
			while let Some(record) = directory.next_record(color, previous, false)? {
				digest = digest.wrapping_add(
					MatchingShard::summarize_records(std::slice::from_ref(&record.column))?.1,
				);
				previous = Some(record.column.source);
			}
		}
		directory.header.record_digest = digest;
		let mut frozen = hash(0x5449_4d45_434f_4831, digest);
		for a in 0..q {
			frozen = hash(hash(frozen, theta[a].to_bits()), phase[a].to_bits());
		}
		directory.frozen = frozen;
		let encoding = ResourceMatchingEncoding::new(directory, replay)?;
		let replay_counts = encoding.recipe().statistics();
		let retained_bytes = add(encoding.retained_bytes()?, size_of::<Self>())?;
		if retained_bytes > limits.replay.max_bytes {
			return Err(invalid());
		}
		let resources = TemporalEncodingResources {
			configuration_dimension: n,
			history_dimension: h,
			record_count: count,
			padded_matching_labels: labels,
			normalization: alpha,
			preparation_work: add(metadata_work, replay_counts.work)?,
			replay_gates: replay_counts.gates,
			resource_queries: replay_counts.queries,
			retained_bytes,
			interpolation_norm_upper_bound: interpolation.norm_upper_bound(),
		};
		Ok(Self {
			encoding,
			resources,
			semantics: interpolation.semantic_words(),
			physical_time: interpolation.physical_time(),
			slab: interpolation.slab(),
			side: interpolation.side(),
		})
	}
	#[must_use]
	pub const fn resources(&self) -> TemporalEncodingResources {
		self.resources
	}
	#[must_use]
	pub const fn history_semantics(&self) -> [u64; 11] {
		self.semantics
	}
	#[must_use]
	pub const fn physical_time(&self) -> f64 {
		self.physical_time
	}
	/// Explicit slab distinguishes the two possible DG traces at a shared time.
	#[must_use]
	pub const fn slab(&self) -> usize {
		self.slab
	}
	/// Position within the selected slab, including one-sided endpoint traces.
	#[must_use]
	pub const fn side(&self) -> TemporalNodeSide {
		self.side
	}
}
impl ReplayEncoding for TemporalEncoding {
	fn descriptor(&self) -> quest_qsvt::Result<EncodingDescriptor> {
		self.encoding.descriptor()
	}
	fn retained_bytes(&self) -> quest_qsvt::Result<usize> {
		Ok(self.resources.retained_bytes)
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> quest_qsvt::Result<()>,
	) -> quest_qsvt::Result<()> {
		self.encoding.visit_replay(adjoint, visitor)
	}
}
