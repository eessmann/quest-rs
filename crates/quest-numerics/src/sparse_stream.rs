//! Bounded COO/CSR streams with stable original ordinals and admitted external sorting.
//! Scratch storage is an opaque caller-supplied run store; this module performs no IO.
use crate::{Complex64, Error, Result, policy::check_limit};
/// One source coefficient. Ordinals order duplicates independently of partition and arrival.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SparseEntry {
	pub row: usize,
	pub column: usize,
	pub ordinal: u64,
	pub value: Complex64,
}
impl SparseEntry {
	/// # Errors
	/// Rejects out-of-range indices and nonfinite coefficients.
	pub const fn validate(self, rows: usize, cols: usize) -> Result<()> {
		if self.row >= rows || self.column >= cols {
			return Err(Error::Length("stream index"));
		}
		if !self.value.re.is_finite() || !self.value.im.is_finite() {
			return Err(Error::NonFinite { index: 0 });
		}
		Ok(())
	}
	const fn key(self) -> (usize, usize, u64) {
		(self.row, self.column, self.ordinal)
	}
}
/// Local owned storage/work and temporary disk limits; filesystem overhead is excluded.
#[derive(Clone, Debug)]
pub struct StreamLimits {
	pub buffer_entries: usize,
	pub max_entries: usize,
	pub max_bytes: usize,
	pub max_spill_bytes: usize,
	pub max_runs: usize,
	pub max_work: usize,
}
impl Default for StreamLimits {
	fn default() -> Self {
		Self {
			buffer_entries: 4096,
			max_entries: 1_048_576,
			max_bytes: 64 * 1024 * 1024,
			max_spill_bytes: 0,
			max_runs: 1024,
			max_work: usize::MAX,
		}
	}
}
/// Opaque sequential scratch reader; implementations belong to the IO layer.
pub trait RunReader {
	/// # Errors
	/// Reports malformed or unreadable scratch records.
	fn next_entry(&mut self) -> Result<Option<SparseEntry>>;
}
/// Scratch storage supplied by callers. Handles must release their resources on drop.
/// Implementations report conservative owned descriptor and encoded-record bytes.
pub trait RunStore {
	type Handle;
	type Reader: RunReader;
	type Writer;
	fn descriptor_bytes(&self) -> usize;
	fn record_bytes(&self) -> usize;
	/// # Errors
	/// Rejects unavailable storage or creation failure.
	fn create(&mut self) -> Result<Self::Writer>;
	/// # Errors
	/// Reports scratch write failure.
	fn write(&mut self, writer: &mut Self::Writer, entry: SparseEntry) -> Result<()>;
	/// # Errors
	/// Reports finalization failure.
	fn finish(&mut self, writer: Self::Writer) -> Result<Self::Handle>;
	/// # Errors
	/// Reports scratch opening failure.
	fn open(&mut self, handle: &Self::Handle) -> Result<Self::Reader>;
}
/// No filesystem dependency and no implicit spilling.
#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryOnly;
pub struct EmptyReader;
impl RunReader for EmptyReader {
	fn next_entry(&mut self) -> Result<Option<SparseEntry>> {
		Ok(None)
	}
}
impl RunStore for MemoryOnly {
	type Handle = ();
	type Reader = EmptyReader;
	type Writer = ();
	fn descriptor_bytes(&self) -> usize {
		0
	}
	fn record_bytes(&self) -> usize {
		0
	}
	fn create(&mut self) -> Result<()> {
		Err(Error::Domain("stream buffer without scratch store"))
	}
	fn write(&mut self, (): &mut (), _: SparseEntry) -> Result<()> {
		Err(Error::Domain("no scratch store"))
	}
	fn finish(&mut self, (): ()) -> Result<()> {
		Err(Error::Domain("no scratch store"))
	}
	fn open(&mut self, (): &()) -> Result<EmptyReader> {
		Err(Error::Domain("no scratch store"))
	}
}
struct Run<H> {
	handle: H,
	entries: usize,
}
fn reserve<T>(count: usize) -> Result<Vec<T>> {
	let mut values = Vec::new();
	values
		.try_reserve_exact(count)
		.map_err(|_| Error::Allocation)?;
	Ok(values)
}
/// Sorts bounded input chunks. Larger inputs require explicit spill admission.
/// Cloning input iterators or retaining caller-owned input is outside this object.
pub struct StreamBuilder<S: RunStore = MemoryOnly> {
	rows: usize,
	cols: usize,
	limits: StreamLimits,
	buffer: Vec<SparseEntry>,
	store: S,
	runs: Vec<Run<S::Handle>>,
	entries: usize,
	work: usize,
	spill_bytes: usize,
}
impl StreamBuilder<MemoryOnly> {
	/// # Errors
	/// Rejects invalid dimensions or admitted memory buffers.
	pub fn new(rows: usize, cols: usize, limits: StreamLimits) -> Result<Self> {
		Self::with_store(rows, cols, limits, MemoryOnly)
	}
}
impl<S: RunStore> StreamBuilder<S> {
	/// # Errors
	/// Rejects invalid dimensions, zero buffers and buffer-storage admission.
	pub fn with_store(rows: usize, cols: usize, limits: StreamLimits, store: S) -> Result<Self> {
		if rows == 0 || cols == 0 || limits.buffer_entries == 0 {
			return Err(Error::Length("stream dimensions/buffer"));
		}
		let bytes = limits
			.buffer_entries
			.checked_mul(size_of::<SparseEntry>())
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| n.checked_add(store.descriptor_bytes()))
			.ok_or(Error::Overflow)?;
		check_limit("stream bytes", bytes, limits.max_bytes)?;
		let buffer = reserve(limits.buffer_entries)?;
		Ok(Self {
			rows,
			cols,
			limits,
			buffer,
			store,
			runs: Vec::new(),
			entries: 0,
			work: 0,
			spill_bytes: 0,
		})
	}
	fn charge(&mut self, work: usize) -> Result<()> {
		self.work = self.work.checked_add(work).ok_or(Error::Overflow)?;
		check_limit("stream work", self.work, self.limits.max_work)
	}
	fn sort_buffer(&mut self) -> Result<()> {
		let work = self
			.buffer
			.len()
			.checked_mul(
				usize::try_from(self.buffer.len().checked_ilog2().unwrap_or(0))
					.map_err(|_| Error::Overflow)?
					.checked_add(2)
					.ok_or(Error::Overflow)?,
			)
			.ok_or(Error::Overflow)?;
		self.charge(work)?;
		self.buffer.sort_unstable_by_key(|entry| entry.key());
		Ok(())
	}
	fn flush(&mut self) -> Result<()> {
		if self.buffer.is_empty() {
			return Ok(());
		}
		if self.store.record_bytes() == 0 {
			return Err(Error::Domain("stream buffer without scratch store"));
		}
		let count = self.runs.len().checked_add(1).ok_or(Error::Overflow)?;
		check_limit("stream runs", count, self.limits.max_runs)?;
		let descriptor_bytes = count
			.checked_mul(2)
			.and_then(|n| {
				n.checked_mul(
					size_of::<Run<S::Handle>>().checked_add(self.store.descriptor_bytes())?,
				)
			})
			.and_then(|n| {
				n.checked_add(
					self.buffer
						.capacity()
						.checked_mul(size_of::<SparseEntry>())?,
				)
			})
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.ok_or(Error::Overflow)?;
		check_limit("stream bytes", descriptor_bytes, self.limits.max_bytes)?;
		let spill = self
			.buffer
			.len()
			.checked_mul(self.store.record_bytes())
			.ok_or(Error::Overflow)?;
		check_limit(
			"stream spill bytes",
			self.spill_bytes.checked_add(spill).ok_or(Error::Overflow)?,
			self.limits.max_spill_bytes,
		)?;
		self.sort_buffer()?;
		self.runs
			.try_reserve_exact(1)
			.map_err(|_| Error::Allocation)?;
		let mut writer = self.store.create()?;
		for &entry in &self.buffer {
			self.store.write(&mut writer, entry)?;
		}
		let run = Run {
			handle: self.store.finish(writer)?,
			entries: self.buffer.len(),
		};
		self.runs.push(run);
		self.spill_bytes = self.spill_bytes.checked_add(spill).ok_or(Error::Overflow)?;
		self.buffer.clear();
		Ok(())
	}
	/// # Errors
	/// Rejects malformed input, aggregate entries, work, memory or temporary disk limits.
	pub fn push(&mut self, entry: SparseEntry) -> Result<()> {
		entry.validate(self.rows, self.cols)?;
		let count = self.entries.checked_add(1).ok_or(Error::Overflow)?;
		check_limit("stream entries", count, self.limits.max_entries)?;
		if self.buffer.len() == self.limits.buffer_entries {
			self.flush()?;
		}
		self.charge(1)?;
		self.buffer.push(entry);
		self.entries = count;
		Ok(())
	}
	/// Finish sorting and expose canonical entries, retaining at most one result run.
	/// # Errors
	/// Rejects merge work/disk budgets and temporary run IO failures.
	pub fn finish(mut self) -> Result<SortedEntries<S::Reader, S::Handle>> {
		self.charge(self.entries)?;
		if self.runs.is_empty() {
			self.sort_buffer()?;
			return Ok(SortedEntries {
				source: SortedSource::Memory(self.buffer.into_iter()),
				pending: None,
				failed: false,
			});
		}
		self.flush()?;
		// Pairwise passes prevent a growing accumulator from causing quadratic merge work.
		while self.runs.len() > 1 {
			let mut next = reserve(self.runs.len().div_ceil(2))?;
			let runs = std::mem::take(&mut self.runs);
			let mut iter = runs.into_iter();
			while let Some(left) = iter.next() {
				if let Some(right) = iter.next() {
					next.push(self.merge(left, right)?);
				} else {
					next.push(left);
				}
			}
			self.runs = next;
		}
		let run = self.runs.pop().ok_or(Error::Length("missing sparse run"))?;
		let reader = self.store.open(&run.handle)?;
		Ok(SortedEntries {
			source: SortedSource::Scratch {
				reader,
				_handle: run.handle,
			},
			pending: None,
			failed: false,
		})
	}
	fn merge(&mut self, left: Run<S::Handle>, right: Run<S::Handle>) -> Result<Run<S::Handle>> {
		let count = left
			.entries
			.checked_add(right.entries)
			.ok_or(Error::Overflow)?;
		let bytes = count
			.checked_mul(self.store.record_bytes())
			.ok_or(Error::Overflow)?;
		check_limit(
			"stream merge spill",
			self.spill_bytes.checked_add(bytes).ok_or(Error::Overflow)?,
			self.limits.max_spill_bytes,
		)?;
		self.charge(count)?;
		let mut output = self.store.create()?;
		let mut a = self.store.open(&left.handle)?;
		let mut b = self.store.open(&right.handle)?;
		let mut x = a.next_entry()?;
		let mut y = b.next_entry()?;
		while x.is_some() || y.is_some() {
			if y.is_none()
				|| x.is_some_and(|entry| y.is_some_and(|other| entry.key() <= other.key()))
			{
				self.store
					.write(&mut output, x.ok_or(Error::Length("merge input"))?)?;
				x = a.next_entry()?;
			} else {
				self.store
					.write(&mut output, y.ok_or(Error::Length("merge input"))?)?;
				y = b.next_entry()?;
			}
		}
		drop((a, b));
		drop((left, right));
		Ok(Run {
			handle: self.store.finish(output)?,
			entries: count,
		})
	}
}
enum SortedSource<R, H> {
	Memory(std::vec::IntoIter<SparseEntry>),
	Scratch { reader: R, _handle: H },
}
impl<R: RunReader, H> SortedSource<R, H> {
	fn next(&mut self) -> Result<Option<SparseEntry>> {
		match self {
			Self::Memory(entries) => Ok(entries.next()),
			Self::Scratch { reader, .. } => reader.next_entry(),
		}
	}
}
/// Canonical stream; duplicate values are summed in increasing ordinal order.
/// Duplicate ordinals for one coordinate and nonfinite sums are errors.
pub struct SortedEntries<R: RunReader = EmptyReader, H = ()> {
	source: SortedSource<R, H>,
	pending: Option<SparseEntry>,
	failed: bool,
}
impl<R: RunReader, H> Iterator for SortedEntries<R, H> {
	type Item = Result<SparseEntry>;
	fn next(&mut self) -> Option<Self::Item> {
		if self.failed {
			return None;
		}
		let result = self.next_canonical();
		match result {
			Ok(entry) => entry.map(Ok),
			Err(error) => {
				self.failed = true;
				Some(Err(error))
			}
		}
	}
}
impl<R: RunReader, H> SortedEntries<R, H> {
	#[allow(
		clippy::arithmetic_side_effects,
		reason = "Ordered binary64 duplicate sums are checked finite immediately"
	)]
	fn next_canonical(&mut self) -> Result<Option<SparseEntry>> {
		loop {
			let Some(mut entry) = self
				.pending
				.take()
				.map_or_else(|| self.source.next(), |entry| Ok(Some(entry)))?
			else {
				return Ok(None);
			};
			let mut last = entry.ordinal;
			while let Some(other) = self.source.next()? {
				if (other.row, other.column) != (entry.row, entry.column) {
					self.pending = Some(other);
					break;
				}
				if other.ordinal == last {
					return Err(Error::Domain("duplicate sparse input ordinal"));
				}
				last = other.ordinal;
				entry.value += other.value;
				if !entry.value.re.is_finite() || !entry.value.im.is_finite() {
					return Err(Error::NonFinite { index: 0 });
				}
			}
			if entry.value.re != 0.0 || entry.value.im != 0.0 {
				return Ok(Some(entry));
			}
		}
	}
}
/// Borrowed CSR rows indexed in global coordinates, with explicit original ordinals.
pub struct LocalCsr<'a> {
	rows: &'a [usize],
	indices: &'a [usize],
	values: &'a [Complex64],
	ordinals: &'a [u64],
	indptr: &'a [usize],
}
impl<'a> LocalCsr<'a> {
	/// # Errors
	/// Rejects malformed pointers, indices, coefficients and ordinal shape.
	pub fn new(
		global_rows: usize,
		global_cols: usize,
		rows: &'a [usize],
		indices: &'a [usize],
		values: &'a [Complex64],
		ordinals: &'a [u64],
		indptr: &'a [usize],
	) -> Result<Self> {
		if global_rows == 0
			|| global_cols == 0
			|| rows.iter().any(|row| *row >= global_rows)
			|| indptr.len() != rows.len().checked_add(1).ok_or(Error::Overflow)?
			|| indptr.first() != Some(&0)
			|| indptr.last() != Some(&values.len())
			|| indices.len() != values.len()
			|| ordinals.len() != values.len()
			|| indptr.windows(2).any(|pair| matches!(pair,[a,b] if a>b))
		{
			return Err(Error::Length("local CSR shape"));
		}
		let result = Self {
			rows,
			indices,
			values,
			ordinals,
			indptr,
		};
		for entry in result.entries() {
			entry?.validate(global_rows, global_cols)?;
		}
		Ok(result)
	}
	/// Zero-copy local entries; no global pointer vector is constructed.
	pub fn entries(&self) -> impl Iterator<Item = Result<SparseEntry>> + '_ {
		self.rows
			.iter()
			.zip(self.indptr.windows(2))
			.filter_map(move |(&row, pointer)| {
				let &[start, end] = pointer else { return None };
				Some((start..end).map(move |i| {
					Ok(SparseEntry {
						row,
						column: *self
							.indices
							.get(i)
							.ok_or(Error::Length("local CSR index"))?,
						ordinal: *self
							.ordinals
							.get(i)
							.ok_or(Error::Length("local CSR ordinal"))?,
						value: *self.values.get(i).ok_or(Error::Length("local CSR value"))?,
					})
				}))
			})
			.flatten()
	}
}
