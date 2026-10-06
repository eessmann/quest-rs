//! Immutable serial-HDF5 matching buckets with bounded, reversible record reads.
//!
//! Files contain completed permutation columns and the original replay angles.
//! Hashes attest persisted bytes/semantics, not block-encoding correctness.
//! Collective ownership and permutation admission remains the runtime's job.
use crate::{Error, Result};
use hdf5_metno::{Dataset, File, H5Type, LinkType, plist::dataset_create::Layout};
use quest_qsvt::{Complex64, MatchingColumn, MatchingHeader};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
	io::{Read, Write},
	ops::{Mul, Sub},
	path::{Component, Path},
};

const VERSION: u32 = 2;
const CONSTRUCTION: &str = "completed-matching-columns-v2";
const HEADER_WORDS: usize = 10;
#[cfg(test)]
mod capacity_tests;
mod manifest_decode;

// Fixed-schema reservation for headers and each chunk/index expansion. The
// actual logical file size is audited against this reservation after flush.
// Filesystem block allocation and the HDF5 metadata cache are separate costs.
const HDF5_METADATA_RESERVE: u64 = 65_536;

/// Independent limits for metadata, application buffers and disk snapshots.
///
/// These limits account explicit Rust payloads with actual-capacity checks before
/// later fill/conversion. Returned excess allocations can exist before rejection.
/// HDF5 handles/caches, metadata discovery and returned header/owner metadata,
/// allocator overhead and RSS are separate; disabling the raw-data chunk cache
/// does not bound native metadata storage.
#[derive(Clone, Copy, Debug)]
pub struct ShardIoLimits {
	pub chunk_records: usize,
	pub max_buffer_bytes: usize,
	pub max_manifest_bytes: usize,
	pub max_buckets: usize,
	pub max_records: usize,
	/// Applies to each immutable file and its temporary verified disk snapshot.
	pub max_file_bytes: u64,
}
impl Default for ShardIoLimits {
	fn default() -> Self {
		Self {
			chunk_records: 4096,
			max_buffer_bytes: 4 * 1024 * 1024,
			max_manifest_bytes: 4 * 1024 * 1024,
			max_buckets: 4096,
			max_records: 16_777_216,
			max_file_bytes: 4_294_967_296,
		}
	}
}
impl ShardIoLimits {
	fn file_reservation(self, records: usize) -> Result<u64> {
		let chunks = records
			.checked_div(self.chunk_records)
			.and_then(|n| {
				n.checked_add(usize::from(
					records.checked_rem(self.chunk_records) != Some(0),
				))
			})
			.ok_or(Error::Budget("HDF5 chunk count"))?;
		let chunk_bytes = word(self.chunk_records)?
			.checked_mul(word(size_of::<DiskRecord>())?)
			.and_then(|n| n.checked_add(HDF5_METADATA_RESERVE))
			.ok_or(Error::Budget("HDF5 chunk reservation"))?;
		let bytes = word(chunks)?
			.checked_mul(chunk_bytes)
			.and_then(|n| n.checked_add(HDF5_METADATA_RESERVE))
			.ok_or(Error::Budget("HDF5 file reservation"))?;
		if bytes > self.max_file_bytes {
			return Err(Error::Budget("HDF5 file reservation"));
		}
		Ok(bytes)
	}
	fn application_bytes(self) -> Result<usize> {
		let record_bytes = size_of::<DiskRecord>()
			.checked_add(size_of::<PersistedMatchingRecord>())
			.ok_or(Error::Budget("shard record bytes"))?;
		self.chunk_records
			.checked_mul(record_bytes)
			.and_then(|n| n.checked_add(8192))
			.ok_or(Error::Budget("shard chunk bytes"))
	}
	fn validate(self) -> Result<()> {
		let bytes = self
			.application_bytes()?
			.checked_add(
				self.chunk_records
					.checked_mul(size_of::<DiskRecord>())
					.ok_or(Error::Budget("HDF5 write chunk bytes"))?,
			)
			.ok_or(Error::Budget("shard concurrent buffers"))?;
		if self.chunk_records == 0
			|| bytes > self.max_buffer_bytes
			|| self.max_buckets == 0
			|| self.max_manifest_bytes == 0
			|| self.max_records == 0
			|| self.max_file_bytes == 0
		{
			return Err(Error::Budget("shard IO limits"));
		}
		Ok(())
	}
}

