//! Distributed physical pressure gauge recovery for the complete implicit box chart.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Checked complete local ranges and collective admission bound indexed algebra"
)]
use super::PreparedBoxConstraints;
use quest::{
	Error, Result,
	collective::{CollectiveEnvironment, CollectiveReservation},
};
use std::ops::Range;
/// Combined live rank/node, total four-query work, and transport admission.
#[derive(Clone, Copy, Debug)]
pub struct DistributedPressureLimits {
	pub max_local_bytes: usize,
	pub max_node_bytes: usize,
	pub ranks_per_node: usize,
	pub max_work: usize,
	pub max_transport_bytes: usize,
	pub relative_tolerance: f64,
}
impl Default for DistributedPressureLimits {
	fn default() -> Self {
		Self {
			max_local_bytes: 268_435_456,
			max_node_bytes: usize::MAX,
			ranks_per_node: usize::MAX,
			max_work: 1_000_000_000,
			max_transport_bytes: usize::MAX,
			relative_tolerance: 1e-8,
		}
	}
}
/// Conservative source and native-query costs; excludes MPI internals/allocator metadata.
#[derive(Clone, Copy, Debug)]
pub struct DistributedPressureResources {
	pub four_native_query_work: usize,
	pub source_work: usize,
	pub total_work: usize,
	pub total_transport_bytes: usize,
	pub local_external_bytes: usize,
	pub maximum_rank_peak_bytes: usize,
	pub node_peak_bytes: usize,
}
/// Owned local normal multipliers and physical pressure coefficients, with a live byte guard.
///
/// Pressure has sign p=-lambda_p and zero volume-weighted mean. Normal multipliers
/// include the compensating gauge shift. Residuals are floating numerical checks,
/// not rigorous error bounds. Global physical coordinates have not been truncated.
pub struct DistributedPhysicalPressure<'env> {
	values: Vec<f64>,
	range: Range<usize>,
	pressure_start: usize,
	normal_len: usize,
	pub removed_multiplier_mean: f64,
	pub momentum_residual: f64,
	pub relative_momentum_residual: f64,
	pub gauge_residual: f64,
	pub relative_gauge_residual: f64,
	pub resources: DistributedPressureResources,
	_reservation: CollectiveReservation<'env>,
}
impl DistributedPhysicalPressure<'_> {
	/// Original global normal-constraint range owned by this rank.
	#[must_use]
	pub fn normal_range(&self) -> Range<usize> {
		self.range.start.min(self.pressure_start)..self.range.end.min(self.pressure_start)
	}
	/// Cell-major pressure basis range, with pressure index zero at the first cell.
	#[must_use]
	pub const fn pressure_range(&self) -> Range<usize> {
		self.range.start.saturating_sub(self.pressure_start)
			..self.range.end.saturating_sub(self.pressure_start)
	}
	/// Gauge-compensated local normal multipliers.
	#[must_use]
	pub fn normal_multipliers(&self) -> &[f64] {
		&self.values[..self.normal_len]
	}
	/// Local physical P0/P1 coefficients, not generic QR multiplier signs.
	#[must_use]
	pub fn pressure_coefficients(&self) -> &[f64] {
		&self.values[self.normal_len..]
	}
}
fn agree<T>(env: &CollectiveEnvironment<'_, '_>, value: Result<T>) -> Result<T> {
	let mut lane = env
		.communicator()
		.collective_lane()
		.map_err(|source| Error::Backend {
			operation: "opening physical pressure agreement",
			source,
		})?;
	if !lane
		.all_agree(value.is_ok())
		.map_err(|source| Error::Backend {
			operation: "agreeing physical pressure admission",
			source,
		})? {
		return Err(Error::Value("collective physical pressure rejected"));
	}
	value
}
fn common(env: &CollectiveEnvironment<'_, '_>, limits: DistributedPressureLimits) -> Result<()> {
	let words = agree(
		env,
		(|| {
			Ok([
				u64::try_from(limits.max_local_bytes).map_err(|_| Error::Overflow)?,
				u64::try_from(limits.max_node_bytes).map_err(|_| Error::Overflow)?,
				u64::try_from(limits.ranks_per_node).map_err(|_| Error::Overflow)?,
				u64::try_from(limits.max_work).map_err(|_| Error::Overflow)?,
				u64::try_from(limits.max_transport_bytes).map_err(|_| Error::Overflow)?,
				limits.relative_tolerance.to_bits(),
			])
		})(),
	)?;
	let mut local = [0; 48];
	for (slot, word) in local.as_chunks_mut::<8>().0.iter_mut().zip(words) {
		slot.copy_from_slice(&word.to_le_bytes());
	}
	let mut remote = local;
	let mut lane = env
		.communicator()
		.collective_lane()
		.map_err(|source| Error::Backend {
			operation: "opening pressure metadata lane",
			source,
		})?;
	lane.broadcast_bytes(0, &mut remote)
		.map_err(|source| Error::Backend {
			operation: "broadcasting pressure metadata",
			source,
		})?;
	if !lane
		.all_agree(local == remote)
		.map_err(|source| Error::Backend {
			operation: "agreeing pressure metadata",
			source,
		})? {
		return Err(Error::Value("physical pressure metadata mismatch"));
	}
	Ok(())
}
fn stats(env: &CollectiveEnvironment<'_, '_>, local: [f64; 5]) -> Result<[f64; 5]> {
	agree(
		env,
		if local.iter().all(|v| v.is_finite()) {
			Ok(())
		} else {
			Err(Error::Value("nonfinite pressure reduction"))
		},
	)?;
	let mut result = [0.; 5];
	let mut lane = env
		.communicator()
		.collective_lane()
		.map_err(|source| Error::Backend {
			operation: "opening pressure reduction",
			source,
		})?;
	for peer in 0..env.size()? {
		let mut frame = [0; 40];
		for (slot, value) in frame.as_chunks_mut::<8>().0.iter_mut().zip(local) {
			slot.copy_from_slice(&value.to_le_bytes());
		}
		lane.broadcast_bytes(peer, &mut frame)
			.map_err(|source| Error::Backend {
				operation: "reducing physical pressure",
				source,
			})?;
		for (i, word) in frame.as_chunks::<8>().0.iter().enumerate() {
			let value = f64::from_le_bytes(*word);
			if i < 2 {
				result[i] += value;
			} else {
				result[i] = result[i].max(value);
			}
		}
	}
	drop(lane);
	agree(
		env,
		if result.iter().all(|v| v.is_finite()) {
			Ok(result)
		} else {
			Err(Error::Value("physical pressure reduction overflow"))
		},
	)
}
fn maximum_bytes(env: &CollectiveEnvironment<'_, '_>, local: usize) -> Result<usize> {
	let mut maximum = 0;
	let mut lane = env
		.communicator()
		.collective_lane()
		.map_err(|source| Error::Backend {
			operation: "opening pressure capacity lane",
			source,
		})?;
	for peer in 0..env.size()? {
		let mut frame = u64::try_from(local)
			.map_err(|_| Error::Overflow)?
			.to_le_bytes();
		lane.broadcast_bytes(peer, &mut frame)
			.map_err(|source| Error::Backend {
				operation: "reducing pressure capacity",
				source,
			})?;
		maximum =
			maximum.max(usize::try_from(u64::from_le_bytes(frame)).map_err(|_| Error::Overflow)?);
	}
	Ok(maximum)
}
impl<'env> PreparedBoxConstraints<'_, 'env, '_, '_> {
	/// Recover physical pressure from an arbitrary local broken momentum force.
	///
	/// All ranks enter in the same order. The input slice must follow complete-cell
	/// row ownership. Its accessible payload is charged here; unrelated backing
	/// storage retained by the caller remains the caller's accounting obligation.
	/// Four full native queries are charged, including nonlocal pressure coupling.
	/// If mean denotes the mean of `lambda_p`, `delta_lambda=mean*(facet_weights,-1)`
	/// centers p=-lambda_p while leaving the original constraint force unchanged.
	/// # Errors
	/// Collectively rejects invalid input/policy, resource limits, overflow, rank-query
	/// failures or a failed original momentum/gauge residual check.
	#[allow(
		clippy::too_many_lines,
		reason = "Keep collective admission, gauge compensation and original momentum verification together"
	)]
	pub fn recover_pressure(
		&self,
		local_force: &[f64],
		limits: DistributedPressureLimits,
	) -> Result<DistributedPhysicalPressure<'env>> {
		let env = self.environment();
		let chart = self.chart();
		let source = self.source();
		common(env, limits)?;
		let parts = usize::try_from(env.size()?).map_err(|_| Error::Overflow)?;
		let per_node = if limits.ranks_per_node == usize::MAX {
			parts
		} else {
			limits.ranks_per_node
		};
		let rows = chart.local_row_range();
		let columns = chart.local_constraint_range();
		agree(
			env,
			if local_force.len() == rows.len()
				&& local_force.iter().all(|v| v.is_finite())
				&& limits.relative_tolerance.is_finite()
				&& limits.relative_tolerance >= 0.
				&& per_node > 0
				&& per_node <= parts
			{
				Ok(())
			} else {
				Err(Error::Value("physical pressure force/policy"))
			},
		)?;
		let native = chart.resources();
		let mut resources = agree(
			env,
			(|| {
				let four_native_query_work =
					native.query_work.checked_mul(4).ok_or(Error::Overflow)?;
				let all_rows = source
					.cell_count()
					.checked_mul(source.local_velocity_dimension())
					.ok_or(Error::Overflow)?;
				let scalar_calls = all_rows
					.checked_mul(source.local_velocity_dimension())
					.and_then(|n| n.checked_add(source.constraint_count()))
					.ok_or(Error::Overflow)?;
				let control = parts
					.checked_mul(parts)
					.and_then(|n| n.checked_mul(1024))
					.ok_or(Error::Overflow)?;
				let source_work = scalar_calls
					.checked_mul(super::BoxConstraintRecipe::scalar_query_work())
					.and_then(|n| n.checked_add(control))
					.ok_or(Error::Overflow)?;
				let total_work = four_native_query_work
					.checked_add(source_work)
					.ok_or(Error::Overflow)?;
				let total_transport_bytes = native
					.query_transport_bytes
					.checked_mul(4)
					.and_then(|n| n.checked_add(control))
					.ok_or(Error::Overflow)?;
				let local_external_bytes = rows
					.len()
					.checked_add(columns.len())
					.and_then(|n| n.checked_mul(8))
					.and_then(|n| {
						n.checked_add(size_of::<DistributedPhysicalPressure<'env>>() + 8192)
					})
					.ok_or(Error::Overflow)?;
				let peak = native
					.query_peak_bytes
					.checked_mul(2)
					.and_then(|n| n.checked_add(local_external_bytes))
					.and_then(|n| n.checked_add(env.view().allocated_bytes()))
					.ok_or(Error::Overflow)?;
				if total_work > limits.max_work
					|| total_transport_bytes > limits.max_transport_bytes
				{
					return Err(Error::Value("physical pressure work/transport budget"));
				}
				Ok(DistributedPressureResources {
					four_native_query_work,
					source_work,
					total_work,
					total_transport_bytes,
					local_external_bytes,
					maximum_rank_peak_bytes: peak,
					node_peak_bytes: 0,
				})
			})(),
		)?;
		resources.maximum_rank_peak_bytes = maximum_bytes(env, resources.maximum_rank_peak_bytes)?;
		resources.node_peak_bytes = agree(
			env,
			resources
				.maximum_rank_peak_bytes
				.checked_mul(per_node)
				.ok_or(Error::Overflow),
		)?;
		agree(
			env,
			if resources.maximum_rank_peak_bytes <= limits.max_local_bytes
				&& resources.node_peak_bytes <= limits.max_node_bytes
			{
				Ok(())
			} else {
				Err(Error::Value("physical pressure live rank/node budget"))
			},
		)?;
		let reservation = env.reserve_external_bytes(resources.local_external_bytes)?;
		let mut multipliers = agree(
			env,
			(|| {
				let mut v = Vec::new();
				v.try_reserve_exact(columns.len())
					.map_err(|_| Error::Allocation)?;
				Ok(v)
			})(),
		)?;
		let coordinates = chart.project_force_to_null(local_force)?;
		let acceleration = chart.lift_null(coordinates.as_slice())?;
		drop(coordinates);
		let generic = chart.multipliers_from_force(local_force)?;
		multipliers.extend_from_slice(generic.values.as_slice());
		drop(generic);
		let pressure_start = source.pressure_constraint_start();
		let weight = source.pressure_integral_weight();
		let normal_len = columns
			.end
			.min(pressure_start)
			.saturating_sub(columns.start);
		let mut first = [0.; 5];
		for &lambda in &multipliers[normal_len..] {
			first[0] += lambda * weight;
			first[1] += weight;
		}
		let first = stats(env, first)?;
		let mean = first[0] / first[1];
		agree(
			env,
			if first[1] > 0. && mean.is_finite() {
				Ok(())
			} else {
				Err(Error::Value("physical pressure mean overflow"))
			},
		)?;
		agree(
			env,
			(|| {
				for (index, lambda) in multipliers.iter_mut().enumerate() {
					*lambda += mean
						* source
							.pressure_gauge_coefficient(columns.start + index)
							.map_err(|_| Error::Value("physical pressure gauge coefficient"))?;
					if !lambda.is_finite() {
						return Err(Error::Value("physical pressure gauge overflow"));
					}
				}
				Ok(())
			})(),
		)?;
		let constraint_force = chart.multiplier_force(&multipliers)?;
		let local = source.local_velocity_dimension();
		let final_stats = agree(
			env,
			(|| {
				let mut values = [0., 0., 0., 1., 1.];
				for &lambda in &multipliers[normal_len..] {
					values[0] += lambda * weight;
					values[4] = values[4].max(lambda.abs());
				}
				for (index, &force) in local_force.iter().enumerate() {
					let row = rows.start + index;
					let cell = row / local;
					let component = row % local;
					let start = index - component;
					let mut mass = 0.;
					for j in 0..local {
						mass += source
							.mass_value(cell, component, j)
							.map_err(|_| Error::Value("pressure cell mass"))?
							* acceleration.as_slice()[start + j];
					}
					let residual = (force - mass - constraint_force.as_slice()[index]).abs();
					if !mass.is_finite() || !residual.is_finite() {
						return Err(Error::Value("physical pressure momentum overflow"));
					}
					values[2] = values[2].max(residual);
					values[3] = values[3].max(force.abs());
				}
				Ok(values)
			})(),
		)?;
		let final_stats = stats(env, final_stats)?;
		let relative_momentum = final_stats[2] / final_stats[3];
		let relative_gauge = (final_stats[0] / first[1]).abs() / final_stats[4];
		agree(
			env,
			if relative_momentum <= limits.relative_tolerance
				&& relative_gauge <= limits.relative_tolerance
			{
				Ok(())
			} else {
				Err(Error::Value(
					"physical pressure momentum/gauge check failed",
				))
			},
		)?;
		drop(constraint_force);
		drop(acceleration);
		for lambda in &mut multipliers[normal_len..] {
			*lambda = -*lambda;
		}
		Ok(DistributedPhysicalPressure {
			values: multipliers,
			range: columns,
			pressure_start,
			normal_len,
			removed_multiplier_mean: mean,
			momentum_residual: final_stats[2],
			relative_momentum_residual: relative_momentum,
			gauge_residual: final_stats[0].abs(),
			relative_gauge_residual: relative_gauge,
			resources,
			_reservation: reservation,
		})
	}
}
