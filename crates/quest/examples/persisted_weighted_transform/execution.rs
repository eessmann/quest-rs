//! Bounded local projection and pair residual; no production state gather.
#![allow(
	clippy::arithmetic_side_effects,
	reason = "Fixed 1024-amplitude state, <=8 ranks, 64-element packets and three independent fresh requests"
)]
use super::{
	phases,
	policy::{self, Result},
	runtime,
};
use quest::{
	Complex64, QubitCount,
	collective::{CollectiveEnvironment, CollectiveRegister},
	qsvt::matching_lcu::collective::PreparedMatchingLcu,
};
use quest_sys::mpi::{MpiCommunicator, abort_job};
use serde_json::{Value, json};
use std::{path::Path, time::Instant};
// Both ends of every XOR pair use this portable tag on the separate readout
// communicator. The blocking exchanges order chunks without per-rank tags.
const READOUT_PAIR_TAG: i32 = 31040;

pub fn decode_pair_packet(incoming: &[u8; 1024], received: usize) -> Result<Vec<Complex64>> {
	if received != incoming.len() {
		return Err("readout pair packet length".into());
	}
	let mut partner = Vec::new();
	partner.try_reserve_exact(64)?;
	if partner.capacity() > 128 {
		return Err("pair allocation capacity".into());
	}
	for i in 0..64 {
		partner.push(Complex64::new(
			f64::from_le_bytes(
				incoming
					.get(i * 16..i * 16 + 8)
					.ok_or("pair packet")?
					.try_into()?,
			),
			f64::from_le_bytes(
				incoming
					.get(i * 16 + 8..i * 16 + 16)
					.ok_or("pair packet")?
					.try_into()?,
			),
		));
	}
	Ok(partner)
}