/// Completed column with frozen angles; completion-only columns carry PI/0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PersistedMatchingRecord {
	pub column: MatchingColumn,
	pub theta: f64,
	pub phase_angle: f64,
	pub is_edge: bool,
}
impl PersistedMatchingRecord {
	fn validate(self, header: MatchingHeader, bucket: usize, buckets: usize) -> Result<()> {
		let c = self.column;
		let dimension = header
			.system_dimension()
			.map_err(|_| Error::Format("matching dimensions"))?;
		if c.color >= header.num_colors
			|| c.source >= dimension
			|| c.destination >= dimension
			|| c.source.checked_rem(buckets) != Some(bucket)
			|| !c.cosine.is_finite()
			|| !c.sine.is_finite()
			|| c.cosine < 0.0
			|| c.sine < 0.0
			|| c.cosine
				.mul_add(c.cosine, c.sine.mul(c.sine))
				.sub(1.0)
				.abs()
				> 1e-12
			|| !c.phase.re.is_finite()
			|| !c.phase.im.is_finite()
			|| c.phase.norm_sqr().sub(1.0).abs() > 1e-12
			|| !self.theta.is_finite()
			|| !self.phase_angle.is_finite()
		{
			return Err(Error::Format("invalid persisted matching column"));
		}
		if self.is_edge {
			if c.source >= header.cols
				|| c.destination >= header.rows
				|| !(0.0..=std::f64::consts::PI).contains(&self.theta)
				|| self.theta.mul(0.5).cos().sub(c.cosine).abs() > 1e-12
				|| self.theta.mul(0.5).sin().sub(c.sine).abs() > 1e-12
				|| self.phase_angle.cos().sub(c.phase.re).abs() > 1e-12
				|| self.phase_angle.sin().sub(c.phase.im).abs() > 1e-12
			{
				return Err(Error::Format("frozen replay angles disagree with column"));
			}
		} else if c.cosine.to_bits() != 0.0_f64.to_bits()
			|| c.sine.to_bits() != 1.0_f64.to_bits()
			|| c.phase != Complex64::new(1.0, 0.0)
			|| self.theta.to_bits() != std::f64::consts::PI.to_bits()
			|| self.phase_angle.to_bits() != 0
		{
			return Err(Error::Format("noncanonical completion column"));
		}
		Ok(())
	}
}

