use googletest::prelude::*;
use quest_numerics::{SparseLimits, SparseMatrix};
use quest_qsvt::{MatchingEncoding, MatchingHeader, MatchingShard, NumericalPolicy};
use quest_qsvt_io::{
	Complex64,
	sharded_matching::{MatchingManifest, PersistedMatchingRecord, ShardIoLimits, write_bucket},
};

fn fixture() -> googletest::Result<(MatchingHeader, Vec<PersistedMatchingRecord>)> {
	let matrix = SparseMatrix::builder(4, 4)
		.entries(
			vec![Complex64::new(0.25, -0.5), Complex64::new(-0.75, 0.0)],
			vec![1, 3],
			vec![0, 1, 1, 2, 2],
		)
		.build(SparseLimits::default())?;
	let encoding = MatchingEncoding::from_sparse(&matrix, NumericalPolicy::default())?;
	let header = MatchingHeader::from_encoding(&encoding)?;
	let shard = MatchingShard::from_encoding(&encoding, 0, 1, NumericalPolicy::default())?;
	let records = shard
		.records()
		.iter()
		.map(|&column| {
			let edge = encoding
				.matchings()
				.get(column.color)
				.and_then(|m| m.entry(column.source));
			PersistedMatchingRecord {
				column,
				theta: edge.map_or(std::f64::consts::PI, |e| e.theta),
				phase_angle: edge.map_or(0.0, |e| e.phase),
				is_edge: edge.is_some(),
			}
		})
		.collect();
	Ok((header, records))
}

#[gtest]
fn sharded_hdf5_roundtrip_preserves_frozen_angles_and_reverse_chunks() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	let limits = ShardIoLimits {
		chunk_records: 1,
		..Default::default()
	};
	let mut receipts = Vec::new();
	for bucket in 0..2 {
		receipts.push(write_bucket(
			directory.path(),
			header,
			bucket,
			2,
			records
				.iter()
				.copied()
				.filter(|r| r.column.source % 2 == bucket)
				.map(Ok),
			limits,
		)?);
	}
	let manifest = MatchingManifest::new(header, receipts, limits)?;
	manifest.publish(directory.path().join("manifest.json"), limits)?;
	let manifest = MatchingManifest::open(directory.path().join("manifest.json"), limits)?;
	for bucket in 0..2 {
		let input = manifest.open_bucket(directory.path(), bucket, limits)?;
		let mut actual = Vec::new();
		input.visit_records(false, |chunk| {
			actual.extend_from_slice(chunk);
			Ok(())
		})?;
		let expected: Vec<_> = records
			.iter()
			.copied()
			.filter(|r| r.column.source % 2 == bucket)
			.collect();
		verify_that!(actual, eq(&expected))?;
		let mut reversed = Vec::new();
		input.visit_records(true, |chunk| {
			reversed.extend_from_slice(chunk);
			Ok(())
		})?;
		verify_that!(
			reversed,
			eq(&expected.into_iter().rev().collect::<Vec<_>>())
		)?;
	}
	Ok(())
}

#[gtest]
fn sharded_manifest_rejects_incomplete_coverage_and_changed_files() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	let limits = ShardIoLimits::default();
	let first = write_bucket(
		directory.path(),
		header,
		0,
		2,
		records
			.iter()
			.copied()
			.filter(|r| r.column.source % 2 == 0)
			.map(Ok),
		limits,
	)?;
	verify_that!(
		MatchingManifest::new(header, vec![first.clone()], limits).is_err(),
		eq(true)
	)?;
	let second = write_bucket(
		directory.path(),
		header,
		1,
		2,
		records
			.iter()
			.copied()
			.filter(|r| r.column.source % 2 == 1)
			.map(Ok),
		limits,
	)?;
	let manifest = MatchingManifest::new(header, vec![first, second], limits)?;
	let receipt = manifest
		.buckets()
		.first()
		.ok_or_else(|| std::io::Error::other("missing receipt"))?;
	std::fs::write(directory.path().join(&receipt.file), b"corrupt")?;
	verify_that!(
		manifest.open_bucket(directory.path(), 0, limits).is_err(),
		eq(true)
	)?;
	Ok(())
}

