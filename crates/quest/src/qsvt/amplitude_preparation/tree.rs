#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Admitted power-of-two partitions, complete table sizes and packet lengths bound the local tree and butterfly indices"
)]
use super::{
	AmplitudeShard, Level,
	wire::{broadcast, float, put},
};
use crate::{Error, Result, error::BackendResult, values::reserve_vec};
use quest_sys::mpi::MpiCollectiveLane;

pub(super) fn zeroes(count: usize) -> Result<Vec<f64>> {
	let mut result = reserve_vec(count)?;
	result.resize(count, 0.);
	Ok(result)
}
fn digest(index: usize, re: u64, im: u64) -> u64 {
	let index = u64::try_from(index).unwrap_or(u64::MAX);
	index.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ re.rotate_left(17) ^ im.rotate_right(11)
}
pub(super) fn input_summary(
	lane: &mut MpiCollectiveLane<'_>,
	shard: &AmplitudeShard,
) -> Result<(f64, u64)> {
	let mut scale = 0.0_f64;
	let mut own = 0u64;
	for (offset, value) in shard.values.iter().enumerate() {
		scale = scale.max(value.re.abs()).max(value.im.abs());
		own = own.wrapping_add(digest(
			shard.start + offset,
			value.re.to_bits(),
			value.im.to_bits(),
		));
	}
	let mut maximum = 0.0_f64;
	let mut identity = 0u64;
	for peer in 0..shard.parts {
		let mut packet = [0; 16];
		packet[..8].copy_from_slice(&scale.to_le_bytes());
		packet[8..].copy_from_slice(&own.to_le_bytes());
		broadcast(lane, peer, &mut packet)?;
		maximum = maximum.max(float(&packet, 0)?);
		identity = identity.wrapping_add(u64::from_le_bytes(
			packet[8..].try_into().map_err(|_| Error::Overflow)?,
		));
	}
	Ok((
		maximum,
		identity
			^ u64::try_from(shard.logical)
				.map_err(|_| Error::Overflow)?
				.rotate_left(7),
	))
}
#[allow(
	clippy::too_many_arguments,
	clippy::too_many_lines,
	reason = "Tree, tables, and bounded exchange buffers share one ordered admitted collective protocol"
)]
pub(super) fn compile(
	lane: &mut MpiCollectiveLane<'_>,
	shard: &AmplitudeShard,
	scale: f64,
	levels: &mut [Level],
	mass: &mut [f64],
	phases: &mut [f64],
	send: &mut [u8],
	receive: &mut [u8],
	chunk: usize,
) -> Result<(f64, f64)> {
	let extent = shard.dimension / shard.parts;
	let rank_depth = usize::try_from(shard.parts.ilog2()).map_err(|_| Error::Overflow)?;
	for (index, value) in shard.values.iter().enumerate() {
		mass[extent + index] = (value.re / scale).hypot(value.im / scale);
		phases[extent + index] = if value.re == 0. && value.im == 0. {
			0.
		} else {
			value.im.atan2(value.re)
		};
	}
	for node in (1..extent).rev() {
		let (left, right) = (mass[2 * node], mass[2 * node + 1]);
		mass[node] = left.hypot(right);
		let depth = usize::try_from(node.ilog2()).map_err(|_| Error::Overflow)?;
		let offset = node - (1 << depth);
		levels[rank_depth + depth].y[offset] = 2. * right.atan2(left);
		levels[rank_depth + depth].z[offset] = phases[2 * node + 1] - phases[2 * node];
		phases[node] = 0.5 * phases[2 * node] + 0.5 * phases[2 * node + 1];
	}
	let mut root_mass = mass[1];
	let mut root_phase = phases[1];
	let mut stride = 1;
	while stride < shard.parts {
		let right = shard.rank % (2 * stride) == stride;
		let left = shard.rank.is_multiple_of(2 * stride);
		let mut packet = [0; 16];
		put(&mut packet, 0, root_mass)?;
		put(&mut packet, 1, root_phase)?;
		if right {
			lane.send_bytes(
				&packet,
				i32::try_from(shard.rank - stride).map_err(|_| Error::Overflow)?,
				3200,
			)
			.context("sending amplitude tree root")?;
		} else if left {
			let count = lane
				.receive_bytes(
					&mut packet,
					i32::try_from(shard.rank + stride).map_err(|_| Error::Overflow)?,
					3200,
				)
				.context("receiving amplitude tree root")?;
			if count != packet.len() {
				return Err(Error::Value("amplitude root packet length"));
			}
			let other_mass = float(&packet, 0)?;
			let other_phase = float(&packet, 1)?;
			let depth =
				rank_depth - 1 - usize::try_from(stride.ilog2()).map_err(|_| Error::Overflow)?;
			levels[depth].y[0] = 2. * other_mass.atan2(root_mass);
			levels[depth].z[0] = other_phase - root_phase;
			root_mass = root_mass.hypot(other_mass);
			root_phase = 0.5 * root_phase + 0.5 * other_phase;
		}
		stride *= 2;
	}
	let mut roots = [0; 16];
	put(&mut roots, 0, root_mass * scale)?;
	put(&mut roots, 1, root_phase)?;
	broadcast(lane, 0, &mut roots)?;
	for (depth, level) in levels.iter_mut().enumerate() {
		walsh(
			lane,
			1 << depth,
			shard.rank,
			shard.parts,
			level,
			send,
			receive,
			chunk,
		)?;
	}
	Ok((float(&roots, 0)?, float(&roots, 1)?))
}
#[allow(
	clippy::too_many_arguments,
	reason = "Admitted distributed level and transport ownership must travel together"
)]
fn walsh(
	lane: &mut MpiCollectiveLane<'_>,
	count: usize,
	rank: usize,
	parts: usize,
	level: &mut Level,
	send: &mut [u8],
	receive: &mut [u8],
	chunk: usize,
) -> Result<()> {
	if level.y.is_empty() {
		return Ok(());
	}
	let local = level.y.len();
	let owner_stride = if count < parts { parts / count } else { 1 };
	let node = if count < parts {
		rank / owner_stride
	} else {
		rank * local
	};
	let mut stride = 1;
	while stride < count {
		if stride < local {
			for base in (0..local).step_by(2 * stride) {
				for offset in 0..stride {
					for values in [&mut level.y, &mut level.z] {
						let (a, b) = (values[base + offset], values[base + offset + stride]);
						values[base + offset] = 0.5 * a + 0.5 * b;
						values[base + offset + stride] = 0.5 * a - 0.5 * b;
					}
				}
			}
		} else {
			let peer = if count < parts {
				(node ^ stride) * owner_stride
			} else {
				rank ^ (stride / local)
			};
			for start in (0..local).step_by(chunk) {
				let length = (local - start).min(chunk);
				let bytes = length * 16;
				for offset in 0..length {
					put(send, 2 * offset, level.y[start + offset])?;
					put(send, 2 * offset + 1, level.z[start + offset])?;
				}
				let received = lane
					.send_receive_bytes(
						&send[..bytes],
						i32::try_from(peer).map_err(|_| Error::Overflow)?,
						3201,
						&mut receive[..bytes],
					)
					.context("exchanging amplitude Walsh butterfly")?;
				if received != bytes {
					return Err(Error::Value("amplitude butterfly packet length"));
				}
				for offset in 0..length {
					let (y, z) = (float(receive, 2 * offset)?, float(receive, 2 * offset + 1)?);
					if node & stride == 0 {
						level.y[start + offset] = 0.5 * level.y[start + offset] + 0.5 * y;
						level.z[start + offset] = 0.5 * level.z[start + offset] + 0.5 * z;
					} else {
						level.y[start + offset] = 0.5 * y - 0.5 * level.y[start + offset];
						level.z[start + offset] = 0.5 * z - 0.5 * level.z[start + offset];
					}
				}
			}
		}
		stride *= 2;
	}
	Ok(())
}
pub(super) fn table_identity(
	lane: &mut MpiCollectiveLane<'_>,
	levels: &[Level],
	rank: usize,
	parts: usize,
) -> Result<u64> {
	let mut own = 0u64;
	for (depth, level) in levels.iter().enumerate() {
		let count = 1usize << depth;
		let start = if count < parts {
			rank / (parts / count)
		} else {
			rank * (count / parts)
		};
		for (offset, (y, z)) in level.y.iter().zip(&level.z).enumerate() {
			own = own.wrapping_add(digest(count - 1 + start + offset, y.to_bits(), z.to_bits()));
		}
	}
	let mut identity = 0u64;
	for peer in 0..parts {
		let mut packet = own.to_le_bytes();
		broadcast(lane, peer, &mut packet)?;
		identity = identity.wrapping_add(u64::from_le_bytes(packet));
	}
	Ok(identity)
}
pub(super) fn butterfly_cost(
	dimension: usize,
	rank: usize,
	parts: usize,
	chunk: usize,
) -> Result<(usize, usize)> {
	let mut bytes = 0usize;
	let mut messages = 0usize;
	let qubits = usize::try_from(dimension.ilog2()).map_err(|_| Error::Overflow)?;
	for depth in 0..qubits {
		let count = 1usize << depth;
		let local = Level::count(count, rank, parts)?;
		if local == 0 {
			continue;
		}
		let mut stride = 1;
		while stride < count {
			if stride >= local {
				bytes = bytes
					.checked_add(local.checked_mul(16).ok_or(Error::Overflow)?)
					.ok_or(Error::Overflow)?;
				messages = messages
					.checked_add(local.div_ceil(chunk))
					.ok_or(Error::Overflow)?;
			}
			stride *= 2;
		}
	}
	Ok((bytes, messages))
}
