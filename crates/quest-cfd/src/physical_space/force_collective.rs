//! Standalone collectively scheduled full physical drift; never an independent `KvN` row callback.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Checked ownership and fixed coefficient frames bound each admitted routing/query phase"
)]
use super::{BoxForceRecipe, PreparedBoxConstraints};
use quest::{
	Error, Result,
	collective::{CollectiveEnvironment, CollectiveReservation},
	distributed_constraints::ChartVector,
};
use std::ops::Range;

/// Admission of preparation and each complete collectively scheduled physical drift.
#[derive(Clone, Copy, Debug)]
pub struct DistributedForceLimits {
	/// Maximum-rank arithmetic ceiling, including replicated source construction.
	pub max_prepare_work: usize,
	pub max_local_bytes: usize,
	pub max_node_bytes: usize,
	pub ranks_per_node: usize,
	/// Maximum-rank arithmetic ceiling per drift, not a rank-summed job budget.
	pub max_work: usize,
	/// Global directed payload ceiling per preparation or drift.
	pub max_transport_bytes: usize,
}
impl Default for DistributedForceLimits {
	fn default() -> Self {
		Self {
			max_prepare_work: 200_000_000,
			max_local_bytes: 268_435_456,
			max_node_bytes: usize::MAX,
			ranks_per_node: usize::MAX,
			max_work: 1_000_000_000,
			max_transport_bytes: usize::MAX,
		}
	}
}
/// Conservative full-query cost and actual external capacities, additional to the chart owner.
#[derive(Clone, Copy, Debug)]
pub struct DistributedForceResources {
	pub two_native_query_work: usize,
	/// Maximum-rank source arithmetic; native work uses its conservative global-size ceiling.
	pub source_work: usize,
	/// Conservative maximum-rank arithmetic ceiling, not aggregate work across ranks.
	pub total_work: usize,
	pub halo_transport_bytes: usize,
	pub total_transport_bytes: usize,
	pub halo_cell_rounds: usize,
	pub source_retained_bytes: usize,
	pub local_output_bytes: usize,
	pub local_query_scratch_bytes: usize,
	pub maximum_rank_peak_bytes: usize,
	pub node_peak_bytes: usize,
}
/// Complete rank-owned broken force and null-space acceleration with live byte guards.
pub struct DistributedBoxDrift<'env> {
	force: Vec<f64>,
	force_range: Range<usize>,
	drift: ChartVector<'env>,
	pub resources: DistributedForceResources,
	_reservation: CollectiveReservation<'env>,
}
impl DistributedBoxDrift<'_> {
	#[must_use]
	pub fn local_force(&self) -> &[f64] {
		&self.force
	}
	#[must_use]
	pub fn force_range(&self) -> Range<usize> {
		self.force_range.clone()
	}
	#[must_use]
	pub fn as_slice(&self) -> &[f64] {
		self.drift.as_slice()
	}
	#[must_use]
	pub fn global_range(&self) -> Range<usize> {
		self.drift.global_range()
	}
}
/// Immutable source/chart owner. The source reservation remains live across repeated queries.
pub struct PreparedBoxForce<'force, 'owner, 'source, 'env, 'comm, 'runtime> {
	recipe: &'force BoxForceRecipe<'source>,
	prepared: &'owner PreparedBoxConstraints<'source, 'env, 'comm, 'runtime>,
	limits: DistributedForceLimits,
	retained_bytes: usize,
	_reservation: CollectiveReservation<'env>,
}
fn add(a: usize, b: usize) -> Result<usize> {
	a.checked_add(b).ok_or(Error::Overflow)
}
fn mul(a: usize, b: usize) -> Result<usize> {
	a.checked_mul(b).ok_or(Error::Overflow)
}
pub(super) fn agree<T>(env: &CollectiveEnvironment<'_, '_>, value: Result<T>) -> Result<T> {
	let mut lane = env
		.communicator()
		.collective_lane()
		.map_err(|source| Error::Backend {
			operation: "opening full force agreement",
			source,
		})?;
	if !lane
		.all_agree(value.is_ok())
		.map_err(|source| Error::Backend {
			operation: "agreeing full force admission",
			source,
		})? {
		return Err(Error::Value("collective full physical force rejected"));
	}
	value
}
pub(super) fn common(env: &CollectiveEnvironment<'_, '_>, words: &[u64]) -> Result<()> {
	agree(
		env,
		if words.len() <= 20 {
			Ok(())
		} else {
			Err(Error::Value("physical force metadata frame capacity"))
		},
	)?;
	let mut frame = [0; 160];
	for (chunk, word) in frame.as_chunks_mut::<8>().0.iter_mut().zip(words) {
		chunk.copy_from_slice(&word.to_le_bytes());
	}
	let local = frame;
	let mut lane = env
		.communicator()
		.collective_lane()
		.map_err(|source| Error::Backend {
			operation: "opening full force metadata",
			source,
		})?;
	lane.broadcast_bytes(0, &mut frame)
		.map_err(|source| Error::Backend {
			operation: "broadcasting full force metadata",
			source,
		})?;
	if !lane
		.all_agree(local == frame)
		.map_err(|source| Error::Backend {
			operation: "agreeing full force metadata",
			source,
		})? {
		return Err(Error::Value("physical force metadata mismatch"));
	}
	Ok(())
}
pub(super) fn maximum(env: &CollectiveEnvironment<'_, '_>, value: usize) -> Result<usize> {
	let mut result = 0;
	let mut lane = env
		.communicator()
		.collective_lane()
		.map_err(|source| Error::Backend {
			operation: "opening full force capacity",
			source,
		})?;
	for peer in 0..env.size()? {
		let mut frame = u64::try_from(value)
			.map_err(|_| Error::Overflow)?
			.to_le_bytes();
		lane.broadcast_bytes(peer, &mut frame)
			.map_err(|source| Error::Backend {
				operation: "reducing full force capacity",
				source,
			})?;
		result =
			result.max(usize::try_from(u64::from_le_bytes(frame)).map_err(|_| Error::Overflow)?);
	}
	Ok(result)
}
pub(super) const fn owner(cells: usize, index: usize, parts: usize) -> usize {
	let base = cells / parts;
	let rem = cells % parts;
	let large = (base + 1) * rem;
	if index < large {
		index / (base + 1)
	} else {
		rem + (index - large) / base
	}
}
const fn node_size(limits: DistributedForceLimits, parts: usize) -> Result<usize> {
	let count = if limits.ranks_per_node == usize::MAX {
		parts
	} else {
		limits.ranks_per_node
	};
	if count == 0 || count > parts {
		return Err(Error::Value("physical force ranks per node"));
	}
	Ok(count)
}
pub(super) fn peak(
	env: &CollectiveEnvironment<'_, '_>,
	limits: DistributedForceLimits,
	extra: usize,
) -> Result<(usize, usize)> {
	let local = agree(env, add(env.view().allocated_bytes(), extra))?;
	let rank = maximum(env, local)?;
	let node = agree(
		env,
		mul(
			rank,
			node_size(
				limits,
				usize::try_from(env.size()?).map_err(|_| Error::Overflow)?,
			)?,
		),
	)?;
	agree(
		env,
		if rank <= limits.max_local_bytes && node <= limits.max_node_bytes {
			Ok((rank, node))
		} else {
			Err(Error::Value("physical force live rank/node budget"))
		},
	)
}
impl<'source, 'env, 'comm, 'runtime> PreparedBoxConstraints<'source, 'env, 'comm, 'runtime> {
	/// Bind a fixed numerical force source to this exact prepared constraint owner.
	/// All ranks enter in the same order; exact physical parameters and limits agree.
	/// The accounted force source must outlive this owner. Its borrowed geometry is
	/// already accounted by the prepared constraints; no global state is introduced.
	/// # Errors
	/// Collectively rejects wrong source ownership, parameter/policy disagreement,
	/// preparation work or live rank/node/environment capacity admission.
	pub fn prepare_force<'force, 'owner>(
		&'owner self,
		recipe: &'force BoxForceRecipe<'source>,
		limits: DistributedForceLimits,
	) -> Result<PreparedBoxForce<'force, 'owner, 'source, 'env, 'comm, 'runtime>> {
		let env = self.environment();
		let parts = usize::try_from(env.size()?).map_err(|_| Error::Overflow)?;
		agree(
			env,
			if parts.is_power_of_two() && std::ptr::eq(recipe.source(), self.source()) {
				node_size(limits, parts).map(|_| ())
			} else {
				Err(Error::Value("physical force uses a different source owner"))
			},
		)?;
		let parameters = recipe.parameters();
		let source = self.source();
		let words = agree(
			env,
			(|| {
				Ok([
					0x4346_4446_4f52_4301,
					u64::try_from(source.dimension()).map_err(|_| Error::Overflow)?,
					u64::try_from(source.order()).map_err(|_| Error::Overflow)?,
					u64::try_from(source.cell_count()).map_err(|_| Error::Overflow)?,
					source.extent().to_bits(),
					u64::from(source.periodic()),
					parameters[0].to_bits(),
					parameters[1].to_bits(),
					u64::try_from(limits.max_prepare_work).map_err(|_| Error::Overflow)?,
					u64::try_from(limits.max_local_bytes).map_err(|_| Error::Overflow)?,
					u64::try_from(limits.max_node_bytes).map_err(|_| Error::Overflow)?,
					u64::try_from(limits.ranks_per_node).map_err(|_| Error::Overflow)?,
					u64::try_from(limits.max_work).map_err(|_| Error::Overflow)?,
					u64::try_from(limits.max_transport_bytes).map_err(|_| Error::Overflow)?,
				])
			})(),
		)?;
		common(env, &words)?;
		let retained_bytes = agree(
			env,
			add(
				recipe.source_bytes(),
				size_of::<PreparedBoxForce<'force, 'owner, 'source, 'env, 'comm, 'runtime>>()
					+ 1024,
			),
		)?;
		let preparation_work = agree(
			env,
			add(
				BoxForceRecipe::construction_work(),
				mul(mul(parts, parts)?, 16384)?,
			),
		)?;
		let preparation_transport = agree(env, mul(mul(parts, parts)?, 16384))?;
		agree(
			env,
			if preparation_work <= limits.max_prepare_work
				&& preparation_transport <= limits.max_transport_bytes
			{
				Ok(())
			} else {
				Err(Error::Value(
					"physical force preparation work/transport budget",
				))
			},
		)?;
		peak(env, limits, retained_bytes)?;
		let reservation = env.reserve_external_bytes(retained_bytes)?;
		Ok(PreparedBoxForce {
			recipe,
			prepared: self,
			limits,
			retained_bytes,
			_reservation: reservation,
		})
	}
}
impl<'env, 'comm, 'runtime> PreparedBoxForce<'_, '_, '_, 'env, 'comm, 'runtime> {
	pub(crate) const fn environment(&self) -> &'env CollectiveEnvironment<'comm, 'runtime> {
		self.prepared.environment()
	}
	pub(crate) fn semantic_words(&self) -> Result<[u64; 9]> {
		let s = self.prepared.source();
		let p = self.recipe.parameters();
		let word = |n| u64::try_from(n).map_err(|_| Error::Overflow);
		Ok([
			0x424f_5846_4f52_4301,
			word(s.dimension())?,
			word(s.order())?,
			word(s.cell_count())?,
			s.extent().to_bits(),
			u64::from(s.periodic()),
			p[0].to_bits(),
			p[1].to_bits(),
			word(self.dimension())?,
		])
	}
	/// Full number of physical mass-orthonormal coordinates; no modes are omitted.
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.prepared.chart().nullity()
	}
	/// The actual complete-chart tail partition, which is not a balanced m/P partition.
	#[must_use]
	pub fn local_coordinate_range(&self) -> Range<usize> {
		self.prepared.chart().local_null_range()
	}
	/// Unique rank owning one full-chart null coordinate.
	/// # Errors
	/// Rejects invalid coordinates or unrepresentable MPI owner indices.
	pub fn coordinate_owner(&self, coordinate: usize) -> Result<i32> {
		if coordinate >= self.dimension() {
			return Err(Error::Value("physical null coordinate index"));
		}
		let parts =
			usize::try_from(self.prepared.environment().size()?).map_err(|_| Error::Overflow)?;
		let row = add(self.prepared.chart().numerical_rank(), coordinate)?;
		let source = self.prepared.source();
		i32::try_from(owner(
			source.cell_count(),
			row / source.local_velocity_dimension(),
			parts,
		))
		.map_err(|_| Error::Overflow)
	}
	/// Preflight one complete drift using the same cost formula as actual execution.
	/// All ranks enter together. Actual output capacity is rechecked during execution.
	/// # Errors
	/// Collectively rejects complete work/transport or live rank/node capacity admission.
	pub fn admit_drift(&self) -> Result<DistributedForceResources> {
		self.resources_with_output(add(
			mul(self.prepared.chart().local_row_range().len(), 8)?,
			size_of::<DistributedBoxDrift<'env>>(),
		)?)
	}
	fn resources_with_output(&self, output_bytes: usize) -> Result<DistributedForceResources> {
		let env = self.prepared.environment();
		let chart = self.prepared.chart();
		let source = self.prepared.source();
		let parts = usize::try_from(env.size()?).map_err(|_| Error::Overflow)?;
		let local = source.local_velocity_dimension();
		let d = source.dimension();
		let rounds = source.cell_count().div_ceil(parts);
		let native = chart.resources();
		let routes = agree(env, mul(mul(rounds, d + 1)?, parts - 1))?;
		let control = agree(env, mul(mul(parts, parts)?, 16384))?;
		let source_work = agree(
			env,
			(|| {
				add(
					add(
						mul(rounds, BoxForceRecipe::cell_query_work())?,
						mul(routes, 4096)?,
					)?,
					add(mul(mul(rounds, local)?, 1024)?, control)?,
				)
			})(),
		)?;
		let two_native_query_work = agree(env, mul(native.query_work, 2))?;
		let total_work = agree(env, add(two_native_query_work, source_work))?;
		let halo_transport_bytes = agree(env, mul(mul(routes, parts)?, 256))?;
		let total_transport_bytes = agree(
			env,
			add(
				add(mul(native.query_transport_bytes, 2)?, halo_transport_bytes)?,
				control,
			),
		)?;
		agree(
			env,
			if total_work <= self.limits.max_work
				&& total_transport_bytes <= self.limits.max_transport_bytes
			{
				Ok(())
			} else {
				Err(Error::Value(
					"complete physical force work/transport budget",
				))
			},
		)?;
		let local_query_scratch_bytes = agree(
			env,
			add(
				mul(chart.local_null_range().len(), 8)?,
				BoxForceRecipe::cell_scratch_bytes() + 8192,
			),
		)?;
		let local_output_bytes = output_bytes;
		let (maximum_rank_peak_bytes, node_peak_bytes) = peak(
			env,
			self.limits,
			add(
				add(local_output_bytes, local_query_scratch_bytes)?,
				native.query_peak_bytes,
			)?,
		)?;
		Ok(DistributedForceResources {
			two_native_query_work,
			source_work,
			total_work,
			halo_transport_bytes,
			total_transport_bytes,
			halo_cell_rounds: rounds,
			source_retained_bytes: self.retained_bytes,
			local_output_bytes,
			local_query_scratch_bytes,
			maximum_rank_peak_bytes,
			node_peak_bytes,
		})
	}

	/// Evaluate a complete physical drift in one collectively scheduled call.
	///
	/// Input/output are null-coordinate shards; only complete owned broken cells
	/// are lifted. Halo routing visits owned cell rounds (including idle empty
	/// ranks), every face slot and XOR peers in a fixed agreed sequence. Requests
	/// and replies have fixed sizes even when idle. MPI transport uses the lane's
	/// private context, distinct from native `QuEST` and metadata coordination.
	/// The lift is dropped before projection; both full native query costs remain
	/// charged. Caller-accessible input payload is charged here; unrelated backing
	/// capacity remains the caller's obligation.
	///
	/// This MUST NOT run inside independently advancing `KvN` row callbacks: all
	/// ranks must agree on each physical state and enter each call in the same order.
	/// # Errors
	/// Collectively rejects malformed input, complete work/transport/live-capacity
	/// admission, allocation, routing/query failure or nonfinite complete force.
	#[allow(
		clippy::too_many_lines,
		reason = "Keep whole-query admission, owned-cell routing and guarded output publication together"
	)]
	pub fn drift(&self, local_null: &[f64]) -> Result<DistributedBoxDrift<'env>> {
		let env = self.prepared.environment();
		let chart = self.prepared.chart();
		let source = self.prepared.source();
		let rows = chart.local_row_range();
		let parts = usize::try_from(env.size()?).map_err(|_| Error::Overflow)?;
		let rank = usize::try_from(env.rank()?).map_err(|_| Error::Overflow)?;
		let local = source.local_velocity_dimension();
		let cells = source.cell_count();
		let d = source.dimension();
		let rounds = cells.div_ceil(parts);
		agree(
			env,
			if parts.is_power_of_two()
				&& local_null.len() == chart.local_null_range().len()
				&& local_null.iter().all(|v| v.is_finite())
			{
				Ok(())
			} else {
				Err(Error::Value("physical force null input"))
			},
		)?;
		common(
			env,
			&[
				0x4346_4446_4f52_4302,
				u64::try_from(cells).map_err(|_| Error::Overflow)?,
				u64::try_from(local).map_err(|_| Error::Overflow)?,
			],
		)?;
		self.admit_drift()?;
		let mut force = agree(
			env,
			(|| {
				let mut words = Vec::new();
				words
					.try_reserve_exact(rows.len())
					.map_err(|_| Error::Allocation)?;
				words.resize(rows.len(), 0.);
				Ok(words)
			})(),
		)?;
		let local_output_bytes = agree(
			env,
			add(
				mul(force.capacity(), 8)?,
				size_of::<DistributedBoxDrift<'env>>(),
			),
		)?;
		let resources = self.resources_with_output(local_output_bytes)?;
		let local_query_scratch_bytes = resources.local_query_scratch_bytes;
		let output_reservation = env.reserve_external_bytes(local_output_bytes)?;
		let scratch_reservation = env.reserve_external_bytes(local_query_scratch_bytes)?;
		let velocity = chart.lift_null(local_null)?;
		let start = rows.start / local;
		let count = rows.len() / local;
		let mut valid = true;
		{
			let mut lane =
				env.communicator()
					.collective_lane()
					.map_err(|source| Error::Backend {
						operation: "opening physical force halo",
						source,
					})?;
			for round in 0..rounds {
				let cell = (round < count).then_some(start + round);
				let mut ids = [usize::MAX; 5];
				let mut blocks = [[0.; 30]; 5];
				if let Some(cell) = cell {
					ids[0] = cell;
					blocks[0][..local]
						.copy_from_slice(&velocity.as_slice()[round * local..(round + 1) * local]);
				}
				for face in 0..=d {
					let neighbour =
						cell.and_then(|cell| source.partner(cell, face).map(|(other, _)| other));
					if let Some(other) = neighbour {
						ids[face + 1] = other;
						if owner(cells, other, parts) == rank {
							let offset = (other - start) * local;
							blocks[face + 1][..local]
								.copy_from_slice(&velocity.as_slice()[offset..offset + local]);
						}
					}
					for xor in 1..parts {
						let peer = rank ^ xor;
						let wanted = neighbour.filter(|&other| owner(cells, other, parts) == peer);
						let send = wanted
							.map_or(u64::MAX, |cell| u64::try_from(cell).unwrap_or(u64::MAX))
							.to_le_bytes();
						let mut received = [0; 8];
						let peer_i32 = i32::try_from(peer).map_err(|_| Error::Overflow)?;
						let length = lane
							.send_receive_bytes(&send, peer_i32, 117, &mut received)
							.map_err(|source| Error::Backend {
								operation: "requesting neighbour force coefficients",
								source,
							})?;
						let requested = u64::from_le_bytes(received);
						let requested_index = usize::try_from(requested).ok();
						let request_valid = length == 8
							&& (requested == u64::MAX
								|| requested_index
									.is_some_and(|i| i >= start && i < start + count));
						valid &= request_valid;
						let mut reply = [0; 248];
						reply[..8].copy_from_slice(&received);
						if request_valid
							&& requested != u64::MAX
							&& let Some(index) = requested_index
						{
							let offset = (index - start) * local;
							for (slot, &value) in reply[8..]
								.as_chunks_mut::<8>()
								.0
								.iter_mut()
								.zip(&velocity.as_slice()[offset..offset + local])
							{
								slot.copy_from_slice(&value.to_le_bytes());
							}
						}
						let mut answer = [0; 248];
						let length = lane
							.send_receive_bytes(&reply, peer_i32, 118, &mut answer)
							.map_err(|source| Error::Backend {
								operation: "exchanging neighbour force coefficients",
								source,
							})?;
						let echoed = u64::from_le_bytes(
							answer[..8].try_into().map_err(|_| Error::Overflow)?,
						);
						valid &= length == 248 && echoed == u64::from_le_bytes(send);
						for (index, word) in answer[8..].as_chunks::<8>().0.iter().enumerate() {
							let value = f64::from_le_bytes(*word);
							valid &= value.is_finite();
							if wanted.is_some() {
								blocks[face + 1][index] = value;
							}
						}
					}
				}
				if let Some(cell) = cell {
					let result = self.recipe.cell_force(cell, &mut |wanted| {
						ids.iter()
							.position(|&id| id == wanted)
							.map(|slot| blocks[slot])
							.ok_or(crate::CfdError::InvalidInput("missing complete force halo"))
					});
					match result {
						Ok(words) => force[round * local..(round + 1) * local]
							.copy_from_slice(&words[..local]),
						Err(_) => valid = false,
					}
				}
			}
			if !lane.all_agree(valid).map_err(|source| Error::Backend {
				operation: "agreeing complete force halo/result",
				source,
			})? {
				return Err(Error::Value(
					"invalid neighbour routing or complete physical force",
				));
			}
		}
		drop(velocity);
		let drift = chart.project_force_to_null(&force)?;
		drop(scratch_reservation);
		Ok(DistributedBoxDrift {
			force,
			force_range: rows,
			drift,
			resources,
			_reservation: output_reservation,
		})
	}
}