#[derive(Clone, Copy, H5Type)]
#[repr(C)]
struct DiskRecord {
	color: u64,
	source: u64,
	destination: u64,
	cosine: f64,
	sine: f64,
	phase_real: f64,
	phase_imag: f64,
	theta: f64,
	phase_angle: f64,
	is_edge: u64,
}
impl TryFrom<PersistedMatchingRecord> for DiskRecord {
	type Error = Error;
	fn try_from(r: PersistedMatchingRecord) -> Result<Self> {
		Ok(Self {
			color: word(r.column.color)?,
			source: word(r.column.source)?,
			destination: word(r.column.destination)?,
			cosine: r.column.cosine,
			sine: r.column.sine,
			phase_real: r.column.phase.re,
			phase_imag: r.column.phase.im,
			theta: r.theta,
			phase_angle: r.phase_angle,
			is_edge: u64::from(r.is_edge),
		})
	}
}
impl TryFrom<DiskRecord> for PersistedMatchingRecord {
	type Error = Error;
	fn try_from(r: DiskRecord) -> Result<Self> {
		if r.is_edge > 1 {
			return Err(Error::Format("invalid matching edge tag"));
		}
		Ok(Self {
			column: MatchingColumn {
				color: index(r.color)?,
				source: index(r.source)?,
				destination: index(r.destination)?,
				cosine: r.cosine,
				sine: r.sine,
				phase: Complex64::new(r.phase_real, r.phase_imag),
			},
			theta: r.theta,
			phase_angle: r.phase_angle,
			is_edge: r.is_edge == 1,
		})
	}
}
fn word(x: usize) -> Result<u64> {
	u64::try_from(x).map_err(|_| Error::Budget("shard index representation"))
}
fn index(x: u64) -> Result<usize> {
	usize::try_from(x).map_err(|_| Error::Budget("shard native index representation"))
}
fn header_words(h: MatchingHeader) -> Result<[u64; HEADER_WORDS]> {
	h.validate().map_err(|_| Error::Format("matching header"))?;
	Ok([
		word(h.rows)?,
		word(h.cols)?,
		word(h.system_qubits)?,
		word(h.color_qubits)?,
		word(h.num_colors)?,
		h.beta.to_bits(),
		h.alpha.to_bits(),
		h.source_identity,
		word(h.record_count)?,
		h.record_digest,
	])
}
fn from_header_words(w: [u64; HEADER_WORDS]) -> Result<MatchingHeader> {
	let [
		rows,
		cols,
		system,
		color,
		colors,
		beta,
		alpha,
		identity,
		count,
		digest,
	] = w;
	let h = MatchingHeader {
		rows: index(rows)?,
		cols: index(cols)?,
		system_qubits: index(system)?,
		color_qubits: index(color)?,
		num_colors: index(colors)?,
		beta: f64::from_bits(beta),
		alpha: f64::from_bits(alpha),
		source_identity: identity,
		record_count: index(count)?,
		record_digest: digest,
	};
	h.validate().map_err(|_| Error::Format("matching header"))?;
	Ok(h)
}
fn semantic_start(header: MatchingHeader, bucket: usize, buckets: usize) -> Result<Sha256> {
	let mut hash = Sha256::new();
	hash.update(CONSTRUCTION.as_bytes());
	for w in header_words(header)?
		.into_iter()
		.chain([word(bucket)?, word(buckets)?])
	{
		hash.update(w.to_le_bytes());
	}
	Ok(hash)
}
fn hash_record(hash: &mut Sha256, r: DiskRecord) {
	for w in [
		r.color,
		r.source,
		r.destination,
		r.cosine.to_bits(),
		r.sine.to_bits(),
		r.phase_real.to_bits(),
		r.phase_imag.to_bits(),
		r.theta.to_bits(),
		r.phase_angle.to_bits(),
		r.is_edge,
	] {
		hash.update(w.to_le_bytes());
	}
}

/// Immutable-file receipt. Byte hashes and semantic hashes serve distinct roles.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchingBucketReceipt {
	pub bucket: usize,
	pub buckets: usize,
	pub file: String,
	pub records: usize,
	pub size_bytes: u64,
	pub sha256: [u8; 32],
	pub semantic_sha256: [u8; 32],
}