fn reduce(comm: &MpiCommunicator<'_>, local: [f64; 6]) -> Result<[f64; 6]> {
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let mut total = [0.; 6];
	for owner in 0..parts {
		let mut frame = [0_u8; 48];
		if owner == rank {
			for (i, x) in local.into_iter().enumerate() {
				frame
					.get_mut(i * 8..(i + 1) * 8)
					.ok_or("sum frame")?
					.copy_from_slice(&x.to_le_bytes());
			}
		}
		comm.collective_lane()?
			.broadcast_bytes(i32::try_from(owner)?, &mut frame)?;
		for (i, sum) in total.iter_mut().enumerate() {
			*sum += f64::from_le_bytes(
				frame
					.get(i * 8..(i + 1) * 8)
					.ok_or("sum frame")?
					.try_into()?,
			);
		}
	}
	if total.iter().any(|x| !x.is_finite()) {
		return Err("readout aggregate nonfinite".into());
	}
	Ok(total)
}
pub fn readout_floor(
	local: usize,
	parts: usize,
	scale: f64,
	bytes: usize,
	work: usize,
) -> Result<(usize, usize)> {
	if !matches!(parts, 1 | 2 | 4 | 8)
		|| local.checked_mul(parts) != Some(1024)
		|| !local.is_multiple_of(64)
		|| !scale.is_finite()
		|| scale <= 0.
	{
		return Err("bounded native partition/rescaling".into());
	}
	let floor = local
		.checked_mul(256)
		.and_then(|n| parts.checked_mul(4096).and_then(|p| n.checked_add(p)))
		.ok_or("readout work overflow")?;
	let payload = if parts == 1 {
		0
	} else {
		local.checked_mul(16).ok_or("readout packet overflow")?
	};
	if bytes < 16_384 || work < floor {
		return Err("readout resource floor".into());
	}
	Ok((floor, payload))
}
#[allow(
	clippy::too_many_lines,
	reason = "Readout keeps fixed owner agreements beside bounded pair packets and full-coordinate arithmetic"
)]
pub fn readout(
	register: &CollectiveRegister<'_, '_, '_>,
	comm: &MpiCommunicator<'_>,
	scale: f64,
	adjoint: bool,
) -> Result<Value> {
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let local = register.deployment().local_amplitudes();
	let (work_floor, payload_floor) = runtime::local(comm, || {
		readout_floor(
			local,
			parts,
			scale,
			policy::READOUT_BYTES,
			policy::READOUT_WORK,
		)
	})?;
	let mut frame = [0_u8; 32];
	for (i, v) in [
		u64::try_from(parts)?,
		u64::try_from(local)?,
		scale.to_bits(),
		u64::from(adjoint),
	]
	.into_iter()
	.enumerate()
	{
		frame
			.get_mut(i * 8..(i + 1) * 8)
			.ok_or("readout frame")?
			.copy_from_slice(&v.to_le_bytes());
	}
	runtime::common(comm, &frame)?;
	let targets = runtime::local(comm, || policy::targets(32))?;
	let mut sums = [0.; 6];
	let mut sent = 0usize;
	let mut reads = 0usize;
	for offset in (0..local).step_by(64) {
		let (values, mut partner) = runtime::local(comm, || {
			let values = register.read_local_amplitudes(offset, 64)?;
			if values.capacity() > 128 {
				return Err("readout actual returned capacity".into());
			}
			let partner = if parts == 1 {
				register.read_local_amplitudes(offset ^ 512, 64)?
			} else {
				Vec::new()
			};
			if partner.capacity() > 128 {
				return Err("partner readout capacity".into());
			}
			Ok((values, partner))
		})?;
		reads += 1 + usize::from(parts == 1);
		if parts > 1 {
			let mut outgoing = [0_u8; 1024];
			let mut incoming = [0_u8; 1024];
			for (i, z) in values.iter().enumerate() {
				outgoing
					.get_mut(i * 16..i * 16 + 8)
					.ok_or("pair packet")?
					.copy_from_slice(&z.re.to_le_bytes());
				outgoing
					.get_mut(i * 16 + 8..i * 16 + 16)
					.ok_or("pair packet")?
					.copy_from_slice(&z.im.to_le_bytes());
			}
			let received = comm.collective_lane()?.send_receive_bytes(
				&outgoing,
				i32::try_from(rank ^ (parts / 2))?,
				READOUT_PAIR_TAG,
				&mut incoming,
			)?;
			sent += 1024;
			partner = runtime::local(comm, || decode_pair_packet(&incoming, received))?;
		}
		let chunk = runtime::local(comm, || {
			let mut chunk = [0.; 6];
			for (i, z) in values.iter().enumerate() {
				let physical = rank * local + offset + i;
				let mass = z.norm_sqr();
				if !mass.is_finite() {
					return Err("nonfinite output".into());
				}
				chunk[0] += mass;
				if physical.trailing_zeros() >= 5 {
					let j = policy::decode_system(physical, &targets)?;
					let x = *z * scale;
					let other = *partner.get(i).ok_or("paired amplitude")? * scale;
					chunk[1] += mass;
					chunk[2] += (x - policy::inverse_entry(j, adjoint)).norm_sqr();
					chunk[3] += policy::residual_entry(x, other, j, adjoint)?.norm_sqr();
					chunk[4] += x.norm_sqr();
					chunk[5] += 1.;
				}
			}
			if chunk.iter().any(|x| !x.is_finite()) {
				return Err("nonfinite readout chunk".into());
			}
			Ok(chunk)
		})?;
		runtime::local(comm, || {
			for (a, b) in sums.iter_mut().zip(chunk) {
				*a += b;
			}
			if sums.iter().any(|x| !x.is_finite()) {
				return Err("local readout accumulation overflow".into());
			}
			Ok(())
		})?;
	}
	runtime::local(comm, || {
		if sums.iter().any(|x| !x.is_finite()) {
			return Err("local readout arithmetic overflow".into());
		}
		Ok(())
	})?;
	let total = reduce(comm, sums)?;
	if total[5].to_bits() != 32_f64.to_bits() || total[1] <= 0. {
		return Err("full success-coordinate readout".into());
	}
	Ok(
		json!({"total_probability":total[0],"success_probability":total[1],"relative_vector_error":(total[2]/(32./13.)).sqrt(),"relative_residual":total[3].sqrt(),"recovered_norm_squared":total[4],"system_coordinates_visited":32,"local_chunks":local/64,"indexed_reads":reads,"local_pair_payload_sent_bytes":sent,"coordinator_broadcast_payload_per_recipient":48*parts,"managed_envelope_bytes":policy::READOUT_BYTES,"max_rank_work_ceiling":policy::READOUT_WORK,"admitted_work_floor":work_floor,"pair_payload_floor":payload_floor,"residual_operator":if adjoint{"A†"}else{"A"}}),
	)
}
fn telemetry(t: quest::qsvt::matching_lcu::telemetry::RoutingTelemetry) -> Value {
	let r = t.routing;
	json!({"source_queries":t.source_queries,"child_events":t.child_events,"exact":t.exact,"routing":{"batches":r.batches,"local_pair_candidates":r.local_pair_candidates,"maximum_batch_pairs":r.maximum_batch_pairs,"maximum_routed_amplitudes":r.maximum_routed_amplitudes,"coordination_calls":r.coordination_calls,"indexed_reads":r.indexed_reads,"indexed_writes":r.indexed_writes,"sent_bytes":r.point_to_point_sent_bytes,"received_bytes":r.point_to_point_received_bytes},"excludes":["PREP","response/projectors","constructor/coordinator","native internal MPI","MPI protocol"]})
}
fn fatal<T>(f: impl FnOnce() -> Result<T>) -> T {
	match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
		Ok(Ok(x)) => x,
		_ => abort_job(),
	}
}
fn handoff_output(
	comm: &MpiCommunicator<'_>,
	output: Value,
	construction: &Value,
) -> Result<Value> {
	let admission = runtime::local(comm, || {
		runtime::admit_json_overlap(
			&[&output, construction],
			&[1, 1],
			262_144 + 65_536,
			policy::READOUT_BYTES,
		)
	});
	if let Err(e) = admission {
		let progress = runtime::local(comm, || runtime::compact_progress(&output))?;
		drop(output);
		return runtime::local(comm, || {
			Ok(
				json!({"status":"rejected","phase":"receipt owner handoff","error":e.to_string(),"discarded_oversize_receipt":true,"progress":progress}),
			)
		});
	}
	Ok(output)
}
fn result_overlap(
	comm: &MpiCommunicator<'_>,
	metadata: &Value,
	construction: &Value,
	calls: &[Value],
	progress: Option<&Value>,
) -> Result<()> {
	runtime::local(comm, || {
		let mut extras = 65_536usize;
		for call in calls {
			extras = extras
				.checked_add(
					runtime::json_payload(call)?
						.checked_mul(2)
						.ok_or("result overlap")?,
				)
				.ok_or("result overlap")?;
		}
		if let Some(progress) = progress {
			extras = extras
				.checked_add(
					runtime::json_payload(progress)?
						.checked_mul(2)
						.ok_or("progress overlap")?,
				)
				.ok_or("progress overlap")?;
		}
		runtime::admit_json_overlap(
			&[metadata, construction],
			&[2, 2],
			extras,
			policy::READOUT_BYTES + policy::LOAD_BYTES,
		)?;
		Ok(())
	})
}
#[allow(
	clippy::too_many_lines,
	reason = "Three fixed fresh requests retain admission, fatal apply, partial progress and owner handoff in one visible lifecycle"
)]
pub fn replay(
	env: &CollectiveEnvironment<'_, '_>,
	readout_comm: &MpiCommunicator<'_>,
	source: PreparedMatchingLcu<'_, '_, '_>,
	directory: &Path,
	construction: &Value,
) -> Result<Value> {
	let comm = env.communicator();
	let parts = usize::try_from(comm.size()?)?;
	let phase_guard = env.reserve_external_bytes(policy::LOAD_BYTES)?;
	let start = Instant::now();
	let imported = runtime::local(comm, || phases::import(&source, directory))?;
	let metadata_hash = runtime::local(comm, || {
		Ok(runtime::hash(&serde_json::to_vec(&imported.metadata)?))
	})?;
	runtime::common(comm, metadata_hash.as_bytes())?;
	let phase_import = runtime::finish(comm, start)?;
	let start = Instant::now();
	let mut prepared =
		env.prepare_matching_lcu_transform(source, 4, imported.schedule, dynamic_limits(parts))?;
	let transform_construction = runtime::finish(comm, start)?;
	let mut state = env.state_vector_local(QubitCount::new(10)?)?;
	let mut calls = runtime::local(comm, || {
		let mut v = Vec::new();
		v.try_reserve_exact(3)?;
		if v.capacity() > 6 {
			return Err("apply receipt capacity".into());
		}
		Ok(v)
	})?;
	let mut cumulative = 0usize;
	for (adjoint, index) in [(false, 0), (true, 1), (false, 2)] {
		let mut progress = json!({"index":index,"adjoint":adjoint,"initialized":false,"applied":false,"readout_completed":false});
		let attempt = (|| -> Result<Value> {
			let admitted = prepared.admit_apply(&state, adjoint, 0, 0)?;
			cumulative = cumulative
				.checked_add(admitted.maximum_rank_work)
				.ok_or("three-request work overflow")?;
			if cumulative > 24_000_000_000 {
				return Err("cumulative replay work ceiling".into());
			}
			let start = Instant::now();
			state.init_zero()?;
			*progress.get_mut("initialized").ok_or("progress field")? = json!(true);
			let initialization = runtime::finish(comm, start)?;
			let start = Instant::now();
			fatal(|| {
				prepared.apply(&mut state, adjoint, 0, 0)?;
				Ok(())
			});
			*progress.get_mut("applied").ok_or("progress field")? = json!(true);
			let application = runtime::finish(comm, start)?;
			let current = runtime::local(comm, || {
				prepared
					.last_apply_telemetry()
					.ok_or_else(|| "missing current complete apply telemetry".into())
			})?;
			*progress
				.as_object_mut()
				.ok_or("progress object")?
				.entry("routing")
				.or_insert(Value::Null) = runtime::local(comm, || Ok(telemetry(current)))?;
			let start = Instant::now();
			let readout = readout(&state, readout_comm, imported.rescaling, adjoint)?;
			*progress
				.get_mut("readout_completed")
				.ok_or("progress field")? = json!(true);
			let readout_stage = runtime::finish(comm, start)?;
			let residual = readout
				.get("relative_residual")
				.and_then(Value::as_f64)
				.ok_or("residual")?;
			let error = readout
				.get("relative_vector_error")
				.and_then(Value::as_f64)
				.ok_or("vector error")?;
			let native_total = readout
				.get("total_probability")
				.and_then(Value::as_f64)
				.ok_or("norm")?;
			let accurate = residual <= 1e-3 && error <= 2e-3 && (native_total - 1.).abs() <= 1e-10;
			if parts > 1 && current.routing.point_to_point_sent_bytes == 0 {
				return Err("expected genuine cross-rank routing".into());
			}
			Ok(
				json!({"index":index,"adjoint":adjoint,"fresh_basis_rhs":true,"coherent_rhs_identity_gates":0,"local_simulator_initialization":{"max_rank_work_ceiling":policy::READOUT_WORK,"stage":initialization},"status":if accurate{"accuracy-passed"}else{"accuracy-failure"},"application":application,"admission":{"max_rank_work":admitted.maximum_rank_work,"aggregate_work":admitted.aggregate_work,"native_dispatches":admitted.native_dispatches,"aggregate_application_payload_ceiling":admitted.aggregate_application_bytes,"control_calls":admitted.control_calls,"preflight_work":admitted.preflight_work,"managed_rank_peak_bytes":admitted.managed_rank_peak_bytes},"routing":telemetry(current),"readout":readout,"readout_stage":readout_stage}),
			)
		})();
		match attempt {
			Ok(v) => calls.push(v),
			Err(e) => {
				result_overlap(
					comm,
					&imported.metadata,
					construction,
					&calls,
					Some(&progress),
				)?;
				let output = json!({"status":"rejected","phase":"replay/readout","error":e.to_string(),"completed_calls":calls,"attempt_index":index,"attempt_progress":progress,"construction":construction,"phase_import":phase_import,"transform_construction":transform_construction,"freeze":imported.metadata});
				drop(imported.metadata);
				drop(calls);
				drop(progress);
				drop(phase_import);
				drop(transform_construction);
				let output = handoff_output(comm, output, construction)?;
				drop(phase_guard);
				return Ok(output);
			}
		}
	}
	let accuracy = calls
		.iter()
		.all(|c| c.get("status").and_then(Value::as_str) == Some("accuracy-passed"));
	result_overlap(comm, &imported.metadata, construction, &calls, None)?;
	let output = json!({"status":if accuracy{"completed"}else{"accuracy-failure"},"calls":calls,"source_orientation":"three weighted sources encode H=A†; forward transforms solve A; literal adjoint solves A†","freeze":imported.metadata,"finite_polynomial_success_target":imported.finite_success,"reciprocal_ideal_success_target":imported.ideal_success,"construction":construction,"phase_import":phase_import,"transform_construction":transform_construction,"transform_constructor_work":prepared.constructor_work(),"cumulative_max_rank_work":cumulative,"fixed_declared_three_apply_work_ceiling":24_000_000_000_u64,"native_state_payload_bytes_per_child":state.deployment().local_amplitudes()*16,"native_state_accounted_allowance_per_child":state.deployment().local_amplitudes()*16*4,"native_os_overhead_bytes":null,"rigorous_inverse_certificate":null});
	drop(imported.metadata);
	drop(calls);
	drop(phase_import);
	drop(transform_construction);
	let output = handoff_output(comm, output, construction)?;
	drop(phase_guard);
	Ok(output)
}
const fn dynamic_limits(
	parts: usize,
) -> quest::qsvt::matching_lcu_transform::TransformExecutionLimits {
	runtime::transform_limits(parts)
}
