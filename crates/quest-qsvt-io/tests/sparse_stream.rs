use quest_numerics::{
	Complex64,
	sparse_stream::{SparseEntry, StreamBuilder, StreamLimits},
};
use quest_qsvt_io::sparse_stream::FileRunStore;

#[test]
#[allow(
	clippy::panic_in_result_fn,
	reason = "Assert retained path capacity is admitted"
)]
fn scratch_store_charges_retained_path_capacity() -> Result<(), Box<dyn std::error::Error>> {
	let directory = tempfile::tempdir()?;
	let mut path = std::path::PathBuf::with_capacity(1_048_576);
	path.push(directory.path());
	let result = StreamBuilder::with_store(
		1,
		1,
		StreamLimits {
			buffer_entries: 1,
			max_bytes: 65_536,
			..StreamLimits::default()
		},
		FileRunStore::new(path),
	);
	assert!(result.is_err());
	assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
	Ok(())
}
#[test]
#[allow(
	clippy::panic_in_result_fn,
	reason = "Assertions express regression outcomes after fallible setup"
)]
fn admitted_external_runs_merge_and_clean_up() -> Result<(), Box<dyn std::error::Error>> {
	let directory = tempfile::tempdir()?;
	let limits = StreamLimits {
		buffer_entries: 2,
		max_spill_bytes: 4096,
		..StreamLimits::default()
	};
	let mut builder =
		StreamBuilder::with_store(2, 3, limits, FileRunStore::new(directory.path().to_owned()))?;
	for (row, column, ordinal, re) in [
		(1, 2, 9, -1e16),
		(0, 1, 2, -3.0),
		(1, 2, 10, 1.0),
		(1, 2, 8, 1e16),
		(0, 1, 1, 3.0),
	] {
		builder.push(SparseEntry {
			row,
			column,
			ordinal,
			value: Complex64::new(re, 0.0),
		})?;
	}
	let sorted = builder.finish()?;
	assert_eq!(std::fs::read_dir(directory.path())?.count(), 1);
	let entries = sorted.collect::<quest_numerics::Result<Vec<_>>>()?;
	assert_eq!(
		entries,
		vec![SparseEntry {
			row: 1,
			column: 2,
			ordinal: 8,
			value: Complex64::new(1.0, 0.0)
		}]
	);
	assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
	let limits = StreamLimits {
		buffer_entries: 1,
		max_spill_bytes: 40,
		..StreamLimits::default()
	};
	let mut builder =
		StreamBuilder::with_store(1, 1, limits, FileRunStore::new(directory.path().to_owned()))?;
	builder.push(SparseEntry {
		row: 0,
		column: 0,
		ordinal: 0,
		value: Complex64::new(1.0, 0.0),
	})?;
	builder.push(SparseEntry {
		row: 0,
		column: 0,
		ordinal: 1,
		value: Complex64::new(1.0, 0.0),
	})?;
	assert!(builder.finish().is_err());
	assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
	Ok(())
}
#[test]
#[allow(
	clippy::panic_in_result_fn,
	reason = "Admission and immutable metadata regression assertions"
)]
fn file_owner_capacity_is_available_before_any_record_write()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_numerics::sparse_stream::{RunReader, RunStore};
	let directory = tempfile::tempdir()?;
	let mut store = FileRunStore::new(directory.path().to_owned());
	let mut writer = store.create()?;
	let bytes = writer.retained_bytes()?;
	assert!(bytes >= directory.path().as_os_str().len());
	let entry = SparseEntry {
		row: 0,
		column: 0,
		ordinal: 7,
		value: Complex64::new(0.5, -0.2),
	};
	store.write(&mut writer, entry)?;
	assert_eq!(writer.retained_bytes()?, bytes);
	let file = store.finish(writer)?;
	assert!(file.retained_bytes()? <= bytes);
	let mut reader = store.open(&file)?;
	assert_eq!(reader.next_entry()?, Some(entry));
	assert_eq!(reader.next_entry()?, None);
	// Close the independent handle before unlinking the run, including on NFS.
	drop(reader);
	drop(file);
	assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
	Ok(())
}