#[gtest]
fn shard_writer_rejects_wrong_owner_and_does_not_overwrite() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	let record = *records
		.first()
		.ok_or_else(|| std::io::Error::other("missing record"))?;
	let limits = ShardIoLimits::default();
	verify_that!(
		write_bucket(
			directory.path(),
			header,
			(record.column.source ^ 1) % 2,
			2,
			[Ok(record)],
			limits
		)
		.is_err(),
		eq(true)
	)?;
	let receipt = write_bucket(
		directory.path(),
		header,
		0,
		1,
		records.iter().copied().map(Ok),
		limits,
	)?;
	let before = std::fs::read(directory.path().join(&receipt.file))?;
	verify_that!(
		write_bucket(
			directory.path(),
			header,
			0,
			1,
			records.iter().copied().map(Ok),
			limits
		)
		.is_err(),
		eq(true)
	)?;
	verify_that!(
		std::fs::read(directory.path().join(&receipt.file))?,
		eq(&before)
	)?;
	verify_that!(std::fs::read_dir(directory.path())?.count(), eq(1))?;
	Ok(())
}

#[gtest]
fn opened_bucket_owns_snapshot_and_rejects_incompatible_repartition() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	let limits = ShardIoLimits::default();
	let receipt = write_bucket(
		directory.path(),
		header,
		0,
		1,
		records.iter().copied().map(Ok),
		limits,
	)?;
	let path = directory.path().join(&receipt.file);
	let manifest = MatchingManifest::new(header, vec![receipt], limits)?;
	verify_that!(manifest.owned_buckets(0, 2).is_err(), eq(true))?;
	let input = manifest.open_bucket(directory.path(), 0, limits)?;
	std::fs::write(path, b"replaced after admission")?;
	let mut actual = Vec::new();
	input.visit_records(false, |chunk| {
		actual.extend_from_slice(chunk);
		Ok(())
	})?;
	verify_that!(actual, eq(&records))?;
	Ok(())
}

#[gtest]
fn shard_admission_charges_chunk_buffers_and_frozen_angle_consistency() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	let tiny = ShardIoLimits {
		max_buffer_bytes: 1,
		..Default::default()
	};
	verify_that!(
		write_bucket(
			directory.path(),
			header,
			0,
			1,
			records.iter().copied().map(Ok),
			tiny
		)
		.is_err(),
		eq(true)
	)?;
	let mut bad = records;
	let edge = bad
		.iter_mut()
		.find(|r| r.is_edge)
		.ok_or_else(|| std::io::Error::other("missing edge"))?;
	edge.theta = 0.125;
	verify_that!(
		write_bucket(
			directory.path(),
			header,
			0,
			1,
			bad.into_iter().map(Ok),
			ShardIoLimits::default()
		)
		.is_err(),
		eq(true)
	)?;
	verify_that!(std::fs::read_dir(directory.path())?.count(), eq(0))?;
	Ok(())
}

#[cfg(target_os = "linux")]
#[gtest]
fn shard_writer_has_no_extra_descriptor_during_hdf5_writes() -> googletest::Result<()> {
	use std::os::unix::fs::MetadataExt;
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	let mut descriptor_counts = Vec::new();
	write_bucket(
		directory.path(),
		header,
		0,
		1,
		records.into_iter().map(|record| {
			let metadata = std::fs::read_dir(directory.path())?
				.next()
				.ok_or_else(|| std::io::Error::other("missing temporary bucket"))??
				.metadata()?;
			let descriptors = std::fs::read_dir("/proc/self/fd")?
				.filter_map(std::result::Result::ok)
				.filter_map(|entry| std::fs::metadata(entry.path()).ok())
				.filter(|target| target.dev() == metadata.dev() && target.ino() == metadata.ino())
				.count();
			descriptor_counts.push(descriptors);
			Ok(record)
		}),
		ShardIoLimits::default(),
	)?;
	// A second descriptor retained by the path guard would still be open when
	// that guard unlinks a rejected bucket, causing an NFS silly-rename.
	verify_that!(descriptor_counts, each(eq(&1)))?;
	Ok(())
}

