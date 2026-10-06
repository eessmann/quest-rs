//! Complete generated physical drift into synchronized `KvN` history construction.
//!
//! Collective physics is private to the agreed global row schedule. The resulting
//! owner-local disk reader performs no MPI; independently advancing sparse readers
//! never invoke physical collectives. Classical preparation is fully charged.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Checked index spaces and fixed agreed row protocols bound arithmetic"
)]
use crate::{
	CfdError,
	configuration::ConfigurationGrid,
	distributed_history::{
		DistributedHistoryLimits, DistributedHistoryOutcome, prepare_history_inverse_from_factory,
	},
	physical_space::{
		DistributedBoxDrift, DistributedTimeBoxDrift, PreparedBoxForce, PreparedTimeBoxForce,
	},
	stream_history::{HistoryRowDynamics, HistoryStreamLimits, TemporalHistoryRecipe},
};
use quest::collective::{CollectiveEnvironment, CollectiveReservation};
use quest_numerics::{
	Complex64,
	sparse_stream::{RunReader, RunStore, SparseEntry},
};
use quest_qsvt::reciprocal::SpectralBounds;
use quest_qsvt_io::sparse_stream::{FileRun, FileRunReader, FileRunStore};
use sha2::{Digest, Sha256};
use std::{
	cell::{Cell, RefCell},
	ops::Range,
	path::PathBuf,
};

