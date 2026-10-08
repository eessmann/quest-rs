//! Explicit real-buffer transport probe; run under a bounded MPI supervisor.
use quest_sys::mpi::MpiRuntime;
use std::{fs, path::PathBuf, time::Instant};

type Error = Box<dyn std::error::Error>;

fn pattern(rank: u64, offset: usize) -> u8 {
	let mut word =
		u64::try_from(offset).unwrap_or_default() ^ rank.wrapping_mul(0x9e37_79b9_7f4a_7c15);
	word = (word ^ (word >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
	word = (word ^ (word >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
	(word ^ (word >> 31)).to_le_bytes()[0]
}

fn proc_bytes(path: &str, key: &str) -> Result<u64, Error> {
	let text = fs::read_to_string(path)?;
	let value = text
		.lines()
		.find_map(|line| {
			let mut words = line.split_whitespace();
			(words.next()? == key).then(|| words.next()).flatten()
		})
		.ok_or("missing /proc memory field")?;
	Ok(value
		.parse::<u64>()?
		.checked_mul(1024)
		.ok_or("memory overflow")?)
}

#[allow(
	clippy::too_many_lines,
	reason = "Keep collective probe phases and their evidence in matching visible order"
)]
fn run() -> Result<(), Error> {
	let mut arguments = std::env::args().skip(1);
	let bytes: usize = arguments
		.next()
		.ok_or("expected logical byte count")?
		.parse()?;
	let directory = PathBuf::from(arguments.next().ok_or("expected receipt directory")?);
	let require_multihost = match arguments.next().as_deref() {
		None => false,
		Some("--require-multihost") => true,
		_ => return Err("unknown probe option".into()),
	};
	if arguments.next().is_some() || bytes == 0 {
		return Err("invalid probe arguments".into());
	}
	let maximum_frame = usize::try_from(i32::MAX)?;
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	if !world.all_agree(world.size()? == 2)? {
		return Err("probe requires exactly two MPI ranks".into());
	}
	let rank = world.rank()?;
	let peer = rank ^ 1;
	let topology = world.shared_memory_topology()?;
	if !world.all_agree(!require_multihost || topology.local_size == 1)? {
		return Err("probe requires one MPI rank per physical node".into());
	}
	let logical_bytes = u64::try_from(bytes)?;
	let cap = 8_u64 << 30;
	let limit = rustix::process::Rlimit {
		current: Some(cap),
		maximum: Some(cap),
	};
	let limited = rustix::process::setrlimit(rustix::process::Resource::As, limit).is_ok();
	let available = proc_bytes("/proc/meminfo", "MemAvailable:");
	let baseline_as = proc_bytes("/proc/self/status", "VmSize:");
	let budget_ok = baseline_as
		.as_ref()
		.is_ok_and(|&n| n.saturating_add(logical_bytes).saturating_add(1 << 30) < cap)
		&& available
			.as_ref()
			.is_ok_and(|&n| n > logical_bytes.saturating_add(1 << 30));
	if !world.all_agree(limited && budget_ok)? {
		return Err("collective process memory admission rejected".into());
	}
	let mut buffer = Vec::new();
	let allocation = buffer.try_reserve_exact(bytes);
	if !world.all_agree(allocation.is_ok())? {
		return Err("collective logical-buffer allocation rejected".into());
	}
	let allocated_capacity = buffer.capacity();
	let allocation_start = Instant::now();
	buffer.resize(bytes, 0_u8);
	let touch_seconds = allocation_start.elapsed().as_secs_f64();
	if !world.all_agree(buffer.len() == bytes)? {
		return Err("collective touched-buffer admission rejected".into());
	}
	let mut rounds = Vec::new();
	for sender in 0..2_i32 {
		let prepare = Instant::now();
		if rank == sender {
			for (offset, byte) in buffer.iter_mut().enumerate() {
				*byte = pattern(sender.to_le_bytes()[0].into(), offset);
			}
		} else {
			buffer.fill(0xff);
		}
		let pattern_seconds = prepare.elapsed().as_secs_f64();
		world.all_agree(true)?;
		let start = Instant::now();
		let received = {
			let mut lane = world.collective_lane()?;
			if rank == sender {
				lane.send_receive_bytes_chunked(&buffer, peer, 27100, &mut [])?
			} else {
				lane.send_receive_bytes_chunked(&[], peer, 27100, &mut buffer)?
			}
		};
		let transport_seconds = start.elapsed().as_secs_f64();
		let verify = Instant::now();
		let expected_count = if rank == sender { 0 } else { bytes };
		let mismatch = buffer.iter().enumerate().find_map(|(offset, &byte)| {
			(byte != pattern(sender.to_le_bytes()[0].into(), offset)).then_some(offset)
		});
		let correct = received == expected_count && mismatch.is_none();
		let verify_seconds = verify.elapsed().as_secs_f64();
		if !world.all_agree(correct)? {
			return Err(format!("payload verification failed at {mismatch:?}").into());
		}
		rounds.push(serde_json::json!({"sender":sender,"sent_bytes":if rank==sender {bytes}else{0},"received_bytes":received,"native_frames":bytes.div_ceil(maximum_frame),"first_frame_bytes":bytes.min(maximum_frame),"last_frame_bytes":bytes.saturating_sub(1).checked_rem(maximum_frame).ok_or("zero frame")?.saturating_add(1),"pattern_seconds":pattern_seconds,"transport_seconds":transport_seconds,"verify_seconds":verify_seconds,"every_byte_verified":true}));
	}
	let receipt = serde_json::json!({"schema":"quest-mpi-large-count-v1","scope":"checked chunked logical-byte transport; no matching-kernel or beyond-memory capacity claim","rank":rank,"ranks":2,"processor_name":topology.processor_name,"shared_memory_local_size":topology.local_size,"shared_memory_leader_rank":topology.leader_rank,"logical_bytes":bytes,"exceeds_i32_max":bytes>maximum_frame,"allocated_capacity":allocated_capacity,"process_as_cap_bytes":cap,"baseline_address_space_bytes":baseline_as?,"mem_available_before_bytes":available?,"touch_seconds":touch_seconds,"peak_rss_bytes":proc_bytes("/proc/self/status","VmHWM:")?,"peak_as_bytes":proc_bytes("/proc/self/status","VmPeak:")?,"mpi_thread_multiple":runtime.thread_multiple(),"omp_num_threads":std::env::var("OMP_NUM_THREADS").ok(),"rounds":rounds,"complete":true});
	let publication = fs::create_dir_all(&directory).and_then(|()| {
		fs::write(
			directory.join(format!("rank-{rank}.json")),
			serde_json::to_vec_pretty(&receipt)?,
		)
	});
	if !world.all_agree(publication.is_ok())? {
		return Err("collective receipt publication failed".into());
	}
	println!("rank={rank} logical_bytes={bytes} complete=true");
	Ok(())
}

fn main() -> Result<(), Error> {
	run()
}
