//! Admitted private scratch files for bounded numerical sparse sorting.
use quest_numerics::{
	Complex64, Error, Result,
	sparse_stream::{RunReader, RunStore, SparseEntry},
};
use std::{
	fs::{File, OpenOptions},
	io::{Read, Write},
	path::PathBuf,
	sync::atomic::{AtomicU64, Ordering},
};
static NEXT_RUN: AtomicU64 = AtomicU64::new(0);
/// Directory-specific scratch adapter. Each handle removes its file on drop.
pub struct FileRunStore {
	directory: PathBuf,
}
impl FileRunStore {
	#[must_use]
	pub const fn new(directory: PathBuf) -> Self {
		Self { directory }
	}
}
pub struct FileRun {
	path: PathBuf,
}
impl FileRun {
	/// Actual owned path capacity and wrapper payload; excludes kernel/filesystem metadata.
	/// # Errors
	/// Rejects byte-accounting overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		self.path
			.capacity()
			.checked_add(size_of::<Self>())
			.ok_or(Error::Overflow)
	}
}
impl Drop for FileRun {
	fn drop(&mut self) {
		let _ = std::fs::remove_file(&self.path);
	}
}
pub struct FileRunWriter {
	file: File,
	run: Option<FileRun>,
}
impl FileRunWriter {
	/// Conservative actual path and file-wrapper payload before the first write.
	/// Excludes kernel/filesystem metadata. The path is stable while writing records.
	/// # Errors
	/// Rejects missing ownership or byte-accounting overflow.
	pub fn retained_bytes(&self) -> Result<usize> {
		self.run
			.as_ref()
			.ok_or(Error::Domain("missing sparse run owner"))?
			.retained_bytes()?
			.checked_add(size_of::<Self>())
			.ok_or(Error::Overflow)
	}
}
pub struct FileRunReader {
	file: File,
}
fn error(_: std::io::Error) -> Error {
	Error::Domain("sparse temporary run IO")
}
impl RunStore for FileRunStore {
	type Handle = FileRun;
	type Reader = FileRunReader;
	type Writer = FileRunWriter;
	fn descriptor_bytes(&self) -> usize {
		self.directory.capacity().saturating_add(128)
	}
	fn record_bytes(&self) -> usize {
		40
	}
	fn create(&mut self) -> Result<FileRunWriter> {
		let id = NEXT_RUN
			.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
			.map_err(|_| Error::Overflow)?;
		let path = self
			.directory
			.join(format!("quest-sparse-{}-{id}.run", std::process::id()));
		let file = OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(&path)
			.map_err(error)?;
		Ok(FileRunWriter {
			run: Some(FileRun { path }),
			file,
		})
	}
	fn write(&mut self, writer: &mut FileRunWriter, entry: SparseEntry) -> Result<()> {
		for word in [
			u64::try_from(entry.row).map_err(|_| Error::Overflow)?,
			u64::try_from(entry.column).map_err(|_| Error::Overflow)?,
			entry.ordinal,
			entry.value.re.to_bits(),
			entry.value.im.to_bits(),
		] {
			writer.file.write_all(&word.to_le_bytes()).map_err(error)?;
		}
		Ok(())
	}
	fn finish(&mut self, mut writer: FileRunWriter) -> Result<FileRun> {
		writer.file.flush().map_err(error)?;
		writer
			.run
			.take()
			.ok_or(Error::Domain("missing sparse run handle"))
	}
	fn open(&mut self, handle: &FileRun) -> Result<FileRunReader> {
		Ok(FileRunReader {
			file: File::open(&handle.path).map_err(error)?,
		})
	}
}
impl RunReader for FileRunReader {
	fn next_entry(&mut self) -> Result<Option<SparseEntry>> {
		let mut bytes = [0; 40];
		if self.file.read(&mut bytes[..1]).map_err(error)? == 0 {
			return Ok(None);
		}
		self.file.read_exact(&mut bytes[1..]).map_err(error)?;
		let mut words = [0; 5];
		for (word, chunk) in words.iter_mut().zip(bytes.as_chunks::<8>().0) {
			*word = u64::from_le_bytes(*chunk);
		}
		Ok(Some(SparseEntry {
			row: usize::try_from(words[0]).map_err(|_| Error::Overflow)?,
			column: usize::try_from(words[1]).map_err(|_| Error::Overflow)?,
			ordinal: words[2],
			value: Complex64::new(f64::from_bits(words[3]), f64::from_bits(words[4])),
		}))
	}
}