/// Common bounded manifest. It can be shared; its large payloads remain sharded.
///
/// Schema strings use their canonical unescaped ASCII spellings. The budgeted
/// loader rejects escapes before JSON decoding can allocate unescaping scratch.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchingManifest {
	schema_version: u32,
	construction: String,
	header: [u64; HEADER_WORDS],
	buckets: Vec<MatchingBucketReceipt>,
}
impl MatchingManifest {
	/// Check metadata coverage before publishing. Collective callers must also
	/// agree on producer success before calling `publish` on one designated rank.
	/// # Errors
	/// Rejects incomplete ownership, inconsistent records and metadata budgets.
	pub fn new(
		header: MatchingHeader,
		mut buckets: Vec<MatchingBucketReceipt>,
		limits: ShardIoLimits,
	) -> Result<Self> {
		buckets.sort_unstable_by_key(|r| r.bucket);
		let result = Self {
			schema_version: VERSION,
			construction: CONSTRUCTION.into(),
			header: header_words(header)?,
			buckets,
		};
		result.validate(limits)?;
		Ok(result)
	}
	fn validate(&self, limits: ShardIoLimits) -> Result<()> {
		limits.validate()?;
		let header = self.header()?;
		let count = self.buckets.len();
		if self.schema_version != VERSION
			|| self.construction != CONSTRUCTION
			|| !count.is_power_of_two()
			|| count > limits.max_buckets
		{
			return Err(Error::Format("unsupported matching manifest"));
		}
		let retained = self
			.buckets
			.capacity()
			.checked_mul(size_of::<MatchingBucketReceipt>())
			.and_then(|n| n.checked_add(self.construction.capacity()))
			.ok_or(Error::Budget("shard manifest allocation"))?;
		let mut retained = retained;
		let mut total = 0usize;
		for (bucket, receipt) in self.buckets.iter().enumerate() {
			retained = retained
				.checked_add(receipt.file.capacity())
				.ok_or(Error::Budget("shard manifest allocation"))?;
			if receipt.bucket != bucket
				|| receipt.buckets != count
				|| receipt.file != bucket_name(bucket)
				|| receipt.size_bytes == 0
				|| receipt.size_bytes > limits.max_file_bytes
				|| receipt.records > limits.max_records
			{
				return Err(Error::Format("incomplete or inconsistent bucket receipt"));
			}
			total = total
				.checked_add(receipt.records)
				.ok_or(Error::Budget("shard record count"))?;
		}
		if retained > limits.max_manifest_bytes {
			return Err(Error::Budget("shard manifest allocation"));
		}
		if total != header.record_count {
			return Err(Error::Format("manifest record coverage"));
		}
		Ok(())
	}
	/// # Errors
	/// Rejects invalid or unrepresentable header fields.
	pub fn header(&self) -> Result<MatchingHeader> {
		from_header_words(self.header)
	}
	#[must_use]
	pub fn buckets(&self) -> &[MatchingBucketReceipt] {
		&self.buckets
	}
	/// Canonical semantic identity independent of JSON/HDF5 serialization details.
	#[must_use]
	pub fn semantic_sha256(&self) -> [u8; 32] {
		let mut hash = Sha256::new();
		hash.update(CONSTRUCTION.as_bytes());
		for word in self.header {
			hash.update(word.to_le_bytes());
		}
		for bucket in &self.buckets {
			hash.update(bucket.semantic_sha256);
		}
		hash.finalize().into()
	}
	/// Bucket ownership reuses files only for a compatible execution partition.
	/// # Errors
	/// Rejects execution layouts requiring an explicit streaming repartition.
	pub fn owned_buckets(
		&self,
		rank: usize,
		parts: usize,
	) -> Result<impl Iterator<Item = usize> + '_> {
		if !parts.is_power_of_two()
			|| rank >= parts
			|| self.buckets.len().checked_rem(parts) != Some(0)
		{
			return Err(Error::Format("bucket layout requires repartition"));
		}
		Ok((rank..self.buckets.len()).step_by(parts))
	}
	/// Atomically publish without replacing existing work. The caller must agree
	/// collectively that every owner has published its data file. Publication
	/// deliberately does not reread remote owners' files on the manifest rank.
	/// # Errors
	/// Rejects metadata/IO limits and existing destination paths.
	pub fn publish(&self, path: impl AsRef<Path>, limits: ShardIoLimits) -> Result<()> {
		self.validate(limits)?;
		let path = path.as_ref();
		let directory = path
			.parent()
			.filter(|p| !p.as_os_str().is_empty())
			.unwrap_or_else(|| Path::new("."));
		// Declare the path before the file so every error closes the descriptor
		// before unlinking. Unlinking an open file leaves NFS cleanup placeholders.
		let (temporary, mut output) = {
			let (file, path) = tempfile::NamedTempFile::new_in(directory)?.into_parts();
			(path, file)
		};
		let mut writer = BoundedWriter {
			output: &mut output,
			remaining: limits.max_manifest_bytes,
		};
		serde_json::to_writer_pretty(&mut writer, self)?;
		output.sync_all()?;
		drop(output);
		temporary
			.persist_noclobber(path)
			.map_err(|e| Error::Io(e.error))?;
		Ok(())
	}
	/// # Errors
	/// Rejects over-budget or malformed metadata before any bucket is opened.
	pub fn open(path: impl AsRef<Path>, limits: ShardIoLimits) -> Result<Self> {
		limits.validate()?;
		let mut file = std::fs::File::open(path)?;
		let length = file.metadata()?.len();
		if length > word(limits.max_manifest_bytes)? {
			return Err(Error::Budget("manifest file bytes"));
		}
		let mut bytes = Vec::new();
		let limit = index(length)?
			.checked_add(1)
			.ok_or(Error::Budget("manifest bytes"))?;
		if manifest_decode::base_bytes(limit)? > limits.max_manifest_bytes {
			return Err(Error::Budget("manifest concurrent metadata allocation"));
		}
		bytes
			.try_reserve_exact(limit)
			.map_err(|_| Error::Budget("manifest allocation"))?;
		#[cfg(test)]
		capacity_tests::inflate(&mut bytes, "manifest-input")?;
		if manifest_decode::base_bytes(bytes.capacity())? > limits.max_manifest_bytes {
			return Err(Error::Budget("manifest actual input capacity"));
		}
		#[cfg(test)]
		capacity_tests::fill();
		bytes.resize(limit, 0);
		let mut count = 0;
		while count < limit {
			let read = file.read(
				bytes
					.get_mut(count..)
					.ok_or(Error::Budget("manifest read offset"))?,
			)?;
			if read == 0 {
				break;
			}
			count = count
				.checked_add(read)
				.ok_or(Error::Budget("manifest read count"))?;
		}
		if count == limit {
			return Err(Error::Budget("manifest grew during bounded read"));
		}
		bytes.truncate(count);
		let result = manifest_decode::decode(&bytes, bytes.capacity(), limits)?;
		result.validate(limits)?;
		Ok(result)
	}
	/// Snapshot one owned bucket to bounded disk storage, verify its bytes, then
	/// admit its typed records. Later changes to the source path cannot affect it.
	/// # Errors
	/// Rejects integrity, schema, ownership, finite-value and resource failures.
	pub fn open_bucket(
		&self,
		directory: impl AsRef<Path>,
		bucket: usize,
		limits: ShardIoLimits,
	) -> Result<MatchingBucketInput> {
		self.validate(limits)?;
		let receipt = self
			.buckets
			.get(bucket)
			.ok_or(Error::Format("bucket index"))?;
		MatchingBucketInput::open(directory.as_ref(), self.header()?, receipt, limits)
	}
}

