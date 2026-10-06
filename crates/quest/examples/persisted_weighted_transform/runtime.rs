//! Collective consumer stages over existing immutable prepared owners.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::cast_precision_loss,
	reason = "Fixed N<=32, P<=8 and bounded packet/count declarations are admitted before numerical loops"
)]
use super::policy::{self, Result};
use quest::{
	Complex64, MemoryBudget, QubitCount,
	collective::{CollectiveEnvironment, MpiRuntime},
	qsvt::{
		matching::{
			collective::{PreparedMatching, RoutingCapacity},
			preprocess::{ProducerLimits, produce_matching},
		},
		matching_lcu::{MatchingLcuLimits, collective::PreparedMatchingLcu},
		matching_lcu_transform::TransformExecutionLimits,
		persisted_matching::{
			PersistedPreparationLimits, PersistenceLimits, load_matching, publish_produced,
		},
	},
};
use quest_qsvt::{
	NumericalPolicy, matching_resource::ResourceReplayLimits, portfolio::LcuPlanLimits,
	state_preparation::PreparationLimits,
};
use quest_qsvt_io::sharded_matching::ShardIoLimits;
use quest_sys::mpi::MpiCommunicator;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Instant};
#[derive(Debug)]
pub struct StageFailure {
	pub phase: &'static str,
	pub partial: Value,
	pub detail: String,
}
impl std::fmt::Display for StageFailure {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}: {}", self.phase, self.detail)
	}
}
impl std::error::Error for StageFailure {}
fn stage_error(
	comm: &MpiCommunicator<'_>,
	phase: &'static str,
	partial: &Value,
	error: &dyn std::error::Error,
) -> Box<dyn std::error::Error> {
	match local(comm, || {
		Ok(StageFailure {
			phase,
			partial: partial.clone(),
			detail: error.to_string(),
		})
	}) {
		Ok(e) => Box::new(e),
		Err(e) => e,
	}
}
pub fn local<T>(comm: &MpiCommunicator<'_>, f: impl FnOnce() -> Result<T>) -> Result<T> {
	let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
		.unwrap_or_else(|_| Err("consumer local panic".into()));
	if !comm.collective_lane()?.all_agree(result.is_ok())? {
		return Err(result
			.err()
			.unwrap_or_else(|| "another consumer owner failed".into()));
	}
	result
}
pub fn barrier(comm: &MpiCommunicator<'_>) -> Result<()> {
	local(comm, || Ok(()))
}
pub fn hash(bytes: &[u8]) -> String {
	let mut result = String::with_capacity(64);
	for b in Sha256::digest(bytes) {
		use std::fmt::Write as _;
		let _ = write!(&mut result, "{b:02x}");
	}
	result
}
pub fn file(path: &Path, cap: usize) -> Result<Vec<u8>> {
	let mut handle = std::fs::File::open(path)?;
	if usize::try_from(handle.metadata()?.len())? > cap {
		return Err("consumer file cap".into());
	}
	let mut bytes = Vec::new();
	bytes.try_reserve_exact(cap.checked_add(1).ok_or("file cap")?)?;
	if bytes.capacity() > 2 * cap + 4096 {
		return Err("file actual capacity".into());
	}
	std::io::Read::read_to_end(
		&mut std::io::Read::take(&mut handle, u64::try_from(cap + 1)?),
		&mut bytes,
	)?;
	if bytes.len() > cap {
		return Err("consumer growing file".into());
	}
	Ok(bytes)
}
pub fn metadata_scan(bytes: &[u8]) -> Result<()> {
	if bytes.len() > 16_384 {
		return Err("metadata raw bytes".into());
	}
	let (mut depth, mut tokens, mut string, mut escape, mut string_bytes) =
		(0usize, 0usize, false, false, 0usize);
	for &b in bytes {
		if string {
			string_bytes += 1;
			if string_bytes > 4096 {
				return Err("metadata string bound".into());
			}
			if escape {
				escape = false;
			} else if b == b'\\' {
				escape = true;
			} else if b == b'"' {
				string = false;
			}
			continue;
		}
		match b {
			b'"' => {
				string = true;
				string_bytes = 0;
				tokens += 1;
			}
			b'{' | b'[' => {
				depth += 1;
				tokens += 1;
			}
			b'}' | b']' => {
				depth = depth.checked_sub(1).ok_or("metadata nesting")?;
			}
			b',' | b':' => tokens += 1,
			_ => {}
		}
		if depth > 16 || tokens > 2048 {
			return Err("metadata node/depth envelope".into());
		}
	}
	if string || depth != 0 {
		return Err("metadata lexical closure".into());
	}
	Ok(())
}
/// Actual String/Vec capacities plus a conservative 512-byte modeled map entry.
/// Allocator/platform metadata and briefly rejected excess capacities are external.
pub fn json_payload(value: &Value) -> Result<usize> {
	match value {
		Value::Array(v) => {
			let mut n = v
				.capacity()
				.checked_mul(size_of::<Value>())
				.ok_or("JSON capacity")?;
			for item in v {
				n = n.checked_add(json_payload(item)?).ok_or("JSON capacity")?;
			}
			Ok(n)
		}
		Value::Object(v) => {
			let mut n = 0usize;
			for (k, item) in v {
				n = n
					.checked_add(k.capacity())
					.and_then(|n| n.checked_add(512))
					.and_then(|n| json_payload(item).ok().and_then(|i| n.checked_add(i)))
					.ok_or("JSON capacity")?;
			}
			Ok(n)
		}
		Value::String(s) => Ok(s.capacity()),
		_ => Ok(0),
	}
}
pub fn admit_json_overlap(
	values: &[&Value],
	multiplicities: &[usize],
	extra: usize,
	limit: usize,
) -> Result<usize> {
	if values.len() != multiplicities.len() {
		return Err("JSON owner count".into());
	}
	let mut bytes = extra;
	for (value, &copies) in values.iter().zip(multiplicities) {
		bytes = bytes
			.checked_add(
				json_payload(value)?
					.checked_mul(copies)
					.ok_or("JSON overlap overflow")?,
			)
			.ok_or("JSON overlap overflow")?;
	}
	if bytes > limit {
		return Err("whole-live JSON/receipt envelope".into());
	}
	Ok(bytes)
}
pub fn move_row_owners(row: &mut Value, stages: Value, payload: Option<Value>) -> Result<()> {
	let object = row.as_object_mut().ok_or("row object")?;
	object.insert("stages".into(), stages);
	object.insert("result".into(), payload.unwrap_or(Value::Null));
	Ok(())
}
pub fn compact_progress(value: &Value) -> Result<Value> {
	let mut calls = Vec::new();
	calls.try_reserve_exact(3)?;
	if calls.capacity() > 6 {
		return Err("progress summary capacity".into());
	}
	if let Some(all) = value
		.get("calls")
		.or_else(|| value.get("completed_calls"))
		.and_then(Value::as_array)
	{
		for call in all.iter().take(3) {
			let status = match call.get("status").and_then(Value::as_str) {
				Some("accuracy-passed") => "accuracy-passed",
				Some("accuracy-failure") => "accuracy-failure",
				_ => "unknown",
			};
			calls.push(json!({"index":call.get("index").and_then(Value::as_u64).filter(|n|*n<3),"adjoint":call.get("adjoint").and_then(Value::as_bool),"status":status}));
		}
	}
	let attempt = value.get("attempt_progress");
	Ok(
		json!({"completed_calls":calls,"attempt_index":value.get("attempt_index").and_then(Value::as_u64).filter(|n|*n<3),"initialized":attempt.and_then(|v|v.get("initialized")).and_then(Value::as_bool),"applied":attempt.and_then(|v|v.get("applied")).and_then(Value::as_bool),"readout_completed":attempt.and_then(|v|v.get("readout_completed")).and_then(Value::as_bool),"completed_terms":value.get("terms").or_else(||value.get("publication").and_then(|v|v.get("terms"))).and_then(Value::as_array).map(|v|v.len().min(3)),"completed_children":value.get("children").and_then(Value::as_array).map(|v|v.len().min(3))}),
	)
}
pub fn canonical(path: &Path) -> Result<Value> {
	let bytes = file(path, 16_384)?;
	metadata_scan(&bytes)?;
	let value: Value = serde_json::from_slice(&bytes)?;
	if json_payload(&value)? > 524_288 {
		return Err("metadata actual modeled payload".into());
	}
	let again = serde_json::to_vec(&value)?;
	if again.capacity() > 32_768 {
		return Err("metadata serialization actual capacity".into());
	}
	if bytes != again {
		return Err("noncanonical/duplicate freeze metadata".into());
	}
	Ok(value)
}
pub fn common(comm: &MpiCommunicator<'_>, bytes: &[u8]) -> Result<()> {
	local(comm, || {
		if bytes.len() <= 1024 {
			Ok(())
		} else {
			Err("common metadata frame".into())
		}
	})?;
	let mut expected = [0_u8; 1024];
	expected
		.get_mut(..bytes.len())
		.ok_or("common frame")?
		.copy_from_slice(bytes);
	let original = expected;
	let mut length = u64::try_from(bytes.len())?.to_le_bytes();
	let mut lane = comm.collective_lane()?;
	lane.broadcast_bytes(0, &mut length)?;
	lane.broadcast_bytes(0, &mut expected)?;
	if !lane
		.all_agree(length == u64::try_from(bytes.len())?.to_le_bytes() && expected == original)?
	{
		return Err("consumer common metadata differs".into());
	}
	Ok(())
}
pub const fn persistence() -> PersistenceLimits {
	PersistenceLimits {
		io: ShardIoLimits {
			chunk_records: 16,
			max_buffer_bytes: 32_768,
			max_manifest_bytes: 262_144,
			max_buckets: 8,
			max_records: 64,
			max_file_bytes: 1_048_576,
		},
		max_local_records: 64,
		max_bytes: policy::LOAD_BYTES,
		max_communication_bytes: 64 * 1024 * 1024,
		replay: ResourceReplayLimits {
			max_bytes: policy::BRIDGE_BYTES,
			max_queries: 16_000_000,
			max_work: 268_000_000,
			max_communication_bytes: 64 * 1024 * 1024,
			max_gates: 1_000_000,
		},
	}
}
pub fn producer() -> ProducerLimits {
	let mut l = ProducerLimits {
		max_bytes: policy::BRIDGE_BYTES,
		max_local_edges: 64,
		max_endpoint_records: 128,
		max_vertex_degree: 2,
		max_rounds: 64,
		max_completion_rounds: 64,
		max_probes: 16_384,
		max_work: 100_000_000,
		max_communication_bytes: 64 * 1024 * 1024,
		batch_entries: 16,
		..Default::default()
	};
	l.stream.buffer_entries = 16;
	l.stream.max_entries = 64;
	l.stream.max_bytes = 2 * 1024 * 1024;
	l.stream.max_work = 100_000_000;
	l.stream.max_spill_bytes = 0;
	l.stream.max_runs = 1;
	l
}
pub const fn bridge(parts: usize) -> PersistedPreparationLimits {
	PersistedPreparationLimits {
		max_local_records: 64,
		max_bytes: policy::RANK_BYTES,
		max_constructor_work: 16_000_000,
		max_application_payload_bytes: 8 * 1024 * 1024,
		policy: NumericalPolicy {
			max_bytes: policy::BRIDGE_BYTES,
		},
		capacity: RoutingCapacity {
			ranks_per_node: parts,
			node_budget: MemoryBudget::new(policy::NODE_BYTES),
		},
	}
}
pub const fn lcu_limits(parts: usize) -> MatchingLcuLimits {
	MatchingLcuLimits {
		plan: LcuPlanLimits {
			max_terms: 3,
			max_bytes: 1024 * 1024,
			max_compile_work: 1_000_000,
			max_primitives: 4096,
			preparation: PreparationLimits {
				max_dimension: 4,
				max_bytes: 32_768,
				max_compile_work: 1_000_000,
				max_gates: 4096,
			},
		},
		max_local_bytes: policy::RANK_BYTES,
		node_budget: MemoryBudget::new(policy::NODE_BYTES),
		ranks_per_node: parts,
		max_constructor_work: 100_000_000,
		max_rank_work: 1_000_000_000,
		max_aggregate_work: 8_000_000_000,
		max_native_dispatches: 1_000_000,
		max_application_bytes: 256 * 1024 * 1024,
		max_control_calls: 1_000_000,
	}
}
pub const fn transform_limits(parts: usize) -> TransformExecutionLimits {
	TransformExecutionLimits {
		max_degree: 81,
		max_schedule_steps: 329,
		max_queries: 162,
		max_constructor_work: 100_000_000,
		max_preflight_work: 1_000_000_000,
		max_local_bytes: policy::RANK_BYTES,
		node_budget: MemoryBudget::new(policy::NODE_BYTES),
		ranks_per_node: parts,
		max_rank_work: 8_000_000_000,
		max_aggregate_work: 64_000_000_000,
		max_native_dispatches: 1_000_000,
		max_application_bytes: 8_000_000_000,
		max_control_calls: 2_000_000,
	}
}
/// Versioned exact numeric declarations, before any fixed term loop.
#[allow(
	clippy::too_many_lines,
	clippy::many_single_char_names,
	reason = "One bounded frame enumerates each typed fixed declaration for byte-wise collective comparison"
)]
pub fn protocol_frame(n: usize, count: usize, parts: usize) -> Result<([u8; 1024], usize)> {
	let p = producer();
	let i = persistence();
	let b = bridge(parts);
	let l = lcu_limits(parts);
	let t = transform_limits(parts);
	let mut frame = [0_u8; 1024];
	let mut len = 0usize;
	let mut word = |v: u64| -> Result<()> {
		let end = len.checked_add(8).ok_or("protocol frame overflow")?;
		frame
			.get_mut(len..end)
			.ok_or("protocol frame size")?
			.copy_from_slice(&v.to_le_bytes());
		len = end;
		Ok(())
	};
	for value in [
		1,
		n,
		count,
		parts,
		3,
		8,
		policy::RANK_BYTES,
		policy::NODE_BYTES,
		policy::LOAD_BYTES,
		policy::BRIDGE_BYTES,
		policy::COMPILE_BYTES,
		policy::READOUT_BYTES,
		policy::READOUT_WORK,
	] {
		word(u64::try_from(value)?)?;
	}
	for (term, weight) in policy::WEIGHTS.into_iter().enumerate() {
		word(u64::try_from(term)?)?;
		word(weight.to_bits())?;
		word(0_f64.to_bits())?;
		for slot in 0..2 {
			let c = policy::coefficient(term, slot)?;
			word(c.re.to_bits())?;
			word(c.im.to_bits())?;
		}
	}
	for value in [
		p.max_local_edges,
		p.max_endpoint_records,
		p.max_vertex_degree,
		p.max_rounds,
		p.max_completion_rounds,
		p.max_probes,
		p.max_bytes,
		p.max_work,
		p.max_communication_bytes,
		p.batch_entries,
		p.stream.buffer_entries,
		p.stream.max_entries,
		p.stream.max_bytes,
		p.stream.max_spill_bytes,
		p.stream.max_runs,
		p.stream.max_work,
		i.io.chunk_records,
		i.io.max_buffer_bytes,
		i.io.max_manifest_bytes,
		i.io.max_buckets,
		i.io.max_records,
		usize::try_from(i.io.max_file_bytes)?,
		i.max_local_records,
		i.max_bytes,
		i.max_communication_bytes,
		i.replay.max_bytes,
		i.replay.max_queries,
		i.replay.max_work,
		i.replay.max_communication_bytes,
		i.replay.max_gates,
		b.max_local_records,
		b.max_bytes,
		b.max_constructor_work,
		b.max_application_payload_bytes,
		b.policy.max_bytes,
		l.plan.max_terms,
		l.plan.max_bytes,
		l.plan.max_compile_work,
		l.plan.max_primitives,
		l.plan.preparation.max_dimension,
		l.plan.preparation.max_bytes,
		l.plan.preparation.max_compile_work,
		l.plan.preparation.max_gates,
		l.max_local_bytes,
		l.max_constructor_work,
		l.max_rank_work,
		l.max_aggregate_work,
		l.max_native_dispatches,
		l.max_application_bytes,
		l.max_control_calls,
		t.max_degree,
		t.max_schedule_steps,
		t.max_queries,
		t.max_constructor_work,
		t.max_preflight_work,
		t.max_local_bytes,
		t.max_rank_work,
		t.max_aggregate_work,
		t.max_native_dispatches,
		t.max_application_bytes,
		t.max_control_calls,
		16_384,
		32 * 1024 * 1024,
		1_000_000_000,
		24_000_000_000,
	] {
		word(u64::try_from(value)?)?;
	}
	for value in [policy::TOLERANCE, 1e-11, 1e-12, 1e-3, 2e-3] {
		word(value.to_bits())?;
	}
	// Fixed algorithm/backend tags: InverseNlftDivideConquer=1, Scalar=1.
	word(1)?;
	word(1)?;
	for value in [
		b.capacity.ranks_per_node,
		b.capacity.node_budget.bytes(),
		l.ranks_per_node,
		l.node_budget.bytes(),
		t.ranks_per_node,
		t.node_budget.bytes(),
	] {
		word(u64::try_from(value)?)?;
	}
	let targets = policy::targets(n)?;
	word(u64::try_from(targets.len())?)?;
	for target in targets {
		word(u64::try_from(target)?)?;
	}
	for v in [2_u64, 3, 4] {
		word(v)?;
	}
	Ok((frame, len))
}
fn admit_protocol(comm: &MpiCommunicator<'_>, n: usize, count: usize, parts: usize) -> Result<()> {
	let (frame, len) = local(comm, || protocol_frame(n, count, parts))?;
	common(comm, frame.get(..len).ok_or("protocol frame")?)?;
	let mut parser = [0_u8; 56];
	for (i, value) in [
		16_384_u64,
		2048,
		16,
		65_536,
		131_072,
		2_147_483_648,
		4_194_304,
	]
	.into_iter()
	.enumerate()
	{
		parser
			.get_mut(i * 8..(i + 1) * 8)
			.ok_or("parser frame")?
			.copy_from_slice(&value.to_le_bytes());
	}
	common(comm, &parser)
}
pub fn memory() -> Result<Value> {
	let s = std::fs::read_to_string("/proc/self/status")?;
	let get = |prefix: &str| -> Result<usize> {
		Ok(s.lines()
			.find_map(|l| l.strip_prefix(prefix))
			.and_then(|l| l.split_whitespace().next())
			.ok_or("Linux memory counter")?
			.parse::<usize>()?
			.checked_mul(1024)
			.ok_or("memory overflow")?)
	};
	Ok(
		json!({"rss_endpoint_bytes":get("VmRSS:")?,"rss_high_water_bytes":get("VmHWM:")?,"address_space_endpoint_bytes":get("VmSize:")?,"address_space_high_water_bytes":get("VmPeak:")?}),
	)
}
pub fn finish(comm: &MpiCommunicator<'_>, start: Instant) -> Result<Value> {
	barrier(comm)?;
	local(comm, || {
		Ok(json!({"seconds":start.elapsed().as_secs_f64(),"memory":memory()?}))
	})
}
pub fn header(h: quest_qsvt::MatchingHeader) -> Value {
	json!({"rows":h.rows,"cols":h.cols,"system_qubits":h.system_qubits,"color_qubits":h.color_qubits,"num_colors":h.num_colors,"beta":h.beta,"alpha":h.alpha,"source_identity":h.source_identity,"record_count":h.record_count,"record_digest":h.record_digest})
}
pub fn validate_header(h: quest_qsvt::MatchingHeader, n: usize) -> Result<()> {
	h.validate()?;
	if h.rows != n
		|| h.cols != n
		|| h.num_colors != 2
		|| h.color_qubits != 1
		|| h.beta.to_bits() != 1_f64.to_bits()
		|| h.alpha.to_bits() != 2_f64.to_bits()
		|| h.record_count != 2 * n
	{
		return Err("fixed equal colored layouts".into());
	}
	Ok(())
}
pub fn publish(env: &CollectiveEnvironment<'_, '_>, directory: &Path, n: usize) -> Result<Value> {
	let comm = env.communicator();
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	if parts != 8 {
		return Err("publication requires exactly eight origin ranks".into());
	}
	admit_protocol(comm, n, if n == 32 { 10 } else { 8 }, parts)?;
	let envelope = env.reserve_external_bytes(policy::BRIDGE_BYTES)?;
	let mut terms = local(comm, || {
		let mut v = Vec::new();
		v.try_reserve_exact(3)?;
		if v.capacity() > 6 {
			return Err("publication receipt capacity".into());
		}
		Ok(v)
	})?;
	for term in 0..3 {
		let attempted = (|| -> Result<()> {
			let path = local(comm, || Ok(directory.join(format!("term-{term}"))))?;
			local(comm, || {
				if rank == 0 {
					std::fs::create_dir(&path)?;
				}
				Ok(())
			})?;
			let started = Instant::now();
			let produced = produce_matching(
				comm,
				n,
				n,
				local(comm, || policy::entries(n, term, rank, parts))?,
				producer(),
			)?;
			local(comm, || validate_header(produced.shard().header(), n))?;
			let generation = finish(comm, started)?;
			let stats = produced.statistics();
			let started = Instant::now();
			let manifest = publish_produced(
				comm,
				&produced,
				&path,
				path.join("manifest.json"),
				8,
				PersistenceLimits {
					max_bytes: policy::RANK_BYTES,
					..persistence()
				},
			)?;
			let publication = finish(comm, started)?;
			let term_receipt = local(comm, || {
				let mut buckets = Vec::new();
				buckets.try_reserve_exact(8)?;
				if buckets.capacity() > 16 {
					return Err("bucket receipt capacity".into());
				}
				for b in manifest.buckets() {
					buckets.push(json!({"bucket":b.bucket,"sha256":b.sha256,"semantic_sha256":b.semantic_sha256,"records":b.records,"file_bytes":b.size_bytes}));
				}
				Ok(
					json!({"term":term,"local_generated_entries":stats.input_entries,"header":header(produced.shard().header()),"manifest_semantic_sha256":manifest.semantic_sha256(),"buckets":buckets,"producer":{"work":stats.work,"sent_bytes":stats.sent_bytes,"managed_peak_bytes":stats.peak_managed_bytes},"generation":generation,"publication":publication}),
				)
			})?;
			terms.push(term_receipt);
			drop(manifest);
			drop(produced);
			Ok(())
		})();
		if let Err(e) = attempted {
			let partial = local(comm, || {
				Ok(
					json!({"terms":terms,"completed_terms":term,"attempt_term":term,"origin_ranks":8,"buckets":8}),
				)
			})?;
			return Err(stage_error(comm, "publication", &partial, e.as_ref()));
		}
	}
	drop(envelope);
	Ok(
		json!({"terms":terms,"buckets":8,"origin_ranks":8,"source_generation":"direct cyclic owned-column iterator; no full source catalogue"}),
	)
}
pub fn children<'env, 'comm, 'runtime>(
	env: &'env CollectiveEnvironment<'comm, 'runtime>,
	directory: &Path,
	n: usize,
	count: usize,
) -> Result<(
	Vec<(Complex64, PreparedMatching<'env, 'comm, 'runtime>)>,
	Value,
)> {
	let comm = env.communicator();
	let parts = usize::try_from(comm.size()?)?;
	let (mut children, mut receipts) = local(comm, || {
		let mut c = Vec::new();
		c.try_reserve_exact(3)?;
		let mut r = Vec::new();
		r.try_reserve_exact(3)?;
		if c.capacity() > 6 || r.capacity() > 6 {
			return Err("children/receipt capacity".into());
		}
		Ok((c, r))
	})?;
	admit_protocol(comm, n, count, parts)?;
	for term in 0..3 {
		let attempted = (|| -> Result<()> {
			let path = local(comm, || Ok(directory.join(format!("term-{term}"))))?;
			let guard = env.reserve_external_bytes(policy::LOAD_BYTES)?;
			let start = Instant::now();
			let loaded = load_matching(comm, path.join("manifest.json"), &path, persistence())?;
			local(comm, || validate_header(loaded.header(), n))?;
			let h = local(comm, || Ok(header(loaded.header())))?;
			let returned_source_bytes = loaded.retained_bytes();
			let load = finish(comm, start)?;
			let start = Instant::now();
			let (count, targets) =
				local(comm, || Ok((QubitCount::new(count)?, policy::targets(n)?)))?;
			let (prepared, r) =
				loaded.into_prepared_matching(env, count, targets, bridge(parts))?;
			drop(guard);
			let preparation = finish(comm, start)?;
			let receipt = local(comm, || {
				Ok(
					json!({"term":term,"header":h,"returned_source_bytes":returned_source_bytes,"manifest_semantic_sha256":r.manifest_sha256,"native_source_identity":r.native_source_identity,"native_construction_identity":r.native_construction_identity,"source_clone_bytes":r.snapshot_bytes,"actual_target_bytes":r.target_bytes,"native_preparation_scratch_allowance":r.native_preparation_scratch_bytes,"whole_rank_admitted_ceiling":r.rank_peak_bytes,"planned_whole_rank_peak":r.planned_rank_peak_bytes,"whole_node_admitted_ceiling":r.node_peak_bytes,"constructor_work":r.constructor_work,"application_payload_ceiling":r.application_payload_bytes,"load":load,"bridge":preparation}),
				)
			})?;
			receipts.push(receipt);
			children.push((
				Complex64::new(*policy::WEIGHTS.get(term).ok_or("weight")?, 0.),
				prepared,
			));
			Ok(())
		})();
		if let Err(e) = attempted {
			let partial = local(comm, || {
				Ok(json!({"children":receipts,"completed_children":term,"attempt_term":term}))
			})?;
			return Err(stage_error(comm, "load/bridge", &partial, e.as_ref()));
		}
	}
	let receipts = local(comm, || Ok(json!(receipts)))?;
	Ok((children, receipts))
}
pub fn lcu<'env, 'comm, 'runtime>(
	env: &'env CollectiveEnvironment<'comm, 'runtime>,
	directory: &Path,
	n: usize,
	count: usize,
) -> Result<(PreparedMatchingLcu<'env, 'comm, 'runtime>, Value)> {
	let (parts, comm) = (
		usize::try_from(env.communicator().size()?)?,
		env.communicator(),
	);
	let (children, child_receipts) = children(env, directory, n, count)?;
	let start = Instant::now();
	let selectors = local(comm, || Ok(vec![2, 3]))?;
	let source = match env.prepare_matching_lcu(children, selectors, lcu_limits(parts)) {
		Ok(s) => s,
		Err(e) => {
			return Err(stage_error(comm, "LCU composition", &child_receipts, &e));
		}
	};
	let stage = finish(comm, start)?;
	let descriptor = source.plan().descriptor();
	let communication = source.composition_communication();
	let receipt = local(comm, || {
		Ok(
			json!({"children":child_receipts,"stage":stage,"normalization":descriptor.normalization,"source_identity":descriptor.source_identity,"construction_identity":descriptor.construction_identity,"compile_work":source.composition_compile_work(),"retained_accounted_bytes":source.retained_bytes()?,"normalization_roundoff":source.plan().resources().normalization_roundoff,"preparation_gates":source.plan().resources().preparation_gates,"primitive_gates":source.plan().resources().primitive_gates,"communication":{"compiled_events":communication.compiled_events,"comparison_collective_calls":communication.comparison_collective_calls,"comparison_broadcast_payload_bytes":communication.aggregate_comparison_broadcast_payload_bytes,"other_constructor_collective_calls_ceiling":communication.other_constructor_collective_calls_upper_bound}}),
		)
	})?;
	Ok((source, receipt))
}
#[allow(
	clippy::too_many_lines,
	reason = "The fixture keeps lifecycle and stage rejection receipts in one explicit entry point"
)]
pub fn run() -> Result<()> {
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	let args: Vec<_> = std::env::args().skip(1).take(3).collect();
	let parsed = local(&world, || {
		if args.len() != 2
			|| args.iter().any(|a| a.len() > 256)
			|| !matches!(
				args.first().map(String::as_str),
				Some("publish" | "compile" | "replay" | "split")
			) {
			return Err("usage: MODE DIRECTORY; fixed modes publish/compile/replay/split".into());
		}
		Ok(())
	});
	parsed?;
	let mode = args.first().ok_or("mode")?;
	common(
		&world,
		format!("{mode}:{}", args.get(1).ok_or("directory")?).as_bytes(),
	)?;
	let mut comm = if mode == "split" {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let readout_comm = comm.duplicate()?;
	let directory = Path::new(args.get(1).ok_or("directory")?);
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(policy::RANK_BYTES))
		.build()?;
	let io_guard = env.reserve_external_bytes(policy::READOUT_BYTES)?;
	let mut phase = "initial admission";
	let mut stages = json!({});
	let result = (|| -> Result<Value> {
		local(&comm, || {
			if !matches!(parts, 1 | 2 | 4 | 8)
				|| policy::RANK_BYTES
					.checked_mul(parts)
					.is_none_or(|n| n > policy::NODE_BYTES)
			{
				return Err("fixed local placement".into());
			}
			Ok(())
		})?;
		if mode == "publish" {
			phase = "publication";
			stages = publish(&env, directory, policy::N)?;
			return Ok(json!({"publication":stages}));
		}
		phase = "load/bridge/LCU";
		let (source, receipt) = lcu(&env, directory, policy::N, 10)?;
		stages = receipt;
		if mode == "compile" {
			phase = "polynomial/phase compilation";
			return super::phases::compile(&env, &source, directory, &stages);
		}
		phase = "frozen phase import/replay";
		super::execution::replay(&env, &readout_comm, source, directory, &stages)
	})();
	let (mut status, mut payload, mut error) = match result {
		Ok(v) => {
			let status = v
				.get("status")
				.and_then(Value::as_str)
				.unwrap_or("completed")
				.to_owned();
			let error = v.get("error").and_then(Value::as_str).map(str::to_owned);
			(status, Some(v), error)
		}
		Err(e) => {
			if let Some(e) = e.downcast_ref::<StageFailure>() {
				phase = e.phase;
				stages = e.partial.clone();
			}
			("rejected".to_owned(), None, Some(e.to_string()))
		}
	};
	let admission = local(&comm, || {
		admit_json_overlap(
			&[payload.as_ref().unwrap_or(&Value::Null), &stages],
			&[1, 1],
			262_144 + 65_536,
			policy::READOUT_BYTES,
		)
	});
	if let Err(e) = admission {
		let progress = local(&comm, || {
			compact_progress(payload.as_ref().unwrap_or(&Value::Null))
		})?;
		let stage_progress = local(&comm, || compact_progress(&stages))?;
		payload = None;
		stages = json!({"discarded_oversize_receipt":true,"progress":progress,"stage_progress":stage_progress});
		status = "rejected".into();
		phase = "receipt admission";
		error = Some(e.to_string());
	}

	let row = local(&comm, || {
		let mut row = json!({"schema":"quest-persisted-weighted-transform-row-v1","mode":mode,"world_rank":world.rank()?,"world_parts":world.size()?,"rank":rank,"parts":parts,"status":status,"phase":phase,"error":error,"stages":null,"result":null,"fixed":{"dimension":32,"origin_ranks":8,"buckets":8,"register_qubits":10,"targets":policy::targets(32)?,"selectors":[2,3],"response":4,"weights":policy::WEIGHTS,"rank_bytes":policy::RANK_BYTES,"node_bytes":policy::NODE_BYTES,"load_bytes":policy::LOAD_BYTES,"conversion_bytes":policy::BRIDGE_BYTES,"compile_bytes":policy::COMPILE_BYTES,"readout_bytes":policy::READOUT_BYTES,"readout_work":policy::READOUT_WORK,"reciprocal_tolerance":policy::TOLERANCE,"max_degree":policy::MAX_DEGREE,"direct_residual_target":1e-3,"relative_vector_target":2e-3},"claims":{"multihost_capacity":false,"uniform_native_error_known":false,"rigorous_inverse_certificate":false},"memory":memory()?,"environment_accounted_bytes":env.view().allocated_bytes()});
		move_row_owners(&mut row, stages, payload)?;
		Ok(row)
	})?;
	let mut buffer = local(&comm, || {
		let mut b = Vec::new();
		b.try_reserve_exact(131_072)?;
		if b.capacity() > 262_144 {
			return Err("receipt actual capacity".into());
		}
		admit_json_overlap(&[&row], &[1], b.capacity() + 65_536, policy::READOUT_BYTES)?;
		b.resize(131_072, 0);
		Ok(b)
	})?;
	let mut writer = std::io::Cursor::new(buffer.as_mut_slice());
	let end = local(&comm, || {
		serde_json::to_writer(&mut writer, &row)?;
		Ok(usize::try_from(writer.position())?)
	})?;
	let bytes = buffer.get(..end).ok_or("receipt length")?;
	let world_rank = world.rank()?;
	let world_parts = world.size()?;
	let filename = format!("receipt-{mode}-world{world_parts}-rank{world_rank}.json");
	let digest = local(&comm, || {
		let path = directory.join(&filename);
		let temp = directory.join(format!(".{filename}.tmp"));
		if path.exists() {
			return Err("immutable receipt already exists".into());
		}
		let mut f = std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(&temp)?;
		std::io::Write::write_all(&mut f, bytes)?;
		f.sync_all()?;
		std::fs::rename(temp, path)?;
		Ok(hash(bytes))
	})?;
	let mut line = local(&comm, || {
		Ok(serde_json::to_vec(
			&json!({"schema":"quest-persisted-weighted-transform-index-v1","mode":mode,"world_rank":world_rank,"world_parts":world_parts,"rank":rank,"parts":parts,"status":status,"receipt":filename,"receipt_sha256":digest,"receipt_bytes":bytes.len()}),
		)?)
	})?;
	if line.len() > 1024 {
		return Err("receipt index cap".into());
	}
	line.push(b'\n');
	std::io::Write::write_all(&mut std::io::stdout().lock(), &line)?;

	drop(io_guard);
	Ok(())
}