#[gtest]
fn rejected_and_closed_snapshots_leave_no_temporary_files() -> googletest::Result<()> {
	const CHILD: &str = "QUEST_SHARD_CLEANUP_TMPDIR";
	let Ok(temporary_directory) = std::env::var(CHILD) else {
		let directory = tempfile::tempdir()?;
		let output = std::process::Command::new(std::env::current_exe()?)
			.args([
				"--exact",
				"rejected_and_closed_snapshots_leave_no_temporary_files",
				"--test-threads=1",
			])
			.env(CHILD, directory.path())
			.env("TMPDIR", directory.path())
			.env("TMP", directory.path())
			.env("TEMP", directory.path())
			.output()?;
		if !output.status.success() {
			return Err(std::io::Error::other(format!(
				"{} {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			))
			.into());
		}
		return Ok(());
	};
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	let limits = ShardIoLimits::default();
	let receipt = write_bucket(
		directory.path(),
		header,
		0,
		1,
		records.into_iter().map(Ok),
		limits,
	)?;
	let manifest = MatchingManifest::new(header, vec![receipt.clone()], limits)?;
	let manifest_path = directory.path().join("manifest.json");
	manifest.publish(&manifest_path, limits)?;
	let published = std::fs::read(&manifest_path)?;
	verify_that!(manifest.publish(&manifest_path, limits).is_err(), eq(true))?;
	verify_that!(std::fs::read(&manifest_path)?, eq(&published))?;
	verify_that!(std::fs::read_dir(directory.path())?.count(), eq(2))?;
	drop(manifest.open_bucket(directory.path(), 0, limits)?);
	verify_that!(std::fs::read_dir(&temporary_directory)?.count(), eq(1))?;
	for corrupt_bytes in [false, true] {
		let mut corrupted = receipt.clone();
		if corrupt_bytes {
			corrupted.sha256[0] ^= 1;
		} else {
			corrupted.semantic_sha256[0] ^= 1;
		}
		let manifest = MatchingManifest::new(header, vec![corrupted], limits)?;
		verify_that!(
			manifest.open_bucket(directory.path(), 0, limits).is_err(),
			eq(true)
		)?;
		verify_that!(std::fs::read_dir(&temporary_directory)?.count(), eq(1))?;
	}
	drop(directory);
	verify_that!(std::fs::read_dir(&temporary_directory)?.count(), eq(0))?;
	Ok(())
}

#[gtest]
fn manifest_bucket_budget_precedes_decoding_an_excess_receipt() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	let limits = ShardIoLimits::default();
	let receipt = write_bucket(
		directory.path(),
		header,
		0,
		1,
		records.into_iter().map(Ok),
		limits,
	)?;
	let manifest = MatchingManifest::new(header, vec![receipt], limits)?;
	let mut json = serde_json::to_value(manifest)?;
	json.get_mut("buckets")
		.and_then(serde_json::Value::as_array_mut)
		.ok_or_else(|| std::io::Error::other("missing bucket array"))?
		.push(serde_json::Value::String("invalid receipt".into()));
	let path = directory.path().join("oversized.json");
	std::fs::write(&path, serde_json::to_vec(&json)?)?;
	let error = MatchingManifest::open(
		path,
		ShardIoLimits {
			max_buckets: 1,
			..limits
		},
	)
	.err()
	.ok_or_else(|| std::io::Error::other("expected bucket budget rejection"))?;
	verify_that!(
		error.to_string(),
		contains_substring("bucket metadata admission")
	)?;
	Ok(())
}

#[gtest]
fn manifest_charges_encoded_and_decoded_storage_together() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	let limits = ShardIoLimits::default();
	let receipt = write_bucket(
		directory.path(),
		header,
		0,
		1,
		records.into_iter().map(Ok),
		limits,
	)?;
	let manifest = MatchingManifest::new(header, vec![receipt], limits)?;
	let path = directory.path().join("manifest.json");
	let bytes = serde_json::to_vec(&manifest)?;
	std::fs::write(&path, &bytes)?;
	let error = MatchingManifest::open(
		&path,
		ShardIoLimits {
			max_manifest_bytes: bytes
				.len()
				.checked_add(1)
				.and_then(|n| n.checked_add(size_of::<MatchingManifest>()))
				.ok_or_else(|| std::io::Error::other("test metadata limit overflow"))?,
			..limits
		},
	)
	.err()
	.ok_or_else(|| std::io::Error::other("expected concurrent storage rejection"))?;
	verify_that!(
		error.to_string(),
		contains_substring("concurrent metadata allocation")
	)?;
	verify_that!(MatchingManifest::open(&path, limits)?.header()?, eq(header))?;
	Ok(())
}

