//! Bounded on-disk source shards and observation of the actual MPI node group.
use quest_numerics::sparse_stream::SparseEntry;
use sha2::{Digest, Sha256};
use std::{
	fs::{File, OpenOptions},
	io::{BufReader, BufWriter, Read, Write},
	path::Path,
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, Ordering},
	},
	thread::JoinHandle,
	time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn native_array_payload(
	input: quest::RegisterDeployment,
	scratch: Option<quest::RegisterDeployment>,
) -> Result<serde_json::Value> {
	let host = input
		.host_array_bytes()
		.checked_add(scratch.map_or(0, quest::RegisterDeployment::host_array_bytes))
		.ok_or("native host array payload overflow")?;
	let device = input
		.device_array_bytes()
		.checked_add(scratch.map_or(0, quest::RegisterDeployment::device_array_bytes))
		.ok_or("native device array payload overflow")?;
	Ok(serde_json::json!({
		"scope":"native array payload only; excludes allocator, MPI, OpenMP stacks and other temporaries",
		"input":{"host_bytes":input.host_array_bytes(),"device_bytes":input.device_array_bytes()},
		"scratch":scratch.map(|deployment| serde_json::json!({"host_bytes":deployment.host_array_bytes(),"device_bytes":deployment.device_array_bytes()})),
		"scratch_mode":if scratch.is_some() { "owned-register" } else { "borrowed-input-communication-buffer" },
		"total_host_bytes":host,"total_device_bytes":device
	}))
}

pub fn load_statistics(
	stats: quest::qsvt::persisted_matching::LoadStatistics,
) -> serde_json::Value {
	serde_json::json!({
		"phase_seconds":{
			"manifest_and_admission":stats.manifest_and_admission.as_secs_f64(),
			"read_validate":stats.read_validate.as_secs_f64(),
			"reverse_directory":stats.reverse_directory.as_secs_f64(),
			"replay_admission":stats.replay_admission.as_secs_f64(),
			"total":stats.total.as_secs_f64()
		},
		"local_records":stats.local_records,"local_reverse_records":stats.local_reverse_records,
		"local_record_capacity":stats.local_record_capacity,"local_reverse_capacity":stats.local_reverse_capacity,
		"reverse_broadcasts":stats.reverse_broadcasts,"replay_admitted":stats.replay_admitted,
		"admission":{
			"next_record_calls":stats.admission.next_record_calls,"forward_calls":stats.admission.forward_calls,
			"reverse_calls":stats.admission.reverse_calls,"broadcasts":stats.admission.broadcasts
		}
	})
}

pub fn source(path: &Path, n: usize, rank: usize, parts: usize) -> Result<(Source, String, usize)> {
	let entries = (rank..n)
		.step_by(parts)
		.count()
		.checked_mul(2)
		.ok_or("source count overflow")?;
	let bytes = entries.checked_mul(32).ok_or("source bytes overflow")?;
	let file = OpenOptions::new().write(true).create_new(true).open(path)?;
	let mut writer = BufWriter::with_capacity(65_536, file);
	let mut hash = Sha256::new();
	for column in (rank..n).step_by(parts) {
		for slot in 0..2 {
			let row = if slot == 0 { column } else { column ^ 1 };
			let (real, imaginary) = if slot == 0 {
				(0.7_f64, 0.1_f64)
			} else {
				(-0.2_f64, 0.05_f64)
			};
			let words = [
				u64::try_from(row)?,
				u64::try_from(column)?,
				real.to_bits(),
				imaginary.to_bits(),
			];
			for word in words {
				let encoded = word.to_le_bytes();
				writer.write_all(&encoded)?;
				hash.update(encoded);
			}
		}
	}
	writer.flush()?;
	writer.get_ref().sync_all()?;
	if writer.get_ref().metadata()?.len() != u64::try_from(bytes)? {
		return Err("stored source length mismatch".into());
	}
	drop(writer);
	let file = File::open(path)?;
	let mut digest = String::with_capacity(64);
	for byte in hash.finalize() {
		use std::fmt::Write as _;
		write!(digest, "{byte:02x}")?;
	}
	Ok((
		Source {
			reader: BufReader::with_capacity(65_536, file),
			remaining: entries,
			rank,
			parts,
			index: 0,
		},
		digest,
		bytes,
	))
}