struct BoundedWriter<'a> {
	output: &'a mut std::fs::File,
	remaining: usize,
}
impl Write for BoundedWriter<'_> {
	fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
		if data.len() > self.remaining {
			return Err(std::io::Error::other("manifest byte budget"));
		}
		let written = self.output.write(data)?;
		self.remaining = self.remaining.saturating_sub(written);
		Ok(written)
	}
	fn flush(&mut self) -> std::io::Result<()> {
		self.output.flush()
	}
}
fn bucket_name(bucket: usize) -> String {
	format!("matching-{bucket:016x}.h5")
}

/// Write one sorted logical bucket with bounded application buffers.
/// No global sparse matrix or complete rank payload is accepted by this API.
/// # Errors
/// Rejects unsorted/duplicate records, ownership, angle and budget failures.
/// Existing destinations are never replaced, including after a partial failure.
pub fn write_bucket(
	directory: impl AsRef<Path>,
	header: MatchingHeader,
	bucket: usize,
	buckets: usize,
	records: impl IntoIterator<Item = Result<PersistedMatchingRecord>>,
	limits: ShardIoLimits,
) -> Result<MatchingBucketReceipt> {
	limits.validate()?;
	if !buckets.is_power_of_two() || buckets > limits.max_buckets || bucket >= buckets {
		return Err(Error::Format("bucket ownership"));
	}
	limits.file_reservation(0)?;
	let directory = directory.as_ref();
	// HDF5 owns the only descriptor; the path guard drops after its handles.
	let temporary = tempfile::NamedTempFile::new_in(directory)?.into_temp_path();
	let file = File::create(&temporary)?;
	file.new_dataset::<u64>()
		.shape(HEADER_WORDS)
		.create("header")?
		.write_raw(&header_words(header)?)?;
	file.new_dataset::<u64>()
		.shape(2)
		.create("owner")?
		.write_raw(&[word(bucket)?, word(buckets)?])?;
	let dataset = file
		.new_dataset::<DiskRecord>()
		.shape((0..,))
		.chunk(limits.chunk_records)
		.chunk_cache(1, 0, 0.0)
		.create("records")?;
	let mut buffer = Vec::new();
	buffer
		.try_reserve_exact(limits.chunk_records)
		.map_err(|_| Error::Budget("shard write allocation"))?;
	let mut hash = semantic_start(header, bucket, buckets)?;
	let mut count = 0usize;
	let mut last = None;
	for record in records {
		let record = record?;
		record.validate(header, bucket, buckets)?;
		let key = (record.column.color, record.column.source);
		if last.is_some_and(|previous| previous >= key) {
			return Err(Error::Format("bucket records must be strictly sorted"));
		}
		last = Some(key);
		count = count.checked_add(1).ok_or(Error::Budget("shard count"))?;
		if count > limits.max_records || count > header.record_count {
			return Err(Error::Budget("shard record count"));
		}
		limits.file_reservation(count)?;
		let disk = DiskRecord::try_from(record)?;
		hash_record(&mut hash, disk);
		buffer.push(disk);
		if buffer.len() == limits.chunk_records {
			append_chunk(&dataset, &buffer, count)?;
			buffer.clear();
			file.flush()?;
			if file.size() > limits.file_reservation(count)? {
				return Err(Error::Format(
					"HDF5 allocation exceeded admitted reservation",
				));
			}
		}
	}
	if !buffer.is_empty() {
		append_chunk(&dataset, &buffer, count)?;
	}
	drop(dataset);
	file.close()?;
	let (size_bytes, sha256) = hash_file(&temporary, limits.max_file_bytes)?;
	if size_bytes > limits.file_reservation(count)? {
		return Err(Error::Format(
			"HDF5 allocation exceeded admitted reservation",
		));
	}
	let file_name = bucket_name(bucket);
	std::fs::OpenOptions::new()
		.write(true)
		.open(&temporary)?
		.sync_all()?;
	temporary
		.persist_noclobber(directory.join(&file_name))
		.map_err(|e| Error::Io(e.error))?;
	Ok(MatchingBucketReceipt {
		bucket,
		buckets,
		file: file_name,
		records: count,
		size_bytes,
		sha256,
		semantic_sha256: hash.finalize().into(),
	})
}
fn append_chunk(dataset: &Dataset, buffer: &[DiskRecord], end: usize) -> Result<()> {
	let start = end
		.checked_sub(buffer.len())
		.ok_or(Error::Budget("shard slice"))?;
	dataset.resize((end,))?;
	dataset.write_slice(buffer, start..end)?;
	Ok(())
}
fn hash_file(path: &Path, maximum: u64) -> Result<(u64, [u8; 32])> {
	let file = std::fs::File::open(path)?;
	if file.metadata()?.len() > maximum {
		return Err(Error::Budget("shard file bytes"));
	}
	copy_hash(file, &mut std::io::sink(), maximum)
}
fn copy_hash(
	mut input: impl Read,
	output: &mut impl Write,
	maximum: u64,
) -> Result<(u64, [u8; 32])> {
	let mut buffer = [0_u8; 8192];
	let mut count = 0u64;
	let mut hash = Sha256::new();
	loop {
		let read = input.read(&mut buffer)?;
		if read == 0 {
			break;
		}
		count = count
			.checked_add(word(read)?)
			.ok_or(Error::Budget("shard file size"))?;
		if count > maximum {
			return Err(Error::Budget("shard file bytes"));
		}
		let bytes = buffer
			.get(..read)
			.ok_or(Error::Format("shard read count"))?;
		hash.update(bytes);
		output.write_all(bytes)?;
	}
	Ok((count, hash.finalize().into()))
}

