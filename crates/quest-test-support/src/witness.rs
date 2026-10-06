//! Durable cross-rank evidence for expected-failure integration tests.
use std::{
	fs::OpenOptions,
	io::{self, Write},
	path::{Path, PathBuf},
	sync::atomic::{AtomicU64, Ordering},
};
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// A unique directory, removed when its coordinator finishes checking witnesses.
/// On multiple nodes, configure TMPDIR to a shared filesystem before the launch.
pub struct WitnessDirectory(PathBuf);
impl WitnessDirectory {
	/// # Errors
	/// Returns filesystem errors while creating a unique private directory.
	pub fn new() -> io::Result<Self> {
		for _ in 0..1024 {
			let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
			let path = std::env::temp_dir()
				.join(format!("quest-mpi-witness-{}-{serial}", std::process::id()));
			let mut builder = std::fs::DirBuilder::new();
			#[cfg(unix)]
			{
				use std::os::unix::fs::DirBuilderExt;
				builder.mode(0o700);
			}
			match builder.create(&path) {
				Ok(()) => return Ok(Self(path)),
				Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
				Err(error) => return Err(error),
			}
		}
		Err(io::Error::other(
			"could not create unique MPI witness directory",
		))
	}
	#[must_use]
	pub fn path(&self) -> &Path {
		&self.0
	}
}
impl Drop for WitnessDirectory {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

/// Create and flush a witness before entering a deliberate unrecoverable path.
/// # Errors
/// Returns filesystem errors, including an already-existing witness.
pub fn write(directory: &Path, name: &str, contents: &[u8]) -> io::Result<()> {
	let mut file = OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(directory.join(name))?;
	file.write_all(contents)?;
	file.sync_all()
}