/// Independent complete-construction ceilings. Arithmetic units are conservative
/// elementary operations, not timings. Transport is global directed payload.
#[derive(Clone, Debug)]
pub struct BoxKvnHistoryLimits {
	pub max_dimension: usize,
	pub max_drift_calls: usize,
	/// Per-rank input validation/hash/ownership admission, including the zero-RHS path.
	pub max_source_prepare_work: usize,
	pub max_rank_work: usize,
	pub max_aggregate_work: usize,
	pub max_transport_bytes: usize,
	pub max_local_bytes: usize,
	pub max_node_bytes: usize,
	pub ranks_per_node: usize,
	pub max_local_disk_bytes: usize,
	pub max_global_disk_bytes: usize,
	pub max_node_disk_bytes: usize,
	/// Local scratch paths may differ; they are not physical source identities.
	pub scratch_directory: PathBuf,
	pub max_path_bytes: usize,
}
impl Default for BoxKvnHistoryLimits {
	fn default() -> Self {
		Self {
			max_dimension: 1_000_000,
			max_drift_calls: 1_000_000,
			max_source_prepare_work: 100_000_000,
			max_rank_work: 100_000_000_000,
			max_aggregate_work: 800_000_000_000,
			max_transport_bytes: 100_000_000_000,
			max_local_bytes: 512 * 1024 * 1024,
			max_node_bytes: usize::MAX,
			ranks_per_node: usize::MAX,
			max_local_disk_bytes: 256 * 1024 * 1024,
			max_global_disk_bytes: 1024 * 1024 * 1024,
			max_node_disk_bytes: 1024 * 1024 * 1024,
			scratch_directory: std::env::temp_dir(),
			max_path_bytes: 4096,
		}
	}
}
/// Whole-construction ceilings and actual rank-owned spool observations.
#[derive(Clone, Copy, Debug)]
pub struct BoxKvnHistoryResources {
	pub physical_coordinates: usize,
	pub configuration_dimension: usize,
	pub history_dimension: usize,
	pub maximum_drift_calls: usize,
	pub actual_drift_calls: usize,
	pub source_prepare_work: usize,
	pub source_prepare_transport_bytes: usize,
	pub maximum_rank_work: usize,
	pub aggregate_work: usize,
	pub global_transport_bytes: usize,
	pub maximum_rank_peak_bytes: usize,
	pub node_peak_bytes: usize,
	pub local_disk_ceiling: usize,
	pub global_disk_ceiling: usize,
	pub node_disk_ceiling: usize,
	pub local_entries: usize,
	pub local_disk_bytes: usize,
	/// Recipe parameter digest; excludes floating QR basis. The produced H header binds actual coefficients.
	pub recipe_metadata_sha256: [u8; 32],
	pub local_spool_sha256: [u8; 32],
}
/// Reusable admitted inverse plus construction evidence. Zero RHS skips the spool.
pub struct BoxKvnHistoryPreparation<'env, 'comm, 'runtime> {
	pub inverse: DistributedHistoryOutcome<'env, 'comm, 'runtime>,
	pub construction: Option<BoxKvnHistoryResources>,
}
#[allow(
	clippy::needless_pass_by_value,
	reason = "map_err consumes backend errors"
)]
fn native(e: quest::Error) -> CfdError {
	CfdError::Unsupported(e.to_string())
}
const fn overflow() -> CfdError {
	CfdError::InvalidInput("box KvN history resource overflow")
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(overflow)
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(overflow)
}
fn agree<T>(
	env: &CollectiveEnvironment<'_, '_>,
	value: Result<T, CfdError>,
) -> Result<T, CfdError> {
	let mut lane = env.communicator().collective_lane().map_err(|e| {
		native(quest::Error::Backend {
			operation: "opening box history agreement",
			source: e,
		})
	})?;
	if !lane.all_agree(value.is_ok()).map_err(|e| {
		native(quest::Error::Backend {
			operation: "agreeing box history stage",
			source: e,
		})
	})? {
		return Err(CfdError::InvalidInput(
			"collective box history stage rejected",
		));
	}
	value
}
fn broadcast(
	env: &CollectiveEnvironment<'_, '_>,
	peer: i32,
	bytes: &mut [u8],
) -> Result<(), CfdError> {
	env.communicator()
		.collective_lane()
		.map_err(|e| {
			native(quest::Error::Backend {
				operation: "opening box history transport",
				source: e,
			})
		})?
		.broadcast_bytes(peer, bytes)
		.map_err(|e| {
			native(quest::Error::Backend {
				operation: "broadcasting box history metadata/scalar",
				source: e,
			})
		})
}
fn maximum(env: &CollectiveEnvironment<'_, '_>, x: usize) -> Result<usize, CfdError> {
	let mut max = 0;
	for peer in 0..env.size().map_err(native)? {
		let mut bytes = u64::try_from(x).map_err(|_| overflow())?.to_le_bytes();
		broadcast(env, peer, &mut bytes)?;
		max = max.max(usize::try_from(u64::from_le_bytes(bytes)).map_err(|_| overflow())?);
	}
	Ok(max)
}
fn common(env: &CollectiveEnvironment<'_, '_>, bytes: &[u8]) -> Result<(), CfdError> {
	agree(
		env,
		if bytes.len() <= 256 {
			Ok(())
		} else {
			Err(CfdError::InvalidInput("box history common frame"))
		},
	)?;
	let mut frame = [0; 256];
	frame[..bytes.len()].copy_from_slice(bytes);
	let local = frame;
	broadcast(env, 0, &mut frame)?;
	agree(
		env,
		if frame == local {
			Ok(())
		} else {
			Err(CfdError::InvalidInput("box history semantics mismatch"))
		},
	)
}
fn words_bytes<const N: usize>(words: [u64; N]) -> Result<[u8; 256], CfdError> {
	if N > 32 {
		return Err(CfdError::InvalidInput("box history metadata word capacity"));
	}
	let mut b = [0; 256];
	for (chunk, word) in b.as_chunks_mut::<8>().0.iter_mut().zip(words) {
		chunk.copy_from_slice(&word.to_le_bytes());
	}
	Ok(b)
}
fn hash_entry(hash: &mut Sha256, e: SparseEntry) -> quest_numerics::Result<()> {
	for w in [
		u64::try_from(e.row).map_err(|_| quest_numerics::Error::Overflow)?,
		u64::try_from(e.column).map_err(|_| quest_numerics::Error::Overflow)?,
		e.ordinal,
		e.value.re.to_bits(),
		e.value.im.to_bits(),
	] {
		hash.update(w.to_le_bytes());
	}
	Ok(())
}
fn word(n: usize) -> Result<u64, CfdError> {
	u64::try_from(n).map_err(|_| overflow())
}
/// Private collectively scheduled dynamics. Never handed to an independent reader.
// Sealed dispatch: arbitrary callbacks cannot introduce unmatched physics collectives.
struct ForceAdmission {
	total_work: usize,
	total_transport_bytes: usize,
	maximum_rank_peak_bytes: usize,
}
enum ForceResult<'env> {
	Autonomous(DistributedBoxDrift<'env>),
	Polynomial(DistributedTimeBoxDrift<'env>),
}
impl ForceResult<'_> {
	fn as_slice(&self) -> &[f64] {
		match self {
			Self::Autonomous(v) => v.as_slice(),
			Self::Polynomial(v) => v.as_slice(),
		}
	}
}
trait CollectiveForce<'env, 'comm, 'runtime> {
	fn environment(&self) -> &CollectiveEnvironment<'comm, 'runtime>;
	fn dimension(&self) -> usize;
	fn local_coordinate_range(&self) -> Range<usize>;
	fn coordinate_owner(&self, index: usize) -> quest::Result<i32>;
	fn semantic_words(&self) -> quest::Result<[u64; 13]>;
	fn admit_drift(&self) -> quest::Result<ForceAdmission>;
	fn drift_at(&self, time: f64, state: &[f64]) -> quest::Result<ForceResult<'env>>;
	fn time_interval(&self) -> Option<[f64; 2]>;
}
impl<'env, 'comm, 'runtime> CollectiveForce<'env, 'comm, 'runtime>
	for PreparedBoxForce<'_, '_, '_, 'env, 'comm, 'runtime>
{
	fn environment(&self) -> &CollectiveEnvironment<'comm, 'runtime> {
		self.environment()
	}
	fn dimension(&self) -> usize {
		self.dimension()
	}
	fn local_coordinate_range(&self) -> Range<usize> {
		self.local_coordinate_range()
	}
	fn coordinate_owner(&self, index: usize) -> quest::Result<i32> {
		self.coordinate_owner(index)
	}
	fn semantic_words(&self) -> quest::Result<[u64; 13]> {
		let mut out = [0; 13];
		out[..9].copy_from_slice(&self.semantic_words()?);
		Ok(out)
	}
	fn admit_drift(&self) -> quest::Result<ForceAdmission> {
		let v = self.admit_drift()?;
		Ok(ForceAdmission {
			total_work: v.total_work,
			total_transport_bytes: v.total_transport_bytes,
			maximum_rank_peak_bytes: v.maximum_rank_peak_bytes,
		})
	}
	fn drift_at(&self, _: f64, state: &[f64]) -> quest::Result<ForceResult<'env>> {
		self.drift(state).map(ForceResult::Autonomous)
	}
	fn time_interval(&self) -> Option<[f64; 2]> {
		None
	}
}
impl<'env, 'comm, 'runtime> CollectiveForce<'env, 'comm, 'runtime>
	for PreparedTimeBoxForce<'_, '_, '_, '_, 'env, 'comm, 'runtime>
{
	fn environment(&self) -> &CollectiveEnvironment<'comm, 'runtime> {
		self.environment()
	}
	fn dimension(&self) -> usize {
		self.dimension()
	}
	fn local_coordinate_range(&self) -> Range<usize> {
		self.local_coordinate_range()
	}
	fn coordinate_owner(&self, index: usize) -> quest::Result<i32> {
		self.coordinate_owner(index)
	}
	fn semantic_words(&self) -> quest::Result<[u64; 13]> {
		self.semantic_words()
	}
	fn admit_drift(&self) -> quest::Result<ForceAdmission> {
		let v = self.admit_drift()?;
		Ok(ForceAdmission {
			total_work: v.maximum_rank_work,
			total_transport_bytes: v.global_transport_bytes,
			maximum_rank_peak_bytes: v.maximum_rank_peak_bytes,
		})
	}
	fn drift_at(&self, time: f64, state: &[f64]) -> quest::Result<ForceResult<'env>> {
		self.drift_at(time, state).map(ForceResult::Polynomial)
	}
	fn time_interval(&self) -> Option<[f64; 2]> {
		Some(self.time_interval())
	}
}
struct CollectiveDynamics<'a, 'env, 'comm, 'runtime> {
	env: &'env CollectiveEnvironment<'comm, 'runtime>,
	force: &'a dyn CollectiveForce<'env, 'comm, 'runtime>,
	grid: &'a ConfigurationGrid,
	point: RefCell<Vec<f64>>,
	left: RefCell<Vec<f64>>,
	calls: Cell<usize>,
	entries: usize,
	query_work: usize,
	prepare_work: usize,
	prepare_transport: usize,
	retained: usize,
	_reservation: CollectiveReservation<'env>,
}
impl<'a, 'env, 'comm, 'runtime> CollectiveDynamics<'a, 'env, 'comm, 'runtime> {
	#[allow(
		clippy::too_many_lines,
		reason = "Keep collective source ownership and complete input admission in order"
	)]
	fn new(
		env: &'env CollectiveEnvironment<'comm, 'runtime>,
		force: &'a dyn CollectiveForce<'env, 'comm, 'runtime>,
		grid: &'a ConfigurationGrid,
		l: &BoxKvnHistoryLimits,
	) -> Result<Self, CfdError> {
		let parts = usize::try_from(env.size().map_err(native)?).map_err(|_| overflow())?;
		let node = if l.ranks_per_node == usize::MAX {
			parts
		} else {
			l.ranks_per_node
		};
		agree(
			env,
			if std::ptr::eq(env, force.environment())
				&& parts.is_power_of_two()
				&& node > 0
				&& node <= parts
				&& grid.axes() == force.dimension()
				&& grid.dimension() <= l.max_dimension
				&& l.scratch_directory.capacity() <= l.max_path_bytes
			{
				Ok(())
			} else {
				Err(CfdError::InvalidInput(
					"box history full coordinates/owner/policy",
				))
			},
		)?;
		let prepare_work = agree(
			env,
			(|| {
				let w = add(
					mul(
						mul(grid.axis_dimension(), add(grid.axis_dimension(), 64)?)?,
						64,
					)?,
					add(
						mul(mul(grid.axes(), grid.axes())?, mul(parts, 64)?)?,
						mul(mul(parts, parts)?, 16384)?,
					)?,
				)?;
				if w > l.max_source_prepare_work
					|| w > l.max_rank_work
					|| mul(w, parts)? > l.max_aggregate_work
				{
					return Err(CfdError::InvalidInput("box history source validation work"));
				}
				Ok(w)
			})(),
		)?;
		let prepare_transport = agree(
			env,
			mul(mul(parts, parts)?, mul(add(grid.axes(), 8)?, 4096)?),
		)?;
		agree(
			env,
			if prepare_transport <= l.max_transport_bytes {
				Ok(())
			} else {
				Err(CfdError::InvalidInput(
					"box history source validation transport",
				))
			},
		)?;
		// Hash every retained one-dimensional grid coefficient, never a tensor catalogue.
		let mut hash = Sha256::new();
		for w in force.semantic_words().map_err(native)? {
			hash.update(w.to_le_bytes());
		}
		hash.update(words_bytes([
			word(grid.axes())?,
			word(grid.dimension())?,
			word(grid.axis_dimension())?,
		])?);
		let mut longest = 0;
		let valid = (|| {
			for i in 0..grid.axis_dimension() {
				let x = grid.axis_node(i).ok_or_else(overflow)?;
				let w = grid.axis_weight(i).ok_or_else(overflow)?;
				if !x.is_finite() || !w.is_finite() || w <= 0. {
					return Err(CfdError::InvalidInput("box history grid coefficients"));
				}
				hash.update(x.to_bits().to_le_bytes());
				hash.update(w.to_bits().to_le_bytes());
				let row = grid.axis_derivative_row(i).ok_or_else(overflow)?;
				longest = longest.max(row.len());
				hash.update((word(row.len())?).to_le_bytes());
				for &(j, d) in row {
					if j >= grid.axis_dimension() || !d.is_finite() {
						return Err(CfdError::InvalidInput("box history grid derivative"));
					}
					hash.update((word(j)?).to_le_bytes());
					hash.update(d.to_bits().to_le_bytes());
				}
			}
			Ok(())
		})();
		agree(env, valid)?;
		let identity: [u8; 32] = hash.finalize().into();
		common(env, &identity)?;
		common(
			env,
			&words_bytes([
				word(l.max_source_prepare_work)?,
				word(l.max_dimension)?,
				word(l.max_drift_calls)?,
				word(l.max_rank_work)?,
				word(l.max_aggregate_work)?,
				word(l.max_transport_bytes)?,
				word(l.max_local_bytes)?,
				word(l.max_node_bytes)?,
				word(l.ranks_per_node)?,
				word(l.max_local_disk_bytes)?,
				word(l.max_global_disk_bytes)?,
				word(l.max_node_disk_bytes)?,
				word(l.max_path_bytes)?,
			])?,
		)?;
		// Independently verify the complete QR tail partition, including empty ranks.
		let own = force.local_coordinate_range();
		let mut cursor = 0;
		for peer in 0..env.size().map_err(native)? {
			let mut b = [0; 16];
			b[..8].copy_from_slice(&(word(own.start)?).to_le_bytes());
			b[8..].copy_from_slice(&(word(own.end)?).to_le_bytes());
			broadcast(env, peer, &mut b)?;
			let a = usize::try_from(u64::from_le_bytes(
				b[..8].try_into().map_err(|_| overflow())?,
			))
			.map_err(|_| overflow())?;
			let z = usize::try_from(u64::from_le_bytes(
				b[8..].try_into().map_err(|_| overflow())?,
			))
			.map_err(|_| overflow())?;
			agree(
				env,
				if a == cursor
					&& a <= z
					&& z <= force.dimension()
					&& (a..z).all(|j| force.coordinate_owner(j).is_ok_and(|r| r == peer))
				{
					Ok(())
				} else {
					Err(CfdError::InvalidInput("box history null ownership"))
				},
			)?;
			cursor = z;
		}
		agree(
			env,
			if cursor == force.dimension() {
				Ok(())
			} else {
				Err(CfdError::InvalidInput("box history null coverage"))
			},
		)?;
		let entries = agree(env, mul(grid.axes(), longest))?;
		let f = force.admit_drift().map_err(native)?;
		let query_work = agree(
			env,
			add(
				mul(add(entries, 1)?, f.total_work)?,
				mul(
					add(entries, grid.axes())?,
					add(mul(grid.axes(), 64)?, mul(mul(parts, parts)?, 4096)?)?,
				)?,
			),
		)?;
		let bytes = agree(
			env,
			(|| {
				add(
					add(mul(own.len(), 16)?, grid.retained_bytes()?)?,
					add(l.scratch_directory.capacity(), 8192)?,
				)
			})(),
		)?;
		let planned_local = agree(env, add(env.view().allocated_bytes(), bytes))?;
		let planned = maximum(env, planned_local)?;
		agree(
			env,
			if planned <= l.max_local_bytes && mul(planned, node)? <= l.max_node_bytes {
				Ok(())
			} else {
				Err(CfdError::InvalidInput("box history source capacity"))
			},
		)?;
		let make = || {
			let mut v = Vec::new();
			v.try_reserve_exact(own.len())
				.map_err(|_| CfdError::InvalidInput("box history point allocation"))?;
			v.resize(own.len(), 0.);
			Ok(v)
		};
		let point = agree(env, make())?;
		let left = agree(env, make())?;
		let retained = agree(
			env,
			(|| {
				add(
					add(
						mul(add(point.capacity(), left.capacity())?, 8)?,
						grid.retained_bytes()?,
					)?,
					add(l.scratch_directory.capacity(), 8192)?,
				)
			})(),
		)?;
		let local_peak = agree(env, add(env.view().allocated_bytes(), retained))?;
		let peak = maximum(env, local_peak)?;
		agree(
			env,
			if peak <= l.max_local_bytes && mul(peak, node)? <= l.max_node_bytes {
				Ok(())
			} else {
				Err(CfdError::InvalidInput("box history actual source capacity"))
			},
		)?;
		let reservation = env.reserve_external_bytes(retained).map_err(native)?;
		Ok(Self {
			env,
			force,
			grid,
			point: RefCell::new(point),
			left: RefCell::new(left),
			calls: Cell::new(0),
			entries,
			query_work,
			prepare_work,
			prepare_transport,
			retained,
			_reservation: reservation,
		})
	}
	fn fill_point(&self, index: usize) -> Result<(), CfdError> {
		if index >= self.grid.dimension() {
			return Err(CfdError::InvalidInput("box history point index"));
		}
		let mut point = self.point.borrow_mut();
		for (axis, v) in self.force.local_coordinate_range().zip(point.iter_mut()) {
			let mut radix = index;
			for _ in 0..axis {
				radix /= self.grid.axis_dimension();
			}
			*v = self
				.grid
				.axis_node(radix % self.grid.axis_dimension())
				.ok_or_else(overflow)?;
		}
		Ok(())
	}
	fn scalar(&self, axis: usize, values: &[f64]) -> Result<f64, CfdError> {
		let range = self.force.local_coordinate_range();
		let value = if range.contains(&axis) {
			values[axis - range.start]
		} else {
			0.
		};
		let mut b = value.to_le_bytes();
		broadcast(
			self.env,
			self.force.coordinate_owner(axis).map_err(native)?,
			&mut b,
		)?;
		let value = f64::from_le_bytes(b);
		if value.is_finite() {
			Ok(value)
		} else {
			Err(CfdError::InvalidInput("box history drift scalar"))
		}
	}
	fn drift(&self, time: f64, index: usize) -> Result<ForceResult<'env>, CfdError> {
		agree(self.env, self.fill_point(index))?;
		self.calls.set(add(self.calls.get(), 1)?);
		self.force
			.drift_at(time, &self.point.borrow())
			.map_err(native)
	}
}
impl HistoryRowDynamics for CollectiveDynamics<'_, '_, '_, '_> {
	fn dimension(&self) -> usize {
		self.grid.dimension()
	}
	fn max_row_entries(&self) -> usize {
		self.entries
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(self.retained)
	}
	fn row_query_bytes(&self) -> usize {
		8192
	}
	fn row_query_work(&self) -> usize {
		self.query_work
	}
	fn source_entry(&self, time: f64, row: usize) -> Result<Complex64, CfdError> {
		// This path is independently queried for RHS preparation: absolutely no MPI.
		if !time.is_finite() || row >= self.grid.dimension() {
			Err(CfdError::InvalidInput("box history source index/time"))
		} else {
			Ok(Complex64::new(0., 0.))
		}
	}
	fn visit_row(
		&self,
		time: f64,
		row: usize,
		v: &mut dyn FnMut(usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		agree(self.env, self.source_entry(time, row).map(|_| ()))?;
		{
			let left = self.drift(time, row)?;
			self.left.borrow_mut().copy_from_slice(left.as_slice());
		}
		let mut stride = 1;
		for axis in 0..self.grid.axes() {
			let a = (row / stride) % self.grid.axis_dimension();
			let wa = self.grid.axis_weight(a).ok_or_else(overflow)?;
			let left = self.scalar(axis, &self.left.borrow())?;
			for &(b, d) in self.grid.axis_derivative_row(a).ok_or_else(overflow)? {
				let column = row - a * stride + b * stride;
				let right = {
					let r = self.drift(time, column)?;
					self.scalar(axis, r.as_slice())?
				};
				let wb = self.grid.axis_weight(b).ok_or_else(overflow)?;
				let value = -0.5 * (left + right) * d * (wa / wb).sqrt();
				agree(
					self.env,
					if value.is_finite() {
						v(column, Complex64::new(value, 0.))
					} else {
						Err(CfdError::InvalidInput("box history KvN arithmetic"))
					},
				)?;
			}
			stride = mul(stride, self.grid.axis_dimension())?;
		}
		Ok(())
	}
}
/// Local immutable spool owner. Keeping its file and accounting guards alive is
/// essential: producer consumption occurs after the collective factory returns.
struct Spool<'env> {
	_file_reservation: CollectiveReservation<'env>,
	reader: FileRunReader,
	_file: FileRun,
	_store: FileRunStore,
	range: Range<usize>,
	dimension: usize,
	slots: usize,
	count: usize,
	seen: usize,
	expected: [u8; 32],
	hash: Sha256,
	last: Option<u64>,
	failed: bool,
	_reservation: CollectiveReservation<'env>,
}
impl Iterator for Spool<'_> {
	type Item = quest_numerics::Result<SparseEntry>;
	fn next(&mut self) -> Option<Self::Item> {
		if self.failed {
			return None;
		}
		let next = (|| {
			let e = self.reader.next_entry()?;
			if let Some(e) = e {
				if self.seen >= self.count
					|| !self.range.contains(&e.row)
					|| e.column >= self.dimension
					|| e.ordinal
						/ (u64::try_from(self.slots)
							.map_err(|_| quest_numerics::Error::Overflow)?)
						!= (u64::try_from(e.row).map_err(|_| quest_numerics::Error::Overflow)?)
					|| self.last.is_some_and(|n| e.ordinal <= n)
					|| !e.value.re.is_finite()
					|| !e.value.im.is_finite()
				{
					return Err(quest_numerics::Error::Domain("box history spool integrity"));
				}
				hash_entry(&mut self.hash, e)?;
				self.seen += 1;
				self.last = Some(e.ordinal);
			} else if self.seen != self.count
				|| <[u8; 32]>::from(self.hash.clone().finalize()) != self.expected
			{
				return Err(quest_numerics::Error::Domain(
					"box history spool digest/count",
				));
			}
			Ok(e)
		})();
		match next {
			Ok(Some(e)) => Some(Ok(e)),
			Ok(None) => {
				self.failed = true;
				None
			}
			Err(e) => {
				self.failed = true;
				Some(Err(e))
			}
		}
	}
}
fn write_entry(
	store: &mut FileRunStore,
	writer: &mut quest_qsvt_io::sparse_stream::FileRunWriter,
	e: SparseEntry,
) -> quest_numerics::Result<()> {
	#[cfg(test)]
	if WRITE_FAILURE_ROW.with(|row| row.get() == Some(e.row)) {
		return Err(quest_numerics::Error::Domain(
			"injected local spool write error",
		));
	}
	store.write(writer, e)
}
#[cfg(test)]
thread_local! { static WRITE_FAILURE_ROW: Cell<Option<usize>>=const{Cell::new(None)}; }
#[allow(
	clippy::too_many_lines,
	reason = "Keep admission and complete row protocol together"
)]
fn build_spool<'env>(
	env: &'env CollectiveEnvironment<'_, '_>,
	source: &CollectiveDynamics<'_, 'env, '_, '_>,
	recipe: &TemporalHistoryRecipe<'_, CollectiveDynamics<'_, 'env, '_, '_>>,
	range: Range<usize>,
	l: &BoxKvnHistoryLimits,
) -> Result<(Spool<'env>, BoxKvnHistoryResources), CfdError> {
	let parts = usize::try_from(env.size().map_err(native)?).map_err(|_| overflow())?;
	let node = if l.ranks_per_node == usize::MAX {
		parts
	} else {
		l.ranks_per_node
	};
	let starting_calls = source.calls.get();
	let n = recipe.dimension();
	let slots = recipe.resources().maximum_row_entries;
	let force = source.force.admit_drift().map_err(native)?;
	let resources = agree(
		env,
		(|| {
			if range.end > n || range.start > range.end || n > l.max_dimension {
				return Err(CfdError::InvalidInput("box history spool shape"));
			}
			let calls = mul(n, add(source.entries, 1)?)?;
			let work = add(
				source.prepare_work,
				add(
					mul(n, add(source.query_work, mul(slots, 4096)?)?)?,
					mul(mul(parts, parts)?, mul(n, 16384)?)?,
				)?,
			)?;
			let transport = add(
				source.prepare_transport,
				add(
					mul(calls, force.total_transport_bytes)?,
					mul(
						mul(n, add(source.entries, source.grid.axes())?)?,
						mul(mul(parts, parts)?, 4096)?,
					)?,
				)?,
			)?;
			let local = mul(mul(range.len(), slots)?, 40)?;
			let global = mul(mul(n, slots)?, 40)?;
			let disk_node = mul(mul(n.div_ceil(parts), slots)?, mul(node, 40)?)?;
			if calls > l.max_drift_calls
				|| work > l.max_rank_work
				|| mul(work, parts)? > l.max_aggregate_work
				|| transport > l.max_transport_bytes
				|| local > l.max_local_disk_bytes
				|| global > l.max_global_disk_bytes
				|| disk_node > l.max_node_disk_bytes
			{
				return Err(CfdError::InvalidInput(
					"box history complete construction budget",
				));
			}
			Ok((calls, work, transport, local, global, disk_node))
		})(),
	)?;
	let mut store = FileRunStore::new(l.scratch_directory.clone());
	let spool_bytes = agree(env, add(store.descriptor_bytes(), 8192))?;
	let reservation = env.reserve_external_bytes(spool_bytes).map_err(native)?;
	// Force admission includes all previously live source, RHS and backend guards.
	let force = source.force.admit_drift().map_err(native)?;
	let local_peak = agree(
		env,
		add(
			force.maximum_rank_peak_bytes,
			recipe.resources().peak_managed_bytes,
		),
	)?;
	let mut rank_peak = maximum(env, local_peak)?;
	let mut node_peak = agree(env, mul(rank_peak, node))?;
	agree(
		env,
		if rank_peak <= l.max_local_bytes && node_peak <= l.max_node_bytes {
			Ok(())
		} else {
			Err(CfdError::InvalidInput("box history full query overlap"))
		},
	)?;
	let mut writer = agree(env, store.create().map_err(CfdError::from))?;
	let actual_file_bytes = agree(env, writer.retained_bytes().map_err(CfdError::from))?;
	let file_reservation = env
		.reserve_external_bytes(actual_file_bytes)
		.map_err(native)?;

	let mut hash = Sha256::new();
	let mut count = 0;
	for row in 0..n {
		// Agree allocation before any rank can begin the next physical row.
		let rows = agree(env, recipe.rows(row..row + 1))?;
		let actual_row_bytes = agree(env, rows.retained_bytes())?;
		let held_row = env
			.reserve_external_bytes(actual_row_bytes)
			.map_err(native)?;
		let row_force = source.force.admit_drift().map_err(native)?;
		let row_peak = maximum(env, row_force.maximum_rank_peak_bytes)?;
		rank_peak = rank_peak.max(row_peak);
		node_peak = mul(rank_peak, node)?;
		agree(
			env,
			if row_peak <= l.max_local_bytes && mul(row_peak, node)? <= l.max_node_bytes {
				Ok(())
			} else {
				Err(CfdError::InvalidInput("box history actual row capacity"))
			},
		)?;
		let mut error = None;
		for entry in rows {
			match entry {
				Ok(e) => {
					if range.contains(&row) && error.is_none() {
						if let Err(e) = write_entry(&mut store, &mut writer, e) {
							error = Some(CfdError::from(e));
						} else {
							hash_entry(&mut hash, e)?;
							count = add(count, 1)?;
						}
					}
				}
				Err(e) => {
					error = Some(e);
					break;
				}
			}
		}
		drop(held_row);
		// File failure never bypasses the current row's collective physics schedule.
		agree(env, error.map_or(Ok(()), Err))?;
	}
	let file = agree(env, store.finish(writer).map_err(CfdError::from))?;
	let reader = agree(env, store.open(&file).map_err(CfdError::from))?;
	let digest: [u8; 32] = hash.finalize().into();
	let mut identity = Sha256::new();
	for w in source.force.semantic_words().map_err(native)? {
		identity.update(w.to_le_bytes());
	}
	for w in recipe.semantic_words()? {
		identity.update(w.to_le_bytes());
	}
	for i in 0..source.grid.axis_dimension() {
		identity.update(
			source
				.grid
				.axis_node(i)
				.ok_or_else(overflow)?
				.to_bits()
				.to_le_bytes(),
		);
		identity.update(
			source
				.grid
				.axis_weight(i)
				.ok_or_else(overflow)?
				.to_bits()
				.to_le_bytes(),
		);
		for &(j, d) in source.grid.axis_derivative_row(i).ok_or_else(overflow)? {
			identity.update((word(j)?).to_le_bytes());
			identity.update(d.to_bits().to_le_bytes());
		}
	}
	let receipt = BoxKvnHistoryResources {
		physical_coordinates: source.grid.axes(),
		configuration_dimension: source.grid.dimension(),
		history_dimension: n,
		maximum_drift_calls: resources.0,
		actual_drift_calls: source.calls.get() - starting_calls,
		source_prepare_work: source.prepare_work,
		source_prepare_transport_bytes: source.prepare_transport,
		maximum_rank_work: resources.1,
		aggregate_work: mul(resources.1, parts)?,
		global_transport_bytes: resources.2,
		maximum_rank_peak_bytes: rank_peak,
		node_peak_bytes: node_peak,
		local_disk_ceiling: resources.3,
		global_disk_ceiling: resources.4,
		node_disk_ceiling: resources.5,
		local_entries: count,
		local_disk_bytes: mul(count, 40)?,
		recipe_metadata_sha256: identity.finalize().into(),
		local_spool_sha256: digest,
	};
	Ok((
		Spool {
			_file_reservation: file_reservation,
			reader,
			_file: file,
			_store: store,
			range,
			dimension: n,
			slots,
			count,
			seen: 0,
			expected: digest,
			hash: Sha256::new(),
			last: None,
			failed: false,
			_reservation: reservation,
		},
		receipt,
	))
}
/// Prepare the complete generated box `KvN` history inverse, retaining every physical coordinate.
///
/// All ranks call together with common physical/grid/time/spectral parameters and
/// compatible policies. Only construction uses globally synchronized rows; the
/// matching producer reads disjoint immutable local files without MPI callbacks.
/// Zero RHS skips construction. The returned inverse is reusable without rebuilding
/// physical samples or files. No coherent evaluation oracle is claimed here.
/// # Errors
/// Collectively rejects owner/shape/policy disagreement, checked work/transport,
/// rank/node memory/disk ceilings, file failure, or downstream inverse admission.
#[allow(
	clippy::too_many_arguments,
	reason = "Explicit independent physical, temporal, spectral and stage premises"
)]
pub fn prepare_box_kvn_history_inverse<'env, 'comm, 'runtime>(
	env: &'env CollectiveEnvironment<'comm, 'runtime>,
	force: &PreparedBoxForce<'_, '_, '_, 'env, 'comm, 'runtime>,
	grid: &ConfigurationGrid,
	horizon: f64,
	time_cells: usize,
	time_order: usize,
	initial: impl FnMut(usize) -> Result<Complex64, CfdError>,
	spectrum: &SpectralBounds,
	source_limits: &BoxKvnHistoryLimits,
	history_limits: DistributedHistoryLimits,
) -> Result<BoxKvnHistoryPreparation<'env, 'comm, 'runtime>, CfdError> {
	prepare_force_history_inverse(
		env,
		force,
		grid,
		horizon,
		time_cells,
		time_order,
		initial,
		spectrum,
		source_limits,
		history_limits,
	)
}
/// Prepare the complete nonautonomous `KvN` history from validated owned field shards.
/// Physical forcing changes the drift; it is not an additive amplitude source.
/// # Errors
/// Rejects uncovered time horizon, common metadata, all source/spool/inverse budgets.
#[allow(
	clippy::too_many_arguments,
	reason = "Independent physical, temporal, spectral and stage premises"
)]
pub fn prepare_time_box_kvn_history_inverse<'env, 'comm, 'runtime>(
	env: &'env CollectiveEnvironment<'comm, 'runtime>,
	force: &PreparedTimeBoxForce<'_, '_, '_, '_, 'env, 'comm, 'runtime>,
	grid: &ConfigurationGrid,
	horizon: f64,
	time_cells: usize,
	time_order: usize,
	initial: impl FnMut(usize) -> Result<Complex64, CfdError>,
	spectrum: &SpectralBounds,
	source_limits: &BoxKvnHistoryLimits,
	history_limits: DistributedHistoryLimits,
) -> Result<BoxKvnHistoryPreparation<'env, 'comm, 'runtime>, CfdError> {
	prepare_force_history_inverse(
		env,
		force,
		grid,
		horizon,
		time_cells,
		time_order,
		initial,
		spectrum,
		source_limits,
		history_limits,
	)
}
#[allow(
	clippy::too_many_arguments,
	reason = "Shared sealed complete force history dispatch"
)]
fn prepare_force_history_inverse<'env, 'comm, 'runtime>(
	env: &'env CollectiveEnvironment<'comm, 'runtime>,
	force: &dyn CollectiveForce<'env, 'comm, 'runtime>,
	grid: &ConfigurationGrid,
	horizon: f64,
	time_cells: usize,
	time_order: usize,
	initial: impl FnMut(usize) -> Result<Complex64, CfdError>,
	spectrum: &SpectralBounds,
	source_limits: &BoxKvnHistoryLimits,
	history_limits: DistributedHistoryLimits,
) -> Result<BoxKvnHistoryPreparation<'env, 'comm, 'runtime>, CfdError> {
	agree(
		env,
		if force
			.time_interval()
			.is_none_or(|interval| interval[0] <= 0. && interval[1] >= horizon)
		{
			Ok(())
		} else {
			Err(CfdError::InvalidInput(
				"physical time source does not cover history horizon",
			))
		},
	)?;
	let source = CollectiveDynamics::new(env, force, grid, source_limits)?;
	let recipe = agree(
		env,
		TemporalHistoryRecipe::new(
			&source,
			horizon,
			time_cells,
			time_order,
			HistoryStreamLimits {
				max_dimension: source_limits.max_dimension,
				max_bytes: source_limits.max_local_bytes,
				max_work_per_visit: source_limits.max_rank_work,
			},
		),
	)?;
	let construction = Cell::new(None);
	let inverse = prepare_history_inverse_from_factory(
		env,
		&recipe,
		initial,
		spectrum,
		history_limits,
		|range| {
			let (spool, receipt) = build_spool(env, &source, &recipe, range, source_limits)?;
			construction.set(Some(receipt));
			Ok(spool)
		},
	)?;
	Ok(BoxKvnHistoryPreparation {
		inverse,
		construction: construction.get(),
	})
}
#[cfg(test)]
#[path = "box_kvn_history_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "box_time_history_tests.rs"]
mod time_tests;