pub struct Source {
	reader: BufReader<File>,
	remaining: usize,
	rank: usize,
	parts: usize,
	index: usize,
}
impl Source {
	fn entry(&mut self) -> quest_numerics::Result<SparseEntry> {
		let mut words = [0_u64; 4];
		for word in &mut words {
			let mut bytes = [0_u8; 8];
			self.reader
				.read_exact(&mut bytes)
				.map_err(|_| quest_numerics::Error::Domain("reading owned source shard"))?;
			*word = u64::from_le_bytes(bytes);
		}
		let [row, column, real, imaginary] = words;
		let column = usize::try_from(column).map_err(|_| quest_numerics::Error::Overflow)?;
		let row = usize::try_from(row).map_err(|_| quest_numerics::Error::Overflow)?;
		let expected_column = (self.index / 2)
			.checked_mul(self.parts)
			.and_then(|c| c.checked_add(self.rank))
			.ok_or(quest_numerics::Error::Overflow)?;
		let (expected_row, re, im) = if self.index.is_multiple_of(2) {
			(expected_column, 0.7_f64, 0.1_f64)
		} else {
			(expected_column ^ 1, -0.2_f64, 0.05_f64)
		};
		if column != expected_column
			|| row != expected_row
			|| real != re.to_bits()
			|| imaginary != im.to_bits()
		{
			return Err(quest_numerics::Error::Domain(
				"source shard ownership or value changed",
			));
		}
		let ordinal = column
			.checked_mul(2)
			.and_then(|n| n.checked_add(self.index % 2))
			.ok_or(quest_numerics::Error::Overflow)?;
		self.index = self
			.index
			.checked_add(1)
			.ok_or(quest_numerics::Error::Overflow)?;
		Ok(SparseEntry {
			row,
			column,
			ordinal: u64::try_from(ordinal).map_err(|_| quest_numerics::Error::Overflow)?,
			value: quest::Complex64::new(f64::from_bits(real), f64::from_bits(imaginary)),
		})
	}
}
impl Iterator for Source {
	type Item = quest_numerics::Result<SparseEntry>;
	fn next(&mut self) -> Option<Self::Item> {
		if self.remaining == 0 {
			return None;
		}
		self.remaining -= 1;
		Some(self.entry())
	}
}

pub fn process_threads() -> Result<usize> {
	let status = std::fs::read_to_string("/proc/self/status")?;
	Ok(status
		.lines()
		.find_map(|line| line.strip_prefix("Threads:"))
		.and_then(|line| line.split_whitespace().next())
		.ok_or("process thread counter")?
		.parse()?)
}

#[derive(Default)]
struct Samples {
	count: usize,
	rss: usize,
	address: usize,
	span: f64,
	failure: Option<String>,
}
pub struct NodeSampler {
	stop: Arc<AtomicBool>,
	thread: Option<JoinHandle<()>>,
	data: Arc<Mutex<Samples>>,
	pids: usize,
}
fn sample(pids: &[u32]) -> Result<(usize, usize, f64)> {
	let start = Instant::now();
	let mut rss = 0_usize;
	let mut address = 0_usize;
	for pid in pids {
		let status = std::fs::read_to_string(format!("/proc/{pid}/status"))?;
		let counter = |prefix| -> Result<usize> {
			let value = status
				.lines()
				.find_map(|line| line.strip_prefix(prefix))
				.and_then(|line| line.split_whitespace().next())
				.ok_or("node process counter")?;
			Ok(value
				.parse::<usize>()?
				.checked_mul(1024)
				.ok_or("node counter overflow")?)
		};
		rss = rss
			.checked_add(counter("VmRSS:")?)
			.ok_or("node RSS overflow")?;
		address = address
			.checked_add(counter("VmSize:")?)
			.ok_or("node address overflow")?;
	}
	Ok((rss, address, start.elapsed().as_secs_f64()))
}
impl NodeSampler {
	pub fn start(pids: Vec<u32>) -> Result<Self> {
		if pids.is_empty() || pids.len() > 32 {
			return Err("bounded node PID coverage".into());
		}
		let count = pids.len();
		let data = Arc::new(Mutex::new(Samples::default()));
		let stop = Arc::new(AtomicBool::new(false));
		let thread_data = Arc::clone(&data);
		let thread_stop = Arc::clone(&stop);
		let thread = std::thread::Builder::new()
			.name("node-memory-sampler".into())
			.stack_size(262_144)
			.spawn(move || {
				loop {
					let observation = sample(&pids);
					let Ok(mut values) = thread_data.lock() else {
						break;
					};
					match observation {
						Ok((rss, address, span)) => {
							values.count = values.count.saturating_add(1);
							values.rss = values.rss.max(rss);
							values.address = values.address.max(address);
							values.span = values.span.max(span);
						}
						Err(error) => values.failure = Some(error.to_string()),
					}
					drop(values);
					if thread_stop.load(Ordering::Acquire) {
						break;
					}
					std::thread::sleep(Duration::from_millis(20));
				}
			})?;
		Ok(Self {
			stop,
			thread: Some(thread),
			data,
			pids: count,
		})
	}
	pub fn finish(mut self) -> Result<serde_json::Value> {
		self.stop.store(true, Ordering::Release);
		self.thread
			.take()
			.ok_or("missing node sampler")?
			.join()
			.map_err(|_| "node sampler panicked")?;
		let values = self.data.lock().map_err(|_| "node sample lock")?;
		if let Some(error) = &values.failure {
			return Err(error.clone().into());
		}
		let receipt = serde_json::json!({"samples":values.count,"maximum_sum_rss_bytes":values.rss,
			"maximum_sum_address_space_bytes":values.address,"max_sample_span_seconds":values.span,
			"interval_milliseconds":20,"pids":self.pids});
		drop(values);
		Ok(receipt)
	}
}
impl Drop for NodeSampler {
	fn drop(&mut self) {
		self.stop.store(true, Ordering::Release);
		if let Some(thread) = self.thread.take() {
			let _result = thread.join();
		}
	}
}
