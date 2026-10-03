//! Explicit maintenance of the pinned, upstream `PennyLane` HDF5 snapshot.
use crate::generate::DynError;
use quest_qsvt_io::{CatalogSource, InverseCatalog, IoPolicy};
use std::{fs, io::Read, path::Path, time::Duration};
use ureq::ResponseExt;

pub fn run(fetch: bool) -> Result<(), DynError> {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.parent()
		.and_then(Path::parent)
		.ok_or("workspace directory")?;
	let directory = root.join("crates/quest-qsvt-io/data/pennylane");
	let source: CatalogSource = serde_json::from_slice(&fs::read(directory.join("source.json"))?)?;
	if source.file != "inverse.h5" {
		return Err("unexpected pinned catalog filename".into());
	}
	let path = directory.join(&source.file);
	let catalog = if fetch {
		let agent = ureq::Agent::config_builder()
			.https_only(true)
			.timeout_global(Some(Duration::from_secs(60)))
			.build()
			.new_agent();
		let mut response = agent.get(&source.download_url).call()?;
		if response.get_uri().to_string() != source.resolved_url {
			return Err("PennyLane download redirect differs from the pinned manifest".into());
		}
		publish_download(response.body_mut().as_reader(), &path, source)?
	} else {
		InverseCatalog::open(&path, source, IoPolicy::default())?
	};
	eprintln!(
		"{} {} PennyLane inverse catalog families; SHA-256 {}",
		if fetch { "Fetched" } else { "Checked" },
		catalog.families().len(),
		catalog.source().sha256
	);
	Ok(())
}

fn publish_download(
	reader: impl Read,
	destination: &Path,
	source: CatalogSource,
) -> Result<InverseCatalog, DynError> {
	let policy = IoPolicy::default();
	if source.size_bytes == 0 || source.size_bytes > u64::try_from(policy.max_bytes)? {
		return Err("catalog download exceeds its storage budget".into());
	}
	let parent = destination
		.parent()
		.ok_or("catalog destination directory")?;
	let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
	let limit = source
		.size_bytes
		.checked_add(1)
		.ok_or("catalog download size overflow")?;
	let received = std::io::copy(&mut reader.take(limit), &mut temporary)?;
	if received != source.size_bytes {
		return Err("catalog download size differs from the pinned manifest".into());
	}
	temporary.as_file().sync_all()?;
	let catalog = InverseCatalog::open(temporary.path(), source, policy)?;
	temporary.persist(destination)?;
	Ok(catalog)
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;
	use std::io::Cursor;

	fn source() -> Result<CatalogSource, DynError> {
		Ok(serde_json::from_str(include_str!(
			"../../quest-qsvt-io/data/pennylane/source.json"
		))?)
	}
	const DATA: &[u8] = include_bytes!("../../quest-qsvt-io/data/pennylane/inverse.h5");

	#[gtest]
	fn pinned_download_replaces_only_after_full_validation() -> googletest::Result<()> {
		let directory = tempfile::tempdir()?;
		let destination = directory.path().join("inverse.h5");
		fs::write(&destination, b"previous")?;
		let mut wrong = source().or_fail()?;
		wrong.sha256 = "00".repeat(32);
		expect_true!(publish_download(Cursor::new(DATA), &destination, wrong).is_err());
		expect_eq!(fs::read(&destination)?, b"previous".to_vec());
		expect_true!(
			publish_download(Cursor::new(b"truncated"), &destination, source().or_fail()?).is_err()
		);
		expect_eq!(fs::read(&destination)?, b"previous".to_vec());
		let mut oversized = DATA.to_vec();
		oversized.push(0);
		expect_true!(
			publish_download(Cursor::new(oversized), &destination, source().or_fail()?).is_err()
		);
		expect_eq!(fs::read(&destination)?, b"previous".to_vec());
		let catalog =
			publish_download(Cursor::new(DATA), &destination, source().or_fail()?).or_fail()?;
		expect_eq!(catalog.families().len(), 21);
		expect_eq!(fs::read(&destination)?, DATA.to_vec());
		expect_eq!(fs::read_dir(directory.path())?.count(), 1);
		Ok(())
	}

	#[gtest]
	fn failed_download_keeps_the_previous_file_and_cleans_staging() -> googletest::Result<()> {
		struct Broken;
		impl Read for Broken {
			fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
				Err(std::io::Error::other("interrupted download"))
			}
		}
		let directory = tempfile::tempdir()?;
		let destination = directory.path().join("inverse.h5");
		fs::write(&destination, b"previous")?;
		expect_true!(publish_download(Broken, &destination, source().or_fail()?).is_err());
		expect_eq!(fs::read(&destination)?, b"previous".to_vec());
		expect_eq!(fs::read_dir(directory.path())?.count(), 1);
		Ok(())
	}
}