/// Owns a verified private snapshot; source replacement does not invalidate it.
pub struct MatchingBucketInput {
	dataset: Dataset,
	file: File,
	_snapshot: tempfile::TempPath,
	header: MatchingHeader,
	bucket: usize,
	buckets: usize,
	count: usize,
	limits: ShardIoLimits,
}
impl MatchingBucketInput {
	fn open(
		directory: &Path,
		header: MatchingHeader,
		receipt: &MatchingBucketReceipt,
		limits: ShardIoLimits,
	) -> Result<Self> {
		if !matches!(
			Path::new(&receipt.file)
				.components()
				.collect::<Vec<_>>()
				.as_slice(),
			[Component::Normal(_)]
		) {
			return Err(Error::Format("bucket must be a local filename"));
		}
		let source = std::fs::File::open(directory.join(&receipt.file))?;
		if source.metadata()?.len() != receipt.size_bytes {
			return Err(Error::Format("bucket size mismatch"));
		}
		let (snapshot, mut output) = {
			let (file, path) = tempfile::NamedTempFile::new()?.into_parts();
			(path, file)
		};
		let (size, hash) = copy_hash(source, &mut output, receipt.size_bytes)?;
		if size != receipt.size_bytes || hash != receipt.sha256 {
			return Err(Error::Format("bucket SHA-256 mismatch"));
		}
		output.flush()?;
		drop(output);
		let file = File::with_options()
			.with_fapl(|p| p.chunk_cache(1, 0, 0.0))
			.open(&snapshot)?;
		if file.len() != 3 {
			return Err(Error::Format("bucket dataset count"));
		}
		let mut hard = true;
		file.iter_visit_default(|_, info| {
			hard &= info.link_type == LinkType::Hard;
			Ok(())
		})?;
		let mut names = file.member_names()?;
		names.sort();
		if !hard || names != ["header", "owner", "records"] {
			return Err(Error::Format("bucket dataset schema"));
		}
		let header_data = file.dataset("header")?;
		let owner_data = file.dataset("owner")?;
		validate_dataset::<u64>(&header_data, HEADER_WORDS, limits)?;
		validate_dataset::<u64>(&owner_data, 2, limits)?;
		if header_data.read_raw::<u64>()? != header_words(header)?
			|| owner_data.read_raw::<u64>()? != [word(receipt.bucket)?, word(receipt.buckets)?]
		{
			return Err(Error::Format("bucket header or ownership mismatch"));
		}
		// These handles must not outlive the input if semantic admission fails.
		drop(header_data);
		drop(owner_data);
		let dataset = file.dataset("records")?;
		validate_dataset::<DiskRecord>(&dataset, receipt.records, limits)?;
		let input = Self {
			dataset,
			file,
			_snapshot: snapshot,
			header,
			bucket: receipt.bucket,
			buckets: receipt.buckets,
			count: receipt.records,
			limits,
		};
		let mut semantic = semantic_start(header, receipt.bucket, receipt.buckets)?;
		let mut last = None;
		input.visit_records(false, |records| {
			for record in records {
				let key = (record.column.color, record.column.source);
				if last.is_some_and(|previous| previous >= key) {
					return Err(Error::Format("unsorted bucket records"));
				}
				last = Some(key);
				hash_record(&mut semantic, DiskRecord::try_from(*record)?);
			}
			Ok(())
		})?;
		let actual: [u8; 32] = semantic.finalize().into();
		if actual != receipt.semantic_sha256 {
			return Err(Error::Format("bucket semantic digest mismatch"));
		}
		Ok(input)
	}
	#[must_use]
	pub const fn len(&self) -> usize {
		self.count
	}
	#[must_use]
	pub const fn is_empty(&self) -> bool {
		self.count == 0
	}
	/// Visit at most `chunk_records` at once, in complete forward or reverse order.
	/// This is record replay; gate decomposition and collective ordering remain
	/// source-construction responsibilities.
	/// # Errors
	/// Propagates visitor, storage and typed record errors immediately.
	pub fn visit_records(
		&self,
		reverse: bool,
		mut visit: impl FnMut(&[PersistedMatchingRecord]) -> Result<()>,
	) -> Result<()> {
		let mut position = if reverse { self.count } else { 0 };
		let mut buffer = Vec::new();
		buffer
			.try_reserve_exact(self.limits.chunk_records)
			.map_err(|_| Error::Budget("shard read allocation"))?;
		#[cfg(test)]
		capacity_tests::inflate(&mut buffer, "typed")?;
		admit_read_buffers(buffer.capacity(), self.limits.chunk_records, self.limits)?;
		while if reverse {
			position > 0
		} else {
			position < self.count
		} {
			let (start, end) = if reverse {
				(position.saturating_sub(self.limits.chunk_records), position)
			} else {
				(
					position,
					position
						.saturating_add(self.limits.chunk_records)
						.min(self.count),
				)
			};
			let values = self.dataset.read_slice_1d::<DiskRecord, _>(start..end)?;
			let (values, offset) = values.into_raw_vec_and_offset();
			if offset != Some(0)
				|| values.len()
					!= end
						.checked_sub(start)
						.ok_or(Error::Budget("shard slice length"))?
			{
				return Err(Error::Format("shard contiguous read layout"));
			}
			#[cfg(test)]
			let values = {
				let mut values = values;
				capacity_tests::inflate(&mut values, "disk")?;
				values
			};
			admit_read_buffers(buffer.capacity(), values.capacity(), self.limits)?;
			buffer.clear();
			#[cfg(test)]
			capacity_tests::fill();
			for value in values {
				let record = PersistedMatchingRecord::try_from(value)?;
				record.validate(self.header, self.bucket, self.buckets)?;
				buffer.push(record);
			}
			if reverse {
				buffer.reverse();
			}
			visit(&buffer)?;
			position = if reverse { start } else { end };
		}
		Ok(())
	}
	/// File size is disk residency, not retained application-buffer size.
	#[must_use]
	pub fn snapshot_bytes(&self) -> u64 {
		self.file.size()
	}
}
fn admit_read_buffers(typed: usize, disk: usize, limits: ShardIoLimits) -> Result<()> {
	let bytes = typed
		.checked_mul(size_of::<PersistedMatchingRecord>())
		.and_then(|n| n.checked_add(disk.checked_mul(size_of::<DiskRecord>())?))
		.and_then(|n| n.checked_add(8192))
		.ok_or(Error::Budget("shard actual read buffers"))?;
	if bytes > limits.max_buffer_bytes {
		return Err(Error::Budget("shard actual read buffers"));
	}
	Ok(())
}
fn validate_dataset<T: H5Type>(
	dataset: &Dataset,
	count: usize,
	limits: ShardIoLimits,
) -> Result<()> {
	let props = dataset.dcpl()?;
	if dataset.shape() != [count]
		|| dataset.dtype()?.to_descriptor()? != T::type_descriptor()
		|| !props.get_external()?.is_empty()
		|| !props.get_filters()?.is_empty()
		|| !matches!(
			props.get_layout()?,
			Layout::Compact | Layout::Contiguous | Layout::Chunked
		) {
		return Err(Error::Format("unsupported shard dataset storage"));
	}
	let application_bytes = limits.application_bytes()?;
	if dataset.chunk().is_some_and(|shape| {
		shape
			.iter()
			.try_fold(1usize, |n, &v| n.checked_mul(v))
			.and_then(|n| n.checked_mul(size_of::<T>()))
			.and_then(|n| n.checked_add(application_bytes))
			.is_none_or(|n| n > limits.max_buffer_bytes)
	}) {
		return Err(Error::Budget("stored HDF5 chunk bytes"));
	}
	Ok(())
}
