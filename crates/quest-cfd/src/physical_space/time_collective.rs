//! Collectively validated complete owned polynomial-time physical fields.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::too_many_lines,
	clippy::suboptimal_flops,
	reason = "Checked complete-cell ranges, finite fixed payloads and common collective schedules"
)]
use super::force_collective::{agree, common, maximum, owner, peak};
use super::{
	BoxConstraintRecipe, BoxForceRecipe, BoxTimeDataShard, DistributedForceLimits,
	DistributedPhysicalPressure, DistributedPressureLimits, PreparedBoxConstraints,
	PreparedBoxForce,
};
use quest::{
	Error, Result,
	collective::{CollectiveEnvironment, CollectiveReservation},
	distributed_constraints::ChartVector,
};
use std::ops::Range;
fn add(a: usize, b: usize) -> Result<usize> {
	a.checked_add(b).ok_or(Error::Overflow)
}
fn mul(a: usize, b: usize) -> Result<usize> {
	a.checked_mul(b).ok_or(Error::Overflow)
}
fn native(_: crate::CfdError) -> Error {
	Error::Value("generated polynomial-time physical source")
}
fn buffer(n: usize) -> Result<Vec<f64>> {
	let mut v = Vec::new();
	v.try_reserve_exact(n).map_err(|_| Error::Allocation)?;
	v.resize(n, 0.);
	Ok(v)
}
/// Complete per-query admission, maximum-rank arithmetic and aggregate job ceiling.
#[derive(Clone, Copy, Debug)]
pub struct DistributedTimeForceResources {
	pub maximum_rank_work: usize,
	pub aggregate_work_ceiling: usize,
	pub global_transport_bytes: usize,
	pub retained_source_bytes: usize,
	pub local_output_bytes: usize,
	pub local_scratch_bytes: usize,
	pub maximum_rank_peak_bytes: usize,
	pub node_peak_bytes: usize,
}
/// Original physical force and effective force `f-M ell_dot` are retained separately.
pub struct DistributedTimeBoxDrift<'env> {
	momentum: Vec<f64>,
	effective: Vec<f64>,
	derivative: Vec<f64>,
	range: Range<usize>,
	drift: ChartVector<'env>,
	source_id: u64,
	chart_identity: usize,
	query_identity: u64,
	time: f64,
	pub resources: DistributedTimeForceResources,
	_reservation: CollectiveReservation<'env>,
}
impl DistributedTimeBoxDrift<'_> {
	#[must_use]
	pub fn local_momentum_force(&self) -> &[f64] {
		&self.momentum
	}
	#[must_use]
	pub fn local_effective_force(&self) -> &[f64] {
		&self.effective
	}
	#[must_use]
	pub fn local_lifting_derivative(&self) -> &[f64] {
		&self.derivative
	}
	#[must_use]
	pub fn force_range(&self) -> Range<usize> {
		self.range.clone()
	}
	#[must_use]
	pub fn as_slice(&self) -> &[f64] {
		self.drift.as_slice()
	}
	#[must_use]
	pub fn global_range(&self) -> Range<usize> {
		self.drift.global_range()
	}
	#[must_use]
	pub const fn time(&self) -> f64 {
		self.time
	}
}
/// Physical pressure with an independently recomputed original momentum residual.
pub struct DistributedTimePressure<'env> {
	pub pressure: DistributedPhysicalPressure<'env>,
	pub original_momentum_residual: f64,
	pub original_relative_momentum_residual: f64,
	pub total_query_work: usize,
	pub total_transport_bytes: usize,
}
/// Prepared full source bound to the exact distributed chart, with a retained guard.
///
/// The numerical coefficient checks retain all coordinates and supplied homogeneous
/// lifting components. Source IDs are provenance, not a constraint certificate.
pub struct PreparedTimeBoxForce<'data, 'force, 'owner, 'source, 'env, 'comm, 'runtime> {
	base: PreparedBoxForce<'force, 'owner, 'source, 'env, 'comm, 'runtime>,
	prepared: &'owner PreparedBoxConstraints<'source, 'env, 'comm, 'runtime>,
	recipe: &'force BoxForceRecipe<'source>,
	data: &'data BoxTimeDataShard<'source>,
	limits: DistributedForceLimits,
	source_id: u64,
	pub maximum_coefficient_defect: f64,
	/// Numerical sum-of-powers amplification; not an outward error certificate.
	pub interval_constraint_defect_envelope: f64,
	pub maximum_coefficient_net_flux_defect: f64,
	pub prepare_maximum_rank_work: usize,
	pub prepare_aggregate_work_ceiling: usize,
	pub prepare_global_transport_bytes: usize,
	_reservation: CollectiveReservation<'env>,
}
fn digest_word(hash: &mut u64, word: u64) {
	for b in word.to_le_bytes() {
		*hash = (*hash ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
	}
}
// Fixed complete-cell halo. Every rank participates even when it owns no cell.
fn halo(
	env: &CollectiveEnvironment<'_, '_>,
	source: &BoxConstraintRecipe,
	range: Range<usize>,
	words: &[f64],
	mut visit: impl FnMut(usize, &[usize; 5], &[[f64; 30]; 5]) -> Result<()>,
) -> Result<()> {
	let local = source.local_velocity_dimension();
	let d = source.dimension();
	let parts = usize::try_from(env.size()?).map_err(|_| Error::Overflow)?;
	let rank = usize::try_from(env.rank()?).map_err(|_| Error::Overflow)?;
	let rounds = source.cell_count().div_ceil(parts);
	agree(
		env,
		if parts.is_power_of_two() && words.len() == mul(range.len(), local)? {
			Ok(())
		} else {
			Err(Error::Value("time halo shape"))
		},
	)?;
	let mut valid = true;
	let mut lane = env
		.communicator()
		.collective_lane()
		.map_err(|source| Error::Backend {
			operation: "time field halo",
			source,
		})?;
	for round in 0..rounds {
		let cell = (round < range.len()).then_some(range.start + round);
		let mut ids = [usize::MAX; 5];
		let mut blocks = [[0.; 30]; 5];
		if let Some(cell) = cell {
			ids[0] = cell;
			blocks[0][..local].copy_from_slice(&words[round * local..(round + 1) * local]);
		}
		for face in 0..=d {
			let neighbour = cell.and_then(|i| source.partner(i, face).map(|(other, _)| other));
			if let Some(other) = neighbour {
				ids[face + 1] = other;
				if owner(source.cell_count(), other, parts) == rank {
					let start = (other - range.start) * local;
					blocks[face + 1][..local].copy_from_slice(&words[start..start + local]);
				}
			}
			for xor in 1..parts {
				let peer = rank ^ xor;
				let wanted = neighbour.filter(|&i| owner(source.cell_count(), i, parts) == peer);
				let send = wanted
					.map_or(u64::MAX, |v| u64::try_from(v).unwrap_or(u64::MAX))
					.to_le_bytes();
				let mut received = [0; 8];
				let peer = i32::try_from(peer).map_err(|_| Error::Overflow)?;
				let len = lane
					.send_receive_bytes(&send, peer, 121, &mut received)
					.map_err(|source| Error::Backend {
						operation: "time halo request",
						source,
					})?;
				let requested = u64::from_le_bytes(received);
				let index = usize::try_from(requested).ok();
				let good = len == 8
					&& (requested == u64::MAX || index.is_some_and(|i| range.contains(&i)));
				valid &= good;
				let mut reply = [0; 248];
				reply[..8].copy_from_slice(&received);
				if good
					&& requested != u64::MAX
					&& let Some(index) = index
				{
					let offset = (index - range.start) * local;
					for (slot, &v) in reply[8..]
						.as_chunks_mut::<8>()
						.0
						.iter_mut()
						.zip(&words[offset..offset + local])
					{
						slot.copy_from_slice(&v.to_le_bytes());
					}
				}
				let mut answer = [0; 248];
				let len = lane
					.send_receive_bytes(&reply, peer, 122, &mut answer)
					.map_err(|source| Error::Backend {
						operation: "time halo reply",
						source,
					})?;
				valid &= len == 248 && answer[..8] == send;
				for (i, b) in answer[8..].as_chunks::<8>().0.iter().enumerate() {
					let v = f64::from_le_bytes(*b);
					valid &= v.is_finite();
					if wanted.is_some() {
						blocks[face + 1][i] = v;
					}
				}
			}
		}
		if let Some(cell) = cell {
			valid &= visit(cell, &ids, &blocks).is_ok();
		}
	}
	if !lane.all_agree(valid).map_err(|source| Error::Backend {
		operation: "time halo completion",
		source,
	})? {
		return Err(Error::Value(
			"time coefficient halo or compatibility failure",
		));
	}
	Ok(())
}
impl<'source, 'env, 'comm, 'runtime> PreparedBoxConstraints<'source, 'env, 'comm, 'runtime> {
	/// Validate every coefficient's complete divergence and prescribed normal traces.
	/// # Errors
	/// Collectively rejects source/ownership mismatch, incompatibility and all budgets.
	pub fn prepare_time_force<'data, 'force, 'owner>(
		&'owner self,
		recipe: &'force BoxForceRecipe<'source>,
		data: &'data BoxTimeDataShard<'source>,
		limits: DistributedForceLimits,
	) -> Result<PreparedTimeBoxForce<'data, 'force, 'owner, 'source, 'env, 'comm, 'runtime>> {
		let env = self.environment();
		let rows = self.chart().local_row_range();
		let n = self.source().local_velocity_dimension();
		let parts = usize::try_from(env.size()?).map_err(|_| Error::Overflow)?;
		let k = data.degree() + 1;
		agree(
			env,
			if std::ptr::eq(data.source(), self.source())
				&& data.cell_range() == (rows.start / n..rows.end / n)
			{
				Ok(())
			} else {
				Err(Error::Value("time source physical ownership"))
			},
		)?;
		common(
			env,
			&[
				0x5449_4d45_5052_4501,
				u64::try_from(k).map_err(|_| Error::Overflow)?,
				data.time_interval()[0].to_bits(),
				data.time_interval()[1].to_bits(),
				data.compatibility_tolerance().to_bits(),
			],
		)?;

		let (bytes, prepare_work, transport, scratch) = agree(
			env,
			(|| {
				let rounds = self.source().cell_count().div_ceil(parts);
				let routes = mul(mul(rounds, self.source().dimension() + 1)?, parts - 1)?;
				let controls = mul(mul(parts, parts)?, 65536)?;
				let validation = mul(
					k,
					add(
						mul(rounds, data.resources().cell_query_work)?,
						add(mul(routes, 4096)?, controls)?,
					)?,
				)?;
				let work = add(
					add(
						data.resources().prepare_work,
						BoxForceRecipe::construction_work(),
					)?,
					validation,
				)?;
				let transport = mul(k, add(mul(mul(routes, parts)?, 256)?, controls)?)?;
				let bytes = add(
					data.resources().retained_bytes,
					size_of::<PreparedTimeBoxForce<'_, '_, '_, '_, '_, '_, '_>>(),
				)?;
				let scratch = add(mul(rows.len(), 8)?, data.resources().query_scratch_bytes)?;
				if work > limits.max_prepare_work || transport > limits.max_transport_bytes {
					return Err(Error::Value("time preparation work/transport"));
				}
				Ok((bytes, work, transport, scratch))
			})(),
		)?;
		peak(env, limits, agree(env, add(bytes, scratch))?)?;
		let reservation = env.reserve_external_bytes(bytes)?;
		let base = self.prepare_force(recipe, limits)?;
		let scratch_guard = env.reserve_external_bytes(scratch)?;
		let mut words = agree(env, buffer(rows.len()))?;
		let actual = agree(env, mul(words.capacity(), 8))?;
		let extra = actual.saturating_sub(rows.len() * 8);
		peak(env, limits, extra)?;
		let actual_guard = env.reserve_external_bytes(extra)?;
		let mut max_defect = 0_f64;
		let mut local_flux = [0_f64; 9];
		let mut hash = 0xcbf2_9ce4_8422_2325;
		for (power, flux) in local_flux.iter_mut().enumerate().take(k) {
			for cell in data.cell_range() {
				let c = data
					.coefficient(power, cell)
					.ok_or(Error::Value("owned time coefficient"))?;
				let offset = (cell - data.cell_range().start) * n;
				words[offset..offset + n].copy_from_slice(&c.lifting[..n]);
				for (face, prescribed) in c
					.prescribed
					.iter()
					.enumerate()
					.take(self.source().dimension() + 1)
				{
					if self
						.source()
						.facet_is_exterior(cell, face)
						.map_err(native)?
					{
						let normal = self
							.source()
							.normal(cell % self.source().permutation_count(), face);
						for mode in 0..self.source().facet_mode_count() {
							let node = self
								.source()
								.facet_velocity_node(face, mode)
								.map_err(native)?;
							let column = (cell * (self.source().dimension() + 1) + face)
								* self.source().facet_mode_count()
								+ mode;
							let weight = self
								.source()
								.pressure_gauge_coefficient(column)
								.map_err(native)?;
							for (axis, &normal) in
								normal.iter().enumerate().take(self.source().dimension())
							{
								*flux += weight
									* normal
									* prescribed[axis * (n / self.source().dimension()) + node];
							}
						}
					}
				}
				digest_word(&mut hash, u64::try_from(cell).map_err(|_| Error::Overflow)?);
				for v in c
					.lifting
					.iter()
					.chain(&c.body_force)
					.chain(c.prescribed.iter().flatten())
				{
					digest_word(&mut hash, v.to_bits());
				}
			}
			halo(
				env,
				self.source(),
				data.cell_range(),
				&words,
				|cell, ids, blocks| {
					let defect = data
						.validate_cell(power, cell, &mut |other| {
							ids.iter()
								.position(|&id| id == other)
								.map(|i| blocks[i])
								.ok_or(crate::CfdError::InvalidInput("time lifting halo"))
						})
						.map_err(native)?;
					max_defect = max_defect.max(defect);
					Ok(())
				},
			)?;
		}
		let mut global_hash = 0xcbf2_9ce4_8422_2325;
		let mut global_defect = 0_f64;
		let mut global_flux = [0_f64; 9];
		{
			let mut lane =
				env.communicator()
					.collective_lane()
					.map_err(|source| Error::Backend {
						operation: "time source identity",
						source,
					})?;
			for peer in 0..env.size()? {
				let mut frame = [0; 88];
				frame[..8].copy_from_slice(&hash.to_le_bytes());
				frame[8..16].copy_from_slice(&max_defect.to_le_bytes());
				for (slot, &flux) in frame[16..]
					.as_chunks_mut::<8>()
					.0
					.iter_mut()
					.zip(&local_flux)
				{
					slot.copy_from_slice(&flux.to_le_bytes());
				}
				lane.broadcast_bytes(peer, &mut frame)
					.map_err(|source| Error::Backend {
						operation: "time source digest",
						source,
					})?;
				digest_word(
					&mut global_hash,
					u64::from_le_bytes(frame[..8].try_into().map_err(|_| Error::Overflow)?),
				);
				global_defect = global_defect.max(f64::from_le_bytes(
					frame[8..16].try_into().map_err(|_| Error::Overflow)?,
				));
				for (sum, word) in global_flux.iter_mut().zip(frame[16..].as_chunks::<8>().0) {
					*sum += f64::from_le_bytes(*word);
				}
			}
		}
		for word in [
			u64::try_from(k).map_err(|_| Error::Overflow)?,
			data.time_interval()[0].to_bits(),
			data.time_interval()[1].to_bits(),
		] {
			digest_word(&mut global_hash, word);
		}
		for word in base.semantic_words()? {
			digest_word(&mut global_hash, word);
		}
		let (amplification, flux_defect) = agree(
			env,
			(|| {
				let time = data.time_interval()[0]
					.abs()
					.max(data.time_interval()[1].abs());
				let mut power = 1.;
				let mut amplification = 0.;
				for _ in 0..k {
					amplification += power * global_defect;
					power *= time;
				}
				let flux = global_flux
					.iter()
					.copied()
					.map(f64::abs)
					.fold(0_f64, f64::max);
				if !amplification.is_finite() || global_flux.iter().any(|x| !x.is_finite()) {
					return Err(Error::Value("time constraint interval/flux overflow"));
				}
				Ok((amplification, flux))
			})(),
		)?;
		let maximum_work = maximum(env, prepare_work)?;
		let aggregate = agree(env, mul(maximum_work, parts))?;
		drop(words);
		drop(actual_guard);
		drop(scratch_guard);
		Ok(PreparedTimeBoxForce {
			base,
			prepared: self,
			recipe,
			data,
			limits,
			source_id: global_hash,
			maximum_coefficient_defect: global_defect,
			interval_constraint_defect_envelope: amplification,
			maximum_coefficient_net_flux_defect: flux_defect,
			prepare_maximum_rank_work: maximum_work,
			prepare_aggregate_work_ceiling: aggregate,
			prepare_global_transport_bytes: transport,
			_reservation: reservation,
		})
	}
}
impl<'env, 'comm, 'runtime> PreparedTimeBoxForce<'_, '_, '_, '_, 'env, 'comm, 'runtime> {
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.base.dimension()
	}
	#[must_use]
	pub fn local_coordinate_range(&self) -> Range<usize> {
		self.base.local_coordinate_range()
	}
	/// Native chart owner of a full coordinate.
	/// # Errors
	/// Rejects invalid coordinates.
	pub fn coordinate_owner(&self, coordinate: usize) -> Result<i32> {
		self.base.coordinate_owner(coordinate)
	}
	#[must_use]
	pub const fn source_identity(&self) -> u64 {
		self.source_id
	}
	#[must_use]
	pub const fn time_interval(&self) -> [f64; 2] {
		self.data.time_interval()
	}
	pub(crate) const fn environment(&self) -> &'env CollectiveEnvironment<'comm, 'runtime> {
		self.prepared.environment()
	}
	pub(crate) fn semantic_words(&self) -> Result<[u64; 13]> {
		let mut words = [0; 13];
		words[..9].copy_from_slice(&self.base.semantic_words()?);
		words[9] = self.source_id;
		words[10] = u64::try_from(self.data.degree()).map_err(|_| Error::Overflow)?;
		words[11] = self.data.time_interval()[0].to_bits();
		words[12] = self.data.time_interval()[1].to_bits();
		Ok(words)
	}
	/// Complete per-query work and simultaneous storage admission without allocation.
	/// # Errors
	/// Collectively rejects budgets, shape arithmetic and rank/node capacity.
	pub fn admit_drift(&self) -> Result<DistributedTimeForceResources> {
		let env = self.environment();
		let base = self.base.admit_drift()?;
		let rows = self.prepared.chart().local_row_range();
		let parts = usize::try_from(env.size()?).map_err(|_| Error::Overflow)?;
		let (work, aggregate, transport, output, scratch) = agree(
			env,
			(|| {
				let rounds = self.data.source().cell_count().div_ceil(parts);
				// Byte-wise state hashing plus rank-ordered digest broadcasts. Null
				// ownership is a tail of the complete physical-row partition.
				let maximum_rows = mul(rounds, self.data.source().local_velocity_dimension())?;
				let digest_work = mul(add(add(maximum_rows, parts)?, 4)?, 128)?;
				let controls = add(mul(mul(parts, parts)?, 65536)?, digest_work)?;
				let work = add(
					base.total_work,
					add(
						mul(mul(rounds, 3)?, self.data.resources().cell_query_work)?,
						controls,
					)?,
				)?;
				let aggregate = mul(work, parts)?;
				let transport = add(base.total_transport_bytes, controls)?;
				let output = add(
					mul(rows.len(), 24)?,
					size_of::<DistributedTimeBoxDrift<'env>>(),
				)?;
				let scratch = add(
					mul(
						add(rows.len(), self.prepared.chart().local_null_range().len())?,
						8,
					)?,
					self.data.resources().query_scratch_bytes,
				)?;
				if work > self.limits.max_work || transport > self.limits.max_transport_bytes {
					return Err(Error::Value("time drift work/transport"));
				}
				Ok((work, aggregate, transport, output, scratch))
			})(),
		)?;
		let native_resources = self.prepared.chart().resources();
		let bytes = agree(
			env,
			add(
				add(output, scratch)?,
				mul(native_resources.query_peak_bytes, 2)?,
			),
		)?;
		let (rank, node) = peak(env, self.limits, bytes)?;
		Ok(DistributedTimeForceResources {
			maximum_rank_work: work,
			aggregate_work_ceiling: aggregate,
			global_transport_bytes: transport,
			retained_source_bytes: agree(
				env,
				add(
					self.data.resources().retained_bytes,
					base.source_retained_bytes,
				),
			)?,
			local_output_bytes: output,
			local_scratch_bytes: scratch,
			maximum_rank_peak_bytes: rank,
			node_peak_bytes: node,
		})
	}
	/// Evaluate the complete nonautonomous physical drift at one common exact time.
	/// # Errors
	/// Collective finite/domain/metadata/budget rejection precedes the fixed halo.
	pub fn drift_at(&self, time: f64, local_null: &[f64]) -> Result<DistributedTimeBoxDrift<'env>> {
		let env = self.environment();
		let chart = self.prepared.chart();
		let source = self.data.source();
		let n = source.local_velocity_dimension();
		let rows = chart.local_row_range();
		common(
			env,
			&[0x5449_4d45_4452_4901, self.source_id, time.to_bits()],
		)?;
		agree(
			env,
			if time.is_finite()
				&& time >= self.data.time_interval()[0]
				&& time <= self.data.time_interval()[1]
				&& local_null.len() == chart.local_null_range().len()
				&& local_null.iter().all(|x| x.is_finite())
			{
				Ok(())
			} else {
				Err(Error::Value("time drift time/state"))
			},
		)?;
		let mut resources = self.admit_drift()?;
		let query_identity = self.query_identity(time, local_null)?;
		let guard = env.reserve_external_bytes(resources.local_output_bytes)?;
		let scratch_guard = env.reserve_external_bytes(resources.local_scratch_bytes)?;
		let (mut momentum, mut effective, mut derivative, mut velocity) = agree(
			env,
			(|| {
				Ok((
					buffer(rows.len())?,
					buffer(rows.len())?,
					buffer(rows.len())?,
					buffer(rows.len())?,
				))
			})(),
		)?;
		let output = agree(
			env,
			add(
				mul(
					add(
						add(momentum.capacity(), effective.capacity())?,
						derivative.capacity(),
					)?,
					8,
				)?,
				size_of::<DistributedTimeBoxDrift<'env>>(),
			),
		)?;
		let scratch = agree(
			env,
			add(
				mul(velocity.capacity(), 8)?,
				self.data.resources().query_scratch_bytes,
			),
		)?;
		let extra = agree(
			env,
			add(
				output.saturating_sub(resources.local_output_bytes),
				scratch.saturating_sub(resources.local_scratch_bytes),
			),
		)?;
		peak(env, self.limits, extra)?;
		let extra_guard = env.reserve_external_bytes(extra)?;
		// Vec<f64> try_reserve_exact is accounted from actual capacities. Retain any
		// extra output capacity in the final owner by replacing the planned guard.
		drop(guard);
		let guard = env.reserve_external_bytes(output)?;
		resources.local_output_bytes = output;
		let lifted = chart.lift_null(local_null)?;
		agree(
			env,
			(|| {
				for cell in self.data.cell_range() {
					let data = self.data.evaluate_cell(time, cell).map_err(native)?;
					let offset = (cell - self.data.cell_range().start) * n;
					for i in 0..n {
						velocity[offset + i] = lifted.as_slice()[offset + i] + data.lifting[i];
						derivative[offset + i] = data.lifting_derivative[i];
						if !velocity[offset + i].is_finite() {
							return Err(Error::Value("time physical velocity overflow"));
						}
					}
				}
				Ok(())
			})(),
		)?;
		halo(
			env,
			source,
			self.data.cell_range(),
			&velocity,
			|cell, ids, blocks| {
				let data = self.data.evaluate_cell(time, cell).map_err(native)?;
				let force = self
					.recipe
					.cell_force_with_data(
						cell,
						&mut |other| {
							ids.iter()
								.position(|&id| id == other)
								.map(|i| blocks[i])
								.ok_or(crate::CfdError::InvalidInput("physical time force halo"))
						},
						&data.prescribed,
						&data.body_force,
					)
					.map_err(native)?;
				let offset = (cell - self.data.cell_range().start) * n;
				for i in 0..n {
					momentum[offset + i] = force[i];
					let mut load = 0.;
					for (j, &derivative) in data.lifting_derivative.iter().enumerate().take(n) {
						load += source.mass_value(cell, i, j).map_err(native)? * derivative;
					}
					effective[offset + i] = force[i] - load;
					if !effective[offset + i].is_finite() {
						return Err(Error::Value("time effective force overflow"));
					}
				}
				Ok(())
			},
		)?;
		drop(lifted);
		drop(velocity);
		drop(scratch_guard);
		drop(extra_guard);
		let drift = chart.project_force_to_null(&effective)?;
		Ok(DistributedTimeBoxDrift {
			momentum,
			effective,
			derivative,
			range: rows,
			drift,
			source_id: self.source_id,
			chart_identity: std::ptr::from_ref(self.prepared).addr(),
			query_identity,
			time,
			resources,
			_reservation: guard,
		})
	}
	// Noncryptographic provenance, not a numerical or collision-free certificate.
	// Equivalent repeated queries have equal identities; no generation counter.
	fn query_identity(&self, time: f64, local_null: &[f64]) -> Result<u64> {
		let env = self.environment();
		let mut local = 0xcbf2_9ce4_8422_2325;
		for value in local_null {
			digest_word(&mut local, value.to_bits());
		}
		let mut global = 0xcbf2_9ce4_8422_2325;
		for word in [self.source_id, time.to_bits()] {
			digest_word(&mut global, word);
		}
		let mut lane = env
			.communicator()
			.collective_lane()
			.map_err(|source| Error::Backend {
				operation: "time drift query identity",
				source,
			})?;
		for peer in 0..env.size()? {
			let mut frame = local.to_le_bytes();
			lane.broadcast_bytes(peer, &mut frame)
				.map_err(|source| Error::Backend {
					operation: "time drift state digest",
					source,
				})?;
			digest_word(&mut global, u64::from_le_bytes(frame));
		}
		Ok(global)
	}
	/// Gauge-correct physical pressure plus original f-M(Q a_dot+ell_dot)-C^T lambda check.
	/// # Errors
	/// Rejects foreign or mixed-time/state results, pressure budgets or failed original
	/// momentum residual. Query identity is noncryptographic provenance.
	pub fn recover_pressure(
		&self,
		result: &DistributedTimeBoxDrift<'env>,
		limits: DistributedPressureLimits,
	) -> Result<DistributedTimePressure<'env>> {
		let env = self.environment();
		let chart = self.prepared.chart();
		let rows = chart.local_row_range();
		let native_resources = chart.resources();
		common(
			env,
			&[
				0x5449_4d45_5052_4501,
				result.source_id,
				result.time.to_bits(),
				result.query_identity,
			],
		)?;
		agree(
			env,
			if result.source_id == self.source_id
				&& result.chart_identity == std::ptr::from_ref(self.prepared).addr()
				&& result.range == rows
			{
				Ok(())
			} else {
				Err(Error::Value("time pressure source identity"))
			},
		)?;
		let (bytes, work, transport) = agree(
			env,
			(|| {
				let parts = usize::try_from(env.size()?).map_err(|_| Error::Overflow)?;
				let control = mul(mul(parts, parts)?, 65536)?;
				let extra_work = add(
					mul(native_resources.query_work, 2)?,
					add(
						mul(
							mul(
								self.data.source().cell_count().div_ceil(parts),
								self.data.source().local_velocity_dimension(),
							)?,
							4096,
						)?,
						control,
					)?,
				)?;
				let bytes = add(
					mul(chart.local_constraint_range().len(), 8)?,
					mul(native_resources.query_peak_bytes, 2)?,
				)?;
				Ok((
					bytes,
					extra_work,
					add(mul(native_resources.query_transport_bytes, 2)?, control)?,
				))
			})(),
		)?;
		let reduced = agree(
			env,
			(|| {
				let mut reduced = limits;
				reduced.max_work = reduced
					.max_work
					.checked_sub(work)
					.ok_or(Error::Value("original pressure work"))?;
				reduced.max_transport_bytes = reduced
					.max_transport_bytes
					.checked_sub(transport)
					.ok_or(Error::Value("original pressure transport"))?;
				Ok(reduced)
			})(),
		)?;
		peak(env, self.limits, bytes)?;
		let guard = env.reserve_external_bytes(bytes)?;
		let pressure = self.prepared.recover_pressure(&result.effective, reduced)?;
		let mut lambda = agree(env, buffer(chart.local_constraint_range().len()))?;
		let actual = agree(env, mul(lambda.capacity(), 8))?;
		let extra = actual.saturating_sub(chart.local_constraint_range().len() * 8);
		peak(env, self.limits, extra)?;
		let extra_guard = env.reserve_external_bytes(extra)?;
		for (v, &a) in lambda.iter_mut().zip(
			pressure
				.normal_multipliers()
				.iter()
				.chain(pressure.pressure_coefficients()),
		) {
			*v = a;
		}
		for v in lambda.iter_mut().skip(pressure.normal_multipliers().len()) {
			*v = -*v;
		}
		let acceleration = chart.lift_null(result.as_slice())?;
		let constraint = chart.multiplier_force(&lambda)?;
		let n = self.data.source().local_velocity_dimension();
		let mut residual = 0_f64;
		let mut scale = 1_f64;
		agree(
			env,
			(|| {
				for (i, &force) in result.momentum.iter().enumerate() {
					let offset = (i / n) * n;
					let cell = (rows.start + i) / n;
					let mut mass = 0.;
					for j in 0..n {
						mass += self
							.data
							.source()
							.mass_value(cell, i % n, j)
							.map_err(native)?
							* (acceleration.as_slice()[offset + j] + result.derivative[offset + j]);
					}
					let r = (force - mass - constraint.as_slice()[i]).abs();
					if !r.is_finite() {
						return Err(Error::Value("original momentum residual overflow"));
					}
					residual = residual.max(r);
					scale = scale.max(force.abs());
				}
				Ok(())
			})(),
		)?;
		let mut global_r = 0_f64;
		let mut global_s = 1_f64;
		{
			let mut lane =
				env.communicator()
					.collective_lane()
					.map_err(|source| Error::Backend {
						operation: "original pressure residual",
						source,
					})?;
			for peer in 0..env.size()? {
				let mut frame = [0; 16];
				frame[..8].copy_from_slice(&residual.to_le_bytes());
				frame[8..].copy_from_slice(&scale.to_le_bytes());
				lane.broadcast_bytes(peer, &mut frame)
					.map_err(|source| Error::Backend {
						operation: "original pressure residual reduction",
						source,
					})?;
				global_r = global_r.max(f64::from_le_bytes(
					frame[..8].try_into().map_err(|_| Error::Overflow)?,
				));
				global_s = global_s.max(f64::from_le_bytes(
					frame[8..].try_into().map_err(|_| Error::Overflow)?,
				));
			}
		}
		agree(
			env,
			if global_r / global_s <= limits.relative_tolerance {
				Ok(())
			} else {
				Err(Error::Value("original momentum residual rejected"))
			},
		)?;
		let total_work = agree(env, add(work, pressure.resources.total_work))?;
		let total_transport = agree(
			env,
			add(transport, pressure.resources.total_transport_bytes),
		)?;
		drop(constraint);
		drop(acceleration);
		drop(lambda);
		drop(extra_guard);
		drop(guard);
		Ok(DistributedTimePressure {
			pressure,
			original_momentum_residual: global_r,
			original_relative_momentum_residual: global_r / global_s,
			total_query_work: total_work,
			total_transport_bytes: total_transport,
		})
	}
}
