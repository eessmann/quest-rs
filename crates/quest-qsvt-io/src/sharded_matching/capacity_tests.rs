//! Test-only real excess capacities, each isolated in a fresh subprocess.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
static FILL: AtomicUsize = AtomicUsize::new(0);
pub(super) fn fill() {
	FILL.fetch_add(1, Ordering::Relaxed);
}
pub(super) fn inflate<T>(values: &mut Vec<T>, stage: &str) -> Result<()> {
	if std::env::var("QUEST_IO_CAPACITY_CHILD").as_deref() == Ok(stage) {
		values
			.try_reserve_exact(match stage {
				"receipts" => 512,
				"typed" | "disk" => 1024,
				_ => 32768,
			})
			.map_err(|_| Error::Budget("test capacity reserve"))?;
	}
	Ok(())
}
pub(super) fn inflate_string(text: &mut String) -> Result<()> {
	if std::env::var("QUEST_IO_CAPACITY_CHILD").as_deref() == Ok("string") {
		text.try_reserve_exact(32768)
			.map_err(|_| Error::Budget("test string capacity"))?;
	}
	Ok(())
}
#[test]
#[allow(
	clippy::panic_in_result_fn,
	reason = "Bounded subprocesses prove real-capacity rejection before Rust fill/conversion"
)]
fn read_capacity_admission_precedes_fill() -> std::result::Result<(), Box<dyn std::error::Error>> {
	if std::env::var("QUEST_IO_CAPACITY_CHILD").is_err() {
		for mode in ["manifest-input", "receipts", "string", "typed", "disk"] {
			let output = std::process::Command::new(std::env::current_exe()?)
				.args([
					"--exact",
					"sharded_matching::capacity_tests::read_capacity_admission_precedes_fill",
					"--nocapture",
					"--test-threads=1",
				])
				.env("QUEST_IO_CAPACITY_CHILD", mode)
				.output()?;
			assert!(
				output.status.success(),
				"{mode}: {} {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
		}
		return Ok(());
	}
	let directory = tempfile::tempdir()?;
	let path = directory.path().join("manifest.json");
	let limits = ShardIoLimits {
		chunk_records: 2,
		max_buffer_bytes: 32768,
		max_manifest_bytes: 32768,
		max_buckets: 1,
		max_records: 2,
		max_file_bytes: 1_048_576,
	};
	let header = MatchingHeader {
		rows: 2,
		cols: 2,
		system_qubits: 1,
		color_qubits: 0,
		num_colors: 1,
		beta: 1.,
		alpha: 1.,
		source_identity: 17,
		record_count: 2,
		record_digest: 0,
	};
	let records = (0..2).map(|i| {
		Ok(PersistedMatchingRecord {
			column: MatchingColumn {
				color: 0,
				source: i,
				destination: i,
				cosine: 1.,
				sine: 0.,
				phase: Complex64::new(1., 0.),
			},
			theta: 0.,
			phase_angle: 0.,
			is_edge: true,
		})
	});
	let receipt = write_bucket(directory.path(), header, 0, 1, records, limits)?;
	MatchingManifest::new(header, vec![receipt], limits)?.publish(&path, limits)?;
	let mode = std::env::var("QUEST_IO_CAPACITY_CHILD")?;
	if ["manifest-input", "receipts", "string"].contains(&mode.as_str()) {
		assert!(MatchingManifest::open(&path, limits).is_err());
		assert_eq!(
			FILL.load(Ordering::Relaxed),
			match mode.as_str() {
				"receipts" => 2,
				"string" => 1,
				_ => 0,
			}
		);
	} else {
		let manifest = MatchingManifest::open(&path, limits)?;
		FILL.store(0, Ordering::Relaxed);
		assert!(manifest.open_bucket(directory.path(), 0, limits).is_err());
		assert_eq!(FILL.load(Ordering::Relaxed), 0);
	}
	Ok(())
}