#[gtest]
fn shard_file_admission_reserves_header_and_final_partial_chunk() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	for max_file_bytes in [1, 65_536] {
		let limits = ShardIoLimits {
			max_file_bytes,
			chunk_records: 4096,
			..Default::default()
		};
		let result = write_bucket(
			directory.path(),
			header,
			0,
			1,
			records.iter().copied().map(Ok),
			limits,
		);
		let error = result
			.err()
			.ok_or_else(|| std::io::Error::other("expected file reservation rejection"))?;
		verify_that!(
			error.to_string(),
			contains_substring("HDF5 file reservation")
		)?;
		verify_that!(std::fs::read_dir(directory.path())?.count(), eq(0))?;
	}
	Ok(())
}

#[gtest]
fn manifest_rejects_large_filename_before_copying_it() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	let limits = ShardIoLimits::default();
	let receipt = write_bucket(
		directory.path(),
		header,
		0,
		1,
		records.into_iter().map(Ok),
		limits,
	)?;
	let manifest = MatchingManifest::new(header, vec![receipt], limits)?;
	let mut json = serde_json::to_value(manifest)?;
	json["buckets"][0]["file"] = serde_json::Value::String("x".repeat(64 * 1024));
	let path = directory.path().join("long-filename.json");
	std::fs::write(&path, serde_json::to_vec(&json)?)?;
	let error = MatchingManifest::open(&path, limits)
		.err()
		.ok_or_else(|| std::io::Error::other("expected filename rejection"))?;
	verify_that!(error.to_string(), contains_substring("filename admission"))?;
	Ok(())
}

#[gtest]
fn manifest_rejects_escaped_strings_before_serde_scratch_allocation() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	let limits = ShardIoLimits::default();
	let receipt = write_bucket(
		directory.path(),
		header,
		0,
		1,
		records.into_iter().map(Ok),
		limits,
	)?;
	let manifest = MatchingManifest::new(header, vec![receipt], limits)?;
	let json = serde_json::to_string(&manifest)?;
	for mutated in [
		json.replace("schema_version", r"schema\u005fversion"),
		json.replace(
			"matching-0000000000000000.h5",
			&format!(r"\u006d{}", "x".repeat(64 * 1024)),
		),
		json.replace("construction", r"construc\u0074ion"),
	] {
		let path = directory.path().join("escaped.json");
		std::fs::write(&path, mutated)?;
		let error = MatchingManifest::open(&path, limits)
			.err()
			.ok_or_else(|| std::io::Error::other("expected canonical string rejection"))?;
		verify_that!(
			error.to_string(),
			contains_substring("unescaped ASCII strings")
		)?;
	}
	Ok(())
}

#[gtest]
fn obsolete_additive_fnv_manifest_version_is_rejected() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let (header, records) = fixture()?;
	let limits = ShardIoLimits::default();
	let bucket = write_bucket(
		directory.path(),
		header,
		0,
		1,
		records.into_iter().map(Ok),
		limits,
	)?;
	let manifest = MatchingManifest::new(header, vec![bucket], limits)?;
	let path = directory.path().join("manifest.json");
	manifest.publish(&path, limits)?;
	let mut json: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
	json["schema_version"] = serde_json::json!(1);
	json["construction"] = serde_json::json!("completed-matching-columns-v1");
	std::fs::write(&path, serde_json::to_vec(&json)?)?;
	verify_that!(MatchingManifest::open(&path, limits).is_err(), eq(true))?;
	Ok(())
}
